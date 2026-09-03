//! 登录认证：密码/2FA/邮箱验证码登录、令牌签发刷新与认证中间件。

use std::sync::Arc;

use axum::Json;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, SameSite};
use bcrypt::verify;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use jsonwebtoken::errors::ErrorKind;
use rand::RngExt;
use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

use crate::extractor::{AccessToken, RefreshToken, SecureFlag};
use crate::meta::{RequestMeta, log_op_best_effort};
use crate::provider::AuthProvider;
use crate::utils::{JwtUtils, extract_token_from_parts, get_client_info_from_parts, hash_password};
use foims_common::AppError;
use foims_common::AppJson;
use foims_common::crypto::{decrypt_password_async, encrypt_password_async};
use foims_common::msg;
use foims_models::{
    EmailLoginRequest, ForgotPasswordRequest, ResetPasswordRequest, SendLoginCodeRequest,
    SendTwoFactorCodeRequest, TwoFactorLoginRequest, User, UserLogin,
};
use totp_rs::{Algorithm, Builder, Secret};

/// TOTP 已用码存储：保留每个用户最近 N 个已用码。
/// 仅记录最后 1 个码时，skew=1 窗口允许的相邻窗口历史码可重放（A-6）；
/// 保留 3 个覆盖 ±1 个 30s 窗口的全部合法码。
type TotpReplayStore =
    std::sync::Mutex<std::collections::HashMap<Uuid, Vec<(String, std::time::Instant)>>>;
static TOTP_REPLAY_STORE: std::sync::OnceLock<TotpReplayStore> = std::sync::OnceLock::new();

/// 每用户保留的已用码数量（覆盖 skew=1 的相邻窗口）
const TOTP_REPLAY_KEEP: usize = 3;

const TOTP_REPLAY_TTL: std::time::Duration = std::time::Duration::from_secs(120);

fn totp_replay_store() -> &'static TotpReplayStore {
    TOTP_REPLAY_STORE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// 吊销检查所需的数据库不可用时返回 503，中间件内无法用 `?` 传播，统一走此响应
fn db_unavailable_response() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(foims_common::ApiResponse::<()>::error(msg(
            "server.db.operation_failed",
        ))),
    )
        .into_response()
}

// ==================== 公开邮件端点发送频控 ====================

/// 每 IP / 每目标标识在窗口内允许的邮件发送次数
const EMAIL_SEND_LIMIT: usize = 3;
/// 邮件发送频控滑动窗口（进程内）
const EMAIL_SEND_WINDOW: std::time::Duration = std::time::Duration::from_secs(3600);

/// 发送时刻记录：键（"ip:<addr>" / "target:<标识>"）→ 窗口内的发送时刻列表
static EMAIL_SEND_RECORDS: std::sync::LazyLock<DashMap<String, Vec<std::time::Instant>>> =
    std::sync::LazyLock::new(DashMap::new);

/// 滑动窗口频控检查并登记一次发送；超过限制时返回需等待的秒数
fn check_email_send_limit(key: &str) -> Result<(), u64> {
    let now = std::time::Instant::now();
    let mut entry = EMAIL_SEND_RECORDS.entry(key.to_string()).or_default();
    entry.retain(|t| now.duration_since(*t) < EMAIL_SEND_WINDOW);
    if entry.len() >= EMAIL_SEND_LIMIT {
        let oldest = entry.first().copied().unwrap_or(now);
        let wait = EMAIL_SEND_WINDOW
            .saturating_sub(now.duration_since(oldest))
            .as_secs()
            .max(1);
        return Err(wait);
    }
    entry.push(now);
    Ok(())
}

/// 公开邮件端点统一频控入口：每 IP 与每目标标识各自限频（防邮件轰炸）。
fn enforce_email_send_limit(ip: &str, target: &str) -> Result<(), AppError> {
    for key in [format!("ip:{ip}"), format!("target:{target}")] {
        if let Err(wait) = check_email_send_limit(&key) {
            return Err(AppError::Validation(
                msg("server.common.rate_limited").with("seconds", wait),
            ));
        }
    }
    Ok(())
}

/// 周期回收邮件发送频控记录：滑动窗口外的发送时刻随查询清理仅覆盖
/// 被命中的键，公开端点可用海量伪造 IP+邮箱组合把 Map 撑到无上限；
/// 由低频清理任务定期调用，移除整键使空 Map 收缩
pub(crate) fn cleanup_email_send_records() {
    let now = std::time::Instant::now();
    EMAIL_SEND_RECORDS.retain(|_, times| {
        times.retain(|t| now.duration_since(*t) < EMAIL_SEND_WINDOW);
        !times.is_empty()
    });
}

// ==================== 请求模型扩展 ====================

/// 邮箱验证码登录请求：在 foims-models 基础模型上扩展图形验证码字段
///（连续失败达到阈值后必填；基础模型跨 crate 共享，字段经 flatten 组合）
#[derive(Debug, Deserialize)]
pub struct EmailCodeLoginRequest {
    #[serde(flatten)]
    pub base: EmailLoginRequest,
    /// 连续失败触发后的图形验证码 id
    pub captcha_id: Option<String>,
    /// 连续失败触发后的图形验证码输入
    pub captcha_text: Option<String>,
}

/// 密码 + TOTP 两步登录请求：在 foims-models 基础模型上扩展图形验证码字段
#[derive(Debug, Deserialize)]
pub struct TwoFactorCodeLoginRequest {
    #[serde(flatten)]
    pub base: TwoFactorLoginRequest,
    /// 连续失败触发后的图形验证码 id
    pub captcha_id: Option<String>,
    /// 连续失败触发后的图形验证码输入
    pub captcha_text: Option<String>,
}

/// 原子占位一个 TOTP 码：未被使用则登记并返回 true，TTL 内已使用则返回 false。
/// 「先占位、后校验、失败回滚」替代旧的「检查→异步校验→记录」三步，
/// 消除并发请求携带同一验证码时同时通过重放检查的竞态窗口。
fn claim_totp_code(user_id: Uuid, code: &str) -> bool {
    let store = totp_replay_store();
    let mut map = store.lock().unwrap_or_else(|e| e.into_inner());
    let entry = map.entry(user_id).or_default();
    // 先按 TTL 清理过期项，再查重与登记
    entry.retain(|(_, used_at)| used_at.elapsed() < TOTP_REPLAY_TTL);
    if entry.iter().any(|(used_code, _)| used_code == code) {
        return false;
    }
    entry.push((code.to_string(), std::time::Instant::now()));
    if entry.len() > TOTP_REPLAY_KEEP {
        let drop_count = entry.len() - TOTP_REPLAY_KEEP;
        entry.drain(0..drop_count);
    }
    true
}

/// 回滚占位：TOTP 校验失败时释放该码，避免占位语义误伤用户的合法重试
fn release_totp_code(user_id: Uuid, code: &str) {
    let store = totp_replay_store();
    let mut map = store.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = map.get_mut(&user_id) {
        entry.retain(|(used_code, _)| used_code != code);
    }
}

pub async fn auth_middleware<P: AuthProvider>(
    State(state): State<Arc<P>>,
    req: Request,
    next: Next,
) -> Response {
    let (mut parts, body) = req.into_parts();

    let Some(token) = extract_token_from_parts(&parts) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(foims_common::ApiResponse::<()>::error(msg(
                "server.auth.auth_failed",
            ))),
        )
            .into_response();
    };

    let claims = match state.jwt_utils().validate_token(&token) {
        Ok(claims) => claims,
        Err(err) => {
            let error_key = match err.kind() {
                ErrorKind::ExpiredSignature => "server.auth.token_expired",
                _ => "server.auth.invalid_token",
            };
            return (
                StatusCode::UNAUTHORIZED,
                Json(foims_common::ApiResponse::<()>::error(msg(error_key))),
            )
                .into_response();
        }
    };

    if claims.token_type != "access" {
        return (
            StatusCode::UNAUTHORIZED,
            Json(foims_common::ApiResponse::<()>::error(msg(
                "server.auth.invalid_token",
            ))),
        )
            .into_response();
    }

    // 检查令牌是否已被撤销。数据库不可用时 fail-closed 拒绝请求
    // （与 refresh 流程策略一致），防止 DB 故障期间被吊销令牌继续通行
    match state.pool() {
        Ok(pool) => match crate::utils::is_token_revoked(&pool.get_conn(), &token).await {
            Ok(true) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(foims_common::ApiResponse::<()>::error(msg(
                        "server.auth.token_revoked",
                    ))),
                )
                    .into_response();
            }
            Ok(false) => {
                // 用户状态与令牌吊销点检查（同一中间件内完成，不延迟到 refresh）：
                //   1. 账户被禁用 → 直接拒绝；
                //   2. 令牌签发时间早于 tokens_invalidated_at（密码重置/权限变更
                //      吊销点）→ 已被吊销，旧 access 令牌立即失效
                let user_id = match Uuid::parse_str(&claims.sub) {
                    Ok(id) => id,
                    Err(_) => {
                        return (
                            StatusCode::UNAUTHORIZED,
                            Json(foims_common::ApiResponse::<()>::error(msg(
                                "server.auth.auth_failed",
                            ))),
                        )
                            .into_response();
                    }
                };
                match sqlx::query_as::<_, (bool, chrono::DateTime<Utc>)>(
                    "SELECT status, tokens_invalidated_at FROM users WHERE id = $1",
                )
                .bind(user_id)
                .fetch_optional(&pool.get_conn())
                .await
                {
                    Ok(Some((status, invalidated_at))) => {
                        if !status {
                            return (
                                StatusCode::UNAUTHORIZED,
                                Json(foims_common::ApiResponse::<()>::error(msg(
                                    "server.auth.account_disabled",
                                ))),
                            )
                                .into_response();
                        }
                        if (claims.iat as i64) <= invalidated_at.timestamp() {
                            return (
                                StatusCode::UNAUTHORIZED,
                                Json(foims_common::ApiResponse::<()>::error(msg(
                                    "server.auth.token_invalidated_relogin",
                                ))),
                            )
                                .into_response();
                        }
                    }
                    // 用户已被删除：令牌随之为失效
                    Ok(None) => {
                        return (
                            StatusCode::UNAUTHORIZED,
                            Json(foims_common::ApiResponse::<()>::error(msg(
                                "server.auth.auth_failed",
                            ))),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        foims_common::log_error!("log.auth.check_revoke_failed", error = e);
                        return db_unavailable_response();
                    }
                }
            }
            Err(e) => {
                foims_common::log_error!("log.auth.check_revoke_failed", error = e);
                return db_unavailable_response();
            }
        },
        Err(e) => {
            foims_common::log_error!("log.auth.check_revoke_failed", error = e);
            return db_unavailable_response();
        }
    }

    let (ip_address, user_agent) = get_client_info_from_parts(&parts);
    let current_fingerprint = JwtUtils::generate_device_fingerprint(&user_agent, &ip_address);

    if let Some(token_fingerprint) = &claims.device_fingerprint
        && token_fingerprint != &current_fingerprint
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(foims_common::ApiResponse::<()>::error(msg(
                "server.auth.device_validation_failed",
            ))),
        )
            .into_response();
    }

    parts.extensions.insert(claims);

    let req = Request::from_parts(parts, body);
    next.run(req).await
}

