//! 邮箱验证码登录与公开端点邮件频控。
//!
//! 由 login.rs 拆分而来（纯移动）。

use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

use crate::jwt::JwtUtils;
use crate::meta::RequestMeta;
use crate::provider::AuthProvider;
use foims_common::AppError;
use foims_common::AppJson;
use foims_common::crypto::constant_time_eq;
use foims_common::msg;
use foims_models::{EmailLoginRequest, SendLoginCodeRequest, User};

use super::{build_login_response, generate_login_tokens, generate_six_digit_code, log_login};

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
pub(super) fn enforce_email_send_limit(ip: &str, target: &str) -> Result<(), AppError> {
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

    let code = generate_six_digit_code();
    let expiry = Utc::now() + chrono::Duration::minutes(5);

    sqlx::query("UPDATE users SET two_factor_email_code = $1, two_factor_email_code_expiry = $2 WHERE id = $3")
        .bind(&code).bind(expiry).bind(id).execute(&conn).await?;

    let email_body = format!("您的登录验证码是：{code}");
    crate::smtp::send_email_async(&conn, email, "登录验证码", &email_body).await?;

    Ok(foims_common::ok_json((), "server.auth.code_sent"))
}
