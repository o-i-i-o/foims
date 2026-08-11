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
    ApiResponse, EmailLoginRequest, ForgotPasswordRequest, ResetPasswordRequest,
    SendLoginCodeRequest, SendTwoFactorCodeRequest, TwoFactorLoginRequest, User, UserLogin,
};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, detect_user_language_from_parts};
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
        let user_lang = detect_user_language_from_parts(&parts);
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::error_i18n("api.auth_failed", &user_lang)),
        )
            .into_response();
    };

    let claims = match state.jwt_utils.validate_token(&token) {
        Ok(claims) => claims,
        Err(err) => {
            let user_lang = detect_user_language_from_parts(&parts);
            let error_msg = match err.kind() {
                ErrorKind::ExpiredSignature => "api.token_expired",
                _ => "api.invalid_token",
            };
            return (
                StatusCode::UNAUTHORIZED,
                Json(ApiResponse::<()>::error_i18n(error_msg, &user_lang)),
            )
                .into_response();
        }
    };

    if claims.token_type != "access" {
        let user_lang = detect_user_language_from_parts(&parts);
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::error_i18n(
                "api.invalid_token",
                &user_lang,
            )),
        )
            .into_response();
    }

    // 检查令牌是否已被撤销
    if let Ok(pool) = state.pool()
        && let Ok(revoked) = crate::utils::common::is_token_revoked(&pool.get_conn(), &token).await
        && revoked
    {
        let user_lang = detect_user_language_from_parts(&parts);
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::error_i18n(
                "api.token_revoked",
                &user_lang,
            )),
        )
            .into_response();
    }

    let (ip_address, user_agent) = get_client_info_from_parts(&parts);
    let current_fingerprint = JwtUtils::generate_device_fingerprint(&user_agent, &ip_address);

    if let Some(token_fingerprint) = &claims.device_fingerprint
        && token_fingerprint != &current_fingerprint
    {
        let user_lang = detect_user_language_from_parts(&parts);
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::error_i18n(
                "api.device_validation_failed",
                &user_lang,
            )),
        )
            .into_response();
    }

    parts.extensions.insert(claims);

    let req = Request::from_parts(parts, body);
    next.run(req).await
}

pub async fn localhost_only_middleware(req: Request, next: Next) -> Response {
    let (parts, body) = req.into_parts();

    // UDS 场景无 peer IP，检查 X-Forwarded-For 是否为 loopback；无则视为本地（UDS 仅本地访问）
    let is_localhost = if let Some(xff) = parts.headers.get("X-Forwarded-For")
        && let Ok(xff_str) = xff.to_str()
        && let Some(first_ip) = xff_str.split(',').next()
    {
        first_ip
            .trim()
            .parse::<std::net::IpAddr>()
            .map(|addr| addr.is_loopback())
            .unwrap_or(false)
    } else {
        true
    };

    if !is_localhost {
        let user_lang = detect_user_language_from_parts(&parts);
        return (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::<()>::error_i18n(
                "api.access_denied",
                &user_lang,
            )),
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
    let user_lang = meta.user_lang.clone();
    let conn = state.pool()?.get_conn();

    req.validate()?;

    // 应用层 fail2ban: 检查 IP 是否被封禁
    let client_ip = meta.ip_address.clone();
    if crate::system::app_fail2ban::is_ip_banned(&client_ip) {
        let remaining = crate::system::app_fail2ban::get_ban_remaining(&client_ip);
        return Err(AppError::Forbidden(format!(
            "登录尝试过于频繁，IP 已被封禁，请 {remaining} 秒后重试"
        )));
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
            crate::system::app_fail2ban::record_login_failure(&client_ip, &req.username, "用户未找到");
            if let Err(e) = log_login(&conn, &req.username, &meta.ip_address, &meta.user_agent, false, Some("用户未找到")).await {
                tracing::warn!("记录登录日志失败: {}", e);
            }
            return Err(AppError::Unauthorized("登录失败".to_string()));
        }
    };

    let (id, username, password_hash, email, role, status, two_factor_enabled) = user_row;

    if !status {
        crate::system::app_fail2ban::record_login_failure(
            &client_ip,
            &username,
            "Account disabled",
        );
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("Account disabled"),
        )
        .await
        {
            tracing::warn!("记录登录日志失败: {}", e);
        }
        return Err(AppError::Unauthorized("登录失败".to_string()));
    }

    let password_for_verify = req.password.clone();
    let hash_for_verify = password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || verify(&password_for_verify, &hash_for_verify))
        .await
        .map_err(|e| AppError::Internal(format!("密码验证任务失败: {e}")))?
        .map_err(|e| AppError::Internal(e.to_string()))?;
    if !valid {
        crate::system::app_fail2ban::record_login_failure(
            &client_ip,
            &username,
            "Invalid password",
        );
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("Invalid password"),
        )
        .await
        {
            tracing::warn!("记录登录日志失败: {}", e);
        }
        return Err(AppError::Unauthorized("登录失败".to_string()));
    }

    if two_factor_enabled {
        return Ok(crate::error::ok_json(
            serde_json::json!({ "requires_two_factor": true, "username": username }),
            "api.success",
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
        tracing::warn!("记录登录日志失败: {}", e);
    }
    crate::system::app_fail2ban::record_login_success(&client_ip, &username);
    tracing::info!("用户 {} 登录成功", username);

    build_login_response(user, login_tokens, &user_lang, meta.is_secure)
}

pub async fn login_with_email_code(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<EmailLoginRequest>,
) -> Result<Response, AppError> {
    let user_lang = meta.user_lang.clone();
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
            if let Err(e) = log_login(&conn, email, &meta.ip_address, &meta.user_agent, false, Some("用户未找到")).await {
                tracing::warn!("记录登录日志失败: {}", e);
            }
            return Err(AppError::Unauthorized("邮箱或验证码无效".to_string()));
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
            Some("Account disabled"),
        )
        .await
        {
            tracing::warn!("记录登录日志失败: {}", e);
        }
        return Err(AppError::Unauthorized("邮箱或验证码无效".to_string()));
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
                tracing::warn!("清除2FA邮箱验证码失败: {}", e);
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
            Some("Invalid email code"),
        )
        .await
        {
            tracing::warn!("记录登录日志失败: {}", e);
        }
        return Err(AppError::Unauthorized("验证码无效或已过期".to_string()));
    }

    if two_factor_enabled {
        return Ok(crate::error::ok_json(
            serde_json::json!({ "requires_two_factor": true, "username": username }),
            "api.success",
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
        tracing::warn!("记录登录日志失败: {}", e);
    }

    build_login_response(user, login_tokens, &user_lang, meta.is_secure)
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
            return Ok(crate::error::ok_json((), "验证码已发送"));
        }
    };

    let (id, _username, status) = user_row;

    if !status {
        return Ok(crate::error::ok_json((), "验证码已发送"));
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

    Ok(crate::error::ok_json((), "验证码已发送"))
}

