//! 站内通知与 MAC 变更告警（自 foims-resource::helpers 下放）。
//!
//! 仅依赖 foims-common（日志）与 foims-auth（SMTP 开关/发送），
//! 供资源域（ip.rs）、采集服务域（alerts.rs/trap.rs）与 SNMP 事件模块共用。

use uuid::Uuid;

use foims_common::{log_error, log_info, log_warn};

/// 组装站内通知内容：以 JSON 形式存储「消息 key + 动态参数」，
/// 前端展示时解析并按用户语言翻译；历史遗留的纯文本内容原样展示。
pub fn encode_notification_content(key: &str, params: &[(&str, &str)]) -> String {
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

/// 工作站 MAC 变更告警：站内通知全部管理员；邮件方式按「通知配置」中
/// MAC 地址变更的邮件开关决定是否发送（SMTP 未配置时降级）。
/// `source_device` 为拉取该 MAC 的来源设备名，站内通知与邮件均注明。
pub async fn send_mac_change_notification(
    pool: &sqlx::PgPool,
    workstation_id: &Uuid,
    ip_address: &str,
    old_mac: &str,
    new_mac: &str,
    source_device: &str,
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
            ("device", source_device),
        ],
    );
    // 通知目标：所有启用状态的管理员（admin/secadmin）按人各发一条，
    // 已读状态随用户独立；此前统一写 user_id = NULL 的行不匹配任何人的
    // 查询条件（user_id = $1），通知列表对所有用户恒为空
    let admin_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM users WHERE enabled = TRUE AND role IN ('admin', 'secadmin')",
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

    // 邮件方式开关关闭时跳过发送（站内通知不受影响）
    if foims_auth::smtp::load_notification_methods(pool)
        .await
        .mac_change
    {
        match foims_auth::smtp::send_mac_change_email(
            pool,
            source_device,
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
            Err(e) => {
                log_error!(
                    "log.mac_change.email_send_failed",
                    workstation = workstation_name,
                    error = e
                );
            }
        }
    }

    Ok(())
}

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
