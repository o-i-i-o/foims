//! 登录认证：密码/2FA/邮箱验证码登录、令牌签发刷新与认证中间件。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Request, State};
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, SameSite};
use bcrypt::verify;
use chrono::{DateTime, Utc};
use jsonwebtoken::errors::ErrorKind;
use rand::RngExt;
use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::auth::extractor::{AccessToken, RefreshToken, SecureFlag};
use crate::auth::utils::{
    JwtUtils, extract_token_from_parts, get_client_info_from_parts, hash_password,
};
use crate::crypto::{decrypt_password_async, encrypt_password_async};
use crate::error::AppError;
use crate::models::{
    EmailLoginRequest, ForgotPasswordRequest, ResetPasswordRequest, SendLoginCodeRequest,
    SendTwoFactorCodeRequest, TwoFactorLoginRequest, User, UserLogin,
};
use crate::routes::static_files::AppJson;
use crate::utils::common::RequestMeta;
use ipma_common::msg;
use totp_rs::{Algorithm, Builder, Secret};

type TotpReplayStore =
    std::sync::Mutex<std::collections::HashMap<Uuid, (String, std::time::Instant)>>;
static TOTP_REPLAY_STORE: std::sync::OnceLock<TotpReplayStore> = std::sync::OnceLock::new();

fn totp_replay_store() -> &'static TotpReplayStore {
    TOTP_REPLAY_STORE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

const TOTP_REPLAY_TTL: std::time::Duration = std::time::Duration::from_secs(90);

fn is_totp_code_replayed(user_id: Uuid, code: &str) -> bool {
    let store = totp_replay_store();
    if let Ok(map) = store.lock()
        && let Some((used_code, used_at)) = map.get(&user_id)
        && used_code == code
        && used_at.elapsed() < TOTP_REPLAY_TTL
    {
        return true;
    }
    false
}

fn record_totp_usage(user_id: Uuid, code: &str) {
    let store = totp_replay_store();
    if let Ok(mut map) = store.lock() {
        map.retain(|_, (_, used_at)| used_at.elapsed() < TOTP_REPLAY_TTL);
        map.insert(user_id, (code.to_string(), std::time::Instant::now()));
    }
}

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let (mut parts, body) = req.into_parts();

    let Some(token) = extract_token_from_parts(&parts) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ipma_common::ApiResponse::<()>::error(msg(
                "server.auth.auth_failed",
            ))),
        )
            .into_response();
    };

    let claims = match state.jwt_utils.validate_token(&token) {
        Ok(claims) => claims,
        Err(err) => {
            let error_key = match err.kind() {
                ErrorKind::ExpiredSignature => "server.auth.token_expired",
                _ => "server.auth.invalid_token",
            };
            return (
                StatusCode::UNAUTHORIZED,
                Json(ipma_common::ApiResponse::<()>::error(msg(error_key))),
            )
                .into_response();
        }
    };

    if claims.token_type != "access" {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ipma_common::ApiResponse::<()>::error(msg(
                "server.auth.invalid_token",
            ))),
        )
            .into_response();
    }

    // 检查令牌是否已被撤销
    if let Ok(pool) = state.pool()
        && let Ok(revoked) = crate::utils::common::is_token_revoked(&pool.get_conn(), &token).await
        && revoked
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ipma_common::ApiResponse::<()>::error(msg(
                "server.auth.token_revoked",
            ))),
        )
            .into_response();
    }

    let (ip_address, user_agent) = get_client_info_from_parts(&parts);
    let current_fingerprint = JwtUtils::generate_device_fingerprint(&user_agent, &ip_address);

    if let Some(token_fingerprint) = &claims.device_fingerprint
        && token_fingerprint != &current_fingerprint
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ipma_common::ApiResponse::<()>::error(msg(
                "server.auth.device_validation_failed",
            ))),
        )
            .into_response();
    }

    parts.extensions.insert(claims);

    let req = Request::from_parts(parts, body);
    next.run(req).await
}

