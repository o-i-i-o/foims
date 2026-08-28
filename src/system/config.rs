//! 系统配置接口（SMTP/通知/超时/备份等）。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tokio::process::Command;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::config::{Config, I18nConfig, ServerConfig};
use crate::error::AppError;
use crate::routes::static_files::AppJson;
use crate::system::smtp::{
    SmtpConfig, get_smtp_config_from_db, save_smtp_config_to_db, send_email_to_users,
};
use ipma_common::{log_error, log_info, log_warn, msg};

static START_TIME: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateSystemConfigRequest {
    pub database: Option<crate::config::DatabaseConfig>,
    pub server: Option<ServerConfig>,
    pub jwt: Option<crate::config::JwtConfig>,
    pub init: Option<crate::config::InitConfig>,
    pub rate_limit: Option<crate::config::RateLimitConfig>,
    pub snmp: Option<crate::config::SnmpConfig>,
}

pub fn record_start_time() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    START_TIME.store(now, Ordering::SeqCst);
}

pub fn init_start_time() {
    record_start_time();
}

async fn save_config_to_file(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = crate::config::get_config_file_path();
    log_info!("log.config.save_start", path = config_path);

    let toml_str = toml::to_string_pretty(config)?;
    log_info!("log.config.toml_length", count = toml_str.len());

    tokio::fs::write(&config_path, &toml_str).await?;
    log_info!("log.config.file_written");

    let verify_content = tokio::fs::read_to_string(&config_path).await?;
    log_info!("log.config.verify_read", count = verify_content.len());

    Ok(())
}

pub async fn get_system_info(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    let database_status = match sqlx::query("SELECT 1")
        .execute(&state.pool()?.get_conn())
        .await
    {
        Ok(_) => "connected".to_string(),
        Err(e) => {
            log_error!("log.system.db_check_failed", error = e);
            format!("disconnected: {}", e)
        }
    };

    let uptime = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        - START_TIME.load(Ordering::SeqCst);

    let pool_metrics = state.pool()?.get_metrics();
    let system_time = chrono::Utc::now();

    let system_info = serde_json::json!({
        "name": "IPMA",
        "version": env!("CARGO_PKG_VERSION"),
        "database_status": database_status,
        "uptime_seconds": uptime,
        "timestamp": system_time.to_rfc3339(),
        "pool_metrics": {
            "active_connections": pool_metrics.active_connections,
            "idle_connections": pool_metrics.idle_connections,
            "waiting_requests": pool_metrics.waiting_requests,
        }
    });

    Ok(crate::error::ok_json(
        system_info,
        "server.system.info_retrieved",
    ))
}

pub async fn get_system_config(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    let mut config = state.config.clone();
    config.database.password = "***".to_string();
    config.jwt.secret = "***".to_string();
    Ok(crate::error::ok_json(
        config,
        "server.system.config_retrieved",
    ))
}

pub async fn update_system_config(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
    AppJson(req): AppJson<UpdateSystemConfigRequest>,
) -> Result<Response, AppError> {
    let mut new_config = state.config.clone();

    // req 各分支互斥且按值取出，避免逐子结构克隆
    if let Some(database) = req.database {
        // 防止脱敏值覆写真实密码
        let mut db_config = database;
        if db_config.password == "***" {
            db_config.password = state.config.database.password.clone();
        }
        new_config.database = db_config;
    }

    if let Some(server) = req.server {
        new_config.server = server;
    }

    if let Some(jwt) = req.jwt {
        // 防止脱敏值覆写真实密钥
        let mut jwt_config = jwt;
        if jwt_config.secret == "***" {
            jwt_config.secret = state.config.jwt.secret.clone();
        }
        new_config.jwt = jwt_config;
    }

    if let Some(init) = req.init {
        if init.enabled != state.config.init.enabled {
            return Err(AppError::Validation(msg(
                "server.system.init_mode_api_forbidden",
            )));
        }
        new_config.init = init;
    }

    if let Some(rate_limit) = req.rate_limit {
        new_config.rate_limit = rate_limit;
    }

    if let Some(snmp) = req.snmp {
        new_config.snmp = snmp;
    }

    let config_path = crate::config::get_config_file_path();
    log_info!("log.config.save_start", path = config_path);

    save_config_to_file(&new_config).await.map_err(|e| {
        AppError::Internal(msg("server.system.config_save_failed").with("error", e))
    })?;
    log_info!("log.config.saved", path = config_path);

    // 响应中脱敏数据库密码与 JWT 密钥（与 get_system_config 一致）：
    // 回传明文 JWT secret 等同于允许接收方伪造任意管理员令牌（A-3）
    let mut masked = new_config;
    masked.database.password = "***".to_string();
    masked.jwt.secret = "***".to_string();

    Ok(crate::error::ok_json(
        masked,
        "server.system.config_updated",
    ))
}

