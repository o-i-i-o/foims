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
    #[validate(length(
        min = 1,
        max = 30,
        message = "server.device.validation.port_number_length"
    ))]
    pub port_number: String,
    #[validate(length(max = 50, message = "server.device.validation.port_name_length"))]
    pub port_name: Option<String>,
    pub port_type: Option<String>,
    pub vlan_id: Option<i32>,
    pub status: Option<String>,
    #[validate(length(max = 20, message = "server.device.validation.speed_length"))]
    pub speed: Option<String>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct DevicePortUpdate {
    #[validate(length(
        min = 1,
        max = 30,
        message = "server.device.validation.port_number_length"
    ))]
    pub port_number: Option<String>,
    #[validate(length(max = 50, message = "server.device.validation.port_name_length"))]
    pub port_name: Option<String>,
    pub port_type: Option<String>,
    pub vlan_id: Option<i32>,
    pub status: Option<String>,
    #[validate(length(max = 20, message = "server.device.validation.speed_length"))]
    pub speed: Option<String>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
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
    #[validate(length(
        min = 1,
        max = 50,
        message = "server.device.validation.nic_name_length"
    ))]
    pub name: String,
    pub card_type: Option<String>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetworkCardUpdate {
    #[validate(length(
        min = 1,
        max = 50,
        message = "server.device.validation.nic_name_length"
    ))]
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
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct DeviceInterfaceUpdate {
    #[validate(length(
        min = 1,
        max = 50,
        message = "server.device.validation.interface_name_length"
    ))]
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
    #[validate(length(
        min = 1,
        max = 100,
        message = "server.device_template.validation.name_length"
    ))]
    pub name: String,
    #[validate(length(
        min = 1,
        max = 30,
        message = "server.device_template.validation.device_type_length"
    ))]
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
    pub hostname: Option<String>,
    pub device_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub workstation_id: Option<Uuid>,
    pub position_id: Option<Uuid>,
    pub room_id: Uuid,
    pub template_id: Option<Uuid>,
    pub seller: Option<String>,
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
    pub hostname: Option<String>,
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
    pub seller: Option<String>,
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
    #[validate(length(min = 1, max = 100, message = "server.device.validation.name_length"))]
    pub name: String,
    #[validate(length(max = 100, message = "server.device.validation.hostname_length"))]
    pub hostname: Option<String>,
    #[validate(custom(
        function = "crate::models::validate_device_type_option",
        message = "server.device.validation.type_invalid"
    ))]
    pub device_type: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub workstation_id: Option<Uuid>,
    pub position_id: Option<Uuid>,
    pub room_id: Uuid,
    pub template_id: Option<Uuid>,
    #[validate(length(max = 50, message = "server.device.validation.seller_length"))]
    pub seller: Option<String>,
    #[validate(length(max = 100, message = "server.device.validation.location_length"))]
    pub location: Option<String>,
    pub snmp_version: Option<String>,
    #[validate(length(max = 100, message = "server.device.validation.snmp_community_length"))]
    pub snmp_community: Option<String>,
    #[validate(length(max = 50, message = "server.device.validation.snmp_username_length"))]
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    #[validate(length(
        max = 100,
        message = "server.device.validation.snmp_auth_password_length"
    ))]
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    #[validate(length(
        max = 100,
        message = "server.device.validation.snmp_priv_password_length"
    ))]
    pub snmp_priv_password: Option<String>,
    pub snmp_port: Option<i32>,
    pub cards: Option<Vec<NetworkCardSyncItem>>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
    pub save_as_template: Option<bool>,
    #[validate(length(max = 100, message = "server.device.validation.template_name_length"))]
    pub template_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct DeviceUpdate {
    #[validate(length(min = 1, max = 100, message = "server.device.validation.name_length"))]
    pub name: Option<String>,
    #[validate(length(max = 100, message = "server.device.validation.hostname_length"))]
    pub hostname: Option<String>,
    #[validate(custom(
        function = "crate::models::validate_device_type_option",
        message = "server.device.validation.type_invalid"
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
    #[validate(length(max = 50, message = "server.device.validation.seller_length"))]
    pub seller: Option<String>,
    #[validate(length(max = 100, message = "server.device.validation.location_length"))]
    pub location: Option<String>,
    pub snmp_version: Option<String>,
    #[validate(length(max = 100, message = "server.device.validation.snmp_community_length"))]
    pub snmp_community: Option<String>,
    #[validate(length(max = 50, message = "server.device.validation.snmp_username_length"))]
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    #[validate(length(
        max = 100,
        message = "server.device.validation.snmp_auth_password_length"
    ))]
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    #[validate(length(
        max = 100,
        message = "server.device.validation.snmp_priv_password_length"
    ))]
    pub snmp_priv_password: Option<String>,
    pub snmp_port: Option<i32>,
    pub cards: Option<Vec<NetworkCardSyncItem>>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
    pub save_as_template: Option<bool>,
    #[validate(length(max = 100, message = "server.device.validation.template_name_length"))]
    pub template_name: Option<String>,
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    // ---------- 设备端口 ----------

    #[test]
    fn test_device_port_create_valid() -> Result<(), serde_json::Error> {
        let req: DevicePortCreate = serde_json::from_value(serde_json::json!({
            "port_number": "G1/0/1",
            "port_name": "上联口",
            "port_type": "ge",
            "vlan_id": 100,
            "status": "up",
            "speed": "1G",
            "description": "核心上联"
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_device_port_create_port_number_length() -> Result<(), serde_json::Error> {
        // 端口号必填且最长 30 字符
        let empty: DevicePortCreate =
            serde_json::from_value(serde_json::json!({ "port_number": "" }))?;
        let Err(errors) = empty.validate() else {
            panic!("空端口号应被拒绝");
        };
        assert!(errors.errors().contains_key("port_number"));

        let long: DevicePortCreate = serde_json::from_value(serde_json::json!({
            "port_number": "P".repeat(31)
        }))?;
        let Err(errors2) = long.validate() else {
            panic!("超长端口号应被拒绝");
        };
        assert!(errors2.errors().contains_key("port_number"));
        Ok(())
    }

    #[test]
    fn test_device_port_update_invalid_fields() -> Result<(), serde_json::Error> {
        // 端口号超长与速度超长同时报错
        let req: DevicePortUpdate = serde_json::from_value(serde_json::json!({
            "port_number": "P".repeat(31),
            "speed": "S".repeat(21)
        }))?;
        let Err(errors) = req.validate() else {
            panic!("非法端口更新应被拒绝");
        };
        assert!(errors.errors().contains_key("port_number"));
        assert!(errors.errors().contains_key("speed"));
        Ok(())
    }

    // ---------- 设备网卡 ----------

    #[test]
    fn test_network_card_create_valid_and_invalid() -> Result<(), serde_json::Error> {
        let ok: NetworkCardCreate = serde_json::from_value(serde_json::json!({
            "name": "eth0",
            "card_type": "onboard",
            "description": "板载网卡"
        }))?;
        assert!(ok.validate().is_ok());

        let bad: NetworkCardCreate = serde_json::from_value(serde_json::json!({ "name": "" }))?;
        let Err(errors) = bad.validate() else {
            panic!("空网卡名应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        Ok(())
    }

    #[test]
    fn test_network_card_update_description_three_states() -> Result<(), serde_json::Error> {
        // 字段缺失 → None（不修改）
        let missing: NetworkCardUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert_eq!(missing.description, None);

        // JSON null → Some(None)（清除描述）
        let null_desc: NetworkCardUpdate = serde_json::from_value(serde_json::json!({
            "description": null
        }))?;
        assert_eq!(null_desc.description, Some(None));

        // JSON 值 → Some(Some(v))（设置新描述）
        let set_desc: NetworkCardUpdate = serde_json::from_value(serde_json::json!({
            "description": "新描述"
        }))?;
        assert_eq!(set_desc.description, Some(Some("新描述".to_string())));
        assert!(set_desc.validate().is_ok());
        Ok(())
    }

    // ---------- 设备接口 ----------

    #[test]
    fn test_device_interface_create_mac_too_long() -> Result<(), serde_json::Error> {
        let req: DeviceInterfaceCreate = serde_json::from_value(serde_json::json!({
            "name": "eth0",
            "mac_address": "AA:BB:CC:DD:EE:FF:00:11"
        }))?;
        let Err(errors) = req.validate() else {
            panic!("超长 MAC 应被拒绝");
        };
        assert!(errors.errors().contains_key("mac_address"));
        Ok(())
    }

    #[test]
    fn test_device_interface_create_valid() -> Result<(), serde_json::Error> {
        let req: DeviceInterfaceCreate = serde_json::from_value(serde_json::json!({
            "name": "eth0",
            "physical_type": "electrical",
            "interface_role": "management",
            "mac_address": "AA:BB:CC:DD:EE:FF",
            "vlan_id": 10
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_device_interface_update_null_semantics() -> Result<(), serde_json::Error> {
        // mac_address 与 description 均为双层 Option：null 表示清除
        let req: DeviceInterfaceUpdate = serde_json::from_value(serde_json::json!({
            "mac_address": null,
            "description": "新描述"
        }))?;
        assert_eq!(req.mac_address, Some(None));
        assert_eq!(req.description, Some(Some("新描述".to_string())));
        assert!(req.validate().is_ok());

        // 字段全部缺失 → 双层均为 None
        let empty: DeviceInterfaceUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert_eq!(empty.mac_address, None);
        assert_eq!(empty.description, None);
        Ok(())
    }

    #[test]
    fn test_device_interface_update_invalid() -> Result<(), serde_json::Error> {
        // 名称超长拒绝
        let req: DeviceInterfaceUpdate = serde_json::from_value(serde_json::json!({
            "name": "N".repeat(51)
        }))?;
        let Err(errors) = req.validate() else {
            panic!("非法接口更新应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        Ok(())
    }

    #[test]
    fn test_device_interface_update_mac_length_not_enforced() -> Result<(), serde_json::Error> {
        // 特征测试（疑似缺陷）：mac_address 为双层 Option，validator 的 length
        // 校验对双层 Option 字段不生效，超长 MAC 当前不会被拒绝。
        // 若未来修复 validator 行为，此断言应改为 is_err()。
        let req: DeviceInterfaceUpdate = serde_json::from_value(serde_json::json!({
            "mac_address": "M".repeat(21)
        }))?;
        assert!(
            req.validate().is_ok(),
            "当前 validator 对双层 Option 的 length 校验不生效"
        );
        Ok(())
    }

    // ---------- 设备创建 / 更新 ----------

    fn valid_device_create_json() -> serde_json::Value {
        serde_json::json!({
            "name": "核心交换机",
            "device_type": "switch",
            "hostname": "core-sw-01",
            "room_id": Uuid::new_v4(),
            "brand": "H3C",
            "model": "S6520",
            "seller": "代理商",
            "location": "A 机柜",
            "snmp_version": "2c",
            "snmp_community": "public",
            "snmp_port": 161,
            "description": "核心设备",
            "save_as_template": false
        })
    }

    #[test]
    fn test_device_create_valid() -> Result<(), serde_json::Error> {
        let req: DeviceCreate = serde_json::from_value(valid_device_create_json())?;
        assert!(req.validate().is_ok());
        assert_eq!(req.device_type.as_deref(), Some("switch"));
        Ok(())
    }

    #[test]
    fn test_device_create_invalid_device_type() -> Result<(), serde_json::Error> {
        // device_type 走自定义校验，非法值拒绝；None 则跳过
        let mut json = valid_device_create_json();
        json["device_type"] = serde_json::json!("hypervisor");
        let req: DeviceCreate = serde_json::from_value(json)?;
        let Err(errors) = req.validate() else {
            panic!("非法设备类型应被拒绝");
        };
        assert!(errors.errors().contains_key("device_type"));

        let mut none_json = valid_device_create_json();
        none_json["device_type"] = serde_json::Value::Null;
        let none_req: DeviceCreate = serde_json::from_value(none_json)?;
        assert!(none_req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_device_create_name_and_hostname_invalid() -> Result<(), serde_json::Error> {
        let mut json = valid_device_create_json();
        json["name"] = serde_json::json!("");
        json["hostname"] = serde_json::json!("H".repeat(101));
        let req: DeviceCreate = serde_json::from_value(json)?;
        let Err(errors) = req.validate() else {
            panic!("空名称与超长主机名应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        assert!(errors.errors().contains_key("hostname"));
        Ok(())
    }

    #[test]
    fn test_device_update_null_clears_optional_uuid() -> Result<(), serde_json::Error> {
        // workstation_id / position_id 双层 Option：null → Some(None) 清除绑定
        use serde::de::Error as _;
        let req: DeviceUpdate = serde_json::from_value(serde_json::json!({
            "workstation_id": null,
            "position_id": "550e8400-e29b-41d4-a716-446655440000"
        }))?;
        assert_eq!(req.workstation_id, Some(None));
        let expected = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
            .map_err(serde_json::Error::custom)?;
        assert_eq!(req.position_id, Some(Some(expected)));
        Ok(())
    }

    #[test]
    fn test_device_update_missing_optional_uuid_is_none() -> Result<(), serde_json::Error> {
        // 字段缺失 → None（不修改）
        let req: DeviceUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert_eq!(req.workstation_id, None);
        assert_eq!(req.position_id, None);
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_device_update_invalid_fields() -> Result<(), serde_json::Error> {
        let req: DeviceUpdate = serde_json::from_value(serde_json::json!({
            "name": "",
            "device_type": "unknown_type"
        }))?;
        let Err(errors) = req.validate() else {
            panic!("非法设备更新应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        assert!(errors.errors().contains_key("device_type"));
        Ok(())
    }

    // ---------- SNMP / MAC / LLDP 辅助模型 ----------

    #[test]
    fn test_snmp_test_request_deserialize() -> Result<(), serde_json::Error> {
        let req: SnmpTestRequest = serde_json::from_value(serde_json::json!({
            "ip_address": "192.168.1.10",
            "snmp_version": "3",
            "snmp_port": 1610
        }))?;
        assert_eq!(req.ip_address.as_deref(), Some("192.168.1.10"));
        assert_eq!(req.device_id, None);
        Ok(())
    }

    #[test]
    fn test_arp_entry_serde_roundtrip() -> Result<(), serde_json::Error> {
        let entry = ArpEntry {
            ip_address: "192.168.1.10".to_string(),
            mac_address: "aa:bb:cc:dd:ee:ff".to_string(),
            interface: Some("G1/0/1".to_string()),
            vlan_id: Some(100),
        };
        let first = serde_json::to_value(&entry)?;
        let back: ArpEntry = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_lldp_neighbor_serde_roundtrip() -> Result<(), serde_json::Error> {
        let neighbor = LldpNeighbor {
            local_port: "G1/0/24".to_string(),
            neighbor_chassis_id: Some("aa:bb:cc:dd:ee:ff".to_string()),
            neighbor_port_id: None,
            neighbor_port_desc: None,
            neighbor_sys_name: Some("core-sw-01".to_string()),
            neighbor_sys_desc: None,
        };
        let first = serde_json::to_value(&neighbor)?;
        let back: LldpNeighbor = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_device_mac_create_deserialize() -> Result<(), serde_json::Error> {
        let req: DeviceMacCreate = serde_json::from_value(serde_json::json!({
            "ip_address": "192.168.1.20",
            "mac_address": "aa:bb:cc:00:00:01"
        }))?;
        assert_eq!(req.interface, None);
        assert_eq!(req.vlan_id, None);
        Ok(())
    }

    #[test]
    fn test_device_lldp_create_deserialize() -> Result<(), serde_json::Error> {
        let req: DeviceLldpCreate = serde_json::from_value(serde_json::json!({
            "local_port": "G1/0/1",
            "neighbor_sys_name": "ap-01"
        }))?;
        assert_eq!(req.local_port, "G1/0/1");
        assert_eq!(req.neighbor_sys_name.as_deref(), Some("ap-01"));
        Ok(())
    }

    // ---------- 设备模板 ----------

    #[test]
    fn test_update_device_template_request_validation() -> Result<(), serde_json::Error> {
        let ok: UpdateDeviceTemplateRequest = serde_json::from_value(serde_json::json!({
            "name": "核心交换机模板",
            "device_type": "switch"
        }))?;
        assert!(ok.validate().is_ok());

        let bad: UpdateDeviceTemplateRequest = serde_json::from_value(serde_json::json!({
            "name": "",
            "device_type": ""
        }))?;
        let Err(errors) = bad.validate() else {
            panic!("空模板名称应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        assert!(errors.errors().contains_key("device_type"));
        Ok(())
    }

    // ---------- 设备实体 ----------

    #[test]
    fn test_device_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let device = Device {
            id: Uuid::new_v4(),
            name: "核心交换机".to_string(),
            hostname: Some("core-sw-01".to_string()),
            device_type: "switch".to_string(),
            brand: Some("H3C".to_string()),
            model: Some("S6520".to_string()),
            serial_number: None,
            workstation_id: None,
            position_id: None,
            room_id: Uuid::new_v4(),
            template_id: None,
            seller: None,
            location: None,
            snmp_version: "2c".to_string(),
            snmp_community: Some("public".to_string()),
            snmp_username: None,
            snmp_auth_protocol: None,
            snmp_auth_password: None,
            snmp_priv_protocol: None,
            snmp_priv_password: None,
            snmp_port: 161,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&device)?;
        let back: Device = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_device_with_details_serde_roundtrip() -> Result<(), serde_json::Error> {
        let details = DeviceWithDetails {
            id: Uuid::new_v4(),
            name: "服务器".to_string(),
            hostname: None,
            device_type: "server".to_string(),
            brand: None,
            model: None,
            serial_number: None,
            workstation_id: None,
            workstation_name: Some("工位 1".to_string()),
            position_id: None,
            room_id: Uuid::new_v4(),
            room_name: Some("机房".to_string()),
            cabinet_id: None,
            cabinet_name: None,
            start_u: Some(1),
            end_u: Some(4),
            template_id: None,
            template_name: None,
            seller: None,
            location: None,
            snmp_version: None,
            snmp_community: None,
            snmp_username: None,
            snmp_auth_protocol: None,
            snmp_auth_password: None,
            snmp_priv_protocol: None,
            snmp_priv_password: None,
            snmp_port: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&details)?;
        let back: DeviceWithDetails = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }
}
