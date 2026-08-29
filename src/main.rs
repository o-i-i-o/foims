//! 程序入口：配置加载、服务启动与优雅退出。

use std::fs;
use std::panic;
use std::path::Path;
use std::sync::Arc;
use tracing::error;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Request};
use axum::http::{HeaderValue, Method, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use ipma::app_state::AppState;
use ipma::routes::get_init_status;
use ipma::routes::init_routes;
use ipma::routes::static_files::get_web_dir;
use ipma::shutdown::{ShutdownSignal, wait_for_shutdown_signal};
use ipma::system::config::init_start_time;
use ipma::system::task_executors::{
    BackupTaskExecutor, LogCleanupTaskExecutor, MacSyncTaskExecutor, TokenCleanupTaskExecutor,
    TokenUsageCleanupTaskExecutor,
};
use ipma::utils::rate_limit::{
    RateLimitState, RateLimiter, rate_limit_middleware, start_cleanup_task,
};
use ipma_common::config::Config;
use ipma_common::db::DbPool;
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
    }));
}

async fn static_cache_control_middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let has_version = req.uri().query().is_some_and(|q| q.contains("v="));
    let mut res = next.run(req).await;

    if path.starts_with("/static/") {
        // 带 ?v= 的资源由版本号机制保证内容变化即换 URL，可长缓存；
        // 其余资源（含无版本号的 JS 模块动态 import）维持每次再验证
        let cache_policy = if has_version {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache, must-revalidate"
        };
        res.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static(cache_policy),
        );
        // 为 .json 文件补充 charset=utf-8（ServeDir 默认只设 application/json）
        if path.ends_with(".json")
            && res
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|ct| ct.starts_with("application/json") && !ct.contains("charset"))
        {
            res.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            );
        }
    }

    res
}

async fn security_headers_middleware(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;

    // frame-ancestors 只能通过 HTTP 头设置，不能通过 <meta> 元素传递
    res.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("frame-ancestors 'none'"),
    );
    res.headers_mut()
        .insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    // 禁止浏览器对响应体做 MIME 嗅探（配合各端点正确的 Content-Type）
    res.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    res.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );

    res
}

fn build_cors_layer(config: &Config) -> CorsLayer {
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

    let allowed_origins = Arc::new(allowed_origins);

    let allow_origin = AllowOrigin::predicate(
        move |origin: &HeaderValue, _parts: &axum::http::request::Parts| {
            let Some(origin_str) = origin.to_str().ok() else {
                return false;
            };
            // 仅精确匹配（含可选的显式 :80/:443 端口写法）。
            // 此前用 starts_with 前缀匹配，http://localhost.evil.com:80 这类
            // 后缀域名可绕过且 allow_credentials(true)（见 security-review A-10）
            for allowed in allowed_origins.iter() {
                let base = allowed.trim_end_matches('/');
                if origin_str == allowed || origin_str == base {
                    return true;
                }
                // 允许显式补写默认端口（http→:80 / https→:443）
                let with_port = if base.starts_with("https://") {
                    format!("{base}:443")
                } else {
                    format!("{base}:80")
                };
                if origin_str == with_port {
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
        },
    );

    CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
            header::ACCEPT_LANGUAGE,
            header::ORIGIN,
        ])
        .allow_credentials(true)
        .max_age(std::time::Duration::from_secs(3600))
        .allow_origin(allow_origin)
}

/// 创建 UDS 监听器
///
/// - 自动创建父目录
/// - 清理已存在的 socket 文件，避免 "Address already in use"
/// - 设置 socket 文件权限 0660 并将属组设为反代进程组（默认 www-data）：
///   仅允许属主（服务账户）与反代访问。此前为 0666，本机任意进程均可
///   直连并伪造 X-Real-IP 头，绕过初始化接口的 localhost 限制与限流/
///   fail2ban（见 security-review I-1/A-1）。非 root 运行无法改属组时
///   保持 0660 仅属主可用（fail-closed），记录告警由运维调整属组。
fn create_uds_listener(path: &str, group: &str) -> std::io::Result<tokio::net::UnixListener> {
    let socket_path = Path::new(path);

    // 确保父目录存在
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // 清理已存在的 socket 文件
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    let listener = tokio::net::UnixListener::bind(path)?;
    use std::os::unix::fs::PermissionsExt;
    // 尝试将属组设为反代进程组（需 root）；失败不阻断启动（保持仅属主可用）
    #[cfg(unix)]
    {
        match lookup_group_gid(group) {
            Ok(Some(gid)) => {
                if let Err(e) = std::os::unix::fs::chown(path, None, Some(gid)) {
                    tracing::warn!(
                        "设置 UDS socket 属组为 {group} 失败（非 root 运行？），保持仅属主可访问: {e}"
                    );
                }
            }
            Ok(None) => {
                tracing::warn!("用户组 {group} 不存在，UDS socket 保持仅属主可访问");
            }
            Err(e) => {
                tracing::warn!("解析用户组 {group} 失败，UDS socket 保持仅属主可访问: {e}");
            }
        }
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))?;

    Ok(listener)
}