pub async fn trigger_service_restart() -> Result<Response, AppError> {
    let service_name = "ipma.service";

    let is_running_as_service = tokio::task::spawn_blocking(check_if_running_as_service)
        .await
        .unwrap_or(false);

    log_info!(
        "log.system.restart_triggered",
        as_service = is_running_as_service
    );

    if is_running_as_service {
        let check_output = Command::new("systemctl")
            .args(["show", "ipma.service", "--property=ActiveState"])
            .output()
            .await;

        let is_active = match check_output {
            Ok(output) => {
                let status = String::from_utf8_lossy(&output.stdout);
                status.contains("ActiveState=active")
            }
            Err(_) => true,
        };

        log_info!("log.system.service_state", active = is_active);

        // 在后台延迟执行 systemctl restart：若直接 await，成功重启会杀死本进程导致响应不可达。
        // 先返回响应，由后台任务触发重启；若 systemctl 因权限等原因未能终止进程，则回退到进程退出方式。
        let service_name_owned = service_name.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            log_info!("log.system.systemctl_restart", service = service_name_owned);
            let _ = Command::new("systemctl")
                .arg("restart")
                .arg(&service_name_owned)
                .output()
                .await;
            // 给 systemctl 一点时间终止本进程；若仍存活则主动退出（systemd Restart=always 会拉起）
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            log_info!("log.system.systemctl_exit_fallback");
            std::process::exit(0);
        });

        Ok(crate::error::ok_json(
            (),
            "server.system.restart_command_sent",
        ))
    } else {
        log_info!("log.system.standalone_restart");
        restart_standalone_process().await
    }
}

pub async fn restart_application(
    _admin: crate::auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    log_info!("log.system.restart_requested");
    trigger_service_restart().await
}

fn check_if_running_as_service() -> bool {
    if std::env::var("INVOCATION_ID").is_ok() {
        return true;
    }

    if let Ok(pid) = std::fs::read_to_string("/proc/self/cgroup")
        && (pid.contains("systemd") || pid.contains(".service"))
    {
        return true;
    }

    std::path::Path::new("/etc/systemd/system/ipma.service").exists()
}

async fn restart_standalone_process() -> Result<Response, AppError> {
    let exe_path = std::env::current_exe()
        .map_err(|e| AppError::Internal(msg("server.system.exe_path_failed").with("error", e)))?;

    let exe_path_str = exe_path
        .to_str()
        .ok_or_else(|| AppError::Internal(msg("server.system.exe_path_invalid")))?;

    let working_dir = std::env::current_dir()
        .map_err(|e| AppError::Internal(msg("server.system.workdir_failed").with("error", e)))?;

    let working_dir_str = working_dir
        .to_str()
        .ok_or_else(|| AppError::Internal(msg("server.system.workdir_invalid")))?;

    let restart_script = r#"#!/bin/bash
sleep 3
cd "$1"
exec "$2"
"#;

    // 随机文件名 + create_new 原子创建（0700）：避免固定路径被本地低权用户
    // 预置符号链接劫持为任意文件写入/执行（security-review I-5）
    let script_path = format!("/tmp/ipma_restart_{}.sh", Uuid::new_v4());
    {
        // tokio::fs::OpenOptions 在 Unix 上原生提供 mode()
        let mut opts = tokio::fs::OpenOptions::new();
        opts.mode(0o700).write(true).create_new(true);
        let mut file = opts.open(&script_path).await.map_err(|e| {
            AppError::Internal(msg("server.system.restart_script_create_failed").with("error", e))
        })?;
        use tokio::io::AsyncWriteExt;
        file.write_all(restart_script.as_bytes())
            .await
            .map_err(|e| {
                AppError::Internal(
                    msg("server.system.restart_script_create_failed").with("error", e),
                )
            })?;
    }

    let script_path_owned = script_path.clone();
    // 脚本已 0700 可执行，直接 spawn；退出前清理
    if let Err(e) = Command::new("nohup")
        .arg(&script_path)
        .arg(working_dir_str)
        .arg(exe_path_str)
        .spawn()
    {
        log_warn!("log.system.restart_script_spawn_failed", error = e);
    }

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let _ = tokio::fs::remove_file(&script_path_owned).await;
        std::process::exit(0);
    });

    Ok(crate::error::ok_json(
        (),
        "server.system.restart_command_sent",
    ))
}

