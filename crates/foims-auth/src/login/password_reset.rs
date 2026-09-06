//! 密码找回 / 重置 / 修改。

use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use bcrypt::verify;
use chrono::Utc;
use uuid::Uuid;
use validator::Validate;

use crate::jwt::hash_password;
use crate::meta::{RequestMeta, log_op_best_effort};
use crate::provider::AuthProvider;
use foims_common::AppError;
use foims_common::AppJson;
use foims_common::msg;
use foims_models::{ForgotPasswordRequest, ResetPasswordRequest};

use super::email_code::enforce_email_send_limit;

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
