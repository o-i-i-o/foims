//! 系统配置接口（SMTP/通知/超时/备份等）。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
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
    pub agent: Option<foims_common::config::AgentConfig>,
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

    // Agent：监听主机与端口合法（绑定失败会令 QUIC 接收不启动）、上报间隔与
    // 历史保留期在业务区间内（与 agent 端 clamp / 清理任务上限对齐）
    let agent = &config.agent;
    if agent.bind_addr.trim().is_empty() {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "agent.bind_addr"),
        ));
    }
    if agent.port == 0 {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "agent.port (1-65535)"),
        ));
    }
    if !(10..=3600).contains(&agent.report_interval_secs) {
        return Err(AppError::Validation(
            msg("server.common.invalid_param")
                .with("param", "agent.report_interval_secs (10-3600)"),
        ));
    }
    if !(1..=60).contains(&agent.collect_interval_secs) {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "agent.collect_interval_secs (1-60)"),
        ));
    }
    if agent.max_report_bytes < 1024 {
        return Err(AppError::Validation(
            msg("server.common.invalid_param")
                .with("param", "agent.max_report_bytes (at least 1024)"),
        ));
    }
    if agent.history_retention_days == 0 {
        return Err(AppError::Validation(
            msg("server.common.invalid_param")
                .with("param", "agent.history_retention_days (at least 1)"),
        ));
    }
    if agent.offline_factor < 2 {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "agent.offline_factor (at least 2)"),
        ));
    }
    // 注册令牌：空/非法值落盘会让 agent 上报鉴权整体失效（持久化自伤）。
    // 该值由启动引导自动生成，配置端点中掩码展示，更新/恢复时空值或掩码
    // 均按磁盘值还原，走到这里的应始终是合法的 64 位十六进制
    if !foims_common::net::valid_enrollment_token(agent.enrollment_token.trim()) {
        return Err(AppError::Validation(
            msg("server.common.invalid_param")
                .with("param", "agent.enrollment_token (64 hex chars)"),
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
    if verify_content != toml_str {
        log_error!(
            "log.config.verify_mismatch",
            expected = toml_str.len(),
            actual = verify_content.len()
        );
        return Err("config file verification failed: content mismatch after write".into());
    }
    log_info!("log.config.verify_read", count = verify_content.len());

    Ok(())
}

/// 系统信息响应载荷（GET /api/system/info）
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct SystemInfo {
    pub name: String,
    pub version: String,
    pub database_status: String,
    pub uptime_seconds: u64,
    /// RFC3339 UTC 服务时间戳
    pub timestamp: String,
    pub pool_metrics: foims_common::db::PoolMetricsSnapshot,
}

pub async fn get_system_info(
    State(state): State<Arc<AppState>>,
    _user: foims_auth::extractor::AuthUser,
) -> Result<Response, AppError> {
    let database_status = match sqlx::query("SELECT 1")
        .execute(&state.pool()?.get_conn())
        .await
    {
        Ok(_) => "connected".to_string(),
        Err(e) => {
            // 错误详情仅入日志，不回传客户端（避免泄漏内网拓扑）
            log_error!("log.system.db_check_failed", error = e);
            "disconnected".to_string()
        }
    };

    // saturating_sub 防时钟回拨下溢（与服务状态 uptime 口径一致）
    let uptime = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(START_TIME.load(Ordering::SeqCst));

    let system_info = SystemInfo {
        name: "FOIMS".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database_status,
        uptime_seconds: uptime,
        timestamp: chrono::Utc::now().to_rfc3339(),
        pool_metrics: state.pool()?.get_metrics(),
    };

    Ok(foims_common::ok_json(
        system_info,
        "server.system.info_retrieved",
    ))
}

pub async fn get_system_config(
    State(state): State<Arc<AppState>>,
    _user: foims_auth::extractor::AuthUser,
) -> Result<Response, AppError> {
    // 读共享槽最新快照（写盘端点成功后已刷新），而非进程启动时快照
    let mut config = (*state.config_snapshot()).clone();
    config.database.password = "***".to_string();
    config.jwt.secret = "***".to_string();
    config.agent.enrollment_token = "***".to_string();
    mask_snmp_trap_secrets(&mut config);
    Ok(foims_common::ok_json(
        config,
        "server.system.config_retrieved",
    ))
}

pub async fn update_system_config(
    State(state): State<Arc<AppState>>,
    _sysadmin: foims_auth::extractor::SysAdminUser,
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

    // JWT/限流配置变更后需重启生效（jwt/rate_limit 在下方被 move，先记录标志）
    let jwt_updated = req.jwt.is_some();

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

    // 限流配置变更后需重启生效（rate_limit 在下方被 move，先记录标志）
    let rate_limit_updated = req.rate_limit.is_some();

    if let Some(rate_limit) = req.rate_limit {
        new_config.rate_limit = rate_limit;
    }

    if let Some(snmp) = req.snmp {
        // 防止脱敏掩码覆写真实密码：掩码密码按用户名还原为磁盘值
        let mut snmp_config = snmp;
        restore_snmp_trap_secrets(&mut snmp_config.trap.users, &new_config.snmp.trap.users);
        new_config.snmp = snmp_config;
    }

    if let Some(mut agent) = req.agent {
        // 防止脱敏掩码/空值覆写真实令牌：掩码按磁盘值还原；空值视同「未携带」
        // （升级窗口期旧版前端缓存提交的 agent 段不含该字段，serde 缺省为空串，
        // 若直接落盘会使所有 agent 上报鉴权失效）
        if agent.enrollment_token == "***" || agent.enrollment_token.is_empty() {
            agent.enrollment_token = new_config.agent.enrollment_token.clone();
        }
        new_config.agent = agent;
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

    // JwtUtils/RateLimiter 在启动时构造一次，密钥轮换与限流调整需重启生效
    if jwt_updated || rate_limit_updated {
        log_warn!("log.config.restart_required_for_jwt_rate_limit");
    }

    // 响应中脱敏数据库密码与 JWT 密钥（与 get_system_config 一致）：
    // 回传明文 JWT secret 等同于允许接收方伪造任意管理员令牌（A-3）
    let mut masked = new_config;
    masked.database.password = "***".to_string();
    masked.jwt.secret = "***".to_string();
    masked.agent.enrollment_token = "***".to_string();
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
    _user: foims_auth::extractor::AuthUser,
) -> Result<Response, AppError> {
    let mut config = (*state.config_snapshot()).clone();
    config.database.password = "***".to_string();
    config.jwt.secret = "***".to_string();
    config.agent.enrollment_token = "***".to_string();
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
    // 备份文件中的注册令牌同为掩码（旧版备份则缺字段为空串）：
    // 掩码/空值一律还原为磁盘真实令牌
    if new_config.agent.enrollment_token == "***" || new_config.agent.enrollment_token.is_empty() {
        new_config.agent.enrollment_token = current_config.agent.enrollment_token.clone();
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

/// 共享注册令牌响应载荷（GET /api/agents/enrollment-token 与 regenerate 共用）
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct EnrollmentTokenResponse {
    pub enrollment_token: String,
}

/// GET /api/agents/enrollment-token：查看共享注册令牌（明文仅本端点返回，
/// 系统配置端点中该字段与数据库密码/JWT 密钥同口径掩码展示）。
pub async fn get_enrollment_token(
    State(state): State<Arc<AppState>>,
    _user: foims_auth::extractor::AdminOrSecAdminUser,
) -> Result<Response, AppError> {
    let config = state.config_snapshot();
    let token = config.agent.enrollment_token.trim().to_string();
    if !foims_common::net::valid_enrollment_token(&token) {
        return Err(AppError::Conflict(msg(
            "server.agent.enrollment_token_missing",
        )));
    }
    Ok(foims_common::ok_json(
        EnrollmentTokenResponse {
            enrollment_token: token,
        },
        "server.agent.enrollment_token_retrieved",
    ))
}

/// POST /api/agents/enrollment-token/regenerate：重新生成共享注册令牌。
/// 共享槽刷新后旧令牌即刻失效：已部署 agent 的上报将被 401 拒收，需
/// 重新下载配置包覆盖安装；不写库，仅改配置文件与内存槽。
pub async fn regenerate_enrollment_token(
    State(state): State<Arc<AppState>>,
    _user: foims_auth::extractor::AdminOrSecAdminUser,
) -> Result<Response, AppError> {
    // 配置写锁：与其他「读-改-写盘」端点互斥
    let _write_guard = state.config_write_lock.lock().await;

    // 以磁盘最新配置为基底（同 restore_config，防旧快照丢更新）
    let mut new_config = tokio::task::spawn_blocking(Config::load)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.system.config_load_failed").with("error", e))
        })?;

    new_config.agent.enrollment_token = foims_common::net::generate_enrollment_token();

    // 落盘前校验（与其他写盘端点同口径）
    validate_config_for_save(&new_config)?;

    save_config_to_file(&new_config).await.map_err(|e| {
        AppError::Internal(msg("server.system.config_save_failed").with("error", e))
    })?;

    // 落盘成功后刷新共享配置槽：上报鉴权即刻启用新令牌
    state.config.store(Arc::new(new_config.clone()));

    log_info!("log.system.enrollment_token_regenerated");

    Ok(foims_common::ok_json(
        EnrollmentTokenResponse {
            enrollment_token: new_config.agent.enrollment_token,
        },
        "server.agent.enrollment_token_regenerated",
    ))
}

#[derive(Debug, Serialize, Deserialize, Validate, ts_rs::TS)]
#[ts(export)]
pub struct UpdateLanguageRequest {
    #[validate(length(min = 2, max = 5, message = "server.system.validation.language_length"))]
    pub language: String,
}

#[derive(Debug, Serialize, Deserialize, Validate, ts_rs::TS)]
#[ts(export)]
pub struct UpdateSessionTimeoutRequest {
    pub session_timeout: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Validate, ts_rs::TS)]
#[ts(export)]
pub struct UpdatePageTimeoutRequest {
    pub page_timeout: Option<u64>,
}

/// 会话超时配置响应载荷（GET /api/system/session-timeout）
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct SessionTimeoutConfig {
    pub session_timeout: Option<u64>,
}

pub async fn get_session_timeout_config(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    // 读共享槽最新快照（写盘端点成功后已刷新）
    let config = state.config_snapshot();
    Ok(foims_common::ok_json(
        SessionTimeoutConfig {
            session_timeout: config.server.session_timeout,
        },
        "server.system.session_timeout_retrieved",
    ))
}

pub async fn update_session_timeout_config(
    State(state): State<Arc<AppState>>,
    _sysadmin: foims_auth::extractor::SysAdminUser,
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

/// 受支持语言条目（GET /api/system/languages）
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct SupportedLanguage {
    pub code: String,
    pub name: String,
    pub native_name: String,
}

pub async fn get_supported_languages() -> Result<Response, AppError> {
    let supported_languages = vec![
        SupportedLanguage {
            code: "en".to_string(),
            name: "English".to_string(),
            native_name: "English".to_string(),
        },
        SupportedLanguage {
            code: "zh".to_string(),
            name: "Chinese".to_string(),
            native_name: "中文".to_string(),
        },
    ];

    Ok(foims_common::ok_json(
        supported_languages,
        "server.system.languages_retrieved",
    ))
}

pub async fn update_language_setting(
    State(state): State<Arc<AppState>>,
    _sysadmin: foims_auth::extractor::SysAdminUser,
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

/// 页面超时配置响应载荷（GET /api/system/page-timeout）
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct PageTimeoutConfig {
    pub page_timeout: Option<u64>,
}

pub async fn get_page_timeout_config(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    // 读共享槽最新快照（写盘端点成功后已刷新）
    let config = state.config_snapshot();
    Ok(foims_common::ok_json(
        PageTimeoutConfig {
            page_timeout: config.server.page_timeout,
        },
        "server.system.page_timeout_retrieved",
    ))
}

pub async fn update_page_timeout_config(
    State(state): State<Arc<AppState>>,
    _sysadmin: foims_auth::extractor::SysAdminUser,
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

#[derive(Debug, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct NotificationSettings {
    pub email_recipients: Vec<Uuid>,
    /// 通知方式矩阵开关（类型 × 邮件，webhook 为前端占位待实现）
    pub methods: foims_auth::smtp::NotificationMethods,
}

pub async fn get_notification_settings(
    State(state): State<Arc<AppState>>,
    _user: foims_auth::extractor::AuthUser,
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
            // 未配置时为全启用默认值，与发送侧口径一致
            methods: foims_auth::smtp::load_notification_methods(&state.pool()?.get_conn()).await,
        },
        "server.notification.settings_retrieved",
    ))
}

pub async fn update_notification_settings(
    State(state): State<Arc<AppState>>,
    _sysadmin: foims_auth::extractor::SysAdminUser,
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

    foims_auth::smtp::save_notification_methods(&state.pool()?.get_conn(), &req.methods).await?;

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
    _user: foims_auth::extractor::AuthUser,
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

#[derive(Debug, Serialize, Deserialize, Validate, ts_rs::TS)]
#[ts(export)]
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
    _sysadmin: foims_auth::extractor::SysAdminUser,
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
    _sysadmin: foims_auth::extractor::SysAdminUser,
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

#[derive(Debug, Serialize, Deserialize, Validate, ts_rs::TS)]
#[ts(export)]
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
    _sysadmin: foims_auth::extractor::SysAdminUser,
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

/// 读取密码策略（未配置时返回等保三级默认值；策略非机密，登录即可读）
pub async fn get_password_policy(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
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

/// 仪表盘统计响应载荷（GET /api/system/dashboard-stats），按业务域分组计数
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DashboardStats {
    pub networks: DashboardNetworksStats,
    pub ips: DashboardIpsStats,
    pub devices: DashboardDevicesStats,
    pub resources: DashboardResourcesStats,
    pub users: DashboardUsersStats,
    pub activity: DashboardActivityStats,
}

/// 网段与区域计数
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DashboardNetworksStats {
    pub networks: i64,
    pub regions: i64,
}

/// IP 地址总量、活跃与状态分布
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DashboardIpsStats {
    pub total: i64,
    pub active: i64,
    pub inactive: i64,
    /// 状态 → 数量分布（status 为自由字符串）
    pub by_status: HashMap<String, i64>,
}

/// 设备总量与类型分布
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DashboardDevicesStats {
    pub total: i64,
    /// 设备类型 → 数量分布
    pub by_type: HashMap<String, i64>,
}

/// 机房资源计数
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DashboardResourcesStats {
    pub rooms: i64,
    pub cabinets: i64,
    pub workstations: i64,
    pub positions: i64,
}

/// 用户总量与活跃数
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DashboardUsersStats {
    pub total: i64,
    pub active: i64,
}

/// 24 小时操作与登录计数
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DashboardActivityStats {
    pub operations_24h: i64,
    pub logins_24h: i64,
}

pub async fn get_dashboard_stats(
    State(state): State<Arc<AppState>>,
    _user: foims_auth::extractor::AuthUser,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let networks_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM network_cidrs")
        .fetch_one(&conn)
        .await?;

    let regions_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM network_regions")
        .fetch_one(&conn)
        .await?;

    let ips_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM interface_ips")
        .fetch_one(&conn)
        .await?;

    let ips_active: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM interface_ips WHERE status = 'active'")
            .fetch_one(&conn)
            .await?;

    // IP 状态分布（status 为自由字符串，按实际取值分组返回）
    let ip_status_rows =
        sqlx::query("SELECT status, COUNT(*) AS cnt FROM interface_ips GROUP BY status")
            .fetch_all(&conn)
            .await?;
    let ip_by_status: HashMap<String, i64> = ip_status_rows
        .iter()
        .map(|r| {
            let status: String = r.get("status");
            let cnt: i64 = r.get("cnt");
            (status, cnt)
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
    let devices_by_type: HashMap<String, i64> = device_type_rows
        .iter()
        .map(|r| {
            let dtype: String = r.get("device_type");
            let cnt: i64 = r.get("cnt");
            (dtype, cnt)
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

    let users_active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE enabled = TRUE")
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

    let stats = DashboardStats {
        networks: DashboardNetworksStats {
            networks: networks_count,
            regions: regions_count,
        },
        ips: DashboardIpsStats {
            total: ips_total,
            active: ips_active,
            inactive: ips_total - ips_active,
            by_status: ip_by_status,
        },
        devices: DashboardDevicesStats {
            total: devices_count,
            by_type: devices_by_type,
        },
        resources: DashboardResourcesStats {
            rooms: rooms_count,
            cabinets: cabinets_count,
            workstations: workstations_count,
            positions: positions_count,
        },
        users: DashboardUsersStats {
            total: users_total,
            active: users_active,
        },
        activity: DashboardActivityStats {
            operations_24h,
            logins_24h,
        },
    };

    Ok(foims_common::ok_json(
        stats,
        "server.system.dashboard_stats_retrieved",
    ))
}