pub async fn localhost_only_middleware<P: AuthProvider>(req: Request, next: Next) -> Response {
    let (parts, body) = req.into_parts();

    // 判定请求来源是否为本机，要点：
    //   1. TCP 直连（extensions 携带 ConnectInfo）时以真实对端地址为准——
    //      X-Real-IP 是普通请求头，内网客户端可任意伪造（如 `X-Real-IP:
    //      127.0.0.1`），直接据此判定回环等于开放鉴权绕过；
    //   2. 仅当对端确为回环地址（本机直连，或经监听在同一台机器上的 nginx
    //      转发）或连接来自 UDS（无对端信息，由本机 nginx 代理）时，才采信
    //      X-Real-IP 头判定真实来源——nginx 以
    //      `proxy_set_header X-Real-IP $remote_addr;` 覆盖式写入，取自
    //      TCP 对端，客户端无法伪造。切勿改用 X-Forwarded-For：其经
    //      `$proxy_add_x_forwarded_for` 会保留客户端伪造的首段值。
    //      注意：nginx 部署在远端主机（对端为非回环的私网地址）时不在
    //      信任范围，本机专用端点经该代理访问会被拒绝；
    //   3. fail-close：无法判定来源时默认拒绝。
    let peer_loopback = parts
        .extensions
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .map(|info| info.0.ip().is_loopback());

    let header_ip = parts
        .headers
        .get("X-Real-IP")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<std::net::IpAddr>().ok());

    let is_localhost = match peer_loopback {
        // 回环对端：本机直连或经本机 nginx 转发，按 X-Real-IP 判定真实来源
        //（无头时视为本机直连放行）
        Some(true) => header_ip.is_none_or(|ip| ip.is_loopback()),
        // 非回环对端：真实来源即对端，请求头不可信，直接拒绝
        Some(false) => false,
        // UDS：无对端信息，仅能依赖本机反代写入的 X-Real-IP 判定
        None => header_ip.is_some_and(|ip| ip.is_loopback()),
    };

    if !is_localhost {
        return (
            StatusCode::FORBIDDEN,
            Json(foims_common::ApiResponse::<()>::error(msg(
                "server.auth.access_denied",
            ))),
        )
            .into_response();
    }

    let req = Request::from_parts(parts, body);
    next.run(req).await
}

pub async fn login<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<UserLogin>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    req.validate()?;

    // 用户键统一使用原始输入（trim 后）：检查键与记录键为同一标识符，
    // 避免出现「按邮箱检查、按用户名记录」的键错位（邮箱爆破绕过账户封禁）
    let login_identifier = req.username.trim();

    // 连续失败达到阈值后要求图形验证码（未达阈值时直接放行）
    if let Err(key) = crate::captcha::enforce(
        &meta.ip_address,
        login_identifier,
        &req.captcha_id,
        &req.captcha_text,
    ) {
        return Err(AppError::Validation(msg(key)));
    }

    // 应用层 fail2ban: 检查 IP 与用户名是否被封禁（用户名维度拦截
    // 分布式来源针对同一账户的爆破，A-1）
    let client_ip = meta.ip_address.as_str();
    if crate::app_fail2ban::is_ip_banned(client_ip) {
        let remaining = crate::app_fail2ban::get_ban_remaining(client_ip);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }
    if crate::app_fail2ban::is_user_banned(login_identifier) {
        let remaining = crate::app_fail2ban::get_user_ban_remaining(login_identifier);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }

    let user_row = match sqlx::query_as::<
        sqlx::Postgres,
        (
            Uuid,
            String,
            String,
            String,
            String,
            bool,
            bool,
            String,
            chrono::DateTime<Utc>,
            chrono::DateTime<Utc>,
        ),
    >(
        "SELECT id, username, password_hash, email, role, status, two_factor_enabled, auth_provider, created_at, updated_at FROM users WHERE username = $1 OR email = $1",
    )
    .bind(login_identifier)
    .fetch_optional(&conn)
    .await?
    {
        Some(row) => row,
        None => {
            // 等价 bcrypt 校验后再返回，抹平用户名枚举时间侧信道（A-4）
            dummy_bcrypt_verify(&req.password).await;
            crate::app_fail2ban::record_login_failure(
                client_ip,
                login_identifier,
                "server.login_log.user_not_found",
            );
            if let Err(e) = log_login(&conn, login_identifier, &meta.ip_address, &meta.user_agent, false, Some("server.login_log.user_not_found")).await {
                foims_common::log_warn!("log.login.record_failed", error = e);
            }
            return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
        }
    };

    let (
        id,
        username,
        password_hash,
        email,
        role,
        status,
        two_factor_enabled,
        auth_provider,
        created_at,
        updated_at,
    ) = user_row;

    // 防「换标识」绕过：命中 DB 记录后对 DB 用户名键复查封禁
    //（以邮箱登录的请求同样受 DB username 维度封禁约束）
    if crate::app_fail2ban::is_user_banned(&username) {
        let remaining = crate::app_fail2ban::get_user_ban_remaining(&username);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }

    if !status {
        crate::app_fail2ban::record_login_failure(
            client_ip,
            login_identifier,
            "server.login_log.account_disabled",
        );
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("server.login_log.account_disabled"),
        )
        .await
        {
            foims_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
    }

    // 外部认证账户（LDAP/SSO）不持有本地密码：
    //   1. 返回与用户不存在/密码错误一致的统一 login_failed 文案，
    //      并等价执行 dummy bcrypt 抹平耗时侧信道——专属错误会暴露
    //      LDAP/SSO 账户名单（账户枚举）；
    //   2. 按失败登录口径计入 fail2ban 与登录审计（该尝试本就是
    //      凭据校验失败，与其余失败分支一致）
    if auth_provider != "local" {
        dummy_bcrypt_verify(&req.password).await;
        crate::app_fail2ban::record_login_failure(
            client_ip,
            login_identifier,
            "server.auth.external_account_use_provider_login",
        );
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("server.auth.external_account_use_provider_login"),
        )
        .await
        {
            foims_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
    }

    let password_for_verify = req.password;
    let hash_for_verify = password_hash;
    let valid = tokio::task::spawn_blocking(move || verify(&password_for_verify, &hash_for_verify))
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.auth.password_verify_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.auth.password_verify_failed").with("error", e))
        })?;
    if !valid {
        crate::app_fail2ban::record_login_failure(
            client_ip,
            login_identifier,
            "server.login_log.invalid_password",
        );
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("server.login_log.invalid_password"),
        )
        .await
        {
            foims_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
    }

    if two_factor_enabled {
        return Ok(foims_common::ok_json(
            serde_json::json!({ "requires_two_factor": true, "username": username }),
            "server.common.success",
        ));
    }

    // 等保密码有效期：过期后拒绝登录（由安全管理员重置或走找回流程）
    if crate::password_policy::is_expired(&conn, id).await? {
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("server.login_log.password_expired"),
        )
        .await
        {
            foims_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg("server.auth.password_expired")));
    }

    let jwt_utils = &state.jwt_utils();
    let device_fingerprint =
        JwtUtils::generate_device_fingerprint(&meta.user_agent, &meta.ip_address);

    let remember_me = req.remember_me.unwrap_or(false);
    let login_tokens = generate_login_tokens(
        jwt_utils,
        &id,
        &username,
        &role,
        &device_fingerprint,
        &meta.ip_address,
        remember_me,
    )?;

    let user = User {
        id,
        username: username.clone(),
        email,
        role: role.clone(),
        status,
        two_factor_enabled,
        two_factor_verified: true,
        created_at,
        updated_at,
    };

    if let Err(e) = log_login(
        &conn,
        &username,
        &meta.ip_address,
        &meta.user_agent,
        true,
        None,
    )
    .await
    {
        foims_common::log_warn!("log.login.record_failed", error = e);
    }
    // 成功后清除本次使用的原始输入键与 DB 用户名键两个维度的失败记录
    crate::app_fail2ban::record_login_success(client_ip, login_identifier);
    crate::app_fail2ban::record_login_success(client_ip, &username);
    foims_common::log_info!("log.login.success", username = username);

    build_login_response(user, login_tokens, meta.is_secure)
}

