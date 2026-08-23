//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use super::ip::IpManager;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== 机位模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct CabinetPosition {
    pub id: Uuid,
    pub name: String,
    pub cabinet_id: Option<Uuid>,
    pub start_u: i32,
    pub end_u: i32,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct CabinetPositionWithDetails {
    pub id: Uuid,
    pub name: String,
    pub cabinet_id: Option<Uuid>,
    pub cabinet_name: Option<String>,
    pub room_id: Option<Uuid>,
    pub room_name: Option<String>,
    pub start_u: i32,
    pub end_u: i32,
    /// IP 明细不从 SQL 映射（列表不携带、详情单独查询后手动填充）
    #[sqlx(skip)]
    pub ips: Vec<IpManager>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetPositionCreate {
    #[validate(length(min = 1, max = 50, message = "server.position.validation.name_length"))]
    pub name: String,
    pub cabinet_id: Option<Uuid>,
    #[validate(range(
        min = 1,
        max = 48,
        message = "server.position.validation.start_u_range"
    ))]
    pub start_u: i32,
    #[validate(range(min = 1, max = 48, message = "server.position.validation.end_u_range"))]
    pub end_u: i32,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetPositionUpdate {
    #[validate(length(min = 1, max = 50, message = "server.position.validation.name_length"))]
    pub name: Option<String>,
    pub cabinet_id: Option<Uuid>,
    #[validate(range(
        min = 1,
        max = 48,
        message = "server.position.validation.start_u_range"
    ))]
    pub start_u: Option<i32>,
    #[validate(range(min = 1, max = 48, message = "server.position.validation.end_u_range"))]
    pub end_u: Option<i32>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    #[test]
    fn test_position_create_valid() -> Result<(), serde_json::Error> {
        let req: CabinetPositionCreate = serde_json::from_value(serde_json::json!({
            "name": "U1-U4",
            "cabinet_id": Uuid::new_v4(),
            "start_u": 1,
            "end_u": 4,
            "description": "服务器机位"
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_position_create_u_range() -> Result<(), serde_json::Error> {
        // start_u / end_u 边界：1 与 48 合法，越界拒绝
        for (start_u, end_u) in [(1, 48), (1, 1), (48, 48)] {
            let req: CabinetPositionCreate = serde_json::from_value(serde_json::json!({
                "name": "机位",
                "start_u": start_u,
                "end_u": end_u
            }))?;
            assert!(req.validate().is_ok(), "({start_u},{end_u}) 应合法");
        }
        for (start_u, end_u) in [(0, 4), (1, 49), (-1, 4)] {
            let req: CabinetPositionCreate = serde_json::from_value(serde_json::json!({
                "name": "机位",
                "start_u": start_u,
                "end_u": end_u
            }))?;
            assert!(req.validate().is_err(), "({start_u},{end_u}) 应被拒绝");
        }
        Ok(())
    }

    #[test]
    fn test_position_create_name_required() -> Result<(), serde_json::Error> {
        let req: CabinetPositionCreate = serde_json::from_value(serde_json::json!({
            "name": "",
            "start_u": 1,
            "end_u": 2
        }))?;
        let Err(errors) = req.validate() else {
            panic!("空机位名应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        Ok(())
    }

    #[test]
    fn test_position_update_valid_and_invalid() -> Result<(), serde_json::Error> {
        let ok: CabinetPositionUpdate = serde_json::from_value(serde_json::json!({
            "name": "U5-U8",
            "start_u": 5,
            "end_u": 8
        }))?;
        assert!(ok.validate().is_ok());

        let bad: CabinetPositionUpdate =
            serde_json::from_value(serde_json::json!({ "end_u": 49 }))?;
        let Err(errors) = bad.validate() else {
            panic!("越界 end_u 应被拒绝");
        };
        assert!(errors.errors().contains_key("end_u"));

        // 全缺省通过
        let empty: CabinetPositionUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert!(empty.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_position_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let position = CabinetPosition {
            id: Uuid::new_v4(),
            name: "U1-U4".to_string(),
            cabinet_id: Some(Uuid::new_v4()),
            start_u: 1,
            end_u: 4,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&position)?;
        let back: CabinetPosition = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_position_with_details_serde_roundtrip() -> Result<(), serde_json::Error> {
        let details = CabinetPositionWithDetails {
            id: Uuid::new_v4(),
            name: "U1-U4".to_string(),
            cabinet_id: Some(Uuid::new_v4()),
            cabinet_name: Some("A 机柜".to_string()),
            room_id: Some(Uuid::new_v4()),
            room_name: Some("机房".to_string()),
            start_u: 1,
            end_u: 4,
            // ips 为 sqlx(skip) 字段，序列化时仍参与 serde 往返
            ips: Vec::new(),
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&details)?;
        let back: CabinetPositionWithDetails = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }
}
