use actix_cors::Cors;
use actix_files::Files;
use actix_web::web::Data;
use actix_web::{App, HttpServer, web};
use std::path::Path;
use tracing::info;

// 引入模块
// 移除本地模块声明，使用库 crate (ipma)
// mod auth;
// mod db;
// mod log;
// mod models;
// mod routes;
// mod system;
// mod utils;

// 引入重构后的模块组件
use ipma::log::setup_logging;
use ipma::routes::static_files::{
    serve_json, https_redirect_handler, json_error_handler, WEB_DIR
};
use ipma::system::cert::{prepare_server_certificate, load_rustls_config};
use ipma::system::http3::start_http3_server;

use ipma::config::Config;
use ipma::db::DbPool;
use ipma::routes::init_routes;
use ipma::system::config::init_start_time;
use ipma::system::cron::start_scheduler;
use ipma::utils::log_bilingual;
use std::fs;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 初始化 rustls 密码学提供者
    rustls::crypto::ring::default_provider()
        .install_default()
        .unwrap();

    // 初始化日志
    let log_file_path = setup_logging();

    // 使用双语日志
    let mut args = std::collections::HashMap::new();
    args.insert("path", log_file_path.as_str());
    log_bilingual("log.output_to");

    // 使用双语日志
    log_bilingual("system.start");

    // 加载配置
    let config = Config::load().expect("Failed to load config");

    // 使用双语日志
    let config_str = format!("{:?}", config);
    let mut args = std::collections::HashMap::new();
    args.insert("config", config_str.as_str());
    log_bilingual("system.config_loaded");

    // 条件创建数据库连接池
    let pool = if !config.init.enabled {
        let p = DbPool::new(&config.database)
            .await
            .expect("Failed to create database pool");
        info!("数据库连接池创建成功");
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
        tokio::spawn(async move {
            if let Err(e) = start_scheduler().await {
                info!("启动cron调度器失败: {:?}", e);
            }
        });

        // 启动数据库连接池健康检查任务（每30秒检查一次）
        db_pool.start_health_check_task(30);
        info!("数据库连接池健康检查任务已启动");
    }

    // 保存服务器配置用于绑定和日志
    let server_host = config.server.host.clone(); // IPv4地址
    let server_host_ipv6 = config.server.host_ipv6.clone(); // IPv6地址
    let http_enabled = config.server.http_enabled.unwrap_or(true);
    let http_port = config.server.http_port.unwrap_or(80);
    let http_version = config.server.http_version.as_deref().unwrap_or("HTTP/1.1");

    // 检查前端资源目录是否存在
    if !Path::new(WEB_DIR).exists() {
        info!("创建web目录");
        fs::create_dir_all(WEB_DIR)?;
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
    let create_http_app = move || {
        let auto_https = http_config.server.auto_https.unwrap_or(false);
        let enable_normal_routes = !auto_https;

        let mut app = App::new()
            .wrap(
                Cors::default()
                    .allow_any_origin()
                    .allow_any_method()
                    .allow_any_header()
                    .max_age(3600),
            )
            .wrap(actix_web::middleware::Logger::default())
            .configure(|cfg| configure_app_services(cfg, &http_config, &http_pool, enable_normal_routes));

        if !http_config.init.enabled && auto_https {
             app = app.default_service(web::route().to(https_redirect_handler));
        }
        
        app
    };

    // 创建应用工厂函数（用于HTTPS服务器）
    let https_config = config.clone();
    let https_pool = pool.clone();
    let create_https_app = move || {
        App::new()
            .wrap(
                Cors::default()
                    .allow_any_origin()
                    .allow_any_method()
                    .allow_any_header()
                    .max_age(3600),
            )
            .wrap(actix_web::middleware::Logger::default())
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

            let http_server = match http_version {
                "HTTP/1.1" => http_server,
                "HTTP/2" => http_server,
                "HTTP/3" => http_server,
                _ => http_server,
            };

            let http_server = http_server.bind((ipv6_address, http_port))?;

            info!("HTTP服务器运行在 http://{}:{}", ipv6_address, http_port);
            info!("HTTP版本: {}", http_version);
            info!("使用双栈模式 (IPv4 和 IPv6)");

            actix_web::rt::spawn(async move {
                if let Err(e) = http_server.run().await {
                    info!("HTTP服务器失败: {:?}", e);
                }
            });
        } else {
            // 情况2：启动多个服务器
            // 启动IPv4服务器
            if !ipv4_address.is_empty() {
                let http_server_ipv4 = HttpServer::new(create_http_app.clone())
                    .workers(std::cmp::max(2, num_cpus::get()));

                let http_server_ipv4 = match http_version {
                    "HTTP/1.1" => http_server_ipv4,
                    "HTTP/2" => http_server_ipv4,
                    "HTTP/3" => http_server_ipv4,
                    _ => http_server_ipv4,
                };

                let http_server_ipv4 = http_server_ipv4.bind((ipv4_address, http_port))?;

                info!(
                    "HTTP IPv4服务器运行在 http://{}:{}",
                    ipv4_address, http_port
                );

                actix_web::rt::spawn(async move {
                    if let Err(e) = http_server_ipv4.run().await {
                        info!("HTTP IPv4服务器失败: {:?}", e);
                    }
                });
            }

            // 启动IPv6服务器
            if !ipv6_address.is_empty() && ipv6_address != "::" {
                let http_server_ipv6 =
                    HttpServer::new(create_http_app).workers(std::cmp::max(2, num_cpus::get()));

                let http_server_ipv6 = match http_version {
                    "HTTP/1.1" => http_server_ipv6,
                    "HTTP/2" => http_server_ipv6,
                    "HTTP/3" => http_server_ipv6,
                    _ => http_server_ipv6,
                };

                let http_server_ipv6 = http_server_ipv6.bind((ipv6_address, http_port))?;

                info!(
                    "HTTP IPv6服务器运行在 http://{}:{}",
                    ipv6_address, http_port
                );

                actix_web::rt::spawn(async move {
                    if let Err(e) = http_server_ipv6.run().await {
                        info!("HTTP IPv6服务器失败: {:?}", e);
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
        if http_version == "HTTP/3" {
            let ipv6_address_str = ipv6_address.to_string();
            let cert_path_str = cert_path.to_string();
            let key_path_str = key_path.to_string();
            let https_port_clone = https_port;

            tokio::spawn(async move {
                match start_http3_server(
                    ipv6_address_str,
                    https_port_clone,
                    &cert_path_str,
                    &key_path_str,
                )
                .await
                {
                    Ok(_) => info!("HTTP/3服务器启动成功"),
                    Err(e) => info!("启动HTTP/3服务器失败: {:?}", e),
                }
            });
        }

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

            server_handles.push(tokio::spawn(async move {
                let https_server_ipv4 = HttpServer::new(create_https_app_ipv4)
                    .workers(std::cmp::max(2, num_cpus::get()));

                let https_server_ipv4 = match http_version_clone.as_str() {
                    "HTTP/1.1" => https_server_ipv4,
                    "HTTP/2" => https_server_ipv4,
                    "HTTP/3" => {
                        info!("启动IPv4的HTTP/3服务器...");
                        https_server_ipv4
                    }
                    _ => https_server_ipv4,
                };

                let https_server_ipv4 = https_server_ipv4
                    .bind_rustls_0_23(
                        (ipv4_address_clone.as_str(), https_port_clone),
                        load_rustls_config(&cert_path_ipv4, &key_path_ipv4).unwrap(),
                    )
                    .unwrap();

                info!(
                    "HTTPS IPv4服务器运行在 https://{}:{}",
                    ipv4_address_clone, https_port_clone
                );
                info!("HTTP版本: {}", http_version_clone);
                info!("证书类型: {}", cert_type_clone);

                // 启动 HTTP/3 服务器
                if http_version_clone == "HTTP/3" {
                    tokio::spawn(async move {
                        match start_http3_server(
                            ipv4_address_clone,
                            https_port_clone,
                            &cert_path_ipv4,
                            &key_path_ipv4,
                        )
                        .await
                        {
                            Ok(_) => info!("HTTP/3 IPv4服务器启动成功"),
                            Err(e) => info!("启动HTTP/3 IPv4服务器失败: {:?}", e),
                        }
                    });
                }

                if let Err(e) = https_server_ipv4.run().await {
                    info!("HTTPS IPv4服务器失败: {:?}", e);
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

            server_handles.push(tokio::spawn(async move {
                let https_server_ipv6 = HttpServer::new(create_https_app_ipv6)
                    .workers(std::cmp::max(2, num_cpus::get()));

                let https_server_ipv6 = match http_version_clone.as_str() {
                    "HTTP/1.1" => https_server_ipv6,
                    "HTTP/2" => https_server_ipv6,
                    "HTTP/3" => {
                        info!("启动IPv6的HTTP/3服务器...");
                        https_server_ipv6
                    }
                    _ => https_server_ipv6,
                };

                let https_server_ipv6 = https_server_ipv6
                    .bind_rustls_0_23(
                        (ipv6_address_clone.as_str(), https_port_clone),
                        load_rustls_config(&cert_path_ipv6, &key_path_ipv6).unwrap(),
                    )
                    .unwrap();

                info!(
                    "HTTPS IPv6服务器运行在 https://{}:{}",
                    ipv6_address_clone, https_port_clone
                );
                info!("HTTP版本: {}", http_version_clone);
                info!("证书类型: {}", cert_type_clone);

                // 启动 HTTP/3 服务器
                if http_version_clone == "HTTP/3" {
                    tokio::spawn(async move {
                        match start_http3_server(
                            ipv6_address_clone,
                            https_port_clone,
                            &cert_path_ipv6,
                            &key_path_ipv6,
                        )
                        .await
                        {
                            Ok(_) => info!("HTTP/3 IPv6服务器启动成功"),
                            Err(e) => info!("启动HTTP/3 IPv6服务器失败: {:?}", e),
                        }
                    });
                }

                if let Err(e) = https_server_ipv6.run().await {
                    info!("HTTPS IPv6服务器失败: {:?}", e);
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
    cfg.app_data(web::JsonConfig::default()
        .limit(10 * 1024 * 1024)
        .error_handler(json_error_handler));
    // 配置表单请求体大小限制（50MB，用于文件上传）
    cfg.app_data(web::FormConfig::default().limit(50 * 1024 * 1024));
    // 配置Payload大小限制
    cfg.app_data(web::PayloadConfig::new(50 * 1024 * 1024));
    
    if let Some(pool) = pool {
        cfg.app_data(Data::new(pool.clone()));
    }

    if config.init.enabled {
        // 开启初始化时，只注册初始化相关的路由和静态文件
        cfg.service(
            web::scope("/api/init")
                .route("", web::post().to(ipma::system::init::init_system))
                .route("/db", web::post().to(ipma::system::init::init_db))
                .route(
                    "/db/clear",
                    web::delete().to(ipma::system::init::clear_database),
                )
                .route(
                    "/db/import",
                    web::post().to(ipma::system::init::import_database),
                )
                .route(
                    "/restart",
                    web::post().to(ipma::system::init::restart_program),
                )
                .route(
                    "/status",
                    web::get().to(ipma::system::init::check_init_status),
                )
                .route(
                    "/db-status",
                    web::get().to(ipma::system::init::check_db_status),
                )
                .route(
                    "/verification-code",
                    web::get().to(ipma::system::init::get_verification_code),
                )
                .route(
                    "/check-pgsql",
                    web::get().to(ipma::system::init::check_pgsql),
                ),
        )
        // 配置初始化页面路由
        .route(
            "/init_index.html",
            web::get().to(|| async {
                actix_web::HttpResponse::Ok()
                    .content_type("text/html")
                    .body(
                        std::fs::read_to_string("web/static/init_index.html").unwrap_or_else(
                            |e| format!("Error reading init_index.html: {}", e),
                        ),
                    )
            }),
        )
        // 配置静态文件服务
        .service(
            Files::new("/static", "web/static")
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
        )
        // 健康检查路由
        .service(web::scope("/health").route(
            "",
            web::get().to(|| async {
                actix_web::HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
            }),
        ));
    } else if enable_normal_routes {
        // 关闭初始化时，注册完整的应用系统路由
        cfg.service(
            Files::new("/static", "web/static")
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
                actix_web::HttpResponse::Ok()
                    .content_type("text/html")
                    .body(
                        std::fs::read_to_string("web/static/main.html")
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
