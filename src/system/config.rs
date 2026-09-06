//! 系统配置接口（SMTP/通知/超时/备份等）。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::system::services::trigger_service_restart;
use foims_auth::smtp::{
    SmtpConfig, get_smtp_config_from_db, save_smtp_config_to_db, send_email_to_users,
};
use foims_common::AppError;
use foims_common::AppJson;
use foims_common::config::{Config, I18nConfig, ServerConfig, SnmpTrapUsmUser};
use foims_common::{log_error, log_info, log_warn, msg};

static START_TIME: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateSystemConfigRequest {
    pub database: Option<foims_common::config::DatabaseConfig>,
    pub server: Option<ServerConfig>,
    pub jwt: Option<foims_common::config::JwtConfig>,
    pub init: Option<foims_common::config::InitConfig>,
    pub rate_limit: Option<foims_common::config::RateLimitConfig>,
    pub snmp: Option<foims_common::config::SnmpConfig>,
}

/// 配置落盘前的校验（对齐 `Config::load` 启动校验与各组件启动期约束）：
/// 写入无法通过启动校验的配置会让服务重启后永久无法启动（持久化自伤）。
fn validate_config_for_save(config: &Config) -> Result<(), AppError> {
    // 数据库：host 非空、端口合法、连接池参数可通过 DbPool 启动期校验
    if config.database.host.trim().is_empty() {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "database.host"),
        ));
    }
    if config.database.port == 0 {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "database.port (1-65535)"),
        ));
    }
    foims_common::db::PoolConfig::from(&config.database)
        .validate()
        .map_err(|e| {
            AppError::Validation(
                msg("server.common.invalid_param").with("param", format!("database.pool: {e}")),
            )
        })?;

    // JWT：密钥非空且 >= 32 字节（与 Config::load 的字节长度校验一致），过期时间必须可解析
    if config.jwt.secret.trim().is_empty() || config.jwt.secret.len() < 32 {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "jwt.secret (at least 32 characters)"),
        ));
    }
    foims_common::config::parse_duration(&config.jwt.access_token_expiry)
        .and_then(|_| foims_common::config::parse_duration(&config.jwt.refresh_token_expiry))
        .map_err(|e| {
            AppError::Validation(
                msg("server.common.invalid_param").with("param", format!("jwt token expiry: {e}")),
            )
        })?;

    // 限流：窗口为 0 会使限流静默失效，限额为 0 会全量 429
    if config.rate_limit.window_secs == 0 || config.rate_limit.email_window_secs == 0 {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "rate_limit.window_secs (at least 1)"),
        ));
    }
    if config.rate_limit.ip_limit == 0
        || config.rate_limit.user_limit == 0
        || config.rate_limit.login_limit == 0
        || config.rate_limit.email_limit == 0
    {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "rate_limit limits (at least 1)"),
        ));
    }

    Ok(())
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

/// 本进程启动时刻（UNIX 秒），供系统信息与服务状态计算运行时长。
pub fn start_time() -> u64 {
    START_TIME.load(Ordering::SeqCst)
}

/// SNMP Trap v3 USM 用户密码的脱敏掩码（与数据库密码/JWT 密钥的 "***" 口径一致）
const SNMP_TRAP_SECRET_MASK: &str = "***";

/// 脱敏配置中的 SNMP Trap v3 USM 用户密码（非空替换为掩码，空保持空）。
/// 用于配置读取响应、更新响应与配置备份，避免明文凭据回传浏览器。
fn mask_snmp_trap_secrets(config: &mut Config) {
    for user in &mut config.snmp.trap.users {
        if !user.auth_password.is_empty() {
            user.auth_password = SNMP_TRAP_SECRET_MASK.to_string();
        }
        if !user.priv_password.is_empty() {
            user.priv_password = SNMP_TRAP_SECRET_MASK.to_string();
        }
    }
}

