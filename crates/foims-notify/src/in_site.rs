//! 站内通知原语：i18n 内容信封、通知落库、管理员收件人查询。
//!
//! 仅依赖 foims-common 与 sqlx，供各业务域 crate（SNMP 事件、主机告警、
//! IP 异动、Trap 等）共用；多渠道外发见 [`crate::dispatch`]。

use uuid::Uuid;

/// 组装站内通知内容：以 JSON 形式存储「消息 key + 动态参数」，
/// 前端展示时解析并按用户语言翻译；历史遗留的纯文本内容原样展示。
pub fn encode_notification_content(key: &str, params: &[(&str, &str)]) -> String {
    let params: std::collections::HashMap<String, String> = params
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    serde_json::json!({ "key": key, "params": params }).to_string()
}

/// 写入一条站内通知（user_id 为 None 时为广播占位历史行），返回新行 ID
/// （告警 episode 需要按 ID 就地刷新同一条通知的文案）。
pub async fn create_notification(
    pool: &sqlx::PgPool,
    title: &str,
    content: &str,
    notification_type: &str,
    user_id: Option<&Uuid>,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query(r"INSERT INTO notifications (id, user_id, title, content, notification_type, read, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7)")
        .bind(id)
        .bind(user_id)
        .bind(title)
        .bind(content)
        .bind(notification_type)
        .bind(false)
        .bind(chrono::Utc::now())
        .execute(pool)
        .await?;
    Ok(id)
}

/// 查启用状态的管理员（admin/secadmin）ID；站内通知按人各发一条，
/// 已读状态随用户独立。SNMP Trap 通道含 sysadmin 角色，由调用方自行查询。
pub async fn active_admin_ids(pool: &sqlx::PgPool) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM users WHERE enabled = TRUE AND role IN ('admin', 'secadmin')",
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_notification_content() {
        let content = encode_notification_content(
            "server.notification.ip_change.body",
            &[("ip", "10.0.0.1")],
        );
        let value: serde_json::Value =
            serde_json::from_str(&content).unwrap_or_else(|e| panic!("应为合法 JSON: {e}"));
        assert_eq!(value["key"], "server.notification.ip_change.body");
        assert_eq!(value["params"]["ip"], "10.0.0.1");
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
