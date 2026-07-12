use actix_cors::Cors;
use actix_files::Files;
use actix_web::middleware as actix_middleware;
use actix_web::middleware::Compress;
use actix_web::web::Data;
use actix_web::{App, HttpServer, web};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::net::{SocketAddr, TcpListener};
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info, warn};

use ipma::app_state::AppState;
use ipma::log::setup_logging;
use ipma::routes::static_files::{
    get_web_dir, https_redirect_handler, json_error_handler, serve_json,
};
use ipma::system::cert::{load_rustls_config, prepare_server_certificate};

use ipma::config::Config;
use ipma::db::DbPool;
use ipma::routes::init_routes;
use ipma::shutdown::{ShutdownSignal, wait_for_shutdown_signal};
use ipma::system::config::init_start_time;
use ipma::system::task_executors::{
    BackupTaskExecutor, LogCleanupTaskExecutor, MacSyncTaskExecutor, TokenCleanupTaskExecutor,
    TokenUsageCleanupTaskExecutor,
};
use ipma::utils::log_bilingual;
use ipma::utils::rate_limit::{RateLimitMiddleware, RateLimiter, start_cleanup_task};
use ipma_init::{DatabaseConfig as InitDatabaseConfig, InitContext};
use ipma_scheduler::{RunningScheduler, SchedulerState, TaskRegistry};

fn setup_panic_handler() {
    panic::set_hook(Box::new(|panic_info| {
        let backtrace = std::backtrace::Backtrace::capture();
        let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());

        error!(
            "程序发生严重panic!\n\
             位置: {}\n\
             消息: {}\n\
             堆栈追踪:\n{}",
            location, msg, backtrace
        );

        eprintln!(
            "程序发生严重panic!\n\
             位置: {}\n\
             消息: {}\n\
             堆栈追踪:\n{}",
            location, msg, backtrace
        );
    }));
}

fn build_cors_middleware(config: &Config) -> Cors {
    let mut allowed_origins = config.server.cors_allowed_origins.clone();
    let allow_localhost = config.server.allow_localhost_cors;

    let public_url = config
        .server
        .public_url
        .trim_end_matches('/')
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    allowed_origins.push(format!("http://{}", public_url));
    allowed_origins.push(format!("https://{}", public_url));

    Cors::default()
        .allowed_methods(vec![
            actix_web::http::Method::GET,
            actix_web::http::Method::POST,
            actix_web::http::Method::PUT,
            actix_web::http::Method::DELETE,
            actix_web::http::Method::PATCH,
            actix_web::http::Method::OPTIONS,
        ])
        .allow_any_header()
        .supports_credentials()
        .max_age(3600)
        .allowed_origin_fn(move |origin, _req_head| {
            if let Ok(origin_str) = origin.to_str() {
                for allowed in &allowed_origins {
                    if origin_str == allowed {
                        return true;
                    }
                    if origin_str.starts_with(allowed.trim_end_matches('/'))
                        && (origin_str.ends_with(":80") || origin_str.ends_with(":443"))
                    {
                        return true;
                    }
                }
                allow_localhost
                    && (origin_str.starts_with("http://localhost:")
                        || origin_str.starts_with("https://localhost:")
                        || origin_str.starts_with("http://127.0.0.1:")
                        || origin_str.starts_with("https://127.0.0.1:")
                        || origin_str.starts_with("http://[::1]:")
                        || origin_str.starts_with("https://[::1]:"))
            } else {
                false
            }
        })
}

use std::fs;

fn parse_socket_addr(addr: &str, port: u16) -> std::io::Result<SocketAddr> {
    if addr.contains(':') {
        format!("[{addr}]:{port}")
    } else {
        format!("{addr}:{port}")
    }
    .parse()
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
}

fn create_tcp_listener(addr: &str, port: u16) -> std::io::Result<TcpListener> {
    let socket_addr = parse_socket_addr(addr, port)?;

    let domain = match socket_addr {
        SocketAddr::V6(_) => Domain::IPV6,
        SocketAddr::V4(_) => Domain::IPV4,
    };

    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;

    if socket_addr.is_ipv6() {
        socket.set_only_v6(true)?;
    }

    socket.bind(&SockAddr::from(socket_addr))?;
    socket.listen(2048)?;

    Ok(socket.into())
}

