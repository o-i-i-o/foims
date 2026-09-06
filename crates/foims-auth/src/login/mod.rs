//! 登录域：密码登录主入口与登录共享辅助。
//!
//! - `middleware`：鉴权中间件与仅限本机守卫
//! - `token`：令牌签发/刷新/登出、外部认证建户与 Cookie
//! - `password_reset`：密码找回/重置/修改
//! - `two_factor`：TOTP 两步验证
//! - `email_code`：邮箱验证码登录与邮件频控

mod email_code;
mod middleware;
mod password_reset;
mod token;
mod two_factor;

pub(crate) use email_code::cleanup_email_send_records;
pub use email_code::{EmailCodeLoginRequest, login_with_email_code, send_login_code};
pub use middleware::{auth_middleware, localhost_only_middleware};
pub use password_reset::{ChangePasswordRequest, change_password, forgot_password, reset_password};
pub(crate) use token::{
    ExternalUser, append_cookie_to_response, build_login_response, create_auth_cookie,
    find_or_create_external_user, generate_login_tokens, issue_external_login_tokens,
};
pub use token::{get_current_user, logout, refresh_token};
pub use two_factor::{
    TwoFactorCodeLoginRequest, TwoFactorDisableRequest, TwoFactorEnableRequest,
    TwoFactorInitRequest, disable_two_factor, enable_two_factor, init_two_factor,
    login_with_two_factor, send_two_factor_code,
};
pub(crate) use two_factor::{requires_two_factor_response, verify_external_totp};

use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use bcrypt::verify;
use chrono::Utc;
use rand::RngExt;
use uuid::Uuid;
use validator::Validate;

use crate::jwt::JwtUtils;
use crate::meta::RequestMeta;
use crate::provider::AuthProvider;
use foims_common::AppError;
use foims_common::AppJson;
use foims_common::msg;
use foims_models::{User, UserLogin};

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

/// 进程级 dummy bcrypt 哈希（首次使用时生成，cost 与真实口令一致）
static DUMMY_BCRYPT_HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// 生成 6 位数字验证码（2FA 邮件验证码的启用与登录两条路径共用）
pub(super) fn generate_six_digit_code() -> String {
    let mut rng = rand::rng();
    (0..6)
        .map(|_| rng.random_range(0..10).to_string())
        .collect()
}

/// 运行时哈希生成失败时使用的静态兜底（"foims-dummy-password" 的合法
/// bcrypt 哈希，cost 12 与 DEFAULT_COST 一致），保证缓解不会静默失效
const DUMMY_BCRYPT_FALLBACK: &str = "$2b$12$NUV.EviGW4zymnRdLE45LO60.7WNTwztAaUGRdQMKrKQmHRa6Ccxq";

/// 用户不存在时对提交口令执行一次等价 bcrypt 校验：
/// 消除「用户不存在立即返回、用户存在时 bcrypt 校验约数百毫秒」的
/// 用户名枚举时间侧信道（security-review A-4）
pub(super) async fn dummy_bcrypt_verify(password: &str) {
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
