// IPMA - IP/MAC Address Management System
// Copyright (c) 2024-2025 oi-io <boss@oi-io.cc>
// SPDX-License-Identifier: MIT

use actix_cors::Cors;
use actix_files::Files;
use actix_web::web::Data;
use actix_web::{App, HttpServer, web};
use std::path::Path;
use tracing::{error, info};

use ipma::log::setup_logging;
use ipma::routes::static_files::{
    get_web_dir, https_redirect_handler, json_error_handler, serve_json,
};
use ipma::system::cert::{load_rustls_config, prepare_server_certificate};

use ipma::config::Config;
use ipma::db::DbPool;
use ipma::routes::init_routes;
use ipma::system::config::init_start_time;
use ipma::system::cron::start_scheduler;
use ipma::utils::log_bilingual;
use ipma::utils::rate_limit::{RateLimitMiddleware, RateLimiter, start_cleanup_task};

fn build_cors_middleware() -> Cors {
    Cors::default()
        .allowed_origin("http://localhost")
        .allowed_origin("http://localhost:80")
        .allowed_origin("http://localhost:443")
        .allowed_origin("https://localhost")
        .allowed_origin_fn(|origin, _req_head| {
            if let Ok(origin_str) = origin.to_str() {
                origin_str.starts_with("http://localhost:")
                    || origin_str.starts_with("https://localhost:")
                    || origin_str.starts_with("http://127.0.0.1:")
                    || origin_str.starts_with("https://127.0.0.1:")
            } else {
                false
            }
        })
        .allow_any_method()
        .allow_any_header()
        .supports_credentials()
        .max_age(3600)
}
use std::fs;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 初始化 rustls 密码学提供者
    if let Err(e) = rustls::crypto::ring::default_provider().install_default() {
        tracing::error!("初始化TLS密码学提供者失败: {:?}", e);
        std::process::exit(1);
    }

    // 初始化日志
    let _log_file_path = setup_logging();

    // 使用双语日志
    log_bilingual("log.output_to");

    // 使用双语日志
    log_bilingual("system.start");

    // 加载配置
    let config = Config::load().expect("Failed to load config");

    // 使用双语日志（不记录敏感信息）
    log_bilingual("system.config_loaded");

    // 条件创建数据库连接池
    let pool = if !config.init.enabled {
        let p = DbPool::new(&config.database)
            .await
            .expect("Failed to create database pool");
        info!("数据库连接池创建成功");

        if let Err(e) = ipma::init::schema::run_migrations_only(&p.pool).await {
            tracing::error!("数据库迁移失败: {}", e);
        }

        Some(p)
    } else {
        // 使用双语日志
        log_bilingual("system.init_mode_enabled");
        None
    };

    // 初始化系统启动时间
    init_start_time();
    // 系统启动时间初始化完成的日志使用简单信息
    info!("[中文] 系统启动时间初始化完成");
    info!("[English] System startup time initialized");

    // 只有在非初始化模式下才启动cron调度器和健康检查
    if let Some(ref db_pool) = pool {
        // 启动cron调度器
        use std::sync::Arc;
        let pool_for_scheduler = Arc::new(db_pool.clone());
        tokio::spawn(async move {
            if let Err(e) = start_scheduler(pool_for_scheduler).await {
                error!("启动cron调度器失败: {:?}", e);
            }
        });

        // 启动数据库连接池健康检查任务（每30秒检查一次）
        db_pool.start_health_check_task(30);
        info!("数据库连接池健康检查任务已启动");
    }

    // 创建速率限制器
    let rate_limiter = RateLimiter::new(
        config.rate_limit.ip_limit,
        config.rate_limit.user_limit,
        config.rate_limit.login_limit,
        config.rate_limit.window_secs,
    );
    let rate_limit_enabled = config.rate_limit.enabled;

    // 启动速率限制清理任务
    if rate_limit_enabled {
        start_cleanup_task(rate_limiter.clone()).await;
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
    }

    // 保存服务器配置用于绑定和日志
    let server_host = config.server.host.clone(); // IPv4地址
    let server_host_ipv6 = config.server.host_ipv6.clone(); // IPv6地址
    let http_enabled = config.server.http_enabled.unwrap_or(true);
    let http_port = config.server.http_port.unwrap_or(80);

    // 检查前端资源目录是否存在
    let web_dir = get_web_dir();
    if !Path::new(web_dir).exists() {
        info!("创建web目录: {}", web_dir);
        fs::create_dir_all(web_dir)?;
    }

    // 输出HTTP服务器启动信息
    let init_enabled = config.init.enabled;
    let auto_https = config.server.auto_https.unwrap_or(false);

    if init_enabled {
        // 使用双语日志
        log_bilingual("system.init_mode_enabled");
    } else {
        // 使用双语日志
        log_bilingual("system.init_mode_disabled");
        if auto_https {
            // 使用双语日志
            log_bilingual("system.auto_https_enabled");
        }
    }

    // 创建应用工厂函数（用于HTTP服务器）
    let http_config = config.clone();
    let http_pool = pool.clone();
    let http_rate_limiter = rate_limiter.clone();
    let http_rate_limit_enabled = rate_limit_enabled;
    let create_http_app = move || {
        let auto_https = http_config.server.auto_https.unwrap_or(false);
        let enable_normal_routes = !auto_https;

        let mut app = App::new()
            .wrap(build_cors_middleware())
            .wrap(actix_web::middleware::Logger::default())
            .wrap(RateLimitMiddleware::new(
                http_rate_limiter.clone(),
                http_rate_limit_enabled,
            ))
            .configure(|cfg| {
                configure_app_services(cfg, &http_config, &http_pool, enable_normal_routes)
            });

        if !http_config.init.enabled && auto_https {
            app = app.default_service(web::route().to(https_redirect_handler));
        }

        app
    };

    // 创建应用工厂函数（用于HTTPS服务器）
    let https_config = config.clone();
    let https_pool = pool.clone();
    let https_rate_limiter = rate_limiter.clone();
    let https_rate_limit_enabled = rate_limit_enabled;
    let create_https_app = move || {
        App::new()
            .wrap(ipma::utils::hsts::hsts_middleware())
            .wrap(build_cors_middleware())
            .wrap(actix_web::middleware::Logger::default())
            .wrap(RateLimitMiddleware::new(
                https_rate_limiter.clone(),
                https_rate_limit_enabled,
            ))
            .configure(|cfg| configure_app_services(cfg, &https_config, &https_pool, true))
    };

    // 处理HTTP服务器
    if http_enabled || auto_https {
        // 检查IPv6地址配置
        let ipv6_address = server_host_ipv6.as_deref().unwrap_or("");
        let ipv4_address = server_host.as_str();

        // 情况1：IPv6地址是::，只启动IPv6服务器（双栈模式）
        if !ipv6_address.is_empty() && ipv6_address == "::" {
            let http_server =
                HttpServer::new(create_http_app).workers(std::cmp::max(2, num_cpus::get()));

            // HTTP服务器只支持HTTP/1.1（HTTP/2需要TLS支持）
            let http_server = http_server.bind((ipv6_address, http_port))?;

            info!("HTTP服务器运行在 http://{}:{}", ipv6_address, http_port);
            info!("HTTP版本: HTTP/1.1 (HTTP/2需要HTTPS)");
            info!("使用双栈模式 (IPv4 和 IPv6)");

            actix_web::rt::spawn(async move {
                if let Err(e) = http_server.run().await {
                    error!("HTTP服务器失败: {:?}", e);
                }
            });
        } else {
            // 情况2：启动多个服务器
            // 启动IPv4服务器
            if !ipv4_address.is_empty() {
                let http_server_ipv4 = HttpServer::new(create_http_app.clone())
                    .workers(std::cmp::max(2, num_cpus::get()));

                // HTTP服务器只支持HTTP/1.1（HTTP/2需要TLS支持）
                let http_server_ipv4 = http_server_ipv4.bind((ipv4_address, http_port))?;

                info!(
                    "HTTP IPv4服务器运行在 http://{}:{}",
                    ipv4_address, http_port
                );
                info!("HTTP版本: HTTP/1.1 (HTTP/2需要HTTPS)");

                actix_web::rt::spawn(async move {
                    if let Err(e) = http_server_ipv4.run().await {
                        error!("HTTP IPv4服务器失败: {:?}", e);
                    }
                });
            }

            // 启动IPv6服务器
            if !ipv6_address.is_empty() && ipv6_address != "::" {
                let http_server_ipv6 =
                    HttpServer::new(create_http_app).workers(std::cmp::max(2, num_cpus::get()));

                // HTTP服务器只支持HTTP/1.1（HTTP/2需要TLS支持）
                let http_server_ipv6 = http_server_ipv6.bind((ipv6_address, http_port))?;

                info!(
                    "HTTP IPv6服务器运行在 http://{}:{}",
                    ipv6_address, http_port
                );
                info!("HTTP版本: HTTP/1.1 (HTTP/2需要HTTPS)");

                actix_web::rt::spawn(async move {
                    if let Err(e) = http_server_ipv6.run().await {
                        error!("HTTP IPv6服务器失败: {:?}", e);
                    }
                });
            }
        }
    }

    // 启用HTTPS服务器（默认启用）
    let http_version = config.server.http_version.as_deref().unwrap_or("HTTP/1.1");
    let https_port = 443;

    // 准备服务器证书
    let cert_type = config.server.cert_type.as_deref().unwrap_or("self_signed");
    let (cert_path, key_path) = prepare_server_certificate(&config)?;

    // 检查IPv6地址配置
    let ipv6_address = server_host_ipv6.as_deref().unwrap_or("");
    let ipv4_address = server_host.as_str();

    // 启动HTTP/3服务器的辅助函数
    fn start_http3_server_if_enabled(
        http_version: &str,
        host: &str,
        port: u16,
        cert_path: &str,
        key_path: &str,
        config: &Config,
        pool: &Option<DbPool>,
    ) {
        if http_version == "HTTP/3" {
            let host_str = host.to_string();
            let cert_path_str = cert_path.to_string();
            let key_path_str = key_path.to_string();
            let port_clone = port;
            let config_clone = config.clone();
            let pool_clone = pool.clone();

            tokio::spawn(async move {
                use std::sync::Arc;
                use tokio::runtime::Handle;
                use tokio::sync::Semaphore;

                let semaphore = Arc::new(Semaphore::new(100)); // 限制并发连接数为100
                let rt_handle = Handle::current();

                let app_state = ipma::system::http3::AppState {
                    config: config_clone,
                    pool: pool_clone,
                    semaphore,
                    rt_handle,
                };

                let host_str_clone = host_str.clone();
                match ipma::system::http3::start_http3_server(
                    host_str,
                    port_clone,
                    &cert_path_str,
                    &key_path_str,
                    app_state,
                )
                .await
                {
                    Ok(_) => info!("HTTP/3服务器启动成功 ({}:{})", host_str_clone, port_clone),
                    Err(e) => info!(
                        "启动HTTP/3服务器失败 ({}:{}): {:?}",
                        host_str_clone, port_clone, e
                    ),
                }
            });
        }
    }

    // 情况1：IPv6地址是::，只启动IPv6服务器（双栈模式）
    if !ipv6_address.is_empty() && ipv6_address == "::" {
        let https_server =
            HttpServer::new(create_https_app).workers(std::cmp::max(2, num_cpus::get()));

        let https_server = match http_version {
            "HTTP/1.1" => https_server,
            "HTTP/2" => https_server,
            "HTTP/3" => {
                info!("启动HTTP/3服务器...");
                https_server
            }
            _ => https_server,
        };

        let https_server = https_server.bind_rustls_0_23(
            (ipv6_address, https_port),
            load_rustls_config(&cert_path, &key_path)?,
        )?;

        info!("HTTPS服务器运行在 https://{}:{}", ipv6_address, https_port);
        info!("HTTP版本: {}", http_version);
        info!("证书类型: {}", cert_type);
        info!("使用双栈模式 (IPv4 和 IPv6)");

        // 启动 HTTP/3 服务器
        start_http3_server_if_enabled(
            http_version,
            ipv6_address,
            https_port,
            &cert_path.to_string(),
            &key_path.to_string(),
            &config,
            &pool,
        );

        return https_server.run().await;
    } else {
        // 情况2：启动多个服务器
        let mut server_handles = Vec::new();

        // 启动IPv4服务器
        if !ipv4_address.is_empty() {
            let create_https_app_ipv4 = create_https_app.clone();
            let cert_path_ipv4 = cert_path.clone();
            let key_path_ipv4 = key_path.clone();
            let ipv4_address_clone = ipv4_address.to_string();
            let http_version_clone = http_version.to_string();
            let cert_type_clone = cert_type.to_string();
            let https_port_clone = https_port;

            let config_clone_ipv4 = config.clone();
            let pool_clone_ipv4 = pool.clone();
            server_handles.push(tokio::spawn(async move {
                let https_server_ipv4 = HttpServer::new(create_https_app_ipv4)
                    .workers(std::cmp::max(2, num_cpus::get()));

                // 加载TLS配置（已包含ALPN协议支持，自动启用HTTP/2）
                let tls_config = match load_rustls_config(&cert_path_ipv4, &key_path_ipv4) {
                    Ok(config) => config,
                    Err(e) => {
                        error!("加载TLS配置失败: {:?}", e);
                        return;
                    }
                };

                let https_server_ipv4 = match https_server_ipv4
                    .bind_rustls_0_23((ipv4_address_clone.as_str(), https_port_clone), tls_config)
                {
                    Ok(server) => server,
                    Err(e) => {
                        error!("绑定HTTPS IPv4服务器失败: {:?}", e);
                        return;
                    }
                };

                info!(
                    "HTTPS IPv4服务器运行在 https://{}:{}",
                    ipv4_address_clone, https_port_clone
                );
                info!("HTTP版本: {} (支持HTTP/1.1和HTTP/2)", http_version_clone);
                info!("证书类型: {}", cert_type_clone);

                // 启动 HTTP/3 服务器
                let config_clone = config_clone_ipv4.clone();
                let pool_clone = pool_clone_ipv4.clone();
                start_http3_server_if_enabled(
                    &http_version_clone,
                    &ipv4_address_clone,
                    https_port_clone,
                    &cert_path_ipv4.to_string(),
                    &key_path_ipv4.to_string(),
                    &config_clone,
                    &pool_clone,
                );

                if let Err(e) = https_server_ipv4.run().await {
                    error!("HTTPS IPv4服务器失败: {:?}", e);
                }
            }));
        }

        // 启动IPv6服务器
        if !ipv6_address.is_empty() && ipv6_address != "::" {
            let create_https_app_ipv6 = create_https_app;
            let cert_path_ipv6 = cert_path;
            let key_path_ipv6 = key_path;
            let ipv6_address_clone = ipv6_address.to_string();
            let http_version_clone = http_version.to_string();
            let cert_type_clone = cert_type.to_string();
            let https_port_clone = https_port;

            let config_clone_ipv6 = config.clone();
            let pool_clone_ipv6 = pool.clone();
            server_handles.push(tokio::spawn(async move {
                let https_server_ipv6 = HttpServer::new(create_https_app_ipv6)
                    .workers(std::cmp::max(2, num_cpus::get()));

                // 加载TLS配置（已包含ALPN协议支持，自动启用HTTP/2）
                let tls_config = match load_rustls_config(&cert_path_ipv6, &key_path_ipv6) {
                    Ok(config) => config,
                    Err(e) => {
                        error!("加载TLS配置失败: {:?}", e);
                        return;
                    }
                };

                let https_server_ipv6 = match https_server_ipv6
                    .bind_rustls_0_23((ipv6_address_clone.as_str(), https_port_clone), tls_config)
                {
                    Ok(server) => server,
                    Err(e) => {
                        error!("绑定HTTPS IPv6服务器失败: {:?}", e);
                        return;
                    }
                };

                info!(
                    "HTTPS IPv6服务器运行在 https://{}:{}",
                    ipv6_address_clone, https_port_clone
                );
                info!("HTTP版本: {} (支持HTTP/1.1和HTTP/2)", http_version_clone);
                info!("证书类型: {}", cert_type_clone);

                // 启动 HTTP/3 服务器
                let config_clone = config_clone_ipv6.clone();
                let pool_clone = pool_clone_ipv6.clone();
                start_http3_server_if_enabled(
                    &http_version_clone,
                    &ipv6_address_clone,
                    https_port_clone,
                    &cert_path_ipv6,
                    &key_path_ipv6,
                    &config_clone,
                    &pool_clone,
                );

                if let Err(e) = https_server_ipv6.run().await {
                    error!("HTTPS IPv6服务器失败: {:?}", e);
                }
            }));
        }

        // 等待所有服务器启动
        if !server_handles.is_empty() {
            let (_result, _index, remaining) =
                futures_util::future::select_all(server_handles).await;
            for handle in remaining {
                let _ = handle.await;
            }
        }

        Ok(())
    }
}

