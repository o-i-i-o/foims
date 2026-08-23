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

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    #[test]
    fn test_ip_manager_create_valid_ipv4() -> Result<(), serde_json::Error> {
        let req: IpManagerCreate = serde_json::from_value(serde_json::json!({
            "device_interface_id": Uuid::new_v4(),
            "network_id": Uuid::new_v4(),
            "ip_address": "192.168.1.10",
            "description": "办公 IP"
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_ip_manager_create_valid_ipv6() -> Result<(), serde_json::Error> {
        let req: IpManagerCreate = serde_json::from_value(serde_json::json!({
            "ip_address": "2001:db8::10"
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_ip_manager_create_invalid_ip() -> Result<(), serde_json::Error> {
        // 非法 IP 走自定义校验拒绝；带掩码后缀同样拒绝
        for bad_ip in ["999.1.1.1", "not-ip", "10.0.0.1/24"] {
            let req: IpManagerCreate = serde_json::from_value(serde_json::json!({
                "ip_address": bad_ip
            }))?;
            let Err(errors) = req.validate() else {
                panic!("非法 IP {bad_ip} 应被拒绝");
            };
            assert!(errors.errors().contains_key("ip_address"));
        }
        Ok(())
    }

    #[test]
    fn test_ip_manager_create_description_too_long() -> Result<(), serde_json::Error> {
        let req: IpManagerCreate = serde_json::from_value(serde_json::json!({
            "ip_address": "1.2.3.4",
            "description": "D".repeat(256)
        }))?;
        let Err(errors) = req.validate() else {
            panic!("超长描述应被拒绝");
        };
        assert!(errors.errors().contains_key("description"));
        Ok(())
    }

    #[test]
    fn test_ip_sync_item_ip_validation() -> Result<(), serde_json::Error> {
        let ok: IpSyncItem = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "network_id": Uuid::new_v4(),
            "ip_address": "10.0.0.1"
        }))?;
        assert!(ok.validate().is_ok());

        let bad: IpSyncItem = serde_json::from_value(serde_json::json!({ "ip_address": "bad" }))?;
        let Err(errors) = bad.validate() else {
            panic!("非法 IP 应被拒绝");
        };
        assert!(errors.errors().contains_key("ip_address"));
        Ok(())
    }

    #[test]
    fn test_port_sync_item_validation() -> Result<(), serde_json::Error> {
        let ok: PortSyncItem = serde_json::from_value(serde_json::json!({
            "name": "eth0",
            "mac_address": "AA:BB:CC:DD:EE:FF",
            "ips": [{ "ip_address": "10.0.0.1" }]
        }))?;
        assert!(ok.validate().is_ok());
        assert_eq!(ok.ips.len(), 1);

        // 空名称 / 超长 MAC 拒绝
        let bad: PortSyncItem = serde_json::from_value(serde_json::json!({
            "name": "",
            "mac_address": "M".repeat(21)
        }))?;
        let Err(errors) = bad.validate() else {
            panic!("非法网口应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        assert!(errors.errors().contains_key("mac_address"));
        Ok(())
    }

    #[test]
    fn test_port_sync_item_ips_default_empty() -> Result<(), serde_json::Error> {
        // ips 字段带 serde(default)，缺失时回填空数组
        let item: PortSyncItem = serde_json::from_value(serde_json::json!({
            "name": "eth0"
        }))?;
        assert!(item.ips.is_empty());
        Ok(())
    }

    #[test]
    fn test_network_card_sync_item_ports_default() -> Result<(), serde_json::Error> {
        // ports 字段带 serde(default)，缺失时回填空数组
        let item: NetworkCardSyncItem =
            serde_json::from_value(serde_json::json!({ "name": "板载网卡" }))?;
        assert!(item.ports.is_empty());
        assert!(item.validate().is_ok());

        // 空网卡名拒绝
        let bad: NetworkCardSyncItem = serde_json::from_value(serde_json::json!({ "name": "" }))?;
        assert!(bad.validate().is_err());
        Ok(())
    }

    #[test]
    fn test_device_network_config_sync_valid() -> Result<(), serde_json::Error> {
        let req: DeviceNetworkConfigSync = serde_json::from_value(serde_json::json!({
            "cards": [
                {
                    "name": "eth0",
                    "ports": [
                        { "name": "eth0", "ips": [{ "ip_address": "10.0.0.1" }] }
                    ]
                }
            ]
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.cards.len(), 1);

        // cards 带 serde(default)，缺失时为空数组
        let empty: DeviceNetworkConfigSync = serde_json::from_value(serde_json::json!({}))?;
        assert!(empty.cards.is_empty());
        assert!(empty.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_auto_assign_and_pull_requests() -> Result<(), serde_json::Error> {
        let assign: AutoAssignIpRequest = serde_json::from_value(serde_json::json!({
            "network_id": Uuid::new_v4(),
            "device_id": Uuid::new_v4(),
            "description": "自动分配"
        }))?;
        assert!(assign.validate().is_ok());
        assert_eq!(assign.device_interface_id, None);

        let pull: PullIpManagersRequest = serde_json::from_value(serde_json::json!({
            "device_id": Uuid::new_v4(),
            "network_id": Uuid::new_v4()
        }))?;
        assert!(pull.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_pull_request_missing_fields_rejected() {
        // device_id / network_id 均为必填，缺失时反序列化失败
        let result: Result<PullIpManagersRequest, _> =
            serde_json::from_value(serde_json::json!({ "device_id": Uuid::new_v4() }));
        assert!(result.is_err());
    }

    #[test]
    fn test_ip_manager_update_semantics() -> Result<(), serde_json::Error> {
        use serde::de::Error as _;
        // device_interface_id 带 serde(default) 但未挂 deserialize_some：
        // 缺失与 null 均为 None；值则包一层 Some
        let missing: IpManagerUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert_eq!(missing.device_interface_id, None);

        let null_id: IpManagerUpdate =
            serde_json::from_value(serde_json::json!({ "device_interface_id": null }))?;
        assert_eq!(null_id.device_interface_id, None);

        let set_id: IpManagerUpdate = serde_json::from_value(serde_json::json!({
            "device_interface_id": "550e8400-e29b-41d4-a716-446655440000"
        }))?;
        let expected = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
            .map_err(serde_json::Error::custom)?;
        assert_eq!(set_id.device_interface_id, Some(Some(expected)));
        assert!(set_id.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_ip_manager_update_status_too_long() -> Result<(), serde_json::Error> {
        let req: IpManagerUpdate = serde_json::from_value(serde_json::json!({
            "status": "S".repeat(21)
        }))?;
        let Err(errors) = req.validate() else {
            panic!("超长状态应被拒绝");
        };
        assert!(errors.errors().contains_key("status"));
        Ok(())
    }

    #[test]
    fn test_ip_manager_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let record = IpManager {
            id: Uuid::new_v4(),
            device_interface_id: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            network_id: Some(Uuid::new_v4()),
            network_region_id: None,
            network_name: Some("办公网".to_string()),
            network_region: None,
            ip_address: "192.168.1.10".to_string(),
            ip_version: 4,
            mac_address: Some("aa:bb:cc:dd:ee:ff".to_string()),
            description: None,
            status: "active".to_string(),
            last_seen: Utc::now(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&record)?;
        let back: IpManager = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_ip_manager_with_names_serde_roundtrip() -> Result<(), serde_json::Error> {
        let record = IpManagerWithNames {
            id: Uuid::new_v4(),
            device_interface_id: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            device_type: Some("server".to_string()),
            device_name: Some("web-01".to_string()),
            network_id: None,
            workstation_name: None,
            cabinet_position_name: None,
            interface_name: Some("eth0".to_string()),
            physical_type: None,
            interface_role: None,
            room_name: Some("机房".to_string()),
            cabinet_name: None,
            org_name: None,
            network_name: "办公网".to_string(),
            network_region: "办公区".to_string(),
            ip_address: "10.1.1.1".to_string(),
            ip_version: 4,
            mac_address: None,
            hostname: None,
            description: None,
            status: "active".to_string(),
            last_seen: Utc::now(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            position_id: Some(Uuid::new_v4()),
        };
        let first = serde_json::to_value(&record)?;
        let back: IpManagerWithNames = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }
}