pub async fn localhost_only_middleware(req: Request, next: Next) -> Response {
    let (parts, body) = req.into_parts();

    // UDS 场景无 peer IP，需依赖反代传入的来源 IP 头判断是否本地访问。
    // 安全要点：
    //   1. 只信任 X-Real-IP —— nginx 用 `proxy_set_header X-Real-IP $remote_addr;`
    //      覆盖式设置，客户端无法伪造（$remote_addr 取自 TCP 对端）。
    //      切勿使用 X-Forwarded-For：其经 `$proxy_add_x_forwarded_for` 会保留客户端
    //      伪造的首段值（如 "127.0.0.1, <真实IP>"），而取首段判断即可被绕过。
    //   2. fail-close：缺失可信来源 IP 头时默认拒绝。
    let is_localhost = parts
        .headers
        .get("X-Real-IP")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<std::net::IpAddr>().ok())
        .map(|addr| addr.is_loopback())
        .unwrap_or(false);

    if !is_localhost {
        return (
            StatusCode::FORBIDDEN,
            Json(ipma_common::ApiResponse::<()>::error(msg(
                "server.auth.access_denied",
            ))),
        )
            .into_response();
    }

    let req = Request::from_parts(parts, body);
    next.run(req).await
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<UserLogin>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    req.validate()?;

    // 应用层 fail2ban: 检查 IP 是否被封禁
    let client_ip = meta.ip_address.clone();
    if crate::system::app_fail2ban::is_ip_banned(&client_ip) {
        let remaining = crate::system::app_fail2ban::get_ban_remaining(&client_ip);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }

    let user_row = match sqlx::query_as::<
        sqlx::Postgres,
        (Uuid, String, String, String, String, bool, bool),
    >(
        "SELECT id, username, password_hash, email, role, status, two_factor_enabled FROM users WHERE username = $1 OR email = $1",
    )
    .bind(&req.username)
    .fetch_optional(&conn)
    .await?
    {
        Some(row) => row,
        None => {
            crate::system::app_fail2ban::record_login_failure(
                &client_ip,
                &req.username,
                "server.login_log.user_not_found",
            );
            if let Err(e) = log_login(&conn, &req.username, &meta.ip_address, &meta.user_agent, false, Some("server.login_log.user_not_found")).await {
                ipma_common::log_warn!("log.login.record_failed", error = e);
            }
            return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
        }
    };

    let (id, username, password_hash, email, role, status, two_factor_enabled) = user_row;

    if !status {
        crate::system::app_fail2ban::record_login_failure(
            &client_ip,
            &username,
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
            ipma_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
    }

    let password_for_verify = req.password.clone();
    let hash_for_verify = password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || verify(&password_for_verify, &hash_for_verify))
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.auth.password_verify_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.auth.password_verify_failed").with("error", e))
        })?;
    if !valid {
        crate::system::app_fail2ban::record_login_failure(
            &client_ip,
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
            ipma_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
    }

    if two_factor_enabled {
        return Ok(crate::error::ok_json(
            serde_json::json!({ "requires_two_factor": true, "username": username }),
            "server.common.success",
        ));
    }

    let jwt_utils = &state.jwt_utils;
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
        created_at: Utc::now(),
        updated_at: Utc::now(),
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
        ipma_common::log_warn!("log.login.record_failed", error = e);
    }
    crate::system::app_fail2ban::record_login_success(&client_ip, &username);
    ipma_common::log_info!("log.login.success", username = username);

    build_login_response(user, login_tokens, meta.is_secure)
}