/// 将请求/备份中携带掩码的 USM 用户密码还原为磁盘配置中的真实密码
/// （按用户名匹配；磁盘上无同名用户时置空，新增用户的密码不应携带掩码）。
fn restore_snmp_trap_secrets(incoming: &mut [SnmpTrapUsmUser], disk: &[SnmpTrapUsmUser]) {
    for user in incoming {
        if user.auth_password == SNMP_TRAP_SECRET_MASK {
            user.auth_password = disk
                .iter()
                .find(|d| d.username == user.username)
                .map_or_else(String::new, |d| d.auth_password.clone());
        }
        if user.priv_password == SNMP_TRAP_SECRET_MASK {
            user.priv_password = disk
                .iter()
                .find(|d| d.username == user.username)
                .map_or_else(String::new, |d| d.priv_password.clone());
        }
    }
}

async fn save_config_to_file(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = foims_common::config::get_config_file_path();
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
    _admin: foims_auth::extractor::AdminUser,
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

    // saturating_sub 防时钟回拨下溢（与服务状态 uptime 口径一致）
    let uptime = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(START_TIME.load(Ordering::SeqCst));

    let pool_metrics = state.pool()?.get_metrics();
    let system_time = chrono::Utc::now();

    let system_info = serde_json::json!({
        "name": "FOIMS",
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

    Ok(foims_common::ok_json(
        system_info,
        "server.system.info_retrieved",
    ))
}

pub async fn get_system_config(
    State(state): State<Arc<AppState>>,
    _admin: foims_auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    // 读共享槽最新快照（写盘端点成功后已刷新），而非进程启动时快照
    let mut config = (*state.config_snapshot()).clone();
    config.database.password = "***".to_string();
    config.jwt.secret = "***".to_string();
    mask_snmp_trap_secrets(&mut config);
    Ok(foims_common::ok_json(
        config,
        "server.system.config_retrieved",
    ))
}

pub async fn update_system_config(
    State(state): State<Arc<AppState>>,
    _admin: foims_auth::extractor::AdminUser,
    AppJson(req): AppJson<UpdateSystemConfigRequest>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 配置写锁：与语言/会话超时/页面超时/恢复配置等「读-改-写盘」端点互斥，
    // 防止并发写盘互相覆盖
    let _write_guard = state.config_write_lock.lock().await;

    // 以磁盘最新配置为基底（而非进程启动时的内存快照）：
    // 先改语言/超时等子配置再改本端点时，避免旧快照整写回盘造成丢失更新
    let mut new_config = tokio::task::spawn_blocking(Config::load)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_failed").with("error", e))
        })?;

    // req 各分支互斥且按值取出，避免逐子结构克隆
    if let Some(database) = req.database {
        // 防止脱敏值覆写真实密码
        let mut db_config = database;
        if db_config.password == "***" {
            db_config.password = new_config.database.password.clone();
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
            jwt_config.secret = new_config.jwt.secret.clone();
        }
        new_config.jwt = jwt_config;
    }

    if let Some(init) = req.init {
        if init.enabled != new_config.init.enabled {
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
        // 防止脱敏掩码覆写真实密码：掩码密码按用户名还原为磁盘值
        let mut snmp_config = snmp;
        restore_snmp_trap_secrets(&mut snmp_config.trap.users, &new_config.snmp.trap.users);
        new_config.snmp = snmp_config;
    }

    // 落盘前校验：拒绝写入启动校验无法通过的配置（防持久化自伤）
    validate_config_for_save(&new_config)?;

    let config_path = foims_common::config::get_config_file_path();
    log_info!("log.config.save_start", path = config_path);

    save_config_to_file(&new_config).await.map_err(|e| {
        AppError::Internal(msg("server.system.config_save_failed").with("error", e))
    })?;
    log_info!("log.config.saved", path = config_path);

    // 落盘成功后刷新共享配置槽，让本进程内后续读取立即生效
    state.config.store(Arc::new(new_config.clone()));

    // 响应中脱敏数据库密码与 JWT 密钥（与 get_system_config 一致）：
    // 回传明文 JWT secret 等同于允许接收方伪造任意管理员令牌（A-3）
    let mut masked = new_config;
    masked.database.password = "***".to_string();
    masked.jwt.secret = "***".to_string();
    mask_snmp_trap_secrets(&mut masked);

    Ok(foims_common::ok_json(
        masked,
        "server.system.config_updated",
    ))
}

// foims 服务重启（含独立进程模式回退）与服务管理接口已迁移至
// system/services.rs：本模块仅保留配置读写与系统信息查询

pub async fn disable_init_mode(
    State(state): State<Arc<AppState>>,
    _admin: foims_auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    log_info!("log.system.disable_init_requested");

    // 与其他「读-改-写盘」端点对齐：持配置写锁 + 以磁盘最新配置为基底 +
    // 落盘前校验。若以启动内存快照为基底整写回盘，启动后的其他写盘变更
    // （数据库密码、JWT 密钥、限流参数）会被旧快照覆写；与其他写盘端点
    // 并发时也存在丢失更新竞态
    let _write_guard = state.config_write_lock.lock().await;

    let mut new_config = tokio::task::spawn_blocking(Config::load)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_failed").with("error", e))
        })?;

    new_config.init.enabled = false;

    // 落盘前校验（与 update_system_config 同口径），拒绝写入启动校验
    // 无法通过的配置
    validate_config_for_save(&new_config)?;

    let config_path = foims_common::config::get_config_file_path();
    let config_str = toml::to_string(&new_config).map_err(|e| {
        AppError::Internal(msg("server.system.config_serialize_failed").with("error", e))
    })?;

    tokio::fs::write(&config_path, config_str)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_write_failed").with("error", e))
        })?;

    // 落盘成功后刷新共享配置槽
    state.config.store(Arc::new(new_config));

    log_info!("log.system.init_disabled_restarting");

    trigger_service_restart().await
}