pub async fn disable_init_mode(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    log_info!("log.system.disable_init_requested");

    let mut new_config = state.config.clone();
    new_config.init.enabled = false;

    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&new_config).map_err(|e| {
        AppError::Internal(msg("server.system.config_serialize_failed").with("error", e))
    })?;

    tokio::fs::write(&config_path, config_str)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_write_failed").with("error", e))
        })?;

    log_info!("log.system.init_disabled_restarting");

    trigger_service_restart().await
}

pub async fn backup_config(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    let mut config = state.config.clone();
    config.database.password = "***".to_string();
    config.jwt.secret = "***".to_string();
    let config_json = serde_json::to_string_pretty(&config).map_err(|e| {
        AppError::Internal(msg("server.system.config_serialize_failed").with("error", e))
    })?;

    Ok((
        StatusCode::OK,
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/json".to_string(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=ipma_config_backup_{}.json",
                    chrono::Utc::now().format("%Y%m%d_%H%M%S")
                ),
            ),
        ],
        config_json,
    )
        .into_response())
}

pub async fn restore_config(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
    AppJson(payload): AppJson<Config>,
) -> Result<Response, AppError> {
    let mut new_config = payload;

    if new_config.init.enabled {
        return Err(AppError::Validation(msg(
            "server.system.init_mode_restore_forbidden",
        )));
    }

    // 防止脱敏值覆写真实密钥
    if new_config.database.password == "***" {
        new_config.database.password = state.config.database.password.clone();
    }
    if new_config.jwt.secret == "***" {
        new_config.jwt.secret = state.config.jwt.secret.clone();
    }

    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&new_config).map_err(|e| {
        AppError::Internal(msg("server.system.config_serialize_failed").with("error", e))
    })?;

    tokio::fs::write(&config_path, config_str)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_write_failed").with("error", e))
        })?;

    Ok(crate::error::ok_json((), "server.system.config_restored"))
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateLanguageRequest {
    #[validate(length(min = 2, max = 5, message = "server.system.validation.language_length"))]
    pub language: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateSessionTimeoutRequest {
    pub session_timeout: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdatePageTimeoutRequest {
    pub page_timeout: Option<u64>,
}

pub async fn get_session_timeout_config(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    Ok(crate::error::ok_json(
        serde_json::json!({
            "session_timeout": state.config.server.session_timeout
        }),
        "server.system.session_timeout_retrieved",
    ))
}

pub async fn update_session_timeout_config(
    State(_state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
    AppJson(req): AppJson<UpdateSessionTimeoutRequest>,
) -> Result<Response, AppError> {
    let mut current_config = tokio::task::spawn_blocking(Config::load)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_failed").with("error", e))
        })?;

    current_config.server.session_timeout = req.session_timeout;

    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&current_config).map_err(|e| {
        AppError::Internal(msg("server.system.config_serialize_failed").with("error", e))
    })?;

    tokio::fs::write(&config_path, config_str)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_write_failed").with("error", e))
        })?;

    Ok(crate::error::ok_json(
        (),
        "server.system.session_timeout_updated",
    ))
}

