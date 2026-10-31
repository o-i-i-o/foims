//! 观测事件模块（snmp_events 落库、站内通知、查询与定位）。

pub mod api;
pub mod notify;

use std::collections::HashSet;

use sqlx::PgPool;
use uuid::Uuid;

use foims_common::{log_error, log_info, log_warn};
use foims_models::{
    SNMP_EVENT_ARP_CONFLICT, SNMP_EVENT_DEVICE_REBOOT, SNMP_EVENT_LINK_FLIP, SNMP_EVENT_POE_OFF,
    SNMP_EVENT_UNKNOWN_DEVICE,
};

use crate::core::truncate_to_column_width;

pub use api::{list_snmp_events, locate_device};
pub use notify::{
    create_notification, encode_notification_content, send_device_swap_notification,
    send_ip_change_notification,
};

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

    /// detail.macs 数组拼接为逗号分隔串：ARP 冲突事件的 MAC 列表在
    /// `macs` 数组而非 `mac` 字段，通知模板的 `{{mac}}` 须取自本函数。
    fn macs_field(&self) -> String {
        self.detail
            .get("macs")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    }
}

/// 是否需要发站内通知（仅关键事件；arp_added/arp_removed/cdp_* 只落表不通知）。
fn should_notify(event_type: &str) -> bool {
    matches!(
        event_type,
        SNMP_EVENT_LINK_FLIP
            | SNMP_EVENT_DEVICE_REBOOT
            | SNMP_EVENT_UNKNOWN_DEVICE
            | SNMP_EVENT_ARP_CONFLICT
            | SNMP_EVENT_POE_OFF
    )
}

/// 该事件对应的通知正文 i18n key（无匹配返回 None）。
fn notification_body_key(event_type: &str) -> Option<&'static str> {
    match event_type {
        SNMP_EVENT_LINK_FLIP => Some("server.notification.snmp_event.body_link_flip"),
        SNMP_EVENT_DEVICE_REBOOT => Some("server.notification.snmp_event.body_reboot"),
        SNMP_EVENT_UNKNOWN_DEVICE => Some("server.notification.snmp_event.body_unknown_device"),
        SNMP_EVENT_ARP_CONFLICT => Some("server.notification.snmp_event.body_arp_conflict"),
        SNMP_EVENT_POE_OFF => Some("server.notification.snmp_event.body_poe_off"),
        _ => None,
    }
}

/// snmp_events.interface 列宽（VARCHAR(100)，与建表契约一致）
const EVENT_INTERFACE_MAX_CHARS: usize = 100;

/// 同源事件抑制窗口（分钟）：同设备 + 同类型 + 同端口 + 同 detail 在窗口内
/// 已落库时不重复落库与通知。冲突/移除等持续性事件在轮询间隔短于其持续
/// 时长时会逐轮重复，会同时放大事件表与通知量；键含 interface，避免不同
/// 端口的同方向事件互相抑制。
const EVENT_SUPPRESSION_WINDOW_MINUTES: i64 = 30;