pub async fn backup_config(
    State(state): State<Arc<AppState>>,
    _admin: foims_auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    let mut config = (*state.config_snapshot()).clone();
    config.database.password = "***".to_string();
    config.jwt.secret = "***".to_string();
    mask_snmp_trap_secrets(&mut config);
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
                    "attachment; filename=foims_config_backup_{}.json",
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
    _admin: foims_auth::extractor::AdminUser,
    AppJson(payload): AppJson<Config>,
) -> Result<Response, AppError> {
    // 与其他「读-改-写盘」配置端点互斥，避免并发写盘互相覆盖
    let _write_guard = state.config_write_lock.lock().await;

    let mut new_config = payload;

    if new_config.init.enabled {
        return Err(AppError::Validation(msg(
            "server.system.init_mode_restore_forbidden",
        )));
    }

    // 防止脱敏值覆写真实密钥：以磁盘最新配置为脱敏值替换来源，
    // 避免把进程启动时的旧密钥写回
    let current_config = tokio::task::spawn_blocking(Config::load)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_failed").with("error", e))
        })?;
    if new_config.database.password == "***" {
        new_config.database.password = current_config.database.password.clone();
    }
    if new_config.jwt.secret == "***" {
        new_config.jwt.secret = current_config.jwt.secret.clone();
    }
    // 备份文件中的 USM 密码同为掩码：还原为磁盘真实密码
    restore_snmp_trap_secrets(
        &mut new_config.snmp.trap.users,
        &current_config.snmp.trap.users,
    );

    // 落盘前校验（与 update_system_config 同口径）：备份文件可能缺校验字段
    // 或被篡改，直接写入会导致重启后服务永久无法启动
    validate_config_for_save(&new_config)?;

    let config_path = foims_common::config::get_config_file_path();
    let config_str = toml::to_string(&new_config).map_err(|e| {
        AppError::Internal(msg("server.system.config_serialize_failed").with("error", e))
    })?;

    tokio::fs::write(&config_path, config_str)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_write_failed").with("error", e))
        })?;

    // 落盘成功后刷新共享配置槽
    state.config.store(Arc::new(new_config));

    Ok(foims_common::ok_json((), "server.system.config_restored"))
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
    // 读共享槽最新快照（写盘端点成功后已刷新）
    let config = state.config_snapshot();
    Ok(foims_common::ok_json(
        serde_json::json!({
            "session_timeout": config.server.session_timeout
        }),
        "server.system.session_timeout_retrieved",
    ))
}

