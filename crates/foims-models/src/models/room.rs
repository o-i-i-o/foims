//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use super::network::NetworkInfo;
use super::workstation::NetOutletBrief;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== 房间模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow, ts_rs::TS)]
#[ts(export)]
pub struct Room {
    pub id: Uuid,
    pub name: String,
    pub room_type: String,
    /// 房间所属组织（NOT NULL：房间必须挂在唯一组织节点下）
    pub org_id: Uuid,
    /// 房间管理人（NOT NULL 且必须属于 org_id 对应组织）：
    /// 无工位的空间（机房/弱电井）由房间管理人兜底承接设备通知
    pub manager_employee_id: Uuid,
    /// 房间总功率上限（瓦，None=不限制）
    pub total_power_watts: Option<i32>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ts_rs::TS)]
#[ts(export)]
pub struct RoomWithNetworks {
    pub id: Uuid,
    pub name: String,
    pub room_type: String,
    pub org_id: Uuid,
    /// 所属组织名称（org_id 非空且 FK RESTRICT，联查必有值）
    pub org_name: String,
    pub manager_employee_id: Uuid,
    /// 房间管理人姓名（联查 employees，NOT NULL + FK 保证存在）
    pub manager_name: String,
    /// 房间总功率上限（瓦，None=不限制）
    pub total_power_watts: Option<i32>,
    /// 房内设备功耗之和（瓦）
    pub allocated_power_watts: i64,
    pub description: Option<String>,
    pub networks: Vec<NetworkInfo>,
    pub workstation_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workstations: Option<Vec<WorkstationBrief>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cabinets: Option<Vec<CabinetBrief>>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub net_outlets: Vec<NetOutletBrief>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 组织节点下的房间摘要（组织页「房间」入口的子树列表）。
#[derive(Debug, Serialize, Deserialize, Clone, FromRow, ts_rs::TS)]
#[ts(export)]
pub struct RoomSummary {
    pub id: Uuid,
    pub name: String,
    pub room_type: String,
    /// 房间直属组织名称（子树口径下用于区分不同下级组织的房间）
    pub org_name: String,
    pub manager_name: String,
    pub workstation_count: i64,
    pub cabinet_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, ts_rs::TS)]
#[ts(export)]
pub struct WorkstationBrief {
    pub id: Uuid,
    pub name: String,
    /// 管理人姓名（联查 employees，manager_employee_id NOT NULL + FK 保证存在）
    pub manager_name: String,
    pub manager_employee_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Clone, ts_rs::TS)]
#[ts(export)]
pub struct CabinetBrief {
    pub id: Uuid,
    pub name: String,
    pub capacity: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone, ts_rs::TS)]
#[ts(export)]
pub struct PositionBrief {
    pub id: Uuid,
    pub name: String,
    pub start_u: i32,
    pub end_u: i32,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate, ts_rs::TS)]
#[ts(export)]
pub struct RoomCreate {
    #[validate(length(min = 1, max = 50, message = "server.room.validation.name_length"))]
    pub name: String,
    #[validate(custom(
        function = "crate::models::validate_room_type_string",
        message = "server.room.validation.type_invalid"
    ))]
    pub room_type: String,
    /// 所属组织（必填：房间必须归属组织节点）
    pub org_id: Uuid,
    /// 房间管理人（必填：必须属于 org_id 对应组织，后端与触发器双重校验）
    pub manager_employee_id: Uuid,
    pub subnet_ids: Vec<Uuid>,
    /// 房间总功率上限（瓦，0..=10_000_000；缺省/null=不限制）
    #[validate(custom(
        function = "crate::models::validate_power_watts",
        message = "server.common.validation.power_range"
    ))]
    pub total_power_watts: Option<i32>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate, ts_rs::TS)]