pub async fn get_supported_languages() -> Result<Response, AppError> {
    let supported_languages = vec![
        serde_json::json!({
            "code": "en",
            "name": "English",
            "native_name": "English"
        }),
        serde_json::json!({
            "code": "zh",
            "name": "Chinese",
            "native_name": "中文"
        }),
    ];

    Ok(crate::error::ok_json(
        supported_languages,
        "server.system.languages_retrieved",
    ))
}

pub async fn update_language_setting(
    State(_state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
    AppJson(req): AppJson<UpdateLanguageRequest>,
) -> Result<Response, AppError> {
    req.validate()?;

    let language = req.language.to_lowercase();
    if language != "en" && language != "zh" {
        return Err(AppError::Validation(msg(
            "server.system.language_unsupported",
        )));
    }

    let mut current_config = tokio::task::spawn_blocking(Config::load)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_failed").with("error", e))
        })?;

    // 该端点设置的是日志语言：在已加载配置基础上仅更新 log_language 字段，
    // 保留 supported_languages / logfiles_i18n_out 等其余 i18n 设置；
    // 配置中尚无 [i18n] 段时新建并带默认支持语言列表。
    let mut i18n = current_config.i18n.clone().unwrap_or_else(|| I18nConfig {
        log_language: String::new(),
        supported_languages: vec!["zh".to_string(), "en".to_string()],
        logfiles_i18n_out: None,
    });
    i18n.log_language = language;
    current_config.i18n = Some(i18n);

    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&current_config).map_err(|e| {
        AppError::Internal(msg("server.system.config_serialize_failed").with("error", e))
    })?;

    tokio::fs::write(&config_path, config_str)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_write_failed").with("error", e))
        })?;

    Ok(crate::error::ok_json((), "server.system.language_updated"))
}

pub async fn get_page_timeout_config(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    Ok(crate::error::ok_json(
        serde_json::json!({
            "page_timeout": state.config.server.page_timeout
        }),
        "server.system.page_timeout_retrieved",
    ))
}

pub async fn update_page_timeout_config(
    State(_state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
    AppJson(req): AppJson<UpdatePageTimeoutRequest>,
) -> Result<Response, AppError> {
    let mut current_config = tokio::task::spawn_blocking(Config::load)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_failed").with("error", e))
        })?;

    current_config.server.page_timeout = req.page_timeout;

    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&current_config).map_err(|e| {
        AppError::Internal(msg("server.system.config_serialize_failed").with("error", e))
    })?;

    tokio::fs::write(&config_path, config_str)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_write_failed").with("error", e))
        })?;

    Ok(crate::error::ok_json(
        (),
        "server.system.page_timeout_updated",
    ))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub email_recipients: Vec<Uuid>,
}

pub async fn get_notification_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    let recipients = match sqlx::query_scalar::<_, String>(
        "SELECT value FROM system_configs WHERE config_type = 'notification' AND key = 'email_recipients'",
    )
    .fetch_optional(&state.pool()?.get_conn())
    .await
    {
        Ok(Some(value)) => serde_json::from_str(&value).map_err(|e| {
            // 存量数据损坏时不能静默清空收件人，否则 MAC 变更通知会失效
            ipma_common::log_error!("log.system.recipients_parse_failed", error = e);
            AppError::Internal(
                msg("server.notification.recipients_parse_failed").with("error", e),
            )
        })?,
        _ => Vec::new(),
    };

    Ok(crate::error::ok_json(
        NotificationSettings {
            email_recipients: recipients,
        },
        "server.notification.settings_retrieved",
    ))
}