async fn bind_with_retry(addr: &str, port: u16, max_retries: u32) -> std::io::Result<TcpListener> {
    for attempt in 0..=max_retries {
        match create_tcp_listener(addr, port) {
            Ok(listener) => {
                if attempt > 0 {
                    info!(
                        "端口 {}:{} 已成功绑定 (第{}次尝试)",
                        addr,
                        port,
                        attempt + 1
                    );
                }
                return Ok(listener);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                if attempt < max_retries {
                    warn!(
                        "端口 {}:{} 已被占用，等待释放... ({}/{})",
                        addr,
                        port,
                        attempt + 1,
                        max_retries
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                } else {
                    error!(
                        "端口 {}:{} 绑定失败，已重试{}次，程序退出",
                        addr, port, max_retries
                    );
                    return Err(e);
                }
            }
            Err(e) => return Err(e),
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        format!("绑定端口 {}:{} 失败: 重试次数耗尽", addr, port),
    ))
}

fn validate_html_path(path: &str) -> Option<PathBuf> {
    let resolved = PathBuf::from(path);
    let canonical = match resolved.canonicalize() {
        Ok(c) => c,
        Err(_) => return None,
    };

    let web_dir = PathBuf::from(get_web_dir());
    let canonical_web_dir = match web_dir.canonicalize() {
        Ok(c) => c,
        Err(_) => return None,
    };

    if canonical.starts_with(&canonical_web_dir) {
        Some(canonical)
    } else {
        None
    }
}

async fn serve_html_file(path: &str) -> actix_web::HttpResponse {
    let validated_path = match validate_html_path(path) {
        Some(p) => p,
        None => return actix_web::HttpResponse::NotFound().finish(),
    };

    match tokio::fs::read_to_string(&validated_path).await {
        Ok(content) => actix_web::HttpResponse::Ok()
            .content_type("text/html")
            .body(content),
        Err(_) => actix_web::HttpResponse::NotFound().finish(),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    setup_panic_handler();

    if let Err(e) = rustls::crypto::ring::default_provider().install_default() {
        tracing::error!("初始化TLS密码学提供者失败: {:?}", e);
        std::process::exit(1);
    }

    setup_logging();

    log_bilingual("log.output_to");
    log_bilingual("system.start");

    let config = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("加载配置文件失败: {:?}", e);
            eprintln!("Error: Failed to load config: {}", e);
            std::process::exit(1);
        }
    };

    log_bilingual("system.config_loaded");

    if let Err(e) = ipma::crypto::check_key_integrity() {
        error!("加密密钥完整性检查失败: {}", e);
    }

    let shutdown = ShutdownSignal::new();

    let pool = if config.init.enabled {
        log_bilingual("system.init_mode_enabled");
        None
    } else {
        match DbPool::new(&config.database).await {
            Ok(p) => {
                info!("数据库连接池创建成功");
                Some(p)
            }
            Err(e) => {
                tracing::error!("创建数据库连接池失败: {:?}", e);
                eprintln!("Error: Failed to create database pool: {}", e);
                std::process::exit(1);
            }
        }
    };

    init_start_time();
    info!("[中文] 系统启动时间初始化完成");
    info!("[English] System startup time initialized");

    let mut running_scheduler: Option<RunningScheduler> = None;

    let task_registry = Arc::new({
        let mut registry = TaskRegistry::new();
        registry.register(Box::new(BackupTaskExecutor));
        registry.register(Box::new(TokenCleanupTaskExecutor));
        registry.register(Box::new(TokenUsageCleanupTaskExecutor));
        registry.register(Box::new(LogCleanupTaskExecutor));
        registry.register(Box::new(MacSyncTaskExecutor));
        registry
    });

    if let Some(ref db_pool) = pool {
        let scheduler_db_config = ipma_data_manager::DatabaseConfig {
            host: db_pool.db_config.host.clone(),
            port: db_pool.db_config.port,
            database: db_pool.db_config.database.clone(),
            username: db_pool.db_config.username.clone(),
            password: db_pool.db_config.password.clone(),
        };

        match SchedulerState::new(
            db_pool.get_conn(),
            scheduler_db_config,
            task_registry.clone(),
        )
        .await
        {
            Ok(mut state) => {
                if let Err(e) = state
                    .add_system_job(
                        "system_backup",
                        "0 0 0 * * *",
                        "backup",
                        serde_json::json!({}),
                    )
                    .await
                {
                    error!("注册备份定时任务失败: {}", e);
                }
                if let Err(e) = state
                    .add_system_job(
                        "system_token_cleanup",
                        "0 0 * * * *",
                        "token_cleanup",
                        serde_json::json!({}),
                    )
                    .await
                {
                    error!("注册Token清理定时任务失败: {}", e);
                }
                if let Err(e) = state
                    .add_system_job(
                        "system_usage_cleanup",
                        "0 0 2 * * *",
                        "token_usage_cleanup",
                        serde_json::json!({}),
                    )
                    .await
                {
                    error!("注册Token使用记录清理定时任务失败: {}", e);
                }

                match state.start().await {
                    Ok(running) => {
                        running_scheduler = Some(running);
                        info!("调度器启动成功");
                    }
                    Err(e) => {
                        error!("启动调度器失败: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("创建调度器失败: {}", e);
            }
        }

        db_pool.start_health_check_task(30, shutdown.subscribe());
        info!("数据库连接池健康检查任务已启动");
    }

    let rate_limiter = RateLimiter::new(
        config.rate_limit.ip_limit,
        config.rate_limit.user_limit,
        config.rate_limit.login_limit,
        config.rate_limit.window_secs,
    )
    .with_email_limit(
        config.rate_limit.email_limit,
        config.rate_limit.email_window_secs,
    );
    let rate_limit_enabled = config.rate_limit.enabled;

    if rate_limit_enabled {
        start_cleanup_task(rate_limiter.clone(), shutdown.subscribe());
        info!("速率限制中间件已启用");
        info!(
            "IP限制: {}/{}秒",
            config.rate_limit.ip_limit, config.rate_limit.window_secs
        );
        info!(
            "用户限制: {}/{}秒",
            config.rate_limit.user_limit, config.rate_limit.window_secs
        );
        info!(
            "登录限制: {}/{}秒",
            config.rate_limit.login_limit, config.rate_limit.window_secs
        );
        info!(
            "邮件发送限制: {}/{}秒",
            config.rate_limit.email_limit, config.rate_limit.email_window_secs
        );
    }

    let server_host_raw = config.server.host.trim().to_string();
    let server_host_ipv6_raw = config
        .server
        .host_ipv6
        .as_ref()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    fn validate_ip_address(addr: &str) -> Result<String, String> {
        if addr.is_empty() {
            return Ok(String::new());
        }
        if addr.parse::<std::net::IpAddr>().is_ok() {
            Ok(addr.to_string())
        } else {
            Err(format!("无效的IP地址格式: {}", addr))
        }
    }

    let validated_ipv4 = validate_ip_address(&server_host_raw).map_err(std::io::Error::other)?;
    let validated_ipv6 =
        validate_ip_address(&server_host_ipv6_raw).map_err(std::io::Error::other)?;

    let (server_host, server_host_ipv6) = if validated_ipv4.is_empty() && validated_ipv6.is_empty()
    {
        error!("配置错误：IPv4和IPv6监听地址均为空，至少需要配置一个监听地址");
        panic!("配置错误：IPv4和IPv6监听地址均为空，至少需要配置一个监听地址");
    } else {
        (
            validated_ipv4,
            if validated_ipv6.is_empty() {
                None
            } else {
                Some(validated_ipv6)
            },
        )
    };

    let http_enabled = config.server.http_enabled.unwrap_or(true);
    let http_port = config.server.http_port.unwrap_or(80);

    let web_dir = get_web_dir();
    if !Path::new(web_dir).exists() {
        info!("创建web目录: {}", web_dir);
        fs::create_dir_all(web_dir)?;
    }

    let init_enabled = config.init.enabled;
    let auto_https = config.server.auto_https.unwrap_or(false);

    if init_enabled {
        log_bilingual("system.init_mode_enabled");
    } else {
        log_bilingual("system.init_mode_disabled");
        if auto_https {
            log_bilingual("system.auto_https_enabled");
        }
    }

    let http_version = config
        .server
        .http_version
        .clone()
        .unwrap_or_else(|| "http2".to_string());

    let app_state = Data::new(
        AppState::new(config.clone(), pool.clone(), task_registry.clone())
            .map_err(std::io::Error::other)?,
    );

    app_state
        .jwt_utils
        .start_cache_cleanup_task(shutdown.subscribe());

    let http_rate_limiter = rate_limiter.clone();
    let http_rate_limit_enabled = rate_limit_enabled;
    let http_app_state = app_state.clone();
    let create_http_app = move || {
        let auto_https = http_app_state.config.server.auto_https.unwrap_or(false);
        let enable_normal_routes = !auto_https;

        let mut app = App::new()
            .wrap(Compress::default())
            .wrap(actix_web::middleware::Logger::default())
            .wrap(build_cors_middleware(&http_app_state.config))
            .wrap(RateLimitMiddleware::new(
                http_rate_limiter.clone(),
                http_rate_limit_enabled,
            ))
            .configure(|cfg| {
                configure_app_services(cfg, &http_app_state, enable_normal_routes);
            });

        if !http_app_state.config.init.enabled && auto_https {
            app = app.default_service(web::route().to(https_redirect_handler));
        }

        app
    };

    let https_rate_limiter = rate_limiter.clone();
    let https_rate_limit_enabled = rate_limit_enabled;
    let https_app_state = app_state.clone();
    let https_port = config.server.https_port.unwrap_or(443);
    let create_https_app = move || {
        App::new()
            .wrap(Compress::default())
            .wrap(actix_web::middleware::Logger::default())
            .wrap(ipma::utils::hsts::hsts_middleware())
            .wrap(build_cors_middleware(&https_app_state.config))
            .wrap(RateLimitMiddleware::new(
                https_rate_limiter.clone(),
                https_rate_limit_enabled,
            ))
            .configure(|cfg| configure_app_services(cfg, &https_app_state, true))
    };

    let local_set = tokio::task::LocalSet::new();

    let shutdown_for_signal = shutdown.clone();
    let signal_handle = tokio::spawn(async move {
        wait_for_shutdown_signal(&shutdown_for_signal).await;
    });

    let shutdown_clone = shutdown.clone();
    local_set
        .run_until(async move {
            let mut all_server_handles: Vec<actix_web::dev::ServerHandle> = Vec::new();
            let mut all_server_join_handles: Vec<tokio::task::JoinHandle<std::io::Result<()>>> =
                Vec::new();

            if http_enabled || auto_https {
                let ipv6_address = server_host_ipv6.as_deref().unwrap_or("");
                let ipv4_address = server_host.as_str();

                if !ipv4_address.is_empty() {
                    let http_listener_ipv4 = bind_with_retry(ipv4_address, http_port, 3).await?;
                    let server_ipv4 = HttpServer::new(create_http_app.clone())
                        .workers(std::cmp::max(2, num_cpus::get()))
                        .disable_signals()
                        .listen(http_listener_ipv4)?
                        .run();
                    let handle_ipv4 = server_ipv4.handle();

                    info!(
                        "HTTP IPv4服务器运行在 http://{}:{}",
                        ipv4_address, http_port
                    );
                    info!("HTTP版本: HTTP/1.1 (HTTP/2需要HTTPS)");

                    all_server_handles.push(handle_ipv4);
                    all_server_join_handles.push(tokio::task::spawn_local(server_ipv4));
                }

                if !ipv6_address.is_empty() {
                    let http_listener_ipv6 = bind_with_retry(ipv6_address, http_port, 3).await?;
                    let server_ipv6 = HttpServer::new(create_http_app)
                        .workers(std::cmp::max(2, num_cpus::get()))
                        .disable_signals()
                        .listen(http_listener_ipv6)?
                        .run();
                    let handle_ipv6 = server_ipv6.handle();

                    info!(
                        "HTTP IPv6服务器运行在 http://[{}]:{}",
                        ipv6_address, http_port
                    );
                    info!("HTTP版本: HTTP/1.1 (HTTP/2需要HTTPS)");

                    all_server_handles.push(handle_ipv6);
                    all_server_join_handles.push(tokio::task::spawn_local(server_ipv6));
                }
            }

            let https_enabled = config.server.https_enabled.unwrap_or(true);

            if https_enabled {
                let cert_type = config.server.cert_type.as_deref().unwrap_or("self_signed");
                let (cert_path, key_path) = prepare_server_certificate(&config).await?;

                let ipv6_address = server_host_ipv6.as_deref().unwrap_or("");
                let ipv4_address = server_host.as_str();

                if !ipv4_address.is_empty() {
                    let https_listener_ipv4 = bind_with_retry(ipv4_address, https_port, 3).await?;
                    let tls_config_ipv4 = load_rustls_config(&cert_path, &key_path).await?;

                    info!(
                        "HTTPS IPv4服务器运行在 https://{}:{}",
                        ipv4_address, https_port
                    );
                    info!("HTTP版本: {} (支持HTTP/1.1和HTTP/2)", http_version);
                    info!("证书类型: {}", cert_type);

                    let server_ipv4 = HttpServer::new(create_https_app.clone())
                        .workers(std::cmp::max(2, num_cpus::get()))
                        .disable_signals()
                        .listen_rustls_0_23(https_listener_ipv4, tls_config_ipv4)
                        .map_err(|e| {
                            error!("创建HTTPS IPv4服务器失败: {:?}", e);
                            e
                        })?
                        .run();
                    let handle_ipv4 = server_ipv4.handle();

                    all_server_handles.push(handle_ipv4);
                    all_server_join_handles.push(tokio::task::spawn_local(server_ipv4));
                }

                if !ipv6_address.is_empty() {
                    let https_listener_ipv6 = bind_with_retry(ipv6_address, https_port, 3).await?;
                    let tls_config_ipv6 = load_rustls_config(&cert_path, &key_path).await?;

                    info!(
                        "HTTPS IPv6服务器运行在 https://[{}]:{}",
                        ipv6_address, https_port
                    );
                    info!("HTTP版本: {} (支持HTTP/1.1和HTTP/2)", http_version);
                    info!("证书类型: {}", cert_type);

                    let server_ipv6 = HttpServer::new(create_https_app)
                        .workers(std::cmp::max(2, num_cpus::get()))
                        .disable_signals()
                        .listen_rustls_0_23(https_listener_ipv6, tls_config_ipv6)
                        .map_err(|e| {
                            error!("创建HTTPS IPv6服务器失败: {:?}", e);
                            e
                        })?
                        .run();
                    let handle_ipv6 = server_ipv6.handle();

                    all_server_handles.push(handle_ipv6);
                    all_server_join_handles.push(tokio::task::spawn_local(server_ipv6));
                }
            } else {
                info!("HTTPS服务器已禁用");
            }

            info!("系统启动完成，等待请求...");

            signal_handle.await?;

            let force_shutdown_handle = tokio::spawn(async {
                if let Err(e) = tokio::signal::ctrl_c().await {
                    warn!("注册强制退出信号处理失败: {}", e);
                }
                warn!("收到第二次中断信号，强制退出！");
                std::process::exit(1);
            });

            info!("1. 停止接收新连接...");
            let server_stop_timeout = tokio::time::Duration::from_secs(5);
            let all_stops: Vec<_> = all_server_handles
                .into_iter()
                .enumerate()
                .map(|(idx, handle)| {
                    let stop_future = handle.stop(true);
                    async move {
                        if let Err(e) = tokio::time::timeout(server_stop_timeout, stop_future).await
                        {
                            warn!("服务器 {} 优雅关闭超时: {}", idx, e);
                        }
                    }
                })
                .collect();
            futures_util::future::join_all(all_stops).await;
            info!("服务器已停止接收新连接");

            info!("2. 等待服务器任务结束...");
            let task_wait_timeout = tokio::time::Duration::from_secs(5);
            for (idx, join_handle) in all_server_join_handles.into_iter().enumerate() {
                if let Err(e) = tokio::time::timeout(task_wait_timeout, join_handle).await {
                    warn!("服务器任务 {} 等待超时: {}", idx, e);
                }
            }
            info!("所有服务器任务已结束");

            info!("3. 关闭后台任务...");
            shutdown_clone.request_shutdown();
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            info!("后台任务已发送关闭信号");

            info!("4. 关闭调度器...");
            if let Some(scheduler) = running_scheduler
                && let Err(e) =
                    tokio::time::timeout(tokio::time::Duration::from_secs(5), scheduler.shutdown())
                        .await
            {
                warn!("调度器关闭超时: {}", e);
            }

            info!("5. 关闭数据库连接池...");
            if let Some(db_pool) = pool
                && let Err(e) =
                    tokio::time::timeout(tokio::time::Duration::from_secs(5), db_pool.close()).await
            {
                warn!("数据库连接池关闭超时: {}", e);
            }

            force_shutdown_handle.abort();

            info!("系统已优雅关闭");
            Ok::<(), std::io::Error>(())
        })
        .await?;

    Ok(())
}

fn configure_app_services(
    cfg: &mut web::ServiceConfig,
    app_state: &Data<AppState>,
    enable_normal_routes: bool,
) {
    cfg.app_data(app_state.clone());
    cfg.app_data(
        web::JsonConfig::default()
            .limit(10 * 1024 * 1024)
            .error_handler(json_error_handler),
    );
    cfg.app_data(web::FormConfig::default().limit(50 * 1024 * 1024));
    cfg.app_data(web::PayloadConfig::new(50 * 1024 * 1024));

    if app_state.config.init.enabled {
        // 创建 InitContext 用于初始化模块
        let init_context = Data::new(InitContext {
            db_config: InitDatabaseConfig {
                host: app_state.config.database.host.clone(),
                port: app_state.config.database.port,
                database: app_state.config.database.database.clone(),
                username: app_state.config.database.username.clone(),
                password: app_state.config.database.password.clone(),
                max_connections: app_state.config.database.max_connections,
                min_connections: app_state.config.database.min_connections,
                acquire_timeout_secs: app_state.config.database.acquire_timeout_secs,
                idle_timeout_secs: app_state.config.database.idle_timeout_secs,
                max_lifetime_secs: app_state.config.database.max_lifetime_secs,
                query_timeout_secs: app_state.config.database.query_timeout_secs,
                slow_query_threshold_ms: app_state.config.database.slow_query_threshold_ms,
                health_check_interval_secs: app_state.config.database.health_check_interval_secs,
            },
            config_path: ipma::config::get_config_file_path(),
            init_enabled: app_state.config.init.enabled,
            restart_fn: Arc::new(|| {
                Box::pin(async move {
                    ipma::system::config::trigger_service_restart()
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(())
                })
            }),
        });

        cfg.app_data(init_context);

        cfg.service(
            web::scope("/api/init")
                .wrap(actix_middleware::from_fn(
                    ipma::auth::login::localhost_only_middleware,
                ))
                .route("", web::post().to(ipma_init::init_system))
                .route("/db", web::post().to(ipma_init::init_db))
                .route("/db/clear", web::post().to(ipma_init::clear_database))
                .route("/db/create", web::post().to(ipma_init::create_database_api))
                .route("/db/import", web::post().to(ipma_init::import_database_api))
                .route(
                    "/db/import-file",
                    web::post().to(ipma_init::import_database_from_file),
                )
                .route("/restart", web::post().to(ipma_init::restart_program))
                .route("/status", web::get().to(ipma_init::check_init_status))
                .route("/db-status", web::get().to(ipma_init::check_db_status))
                .route(
                    "/verification-code",
                    web::get().to(ipma_init::get_verification_code),
                )
                .route("/check-pgsql", web::get().to(ipma_init::check_pgsql)),
        )
        .route(
            "/init_index.html",
            web::get().to(|| async {
                let web_dir = get_web_dir();
                let path = format!("{web_dir}/static/init_index.html");
                serve_html_file(&path).await
            }),
        )
        .service(
            Files::new("/static", format!("{}/static", get_web_dir()))
                .prefer_utf8(true)
                .use_etag(true)
                .use_last_modified(true),
        )
        .route(
            "/",
            web::get().to(|| async {
                actix_web::HttpResponse::Found()
                    .insert_header((actix_web::http::header::LOCATION, "/init_index.html"))
                    .finish()
            }),
        );
    } else if enable_normal_routes {
        let static_path = format!("{}/static", get_web_dir());
        cfg.service(
            Files::new("/static", &static_path)
                .prefer_utf8(true)
                .use_etag(true)
                .use_last_modified(true),
        )
        .route(
            "/static/locales/{lang}/{file:.*\\.json}",
            web::get().to(serve_json),
        )
        .route("/static/{path:.*\\.json}", web::get().to(serve_json))
        .configure(init_routes)
        .route(
            "/main.html",
            web::get().to(|| async {
                let web_dir = get_web_dir();
                let path = format!("{web_dir}/static/main.html");
                serve_html_file(&path).await
            }),
        )
        .route(
            "/",
            web::get().to(|| async {
                actix_web::HttpResponse::Found()
                    .insert_header((actix_web::http::header::LOCATION, "/static/index.html"))
                    .finish()
            }),
        );
    }
}