/// 从 /etc/group 解析组名对应的 GID（避免引入额外 crate）。
/// /etc/group 字段序为「组名:口令:GID:成员」，GID 是第 3 列。
/// 返回 Ok(None) 表示组不存在。
fn lookup_group_gid(group: &str) -> std::io::Result<Option<u32>> {
    let content = std::fs::read_to_string("/etc/group")?;
    for line in content.lines() {
        let mut fields = line.split(':');
        if fields.next() == Some(group)
            && let Some(gid) = fields.nth(1) // 跳过口令列，取 GID 列
            && let Ok(gid) = gid.parse::<u32>()
        {
            return Ok(Some(gid));
        }
    }
    Ok(None)
}

async fn redirect_to_index() -> Response {
    Redirect::to("/static/index.html").into_response()
}

async fn redirect_to_init_index() -> Response {
    Redirect::to("/init_index.html").into_response()
}

fn configure_app_services(
    app_state: Arc<AppState>,
    serve_static: bool,
    init_enabled: bool,
    rate_limit_state: RateLimitState,
) -> Router {
    let cors_layer = build_cors_layer(&app_state.config);

    let router = if init_enabled {
        // 创建 InitContext 用于初始化模块
        let init_context = Arc::new(InitContext::new(
            InitDatabaseConfig {
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
                health_check_interval_secs: app_state.config.database.health_check_interval_secs,
            },
            ipma_common::config::get_config_file_path(),
            app_state.config.init.enabled,
            Arc::new(|| {
                Box::pin(async move {
                    ipma::system::config::trigger_service_restart()
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(())
                })
            }),
        ));

        let init_router = Router::new()
            .route("/api/init", post(ipma_init::init_system))
            .route("/api/init/db", post(ipma_init::init_db))
            .route("/api/init/db/clear", post(ipma_init::clear_database))
            .route("/api/init/db/create", post(ipma_init::create_database_api))
            .route("/api/init/db/import", post(ipma_init::import_database_api))
            .route(
                "/api/init/db/import-file",
                post(ipma_init::import_database_from_file)
                    .layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
            )
            .route("/api/init/restart", post(ipma_init::restart_program))
            .route("/api/init/status", get(ipma_init::check_init_status))
            .route("/api/init/db-status", get(ipma_init::check_db_status))
            .route(
                "/api/init/verification-code",
                get(ipma_init::get_verification_code),
            )
            .route("/api/init/check-pgsql", get(ipma_init::check_pgsql))
            .route_layer(middleware::from_fn(
                ipma::auth::login::localhost_only_middleware,
            ))
            .with_state(init_context);

        let router = Router::new().merge(init_router);

        // 仅在 serve_static=true 时托管前端文件
        // 生产模式（UDS + nginx）应禁用，由 nginx 直接服务静态资源
        if serve_static {
            router
                .route_service(
                    "/init_index.html",
                    ServeFile::new(format!("{}/static/init_index.html", get_web_dir())),
                )
                .nest_service(
                    "/static",
                    ServeDir::new(format!("{}/static", get_web_dir())),
                )
                .route("/", get(redirect_to_init_index))
        } else {
            router
        }
    } else {
        // API 路由
        let api_router = init_routes(app_state.clone()).with_state(app_state.clone());

        let router = Router::new().merge(api_router);

        if serve_static {
            // 开发模式：axum 直接托管静态文件 + main.html
            router
                .nest_service(
                    "/static",
                    ServeDir::new(format!("{}/static", get_web_dir())),
                )
                .route_service(
                    "/main.html",
                    ServeFile::new(format!("{}/static/main.html", get_web_dir())),
                )
                .route("/", get(redirect_to_index))
        } else {
            // 生产模式：仅注册 API 路由，静态文件由 nginx 托管
            router
        }
    };

    // 始终注册的公开路由（初始化模式与正常模式都可用）
    // 让前端登录页能查询初始化开关，决定是否跳转到 /init_index.html
    let always_on_routes = Router::new()
        .route("/api/auth/init-status", get(get_init_status))
        .with_state(app_state.clone());

    router
        .merge(always_on_routes)
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer)
        .layer(middleware::from_fn_with_state(
            rate_limit_state,
            rate_limit_middleware,
        ))
        .layer(middleware::from_fn(static_cache_control_middleware))
        .layer(middleware::from_fn(security_headers_middleware))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    setup_panic_handler();

    // 先加载配置：日志系统的语言（log_language / logfiles_i18n_out）依赖配置。
    // 配置加载失败时尚无订阅器，先以默认英文初始化日志系统，记录错误后退出
    let config = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            let _ = ipma::log::setup_logging(None);
            ipma_common::log_error!("system.config_load_failed", error = e);
            std::process::exit(1);
        }
    };

    let log_files = ipma::log::setup_logging(config.i18n.as_ref());

    ipma_common::log_info!("log.output_to", path = log_files.join(", "));
    ipma_common::log_info!("system.start");

    ipma_common::log_info!("system.config_loaded");

    if let Err(e) = ipma_common::crypto::check_key_integrity() {
        ipma_common::log_error!("system.key_integrity_check_failed", error = e);
    }

    let shutdown = ShutdownSignal::new();

    let pool = if config.init.enabled {
        ipma_common::log_info!("system.init_mode_enabled");
        None
    } else {
        match DbPool::new(&config.database).await {
            // 池创建成功的日志（含全部参数）由 DbPool::new_with_config 统一记录
            Ok(p) => Some(p),
            Err(e) => {
                ipma_common::log_error!("system.db_pool_create_failed", error = e);
                std::process::exit(1);
            }
        }
    };

    init_start_time();
    ipma_common::log_info!("system.start_time_initialized");

    // 启动应用层 fail2ban 清理任务
    ipma::system::app_fail2ban::start_cleanup_task();
    ipma_common::log_info!("system.fail2ban_cleanup_started");

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
        let scheduler_db_config = ipma_data_management::DatabaseConfig {
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
                    ipma_common::log_error!("system.register_backup_job_failed", error = e);
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
                    ipma_common::log_error!("system.register_token_cleanup_job_failed", error = e);
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
                    ipma_common::log_error!("system.register_usage_cleanup_job_failed", error = e);
                }

                match state.start().await {
                    Ok(running) => {
                        running_scheduler = Some(running);
                        ipma_common::log_info!("system.scheduler_started");
                    }
                    Err(e) => {
                        ipma_common::log_error!("system.scheduler_start_failed", error = e);
                    }
                }
            }
            Err(e) => {
                ipma_common::log_error!("system.scheduler_create_failed", error = e);
            }
        }

        let health_interval = config.database.health_check_interval_secs.max(1) as u64;
        db_pool.start_health_check_task(health_interval, shutdown.subscribe());
        ipma_common::log_info!("system.db_health_check_started", interval = health_interval);
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
        ipma_common::log_info!("system.rate_limit_enabled");
        ipma_common::log_info!(
            "system.rate_limit_ip",
            limit = config.rate_limit.ip_limit,
            window = config.rate_limit.window_secs
        );
        ipma_common::log_info!(
            "system.rate_limit_user",
            limit = config.rate_limit.user_limit,
            window = config.rate_limit.window_secs
        );
        ipma_common::log_info!(
            "system.rate_limit_login",
            limit = config.rate_limit.login_limit,
            window = config.rate_limit.window_secs
        );
        ipma_common::log_info!(
            "system.rate_limit_email",
            limit = config.rate_limit.email_limit,
            window = config.rate_limit.email_window_secs
        );
    }

    let uds_path = config.server.listen.uds_path.clone();
    let serve_static = config.server.listen.serve_static;

    let web_dir = get_web_dir();
    if serve_static && !Path::new(web_dir).exists() {
        ipma_common::log_info!("system.web_dir_created", path = web_dir);
        fs::create_dir_all(web_dir)?;
    }

    let init_enabled = config.init.enabled;

    if init_enabled {
        ipma_common::log_info!("system.init_mode_enabled");
    } else {
        ipma_common::log_info!("system.init_mode_disabled");
    }

    ipma_common::log_info!("system.listening_uds", path = uds_path);
    ipma_common::log_info!(
        "system.static_serve_mode",
        mode = if serve_static { "axum" } else { "nginx" }
    );

    let app_state = Arc::new(
        AppState::new(config.clone(), pool.clone(), task_registry.clone())
            .map_err(std::io::Error::other)?,
    );

    app_state
        .jwt_utils
        .start_cache_cleanup_task(shutdown.subscribe());

    let rate_limit_state = RateLimitState::new(rate_limiter.clone(), rate_limit_enabled);

    // 单实例保护：若已有进程在监听该 UDS，则拒绝启动，避免 remove_file 偷删
    // 正在服务的 socket 文件后出现「两个进程、孤儿 listener」的隐患。
    if Path::new(&uds_path).exists() {
        match tokio::net::UnixStream::connect(&uds_path).await {
            Ok(_) => {
                ipma_common::log_error!("system.uds_in_use", path = uds_path);
                std::process::exit(1);
            }
            Err(_) => {
                // 文件存在但无人监听（上次进程异常退出残留）→ 安全清理
                ipma_common::log_info!("system.uds_stale_cleaned", path = uds_path);
            }
        }
    }

    // 创建 UDS 监听器（0660 + 反代属组，见 create_uds_listener 注释）
    let uds_listener = create_uds_listener(&uds_path, &app_state.config.server.listen.uds_group)?;

    // 构建应用
    let app = configure_app_services(
        app_state.clone(),
        serve_static,
        init_enabled,
        rate_limit_state,
    );

    ipma_common::log_info!("system.uds_server_started", path = uds_path);
    ipma_common::log_info!("system.ready");

    // 启动服务器（使用 oneshot 通道在收到第一次信号时通知主任务）
    let (signal_tx, signal_rx) = tokio::sync::oneshot::channel::<()>();
    let shutdown_for_signal = shutdown.clone();
    let graceful_shutdown = async move {
        wait_for_shutdown_signal(&shutdown_for_signal).await;
        let _ = signal_tx.send(());
    };

    // axum 0.8 启用 http2 feature 后，serve() 内部使用 auto::Builder 自动检测 HTTP/1 和 h2c
    let serve = axum::serve(uds_listener, app);
    let server_task = tokio::spawn(async move {
        if let Err(e) = serve.with_graceful_shutdown(graceful_shutdown).await {
            ipma_common::log_error!("system.server_run_error", error = e);
        }
    });

    // 等待第一次关闭信号（通过 oneshot 通道）
    let _ = signal_rx.await;

    // 注册强制退出信号处理（第二次 Ctrl-C）
    let force_shutdown_handle = tokio::spawn(async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            ipma_common::log_warn!("system.force_exit_register_failed", error = e);
        }
        ipma_common::log_warn!("system.force_exit");
        std::process::exit(1);
    });

    // 1. 停止接收新连接并等待请求完成（graceful_shutdown 已触发，等待服务器结束）
    ipma_common::log_info!("system.shutdown_step_connections");
    let server_stop_timeout = tokio::time::Duration::from_secs(10);
    if let Err(e) = tokio::time::timeout(server_stop_timeout, server_task).await {
        ipma_common::log_warn!("system.graceful_shutdown_timeout", error = e);
    }
    ipma_common::log_info!("system.shutdown_connections_closed");

    // 2. 服务器任务已结束
    ipma_common::log_info!("system.shutdown_step_server");
    ipma_common::log_info!("system.shutdown_server_done");

    // 3. 关闭后台任务
    ipma_common::log_info!("system.shutdown_step_background");
    shutdown.request_shutdown();
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    ipma_common::log_info!("system.shutdown_background_signaled");

    // 4. 关闭调度器
    ipma_common::log_info!("system.shutdown_step_scheduler");
    if let Some(scheduler) = running_scheduler
        && let Err(e) =
            tokio::time::timeout(tokio::time::Duration::from_secs(5), scheduler.shutdown()).await
    {
        ipma_common::log_warn!("system.scheduler_shutdown_timeout", error = e);
    }

    // 5. 关闭数据库连接池
    ipma_common::log_info!("system.shutdown_step_db");
    if let Some(db_pool) = pool
        && let Err(e) =
            tokio::time::timeout(tokio::time::Duration::from_secs(5), db_pool.close()).await
    {
        ipma_common::log_warn!("system.db_pool_close_timeout", error = e);
    }

    force_shutdown_handle.abort();

    ipma_common::log_info!("system.shutdown_complete");

    Ok(())
}