pub async fn update_notification_settings(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
    AppJson(req): AppJson<NotificationSettings>,
) -> Result<Response, AppError> {
    let value = serde_json::to_string(&req.email_recipients)
        .map_err(|e| AppError::Internal(msg("server.system.serialize_failed").with("error", e)))?;

    sqlx::query(
        "INSERT INTO system_configs (config_type, key, value) VALUES ('notification', 'email_recipients', $1)
         ON CONFLICT (config_type, key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(&value)
    .execute(&state.pool()?.get_conn())
    .await
    .map_err(|e| AppError::Database(msg("server.db.operation_failed").with("error", e)))?;

    Ok(crate::error::ok_json(
        (),
        "server.notification.settings_updated",
    ))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SmtpConfigResponse {
    /// 业务状态：未配置是正常状态（HTTP 200），而非错误
    pub configured: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub from: String,
    pub secure: bool,
    pub has_password: bool,
}

pub async fn get_smtp_config(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    // 未配置属于业务状态而非错误：返回 200 + configured=false，避免浏览器控制台出现 404
    let resp = match get_smtp_config_from_db(&state.pool()?.get_conn()).await {
        Some(config) => SmtpConfigResponse {
            configured: true,
            host: config.host,
            port: config.port,
            username: config.username,
            from: config.from,
            secure: config.secure,
            has_password: !config.password.is_empty(),
        },
        None => SmtpConfigResponse {
            configured: false,
            host: String::new(),
            port: 0,
            username: String::new(),
            from: String::new(),
            secure: false,
            has_password: false,
        },
    };

    Ok(crate::error::ok_json(resp, "server.smtp.config_retrieved"))
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateSmtpConfigRequest {
    #[validate(length(min = 1, max = 255, message = "server.smtp.validation.host_length"))]
    pub host: String,
    pub port: u16,
    #[validate(length(min = 1, max = 100, message = "server.smtp.validation.username_length"))]
    pub username: String,
    #[validate(length(max = 200, message = "server.smtp.validation.password_length"))]
    pub password: String,
    #[validate(email(message = "server.common.validation.email_format"))]
    pub from: String,
    pub secure: bool,
}

pub async fn update_smtp_config(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
    AppJson(req): AppJson<UpdateSmtpConfigRequest>,
) -> Result<Response, AppError> {
    req.validate()?;
    if req.port == 0 {
        return Err(AppError::Validation(msg("server.smtp.port_zero")));
    }
    let password = if req.password.is_empty() {
        let existing = get_smtp_config_from_db(&state.pool()?.get_conn())
            .await
            .ok_or_else(|| {
                log_warn!("log.smtp.not_configured_password");
                AppError::Validation(msg("server.smtp.not_configured_password"))
            })?;
        existing.password
    } else {
        req.password.clone()
    };

    let config = SmtpConfig {
        host: req.host.clone(),
        port: req.port,
        username: req.username.clone(),
        password,
        from: req.from.clone(),
        secure: req.secure,
    };

    save_smtp_config_to_db(&state.pool()?.get_conn(), &config).await?;

    Ok(crate::error::ok_json((), "server.smtp.config_updated"))
}

/// 测试已保存的通知邮件（SMTP）配置连通性。无需请求体。
pub async fn test_smtp_connection(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    let config = match get_smtp_config_from_db(&state.pool()?.get_conn()).await {
        Some(c) => c,
        None => {
            log_warn!("log.smtp.not_configured");
            return Err(AppError::Validation(msg("server.smtp.not_configured")));
        }
    };

    crate::system::smtp::test_smtp_connection(&config).await?;

    Ok(crate::error::ok_json((), "server.smtp.test_success"))
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SendSystemEmailRequest {
    #[validate(length(min = 1, message = "server.smtp.validation.recipients_required"))]
    pub user_ids: Vec<Uuid>,
    #[validate(length(min = 1, max = 255, message = "server.smtp.validation.subject_length"))]
    pub subject: String,
    #[validate(length(min = 1, message = "server.smtp.validation.body_required"))]
    pub body: String,
}

pub async fn send_system_email(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::extractor::AdminUser,
    AppJson(req): AppJson<SendSystemEmailRequest>,
) -> Result<Response, AppError> {
    req.validate()?;

    send_email_to_users(
        &state.pool()?.get_conn(),
        &req.user_ids,
        &req.subject,
        &req.body,
    )
    .await?;

    Ok(crate::error::ok_json((), "server.smtp.email_sent"))
}

// ==================== 等保密码策略配置 ====================

/// 读取密码策略（未配置时返回等保三级默认值）
pub async fn get_password_policy(
    State(state): State<Arc<AppState>>,
    _secadmin: crate::auth::extractor::SecAdminUser,
) -> Result<Response, AppError> {
    let policy = crate::auth::password_policy::load(&state.pool()?.get_conn()).await;
    Ok(crate::error::ok_json(
        policy,
        "server.system.config_retrieved",
    ))
}

/// 保存密码策略（长度下限 8、各数值范围由模块内钳制）
pub async fn update_password_policy(
    State(state): State<Arc<AppState>>,
    _secadmin: crate::auth::extractor::SecAdminUser,
    AppJson(req): AppJson<crate::auth::password_policy::PasswordPolicy>,
) -> Result<Response, AppError> {
    crate::auth::password_policy::save(&state.pool()?.get_conn(), &req).await?;
    Ok(crate::error::ok_json(
        (),
        "server.system.password_policy_updated",
    ))
}

#[derive(Debug, Serialize)]
pub struct ServiceStatus {
    pub registered: bool,
    pub running_as_service: bool,
    pub service_file_exists: bool,
    pub active: bool,
    pub status: Option<String>,
    pub enabled: bool,
    pub uptime_seconds: Option<u64>,
}

pub async fn get_service_status() -> Result<Response, AppError> {
    let running_as_service = tokio::task::spawn_blocking(check_if_running_as_service)
        .await
        .unwrap_or(false);
    let service_file_exists = tokio::fs::try_exists("/etc/systemd/system/ipma.service")
        .await
        .unwrap_or(false);

    let (active, status, enabled, uptime_seconds) = if running_as_service {
        let active_output = Command::new("systemctl")
            .args(["show", "ipma.service", "--property=ActiveState"])
            .output()
            .await;

        let active = match active_output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.contains("ActiveState=active")
            }
            Err(_) => false,
        };

        let status_output = Command::new("systemctl")
            .args(["show", "ipma.service", "--property=StatusText"])
            .output()
            .await;

        let status = match status_output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let status_text = stdout.trim().strip_prefix("StatusText=");
                status_text.map(|s| s.to_string())
            }
            Err(_) => None,
        };

        let enabled_output = Command::new("systemctl")
            .args(["is-enabled", "ipma.service"])
            .output()
            .await;

        let enabled = match enabled_output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.trim() == "enabled"
            }
            Err(_) => false,
        };

        let uptime_seconds = if active {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let start_time = START_TIME.load(Ordering::SeqCst);
            Some(now.saturating_sub(start_time))
        } else {
            None
        };

        (active, status, enabled, uptime_seconds)
    } else {
        (false, None, false, None)
    };

    Ok(crate::error::ok_json(
        ServiceStatus {
            registered: service_file_exists,
            running_as_service,
            service_file_exists,
            active,
            status,
            enabled,
            uptime_seconds,
        },
        "server.system.service_status_retrieved",
    ))
}

