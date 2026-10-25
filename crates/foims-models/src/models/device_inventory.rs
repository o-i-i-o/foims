//! SNMP 第二层网络侧清单/台账共享类型：6 类设备台账的数据库行模型
//!（`FromRow` + `Serialize` + `ts_rs::TS`，供 GET 读库与同步回填）与
//! 采集原始记录（供 foims-snmp::collect 返回、资源域 handler 落库）。
//!
//! 设计口径与第一层 LLDP/ARP 完全同构：
//! - 采集原始类型（`*Record`）只含 SNMP 可得的数据列，不含 id/device_id/
//!   时间戳——它们由落库 handler 补齐；
//! - 数据库行类型（`Device*`）与 `schema/tables/device_inventory.rs` 建表
//!   列一一对应，JSONB 列（成员端口 / 聚合成员）用 `serde_json::Value`
//!   承载（ts-rs 生成 `unknown`，前端按数组收窄）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ==================== ENTITY-MIB 物理实体 ====================

/// `entPhysicalTable` 采集原始记录（落库前由 handler 补齐 id/device_id/时间）。
#[derive(Debug, Clone)]
pub struct EntityRecord {
    pub ent_index: i32,
    pub ent_class: Option<String>,
    pub name: Option<String>,
    pub descr: Option<String>,
    pub model_name: Option<String>,
    pub serial_num: Option<String>,
    pub mfg_name: Option<String>,
    pub hw_rev: Option<String>,
    pub fw_rev: Option<String>,
    pub sw_rev: Option<String>,
    pub is_fru: Option<bool>,
    pub parent_index: Option<i32>,
}

/// `device_entities` 表行模型：设备硬件物理实体清单（含序列号/固件版本等）。
#[derive(Debug, Serialize, Deserialize, Clone, FromRow, ts_rs::TS)]
#[ts(export)]
pub struct DeviceEntity {
    pub id: Uuid,
    pub device_id: Uuid,
    pub ent_index: i32,
    pub ent_class: Option<String>,
    pub name: Option<String>,
    pub descr: Option<String>,
    pub model_name: Option<String>,
    pub serial_num: Option<String>,
    pub mfg_name: Option<String>,
    pub hw_rev: Option<String>,
    pub fw_rev: Option<String>,
    pub sw_rev: Option<String>,
    pub is_fru: Option<bool>,
    pub parent_index: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ==================== ENTITY-SENSOR-MIB 传感器 ====================

/// `entSensorTable` 采集原始记录。
#[derive(Debug, Clone)]
pub struct SensorRecord {
    pub ent_index: i32,
    pub sensor_type: Option<i32>,
    pub sensor_scale: Option<i32>,
    pub sensor_precision: Option<i32>,
    pub sensor_value: Option<f64>,
    pub sensor_status: Option<i32>,
    pub name: Option<String>,
}

/// `device_sensors` 表行模型：设备传感器读数（温度/电压等，当前态）。
#[derive(Debug, Serialize, Deserialize, Clone, FromRow, ts_rs::TS)]
#[ts(export)]
pub struct DeviceSensor {
    pub id: Uuid,
    pub device_id: Uuid,
    pub ent_index: i32,
    pub sensor_type: Option<i32>,
    pub sensor_scale: Option<i32>,
    pub sensor_precision: Option<i32>,
    pub sensor_value: Option<f64>,
    pub sensor_status: Option<i32>,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ==================== Q-BRIDGE-MIB VLAN 台账 ====================

/// `dot1qVlanStaticTable` 采集原始记录（成员为 ifIndex 数组）。
#[derive(Debug, Clone)]
pub struct VlanRecord {
    pub vlan_id: i32,
    pub name: Option<String>,
    pub member_ports: Vec<u32>,
    pub untagged_ports: Vec<u32>,
}

/// `device_vlans` 表行模型：设备 VLAN 台账（成员端口以 JSONB 数组落库）。
#[derive(Debug, Serialize, Deserialize, Clone, FromRow, ts_rs::TS)]
#[ts(export)]
pub struct DeviceVlan {
    pub id: Uuid,
    pub device_id: Uuid,
    pub vlan_id: i32,
    pub name: Option<String>,
    #[ts(type = "unknown")]
    pub member_ports: serde_json::Value,
    #[ts(type = "unknown")]
    pub untagged_ports: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ==================== CISCO-CDP-MIB 邻居 ====================

/// `cdpCacheTable` 采集原始记录。
#[derive(Debug, Clone)]
pub struct CdpRecord {
    pub local_port: String,
    pub neighbor_device_id: Option<String>,
    pub neighbor_port: Option<String>,
    pub neighbor_address: Option<String>,
    pub neighbor_platform: Option<String>,
}

/// `device_cdps` 表行模型：设备 CDP 邻居（Cisco 专有，非 Cisco 设备空集）。
#[derive(Debug, Serialize, Deserialize, Clone, FromRow, ts_rs::TS)]
#[ts(export)]
pub struct DeviceCdp {
    pub id: Uuid,
    pub device_id: Uuid,
    pub local_port: String,
    pub neighbor_device_id: Option<String>,
    pub neighbor_port: Option<String>,
    pub neighbor_address: Option<String>,
    pub neighbor_platform: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ==================== IEEE8023-LAG-MIB 聚合口 ====================

/// `dot3adAggPortTable` 聚合口采集原始记录。
#[derive(Debug, Clone)]
pub struct LagRecord {
    pub agg_ifindex: i32,
    pub agg_name: Option<String>,
    pub member_ifindexes: Vec<u32>,
    pub member_names: Vec<String>,
    pub lacp_state: Option<String>,
}

/// `device_lags` 表行模型：设备 LACP 聚合口台账（成员以 JSONB 数组落库）。
#[derive(Debug, Serialize, Deserialize, Clone, FromRow, ts_rs::TS)]
#[ts(export)]
pub struct DeviceLag {
    pub id: Uuid,
    pub device_id: Uuid,
    pub agg_ifindex: i32,
    pub agg_name: Option<String>,
    #[ts(type = "unknown")]
    pub member_ifindexes: serde_json::Value,
    #[ts(type = "unknown")]
    pub member_names: serde_json::Value,
    pub lacp_state: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ==================== PFN-MIB PoE 供电端口 ====================

/// `pethPsePortTable` 采集原始记录。
#[derive(Debug, Clone)]
pub struct PoeRecord {
    pub if_index: i32,
    pub if_name: Option<String>,
    pub admin_status: Option<i32>,
    pub detection_status: Option<i32>,
    pub power_class: Option<i32>,
    pub measured_power_w: Option<f64>,
}

/// `device_poe_ports` 表行模型：设备 PoE 供电端口当前态。
#[derive(Debug, Serialize, Deserialize, Clone, FromRow, ts_rs::TS)]
#[ts(export)]
pub struct DevicePoePort {
    pub id: Uuid,
    pub device_id: Uuid,
    pub if_index: i32,
    pub if_name: Option<String>,
    pub admin_status: Option<i32>,
    pub detection_status: Option<i32>,
    pub power_class: Option<i32>,
    pub measured_power_w: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