/// 批量落库观测事件，并对关键事件逐条发站内通知；`unknown_device`
/// 另按「通知配置」中 snmp_event 邮件开关发邮件。
///
/// 事件表为事实源：落库失败即返回错误（由调用方记日志），通知/邮件失败
/// 仅记日志、不影响事件落库。文案用的设备名查询失败降级为 UUID。
pub async fn record_events(
    pool: &PgPool,
    device_id: Uuid,
    events: &[ObsEvent],
) -> Result<(), sqlx::Error> {
    if events.is_empty() {
        return Ok(());
    }

    let device_name: String =
        match sqlx::query_scalar::<_, String>("SELECT name FROM devices WHERE id = $1")
            .bind(device_id)
            .fetch_optional(pool)
            .await
        {
            Ok(Some(name)) => name,
            Ok(None) => device_id.to_string(),
            Err(e) => {
                log_warn!(
                    "log.snmp.device_name_lookup_failed",
                    device_id = device_id,
                    error = e
                );
                device_id.to_string()
            }
        };

    // 抑制过滤：命中窗口内已落库事件或本批重复键的条目丢弃
    let window_start =
        chrono::Utc::now() - chrono::Duration::minutes(EVENT_SUPPRESSION_WINDOW_MINUTES);
    let recent: Vec<(String, Option<String>, serde_json::Value)> = sqlx::query_as(
        "SELECT event_type, interface, detail FROM snmp_events WHERE device_id = $1 AND occurred_at >= $2",
    )
    .bind(device_id)
    .bind(window_start)
    .fetch_all(pool)
    .await?;
    let mut seen_keys: HashSet<(String, String, String)> = recent
        .into_iter()
        .map(|(t, i, d)| (t, i.unwrap_or_default(), d.to_string()))
        .collect();
    let pending: Vec<&ObsEvent> = events
        .iter()
        .filter(|ev| {
            seen_keys.insert((
                ev.event_type.to_string(),
                ev.interface.clone().unwrap_or_default(),
                ev.detail.to_string(),
            ))
        })
        .collect();
    if pending.is_empty() {
        return Ok(());
    }

    // 单语句 UNNEST 批量插入：逐行插入在首轮全量 arp_added 场景下会放大成
    // 数百次网络往返
    let occurred_at = chrono::Utc::now();
    let ids: Vec<Uuid> = pending.iter().map(|_| Uuid::new_v4()).collect();
    let event_types: Vec<&str> = pending.iter().map(|e| e.event_type).collect();
    let interfaces: Vec<Option<String>> = pending
        .iter()
        .map(|e| {
            e.interface
                .as_deref()
                .map(|i| truncate_to_column_width(i, EVENT_INTERFACE_MAX_CHARS))
        })
        .collect();
    let severities: Vec<&str> = pending.iter().map(|e| e.severity.as_str()).collect();
    let details: Vec<&serde_json::Value> = pending.iter().map(|e| &e.detail).collect();

    let rows = sqlx::query(
        r"INSERT INTO snmp_events (id, device_id, event_type, interface, severity, detail, occurred_at, created_at)
          SELECT unnest($3::uuid[]), $1, unnest($4::text[]), unnest($5::text[]), unnest($6::text[]), unnest($7::jsonb[]), $2, $2",
    )
    .bind(device_id)
    .bind(occurred_at)
    .bind(&ids)
    .bind(&event_types)
    .bind(&interfaces)
    .bind(&severities)
    .bind(&details)
    .execute(pool)
    .await?;
    log_info!(
        "log.snmp.event_recorded",
        device_id = device_id,
        count = rows.rows_affected()
    );

    // 通知只发抑制后的集合，逐条降级不回滚已落库事件
    if pending.iter().any(|e| should_notify(e.event_type)) {
        let admin_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM users WHERE enabled = TRUE AND role IN ('admin', 'secadmin')",
        )
        .fetch_all(pool)
        .await?;
        for ev in pending.iter().filter(|e| should_notify(e.event_type)) {
            notify_event(pool, &device_name, ev, &admin_ids).await;
        }
    }

    Ok(())
}

/// 对单条关键事件发站内通知（全体管理员各一条），`unknown_device` 另发邮件。
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
        SNMP_EVENT_UNKNOWN_DEVICE => vec![
            ("device".to_string(), device_name.to_string()),
            ("interface".to_string(), iface.clone()),
            ("ip".to_string(), ev.str_field("ip")),
            ("mac".to_string(), ev.str_field("mac")),
        ],
        SNMP_EVENT_ARP_CONFLICT => vec![
            ("device".to_string(), device_name.to_string()),
            ("interface".to_string(), iface.clone()),
            ("ip".to_string(), ev.str_field("ip")),
            ("mac".to_string(), ev.macs_field()),
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

    if ev.event_type == SNMP_EVENT_UNKNOWN_DEVICE
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
        if let Err(e) = foims_auth::smtp::send_unknown_device_email(
            pool,
            device_name,
            &iface,
            vlan_id,
            &ip,
            &mac,
        )
        .await
        {
            log_warn!("log.snmp.unknown_device_email_failed", error = e);
        }
    }
    // Webhook 渠道（内部受 webhook_snmp_event 开关门控，未配置渠道静默跳过）
    if ev.event_type == SNMP_EVENT_UNKNOWN_DEVICE {
        let vlan_id = ev
            .detail
            .get("vlan_id")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32);
        let ip = ev.str_field("ip");
        let mac = ev.str_field("mac");
        foims_auth::webhook::send_unknown_device_webhook(
            pool,
            device_name,
            &iface,
            vlan_id,
            &ip,
            &mac,
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_notify_covers_key_events() {
        assert!(should_notify(SNMP_EVENT_LINK_FLIP));
        assert!(should_notify(SNMP_EVENT_DEVICE_REBOOT));
        assert!(should_notify(SNMP_EVENT_UNKNOWN_DEVICE));
        assert!(should_notify(SNMP_EVENT_ARP_CONFLICT));
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
        assert_eq!(ev.str_field("absent"), "");
    }

    #[test]
    fn obs_event_macs_field_joins_array() {
        let ev = ObsEvent::new(
            SNMP_EVENT_ARP_CONFLICT,
            None,
            "warning",
            serde_json::json!({ "ip": "10.0.0.1", "macs": ["AA:01", "BB:02"] }),
        );
        assert_eq!(ev.macs_field(), "AA:01, BB:02");
        let plain = ObsEvent::new(
            SNMP_EVENT_UNKNOWN_DEVICE,
            None,
            "warning",
            serde_json::json!({ "mac": "AA:01" }),
        );
        assert_eq!(plain.macs_field(), "");
    }
}
