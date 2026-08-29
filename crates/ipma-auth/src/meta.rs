//! 请求元信息与操作日志（审计）。
//!
//! `RequestMeta` 从请求 parts 中提取 IP、语言、User-Agent、JWT claims；
//! 操作日志写入 `operation_logs` 表。审计外发（syslog）通过
//! [`set_forward_hook`] 由二进制 crate 在启动时注入（沿用 log_i18n
//! 的钩子注入先例），本 crate 自身不持有外发实现。

use std::sync::OnceLock;

use sqlx::PgPool;
use uuid::Uuid;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use ipma_common::log_warn;
use ipma_common::net::{
    detect_user_language_from_parts, get_real_ip_from_parts, get_user_agent_from_parts,
    is_secure_from_parts,
};

use crate::utils::JwtClaims;

/// 审计外发钩子：(连接池, 消息) -> ()。由二进制 crate 注册。
type ForwardFn = fn(PgPool, String);

static FORWARD_HOOK: OnceLock<ForwardFn> = OnceLock::new();

/// 注册审计外发钩子（由持有 syslog 外发实现的二进制 crate 在启动时调用一次）。
pub fn set_forward_hook(f: ForwardFn) {
    let _ = FORWARD_HOOK.set(f);
}

/// 转发审计外发消息（未注册钩子时静默跳过）。
pub fn forward_op_log(pool: PgPool, message: String) {
    if let Some(f) = FORWARD_HOOK.get() {
        f(pool, message);
    }
}

/// 请求元信息：从请求 parts 中提取 IP、语言、User-Agent、JWT claims、是否 HTTPS
/// 用于操作日志记录、语言检测、IP 提取等场景
#[derive(Clone, Debug, Default)]
pub struct RequestMeta {
    pub ip_address: String,
    pub user_lang: String,
    pub user_agent: String,
    pub is_secure: bool,
    pub claims: Option<JwtClaims>,
}

impl RequestMeta {
    /// 从 JWT claims 解析用户 ID
    #[must_use]
    pub fn user_id(&self) -> Option<Uuid> {
        self.claims
            .as_ref()
            .and_then(|c| Uuid::parse_str(&c.sub).ok())
    }
}

impl<S: Send + Sync> FromRequestParts<S> for RequestMeta {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(RequestMeta {
            ip_address: get_real_ip_from_parts(parts),
            user_lang: detect_user_language_from_parts(parts),
            user_agent: get_user_agent_from_parts(parts),
            is_secure: is_secure_from_parts(parts),
            claims: parts.extensions.get::<JwtClaims>().cloned(),
        })
    }
}

// ==================== 操作日志 ====================

pub struct OperationLogParams<'a> {
    pub ip_address: &'a str,
    pub user_id: Option<Uuid>,
    pub action: &'a str,
    pub resource_type: &'a str,
    pub resource_id: Option<&'a Uuid>,
    pub details: &'a serde_json::Value,
    pub result: bool,
}

pub async fn log_system_operation(
    pool: &sqlx::PgPool,
    params: OperationLogParams<'_>,
) -> Result<(), sqlx::Error> {
    // 检查用户是否存在，不存在则使用 NULL
    let valid_user_id = if let Some(uid) = params.user_id {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(uid)
            .fetch_one(pool)
            .await?;
        if exists { Some(uid) } else { None }
    } else {
        None
    };

    sqlx::query(r"INSERT INTO operation_logs (id, user_id, action, resource_type, resource_id, details, result, ip_address, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)")
        .bind(Uuid::new_v4())
        .bind(valid_user_id)
        .bind(params.action)
        .bind(params.resource_type)
        .bind(params.resource_id)
        .bind(params.details)
        .bind(params.result)
        .bind(params.ip_address)
        .bind(chrono::Utc::now())
        .execute(pool)
        .await?;
    Ok(())
}

/// 尽力而为地记录操作日志：失败时仅打印警告，不影响主流程。
///
/// 用于替代各 handler 中重复的 `if let Err(e) = log_system_operation(...).await { warn!(...) }` 样板。
/// 所有调用点均使用 `result: true`（失败路径由各 handler 自行返回错误）。
pub async fn log_op_best_effort(
    pool: &sqlx::PgPool,
    meta: &RequestMeta,
    action: &str,
    resource_type: &str,
    resource_id: Option<&Uuid>,
    details: &serde_json::Value,
) {
    if let Err(e) = log_system_operation(
        pool,
        OperationLogParams {
            ip_address: &meta.ip_address,
            user_id: meta.user_id(),
            action,
            resource_type,
            resource_id,
            details,
            result: true,
        },
    )
    .await
    {
        log_warn!("log.operation.record_failed", error = e);
    }

    // 审计外发（syslog）：旁路尽力而为，未启用（未注册钩子）时内部直接跳过
    let forward_message = format!(
        "op action={action} resource={resource_type} ip={} user={} details={details}",
        meta.ip_address,
        meta.user_id()
            .map(|u| u.to_string())
            .unwrap_or_else(|| "-".to_string()),
    );
    forward_op_log(pool.clone(), forward_message);
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    /// 构造 JwtClaims 的测试辅助函数
    fn claims_with_sub(sub: &str) -> JwtClaims {
        JwtClaims {
            sub: sub.to_string(),
            username: "tester".to_string(),
            role: "admin".to_string(),
            exp: 4_102_444_800,
            iat: 1_700_000_000,
            iss: "ipma".to_string(),
            jti: "test-jti".to_string(),
            aud: "ipma-web".to_string(),
            token_type: "access".to_string(),
            device_fingerprint: None,
            ip_address: None,
            remember_me: None,
        }
    }

    #[test]
    fn test_request_meta_user_id_from_claims() {
        // 合法 UUID 的 sub 可解析出用户 ID
        let uid = Uuid::new_v4();
        let meta = RequestMeta {
            claims: Some(claims_with_sub(&uid.to_string())),
            ..RequestMeta::default()
        };
        assert_eq!(meta.user_id(), Some(uid));
    }

    #[test]
    fn test_request_meta_user_id_invalid_or_missing() {
        // sub 非法或缺失 claims 时返回 None
        let meta_bad = RequestMeta {
            claims: Some(claims_with_sub("not-a-uuid")),
            ..RequestMeta::default()
        };
        assert_eq!(meta_bad.user_id(), None);

        let meta_none = RequestMeta::default();
        assert_eq!(meta_none.user_id(), None);
        assert_eq!(meta_none.ip_address, "");
        assert_eq!(meta_none.user_lang, "");
    }
}
