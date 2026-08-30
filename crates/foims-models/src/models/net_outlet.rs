//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== 信息点模型 ====================
// 信息点特指网络插座，仅隶属房间；配线架见下方独立模型

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct NetOutlet {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct NetOutletWithDetails {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetOutletCreate {
    #[validate(length(
        min = 1,
        max = 100,
        message = "server.net_outlet.validation.name_length"
    ))]
    pub name: String,
    pub room_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetOutletUpdate {
    #[validate(length(
        min = 1,
        max = 100,
        message = "server.net_outlet.validation.name_length"
    ))]
    pub name: Option<String>,
    pub room_id: Option<Uuid>,
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    #[test]
    fn test_net_outlet_create_valid() -> Result<(), serde_json::Error> {
        let req: NetOutletCreate = serde_json::from_value(serde_json::json!({
            "name": "D101",
            "room_id": Uuid::new_v4()
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_net_outlet_create_name_length() -> Result<(), serde_json::Error> {
        // 空名 / 超长名（>100）均拒绝
        for name in ["".to_string(), "N".repeat(101)] {
            let req: NetOutletCreate = serde_json::from_value(serde_json::json!({
                "name": name,
                "room_id": Uuid::new_v4()
            }))?;
            assert!(req.validate().is_err(), "超长或空名字应被拒绝");
        }
        // 恰好 100 字符合法
        let boundary: NetOutletCreate = serde_json::from_value(serde_json::json!({
            "name": "N".repeat(100),
            "room_id": Uuid::new_v4()
        }))?;
        assert!(boundary.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_net_outlet_update_valid_and_invalid() -> Result<(), serde_json::Error> {
        let ok: NetOutletUpdate = serde_json::from_value(serde_json::json!({
            "name": "D102",
            "room_id": Uuid::new_v4()
        }))?;
        assert!(ok.validate().is_ok());

        let bad: NetOutletUpdate = serde_json::from_value(serde_json::json!({ "name": "" }))?;
        let Err(errors) = bad.validate() else {
            panic!("空信息点名应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));

        // 全缺省通过
        let empty: NetOutletUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert!(empty.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_net_outlet_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let outlet = NetOutlet {
            id: Uuid::new_v4(),
            name: "D101".to_string(),
            room_id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&outlet)?;
        let back: NetOutlet = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_net_outlet_with_details_serde_roundtrip() -> Result<(), serde_json::Error> {
        let details = NetOutletWithDetails {
            id: Uuid::new_v4(),
            name: "D101".to_string(),
            room_id: Uuid::new_v4(),
            room_name: Some("301".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&details)?;
        let back: NetOutletWithDetails = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }
}
