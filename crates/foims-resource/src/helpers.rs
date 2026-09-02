//! 资源域内部辅助：子网查询、房间校验、站内通知与 MAC 变更告警。
//!
//! 仅由本 crate 的各资源模块使用；不对外再导出。

use uuid::Uuid;

use foims_common::AppError;
use foims_common::msg;
use foims_common::{log_error, log_info, log_warn};

// ==================== 子网业务查询 ====================

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
) -> Result<foims_models::Network, AppError> {
    use sqlx::Row;

    Ok(foims_models::Network {
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
                    AppError::Internal(msg("server.common.deserialize_failed").with("error", e))
                })
            })
            .transpose()?,
        ipv6_dns: row
            .get::<Option<serde_json::Value>, _>(9)
            .map(|v| {
                serde_json::from_value(v).map_err(|e| {
                    AppError::Internal(msg("server.common.deserialize_failed").with("error", e))
                })
            })
            .transpose()?,
        description: row.get(10),
        created_at: row.get(11),
        updated_at: row.get(12),
    })
}

pub async fn validate_network_in_room<'e, E>(
    executor: E,
    room_id: Uuid,
    subnet_id: Option<Uuid>,
) -> Result<(), AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let Some(nid) = subnet_id else {
        return Ok(());
    };

    let network_in_room: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM room_networks WHERE room_id = $1 AND subnet_id = $2)",
    )
    .bind(room_id)
    .bind(nid)
    .fetch_one(executor)
    .await?;

    if !network_in_room {
        return Err(AppError::Validation(msg("server.network.not_in_room")));
    }

    Ok(())
}

pub async fn get_room_id_by_workstation<'e, E>(
    executor: E,
    workstation_id: Uuid,
) -> Result<Option<Uuid>, AppError>
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
) -> Result<Option<Uuid>, AppError>
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

// ==================== 站内通知与 MAC 变更告警 ====================

/// 组装站内通知内容：以 JSON 形式存储「消息 key + 动态参数」，
/// 前端展示时解析并按用户语言翻译；历史遗留的纯文本内容原样展示。
fn encode_notification_content(key: &str, params: &[(&str, &str)]) -> String {
    let params: std::collections::HashMap<String, String> = params
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    serde_json::json!({ "key": key, "params": params }).to_string()
}

/// 写入一条站内通知（user_id 为 None 时为广播占位历史行）。
pub async fn create_notification(
    pool: &sqlx::PgPool,
    title: &str,
    content: &str,
    notification_type: &str,
    user_id: Option<&Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(r"INSERT INTO notifications (id, user_id, title, content, notification_type, read, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7)")
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(title)
        .bind(content)
        .bind(notification_type)
        .bind(false)
        .bind(chrono::Utc::now())
        .execute(pool)
        .await?;
    Ok(())
}

/// 工作站 MAC 变更告警：站内通知全部管理员并发送邮件（SMTP 未配置时降级）。
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
        create_notification(
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

    match foims_auth::smtp::send_mac_change_email(
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

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

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
