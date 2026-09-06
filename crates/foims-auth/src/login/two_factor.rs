//! 两步验证（TOTP 动态码）：初始化/启用/停用、动态码登录与重放防护。
//!
//! 由 login.rs 拆分而来（纯移动）。

use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use bcrypt::verify;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

use crate::jwt::JwtUtils;
use crate::meta::RequestMeta;
use crate::provider::AuthProvider;
use foims_common::AppError;
use foims_common::AppJson;
use foims_common::crypto::{decrypt_password_async, encrypt_password_async};
use foims_common::msg;
use foims_models::{SendTwoFactorCodeRequest, TwoFactorLoginRequest, User};
use totp_rs::{Algorithm, Builder, Secret};

use super::email_code::enforce_email_send_limit;
use super::{
    ExternalUser, build_login_response, dummy_bcrypt_verify, generate_login_tokens,
    generate_six_digit_code, log_login,
};

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

    let code = generate_six_digit_code();
    let expiry = Utc::now() + chrono::Duration::minutes(5);
    sqlx::query("UPDATE users SET two_factor_email_code = $1, two_factor_email_code_expiry = $2 WHERE id = $3")
        .bind(&code).bind(expiry).bind(id).execute(&conn).await?;

    let email_body = format!("您的两步验证码是：{code}");
    crate::smtp::send_email_async(&conn, &email, "两步验证码", &email_body).await?;

    Ok(foims_common::ok_json((), "server.auth.code_sent"))
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