#[ts(export)]
pub struct RoomUpdate {
    #[validate(length(min = 1, max = 50, message = "server.room.validation.name_length"))]
    pub name: Option<String>,
    #[validate(custom(
        function = "crate::models::validate_room_type_option",
        message = "server.room.validation.type_invalid"
    ))]
    pub room_type: Option<String>,
    /// 缺省/null 均表示不修改（org_id 非空，不支持解绑组织）
    pub org_id: Option<Uuid>,
    /// 缺省/null 均表示不修改（manager_employee_id 非空，不支持清空）
    pub manager_employee_id: Option<Uuid>,
    pub subnet_ids: Option<Vec<Uuid>>,
    /// 双层 Option：字段缺失不修改、JSON null 清空上限（不限制）、值设置新值；
    /// 设置新值时房内设备功耗之和不得超过该值
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    #[validate(custom(
        function = "crate::models::validate_power_watts",
        message = "server.common.validation.power_range"
    ))]
    pub total_power_watts: Option<Option<i32>>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    #[test]
    fn test_room_create_valid() -> Result<(), serde_json::Error> {
        let req: RoomCreate = serde_json::from_value(serde_json::json!({
            "name": "301 会议室",
            "room_type": "office",
            "org_id": Uuid::new_v4(),
            "manager_employee_id": Uuid::new_v4(),
            "subnet_ids": [Uuid::new_v4()],
            "description": "三楼办公区"
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.subnet_ids.len(), 1);
        Ok(())
    }

    #[test]
    fn test_room_create_room_type_case_insensitive() -> Result<(), serde_json::Error> {
        // 房间类型校验大小写不敏感
        let req: RoomCreate = serde_json::from_value(serde_json::json!({
            "name": "机房",
            "room_type": "Data_Center",
            "org_id": Uuid::new_v4(),
            "manager_employee_id": Uuid::new_v4(),
            "subnet_ids": []
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.room_type, "Data_Center");
        Ok(())
    }

    #[test]
    fn test_room_create_invalid_room_type() -> Result<(), serde_json::Error> {
        let req: RoomCreate = serde_json::from_value(serde_json::json!({
            "name": "杂物间",
            "room_type": "storage",
            "org_id": Uuid::new_v4(),
            "manager_employee_id": Uuid::new_v4(),
            "subnet_ids": []
        }))?;
        let Err(errors) = req.validate() else {
            panic!("非法房间类型应被拒绝");
        };
        assert!(errors.errors().contains_key("room_type"));
        Ok(())
    }

    #[test]
    fn test_room_create_name_length() -> Result<(), serde_json::Error> {
        let req: RoomCreate = serde_json::from_value(serde_json::json!({
            "name": "R".repeat(51),
            "room_type": "office",
            "org_id": Uuid::new_v4(),
            "manager_employee_id": Uuid::new_v4(),
            "subnet_ids": []
        }))?;
        let Err(errors) = req.validate() else {
            panic!("超长房间名应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        Ok(())
    }

    #[test]
    fn test_room_create_required_fields() {
        // org_id / manager_employee_id / subnet_ids 均为必填：缺失即反序列化失败
        for missing in ["org_id", "manager_employee_id", "subnet_ids"] {
            let mut value = serde_json::json!({
                "name": "301",
                "room_type": "office",
                "org_id": Uuid::new_v4(),
                "manager_employee_id": Uuid::new_v4(),
                "subnet_ids": []
            });
            value
                .as_object_mut()
                .map(|obj| obj.remove(missing))
                .unwrap_or_default();
            assert!(
                serde_json::from_value::<RoomCreate>(value).is_err(),
                "缺失 {missing} 应反序列化失败"
            );
        }
    }

    #[test]
    fn test_room_update_valid() -> Result<(), serde_json::Error> {
        let req: RoomUpdate = serde_json::from_value(serde_json::json!({
            "name": "302 会议室",
            "room_type": "LOBBY",
            "subnet_ids": []
        }))?;
        // room_type 大小写不敏感，LOBBY 合法
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_room_update_invalid() -> Result<(), serde_json::Error> {
        // 非法类型 + 超长描述同时拒绝
        let req: RoomUpdate = serde_json::from_value(serde_json::json!({
            "room_type": "balcony",
            "description": "D".repeat(256)
        }))?;
        let Err(errors) = req.validate() else {
            panic!("非法更新请求应被拒绝");
        };
        assert!(errors.errors().contains_key("room_type"));
        assert!(errors.errors().contains_key("description"));
        Ok(())
    }

    #[test]
    fn test_room_update_org_id_optional() -> Result<(), serde_json::Error> {
        // org_id / manager_employee_id 为单层 Option：缺失与 null 均表示不修改，
        // 提供值则切换（非空列不支持解绑，故不存在三态）
        let missing: RoomUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert_eq!(missing.org_id, None);
        assert_eq!(missing.manager_employee_id, None);

        let null_org: RoomUpdate = serde_json::from_value(serde_json::json!({
            "org_id": null,
            "manager_employee_id": null
        }))?;
        assert_eq!(null_org.org_id, None);
        assert_eq!(null_org.manager_employee_id, None);

        let expected = Uuid::new_v4();
        let set_org: RoomUpdate = serde_json::from_value(serde_json::json!({
            "org_id": expected,
            "manager_employee_id": expected
        }))?;
        assert_eq!(set_org.org_id, Some(expected));
        assert_eq!(set_org.manager_employee_id, Some(expected));
        Ok(())
    }

    #[test]
    fn test_room_with_networks_skip_serialization() -> Result<(), serde_json::Error> {
        // workstations/cabinets 为 None 与 net_outlets 为空时应跳过序列化
        let room = RoomWithNetworks {
            id: Uuid::new_v4(),
            name: "301".to_string(),
            room_type: "office".to_string(),
            org_id: Uuid::new_v4(),
            org_name: "研发部".to_string(),
            manager_employee_id: Uuid::new_v4(),
            manager_name: "张三".to_string(),
            total_power_watts: Some(50_000),
            allocated_power_watts: 12_500,
            description: None,
            networks: Vec::new(),
            workstation_count: 0,
            workstations: None,
            cabinets: None,
            net_outlets: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let value = serde_json::to_value(&room)?;
        let obj = value
            .as_object()
            .unwrap_or_else(|| panic!("应为 JSON 对象"));
        assert!(!obj.contains_key("workstations"), "None 工位列表应跳过");
        assert!(!obj.contains_key("cabinets"), "None 机柜列表应跳过");
        assert!(!obj.contains_key("net_outlets"), "空信息点列表应跳过");
        assert!(obj.contains_key("networks"));

        // 有值时全部包含
        let full = RoomWithNetworks {
            workstations: Some(vec![WorkstationBrief {
                id: Uuid::new_v4(),
                name: "工位 1".to_string(),
                manager_name: "张三".to_string(),
                manager_employee_id: Uuid::new_v4(),
            }]),
            cabinets: Some(vec![CabinetBrief {
                id: Uuid::new_v4(),
                name: "A 柜".to_string(),
                capacity: 42,
            }]),
            net_outlets: vec![NetOutletBrief {
                id: Uuid::new_v4(),
                name: "D101".to_string(),
            }],
            ..room
        };
        let full_value = serde_json::to_value(&full)?;
        let full_obj = full_value
            .as_object()
            .unwrap_or_else(|| panic!("应为 JSON 对象"));
        assert!(full_obj.contains_key("workstations"));
        assert!(full_obj.contains_key("cabinets"));
        assert!(full_obj.contains_key("net_outlets"));
        Ok(())
    }

    #[test]
    fn test_room_with_networks_deserialize_defaults() -> Result<(), serde_json::Error> {
        // 反序列化方向：缺失的 net_outlets 回填为空数组
        let value = serde_json::json!({
            "id": Uuid::new_v4(),
            "name": "301",
            "room_type": "office",
            "org_id": Uuid::new_v4(),
            "org_name": "研发部",
            "manager_employee_id": Uuid::new_v4(),
            "manager_name": "张三",
            "total_power_watts": null,
            "allocated_power_watts": 0,
            "description": null,
            "networks": [],
            "workstation_count": 0,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        });
        let room: RoomWithNetworks = serde_json::from_value(value)?;
        assert!(room.net_outlets.is_empty());
        assert!(room.workstations.is_none());
        assert_eq!(room.total_power_watts, None);
        Ok(())
    }

    #[test]
    fn test_room_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let room = Room {
            id: Uuid::new_v4(),
            name: "301".to_string(),
            room_type: "office".to_string(),
            org_id: Uuid::new_v4(),
            manager_employee_id: Uuid::new_v4(),
            total_power_watts: Some(50_000),
            description: Some("三楼".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&room)?;
        let back: Room = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }
}