pub async fn register_service(
    _admin: crate::auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    let exe_path = std::env::current_exe()
        .map_err(|e| AppError::Internal(msg("server.system.exe_path_failed").with("error", e)))?;
    let exe_path_str = exe_path
        .to_str()
        .ok_or_else(|| AppError::Internal(msg("server.system.exe_path_invalid")))?;

    let working_dir = std::env::current_dir()
        .map_err(|e| AppError::Internal(msg("server.system.workdir_failed").with("error", e)))?;
    let working_dir_str = working_dir
        .to_str()
        .ok_or_else(|| AppError::Internal(msg("server.system.workdir_invalid")))?;

    let service_content = format!(
        r#"[Unit]
Description=IPMA - IP/MAC Address Management System
After=network.target postgresql.service

[Service]
Type=simple
WorkingDirectory={}
ExecStart={}
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
"#,
        working_dir_str, exe_path_str
    );

    let service_path = "/etc/systemd/system/ipma.service";
    tokio::fs::write(service_path, service_content)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.service_file_write_failed").with("error", e))
        })?;

    let daemon_reload = Command::new("systemctl")
        .arg("daemon-reload")
        .output()
        .await;

    if let Err(e) = daemon_reload {
        log_warn!("log.system.daemon_reload_failed", error = e);
    }

    let enable_output = Command::new("systemctl")
        .args(["enable", "ipma.service"])
        .output()
        .await;

    if let Err(e) = enable_output {
        log_warn!("log.system.service_enable_failed", error = e);
    }

    let start_output = Command::new("systemctl")
        .args(["start", "ipma.service"])
        .output()
        .await;

    match start_output {
        Ok(output) if output.status.success() => Ok(crate::error::ok_json(
            (),
            "server.system.service_registered",
        )),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(AppError::Internal(
                msg("server.system.service_start_failed").with("error", stderr.trim()),
            ))
        }
        Err(e) => Err(AppError::Internal(
            msg("server.system.service_start_failed").with("error", e),
        )),
    }
}