pub async fn update_session_timeout_config(
    State(state): State<Arc<AppState>>,
    _admin: foims_auth::extractor::AdminUser,
    AppJson(req): AppJson<UpdateSessionTimeoutRequest>,
) -> Result<Response, AppError> {
    // 配置写锁：与其他「读-改-写盘」端点互斥
    let _write_guard = state.config_write_lock.lock().await;

    let mut current_config = tokio::task::spawn_blocking(Config::load)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_failed").with("error", e))
        })?;

    current_config.server.session_timeout = req.session_timeout;

    let config_path = foims_common::config::get_config_file_path();
    let config_str = toml::to_string(&current_config).map_err(|e| {
        AppError::Internal(msg("server.system.config_serialize_failed").with("error", e))
    })?;

    tokio::fs::write(&config_path, config_str)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_write_failed").with("error", e))
        })?;

    // 落盘成功后刷新共享配置槽
    state.config.store(Arc::new(current_config));

    Ok(foims_common::ok_json(
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

    Ok(foims_common::ok_json(
        supported_languages,
        "server.system.languages_retrieved",
    ))
}

pub async fn update_language_setting(
    State(state): State<Arc<AppState>>,
    _admin: foims_auth::extractor::AdminUser,
    AppJson(req): AppJson<UpdateLanguageRequest>,
) -> Result<Response, AppError> {
    req.validate()?;

    let language = req.language.to_lowercase();
    if language != "en" && language != "zh" {
        return Err(AppError::Validation(msg(
            "server.system.language_unsupported",
        )));
    }

    // 配置写锁：与其他「读-改-写盘」端点互斥
    let _write_guard = state.config_write_lock.lock().await;

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

    let config_path = foims_common::config::get_config_file_path();
    let config_str = toml::to_string(&current_config).map_err(|e| {
        AppError::Internal(msg("server.system.config_serialize_failed").with("error", e))
    })?;

    tokio::fs::write(&config_path, config_str)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_write_failed").with("error", e))
        })?;

    // 落盘成功后刷新共享配置槽
    state.config.store(Arc::new(current_config));

    Ok(foims_common::ok_json((), "server.system.language_updated"))
}

pub async fn get_page_timeout_config(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    // 读共享槽最新快照（写盘端点成功后已刷新）
    let config = state.config_snapshot();
    Ok(foims_common::ok_json(
        serde_json::json!({
            "page_timeout": config.server.page_timeout
        }),
        "server.system.page_timeout_retrieved",
    ))
}

