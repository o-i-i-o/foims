//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use super::ip::NetworkCardSyncItem;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== 交换机端口模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DevicePort {
    pub id: Uuid,
    pub device_id: Uuid,
    pub port_number: String,
    pub port_name: Option<String>,
    pub port_type: String,
    pub vlan_id: Option<i32>,
    pub status: String,
    pub speed: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DevicePortWithDevice {
    pub id: Uuid,
    pub device_id: Uuid,
    pub device_name: String,
    pub device_ip: Option<String>,
    #[sqlx(default)]
    pub device_network_name: Option<String>,
    #[sqlx(default)]
    pub device_network_region: Option<String>,
    pub port_number: String,
    pub port_name: Option<String>,
    pub port_type: String,
    pub vlan_id: Option<i32>,
    pub status: String,
    pub speed: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct DevicePortCreate {
    #[validate(length(min = 1, max = 30, message = "端口号长度必须在1到30个字符之间"))]
    pub port_number: String,
    #[validate(length(max = 50, message = "端口名称长度不能超过50个字符"))]
    pub port_name: Option<String>,
    pub port_type: Option<String>,
    pub vlan_id: Option<i32>,
    pub status: Option<String>,
    #[validate(length(max = 20, message = "速率长度不能超过20个字符"))]
    pub speed: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct DevicePortUpdate {
    #[validate(length(min = 1, max = 30, message = "端口号长度必须在1到30个字符之间"))]
    pub port_number: Option<String>,
    #[validate(length(max = 50, message = "端口名称长度不能超过50个字符"))]
    pub port_name: Option<String>,
    pub port_type: Option<String>,
    pub vlan_id: Option<i32>,
    pub status: Option<String>,
    #[validate(length(max = 20, message = "速率长度不能超过20个字符"))]
    pub speed: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

// ==================== 设备网卡模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct NetworkCard {
    pub id: Uuid,
    pub device_id: Uuid,
    pub name: String,
    pub card_type: String,
    pub description: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetworkCardCreate {
    #[validate(length(min = 1, max = 50, message = "网卡名称长度必须在1到50个字符之间"))]
    pub name: String,
    pub card_type: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetworkCardUpdate {
    #[validate(length(min = 1, max = 50, message = "网卡名称长度必须在1到50个字符之间"))]
    pub name: Option<String>,
    pub card_type: Option<String>,
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    pub description: Option<Option<String>>,
}

// ==================== 设备三层接口/网口模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceInterface {
    pub id: Uuid,
    pub device_id: Uuid,
    pub nic_id: Option<Uuid>,
    pub name: String,
    pub physical_type: String,
    pub interface_role: String,
    pub mac_address: Option<String>,
    pub vlan_id: Option<i32>,
    pub description: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceInterfaceWithDevice {
    pub id: Uuid,
    pub device_id: Uuid,
    pub device_name: String,
    pub nic_id: Option<Uuid>,
    pub name: String,
    pub physical_type: String,
    pub interface_role: String,
    pub mac_address: Option<String>,
    pub vlan_id: Option<i32>,
    pub description: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct DeviceInterfaceCreate {
    #[validate(length(min = 1, max = 50, message = "接口名称长度必须在1到50个字符之间"))]
    pub name: String,
    pub physical_type: Option<String>,
    pub interface_role: Option<String>,
    #[validate(length(max = 20, message = "MAC地址长度不能超过20个字符"))]
    pub mac_address: Option<String>,
    pub vlan_id: Option<i32>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct DeviceInterfaceUpdate {
    #[validate(length(min = 1, max = 50, message = "接口名称长度必须在1到50个字符之间"))]
    pub name: Option<String>,
    pub physical_type: Option<String>,
    pub interface_role: Option<String>,
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    pub mac_address: Option<Option<String>>,
    pub vlan_id: Option<i32>,
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    pub description: Option<Option<String>>,
}

// ==================== SNMP 相关模型 ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct SnmpTestRequest {
    pub device_id: Option<Uuid>,
    pub ip_address: Option<String>,
    pub snmp_version: Option<String>,
    pub snmp_community: Option<String>,
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    pub snmp_priv_password: Option<String>,
    pub snmp_port: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ArpEntry {
    pub ip_address: String,
    pub mac_address: String,
    pub interface: Option<String>,
    pub vlan_id: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LldpNeighbor {
    pub local_port: String,
    pub neighbor_chassis_id: Option<String>,
    pub neighbor_port_id: Option<String>,
    pub neighbor_port_desc: Option<String>,
    pub neighbor_sys_name: Option<String>,
    pub neighbor_sys_desc: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceMac {
    pub id: Uuid,
    pub device_id: Uuid,
    pub ip_address: String,
    pub mac_address: String,
    pub interface: Option<String>,
    pub vlan_id: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceMacCreate {
    pub ip_address: String,
    pub mac_address: String,
    pub interface: Option<String>,
    pub vlan_id: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceLldp {
    pub id: Uuid,
    pub device_id: Uuid,
    pub local_port: String,
    pub neighbor_chassis_id: Option<String>,
    pub neighbor_port_id: Option<String>,
    pub neighbor_port_desc: Option<String>,
    pub neighbor_sys_name: Option<String>,
    pub neighbor_sys_desc: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceLldpCreate {
    pub local_port: String,
    pub neighbor_chassis_id: Option<String>,
    pub neighbor_port_id: Option<String>,
    pub neighbor_port_desc: Option<String>,
    pub neighbor_sys_name: Option<String>,
    pub neighbor_sys_desc: Option<String>,
}

// ==================== 设备模板模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceTemplate {
    pub id: Uuid,
    pub name: String,
    pub device_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceTemplateSummary {
    pub id: Uuid,
    pub name: String,
    pub device_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateDeviceTemplateRequest {
    #[validate(length(min = 1, max = 100, message = "模板名称不能为空且不超过100个字符"))]
    pub name: String,
    #[validate(length(min = 1, max = 30, message = "设备类型不能为空"))]
    pub device_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub description: Option<String>,
}

// ==================== 设备模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Device {
    pub id: Uuid,
    pub name: String,
    pub device_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub workstation_id: Option<Uuid>,
    pub position_id: Option<Uuid>,
    pub room_id: Uuid,
    pub template_id: Option<Uuid>,
    pub vendor: Option<String>,
    pub location: Option<String>,
    pub snmp_version: String,
    pub snmp_community: Option<String>,
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    pub snmp_priv_password: Option<String>,
    pub snmp_port: i32,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceWithDetails {
    pub id: Uuid,
    pub name: String,
    pub device_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub workstation_id: Option<Uuid>,
    pub workstation_name: Option<String>,
    pub position_id: Option<Uuid>,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub cabinet_id: Option<Uuid>,
    pub cabinet_name: Option<String>,
    pub start_u: Option<i32>,
    pub end_u: Option<i32>,
    pub template_id: Option<Uuid>,
    pub template_name: Option<String>,
    pub vendor: Option<String>,
    pub location: Option<String>,
    pub snmp_version: Option<String>,
    pub snmp_community: Option<String>,
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    pub snmp_priv_password: Option<String>,
    pub snmp_port: Option<i32>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct DeviceCreate {
    #[validate(length(min = 1, max = 100, message = "设备名称长度必须在1到100个字符之间"))]
    pub name: String,
    #[validate(custom(
        function = "crate::models::validate_device_type_option",
        message = "设备类型必须是pc/laptop/printer/server/network_device/switch/camera/phone/other"
    ))]
    pub device_type: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub workstation_id: Option<Uuid>,
    pub position_id: Option<Uuid>,
    pub room_id: Uuid,
    pub template_id: Option<Uuid>,
    #[validate(length(max = 50, message = "厂商长度不能超过50个字符"))]
    pub vendor: Option<String>,
    #[validate(length(max = 100, message = "位置长度不能超过100个字符"))]
    pub location: Option<String>,
    pub snmp_version: Option<String>,
    #[validate(length(max = 100, message = "SNMP团体字符串长度不能超过100个字符"))]
    pub snmp_community: Option<String>,
    #[validate(length(max = 50, message = "SNMP用户名长度不能超过50个字符"))]
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    #[validate(length(max = 100, message = "SNMP认证密码长度不能超过100个字符"))]
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    #[validate(length(max = 100, message = "SNMP隐私密码长度不能超过100个字符"))]
    pub snmp_priv_password: Option<String>,
    pub snmp_port: Option<i32>,
    pub cards: Option<Vec<NetworkCardSyncItem>>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    pub save_as_template: Option<bool>,
    #[validate(length(max = 100, message = "模板名称长度不能超过100个字符"))]
    pub template_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct DeviceUpdate {
    #[validate(length(min = 1, max = 100, message = "设备名称长度必须在1到100个字符之间"))]
    pub name: Option<String>,
    #[validate(custom(
        function = "crate::models::validate_device_type_option",
        message = "设备类型必须是pc/laptop/printer/server/network_device/switch/camera/phone/other"
    ))]
    pub device_type: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    pub workstation_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    pub position_id: Option<Option<Uuid>>,
    pub room_id: Option<Uuid>,
    #[validate(length(max = 50, message = "厂商长度不能超过50个字符"))]
    pub vendor: Option<String>,
    #[validate(length(max = 100, message = "位置长度不能超过100个字符"))]
    pub location: Option<String>,
    pub snmp_version: Option<String>,
    #[validate(length(max = 100, message = "SNMP团体字符串长度不能超过100个字符"))]
    pub snmp_community: Option<String>,
    #[validate(length(max = 50, message = "SNMP用户名长度不能超过50个字符"))]
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    #[validate(length(max = 100, message = "SNMP认证密码长度不能超过100个字符"))]
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    #[validate(length(max = 100, message = "SNMP隐私密码长度不能超过100个字符"))]
    pub snmp_priv_password: Option<String>,
    pub snmp_port: Option<i32>,
    pub cards: Option<Vec<NetworkCardSyncItem>>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    pub save_as_template: Option<bool>,
    #[validate(length(max = 100, message = "模板名称长度不能超过100个字符"))]
    pub template_name: Option<String>,
}