pub async fn login_with_two_factor(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<TwoFactorLoginRequest>,
) -> Result<Response, AppError> {
    let user_lang = meta.user_lang.clone();
    let conn = state.pool()?.get_conn();

    // 应用层 fail2ban: 检查 IP 是否被封禁
    let client_ip = meta.ip_address.clone();
    if crate::system::app_fail2ban::is_ip_banned(&client_ip) {
        let remaining = crate::system::app_fail2ban::get_ban_remaining(&client_ip);
        return Err(AppError::Forbidden(format!(
            "登录尝试过于频繁，IP 已被封禁，请 {remaining} 秒后重试"
        )));
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
            crate::system::app_fail2ban::record_login_failure(&client_ip, &req.username, "用户未找到");
            return Err(AppError::Unauthorized("登录失败".to_string()));
        }
    };

    let (id, username, password_hash, email, role, status, two_factor_enabled, secret) = user_row;

    let password = req
        .password
        .as_deref()
        .ok_or_else(|| AppError::Validation("2FA登录必须提供密码".to_string()))?;
    let password_for_verify = password.to_string();
    let hash_for_verify = password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || verify(&password_for_verify, &hash_for_verify))
        .await
        .map_err(|e| AppError::Internal(format!("密码验证任务失败: {e}")))?
        .map_err(|e| AppError::Internal(e.to_string()))?;
    if !valid {
        crate::system::app_fail2ban::record_login_failure(
            &client_ip,
            &username,
            "Invalid password",
        );
        return Err(AppError::Unauthorized("登录失败".to_string()));
    }

    if !status {
        crate::system::app_fail2ban::record_login_failure(
            &client_ip,
            &username,
            "Account disabled",
        );
        return Err(AppError::Unauthorized("账户已禁用".to_string()));
    }

    if !two_factor_enabled {
        return Err(AppError::Validation("2FA not enabled".to_string()));
    }

    let mut verified = false;
    if let Some(encrypted_secret) = secret {
        let secret = decrypt_password_async(encrypted_secret)
            .await
            .map_err(|e| AppError::Internal(format!("2FA密钥解密失败: {e}")))?;
        let secret = match Secret::try_from_base32(&secret) {
            Ok(s) => s,
            Err(e) => {
                if let Err(e) = log_login(
                    &conn,
                    &username,
                    &meta.ip_address,
                    &meta.user_agent,
                    false,
                    Some("Invalid 2FA secret format"),
                )
                .await
                {
                    tracing::warn!("记录登录日志失败: {}", e);
                }
                return Err(AppError::Internal(format!("2FA密钥格式错误: {e}")));
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
                        Some("TOTP code replayed"),
                    )
                    .await
                    {
                        tracing::warn!("记录登录日志失败: {}", e);
                    }
                    return Err(AppError::Unauthorized(
                        "2FA验证码已使用，请等待新验证码".to_string(),
                    ));
                }
                let code_for_check = code.clone();
                let valid =
                    tokio::task::spawn_blocking(move || {
                        totp.check_current(&code_for_check).is_some()
                    })
                        .await
                        .map_err(|e| AppError::Internal(format!("2FA验证任务失败: {e}")))?;
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
                    Some("2FA secret too short"),
                )
                .await
                {
                    tracing::warn!("记录登录日志失败: {}", e);
                }
                return Err(AppError::Internal(format!(
                    "2FA密钥长度不足，请重新设置: {e}"
                )));
            }
        }
    }

    if !verified {
        crate::system::app_fail2ban::record_login_failure(
            &client_ip,
            &username,
            "Invalid 2FA code",
        );
        if let Err(e) = log_login(
            &conn,
            &username,
            &meta.ip_address,
            &meta.user_agent,
            false,
            Some("Invalid 2FA code"),
        )
        .await
        {
            tracing::warn!("记录登录日志失败: {}", e);
        }
        return Err(AppError::Unauthorized("验证码无效".to_string()));
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
        tracing::warn!("记录登录日志失败: {}", e);
    }
    crate::system::app_fail2ban::record_login_success(&client_ip, &user.username);
    tracing::info!("用户 {} 2FA验证登录成功", user.username);

    build_login_response(user, login_tokens, &user_lang, meta.is_secure)
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
        return Ok(crate::error::ok_json((), "发送成功"));
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

    Ok(crate::error::ok_json((), "验证码已发送"))
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
            tracing::warn!("撤销access_token失败: {}", e);
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
            tracing::warn!("撤销refresh_token失败: {}", e);
        }
    }

    let access_cookie = create_clear_cookie("access_token", secure);
    let refresh_cookie = create_clear_cookie("refresh_token", secure);

    let mut response = crate::error::ok_json((), "Success");
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
        None => return Err(AppError::Unauthorized("认证失败".to_string())),
    };

    let jwt_utils = &state.jwt_utils;

    let claims = jwt_utils.validate_token(&token).map_err(|err| {
        let msg = match err.kind() {
            ErrorKind::ExpiredSignature => "令牌已过期",
            _ => "无效令牌",
        };
        AppError::Unauthorized(msg.to_string())
    })?;

    if claims.token_type != "refresh" {
        return Err(AppError::Unauthorized("无效的刷新令牌".to_string()));
    }

    if crate::utils::is_token_revoked(&conn, &token)
        .await
        .map_err(|e| {
            tracing::error!("检查令牌撤销状态失败: {}", e);
            AppError::Database(e.to_string())
        })?
    {
        return Err(AppError::Unauthorized("令牌已撤销".to_string()));
    }

    let ip_address = &meta.ip_address;
    let user_agent = &meta.user_agent;
    let current_fingerprint = JwtUtils::generate_device_fingerprint(user_agent, ip_address);

    if let Some(ref token_fingerprint) = claims.device_fingerprint
        && token_fingerprint != &current_fingerprint
    {
        return Err(AppError::Unauthorized("设备验证失败".to_string()));
    }

    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|e| AppError::Internal(format!("无效的用户标识: {e}")))?;
    let token_expiry =
        chrono::DateTime::from_timestamp(claims.exp as i64, 0).unwrap_or_else(Utc::now);
    if let Err(e) = crate::utils::revoke_token(&conn, &token, Some(user_id), token_expiry).await {
        tracing::error!("撤销令牌失败: {}", e);
    }

    let token_duration = claims.exp.saturating_sub(claims.iat);
    let remember_me = token_duration > 86400;

    let access_token = jwt_utils
        .generate_access_token(
            &user_id,
            &claims.username,
            &claims.role,
            Some(&current_fingerprint),
            Some(ip_address),
        )
        .map_err(|e| AppError::Internal(format!("令牌生成失败: {e}")))?;
    let new_refresh_token = jwt_utils
        .generate_refresh_token(
            &user_id,
            &claims.username,
            &claims.role,
            Some(&current_fingerprint),
            Some(ip_address),
            remember_me,
        )
        .map_err(|e| AppError::Internal(format!("令牌生成失败: {e}")))?;

    let access_token_expiry = jwt_utils.get_access_token_expiry();
    let refresh_token_expiry = jwt_utils.get_actual_refresh_token_expiry(remember_me);

    tracing::debug!("用户 {} 令牌刷新成功", claims.username);

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
        "Success",
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
        "Success",
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

    let success_msg = "如果该邮箱已注册，重置邮件已发送";

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
            tracing::error!("保存重置令牌失败: {}", e);
        } else {
            let smtp_config = crate::system::smtp::get_smtp_config_from_db(&conn).await;
            if let Some(ref config) = smtp_config {
                let reset_link = format!("{}/reset-password?token={}", config.host, reset_token);
                let email_body = format!("请点击以下链接重置密码：{reset_link}");
                if let Err(e) =
                    crate::system::smtp::send_email_async(&conn, email, "密码重置", &email_body)
                        .await
                {
                    tracing::error!("发送重置邮件失败: {}", e);
                }
            }
        }
    }

    Ok(crate::error::ok_json((), success_msg))
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
                "UPDATE users SET password_hash = $1, reset_token = NULL, reset_token_expiry = NULL WHERE id = $2",
            )
            .bind(&hashed_password)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            Ok(crate::error::ok_json((), "密码重置成功"))
        }
        None => Err(AppError::Validation("重置链接无效或已过期".to_string())),
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
            return Err(AppError::Forbidden(
                "只有管理员可以为其他用户初始化2FA".to_string(),
            ));
        }
        user_id
    } else {
        Uuid::parse_str(&auth.sub)
            .map_err(|e| AppError::Validation(format!("无效的用户ID: {e}")))?
    };

    let target_username: String =
        match sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&conn)
            .await?
        {
            Some(name) => name,
            None => {
                return Err(AppError::NotFound("用户不存在".to_string()));
            }
        };

    let two_factor_enabled: bool =
        sqlx::query_scalar("SELECT two_factor_enabled FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_one(&conn)
            .await?;
    if two_factor_enabled {
        return Err(AppError::Conflict(
            "2FA已启用，请先禁用后再重新初始化".to_string(),
        ));
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
        .map_err(|e| AppError::Internal(format!("生成TOTP失败: {e}")))?;

    let encrypted_secret = encrypt_password_async(secret_base32).await?;
    sqlx::query("UPDATE users SET two_factor_secret = $1 WHERE id = $2")
        .bind(&encrypted_secret)
        .bind(target_user_id)
        .execute(&conn)
        .await?;

    let otpauth_url = totp
        .to_url()
        .map_err(|e| AppError::Internal(format!("生成otpauth URL失败: {e}")))?;
    let totp_for_qr = totp;
    let qr_code_base64 =
        tokio::task::spawn_blocking(move || totp_for_qr.to_qr_base64().unwrap_or_default())
            .await
            .map_err(|e| AppError::Internal(format!("QR码生成任务失败: {e}")))?;

    Ok(crate::error::ok_json(
        serde_json::json!({
            "otpauth_url": otpauth_url,
            "qr_code_base64": qr_code_base64,
        }),
        "2FA初始化成功，请使用认证器应用扫描二维码",
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
            return Err(AppError::Forbidden(
                "只有管理员可以为其他用户启用2FA".to_string(),
            ));
        }
        user_id
    } else {
        Uuid::parse_str(&auth.sub)
            .map_err(|e| AppError::Validation(format!("无效的用户ID: {e}")))?
    };

    let secret: Option<String> =
        match sqlx::query_scalar("SELECT two_factor_secret FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&conn)
            .await?
        {
            Some(s) => s,
            None => {
                return Err(AppError::Validation("请先初始化2FA".to_string()));
            }
        };

    let Some(encrypted_secret) = secret else {
        return Err(AppError::Validation("请先初始化2FA".to_string()));
    };

    let secret = decrypt_password_async(encrypted_secret)
        .await
        .map_err(|e| AppError::Internal(format!("2FA密钥解密失败: {e}")))?;

    let target_username: String =
        match sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&conn)
            .await?
        {
            Some(name) => name,
            None => {
                return Err(AppError::NotFound("用户不存在".to_string()));
            }
        };

    let secret = Secret::try_from_base32(&secret)
        .map_err(|_| AppError::Validation("密钥格式错误".to_string()))?;

    let totp = Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(secret)
        .with_issuer(Some("IPMA"))
        .with_account_name(target_username)
        .build()
        .map_err(|e| AppError::Internal(format!("TOTP创建失败: {e}")))?;

    let code = req.code.clone();
    if is_totp_code_replayed(target_user_id, &code) {
        return Err(AppError::Validation(
            "验证码已使用，请等待新验证码".to_string(),
        ));
    }
    let code_for_check = code.clone();
    let valid = tokio::task::spawn_blocking(move || totp.check_current(&code_for_check).is_some())
        .await
        .map_err(|e| AppError::Internal(format!("2FA验证任务失败: {e}")))?;
    if !valid {
        return Err(AppError::Validation("验证码错误".to_string()));
    }
    record_totp_usage(target_user_id, &code);

    sqlx::query(
        "UPDATE users SET two_factor_enabled = true, two_factor_verified = true WHERE id = $1",
    )
    .bind(target_user_id)
    .execute(&conn)
    .await?;

    Ok(crate::error::ok_json((), "2FA已启用"))
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
            return Err(AppError::Forbidden(
                "只有管理员可以为其他用户禁用2FA".to_string(),
            ));
        }
        user_id
    } else {
        Uuid::parse_str(&auth.sub)
            .map_err(|e| AppError::Validation(format!("无效的用户ID: {e}")))?
    };

    let (secret, two_factor_enabled): (Option<String>, bool) =
        sqlx::query_as("SELECT two_factor_secret, two_factor_enabled FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_one(&conn)
            .await?;

    if !two_factor_enabled {
        return Err(AppError::Validation("2FA未启用".to_string()));
    }

    let target_username: String =
        match sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&conn)
            .await?
        {
            Some(name) => name,
            None => {
                return Err(AppError::NotFound("用户不存在".to_string()));
            }
        };

    let mut verified = false;

    if let Some(encrypted_secret) = secret {
        let secret = decrypt_password_async(encrypted_secret)
            .await
            .map_err(|e| AppError::Internal(format!("2FA密钥解密失败: {e}")))?;
        let secret = Secret::try_from_base32(&secret)
            .map_err(|e| AppError::Internal(format!("2FA密钥格式错误: {e}")))?;
        let totp = Builder::new()
            .with_algorithm(Algorithm::SHA1)
            .with_digits(6)
            .with_skew(1)
            .with_step_duration(30)
            .with_secret(secret)
            .with_issuer(Some("IPMA"))
            .with_account_name(target_username)
            .build()
            .map_err(|e| AppError::Internal(format!("2FA密钥长度不足: {e}")))?;
        let code = req.code.clone();
        if is_totp_code_replayed(target_user_id, &code) {
            return Err(AppError::Validation(
                "验证码已使用，请等待新验证码".to_string(),
            ));
        }
        let code_for_check = code.clone();
        let valid = tokio::task::spawn_blocking(move || totp.check_current(&code_for_check).is_some())
            .await
            .map_err(|e| AppError::Internal(format!("2FA验证任务失败: {e}")))?;
        if valid {
            record_totp_usage(target_user_id, &code);
            verified = true;
        }
    }

    if !verified {
        return Err(AppError::Validation("验证码错误".to_string()));
    }

    sqlx::query(
        "UPDATE users SET two_factor_enabled = false, two_factor_secret = NULL, two_factor_verified = false WHERE id = $1"
    )
    .bind(target_user_id)
    .execute(&conn)
    .await?;

    Ok(crate::error::ok_json((), "2FA已禁用"))
}

#[derive(Debug, Deserialize, Validate)]
pub struct TwoFactorInitRequest {
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct TwoFactorEnableRequest {
    #[validate(length(min = 6, max = 6, message = "验证码长度必须为6个字符"))]
    pub code: String,
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct TwoFactorDisableRequest {
    #[validate(length(min = 6, max = 6, message = "验证码长度必须为6个字符"))]
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
    user_lang: &str,
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
        Json(ApiResponse::success_i18n(
            serde_json::json!({ "user": user, "expires_in": login_tokens.access_token_expiry }),
            "api.success",
            user_lang,
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
        .map_err(|e| AppError::Internal(format!("令牌生成失败: {e}")))?;
    let refresh_token = jwt_utils
        .generate_refresh_token(
            id,
            username,
            role,
            Some(device_fingerprint),
            Some(ip_address),
            remember_me,
        )
        .map_err(|e| AppError::Internal(format!("令牌生成失败: {e}")))?;
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
        .map_err(|e| AppError::Internal(format!("无效的cookie值: {e}")))?;
    response.headers_mut().append(SET_COOKIE, header_value);
    Ok(())
}