pub async fn login_with_email_code<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<EmailCodeLoginRequest>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    req.base.validate()?;

    // 连续失败达到阈值后要求图形验证码（与本地密码登录同阈值逻辑）
    if let Err(key) = crate::captcha::enforce(
        &meta.ip_address,
        req.base.email.trim(),
        &req.captcha_id,
        &req.captcha_text,
    ) {
        return Err(AppError::Validation(msg(key)));
    }

    let req = req.base;
    let email = req.email.trim();

    // 应用层 fail2ban：邮箱验证码同样纳入 IP/账户维度爆破防护（A-5）
    //（此前仅密码/TOTP 登录有联动，6 位数字码可被不限速爆破）
    // 用户键统一使用原始输入（trim 后的邮箱）：检查键与记录键为同一标识符
    let client_ip = meta.ip_address.as_str();
    if crate::app_fail2ban::is_ip_banned(client_ip) {
        let remaining = crate::app_fail2ban::get_ban_remaining(client_ip);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }
    if crate::app_fail2ban::is_user_banned(email) {
        let remaining = crate::app_fail2ban::get_user_ban_remaining(email);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }

    let user_row = match sqlx::query_as::<
        sqlx::Postgres,
        (
            Uuid,
            String,
            String,
            String,
            bool,
            bool,
            Option<String>,
            Option<DateTime<Utc>>,
            String,
            DateTime<Utc>,
            DateTime<Utc>,
        ),
    >(
        "SELECT id, username, email, role, status, two_factor_enabled, two_factor_email_code, two_factor_email_code_expiry, auth_provider, created_at, updated_at FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(&conn)
    .await?
    {
        Some(row) => row,
        None => {
            // 未注册邮箱：记录失败计入 fail2ban（防止对任意邮箱爆破验证码）
            crate::app_fail2ban::record_login_failure(
                client_ip,
                email,
                "server.login_log.user_not_found",
            );
            if let Err(e) = log_login(&conn, email, &meta.ip_address, &meta.user_agent, false, Some("server.login_log.user_not_found")).await {
                foims_common::log_warn!("log.login.record_failed", error = e);
            }
            return Err(AppError::Unauthorized(msg("server.auth.email_or_code_invalid")));
        }
    };

    let (
        id,
        username,
        email,
        role,
        status,
        two_factor_enabled,
        code,
        expiry,
        auth_provider,
        created_at,
        updated_at,
    ) = user_row;

    // 防「换标识」绕过：命中 DB 记录后对 DB 用户名键复查封禁
    //（按邮箱登录的请求同样受 DB username 维度封禁约束）
    if crate::app_fail2ban::is_user_banned(&username) {
        let remaining = crate::app_fail2ban::get_user_ban_remaining(&username);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }

    // 外部认证账户不提供邮箱验证码通道
    if auth_provider != "local" {
        return Err(AppError::Unauthorized(msg(
            "server.auth.external_account_use_provider_login",
        )));
    }

    if !status {
        // 禁用账户同样计入 fail2ban（与本地密码登录的禁用分支口径一致）
        crate::app_fail2ban::record_login_failure(
            client_ip,
            &email,
            "server.login_log.account_disabled",
        );
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("server.login_log.account_disabled"),
        )
        .await
        {
            foims_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg(
            "server.auth.email_or_code_invalid",
        )));
    }

    let mut verified = false;
    if let (Some(c), Some(e)) = (code, expiry) {
        let trimmed_input_code = req.code.trim();
        let trimmed_db_code = c.trim();
        if constant_time_eq(trimmed_db_code, trimmed_input_code) && e > Utc::now() {
            // 验证与作废同语句原子完成：验证码一次性语义不被并发请求破坏
            match sqlx::query(
                "UPDATE users SET two_factor_email_code = NULL, two_factor_email_code_expiry = NULL \
                 WHERE id = $1 AND BTRIM(two_factor_email_code) = $2",
            )
            .bind(id)
            .bind(trimmed_db_code)
            .execute(&conn)
            .await
            {
                Ok(result) if result.rows_affected() > 0 => verified = true,
                // affected=0：验证码已被并发请求抢先消费，按验证失败处理
                Ok(_) => {}
                // 清除失败按验证失败处理（fail-closed），用户重新获取验证码即可
                Err(e) => {
                    return Err(AppError::Database(
                        msg("server.db.operation_failed").with("error", e),
                    ));
                }
            }
        }
    }

    if !verified {
        // 错误验证码计入 fail2ban：连续失败即封禁该 IP 与该邮箱（A-5）
        // 记录键与入口检查键一致（原始输入的邮箱），保证计数可触发封禁
        crate::app_fail2ban::record_login_failure(
            client_ip,
            &email,
            "server.login_log.invalid_email_code",
        );
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("server.login_log.invalid_email_code"),
        )
        .await
        {
            foims_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg(
            "server.auth.code_invalid_or_expired",
        )));
    }

    if two_factor_enabled {
        return Ok(foims_common::ok_json(
            serde_json::json!({ "requires_two_factor": true, "username": username }),
            "server.common.success",
        ));
    }

    // 等保密码有效期：与本地密码登录/两步登录一致，签发令牌前检查有效期，
    // 防止密码过期用户凭邮箱验证码取得完整登录态
    if crate::password_policy::is_expired(&conn, id).await? {
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("server.login_log.password_expired"),
        )
        .await
        {
            foims_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg("server.auth.password_expired")));
    }

    let jwt_utils = &state.jwt_utils();
    let device_fingerprint =
        JwtUtils::generate_device_fingerprint(&meta.user_agent, &meta.ip_address);

    let remember_me = req.remember_me.unwrap_or(false);
    let login_tokens = generate_login_tokens(
        jwt_utils,
        &id,
        &username,
        &role,
        &device_fingerprint,
        &meta.ip_address,
        remember_me,
    )?;

    let user = User {
        id,
        username: username.clone(),
        email,
        role: role.clone(),
        status,
        two_factor_enabled,
        two_factor_verified: true,
        created_at,
        updated_at,
    };

    if let Err(e) = log_login(
        &conn,
        &username,
        &meta.ip_address,
        &meta.user_agent,
        true,
        None,
    )
    .await
    {
        foims_common::log_warn!("log.login.record_failed", error = e);
    }
    // 成功后清除本次使用的原始输入键（邮箱）与 DB 用户名键两个维度的失败记录
    crate::app_fail2ban::record_login_success(client_ip, &user.email);
    crate::app_fail2ban::record_login_success(client_ip, &username);
    foims_common::log_info!("log.login.email_code_success", username = username);

    build_login_response(user, login_tokens, meta.is_secure)
}

pub async fn send_login_code<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<SendLoginCodeRequest>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();
    let email = req.email.trim();

    req.validate()?;

    // 邮件轰炸防护：每 IP 与每目标邮箱滑动窗口限频
    enforce_email_send_limit(&meta.ip_address, email)?;

    let user_row = match sqlx::query_as::<sqlx::Postgres, (Uuid, String, bool)>(
        "SELECT id, username, status FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(&conn)
    .await?
    {
        Some(row) => row,
        None => {
            return Ok(foims_common::ok_json((), "server.auth.code_sent"));
        }
    };

    let (id, _username, status) = user_row;

    if !status {
        return Ok(foims_common::ok_json((), "server.auth.code_sent"));
    }

    let code: String = {
        let mut rng = rand::rng();
        (0..6)
            .map(|_| rng.random_range(0..10).to_string())
            .collect()
    };
    let expiry = Utc::now() + chrono::Duration::minutes(5);

    sqlx::query("UPDATE users SET two_factor_email_code = $1, two_factor_email_code_expiry = $2 WHERE id = $3")
        .bind(&code).bind(expiry).bind(id).execute(&conn).await?;

    let email_body = format!("您的登录验证码是：{code}");
    crate::smtp::send_email_async(&conn, email, "登录验证码", &email_body).await?;

    Ok(foims_common::ok_json((), "server.auth.code_sent"))
}