pub async fn login_with_email_code(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<EmailLoginRequest>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    req.validate()?;
    let email = req.email.trim();

    let user_row = match sqlx::query_as::<
        sqlx::Postgres,
        (Uuid, String, String, String, bool, bool, Option<String>, Option<DateTime<Utc>>),
    >(
        "SELECT id, username, email, role, status, two_factor_enabled, two_factor_email_code, two_factor_email_code_expiry FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(&conn)
    .await?
    {
        Some(row) => row,
        None => {
            if let Err(e) = log_login(&conn, email, &meta.ip_address, &meta.user_agent, false, Some("server.login_log.user_not_found")).await {
                ipma_common::log_warn!("log.login.record_failed", error = e);
            }
            return Err(AppError::Unauthorized(msg("server.auth.email_or_code_invalid")));
        }
    };

    let (id, username, email, role, status, two_factor_enabled, code, expiry) = user_row;

    if !status {
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
            ipma_common::log_warn!("log.login.record_failed", error = e);
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
            verified = true;
            if let Err(e) = sqlx::query("UPDATE users SET two_factor_email_code = NULL, two_factor_email_code_expiry = NULL WHERE id = $1")
                .bind(id).execute(&conn).await
            {
                ipma_common::log_warn!("log.auth.clear_2fa_code_failed", error = e);
            }
        }
    }

    if !verified {
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
            ipma_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg(
            "server.auth.code_invalid_or_expired",
        )));
    }

    if two_factor_enabled {
        return Ok(crate::error::ok_json(
            serde_json::json!({ "requires_two_factor": true, "username": username }),
            "server.common.success",
        ));
    }

    let jwt_utils = &state.jwt_utils;
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
        created_at: Utc::now(),
        updated_at: Utc::now(),
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
        ipma_common::log_warn!("log.login.record_failed", error = e);
    }

    build_login_response(user, login_tokens, meta.is_secure)
}

pub async fn send_login_code(
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<SendLoginCodeRequest>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();
    let email = req.email.trim();

    req.validate()?;

    let user_row = match sqlx::query_as::<sqlx::Postgres, (Uuid, String, bool)>(
        "SELECT id, username, status FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(&conn)
    .await?
    {
        Some(row) => row,
        None => {
            return Ok(crate::error::ok_json((), "server.auth.code_sent"));
        }
    };

    let (id, _username, status) = user_row;

    if !status {
        return Ok(crate::error::ok_json((), "server.auth.code_sent"));
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
    crate::system::smtp::send_email_async(&conn, email, "登录验证码", &email_body).await?;

    Ok(crate::error::ok_json((), "server.auth.code_sent"))
}

pub async fn login_with_two_factor(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<TwoFactorLoginRequest>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    // 应用层 fail2ban: 检查 IP 是否被封禁
    let client_ip = meta.ip_address.clone();
    if crate::system::app_fail2ban::is_ip_banned(&client_ip) {
        let remaining = crate::system::app_fail2ban::get_ban_remaining(&client_ip);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }

    let user_row = match sqlx::query_as::<
        sqlx::Postgres,
        (Uuid, String, String, String, String, bool, bool, Option<String>),
    >(
        "SELECT id, username, password_hash, email, role, status, two_factor_enabled, two_factor_secret FROM users WHERE username = $1",
    )
    .bind(&req.username)
    .fetch_optional(&conn)
    .await?
    {
        Some(row) => row,
        None => {
            crate::system::app_fail2ban::record_login_failure(
                &client_ip,
                &req.username,
                "server.login_log.user_not_found",
            );
            return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
        }
    };

    let (id, username, password_hash, email, role, status, two_factor_enabled, secret) = user_row;

    let password = req
        .password
        .as_deref()
        .ok_or_else(|| AppError::Validation(msg("server.auth.2fa_password_required")))?;
    let password_for_verify = password.to_string();
    let hash_for_verify = password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || verify(&password_for_verify, &hash_for_verify))
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.auth.password_verify_task_failed").with("error", e))
        })?
        .map_err(|e| {
            AppError::Internal(msg("server.auth.password_verify_failed").with("error", e))
        })?;
    if !valid {
        crate::system::app_fail2ban::record_login_failure(
            &client_ip,
            &username,
            "server.login_log.invalid_password",
        );
        return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
    }

    if !status {
        crate::system::app_fail2ban::record_login_failure(
            &client_ip,
            &username,
            "server.login_log.account_disabled",
        );
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
                    ipma_common::log_warn!("log.login.record_failed", error = e);
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
                let code = req.two_factor_code.clone();
                if is_totp_code_replayed(id, &code) {
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
                        ipma_common::log_warn!("log.login.record_failed", error = e);
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
                    record_totp_usage(id, &code);
                    verified = true;
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
                    ipma_common::log_warn!("log.login.record_failed", error = e);
                }
                return Err(AppError::Internal(
                    msg("server.auth.2fa_secret_too_short").with("error", e),
                ));
            }
        }
    }

    if !verified {
        crate::system::app_fail2ban::record_login_failure(
            &client_ip,
            &username,
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
            ipma_common::log_warn!("log.login.record_failed", error = e);
        }
        return Err(AppError::Unauthorized(msg("server.auth.code_invalid")));
    }

    let jwt_utils = &state.jwt_utils;
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
        created_at: Utc::now(),
        updated_at: Utc::now(),
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
        ipma_common::log_warn!("log.login.record_failed", error = e);
    }
    crate::system::app_fail2ban::record_login_success(&client_ip, &user.username);
    ipma_common::log_info!("log.login.2fa_success", username = user.username);

    build_login_response(user, login_tokens, meta.is_secure)
}

