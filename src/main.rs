// IPMA - IP/MAC Address Management System
// Copyright (c) 2024-2025 oi-io <boss@oi-io.cc>
// SPDX-License-Identifier: MIT

use actix_cors::Cors;
use actix_files::Files;
use actix_web::web::Data;
use actix_web::{App, HttpServer, web};
use std::net::TcpListener;
use std::path::Path;
use tracing::{error, info, warn};

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

fn build_cors_middleware(config: &Config) -> Cors {
    let mut cors = Cors::default()
        .allow_any_method()
        .allow_any_header()
        .supports_credentials()
        .max_age(3600);

    let allowed_origins = config.server.cors_allowed_origins.clone();

    for origin in &allowed_origins {
        cors = cors.allowed_origin(origin.as_str());
    }

    cors = cors.allowed_origin_fn(move |origin, _req_head| {
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
            origin_str.starts_with("http://localhost:")
                || origin_str.starts_with("https://localhost:")
                || origin_str.starts_with("http://127.0.0.1:")
                || origin_str.starts_with("https://127.0.0.1:")
                || origin_str.starts_with("http://[::1]:")
                || origin_str.starts_with("https://[::1]:")
        } else {
            false
        }
    });

    cors
}
use std::fs;

fn check_port_available(addr: &str, port: u16) -> std::io::Result<()> {
    match TcpListener::bind((addr, port)) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            warn!("端口 {}:{} 已被占用，等待释放...", addr, port);
            Err(e)
        }
        Err(e) => Err(e),
    }
}

