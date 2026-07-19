use actix_cors::Cors;
use actix_files::Files;
use actix_web::body::MessageBody;
use actix_web::dev::{ServerHandle, ServiceRequest, ServiceResponse};
use actix_web::middleware as actix_middleware;
use actix_web::middleware::Compress;
use actix_web::middleware::Next;
use actix_web::web::Data;
use actix_web::{App, HttpServer, web};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::net::{SocketAddr, TcpListener};
use std::os::unix::net::UnixListener;
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info, warn};

use ipma::app_state::AppState;
use ipma::log::setup_logging;
use ipma::routes::static_files::{
    get_web_dir, https_redirect_handler, json_error_handler, serve_json,
};

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

async fn static_cache_control_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    let path = req.path().to_string();
    let mut res = next.call(req).await?;

    if path.starts_with("/static/") {
        res.headers_mut().insert(
            actix_web::http::header::CACHE_CONTROL,
            actix_web::http::header::HeaderValue::from_static("no-cache, must-revalidate"),
        );
    }

    Ok(res)
}

async fn security_headers_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    let mut res = next.call(req).await?;

    // frame-ancestors 只能通过 HTTP 头设置，不能通过 <meta> 元素传递
    res.headers_mut().insert(
        actix_web::http::header::HeaderName::from_static("content-security-policy"),
        actix_web::http::header::HeaderValue::from_static("frame-ancestors 'none'"),
    );

    Ok(res)
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

/// 创建 UDS 监听器
///
/// - 自动创建父目录
/// - 清理已存在的 socket 文件，避免 "Address already in use"
/// - 设置 socket 文件权限 0666，允许 nginx (www-data) 等其他用户进程访问
///   安全考虑：socket 文件本身不存储敏感数据，应用层有 JWT 认证保护，
///   且内核保证 bind 路径不可被重新 bind，因此 0666 不会导致劫持风险
///   生产环境若需更严格权限，可通过 systemd SocketUser/SocketGroup 实现
fn create_uds_listener(path: &str) -> std::io::Result<UnixListener> {
    let socket_path = Path::new(path);

    // 确保父目录存在
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // 清理已存在的 socket 文件
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    let listener = UnixListener::bind(path)?;
    // 设置 socket 文件权限 0666：允许 nginx 等其他用户进程访问
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666))?;

    Ok(listener)
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

    // 启动应用层 fail2ban 清理任务
    ipma::system::app_fail2ban::start_cleanup_task();
    info!("[中文] 应用层 Fail2ban 清理任务已启动");
    info!("[English] Application fail2ban cleanup task started");

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

    let uds_path = config.server.listen.uds_path.clone();
    let serve_static = config.server.listen.serve_static;
    let debug_tcp_port = config.server.listen.debug_tcp_port;

    let web_dir = get_web_dir();
    if serve_static && !Path::new(web_dir).exists() {
        info!("创建web目录: {}", web_dir);
        fs::create_dir_all(web_dir)?;
    }

    let init_enabled = config.init.enabled;

    if init_enabled {
        log_bilingual("system.init_mode_enabled");
    } else {
        log_bilingual("system.init_mode_disabled");
    }

    info!("监听方式: UDS ({})", uds_path);
    info!(
        "静态文件托管: {}",
        if serve_static {
            "启用（actix 直接服务）"
        } else {
            "禁用（由 nginx 托管）"
        }
    );
    if debug_tcp_port > 0 {
        info!("调试 TCP 端口: {} (curl 直连测试用)", debug_tcp_port);
    }

    let app_state = Data::new(
        AppState::new(config.clone(), pool.clone(), task_registry.clone())
            .map_err(std::io::Error::other)?,
    );

    app_state
        .jwt_utils
        .start_cache_cleanup_task(shutdown.subscribe());

    let app_rate_limiter = rate_limiter.clone();
    let app_rate_limit_enabled = rate_limit_enabled;
    let app_state_for_app = app_state.clone();
    let create_app = move || {
        let mut app = App::new()
            .wrap(Compress::default())
            .wrap(actix_web::middleware::Logger::default())
            .wrap(build_cors_middleware(&app_state_for_app.config))
            .wrap(RateLimitMiddleware::new(
                app_rate_limiter.clone(),
                app_rate_limit_enabled,
            ))
            .wrap(actix_middleware::from_fn(static_cache_control_middleware))
            .wrap(actix_middleware::from_fn(security_headers_middleware))
            .configure(|cfg| {
                configure_app_services(cfg, &app_state_for_app, serve_static);
            });

        // init 模式下保留 https_redirect_handler 作为默认服务（用于 auto_https 兼容旧配置）
        if !app_state_for_app.config.init.enabled
            && app_state_for_app.config.server.auto_https.unwrap_or(false)
        {
            app = app.default_service(web::route().to(https_redirect_handler));
        }

        app
    };

    let local_set = tokio::task::LocalSet::new();

    let shutdown_for_signal = shutdown.clone();
    let signal_handle = tokio::spawn(async move {
        wait_for_shutdown_signal(&shutdown_for_signal).await;
    });

    let shutdown_clone = shutdown.clone();
    local_set
        .run_until(async move {
            let mut all_server_handles: Vec<ServerHandle> = Vec::new();
            let mut all_server_join_handles: Vec<tokio::task::JoinHandle<std::io::Result<()>>> =
                Vec::new();

            let workers = std::cmp::max(2, num_cpus::get());
            let keep_alive = std::time::Duration::from_secs(5);

            // UDS 主监听器
            let uds_listener = create_uds_listener(&uds_path)?;
            let server_uds = HttpServer::new(create_app.clone())
                .workers(workers)
                .keep_alive(keep_alive)
                .disable_signals()
                .listen_uds(uds_listener)?
                .run();
            let handle_uds = server_uds.handle();
            info!("UDS 服务器启动: {}", uds_path);
            all_server_handles.push(handle_uds);
            all_server_join_handles.push(tokio::task::spawn_local(server_uds));

            // 可选 TCP 调试端口（便于 curl 直连测试 API）
            if debug_tcp_port > 0 {
                let tcp_listener = create_tcp_listener("127.0.0.1", debug_tcp_port)?;
                let server_tcp = HttpServer::new(create_app)
                    .workers(2)
                    .keep_alive(keep_alive)
                    .disable_signals()
                    .listen(tcp_listener)?
                    .run();
                let handle_tcp = server_tcp.handle();
                info!("调试 TCP 服务器启动: http://127.0.0.1:{}", debug_tcp_port);
                all_server_handles.push(handle_tcp);
                all_server_join_handles.push(tokio::task::spawn_local(server_tcp));
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

            info!("1. 停止接收新连接并等待请求完成...");
            let server_stop_timeout = tokio::time::Duration::from_secs(10);
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
            let task_wait_timeout = tokio::time::Duration::from_secs(3);
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
    serve_static: bool,
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
        );

        // 仅在 serve_static=true 时托管前端文件
        // 生产模式（UDS + nginx）应禁用，由 nginx 直接服务静态资源
        if serve_static {
            cfg.route(
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
        }
    } else if serve_static {
        // 开发模式：actix 直接托管静态文件 + main.html
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
    } else {
        // 生产模式：仅注册 API 路由，静态文件由 nginx 托管
        cfg.configure(init_routes);
    }
}
