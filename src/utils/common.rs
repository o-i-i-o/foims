//! 业务侧通用工具（请求元信息、操作日志、令牌黑名单、网络业务查询等）。
//!
//! 纯网络/HTTP 工具已下沉至 `ipma_common::net`，并经 `utils::mod` 再导出；
//! 本模块保留依赖业务表、`JwtClaims`、通知与 SMTP 的部分。

use uuid::Uuid;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use ipma_common::{log_debug, log_error, log_info, log_warn};

use ipma_common::AppError;
use ipma_common::msg;

use super::{
    detect_user_language_from_parts, generate_token_hash, get_real_ip_from_parts,
    get_user_agent_from_parts, is_secure_from_parts,
};

pub async fn validate_network_in_room<'e, E>(
    executor: E,
    room_id: Uuid,
    network_id: Option<Uuid>,
) -> Result<(), ipma_common::AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let Some(nid) = network_id else {
        return Ok(());
    };

    let network_in_room: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM room_networks WHERE room_id = $1 AND network_id = $2)",
    )
    .bind(room_id)
    .bind(nid)
    .fetch_one(executor)
    .await?;

    if !network_in_room {
        return Err(ipma_common::AppError::Validation(msg(
            "server.network.not_in_room",
        )));
    }

    Ok(())
}

pub async fn get_room_id_by_workstation<'e, E>(
    executor: E,
    workstation_id: Uuid,
) -> Result<Option<Uuid>, ipma_common::AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let room_id: Option<Uuid> =
        sqlx::query_scalar("SELECT room_id FROM workstations WHERE id = $1")
            .bind(workstation_id)
            .fetch_optional(executor)
            .await?
            .flatten();

    Ok(room_id)
}

pub async fn get_room_id_by_position<'e, E>(
    executor: E,
    position_id: Uuid,
) -> Result<Option<Uuid>, ipma_common::AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let room_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT c.room_id FROM positions p LEFT JOIN cabinets c ON p.cabinet_id = c.id WHERE p.id = $1",
    )
    .bind(position_id)
    .fetch_optional(executor)
    .await?
    .flatten();

    Ok(room_id)
}

// ==================== Token 管理 ====================

pub async fn is_token_revoked(pool: &sqlx::PgPool, token: &str) -> Result<bool, sqlx::Error> {
    let token_hash = generate_token_hash(token);

    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM revoked_tokens WHERE token_hash = $1 AND expiry > NOW()",
    )
    .bind(&token_hash)
    .fetch_one(pool)
    .await?;

    Ok(count > 0)
}

pub async fn revoke_token(
    pool: &sqlx::PgPool,
    token: &str,
    user_id: Option<Uuid>,
    expiry: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    let token_hash = generate_token_hash(token);

    // 检查用户是否存在，不存在则使用 NULL
    let valid_user_id = if let Some(uid) = user_id {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(uid)
            .fetch_one(pool)
            .await?;
        if exists { Some(uid) } else { None }
    } else {
        None
    };

    sqlx::query("INSERT INTO revoked_tokens (token_hash, user_id, expiry) VALUES ($1, $2, $3)")
        .bind(&token_hash)
        .bind(valid_user_id)
        .bind(expiry)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn cleanup_expired_revoked_tokens(pool: &sqlx::PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM revoked_tokens WHERE expiry < NOW()")
        .execute(pool)
        .await?;

    let deleted_count = result.rows_affected();
    if deleted_count > 0 {
        // 仅供 token_cleanup 定时任务调用，例行日志降为 debug 避免刷屏
        log_debug!("log.token.revoked_cleaned", count = deleted_count);
    }

    Ok(deleted_count)
}

pub async fn cleanup_old_token_usage(
    pool: &sqlx::PgPool,
    days_to_keep: i32,
) -> Result<u64, sqlx::Error> {
    let result =
        sqlx::query("DELETE FROM token_usage WHERE created_at < NOW() - INTERVAL '1 day' * $1")
            .bind(days_to_keep)
            .execute(pool)
            .await?;

    let deleted_count = result.rows_affected();
    if deleted_count > 0 {
        log_info!(
            "log.token.usage_cleaned",
            count = deleted_count,
            days = days_to_keep
        );
    }

    Ok(deleted_count)
}

// ==================== 请求元信息提取器 ====================

/// 请求元信息：从请求 parts 中提取 IP、语言、User-Agent、JWT claims、是否 HTTPS
/// 用于操作日志记录、语言检测、IP 提取等场景
#[derive(Clone, Debug, Default)]
pub struct RequestMeta {
    pub ip_address: String,
    pub user_lang: String,
    pub user_agent: String,
    pub is_secure: bool,
    pub claims: Option<crate::auth::utils::JwtClaims>,
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
            claims: parts
                .extensions
                .get::<crate::auth::utils::JwtClaims>()
                .cloned(),
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

    // 审计外发（syslog）：旁路尽力而为，未启用时内部直接跳过
    let forward_message = format!(
        "op action={action} resource={resource_type} ip={} user={} details={details}",
        meta.ip_address,
        meta.user_id()
            .map(|u| u.to_string())
            .unwrap_or_else(|| "-".to_string()),
    );
    crate::log::forwarding::spawn_forward(pool.clone(), forward_message);
}

// ==================== 通知与告警 ====================

/// 组装站内通知内容：以 JSON 形式存储「消息 key + 动态参数」，
/// 前端展示时解析并按用户语言翻译；历史遗留的纯文本内容原样展示。
fn encode_notification_content(key: &str, params: &[(&str, &str)]) -> String {
    let params: std::collections::HashMap<String, String> = params
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    serde_json::json!({ "key": key, "params": params }).to_string()
}

pub async fn send_mac_change_notification(
    pool: &sqlx::PgPool,
    workstation_id: &Uuid,
    ip_address: &str,
    old_mac: &str,
    new_mac: &str,
) -> Result<(), sqlx::Error> {
    let workstation_name =
        match sqlx::query_scalar::<_, String>("SELECT name FROM workstations WHERE id = $1")
            .bind(workstation_id)
            .fetch_optional(pool)
            .await
        {
            Ok(Some(name)) => name,
            Ok(None) => {
                log_warn!("log.workstation.not_found", id = workstation_id);
                workstation_id.to_string()
            }
            Err(e) => {
                log_warn!("log.workstation.query_name_failed", error = e);
                workstation_id.to_string()
            }
        };

    let content = encode_notification_content(
        "server.notification.mac_change.body",
        &[
            ("workstation", &workstation_name),
            ("ip", ip_address),
            ("old_mac", old_mac),
            ("new_mac", new_mac),
        ],
    );
    // 通知目标：所有启用状态的管理员（admin/secadmin）按人各发一条，
    // 已读状态随用户独立；此前统一写 user_id = NULL 的行不匹配任何人的
    // 查询条件（user_id = $1），通知列表对所有用户恒为空
    let admin_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM users WHERE status = TRUE AND role IN ('admin', 'secadmin')",
    )
    .fetch_all(pool)
    .await?;

    for admin_id in &admin_ids {
        crate::log::notification::create_notification(
            pool,
            "server.notification.mac_change.title",
            &content,
            "mac_change",
            Some(admin_id),
        )
        .await?;
    }
    log_info!(
        "log.mac_change.notification_created",
        workstation = workstation_name,
        ip = ip_address,
        recipients = admin_ids.len()
    );

    match crate::system::smtp::send_mac_change_email(
        pool,
        &workstation_name,
        ip_address,
        old_mac,
        new_mac,
    )
    .await
    {
        Ok(()) => {
            log_info!("log.mac_change.email_sent", workstation = workstation_name)
        }
        // SMTP 未配置属预期情形，降级为告警日志
        Err(AppError::NotFound(m)) => {
            log_warn!(
                "log.mac_change.email_skipped",
                workstation = workstation_name,
                reason = m.key()
            );
        }
        Err(e) => {
            log_error!(
                "log.mac_change.email_send_failed",
                workstation = workstation_name,
                error = e
            );
        }
    }

    Ok(())
}

