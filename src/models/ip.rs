//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== IP 查询模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct IpManager {
    pub id: Uuid,
    pub device_interface_id: Uuid,
    #[sqlx(default)]
    pub device_id: Uuid,
    pub network_id: Option<Uuid>,
    #[sqlx(default)]
    pub network_region_id: Option<Uuid>,
    #[sqlx(default)]
    pub network_name: Option<String>,
    #[sqlx(default)]
    pub network_region: Option<String>,
    pub ip_address: String,
    pub ip_version: i16,
    #[sqlx(default)]
    pub mac_address: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub last_seen: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct IpManagerWithNames {
    pub id: Uuid,
    pub device_interface_id: Uuid,
    pub device_id: Uuid,
    pub device_type: Option<String>,
    pub device_name: Option<String>,
    pub network_id: Option<Uuid>,
    pub workstation_name: Option<String>,
    pub cabinet_position_name: Option<String>,
    pub interface_name: Option<String>,
    pub physical_type: Option<String>,
    pub interface_role: Option<String>,
    pub room_name: Option<String>,
    pub cabinet_name: Option<String>,
    pub org_name: Option<String>,
    pub network_name: String,
    pub network_region: String,
    pub ip_address: String,
    pub ip_version: i16,
    pub mac_address: Option<String>,
    pub hostname: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub last_seen: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// 设备绑定的机位 ID（机柜可视化按机位批量过滤 IP）。
    pub position_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct IpManagerCreate {
    pub device_interface_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub network_id: Option<Uuid>,
    #[validate(custom(
        function = "crate::models::validate_ip_address",
        message = "server.ip.validation.ip_address_invalid"
    ))]
    pub ip_address: String,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

// ==================== 设备网卡配置同步模型（网卡 → 网口 → IP） ====================

#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct IpSyncItem {
    pub id: Option<Uuid>,
    pub network_id: Option<Uuid>,
    #[validate(custom(
        function = "crate::models::validate_ip_address",
        message = "server.ip.validation.ip_address_invalid"
    ))]
    pub ip_address: String,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct PortSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(
        min = 1,
        max = 50,
        message = "server.device.validation.interface_name_length"
    ))]
    pub name: String,
    pub physical_type: Option<String>,
    pub interface_role: Option<String>,
    #[validate(length(max = 20, message = "server.device.validation.mac_length"))]
    pub mac_address: Option<String>,
    pub vlan_id: Option<i32>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
    #[serde(default)]
    pub ips: Vec<IpSyncItem>,
}

#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct NetworkCardSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(
        min = 1,
        max = 50,
        message = "server.device.validation.nic_name_length"
    ))]
    pub name: String,
    pub card_type: Option<String>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
    #[serde(default)]
    pub ports: Vec<PortSyncItem>,
}

#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct DeviceNetworkConfigSync {
    #[serde(default)]
    pub cards: Vec<NetworkCardSyncItem>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct AutoAssignIpRequest {
    pub network_id: Uuid,
    pub device_interface_id: Option<Uuid>,
    pub device_id: Uuid,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct PullIpManagersRequest {
    pub device_id: Uuid,
    pub network_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct IpManagerUpdate {
    #[serde(default)]
    pub device_interface_id: Option<Option<Uuid>>,
    pub ip_address: Option<String>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
    #[validate(length(max = 20, message = "server.ip.validation.status_length"))]
    pub status: Option<String>,
    pub ip_version: Option<i16>,
}