pub async fn login_with_two_factor<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<TwoFactorCodeLoginRequest>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    req.base.validate()?;

    // 连续失败达到阈值后要求图形验证码（与本地密码登录同阈值逻辑，
    // 防止在 2FA/邮箱码端点间切换绕过验证码递进）
    if let Err(key) = crate::captcha::enforce(
        &meta.ip_address,
        req.base.username.trim(),
        &req.captcha_id,
        &req.captcha_text,
    ) {
        return Err(AppError::Validation(msg(key)));
    }

    let req = req.base;

    // 应用层 fail2ban: 检查 IP 与用户名是否被封禁
    // 用户键统一使用原始输入（trim 后）：检查键与记录键为同一标识符
    let login_identifier = req.username.trim();
    let client_ip = meta.ip_address.as_str();
    if crate::app_fail2ban::is_ip_banned(client_ip) {
        let remaining = crate::app_fail2ban::get_ban_remaining(client_ip);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }
    if crate::app_fail2ban::is_user_banned(login_identifier) {
        let remaining = crate::app_fail2ban::get_user_ban_remaining(login_identifier);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }

    let user_row = match sqlx::query_as::<
        sqlx::Postgres,
        (
            Uuid,
            String,
            String,
            String,
            String,
            bool,
            bool,
            Option<String>,
            DateTime<Utc>,
            DateTime<Utc>,
        ),
    >(
        "SELECT id, username, password_hash, email, role, status, two_factor_enabled, two_factor_secret, created_at, updated_at FROM users WHERE username = $1",
    )
    .bind(login_identifier)
    .fetch_optional(&conn)
    .await?
    {
        Some(row) => row,
        None => {
            // 等价 bcrypt 校验抹平时间侧信道（A-4）
            if let Some(password) = req.password.as_deref() {
                dummy_bcrypt_verify(password).await;
            }
            crate::app_fail2ban::record_login_failure(
                client_ip,
                login_identifier,
                "server.login_log.user_not_found",
            );
            // 登录审计对齐本地密码登录：用户不存在分支同样写入 login_logs
            if let Err(e) = log_login(
                &conn,
                login_identifier,
                &meta.ip_address,
                &meta.user_agent,
                false,
                Some("server.login_log.user_not_found"),
            )
            .await
            {
                foims_common::log_warn!("log.login.record_failed", error = e);
            }
            return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
        }
    };

    let (
        id,
        username,
        password_hash,
        email,
        role,
        status,
        two_factor_enabled,
        secret,
        created_at,
        updated_at,
    ) = user_row;

    // 防「换标识」绕过：命中 DB 记录后对 DB 用户名键复查封禁
    if crate::app_fail2ban::is_user_banned(&username) {
        let remaining = crate::app_fail2ban::get_user_ban_remaining(&username);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }

    let password_for_verify = req
        .password
        .ok_or_else(|| AppError::Validation(msg("server.auth.2fa_password_required")))?;
    let hash_for_verify = password_hash;
    let valid = tokio::task::spawn_blocking(move || verify(&password_for_verify, &hash_for_verify))
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.auth.password_verify_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.auth.password_verify_failed").with("error", e))
        })?;
    if !valid {
        crate::app_fail2ban::record_login_failure(
            client_ip,
            login_identifier,
            "server.login_log.invalid_password",
        );
        return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
    }

    if !status {
        crate::app_fail2ban::record_login_failure(
            client_ip,
            login_identifier,
            "server.login_log.account_disabled",
        );
        // 登录审计对齐本地密码登录：禁用分支同样写入 login_logs
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("server.login_log.account_disabled"),
        )
        .await
        {
            foims_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg("server.auth.account_disabled")));
    }

    if !two_factor_enabled {
        return Err(AppError::Validation(msg("server.auth.2fa_not_enabled")));
    }

    let mut verified = false;
    if let Some(encrypted_secret) = secret {
        let secret = decrypt_password_async(encrypted_secret)
            .await
            .map_err(|e| {
                AppError::Internal(msg("server.auth.2fa_secret_decrypt_failed").with("error", e))
            })?;
        let secret = match Secret::try_from_base32(&secret) {
            Ok(s) => s,
            Err(e) => {
                if let Err(e) = log_login(
                    &conn,
                    &username,
                    &meta.ip_address,
                    &meta.user_agent,
                    false,
                    Some("server.login_log.invalid_2fa_secret_format"),
                )
                .await
                {
                    foims_common::log_warn!("log.login.record_failed", error = e);
                }
                return Err(AppError::Internal(
                    msg("server.auth.2fa_secret_format_invalid").with("error", e),
                ));
            }
        };
        match Builder::new()
            .with_algorithm(Algorithm::SHA1)
            .with_digits(6)
            .with_skew(1)
            .with_step_duration(30)
            .with_secret(secret)
            .build()
        {
            Ok(totp) => {
                let code = req.two_factor_code;
                // 原子占位后再校验，失败回滚，见 claim_totp_code 注释
                if !claim_totp_code(id, &code) {
                    if let Err(e) = log_login(
                        &conn,
                        &username,
                        &meta.ip_address,
                        &meta.user_agent,
                        false,
                        Some("server.login_log.totp_code_replayed"),
                    )
                    .await
                    {
                        foims_common::log_warn!("log.login.record_failed", error = e);
                    }
                    return Err(AppError::Unauthorized(msg("server.auth.2fa_code_reused")));
                }
                let code_for_check = code.clone();
                let valid = tokio::task::spawn_blocking(move || {
                    totp.check_current(&code_for_check).is_some()
                })
                .await
                .map_err(|e| {
                    AppError::Internal(msg("server.auth.2fa_verify_task_failed").with("error", e))
                })?;
                if valid {
                    verified = true;
                } else {
                    release_totp_code(id, &code);
                }
            }
            Err(e) => {
                if let Err(e) = log_login(
                    &conn,
                    &username,
                    &meta.ip_address,
                    &meta.user_agent,
                    false,
                    Some("server.login_log.secret_too_short"),
                )
                .await
                {
                    foims_common::log_warn!("log.login.record_failed", error = e);
                }
                return Err(AppError::Internal(
                    msg("server.auth.2fa_secret_too_short").with("error", e),
                ));
            }
        }
    }

    if !verified {
        crate::app_fail2ban::record_login_failure(
            client_ip,
            login_identifier,
            "server.login_log.invalid_2fa_code",
        );
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("server.login_log.invalid_2fa_code"),
        )
        .await
        {
            foims_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg("server.auth.code_invalid")));
    }

    // 等保密码有效期：与本地密码登录一致，密码验证通过后仍需检查有效期，
    // 防止启用 2FA 的用户在密码过期后经本端点绕过有效期策略
    if crate::password_policy::is_expired(&conn, id).await? {
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("server.login_log.password_expired"),
        )
        .await
        {
            foims_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg("server.auth.password_expired")));
    }

    let jwt_utils = &state.jwt_utils();
    let device_fingerprint =
        JwtUtils::generate_device_fingerprint(&meta.user_agent, &meta.ip_address);
    let remember_me = req.remember_me.unwrap_or(false);
    let login_tokens = generate_login_tokens(
        jwt_utils,
        &id,
        &username,
        &role,
        &device_fingerprint,
        &meta.ip_address,
        remember_me,
    )?;

    let user = User {
        id,
        username,
        email,
        role,
        status,
        two_factor_enabled,
        two_factor_verified: true,
        created_at,
        updated_at,
    };

    if let Err(e) = log_login(
        &conn,
        &user.username,
        &meta.ip_address,
        &meta.user_agent,
        true,
        None,
    )
    .await
    {
        foims_common::log_warn!("log.login.record_failed", error = e);
    }
    // 成功后清除本次使用的原始输入键与 DB 用户名键两个维度的失败记录
    crate::app_fail2ban::record_login_success(client_ip, login_identifier);
    crate::app_fail2ban::record_login_success(client_ip, &user.username);
    foims_common::log_info!("log.login.2fa_success", username = user.username);

    build_login_response(user, login_tokens, meta.is_secure)
}

pub async fn send_two_factor_code<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<SendTwoFactorCodeRequest>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    req.validate()?;
    let username = req.username.trim();

    // 应用层 fail2ban：IP 或账户维度封禁中直接拒绝——该端点公开可达且
    // 携带真实 bcrypt 校验，封禁者继续放行等于提供免费的口令探测通道
    if crate::app_fail2ban::is_ip_banned(&meta.ip_address) {
        let remaining = crate::app_fail2ban::get_ban_remaining(&meta.ip_address);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }
    if crate::app_fail2ban::is_user_banned(username) {
        let remaining = crate::app_fail2ban::get_user_ban_remaining(username);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }

    // 邮件轰炸防护：每 IP 与每目标用户滑动窗口限频
    enforce_email_send_limit(&meta.ip_address, username)?;

    // 密码必填并真实验证：该端点公开可达，凭用户名即可触发邮件的
    // 行为等同于账户密码探测通道，必须先通过口令校验
    //（密码不做 trim，与本地登录的口令校验口径一致）
    let Some(password) = req.password.as_deref().filter(|p| !p.is_empty()) else {
        return Err(AppError::Validation(msg(
            "server.auth.2fa_password_required",
        )));
    };

    let user_row = sqlx::query_as::<sqlx::Postgres, (Uuid, String, String, String)>(
        "SELECT id, username, email, password_hash FROM users WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(&conn)
    .await?;

    let (id, username, email, password_hash) = match user_row {
        Some(row) => row,
        None => {
            // 用户不存在：等价 bcrypt 校验抹平时间侧信道后返回统一成功响应
            //（防用户枚举，与未启用 2FA 等场景的对外表现一致）
            dummy_bcrypt_verify(password).await;
            return Ok(foims_common::ok_json((), "server.auth.send_ok"));
        }
    };

    let password_for_verify = password.to_string();
    let hash_for_verify = password_hash;
    let valid = tokio::task::spawn_blocking(move || verify(&password_for_verify, &hash_for_verify))
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.auth.password_verify_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.auth.password_verify_failed").with("error", e))
        })?;
    if !valid {
        // 密码错误计入 fail2ban；对外返回与用户不存在一致的响应（防枚举）
        crate::app_fail2ban::record_login_failure(
            &meta.ip_address,
            &username,
            "server.login_log.invalid_password",
        );
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("server.login_log.invalid_password"),
        )
        .await
        {
            foims_common::log_warn!("log.login.record_failed", error = e);
        }
        return Ok(foims_common::ok_json((), "server.auth.send_ok"));
    }

    let code: String = {
        let mut rng = rand::rng();
        (0..6)
            .map(|_| rng.random_range(0..10).to_string())
            .collect()
    };
    let expiry = Utc::now() + chrono::Duration::minutes(5);
    sqlx::query("UPDATE users SET two_factor_email_code = $1, two_factor_email_code_expiry = $2 WHERE id = $3")
        .bind(&code).bind(expiry).bind(id).execute(&conn).await?;

    let email_body = format!("您的两步验证码是：{code}");
    crate::smtp::send_email_async(&conn, &email, "两步验证码", &email_body).await?;

    Ok(foims_common::ok_json((), "server.auth.code_sent"))
}

