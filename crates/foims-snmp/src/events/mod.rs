//! 观测事件模块（snmp_events 落库、站内通知、查询与定位）。
//!
//! 第 0 步自 foims-resource 下放通知 helper（create_notification 等），
//! 事件写入/查询 API 在本模块随功能叠加。

pub mod api;
pub mod notify;

use sqlx::PgPool;
use uuid::Uuid;

use foims_common::{log_error, log_info, log_warn};
use foims_models::{
    SNMP_EVENT_ARP_CONFLICT, SNMP_EVENT_DEVICE_REBOOT, SNMP_EVENT_LINK_FLIP, SNMP_EVENT_POE_OFF,
    SNMP_EVENT_UNKNOWN_ENDPOINT,
};

// 通知 helper 与 API handler 一并再导出，供 crate 内/外同路径调用
pub use api::{list_snmp_events, locate_device};
pub use notify::{create_notification, encode_notification_content, send_mac_change_notification};

/// 观测事件载体：检测/比对纯函数产出，[`record_events`] 落库并按需通知。
///
/// `event_type` 取 [`foims_models`] 中的 `SNMP_EVENT_*` 常量；`detail`
/// 为结构化上下文（端口方向/IP/MAC/停机时长等），同时供通知插值。
#[derive(Debug, Clone)]
pub struct ObsEvent {
    pub event_type: &'static str,
    pub interface: Option<String>,
    pub severity: String,
    pub detail: serde_json::Value,
}

impl ObsEvent {
    #[must_use]
    pub fn new(
        event_type: &'static str,
        interface: Option<String>,
        severity: &str,
        detail: serde_json::Value,
    ) -> Self {
        Self {
            event_type,
            interface,
            severity: severity.to_string(),
            detail,
        }
    }

    /// detail 中取字符串字段（缺失/非字符串返回空串，供通知插值）
    fn str_field(&self, key: &str) -> String {
        self.detail
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }
}

/// 是否需要发站内通知（仅关键事件；arp_added/arp_removed/cdp_* 只落表不通知）。
fn should_notify(event_type: &str) -> bool {
    matches!(
        event_type,
        SNMP_EVENT_LINK_FLIP
            | SNMP_EVENT_DEVICE_REBOOT
            | SNMP_EVENT_UNKNOWN_ENDPOINT
            | SNMP_EVENT_ARP_CONFLICT
            | SNMP_EVENT_POE_OFF
    )
}

/// 该事件对应的通知正文 i18n key（无匹配返回 None）。
fn notification_body_key(event_type: &str) -> Option<&'static str> {
    match event_type {
        SNMP_EVENT_LINK_FLIP => Some("server.notification.snmp_event.body_link_flip"),
        SNMP_EVENT_DEVICE_REBOOT => Some("server.notification.snmp_event.body_reboot"),
        SNMP_EVENT_UNKNOWN_ENDPOINT => Some("server.notification.snmp_event.body_unknown_endpoint"),
        SNMP_EVENT_ARP_CONFLICT => Some("server.notification.snmp_event.body_arp_conflict"),
        SNMP_EVENT_POE_OFF => Some("server.notification.snmp_event.body_poe_off"),
        _ => None,
    }
}

/// 批量落库观测事件，并对关键事件逐条发站内通知；`unknown_endpoint`
/// 另按「通知配置」中 snmp_event 邮件开关发邮件。
///
/// 事件表为事实源：落库失败即返回错误（由调用方记日志），通知/邮件失败
/// 仅记日志、不影响事件落库。
pub async fn record_events(
    pool: &PgPool,
    device_id: Uuid,
    events: &[ObsEvent],
) -> Result<(), sqlx::Error> {
    if events.is_empty() {
        return Ok(());
    }

    // 设备名用于通知与邮件文案（查询失败降级为 UUID 字符串）
    let device_name: String =
        sqlx::query_scalar::<_, String>("SELECT name FROM devices WHERE id = $1")
            .bind(device_id)
            .fetch_optional(pool)
            .await?
            .unwrap_or_else(|| device_id.to_string());

    let mut inserted = 0usize;
    for ev in events {
        let occurred_at = chrono::Utc::now();
        sqlx::query(
            r"INSERT INTO snmp_events (id, device_id, event_type, interface, severity, detail, occurred_at, created_at)
              VALUES ($1, $2, $3, $4, $5, $6, $7, $7)",
        )
        .bind(Uuid::new_v4())
        .bind(device_id)
        .bind(ev.event_type)
        .bind(ev.interface.as_deref())
        .bind(&ev.severity)
        .bind(&ev.detail)
        .bind(occurred_at)
        .execute(pool)
        .await?;
        inserted += 1;
    }
    log_info!(
        "log.snmp.event_recorded",
        device_id = device_id,
        count = inserted
    );

    // 通知仅对关键事件；失败逐条降级记日志，不回滚已落库事件
    if events.iter().any(|e| should_notify(e.event_type)) {
        let admin_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM users WHERE enabled = TRUE AND role IN ('admin', 'secadmin')",
        )
        .fetch_all(pool)
        .await?;
        for ev in events.iter().filter(|e| should_notify(e.event_type)) {
            notify_event(pool, &device_name, ev, &admin_ids).await;
        }
    }

    Ok(())
}