pub async fn update_page_timeout_config(
    State(state): State<Arc<AppState>>,
    _admin: foims_auth::extractor::AdminUser,
    AppJson(req): AppJson<UpdatePageTimeoutRequest>,
) -> Result<Response, AppError> {
    // 配置写锁：与其他「读-改-写盘」端点互斥
    let _write_guard = state.config_write_lock.lock().await;

    let mut current_config = tokio::task::spawn_blocking(Config::load)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_failed").with("error", e))
        })?;

    current_config.server.page_timeout = req.page_timeout;

    let config_path = foims_common::config::get_config_file_path();
    let config_str = toml::to_string(&current_config).map_err(|e| {
        AppError::Internal(msg("server.system.config_serialize_failed").with("error", e))
    })?;

    tokio::fs::write(&config_path, config_str)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_write_failed").with("error", e))
        })?;

    // 落盘成功后刷新共享配置槽
    state.config.store(Arc::new(current_config));

    Ok(foims_common::ok_json(
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
    _admin: foims_auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    let recipients =
        match sqlx::query_scalar::<_, String>(
            "SELECT value FROM system_configs WHERE config_type = 'notification' AND key = 'email_recipients'",
        )
        .fetch_optional(&state.pool()?.get_conn())
        .await
        {
            Ok(Some(value)) => serde_json::from_str(&value).map_err(|e| {
                // 存量数据损坏时不能静默清空收件人，否则 MAC 变更通知会失效
                foims_common::log_error!("log.system.recipients_parse_failed", error = e);
                AppError::Internal(
                    msg("server.notification.recipients_parse_failed").with("error", e),
                )
            })?,
            // 查询失败必须向上传播：吞成空列表会把 DB 故障伪装成「无收件人」
            Ok(None) => Vec::new(),
            Err(e) => {
                return Err(AppError::Database(
                    msg("server.db.operation_failed").with("error", e),
                ));
            }
        };

    Ok(foims_common::ok_json(
        NotificationSettings {
            email_recipients: recipients,
        },
        "server.notification.settings_retrieved",
    ))
}

pub async fn update_notification_settings(
    State(state): State<Arc<AppState>>,
    _admin: foims_auth::extractor::AdminUser,
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

    Ok(foims_common::ok_json(
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
    _admin: foims_auth::extractor::AdminUser,
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

    Ok(foims_common::ok_json(resp, "server.smtp.config_retrieved"))
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
    _admin: foims_auth::extractor::AdminUser,
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

    Ok(foims_common::ok_json((), "server.smtp.config_updated"))
}

/// 测试已保存的通知邮件（SMTP）配置连通性。无需请求体。
pub async fn test_smtp_connection(
    State(state): State<Arc<AppState>>,
    _admin: foims_auth::extractor::AdminUser,
) -> Result<Response, AppError> {
    let config = match get_smtp_config_from_db(&state.pool()?.get_conn()).await {
        Some(c) => c,
        None => {
            log_warn!("log.smtp.not_configured");
            return Err(AppError::Validation(msg("server.smtp.not_configured")));
        }
    };

    foims_auth::smtp::test_smtp_connection(&config).await?;

    Ok(foims_common::ok_json((), "server.smtp.test_success"))
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
    _admin: foims_auth::extractor::AdminUser,
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

    Ok(foims_common::ok_json((), "server.smtp.email_sent"))
}

// ==================== 等保密码策略配置 ====================

/// 读取密码策略（未配置时返回等保三级默认值）
pub async fn get_password_policy(
    State(state): State<Arc<AppState>>,
    _secadmin: foims_auth::extractor::SecAdminUser,
) -> Result<Response, AppError> {
    let policy = foims_auth::password_policy::load(&state.pool()?.get_conn()).await;
    Ok(foims_common::ok_json(
        policy,
        "server.system.config_retrieved",
    ))
}

/// 保存密码策略（长度下限 8、各数值范围由模块内钳制）
pub async fn update_password_policy(
    State(state): State<Arc<AppState>>,
    _secadmin: foims_auth::extractor::SecAdminUser,
    AppJson(req): AppJson<foims_auth::password_policy::PasswordPolicy>,
) -> Result<Response, AppError> {
    foims_auth::password_policy::save(&state.pool()?.get_conn(), &req).await?;
    Ok(foims_common::ok_json(
        (),
        "server.system.password_policy_updated",
    ))
}

// ==================== 服务管理 ====================
// foims / nginx 的状态查询与管理操作见 system/services.rs
// （GET /api/system/services、POST /api/system/services/{service}/{op}）

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

    Ok(foims_common::ok_json(
        stats,
        "server.system.dashboard_stats_retrieved",
    ))
}