pub async fn logout<P: AuthProvider>(
    State(state): State<Arc<P>>,
    AccessToken(access_token): AccessToken,
    RefreshToken(refresh_token): RefreshToken,
    SecureFlag(secure): SecureFlag,
) -> Result<Response, AppError> {
    // 撤销 access_token
    if let Some(access_token) = access_token
        && let Ok(claims) = state.jwt_utils().validate_token(&access_token)
    {
        let user_id = Uuid::parse_str(&claims.sub).ok();
        let expiry = chrono::DateTime::from_timestamp(claims.exp as i64, 0)
            .unwrap_or(chrono::Utc::now() + chrono::Duration::hours(1));
        if let Err(e) =
            crate::utils::revoke_token(&state.pool()?.get_conn(), &access_token, user_id, expiry)
                .await
        {
            foims_common::log_warn!("log.auth.revoke_access_token_failed", error = e);
        }
    }

    // 撤销 refresh_token（长期凭证，撤销失败必须让客户端感知登出未完成）
    if let Some(refresh_token) = refresh_token
        && let Ok(claims) = state.jwt_utils().validate_token(&refresh_token)
    {
        let user_id = Uuid::parse_str(&claims.sub).ok();
        let expiry = chrono::DateTime::from_timestamp(claims.exp as i64, 0)
            .unwrap_or(chrono::Utc::now() + chrono::Duration::days(7));
        if let Err(e) =
            crate::utils::revoke_token(&state.pool()?.get_conn(), &refresh_token, user_id, expiry)
                .await
        {
            foims_common::log_error!("log.auth.revoke_refresh_token_failed", error = e);
            return Err(AppError::Database(
                msg("server.db.operation_failed").with("error", e),
            ));
        }
    }

    let access_cookie = create_clear_cookie("access_token", secure);
    let refresh_cookie = create_clear_cookie("refresh_token", secure);

    let mut response = foims_common::ok_json((), "server.common.success");
    append_cookie_to_response(&mut response, &access_cookie)?;
    append_cookie_to_response(&mut response, &refresh_cookie)?;
    Ok(response)
}

pub async fn refresh_token<P: AuthProvider>(
    State(state): State<Arc<P>>,
    RefreshToken(token): RefreshToken,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let token = match token {
        Some(t) => t,
        None => return Err(AppError::Unauthorized(msg("server.auth.auth_failed"))),
    };

    let jwt_utils = &state.jwt_utils();

    let claims = jwt_utils.validate_token(&token).map_err(|err| {
        let key = match err.kind() {
            ErrorKind::ExpiredSignature => "server.auth.token_expired",
            _ => "server.auth.invalid_token",
        };
        AppError::Unauthorized(msg(key))
    })?;

    if claims.token_type != "refresh" {
        return Err(AppError::Unauthorized(msg(
            "server.auth.refresh_token_invalid",
        )));
    }

    if crate::utils::is_token_revoked(&conn, &token)
        .await
        .map_err(|e| {
            foims_common::log_error!("log.auth.check_revoke_failed", error = e);
            AppError::Database(msg("server.db.operation_failed"))
        })?
    {
        return Err(AppError::Unauthorized(msg("server.auth.token_revoked")));
    }

    let ip_address = &meta.ip_address;
    let user_agent = &meta.user_agent;
    let current_fingerprint = JwtUtils::generate_device_fingerprint(user_agent, ip_address);

    if let Some(ref token_fingerprint) = claims.device_fingerprint
        && token_fingerprint != &current_fingerprint
    {
        return Err(AppError::Unauthorized(msg(
            "server.auth.device_validation_failed",
        )));
    }

    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|e| AppError::Internal(msg("server.auth.user_id_invalid").with("error", e)))?;

    // 重新校验用户当前状态与角色（防止禁用/降权/删除后旧令牌仍可刷新），
    // 并强制在 tokens_invalidated_at 之后签发的令牌才能刷新（密码重置/权限变更后吊销历史令牌）
    let user_state = sqlx::query_as::<_, (String, bool, chrono::DateTime<chrono::Utc>)>(
        "SELECT role, status, tokens_invalidated_at FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&conn)
    .await
    .map_err(|e| AppError::Database(msg("server.db.operation_failed").with("error", e)))?;

    let current_role = match user_state {
        Some((role, status, invalidated_at)) => {
            if !status {
                return Err(AppError::Unauthorized(msg("server.auth.account_disabled")));
            }
            // 令牌签发时间不晚于吊销时间点 → 已被吊销。
            // 与 auth_middleware 的比较口径一致（<=）：恰落在吊销秒上
            // 签发的令牌不允许「中间件拒绝但可刷新」的缝隙
            if (claims.iat as i64) <= invalidated_at.timestamp() {
                return Err(AppError::Unauthorized(msg(
                    "server.auth.token_invalidated_relogin",
                )));
            }
            role
        }
        None => return Err(AppError::Unauthorized(msg("server.auth.account_not_found"))),
    };

    // 等保密码有效期：密码过期用户禁止续期，防止持有未过期 refresh
    // Cookie 绕过各登录路径的过期检查无限续期
    if crate::password_policy::is_expired(&conn, user_id).await? {
        return Err(AppError::Unauthorized(msg("server.auth.password_expired")));
    }

    let token_expiry =
        chrono::DateTime::from_timestamp(claims.exp as i64, 0).unwrap_or_else(Utc::now);
    // 旋转令牌必须先成功撤销旧令牌：撤销失败（如 DB 故障）仍签发新令牌的话，
    // 旧 refresh token 在其有效期内继续可用，轮换防重放失效（A-8）
    if let Err(e) = crate::utils::revoke_token(&conn, &token, Some(user_id), token_expiry).await {
        foims_common::log_error!("log.auth.revoke_token_failed", error = e);
        return Err(AppError::Database(msg("server.db.operation_failed")));
    }

    // 保持登录意图读签发时写入的显式 claim；未携带该 claim 的存量令牌
    // 缺省按非持久会话处理（刷新后回落短期会话），不再以 token 时长
    // 反推用户意图——该启发式会把普通会话升级为 7 天持久会话
    let remember_me = claims.remember_me.unwrap_or(false);

    let access_token = jwt_utils
        .generate_access_token(
            &user_id,
            &claims.username,
            &current_role,
            Some(&current_fingerprint),
            Some(ip_address),
        )
        .map_err(|e| {
            AppError::Internal(msg("server.auth.token_generate_failed").with("error", e))
        })?;
    let new_refresh_token = jwt_utils
        .generate_refresh_token(
            &user_id,
            &claims.username,
            &current_role,
            Some(&current_fingerprint),
            Some(ip_address),
            remember_me,
        )
        .map_err(|e| {
            AppError::Internal(msg("server.auth.token_generate_failed").with("error", e))
        })?;

    let access_token_expiry = jwt_utils.get_access_token_expiry();
    let refresh_token_expiry = jwt_utils.get_actual_refresh_token_expiry(remember_me);

    foims_common::log_debug!("log.login.token_refreshed", username = claims.username);

    let secure = meta.is_secure;
    let access_cookie = create_auth_cookie(
        "access_token",
        &access_token,
        access_token_expiry as i64,
        secure,
    );
    let refresh_cookie = create_auth_cookie(
        "refresh_token",
        &new_refresh_token,
        refresh_token_expiry as i64,
        secure,
    );

    let mut response = foims_common::ok_json(
        serde_json::json!({ "expires_in": access_token_expiry, "remember_me": remember_me }),
        "server.common.success",
    );
    append_cookie_to_response(&mut response, &access_cookie)?;
    append_cookie_to_response(&mut response, &refresh_cookie)?;
    Ok(response)
}

pub async fn get_current_user<P: AuthProvider>(
    auth: crate::extractor::AuthUser,
) -> Result<Response, AppError> {
    Ok(foims_common::ok_json(
        serde_json::json!({ "id": auth.sub, "username": auth.username, "role": auth.role }),
        "server.common.success",
    ))
}

pub async fn forgot_password<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<ForgotPasswordRequest>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    req.validate()?;
    let email = req.email.trim();

    // 邮件轰炸防护：每 IP 与每目标邮箱滑动窗口限频
    enforce_email_send_limit(&meta.ip_address, email)?;

    let user_result = sqlx::query_as::<_, (Uuid, String, bool)>(
        "SELECT id, username, two_factor_enabled FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(&conn)
    .await;

    match user_result {
        Ok(Some((_user_id, _username, _two_factor_enabled))) => {
            let reset_token = Uuid::new_v4().to_string();
            let expiry = Utc::now() + chrono::Duration::hours(1);

            if let Err(e) = sqlx::query(
                "UPDATE users SET reset_token = $1, reset_token_expiry = $2 WHERE email = $3",
            )
            .bind(&reset_token)
            .bind(expiry)
            .bind(email)
            .execute(&conn)
            .await
            {
                foims_common::log_error!("log.auth.save_reset_token_failed", error = e);
            } else {
                let smtp_config = crate::smtp::get_smtp_config_from_db(&conn).await;
                if let Some(ref _config) = smtp_config {
                    // 使用应用公共URL（而非SMTP主机名）构建重置链接
                    let base_url = state.config().server.public_url.trim_end_matches('/');
                    let reset_link = format!("{base_url}/reset-password?token={reset_token}");
                    let email_body = format!("请点击以下链接重置密码：{reset_link}");
                    if let Err(e) =
                        crate::smtp::send_email_async(&conn, email, "密码重置", &email_body).await
                    {
                        foims_common::log_error!("log.auth.send_reset_email_failed", error = e);
                    }
                }
            }
        }
        Ok(None) => {}
        // 对外仍返回统一成功响应（防用户枚举），但 DB 错误必须记录：
        // 否则用户"收不到重置邮件"时无任何排查线索
        Err(e) => {
            foims_common::log_error!("log.auth.forgot_password_query_failed", error = e);
        }
    }

    Ok(foims_common::ok_json(
        (),
        "server.auth.forgot_password_sent",
    ))
}

