//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use super::room::PositionBrief;
use super::workstation::PatchPanelBrief;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== 机柜模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Cabinet {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub capacity: i32,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CabinetWithNetworks {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub capacity: i32,
    pub position_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub positions: Option<Vec<PositionBrief>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_panels: Option<Vec<PatchPanelBrief>>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetCreate {
    #[validate(length(min = 1, max = 50, message = "server.cabinet.validation.name_length"))]
    pub name: String,
    pub room_id: Uuid,
    #[validate(range(
        min = 1,
        max = 48,
        message = "server.cabinet.validation.capacity_range"
    ))]
    pub capacity: i32,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetUpdate {
    #[validate(length(min = 1, max = 50, message = "server.cabinet.validation.name_length"))]
    pub name: Option<String>,
    pub room_id: Option<Uuid>,
    #[validate(range(
        min = 1,
        max = 48,
        message = "server.cabinet.validation.capacity_range"
    ))]
    pub capacity: Option<i32>,
    /// 双层 Option：字段缺失不修改、JSON null 清空（SET NULL）、值设置新值
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    #[validate(custom(
        function = "crate::models::validate_description_opt",
        message = "server.common.validation.description_length"
    ))]
    pub description: Option<Option<String>>,
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    #[test]
    fn test_cabinet_create_valid() -> Result<(), serde_json::Error> {
        let req: CabinetCreate = serde_json::from_value(serde_json::json!({
            "name": "A 机柜",
            "room_id": Uuid::new_v4(),
            "capacity": 42,
            "description": "主机柜"
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_cabinet_create_capacity_range() -> Result<(), serde_json::Error> {
        // 容量边界：1 与 48 合法；0 与 49 非法
        for capacity in [1, 48] {
            let req: CabinetCreate = serde_json::from_value(serde_json::json!({
                "name": "A 机柜",
                "room_id": Uuid::new_v4(),
                "capacity": capacity
            }))?;
            assert!(req.validate().is_ok(), "容量 {capacity} 应合法");
        }
        for capacity in [0, 49, -1] {
            let req: CabinetCreate = serde_json::from_value(serde_json::json!({
                "name": "A 机柜",
                "room_id": Uuid::new_v4(),
                "capacity": capacity
            }))?;
            let Err(errors) = req.validate() else {
                panic!("容量 {capacity} 应被拒绝");
            };
            assert!(errors.errors().contains_key("capacity"));
        }
        Ok(())
    }

    #[test]
    fn test_cabinet_create_name_length() -> Result<(), serde_json::Error> {
        let req: CabinetCreate = serde_json::from_value(serde_json::json!({
            "name": "",
            "room_id": Uuid::new_v4(),
            "capacity": 42
        }))?;
        let Err(errors) = req.validate() else {
            panic!("空机柜名应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        Ok(())
    }

    #[test]
    fn test_cabinet_update_valid_and_invalid() -> Result<(), serde_json::Error> {
        // 全缺省通过
        let empty: CabinetUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert!(empty.validate().is_ok());
        assert_eq!(empty.description, None, "字段缺失表示不修改");

        // 容量越界拒绝
        let bad: CabinetUpdate = serde_json::from_value(serde_json::json!({ "capacity": 100 }))?;
        let Err(errors) = bad.validate() else {
            panic!("超界容量应被拒绝");
        };
        assert!(errors.errors().contains_key("capacity"));
        Ok(())
    }

    #[test]
    fn test_cabinet_update_description_three_states() -> Result<(), serde_json::Error> {
        // 双层 Option：null 清空描述、值设置新值且超长被拒绝
        let cleared: CabinetUpdate = serde_json::from_value(serde_json::json!({
            "description": null
        }))?;
        assert_eq!(cleared.description, Some(None));
        assert!(cleared.validate().is_ok());

        let bad: CabinetUpdate = serde_json::from_value(serde_json::json!({
            "description": "D".repeat(256)
        }))?;
        let Err(errors) = bad.validate() else {
            panic!("超长描述应被拒绝");
        };
        assert!(errors.errors().contains_key("description"));
        Ok(())
    }

    #[test]
    fn test_cabinet_with_networks_skip_serialization() -> Result<(), serde_json::Error> {
        // positions / patch_panels 为 None 时跳过序列化
        let cabinet = CabinetWithNetworks {
            id: Uuid::new_v4(),
            name: "A 机柜".to_string(),
            room_id: Uuid::new_v4(),
            room_name: Some("机房".to_string()),
            capacity: 42,
            position_count: 3,
            positions: None,
            patch_panels: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let value = serde_json::to_value(&cabinet)?;
        let obj = value
            .as_object()
            .unwrap_or_else(|| panic!("应为 JSON 对象"));
        assert!(!obj.contains_key("positions"));
        assert!(!obj.contains_key("patch_panels"));

        // 有值时包含
        let full = CabinetWithNetworks {
            positions: Some(vec![PositionBrief {
                id: Uuid::new_v4(),
                name: "U1-U4".to_string(),
                start_u: 1,
                end_u: 4,
                description: None,
            }]),
            patch_panels: Some(vec![PatchPanelBrief {
                id: Uuid::new_v4(),
                name: "配线架 1".to_string(),
            }]),
            ..cabinet
        };
        let full_value = serde_json::to_value(&full)?;
        let full_obj = full_value
            .as_object()
            .unwrap_or_else(|| panic!("应为 JSON 对象"));
        assert!(full_obj.contains_key("positions"));
        assert!(full_obj.contains_key("patch_panels"));
        Ok(())
    }

    #[test]
    fn test_cabinet_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let cabinet = Cabinet {
            id: Uuid::new_v4(),
            name: "A 机柜".to_string(),
            room_id: Uuid::new_v4(),
            capacity: 42,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&cabinet)?;
        let back: Cabinet = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }
}
