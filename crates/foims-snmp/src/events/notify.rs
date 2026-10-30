//! 站内通知与 IP 变更/设备更换告警。
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

/// 解析工位名（站内通知与邮件文案用）；查询失败/缺行降级为 UUID 字符串。
async fn resolve_workstation_name(pool: &sqlx::PgPool, workstation_id: &Uuid) -> String {
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
    }
}

/// 查启用状态的管理员（admin/secadmin）ID；站内通知按人各发一条，
/// 已读状态随用户独立。
async fn active_admin_ids(pool: &sqlx::PgPool) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM users WHERE enabled = TRUE AND role IN ('admin', 'secadmin')",
    )
    .fetch_all(pool)
    .await
}

/// 工位设备 IP 变更告警（MAC 物理设备从登记 IP 移到了新的观测 IP）：
/// 站内通知全部管理员；邮件按「通知配置」中 IP 地址变更的开关决定是否
/// 发送（SMTP 未配置时降级）。`source_device` 为拉取到该观测的来源设备名。
pub async fn send_ip_change_notification(
    pool: &sqlx::PgPool,
    workstation_id: &Uuid,
    mac_address: &str,
    registered_ip: &str,
    observed_ip: &str,
    source_device: &str,
) -> Result<(), sqlx::Error> {
    let workstation_name = resolve_workstation_name(pool, workstation_id).await;

    let content = encode_notification_content(
        "server.notification.ip_change.body",
        &[
            ("workstation", &workstation_name),
            ("mac", mac_address),
            ("registered_ip", registered_ip),
            ("observed_ip", observed_ip),
            ("device", source_device),
        ],
    );
    let admin_ids = active_admin_ids(pool).await?;
    for admin_id in &admin_ids {
        create_notification(
            pool,
            "server.notification.ip_change.title",
            &content,
            "ip_change",
            Some(admin_id),
        )
        .await?;
    }
    log_info!(
        "log.ip_change.notification_created",
        workstation = workstation_name,
        observed_ip = observed_ip,
        recipients = admin_ids.len()
    );

    if foims_auth::smtp::load_notification_methods(pool)
        .await
        .ip_change
    {
        match foims_auth::smtp::send_ip_change_email(
            pool,
            source_device,
            &workstation_name,
            mac_address,
            registered_ip,
            observed_ip,
        )
        .await
        {
            Ok(()) => log_info!("log.ip_change.email_sent", workstation = workstation_name),
            Err(e) => log_error!(
                "log.ip_change.email_send_failed",
                workstation = workstation_name,
                error = e
            ),
        }
    }

    Ok(())
}

/// 工位 IP 上设备更换告警（登记 IP 不变、学到新 MAC，即旧“MAC 变更”语义）。
/// 与 IP 变更同属“MAC/IP 对应关系异动”类，邮件同样受 ip_change 开关门控。
/// `source_device` 为拉取到该观测的来源设备名。
pub async fn send_device_swap_notification(
    pool: &sqlx::PgPool,
    workstation_id: &Uuid,
    ip_address: &str,
    old_mac: &str,
    new_mac: &str,
    source_device: &str,
) -> Result<(), sqlx::Error> {
    let workstation_name = resolve_workstation_name(pool, workstation_id).await;

    let content = encode_notification_content(
        "server.notification.device_swap.body",
        &[
            ("workstation", &workstation_name),
            ("ip", ip_address),
            ("old_mac", old_mac),
            ("new_mac", new_mac),
            ("device", source_device),
        ],
    );
    let admin_ids = active_admin_ids(pool).await?;
    for admin_id in &admin_ids {
        create_notification(
            pool,
            "server.notification.device_swap.title",
            &content,
            "ip_change",
            Some(admin_id),
        )
        .await?;
    }
    log_info!(
        "log.ip_change.device_swap_created",
        workstation = workstation_name,
        ip = ip_address,
        recipients = admin_ids.len()
    );

    if foims_auth::smtp::load_notification_methods(pool)
        .await
        .ip_change
    {
        match foims_auth::smtp::send_device_swap_email(
            pool,
            source_device,
            &workstation_name,
            ip_address,
            old_mac,
            new_mac,
        )
        .await
        {
            Ok(()) => log_info!(
                "log.ip_change.device_swap_email_sent",
                workstation = workstation_name
            ),
            Err(e) => log_error!(
                "log.ip_change.device_swap_email_failed",
                workstation = workstation_name,
                error = e
            ),
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