fn wait_for_port(addr: &str, port: u16, max_retries: u32) -> std::io::Result<()> {
    for i in 0..max_retries {
        if check_port_available(addr, port).is_ok() {
            return Ok(());
        }
        if i < max_retries - 1 {
            warn!("等待端口 {}:{} 释放... ({}/{})", addr, port, i + 1, max_retries);
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    check_port_available(addr, port)
}

fn is_http3_enabled(http_version: &str) -> bool {
    matches!(http_version.to_lowercase().as_str(), "http3" | "http/3")
}

struct Http3StartParams<'a> {
    http_version: &'a str,
    host: &'a str,
    port: u16,
    cert_path: &'a str,
    key_path: &'a str,
    config: &'a Config,
    pool: &'a Option<DbPool>,
    http_port: u16,
}

fn start_http3_server_if_enabled(params: Http3StartParams) {
    if !is_http3_enabled(params.http_version) {
        return;
    }

    let host_str = params.host.to_string();
    let cert_path_str = params.cert_path.to_string();
    let key_path_str = params.key_path.to_string();
    let config_clone = params.config.clone();
    let pool_clone = params.pool.clone();
    let port = params.port;
    let http_port = params.http_port;

    tokio::spawn(async move {
        use std::sync::Arc;
        use tokio::sync::Semaphore;

        let semaphore = Arc::new(Semaphore::new(100));

        let app_state = ipma::system::http3::AppState {
            config: config_clone,
            pool: pool_clone,
            semaphore,
            http_port,
        };

        let host_str_clone = host_str.clone();
        match ipma::system::http3::start_http3_server(
            host_str,
            port,
            &cert_path_str,
            &key_path_str,
            app_state,
        )
        .await
        {
            Ok(()) => info!("HTTP/3服务器启动成功 ({}:{})", host_str_clone, port),
            Err(e) => error!(
                "启动HTTP/3服务器失败 ({}:{}): {:?}",
                host_str_clone, port, e
            ),
        }
    });
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    if let Err(e) = rustls::crypto::ring::default_provider().install_default() {
        tracing::error!("初始化TLS密码学提供者失败: {:?}", e);
        std::process::exit(1);
    }

    let _log_file_path = setup_logging();

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

    let pool = if config.init.enabled {
        log_bilingual("system.init_mode_enabled");
        None
    } else {
        match DbPool::new(&config.database).await {
            Ok(p) => {
                info!("数据库连接池创建成功");
                if let Err(e) = ipma::init::schema::run_migrations_only(&p.get_conn()).await {
                    tracing::error!("数据库迁移失败: {}", e);
                    std::process::exit(1);
                }
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

    if let Some(ref db_pool) = pool {
        use std::sync::Arc;
        let pool_for_scheduler = Arc::new(db_pool.clone());
        tokio::spawn(async move {
            if let Err(e) = start_scheduler(pool_for_scheduler).await {
                error!("启动cron调度器失败: {:?}", e);
            }
        });

        db_pool.start_health_check_task(30);
        info!("数据库连接池健康检查任务已启动");
    }

    let rate_limiter = RateLimiter::new(
        config.rate_limit.ip_limit,
        config.rate_limit.user_limit,
        config.rate_limit.login_limit,
        config.rate_limit.window_secs,
    );
    let rate_limit_enabled = config.rate_limit.enabled;

    if rate_limit_enabled {
        start_cleanup_task(rate_limiter.clone());
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

    let server_host = config.server.host.clone();
    let server_host_ipv6 = config.server.host_ipv6.clone();
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

    let http_version = config.server.http_version.as_deref().unwrap_or("http2");
    let http3_enabled = is_http3_enabled(http_version);

    let http_config = config.clone();
    let http_pool = pool.clone();
    let http_rate_limiter = rate_limiter.clone();
    let http_rate_limit_enabled = rate_limit_enabled;
    let create_http_app = move || {
        let auto_https = http_config.server.auto_https.unwrap_or(false);
        let enable_normal_routes = !auto_https;

        let mut app = App::new()
            .wrap(build_cors_middleware(&http_config))
            .wrap(actix_web::middleware::Logger::default())
            .wrap(RateLimitMiddleware::new(
                http_rate_limiter.clone(),
                http_rate_limit_enabled,
            ))
            .configure(|cfg| {
                configure_app_services(cfg, &http_config, &http_pool, enable_normal_routes);
            });

        if !http_config.init.enabled && auto_https {
            app = app.default_service(web::route().to(https_redirect_handler));
        }

        app
    };

    let https_config = config.clone();
    let https_pool = pool.clone();
    let https_rate_limiter = rate_limiter.clone();
    let https_rate_limit_enabled = rate_limit_enabled;
    let https_port = config.server.https_port.unwrap_or(443);
    let create_https_app = move || {
        App::new()
            .wrap(ipma::utils::alt_svc::AltSvc::new(http3_enabled, https_port))
            .wrap(ipma::utils::hsts::hsts_middleware())
            .wrap(build_cors_middleware(&https_config))
            .wrap(actix_web::middleware::Logger::default())
            .wrap(RateLimitMiddleware::new(
                https_rate_limiter.clone(),
                https_rate_limit_enabled,
            ))
            .configure(|cfg| configure_app_services(cfg, &https_config, &https_pool, true))
    };

    if http_enabled || auto_https {
        let ipv6_address = server_host_ipv6.as_deref().unwrap_or("");
        let ipv4_address = server_host.as_str();

        if !ipv6_address.is_empty() && ipv6_address == "::" {
            wait_for_port(ipv6_address, http_port, 10)?;
            let http_server =
                HttpServer::new(create_http_app).workers(std::cmp::max(2, num_cpus::get()));

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
            if !ipv4_address.is_empty() {
                wait_for_port(ipv4_address, http_port, 10)?;
                let http_server_ipv4 = HttpServer::new(create_http_app.clone())
                    .workers(std::cmp::max(2, num_cpus::get()));

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

            if !ipv6_address.is_empty() && ipv6_address != "::" {
                wait_for_port(ipv6_address, http_port, 10)?;
                let http_server_ipv6 =
                    HttpServer::new(create_http_app).workers(std::cmp::max(2, num_cpus::get()));

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

    let https_enabled = config.server.https_enabled.unwrap_or(true);

    if https_enabled {
        let cert_type = config.server.cert_type.as_deref().unwrap_or("self_signed");
        let (cert_path, key_path) = prepare_server_certificate(&config)?;

        let ipv6_address = server_host_ipv6.as_deref().unwrap_or("");
        let ipv4_address = server_host.as_str();

        if !ipv6_address.is_empty() && ipv6_address == "::" {
            wait_for_port(ipv6_address, https_port, 10)?;
            let https_server =
                HttpServer::new(create_https_app).workers(std::cmp::max(2, num_cpus::get()));

            let https_server = https_server.bind_rustls_0_23(
                (ipv6_address, https_port),
                load_rustls_config(&cert_path, &key_path, http3_enabled)?,
            )?;

            info!("HTTPS服务器运行在 https://{}:{}", ipv6_address, https_port);
            if http3_enabled {
                info!("HTTP版本: HTTP/3 (同时支持HTTP/1.1和HTTP/2 over TCP, HTTP/3 over QUIC/UDP)");
            } else {
                info!("HTTP版本: {} (支持HTTP/1.1和HTTP/2)", http_version);
            }
            info!("证书类型: {}", cert_type);
            info!("使用双栈模式 (IPv4 和 IPv6)");

            start_http3_server_if_enabled(Http3StartParams {
                http_version,
                host: ipv6_address,
                port: https_port,
                cert_path: &cert_path.clone(),
                key_path: &key_path.clone(),
                config: &config,
                pool: &pool,
                http_port,
            });

            return https_server.run().await;
        } else {
            let mut server_handles = Vec::new();

            if !ipv4_address.is_empty() {
                wait_for_port(ipv4_address, https_port, 10)?;
                let create_https_app_ipv4 = create_https_app.clone();
                let cert_path_ipv4 = cert_path.clone();
                let key_path_ipv4 = key_path.clone();
                let ipv4_address_clone = ipv4_address.to_string();
                let http_version_clone = http_version.to_string();
                let cert_type_clone = cert_type.to_string();
                let https_port_clone = https_port;
                let http3_enabled_clone = http3_enabled;

                let config_clone_ipv4 = config.clone();
                let pool_clone_ipv4 = pool.clone();
                server_handles.push(tokio::spawn(async move {
                    let https_server_ipv4 = HttpServer::new(create_https_app_ipv4)
                        .workers(std::cmp::max(2, num_cpus::get()));

                    let tls_config = match load_rustls_config(&cert_path_ipv4, &key_path_ipv4, http3_enabled_clone) {
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
                    if http3_enabled_clone {
                        info!("HTTP版本: HTTP/3 (同时支持HTTP/1.1和HTTP/2 over TCP, HTTP/3 over QUIC/UDP)");
                    } else {
                        info!("HTTP版本: {} (支持HTTP/1.1和HTTP/2)", http_version_clone);
                    }
                    info!("证书类型: {}", cert_type_clone);

                    let config_clone = config_clone_ipv4.clone();
                    let pool_clone = pool_clone_ipv4.clone();
                    start_http3_server_if_enabled(Http3StartParams {
                        http_version: &http_version_clone,
                        host: &ipv4_address_clone,
                        port: https_port_clone,
                        cert_path: &cert_path_ipv4.clone(),
                        key_path: &key_path_ipv4.clone(),
                        config: &config_clone,
                        pool: &pool_clone,
                        http_port,
                    });

                    if let Err(e) = https_server_ipv4.run().await {
                        error!("HTTPS IPv4服务器失败: {:?}", e);
                    }
                }));
            }

            if !ipv6_address.is_empty() && ipv6_address != "::" {
                wait_for_port(ipv6_address, https_port, 10)?;
                let create_https_app_ipv6 = create_https_app;
                let cert_path_ipv6 = cert_path;
                let key_path_ipv6 = key_path;
                let ipv6_address_clone = ipv6_address.to_string();
                let http_version_clone = http_version.to_string();
                let cert_type_clone = cert_type.to_string();
                let https_port_clone = https_port;
                let http3_enabled_clone = http3_enabled;

                let config_clone_ipv6 = config.clone();
                let pool_clone_ipv6 = pool.clone();
                server_handles.push(tokio::spawn(async move {
                    let https_server_ipv6 = HttpServer::new(create_https_app_ipv6)
                        .workers(std::cmp::max(2, num_cpus::get()));

                    let tls_config = match load_rustls_config(&cert_path_ipv6, &key_path_ipv6, http3_enabled_clone) {
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
                    if http3_enabled_clone {
                        info!("HTTP版本: HTTP/3 (同时支持HTTP/1.1和HTTP/2 over TCP, HTTP/3 over QUIC/UDP)");
                    } else {
                        info!("HTTP版本: {} (支持HTTP/1.1和HTTP/2)", http_version_clone);
                    }
                    info!("证书类型: {}", cert_type_clone);

                    let config_clone = config_clone_ipv6.clone();
                    let pool_clone = pool_clone_ipv6.clone();
                    start_http3_server_if_enabled(Http3StartParams {
                        http_version: &http_version_clone,
                        host: &ipv6_address_clone,
                        port: https_port_clone,
                        cert_path: &cert_path_ipv6,
                        key_path: &key_path_ipv6,
                        config: &config_clone,
                        pool: &pool_clone,
                        http_port,
                    });

                    if let Err(e) = https_server_ipv6.run().await {
                        error!("HTTPS IPv6服务器失败: {:?}", e);
                    }
                }));
            }

            if !server_handles.is_empty() {
                let (_result, _index, remaining) =
                    futures_util::future::select_all(server_handles).await;
                for handle in remaining {
                    if let Err(e) = handle.await {
                        tracing::error!("服务器任务异常退出: {:?}", e);
                    }
                }
            }

            Ok(())
        }
    } else {
        info!("HTTPS服务器已禁用");
        Ok(())
    }
}

fn configure_app_services(
    cfg: &mut web::ServiceConfig,
    config: &Config,
    pool: &Option<DbPool>,
    enable_normal_routes: bool,
) {
    cfg.app_data(Data::new(config.clone()));
    cfg.app_data(
        web::JsonConfig::default()
            .limit(10 * 1024 * 1024)
            .error_handler(json_error_handler),
    );
    cfg.app_data(web::FormConfig::default().limit(50 * 1024 * 1024));
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
        .route(
            "/init_index.html",
            web::get().to(|| async {
                let web_dir = get_web_dir();
                let path = format!("{web_dir}/static/init_index.html");
                actix_web::HttpResponse::Ok()
                    .content_type("text/html")
                    .body(
                        std::fs::read_to_string(&path)
                            .unwrap_or_else(|e| format!("Error reading init_index.html: {e}")),
                    )
            }),
        )
        .service(
            Files::new("/static", format!("{}/static", get_web_dir()))
                .prefer_utf8(true)
                .use_etag(true),
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
                .use_etag(true),
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
                actix_web::HttpResponse::Ok()
                    .content_type("text/html")
                    .body(
                        std::fs::read_to_string(&path)
                            .unwrap_or_else(|e| format!("Error reading main.html: {e}")),
                    )
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
