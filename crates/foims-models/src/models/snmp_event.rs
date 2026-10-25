//! SNMP 第一层观测事件共享类型：事件行模型、列表查询、终端定位结果，
//! 以及集中定义的事件类型常量（避免字符串散落）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ==================== 事件类型常量 ====================

/// 端口链路翻动（up↔down）
pub const SNMP_EVENT_LINK_FLIP: &str = "link_flip";
/// 设备重启（sysUpTime 回退）
pub const SNMP_EVENT_DEVICE_REBOOT: &str = "device_reboot";
/// ARP/FDB 新增条目
pub const SNMP_EVENT_ARP_ADDED: &str = "arp_added";
/// ARP/FDB 移除条目
pub const SNMP_EVENT_ARP_REMOVED: &str = "arp_removed";
/// ARP/FDB 冲突（同 IP 对应多 MAC 或反之）
pub const SNMP_EVENT_ARP_CONFLICT: &str = "arp_conflict";
/// 未登记终端（同步 IP 未匹配任何 interface_ips 且为新增）
pub const SNMP_EVENT_UNKNOWN_ENDPOINT: &str = "unknown_endpoint";
/// PoE 端口供电中断（detection_status 由 powering 转为非供电态）
pub const SNMP_EVENT_POE_OFF: &str = "poe_port_off";
/// CDP 邻居新增
pub const SNMP_EVENT_CDP_ADDED: &str = "cdp_neighbor_added";
/// CDP 邻居消失
pub const SNMP_EVENT_CDP_REMOVED: &str = "cdp_neighbor_removed";

/// 事件严重级别常量（与 snmp_events.severity CHECK 值域一致）。
pub const SNMP_SEVERITY_INFO: &str = "info";
pub const SNMP_SEVERITY_WARNING: &str = "warning";
pub const SNMP_SEVERITY_CRITICAL: &str = "critical";

// ==================== 事件行模型 ====================

/// `snmp_events` 表行模型：轮询/MAC 同步产出的观测事件（事实源）。
#[derive(Debug, Serialize, Deserialize, Clone, FromRow, ts_rs::TS)]
#[ts(export)]
pub struct SnmpEvent {
    pub id: Uuid,
    pub device_id: Uuid,
    pub event_type: String,
    pub interface: Option<String>,
    pub severity: String,
    pub detail: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// GET /api/snmp/events 分页列表查询参数。
#[derive(Debug, Serialize, Deserialize, Clone, ts_rs::TS)]
#[ts(export)]
pub struct SnmpEventQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub device_id: Option<Uuid>,
    pub event_type: Option<String>,
    pub severity: Option<String>,
    /// ISO-8601 起始时间（含），按 occurred_at 过滤
    pub start: Option<String>,
    /// ISO-8601 结束时间（含），按 occurred_at 过滤
    pub end: Option<String>,
}

// ==================== 终端定位结果 ====================

/// GET /api/snmp/locate 定位命中行：按 MAC/IP 关联设备、端口、VLAN、登记状态。
#[derive(Debug, Serialize, Deserialize, Clone, FromRow, ts_rs::TS)]
#[ts(export)]
pub struct DeviceLocateResult {
    pub device_id: Uuid,
    pub device_name: Option<String>,
    pub interface: Option<String>,
    pub vlan_id: Option<i32>,
    pub ip_address: Option<String>,
    pub mac_address: Option<String>,
    /// 是否在 interface_ips 登记（true=已登记资产，false=未知终端）
    pub registered: bool,
    /// 命中来源：arp（ARP/FDB 学到）/ static（网口 mac_address 直查）
    pub source: String,
    pub occurred_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}