pub async fn reset_password<P: AuthProvider>(
    State(state): State<Arc<P>>,
    AppJson(req): AppJson<ResetPasswordRequest>,
) -> Result<Response, AppError> {
    let mut tx = state.pool()?.begin().await?;

    req.validate()?;

    let user_result = sqlx::query_as::<_, (Uuid,)>(
        "SELECT id FROM users WHERE reset_token = $1 AND reset_token_expiry > NOW() FOR UPDATE",
    )
    .bind(&req.token)
    .fetch_optional(&mut *tx)
    .await?;

    match user_result {
        Some((user_id,)) => {
            // 等保密码策略：复杂度 + 历史重复检查
            crate::password_policy::validate_password(
                &state.pool()?.get_conn(),
                user_id,
                &req.new_password,
            )
            .await?;

            let hashed_password = hash_password(&req.new_password).await?;

            sqlx::query(
                "UPDATE users SET password_hash = $1, reset_token = NULL, reset_token_expiry = NULL, tokens_invalidated_at = NOW(), password_changed_at = NOW() WHERE id = $2",
            )
            .bind(&hashed_password)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

            // 密码写入与历史记录同事务提交，避免「密码已生效但历史缺失」
            crate::password_policy::record_history(
                &state.pool()?.get_conn(),
                &mut tx,
                user_id,
                &hashed_password,
            )
            .await
            .map_err(AppError::from)?;

            tx.commit().await?;

            Ok(foims_common::ok_json(
                (),
                "server.auth.reset_password_success",
            ))
        }
        None => Err(AppError::Validation(msg("server.auth.reset_link_invalid"))),
    }
}

/// 自助修改密码（已登录用户；修改成功后吊销全部令牌强制重新登录）。
#[derive(Debug, serde::Deserialize, validator::Validate)]
pub struct ChangePasswordRequest {
    #[validate(length(min = 1, message = "server.auth.validation.password_required"))]
    pub old_password: String,
    #[validate(length(min = 8, message = "server.user.validation.password_length"))]
    pub new_password: String,
}

pub async fn change_password<P: AuthProvider>(
    State(state): State<Arc<P>>,
    auth: crate::extractor::AuthUser,
    meta: RequestMeta,
    AppJson(req): AppJson<ChangePasswordRequest>,
) -> Result<Response, AppError> {
    req.validate()?;

    let conn = state.pool()?.get_conn();
    let user_id = Uuid::parse_str(&auth.sub)
        .map_err(|e| AppError::Validation(msg("server.common.user_id_invalid").with("error", e)))?;

    let (password_hash,): (String,) =
        sqlx::query_as("SELECT password_hash FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&conn)
            .await?
            .ok_or_else(|| AppError::NotFound(msg("server.user.not_found")))?;

    // 旧密码校验
    let old_for_verify = req.old_password.clone();
    let hash_for_verify = password_hash;
    let valid = tokio::task::spawn_blocking(move || verify(&old_for_verify, &hash_for_verify))
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.auth.password_verify_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.auth.password_verify_failed").with("error", e))
        })?;
    if !valid {
        return Err(AppError::Validation(msg(
            "server.auth.old_password_incorrect",
        )));
    }

    // 等保密码策略：复杂度 + 历史重复检查
    crate::password_policy::validate_password(&conn, user_id, &req.new_password).await?;

    let hashed_password = hash_password(&req.new_password).await?;

    // 密码写入与历史记录同事务提交，避免「密码已生效但历史缺失」
    let mut tx = conn.begin().await?;

    sqlx::query(
        "UPDATE users SET password_hash = $1, tokens_invalidated_at = NOW(), password_changed_at = NOW(), updated_at = NOW() WHERE id = $2",
    )
    .bind(&hashed_password)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    crate::password_policy::record_history(&conn, &mut tx, user_id, &hashed_password)
        .await
        .map_err(AppError::from)?;

    tx.commit().await?;

    let details = serde_json::json!({ "username": auth.username });
    log_op_best_effort(
        &conn,
        &meta,
        "change_password",
        "user",
        Some(&user_id),
        &details,
    )
    .await;

    Ok(foims_common::ok_json((), "server.auth.password_changed"))
}

pub async fn init_two_factor<P: AuthProvider>(
    State(state): State<Arc<P>>,
    auth: crate::extractor::AuthUser,
    AppJson(req): AppJson<TwoFactorInitRequest>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    req.validate()?;

    let target_user_id = if let Some(user_id) = req.user_id {
        if auth.sub != user_id.to_string() && auth.role != "admin" {
            return Err(AppError::Forbidden(msg("server.auth.2fa_admin_only_init")));
        }
        user_id
    } else {
        Uuid::parse_str(&auth.sub).map_err(|e| {
            AppError::Validation(msg("server.common.user_id_invalid").with("error", e))
        })?
    };

    let target_username: String =
        match sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&conn)
            .await?
        {
            Some(name) => name,
            None => {
                return Err(AppError::NotFound(msg("server.user.not_found")));
            }
        };

    let two_factor_enabled: bool =
        sqlx::query_scalar("SELECT two_factor_enabled FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_one(&conn)
            .await?;
    if two_factor_enabled {
        return Err(AppError::Conflict(msg("server.auth.2fa_already_enabled")));
    }

    let secret_bytes: Vec<u8> = {
        use rand::Rng;
        let mut bytes = vec![0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        bytes
    };
    let secret = Secret::from(secret_bytes);
    let secret_base32 = secret.to_base32();

    let totp = Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(secret)
        .with_issuer(Some("FOIMS"))
        .with_account_name(target_username.clone())
        .build()
        .map_err(|e| {
            AppError::Internal(msg("server.auth.totp_generate_failed").with("error", e))
        })?;

    let encrypted_secret = encrypt_password_async(secret_base32.clone()).await?;
    sqlx::query("UPDATE users SET two_factor_secret = $1 WHERE id = $2")
        .bind(&encrypted_secret)
        .bind(target_user_id)
        .execute(&conn)
        .await?;

    let otpauth_url = totp
        .to_url()
        .map_err(|e| AppError::Internal(msg("server.auth.otpauth_url_failed").with("error", e)))?;
    let totp_for_qr = totp;
    let qr_code_base64 =
        tokio::task::spawn_blocking(move || totp_for_qr.to_qr_base64().map_err(|e| e.to_string()))
            .await
            .map_err(|e| {
                AppError::Internal(msg("server.auth.qr_generate_task_failed").with("error", e))
            })?
            .map_err(|e| {
                AppError::Internal(msg("server.auth.qr_generate_failed").with("error", e))
            })?;

    Ok(foims_common::ok_json(
        serde_json::json!({
            "otpauth_url": otpauth_url,
            "qr_code_base64": qr_code_base64,
            "secret": secret_base32,
        }),
        "server.auth.2fa_init_success",
    ))
}

pub async fn enable_two_factor<P: AuthProvider>(
    State(state): State<Arc<P>>,
    auth: crate::extractor::AuthUser,
    AppJson(req): AppJson<TwoFactorEnableRequest>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    req.validate()?;

    let target_user_id = if let Some(user_id) = req.user_id {
        if auth.sub != user_id.to_string() && auth.role != "admin" {
            return Err(AppError::Forbidden(msg(
                "server.auth.2fa_admin_only_enable",
            )));
        }
        user_id
    } else {
        Uuid::parse_str(&auth.sub).map_err(|e| {
            AppError::Validation(msg("server.common.user_id_invalid").with("error", e))
        })?
    };

    let secret: Option<String> =
        match sqlx::query_scalar("SELECT two_factor_secret FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&conn)
            .await?
        {
            Some(s) => s,
            None => {
                return Err(AppError::Validation(msg("server.auth.2fa_not_initialized")));
            }
        };

    let Some(encrypted_secret) = secret else {
        return Err(AppError::Validation(msg("server.auth.2fa_not_initialized")));
    };

    let secret = decrypt_password_async(encrypted_secret)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.auth.2fa_secret_decrypt_failed").with("error", e))
        })?;

    let target_username: String =
        match sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&conn)
            .await?
        {
            Some(name) => name,
            None => {
                return Err(AppError::NotFound(msg("server.user.not_found")));
            }
        };

    let secret = Secret::try_from_base32(&secret)
        .map_err(|_| AppError::Validation(msg("server.auth.2fa_secret_format_invalid")))?;

    let totp = Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(secret)
        .with_issuer(Some("FOIMS"))
        .with_account_name(target_username)
        .build()
        .map_err(|e| AppError::Internal(msg("server.auth.totp_create_failed").with("error", e)))?;

    let code = req.code;
    // 原子占位后再校验，失败回滚，见 claim_totp_code 注释
    if !claim_totp_code(target_user_id, &code) {
        return Err(AppError::Validation(msg("server.auth.2fa_code_reused")));
    }
    let code_for_check = code.clone();
    let valid = tokio::task::spawn_blocking(move || totp.check_current(&code_for_check).is_some())
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.auth.2fa_verify_task_failed").with("error", e))
        })?;
    if !valid {
        release_totp_code(target_user_id, &code);
        return Err(AppError::Validation(msg("server.auth.code_wrong")));
    }

    sqlx::query(
        "UPDATE users SET two_factor_enabled = true, two_factor_verified = true WHERE id = $1",
    )
    .bind(target_user_id)
    .execute(&conn)
    .await?;

    Ok(foims_common::ok_json((), "server.auth.2fa_enabled"))
}