pub async fn send_two_factor_code(
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<SendTwoFactorCodeRequest>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let Some(user) = sqlx::query_as::<sqlx::Postgres, (Uuid, String, String)>(
        "SELECT id, username, email FROM users WHERE username = $1",
    )
    .bind(&req.username)
    .fetch_optional(&conn)
    .await?
    else {
        return Ok(crate::error::ok_json((), "server.auth.send_ok"));
    };

    let code: String = {
        let mut rng = rand::rng();
        (0..6)
            .map(|_| rng.random_range(0..10).to_string())
            .collect()
    };
    let expiry = Utc::now() + chrono::Duration::minutes(5);
    sqlx::query("UPDATE users SET two_factor_email_code = $1, two_factor_email_code_expiry = $2 WHERE id = $3")
        .bind(&code).bind(expiry).bind(user.0).execute(&conn).await?;

    let email_body = format!("您的两步验证码是：{code}");
    crate::system::smtp::send_email_async(&conn, &user.2, "两步验证码", &email_body).await?;

    Ok(crate::error::ok_json((), "server.auth.code_sent"))
}

pub async fn logout(
    State(state): State<Arc<AppState>>,
    AccessToken(access_token): AccessToken,
    RefreshToken(refresh_token): RefreshToken,
    SecureFlag(secure): SecureFlag,
) -> Result<Response, AppError> {
    // 撤销 access_token
    if let Some(access_token) = access_token
        && let Ok(claims) = state.jwt_utils.validate_token(&access_token)
    {
        let user_id = Uuid::parse_str(&claims.sub).ok();
        let expiry = chrono::DateTime::from_timestamp(claims.exp as i64, 0)
            .unwrap_or(chrono::Utc::now() + chrono::Duration::hours(1));
        if let Err(e) = crate::utils::common::revoke_token(
            &state.pool()?.get_conn(),
            &access_token,
            user_id,
            expiry,
        )
        .await
        {
            ipma_common::log_warn!("log.auth.revoke_access_token_failed", error = e);
        }
    }

    // 撤销 refresh_token
    if let Some(refresh_token) = refresh_token
        && let Ok(claims) = state.jwt_utils.validate_token(&refresh_token)
    {
        let user_id = Uuid::parse_str(&claims.sub).ok();
        let expiry = chrono::DateTime::from_timestamp(claims.exp as i64, 0)
            .unwrap_or(chrono::Utc::now() + chrono::Duration::days(7));
        if let Err(e) = crate::utils::common::revoke_token(
            &state.pool()?.get_conn(),
            &refresh_token,
            user_id,
            expiry,
        )
        .await
        {
            ipma_common::log_warn!("log.auth.revoke_refresh_token_failed", error = e);
        }
    }

    let access_cookie = create_clear_cookie("access_token", secure);
    let refresh_cookie = create_clear_cookie("refresh_token", secure);

    let mut response = crate::error::ok_json((), "server.common.success");
    append_cookie_to_response(&mut response, &access_cookie)?;
    append_cookie_to_response(&mut response, &refresh_cookie)?;
    Ok(response)
}