pub async fn get_dashboard_stats(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let networks_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM network_cidrs")
        .fetch_one(&conn)
        .await?;

    let regions_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM network_regions")
        .fetch_one(&conn)
        .await?;

    let ips_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ips")
        .fetch_one(&conn)
        .await?;

    let ips_active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ips WHERE status = 'active'")
        .fetch_one(&conn)
        .await?;

    // IP 状态分布（status 为自由字符串，按实际取值分组返回）
    let ip_status_rows = sqlx::query("SELECT status, COUNT(*) AS cnt FROM ips GROUP BY status")
        .fetch_all(&conn)
        .await?;
    let ip_by_status: serde_json::Map<String, serde_json::Value> = ip_status_rows
        .iter()
        .map(|r| {
            let status: String = r.get("status");
            let cnt: i64 = r.get("cnt");
            (status, serde_json::Value::from(cnt))
        })
        .collect();

    // 设备类型分布（设备分布图表数据源）
    let device_type_rows =
        sqlx::query("SELECT device_type, COUNT(*) AS cnt FROM devices GROUP BY device_type")
            .fetch_all(&conn)
            .await?;
    let devices_count: i64 = device_type_rows
        .iter()
        .map(|r| {
            let cnt: i64 = r.get("cnt");
            cnt
        })
        .sum();
    let devices_by_type: serde_json::Map<String, serde_json::Value> = device_type_rows
        .iter()
        .map(|r| {
            let dtype: String = r.get("device_type");
            let cnt: i64 = r.get("cnt");
            (dtype, serde_json::Value::from(cnt))
        })
        .collect();

    let rooms_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms")
        .fetch_one(&conn)
        .await?;

    let cabinets_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cabinets")
        .fetch_one(&conn)
        .await?;

    let workstations_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workstations")
        .fetch_one(&conn)
        .await?;

    let positions_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM positions")
        .fetch_one(&conn)
        .await?;

    let users_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&conn)
        .await?;

    let users_active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE status = TRUE")
        .fetch_one(&conn)
        .await?;

    let operations_24h: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM operation_logs WHERE created_at >= NOW() - INTERVAL '24 hours'",
    )
    .fetch_one(&conn)
    .await?;

    let logins_24h: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM login_logs WHERE created_at >= NOW() - INTERVAL '24 hours'",
    )
    .fetch_one(&conn)
    .await?;

    let stats = serde_json::json!({
        "networks": {
            "networks": networks_count,
            "regions": regions_count
        },
        "ips": {
            "total": ips_total,
            "active": ips_active,
            "inactive": ips_total - ips_active,
            "by_status": ip_by_status
        },
        "devices": {
            "total": devices_count,
            "by_type": devices_by_type
        },
        "resources": {
            "rooms": rooms_count,
            "cabinets": cabinets_count,
            "workstations": workstations_count,
            "positions": positions_count
        },
        "users": {
            "total": users_total,
            "active": users_active
        },
        "activity": {
            "operations_24h": operations_24h,
            "logins_24h": logins_24h
        }
    });

    Ok(crate::error::ok_json(
        stats,
        "server.system.dashboard_stats_retrieved",
    ))
}
