//! 令牌签发与会话：登出、刷新、当前用户、外部认证建户与 Cookie 封装。
//!
//! 由 login.rs 拆分而来（纯移动）。

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::{DateTime, Utc};
use jsonwebtoken::errors::ErrorKind;
use uuid::Uuid;

use super::log_login;
use crate::extractor::{AccessToken, RefreshToken, SecureFlag};
use crate::jwt::{JwtUtils, hash_password};
use crate::meta::RequestMeta;
use crate::provider::AuthProvider;
use foims_common::AppError;
use foims_common::msg;
use foims_models::User;

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
            crate::jwt::revoke_token(&state.pool()?.get_conn(), &access_token, user_id, expiry)
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
            crate::jwt::revoke_token(&state.pool()?.get_conn(), &refresh_token, user_id, expiry)
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

    if crate::jwt::is_token_revoked(&conn, &token)
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
    if let Err(e) = crate::jwt::revoke_token(&conn, &token, Some(user_id), token_expiry).await {
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