pub async fn refresh_token(
    State(state): State<Arc<AppState>>,
    RefreshToken(token): RefreshToken,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let token = match token {
        Some(t) => t,
        None => return Err(AppError::Unauthorized(msg("server.auth.auth_failed"))),
    };

    let jwt_utils = &state.jwt_utils;

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
            ipma_common::log_error!("log.auth.check_revoke_failed", error = e);
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
            // 令牌签发时间早于吊销时间点 → 已被吊销
            if (claims.iat as i64) < invalidated_at.timestamp() {
                return Err(AppError::Unauthorized(msg(
                    "server.auth.token_invalidated_relogin",
                )));
            }
            role
        }
        None => return Err(AppError::Unauthorized(msg("server.auth.account_not_found"))),
    };

    let token_expiry =
        chrono::DateTime::from_timestamp(claims.exp as i64, 0).unwrap_or_else(Utc::now);
    if let Err(e) = crate::utils::revoke_token(&conn, &token, Some(user_id), token_expiry).await {
        ipma_common::log_error!("log.auth.revoke_token_failed", error = e);
    }

    let token_duration = claims.exp.saturating_sub(claims.iat);
    let remember_me = token_duration > 86400;

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

    ipma_common::log_debug!("log.login.token_refreshed", username = claims.username);

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

    let mut response = crate::error::ok_json(
        serde_json::json!({ "expires_in": access_token_expiry, "remember_me": remember_me }),
        "server.common.success",
    );
    append_cookie_to_response(&mut response, &access_cookie)?;
    append_cookie_to_response(&mut response, &refresh_cookie)?;
    Ok(response)
}

pub async fn get_current_user(
    auth: crate::auth::extractor::AuthUser,
) -> Result<Response, AppError> {
    Ok(crate::error::ok_json(
        serde_json::json!({ "id": auth.sub, "username": auth.username, "role": auth.role }),
        "server.common.success",
    ))
}

pub async fn forgot_password(
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<ForgotPasswordRequest>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    req.validate()?;
    let email = req.email.trim();

    let user_result = sqlx::query_as::<_, (Uuid, String, bool)>(
        "SELECT id, username, two_factor_enabled FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(&conn)
    .await;

    if let Ok(Some((_user_id, _username, _two_factor_enabled))) = user_result {
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
            ipma_common::log_error!("log.auth.save_reset_token_failed", error = e);
        } else {
            let smtp_config = crate::system::smtp::get_smtp_config_from_db(&conn).await;
            if let Some(ref _config) = smtp_config {
                // 使用应用公共URL（而非SMTP主机名）构建重置链接
                let base_url = state.config.server.public_url.trim_end_matches('/');
                let reset_link = format!("{base_url}/reset-password?token={reset_token}");
                let email_body = format!("请点击以下链接重置密码：{reset_link}");
                if let Err(e) =
                    crate::system::smtp::send_email_async(&conn, email, "密码重置", &email_body)
                        .await
                {
                    ipma_common::log_error!("log.auth.send_reset_email_failed", error = e);
                }
            }
        }
    }

    Ok(crate::error::ok_json(
        (),
        "server.auth.forgot_password_sent",
    ))
}

pub async fn reset_password(
    State(state): State<Arc<AppState>>,
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
            let hashed_password = hash_password(&req.new_password).await?;

            sqlx::query(
                "UPDATE users SET password_hash = $1, reset_token = NULL, reset_token_expiry = NULL, tokens_invalidated_at = NOW() WHERE id = $2",
            )
            .bind(&hashed_password)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            Ok(crate::error::ok_json(
                (),
                "server.auth.reset_password_success",
            ))
        }
        None => Err(AppError::Validation(msg("server.auth.reset_link_invalid"))),
    }
}

