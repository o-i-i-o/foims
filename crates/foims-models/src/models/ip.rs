//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== IP 详情模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct IpDetail {
    pub id: Uuid,
    pub device_interface_id: Uuid,
    #[sqlx(default)]
    pub device_id: Uuid,
    pub subnet_id: Uuid,
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
pub struct IpDetailWithNames {
    pub id: Uuid,
    pub device_interface_id: Uuid,
    pub device_id: Uuid,
    pub device_type: Option<String>,
    pub device_name: Option<String>,
    pub subnet_id: Uuid,
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
pub struct IpDetailCreate {
    pub device_interface_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub subnet_id: Option<Uuid>,
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
    pub subnet_id: Option<Uuid>,
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
    /// 仅允许 1..=4094（IEEE 802.1Q），负数与 4095+ 拒绝入库
    #[validate(custom(
        function = "crate::models::validate_vlan_id_option",
        message = "server.device.validation.vlan_id_range"
    ))]
    pub vlan_id: Option<i32>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
    /// nested 显式开启：validator derive 不自动展开 Vec 元素，
    /// 缺失时 IpSyncItem 的 IP 格式校验在同步路径不会执行
    #[validate(nested)]
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
    /// nested 显式开启：网卡→网口→IP 嵌套链的模型声明与校验点保持一致
    #[validate(nested)]
    #[serde(default)]
    pub ports: Vec<PortSyncItem>,
}