fn configure_app_services(
    cfg: &mut web::ServiceConfig,
    config: &Config,
    pool: &Option<DbPool>,
    enable_normal_routes: bool,
) {
    // 初始化应用数据
    cfg.app_data(Data::new(config.clone()));
    // 配置JSON请求体大小限制（10MB）
    cfg.app_data(
        web::JsonConfig::default()
            .limit(10 * 1024 * 1024)
            .error_handler(json_error_handler),
    );
    // 配置表单请求体大小限制（50MB，用于文件上传）
    cfg.app_data(web::FormConfig::default().limit(50 * 1024 * 1024));
    // 配置Payload大小限制
    cfg.app_data(web::PayloadConfig::new(50 * 1024 * 1024));

    if let Some(pool) = pool {
        cfg.app_data(Data::new(pool.clone()));
    }

    if config.init.enabled {
        cfg.service(
            web::scope("/api/init")
                .route("", web::post().to(ipma::init::init_system))
                .route("/db", web::post().to(ipma::init::init_db))
                .route("/db/clear", web::post().to(ipma::init::clear_database))
                .route(
                    "/db/create",
                    web::post().to(ipma::init::create_database_api),
                )
                .route(
                    "/db/import",
                    web::post().to(ipma::init::import_database_api),
                )
                .route(
                    "/db/import-file",
                    web::post().to(ipma::init::import_database_from_file),
                )
                .route("/restart", web::post().to(ipma::init::restart_program))
                .route("/status", web::get().to(ipma::init::check_init_status))
                .route("/db-status", web::get().to(ipma::init::check_db_status))
                .route(
                    "/verification-code",
                    web::get().to(ipma::init::get_verification_code),
                )
                .route("/check-pgsql", web::get().to(ipma::init::check_pgsql)),
        )
        // 配置初始化页面路由
        .route(
            "/init_index.html",
            web::get().to(|| async {
                let web_dir = get_web_dir();
                let path = format!("{}/static/init_index.html", web_dir);
                actix_web::HttpResponse::Ok()
                    .content_type("text/html")
                    .body(
                        std::fs::read_to_string(&path)
                            .unwrap_or_else(|e| format!("Error reading init_index.html: {}", e)),
                    )
            }),
        )
        // 配置静态文件服务
        .service(
            Files::new("/static", format!("{}/static", get_web_dir()))
                .prefer_utf8(true)
                .use_etag(true),
        )
        // 根路径跳转到初始化页面
        .route(
            "/",
            web::get().to(|| async {
                actix_web::HttpResponse::Found()
                    .insert_header((actix_web::http::header::LOCATION, "/init_index.html"))
                    .finish()
            }),
        );
    } else if enable_normal_routes {
        // 关闭初始化时，注册完整的应用系统路由
        let static_path = format!("{}/static", get_web_dir());
        cfg.service(
            Files::new("/static", &static_path)
                .prefer_utf8(true)
                .use_etag(true),
        )
        // 添加专门处理 JSON 文件的路径
        .route(
            "/static/locales/{lang}/{file:.*\\.json}",
            web::get().to(serve_json),
        )
        .route("/static/{path:.*\\.json}", web::get().to(serve_json))
        // 配置完整API路由
        .configure(init_routes)
        // 配置应用首页路由
        .route(
            "/main.html",
            web::get().to(|| async {
                let web_dir = get_web_dir();
                let path = format!("{}/static/main.html", web_dir);
                actix_web::HttpResponse::Ok()
                    .content_type("text/html")
                    .body(
                        std::fs::read_to_string(&path)
                            .unwrap_or_else(|e| format!("Error reading main.html: {}", e)),
                    )
            }),
        )
        // 根路径跳转到登录页面
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