pub async fn init_two_factor(
    State(state): State<Arc<AppState>>,
    auth: crate::auth::extractor::AuthUser,
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
        .with_issuer(Some("IPMA"))
        .with_account_name(target_username.clone())
        .build()
        .map_err(|e| {
            AppError::Internal(msg("server.auth.totp_generate_failed").with("error", e))
        })?;

    let encrypted_secret = encrypt_password_async(secret_base32).await?;
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
        tokio::task::spawn_blocking(move || totp_for_qr.to_qr_base64().unwrap_or_default())
            .await
            .map_err(|e| {
                AppError::Internal(msg("server.auth.qr_generate_task_failed").with("error", e))
            })?;

    Ok(crate::error::ok_json(
        serde_json::json!({
            "otpauth_url": otpauth_url,
            "qr_code_base64": qr_code_base64,
        }),
        "server.auth.2fa_init_success",
    ))
}

pub async fn enable_two_factor(
    State(state): State<Arc<AppState>>,
    auth: crate::auth::extractor::AuthUser,
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
        .with_issuer(Some("IPMA"))
        .with_account_name(target_username)
        .build()
        .map_err(|e| AppError::Internal(msg("server.auth.totp_create_failed").with("error", e)))?;

    let code = req.code.clone();
    if is_totp_code_replayed(target_user_id, &code) {
        return Err(AppError::Validation(msg("server.auth.2fa_code_reused")));
    }
    let code_for_check = code.clone();
    let valid = tokio::task::spawn_blocking(move || totp.check_current(&code_for_check).is_some())
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.auth.2fa_verify_task_failed").with("error", e))
        })?;
    if !valid {
        return Err(AppError::Validation(msg("server.auth.code_wrong")));
    }
    record_totp_usage(target_user_id, &code);

    sqlx::query(
        "UPDATE users SET two_factor_enabled = true, two_factor_verified = true WHERE id = $1",
    )
    .bind(target_user_id)
    .execute(&conn)
    .await?;

    Ok(crate::error::ok_json((), "server.auth.2fa_enabled"))
}

pub async fn disable_two_factor(
    State(state): State<Arc<AppState>>,
    auth: crate::auth::extractor::AuthUser,
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
            .with_issuer(Some("IPMA"))
            .with_account_name(target_username)
            .build()
            .map_err(|e| {
                AppError::Internal(msg("server.auth.2fa_secret_too_short").with("error", e))
            })?;
        let code = req.code.clone();
        if is_totp_code_replayed(target_user_id, &code) {
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
            record_totp_usage(target_user_id, &code);
            verified = true;
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

    Ok(crate::error::ok_json((), "server.auth.2fa_disabled"))
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

struct LoginTokens {
    access_token: String,
    refresh_token: String,
    access_token_expiry: u64,
    refresh_token_expiry: u64,
}

fn build_login_response(
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
        Json(ipma_common::ApiResponse::success(
            serde_json::json!({ "user": user, "expires_in": login_tokens.access_token_expiry }),
            msg("server.common.success"),
        )),
    )
        .into_response();

    append_cookie_to_response(&mut response, &access_cookie)?;
    append_cookie_to_response(&mut response, &refresh_cookie)?;

    Ok(response)
}

fn generate_login_tokens(
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

async fn log_login(
    pool: &sqlx::PgPool,
    username: &str,
    ip_address: &str,
    user_agent: &str,
    success: bool,
    error_message: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO login_logs (id, username, ip_address, user_agent, success, error_message, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(Uuid::new_v4())
    .bind(username)
    .bind(ip_address)
    .bind(user_agent)
    .bind(success)
    .bind(error_message)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(())
}

fn create_auth_cookie(name: &str, value: &str, max_age: i64, secure: bool) -> Cookie<'static> {
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

fn append_cookie_to_response(response: &mut Response, cookie: &Cookie) -> Result<(), AppError> {
    let header_value = HeaderValue::from_str(&cookie.to_string())
        .map_err(|e| AppError::Internal(msg("server.common.cookie_invalid").with("error", e)))?;
    response.headers_mut().append(SET_COOKIE, header_value);
    Ok(())
}