pub async fn disable_two_factor<P: AuthProvider>(
    State(state): State<Arc<P>>,
    auth: crate::extractor::AuthUser,
    AppJson(req): AppJson<TwoFactorDisableRequest>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    req.validate()?;

    let target_user_id = if let Some(user_id) = req.user_id {
        if auth.sub != user_id.to_string() && auth.role != "admin" {
            return Err(AppError::Forbidden(msg(
                "server.auth.2fa_admin_only_disable",
            )));
        }
        user_id
    } else {
        Uuid::parse_str(&auth.sub).map_err(|e| {
            AppError::Validation(msg("server.common.user_id_invalid").with("error", e))
        })?
    };

    let (secret, two_factor_enabled): (Option<String>, bool) =
        sqlx::query_as("SELECT two_factor_secret, two_factor_enabled FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_one(&conn)
            .await?;

    if !two_factor_enabled {
        return Err(AppError::Validation(msg("server.auth.2fa_not_enabled")));
    }

    let target_username: String =
        match sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&conn)
            .await?
        {
            Some(name) => name,
            None => {
                return Err(AppError::NotFound(msg("server.user.not_found")));
            }
        };

    let mut verified = false;

    if let Some(encrypted_secret) = secret {
        let secret = decrypt_password_async(encrypted_secret)
            .await
            .map_err(|e| {
                AppError::Internal(msg("server.auth.2fa_secret_decrypt_failed").with("error", e))
            })?;
        let secret = Secret::try_from_base32(&secret).map_err(|e| {
            AppError::Validation(msg("server.auth.2fa_secret_format_invalid").with("error", e))
        })?;
        let totp = Builder::new()
            .with_algorithm(Algorithm::SHA1)
            .with_digits(6)
            .with_skew(1)
            .with_step_duration(30)
            .with_secret(secret)
            .with_issuer(Some("FOIMS"))
            .with_account_name(target_username)
            .build()
            .map_err(|e| {
                AppError::Internal(msg("server.auth.2fa_secret_too_short").with("error", e))
            })?;
        let code = req.code;
        // 原子占位后再校验，失败回滚，见 claim_totp_code 注释
        if !claim_totp_code(target_user_id, &code) {
            return Err(AppError::Validation(msg("server.auth.2fa_code_reused")));
        }
        let code_for_check = code.clone();
        let valid =
            tokio::task::spawn_blocking(move || totp.check_current(&code_for_check).is_some())
                .await
                .map_err(|e| {
                    AppError::Internal(msg("server.auth.2fa_verify_task_failed").with("error", e))
                })?;
        if valid {
            verified = true;
        } else {
            release_totp_code(target_user_id, &code);
        }
    }

    if !verified {
        return Err(AppError::Validation(msg("server.auth.code_wrong")));
    }

    sqlx::query(
        "UPDATE users SET two_factor_enabled = false, two_factor_secret = NULL, two_factor_verified = false WHERE id = $1"
    )
    .bind(target_user_id)
    .execute(&conn)
    .await?;

    Ok(foims_common::ok_json((), "server.auth.2fa_disabled"))
}

#[derive(Debug, Deserialize, Validate)]
pub struct TwoFactorInitRequest {
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct TwoFactorEnableRequest {
    #[validate(length(min = 6, max = 6, message = "server.auth.validation.code_length"))]
    pub code: String,
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct TwoFactorDisableRequest {
    #[validate(length(min = 6, max = 6, message = "server.auth.validation.code_length"))]
    pub code: String,
    pub user_id: Option<Uuid>,
}

pub(crate) struct LoginTokens {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) access_token_expiry: u64,
    pub(crate) refresh_token_expiry: u64,
}

pub(crate) struct ExternalUser {
    pub(crate) id: Uuid,
    pub(crate) username: String,
    pub(crate) email: String,
    pub(crate) role: String,
    pub(crate) status: bool,
    pub(crate) two_factor_enabled: bool,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

/// 外部认证（LDAP/SSO）用户查找或自动建户：
/// - 本地账户（auth_provider='local'）不允许外部登录接管；
/// - 同 provider 已存在 → 复用并同步邮箱；
/// - 不存在 → 以随机不可用密码哈希建户（仅能通过对应外部方式登录）。
pub(crate) async fn find_or_create_external_user(
    conn: &sqlx::PgPool,
    provider: &str,
    username: &str,
    email: Option<&str>,
    default_role: &str,
) -> Result<ExternalUser, AppError> {
    let username = username.trim();
    if username.len() < 3 || username.len() > 50 {
        return Err(AppError::Validation(msg(
            "server.auth.external_username_invalid",
        )));
    }

    if let Some(row) = sqlx::query_as::<
        sqlx::Postgres,
        (
            Uuid,
            String,
            String,
            String,
            bool,
            bool,
            String,
            DateTime<Utc>,
            DateTime<Utc>,
        ),
    >(
        "SELECT id, username, email, role, status, two_factor_enabled, auth_provider, created_at, updated_at
           FROM users WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(conn)
    .await?
    {
        let (id, db_username, mut db_email, role, status, two_factor_enabled, auth_provider, created_at, updated_at) = row;
        if auth_provider != provider {
            return Err(AppError::Conflict(msg("server.auth.provider_mismatch")));
        }
        // 同步外部侧邮箱变化（同步失败保留原值并告警，外部账户仍可正常登录）
        if let Some(new_email) = email
            && new_email != db_email
        {
            match sqlx::query("UPDATE users SET email = $2, updated_at = NOW() WHERE id = $1")
                .bind(id)
                .bind(new_email)
                .execute(conn)
                .await
            {
                Ok(_) => db_email = new_email.to_string(),
                Err(e) => {
                    foims_common::log_warn!("log.auth.external_email_sync_failed", error = e);
                }
            }
        }
        return Ok(ExternalUser {
            id,
            username: db_username,
            email: db_email,
            role,
            status,
            two_factor_enabled,
            created_at,
            updated_at,
        });
    }

    // 角色合法性兜底（配置更新时已校验，此处防御性重验）
    let role = if foims_models::validate_role(default_role).is_ok() {
        default_role.to_string()
    } else {
        "user".to_string()
    };

    // 随机 32 字节口令的哈希：外部账户无本地密码，该哈希永不匹配任何用户输入
    let random_secret = {
        use rand::Rng;
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        hex::encode(bytes)
    };
    let password_hash = hash_password(&random_secret).await?;

    let email_value = email
        .filter(|e| e.contains('@'))
        .map(String::from)
        .unwrap_or_else(|| format!("{username}@{provider}.invalid"));

    let inserted = sqlx::query_as::<
        sqlx::Postgres,
        (
            Uuid,
            String,
            String,
            String,
            bool,
            bool,
            DateTime<Utc>,
            DateTime<Utc>,
        ),
    >(
        "INSERT INTO users (username, password_hash, email, role, status, auth_provider)
         VALUES ($1, $2, $3, $4, TRUE, $5)
         RETURNING id, username, email, role, status, two_factor_enabled, created_at, updated_at",
    )
    .bind(username)
    .bind(&password_hash)
    .bind(&email_value)
    .bind(&role)
    .bind(provider)
    .fetch_one(conn)
    .await;

    match inserted {
        Ok((id, username, email, role, status, two_factor_enabled, created_at, updated_at)) => {
            foims_common::log_info!(
                "log.auth.external_user_created",
                username = username,
                provider = provider
            );
            Ok(ExternalUser {
                id,
                username,
                email,
                role,
                status,
                two_factor_enabled,
                created_at,
                updated_at,
            })
        }
        Err(e) => {
            // 邮箱已被其他账户占用：退化为占位邮箱重试，避免阻断登录
            if e.to_string().contains("users_email_key") {
                let placeholder = format!("{username}@{provider}.invalid");
                let (id, username, email, role, status, two_factor_enabled, created_at, updated_at) =
                    sqlx::query_as::<
                        sqlx::Postgres,
                        (
                            Uuid,
                            String,
                            String,
                            String,
                            bool,
                            bool,
                            DateTime<Utc>,
                            DateTime<Utc>,
                        ),
                    >(
                        "INSERT INTO users (username, password_hash, email, role, status, auth_provider)
                         VALUES ($1, $2, $3, $4, TRUE, $5)
                         RETURNING id, username, email, role, status, two_factor_enabled, created_at, updated_at",
                    )
                    .bind(username)
                    .bind(&password_hash)
                    .bind(&placeholder)
                    .bind(&role)
                    .bind(provider)
                    .fetch_one(conn)
                    .await?;
                foims_common::log_warn!(
                    "log.auth.external_email_conflict",
                    username = username,
                    email = placeholder
                );
                return Ok(ExternalUser {
                    id,
                    username,
                    email,
                    role,
                    status,
                    two_factor_enabled,
                    created_at,
                    updated_at,
                });
            }
            Err(e.into())
        }
    }
}

pub(crate) fn build_login_response(
    user: User,
    login_tokens: LoginTokens,
    secure: bool,
) -> Result<Response, AppError> {
    let access_cookie = create_auth_cookie(
        "access_token",
        &login_tokens.access_token,
        login_tokens.access_token_expiry as i64,
        secure,
    );
    let refresh_cookie = create_auth_cookie(
        "refresh_token",
        &login_tokens.refresh_token,
        login_tokens.refresh_token_expiry as i64,
        secure,
    );

    let mut response = (
        StatusCode::OK,
        Json(foims_common::ApiResponse::success(
            serde_json::json!({ "user": user, "expires_in": login_tokens.access_token_expiry }),
            msg("server.common.success"),
        )),
    )
        .into_response();

    append_cookie_to_response(&mut response, &access_cookie)?;
    append_cookie_to_response(&mut response, &refresh_cookie)?;

    Ok(response)
}

/// 校验外部登录（LDAP/SSO）账户的 TOTP 动态码。
///
/// 复用本地登录的 TOTP 构建与已用码占位逻辑（含并发重放防护）：
/// 返回 true 表示校验通过（已用码保持占用）；false 表示验证失败
///（占位已回滚，可用新码重试）。
pub(crate) async fn verify_external_totp(
    conn: &sqlx::PgPool,
    external: &ExternalUser,
    code: &str,
) -> Result<bool, AppError> {
    let encrypted_secret: Option<String> =
        sqlx::query_scalar("SELECT two_factor_secret FROM users WHERE id = $1")
            .bind(external.id)
            .fetch_optional(conn)
            .await?
            .flatten();
    let Some(encrypted_secret) = encrypted_secret else {
        // 已启用 2FA 开关但缺少密钥：按未验证处理（fail-closed）
        return Ok(false);
    };

    let secret = decrypt_password_async(encrypted_secret)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.auth.2fa_secret_decrypt_failed").with("error", e))
        })?;
    let secret = Secret::try_from_base32(&secret)
        .map_err(|_| AppError::Validation(msg("server.auth.2fa_secret_format_invalid")))?;
    let totp = Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(secret)
        .build()
        .map_err(|e| {
            AppError::Internal(msg("server.auth.2fa_secret_too_short").with("error", e))
        })?;