#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct DeviceNetworkConfigSync {
    /// nested 显式开启：网卡→网口→IP 嵌套链的模型声明与校验点保持一致
    #[validate(nested)]
    #[serde(default)]
    pub cards: Vec<NetworkCardSyncItem>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct AutoAssignIpRequest {
    pub subnet_id: Uuid,
    pub device_interface_id: Option<Uuid>,
    pub device_id: Uuid,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct PullIpDetailsRequest {
    pub device_id: Uuid,
    pub subnet_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct IpDetailUpdate {
    // 挂 deserialize_some 后 JSON null → Some(None)：可通过 null 解绑接口
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    pub device_interface_id: Option<Option<Uuid>>,
    /// 有值才校验：与 IpDetailCreate 同口径拒绝带掩码/非法地址
    #[validate(custom(
        function = "crate::models::validate_ip_address_option",
        message = "server.ip.validation.ip_address_invalid"
    ))]
    pub ip_address: Option<String>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
    /// 取值白名单：active / inactive / reserved（与前端 formatter 展示口径一致；
    /// DB 无 CHECK 约束，应用层前置拦截非法值）
    #[validate(custom(
        function = "crate::models::validate_ip_status_option",
        message = "server.ip.validation.status_invalid"
    ))]
    pub status: Option<String>,
    /// 有值才校验：仅允许 4 / 6
    #[validate(custom(
        function = "crate::models::validate_ip_version_option",
        message = "server.ip.validation.ip_version_invalid"
    ))]
    pub ip_version: Option<i16>,
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    #[test]
    fn test_ip_detail_create_valid_ipv4() -> Result<(), serde_json::Error> {
        let req: IpDetailCreate = serde_json::from_value(serde_json::json!({
            "device_interface_id": Uuid::new_v4(),
            "subnet_id": Uuid::new_v4(),
            "ip_address": "192.168.1.10",
            "description": "办公 IP"
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_ip_detail_create_valid_ipv6() -> Result<(), serde_json::Error> {
        let req: IpDetailCreate = serde_json::from_value(serde_json::json!({
            "ip_address": "2001:db8::10"
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_ip_detail_create_invalid_ip() -> Result<(), serde_json::Error> {
        // 非法 IP 走自定义校验拒绝；带掩码后缀同样拒绝
        for bad_ip in ["999.1.1.1", "not-ip", "10.0.0.1/24"] {
            let req: IpDetailCreate = serde_json::from_value(serde_json::json!({
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
    fn test_ip_detail_create_description_too_long() -> Result<(), serde_json::Error> {
        let req: IpDetailCreate = serde_json::from_value(serde_json::json!({
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
            "subnet_id": Uuid::new_v4(),
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
    fn test_sync_chain_nested_validation() -> Result<(), serde_json::Error> {
        // nested 链路生效：卡片→网口→IP 任一层非法均使容器校验失败
        let bad_card: NetworkCardSyncItem = serde_json::from_value(serde_json::json!({
            "name": "eth0",
            "ports": [{ "name": "", "ips": [] }]
        }))?;
        assert!(bad_card.validate().is_err(), "空网口名应使网卡被拒绝");

        let bad_port: NetworkCardSyncItem = serde_json::from_value(serde_json::json!({
            "name": "eth0",
            "ports": [{ "name": "eth0", "ips": [{ "ip_address": "10.0.0.999" }] }]
        }))?;
        assert!(bad_port.validate().is_err(), "非法 IP 应使网卡被拒绝");

        let bad_ip: PortSyncItem = serde_json::from_value(serde_json::json!({
            "name": "eth0",
            "ips": [{ "ip_address": "not-an-ip" }]
        }))?;
        assert!(bad_ip.validate().is_err(), "非法 IP 应使网口被拒绝");
        Ok(())
    }

    #[test]
    fn test_auto_assign_and_pull_requests() -> Result<(), serde_json::Error> {
        let assign: AutoAssignIpRequest = serde_json::from_value(serde_json::json!({
            "subnet_id": Uuid::new_v4(),
            "device_id": Uuid::new_v4(),
            "description": "自动分配"
        }))?;
        assert!(assign.validate().is_ok());
        assert_eq!(assign.device_interface_id, None);

        let pull: PullIpDetailsRequest = serde_json::from_value(serde_json::json!({
            "device_id": Uuid::new_v4(),
            "subnet_id": Uuid::new_v4()
        }))?;
        assert!(pull.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_pull_request_missing_fields_rejected() {
        // device_id / subnet_id 均为必填，缺失时反序列化失败
        let result: Result<PullIpDetailsRequest, _> =
            serde_json::from_value(serde_json::json!({ "device_id": Uuid::new_v4() }));
        assert!(result.is_err());
    }

    #[test]
    fn test_ip_detail_update_semantics() -> Result<(), serde_json::Error> {
        use serde::de::Error as _;
        // device_interface_id 带 serde(default) 且已挂 deserialize_some（null 清除语义可达）：
        // 缺失与 null 均为 None；值则包一层 Some
        let missing: IpDetailUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert_eq!(missing.device_interface_id, None);

        let null_id: IpDetailUpdate =
            serde_json::from_value(serde_json::json!({ "device_interface_id": null }))?;
        assert_eq!(null_id.device_interface_id, Some(None));

        let set_id: IpDetailUpdate = serde_json::from_value(serde_json::json!({
            "device_interface_id": "550e8400-e29b-41d4-a716-446655440000"
        }))?;
        let expected = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
            .map_err(serde_json::Error::custom)?;
        assert_eq!(set_id.device_interface_id, Some(Some(expected)));
        assert!(set_id.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_ip_detail_update_status_too_long() -> Result<(), serde_json::Error> {
        let req: IpDetailUpdate = serde_json::from_value(serde_json::json!({
            "status": "S".repeat(21)
        }))?;
        let Err(errors) = req.validate() else {
            panic!("超长状态应被拒绝");
        };
        assert!(errors.errors().contains_key("status"));
        Ok(())
    }

    #[test]
    fn test_ip_detail_update_status_whitelist() -> Result<(), serde_json::Error> {
        // 白名单：active / inactive / reserved 合法，其余拒绝
        for ok_status in ["active", "inactive", "reserved"] {
            let req: IpDetailUpdate =
                serde_json::from_value(serde_json::json!({ "status": ok_status }))?;
            assert!(req.validate().is_ok(), "状态 {ok_status} 应合法");
        }
        let bad: IpDetailUpdate =
            serde_json::from_value(serde_json::json!({ "status": "enabled" }))?;
        let Err(errors) = bad.validate() else {
            panic!("白名单外状态应被拒绝");
        };
        assert!(errors.errors().contains_key("status"));
        Ok(())
    }

    #[test]
    fn test_ip_detail_update_ip_address_validation() -> Result<(), serde_json::Error> {
        // 更新路径与创建路径同口径：带掩码 / 非法地址拒绝；合法地址通过
        let bad: IpDetailUpdate = serde_json::from_value(serde_json::json!({
            "ip_address": "10.0.0.1/24"
        }))?;
        let Err(errors) = bad.validate() else {
            panic!("带掩码地址应被拒绝");
        };
        assert!(errors.errors().contains_key("ip_address"));

        let ok: IpDetailUpdate = serde_json::from_value(serde_json::json!({
            "ip_address": "10.0.0.1"
        }))?;
        assert!(ok.validate().is_ok());

        // 缺省（None）跳过校验
        let missing: IpDetailUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert!(missing.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_ip_detail_update_ip_version_validation() -> Result<(), serde_json::Error> {
        // ip_version 仅允许 4 / 6（有值才校验）
        for bad_version in [0i16, 5, -1, 7] {
            let req: IpDetailUpdate =
                serde_json::from_value(serde_json::json!({ "ip_version": bad_version }))?;
            let Err(errors) = req.validate() else {
                panic!("ip_version={bad_version} 应被拒绝");
            };
            assert!(errors.errors().contains_key("ip_version"));
        }
        for ok_version in [4i16, 6] {
            let req: IpDetailUpdate =
                serde_json::from_value(serde_json::json!({ "ip_version": ok_version }))?;
            assert!(req.validate().is_ok(), "ip_version={ok_version} 应合法");
        }
        Ok(())
    }

    #[test]
    fn test_port_sync_item_vlan_id_range() -> Result<(), serde_json::Error> {
        // VLAN ID 仅允许 1..=4094
        for bad_vlan in [0i32, -1, 4095, 65535] {
            let item: PortSyncItem = serde_json::from_value(serde_json::json!({
                "name": "eth0",
                "vlan_id": bad_vlan
            }))?;
            let Err(errors) = item.validate() else {
                panic!("vlan_id={bad_vlan} 应被拒绝");
            };
            assert!(errors.errors().contains_key("vlan_id"));
        }
        let ok: PortSyncItem = serde_json::from_value(serde_json::json!({
            "name": "eth0",
            "vlan_id": 4094
        }))?;
        assert!(ok.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_ip_detail_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let record = IpDetail {
            id: Uuid::new_v4(),
            device_interface_id: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            subnet_id: Uuid::new_v4(),
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
        let back: IpDetail = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_ip_detail_with_names_serde_roundtrip() -> Result<(), serde_json::Error> {
        let record = IpDetailWithNames {
            id: Uuid::new_v4(),
            device_interface_id: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            device_type: Some("server".to_string()),
            device_name: Some("web-01".to_string()),
            subnet_id: Uuid::new_v4(),
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
        let back: IpDetailWithNames = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }
}