/// 对单条关键事件发站内通知（全体管理员各一条），`unknown_endpoint` 另发邮件。
async fn notify_event(pool: &PgPool, device_name: &str, ev: &ObsEvent, admin_ids: &[Uuid]) {
    let Some(body_key) = notification_body_key(ev.event_type) else {
        return;
    };
    let iface = ev.interface.clone().unwrap_or_default();
    let params: Vec<(String, String)> = match ev.event_type {
        SNMP_EVENT_LINK_FLIP => vec![
            ("device".to_string(), device_name.to_string()),
            ("interface".to_string(), iface.clone()),
            ("direction".to_string(), ev.str_field("direction")),
        ],
        SNMP_EVENT_DEVICE_REBOOT => vec![
            ("device".to_string(), device_name.to_string()),
            ("downtime".to_string(), ev.str_field("downtime")),
        ],
        SNMP_EVENT_UNKNOWN_ENDPOINT | SNMP_EVENT_ARP_CONFLICT => vec![
            ("device".to_string(), device_name.to_string()),
            ("interface".to_string(), iface.clone()),
            ("ip".to_string(), ev.str_field("ip")),
            ("mac".to_string(), ev.str_field("mac")),
        ],
        SNMP_EVENT_POE_OFF => vec![
            ("device".to_string(), device_name.to_string()),
            ("interface".to_string(), iface.clone()),
            (
                "class".to_string(),
                ev.detail
                    .get("power_class")
                    .and_then(|v| v.as_i64())
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
            ),
        ],
        _ => vec![("device".to_string(), device_name.to_string())],
    };
    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let content = encode_notification_content(body_key, &param_refs);

    for admin_id in admin_ids {
        if let Err(e) = create_notification(
            pool,
            "server.notification.snmp_event.title",
            &content,
            "snmp_event",
            Some(admin_id),
        )
        .await
        {
            log_error!(
                "log.snmp.notification_failed",
                event_type = ev.event_type,
                error = e
            );
        }
    }

    // 邮件仅未知终端场景，且受 snmp_event 方式开关门控
    if ev.event_type == SNMP_EVENT_UNKNOWN_ENDPOINT
        && foims_auth::smtp::load_notification_methods(pool)
            .await
            .snmp_event
    {
        let vlan_id = ev
            .detail
            .get("vlan_id")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32);
        let ip = ev.str_field("ip");
        let mac = ev.str_field("mac");
        if let Err(e) = foims_auth::smtp::send_unknown_endpoint_email(
            pool,
            device_name,
            &iface,
            vlan_id,
            &ip,
            &mac,
        )
        .await
        {
            log_warn!("log.snmp.unknown_endpoint_email_failed", error = e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_notify_covers_key_events() {
        assert!(should_notify(SNMP_EVENT_LINK_FLIP));
        assert!(should_notify(SNMP_EVENT_DEVICE_REBOOT));
        assert!(should_notify(SNMP_EVENT_UNKNOWN_ENDPOINT));
        assert!(should_notify(SNMP_EVENT_ARP_CONFLICT));
        // 高频/低价值事件只落表不通知
        assert!(!should_notify("arp_added"));
        assert!(!should_notify("arp_removed"));
    }

    #[test]
    fn notification_body_key_matches_types() {
        assert_eq!(
            notification_body_key(SNMP_EVENT_LINK_FLIP),
            Some("server.notification.snmp_event.body_link_flip")
        );
        assert_eq!(notification_body_key("arp_added"), None);
    }

    #[test]
    fn obs_event_str_field_reads_detail() {
        let ev = ObsEvent::new(
            SNMP_EVENT_LINK_FLIP,
            Some("ge1/0/1".to_string()),
            "warning",
            serde_json::json!({ "direction": "down" }),
        );
        assert_eq!(ev.str_field("direction"), "down");
        // 缺失字段返回空串而非 panic
        assert_eq!(ev.str_field("absent"), "");
    }
}