    // 原子占位后再校验，失败回滚，见 claim_totp_code 注释
    if !claim_totp_code(external.id, code) {
        return Ok(false);
    }
    let code_for_check = code.to_string();
    let valid = tokio::task::spawn_blocking(move || totp.check_current(&code_for_check).is_some())
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.auth.2fa_verify_task_failed").with("error", e))
        })?;
    if !valid {
        release_totp_code(external.id, code);
    }
    Ok(valid)
}

/// 外部登录启用 2FA 且请求未携带动态码时返回的响应。
///
/// 字段形态与本地登录 2FA 分支完全一致：`requires_two_factor: true` +
/// `username`，消息键 `server.common.success`；前端据此弹出动态码输入，
/// 携带 code 重试同一登录端点。
pub(crate) fn requires_two_factor_response(username: &str) -> Response {
    foims_common::ok_json(
        serde_json::json!({ "requires_two_factor": true, "username": username }),
        "server.common.success",
    )
}

/// 外部认证（LDAP/SSO）通过后的通用收尾：状态检查、签发令牌、记录登录日志。
///
/// `totp_verified` 表示该账户的 TOTP 动态码是否已通过校验：启用了 2FA 的
/// 外部账户在未通过校验时拒绝签发（兜底拦截，防止新增外部路径遗漏校验）。
pub(crate) async fn issue_external_login_tokens<P: AuthProvider>(
    state: &Arc<P>,
    meta: &RequestMeta,
    external: &ExternalUser,
    remember_me: bool,
    totp_verified: bool,
) -> Result<LoginTokens, AppError> {
    let conn = state.pool()?.get_conn();

    if !external.status {
        return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
    }

    if external.two_factor_enabled && !totp_verified {
        return Err(AppError::Unauthorized(msg(
            "server.auth.two_factor_required",
        )));
    }

    let device_fingerprint =
        JwtUtils::generate_device_fingerprint(&meta.user_agent, &meta.ip_address);
    let login_tokens = generate_login_tokens(
        state.jwt_utils(),
        &external.id,
        &external.username,
        &external.role,
        &device_fingerprint,
        &meta.ip_address,
        remember_me,
    )?;

    if let Err(e) = log_login(
        &conn,
        &external.username,
        &meta.ip_address,
        &meta.user_agent,
        true,
        None,
    )
    .await
    {
        foims_common::log_warn!("log.login.record_failed", error = e);
    }
    crate::app_fail2ban::record_login_success(&meta.ip_address, &external.username);
    foims_common::log_info!("log.login.success", username = external.username);

    Ok(login_tokens)
}

pub(crate) fn generate_login_tokens(
    jwt_utils: &JwtUtils,
    id: &Uuid,
    username: &str,
    role: &str,
    device_fingerprint: &str,
    ip_address: &str,
    remember_me: bool,
) -> Result<LoginTokens, AppError> {
    let access_token = jwt_utils
        .generate_access_token(
            id,
            username,
            role,
            Some(device_fingerprint),
            Some(ip_address),
        )
        .map_err(|e| {
            AppError::Internal(msg("server.auth.token_generate_failed").with("error", e))
        })?;
    let refresh_token = jwt_utils
        .generate_refresh_token(
            id,
            username,
            role,
            Some(device_fingerprint),
            Some(ip_address),
            remember_me,
        )
        .map_err(|e| {
            AppError::Internal(msg("server.auth.token_generate_failed").with("error", e))
        })?;
    let access_token_expiry = jwt_utils.get_access_token_expiry();
    let refresh_token_expiry = jwt_utils.get_actual_refresh_token_expiry(remember_me);

    Ok(LoginTokens {
        access_token,
        refresh_token,
        access_token_expiry,
        refresh_token_expiry,
    })
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

/// 进程级 dummy bcrypt 哈希（首次使用时生成，cost 与真实口令一致）
static DUMMY_BCRYPT_HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// 运行时哈希生成失败时使用的静态兜底（"foims-dummy-password" 的合法
/// bcrypt 哈希，cost 12 与 DEFAULT_COST 一致），保证缓解不会静默失效
const DUMMY_BCRYPT_FALLBACK: &str = "$2b$12$NUV.EviGW4zymnRdLE45LO60.7WNTwztAaUGRdQMKrKQmHRa6Ccxq";

/// 用户不存在时对提交口令执行一次等价 bcrypt 校验：
/// 消除「用户不存在立即返回、用户存在时 bcrypt 校验约数百毫秒」的
/// 用户名枚举时间侧信道（security-review A-4）
async fn dummy_bcrypt_verify(password: &str) {
    let hash = DUMMY_BCRYPT_HASH.get_or_init(|| {
        bcrypt::hash("foims-dummy-password", bcrypt::DEFAULT_COST)
            .unwrap_or_else(|_| DUMMY_BCRYPT_FALLBACK.to_string())
    });
    let password = password.to_string();
    let hash = hash.clone();
    // 校验结果本身无业务意义（诱饵哈希），仅对任务崩溃留痕
    if let Err(e) = tokio::task::spawn_blocking(move || verify(&password, &hash)).await {
        foims_common::log_warn!("log.auth.dummy_verify_task_failed", error = e);
    }
}

pub(crate) async fn log_login(
    pool: &sqlx::PgPool,
    username: &str,
    ip_address: &str,
    user_agent: &str,
    success: bool,
    error_message: Option<&str>,
) -> Result<(), sqlx::Error> {
    // login_logs.username 列宽 VARCHAR(50)：邮箱验证码登录以完整邮箱作为
    // 标识写入，超长邮箱直写会报错丢日志——按字符截断对齐列宽
    let username = if username.chars().count() > 50 {
        username.chars().take(50).collect::<String>()
    } else {
        username.to_string()
    };
    sqlx::query(
        "INSERT INTO login_logs (id, username, ip_address, user_agent, success, error_message, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(Uuid::new_v4())
    .bind(username.clone())
    .bind(ip_address)
    .bind(user_agent)
    .bind(success)
    .bind(error_message)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    // 审计外发（syslog）：旁路尽力而为，未启用时内部直接跳过
    let forward_message = format!(
        "login user={username} ip={ip_address} success={success} error={}",
        error_message.unwrap_or("-")
    );
    crate::meta::forward_op_log(pool.clone(), forward_message);

    Ok(())
}

pub(crate) fn create_auth_cookie(
    name: &str,
    value: &str,
    max_age: i64,
    secure: bool,
) -> Cookie<'static> {
    let mut cookie = Cookie::build((name.to_string(), value.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(max_age))
        .build();
    if secure {
        cookie.set_secure(true);
    }
    cookie
}

fn create_clear_cookie(name: &str, secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::build((name.to_string(), String::new()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(0))
        .build();
    if secure {
        cookie.set_secure(true);
    }
    cookie
}

pub(crate) fn append_cookie_to_response(
    response: &mut Response,
    cookie: &Cookie,
) -> Result<(), AppError> {
    let header_value = HeaderValue::from_str(&cookie.to_string())
        .map_err(|e| AppError::Internal(msg("server.common.cookie_invalid").with("error", e)))?;
    response.headers_mut().append(SET_COOKIE, header_value);
    Ok(())
}
