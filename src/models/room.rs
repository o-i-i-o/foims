//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use super::network::NetworkInfo;
use super::workstation::NetOutletBrief;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== 房间模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Room {
    pub id: Uuid,
    pub name: String,
    pub room_type: String,
    pub org_id: Option<Uuid>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct RoomNetwork {
    pub id: Uuid,
    pub room_id: Uuid,
    pub network_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct RoomNetworkDetail {
    pub room_id: Uuid,
    pub room_name: String,
    pub room_type: String,
    pub description: Option<String>,
    pub network_id: Option<Uuid>,
    pub network_name: Option<String>,
    pub network_region: Option<String>,
    pub network_region_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RoomWithNetworks {
    pub id: Uuid,
    pub name: String,
    pub room_type: String,
    pub org_id: Option<Uuid>,
    pub org_name: Option<String>,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkstationBrief {
    pub id: Uuid,
    pub name: String,
    pub manager: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CabinetBrief {
    pub id: Uuid,
    pub name: String,
    pub capacity: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PositionBrief {
    pub id: Uuid,
    pub name: String,
    pub start_u: i32,
    pub end_u: i32,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RoomCreate {
    #[validate(length(min = 1, max = 50, message = "server.room.validation.name_length"))]
    pub name: String,
    #[validate(custom(
        function = "crate::models::validate_room_type_string",
        message = "server.room.validation.type_invalid"
    ))]
    pub room_type: String,
    pub org_id: Option<Uuid>,
    pub network_ids: Vec<Uuid>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RoomUpdate {
    #[validate(length(min = 1, max = 50, message = "server.room.validation.name_length"))]
    pub name: Option<String>,
    #[validate(custom(
        function = "crate::models::validate_room_type_option",
        message = "server.room.validation.type_invalid"
    ))]
    pub room_type: Option<String>,
    pub org_id: Option<Option<Uuid>>,
    pub network_ids: Option<Vec<Uuid>>,
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
            "network_ids": [Uuid::new_v4()],
            "description": "三楼办公区"
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.network_ids.len(), 1);
        Ok(())
    }

    #[test]
    fn test_room_create_room_type_case_insensitive() -> Result<(), serde_json::Error> {
        // 房间类型校验大小写不敏感
        let req: RoomCreate = serde_json::from_value(serde_json::json!({
            "name": "机房",
            "room_type": "Data_Center",
            "network_ids": []
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
            "network_ids": []
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
            "network_ids": []
        }))?;
        let Err(errors) = req.validate() else {
            panic!("超长房间名应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        Ok(())
    }

    #[test]
    fn test_room_create_network_ids_required() {
        // network_ids 为必填字段，缺失时反序列化失败
        let result: Result<RoomCreate, _> = serde_json::from_value(serde_json::json!({
            "name": "301",
            "room_type": "office"
        }));
        assert!(result.is_err(), "缺失 network_ids 应反序列化失败");
    }

    #[test]
    fn test_room_update_valid() -> Result<(), serde_json::Error> {
        let req: RoomUpdate = serde_json::from_value(serde_json::json!({
            "name": "302 会议室",
            "room_type": "LOBBY",
            "network_ids": []
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
    fn test_room_update_org_id_null_semantics() -> Result<(), serde_json::Error> {
        use serde::de::Error as _;
        // org_id 为 Option<Option<Uuid>> 但未挂 deserialize_some：
        // 缺失与 null 均反序列化为 None（无法通过 null 表达"解绑组织"）
        let missing: RoomUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert_eq!(missing.org_id, None);

        let null_org: RoomUpdate = serde_json::from_value(serde_json::json!({
            "org_id": null
        }))?;
        assert_eq!(null_org.org_id, None);

        let set_org: RoomUpdate = serde_json::from_value(serde_json::json!({
            "org_id": "550e8400-e29b-41d4-a716-446655440000"
        }))?;
        let expected = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
            .map_err(serde_json::Error::custom)?;
        assert_eq!(set_org.org_id, Some(Some(expected)));
        Ok(())
    }

    #[test]
    fn test_room_with_networks_skip_serialization() -> Result<(), serde_json::Error> {
        // workstations/cabinets 为 None 与 net_outlets 为空时应跳过序列化
        let room = RoomWithNetworks {
            id: Uuid::new_v4(),
            name: "301".to_string(),
            room_type: "office".to_string(),
            org_id: None,
            org_name: None,
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
                manager: None,
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
            "org_id": null,
            "org_name": null,
            "description": null,
            "networks": [],
            "workstation_count": 0,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        });
        let room: RoomWithNetworks = serde_json::from_value(value)?;
        assert!(room.net_outlets.is_empty());
        assert!(room.workstations.is_none());
        Ok(())
    }

    #[test]
    fn test_room_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let room = Room {
            id: Uuid::new_v4(),
            name: "301".to_string(),
            room_type: "office".to_string(),
            org_id: None,
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