// ==================== 网络查询工具 ====================

pub const NETWORK_QUERY: &str = r"
    SELECT n.id, n.name, n.network_region_id, nt.name as network_region,
           n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT,
           host(n.ipv4_gateway), host(n.ipv6_gateway),
           (SELECT json_agg(host(d)) FROM unnest(n.ipv4_dns) AS d) as ipv4_dns,
           (SELECT json_agg(host(d)) FROM unnest(n.ipv6_dns) AS d) as ipv6_dns,
           n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ
    FROM network_cidrs n
    JOIN network_regions nt ON n.network_region_id = nt.id
    WHERE n.id = $1
";

pub fn parse_network_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<ipma_models::Network, ipma_common::AppError> {
    use sqlx::Row;

    Ok(ipma_models::Network {
        id: row.get(0),
        name: row.get(1),
        network_region_id: row.get(2),
        network_region: row.get(3),
        ipv4_cidr: row.get(4),
        ipv6_cidr: row.get(5),
        ipv4_gateway: row.get(6),
        ipv6_gateway: row.get(7),
        ipv4_dns: row
            .get::<Option<serde_json::Value>, _>(8)
            .map(|v| {
                serde_json::from_value(v).map_err(|e| {
                    ipma_common::AppError::Internal(
                        msg("server.common.deserialize_failed").with("error", e),
                    )
                })
            })
            .transpose()?,
        ipv6_dns: row
            .get::<Option<serde_json::Value>, _>(9)
            .map(|v| {
                serde_json::from_value(v).map_err(|e| {
                    ipma_common::AppError::Internal(
                        msg("server.common.deserialize_failed").with("error", e),
                    )
                })
            })
            .transpose()?,
        description: row.get(10),
        created_at: row.get(11),
        updated_at: row.get(12),
    })
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- RequestMeta::user_id ----------

    /// 构造 JwtClaims 的测试辅助函数
    fn claims_with_sub(sub: &str) -> crate::auth::utils::JwtClaims {
        crate::auth::utils::JwtClaims {
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

    // ---------- 通知内容编码（私有辅助函数） ----------

    #[test]
    fn test_encode_notification_content() {
        let content = encode_notification_content(
            "server.notification.mac_change.body",
            &[("ip", "10.0.0.1"), ("old_mac", "aa:aa")],
        );
        let value: serde_json::Value =
            serde_json::from_str(&content).unwrap_or_else(|e| panic!("应为合法 JSON: {e}"));
        assert_eq!(value["key"], "server.notification.mac_change.body");
        assert_eq!(value["params"]["ip"], "10.0.0.1");
        assert_eq!(value["params"]["old_mac"], "aa:aa");
    }

    #[test]
    fn test_encode_notification_content_empty_params() {
        // 无参数时 params 为空对象
        let content = encode_notification_content("some.key", &[]);
        let value: serde_json::Value =
            serde_json::from_str(&content).unwrap_or_else(|e| panic!("应为合法 JSON: {e}"));
        assert_eq!(value["key"], "some.key");
        assert_eq!(
            value["params"].as_object().map(serde_json::Map::len),
            Some(0)
        );
    }
}
