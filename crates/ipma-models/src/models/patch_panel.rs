//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ==================== 配线架模型 ====================
// 配线架与信息点是不同概念，隶属机柜，由机柜弹窗内联同步管理

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct PatchPanel {
    pub id: Uuid,
    pub name: String,
    pub cabinet_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct PatchPanelWithDetails {
    pub id: Uuid,
    pub name: String,
    pub cabinet_id: Uuid,
    pub cabinet_name: String,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_panel_serde_roundtrip() -> Result<(), serde_json::Error> {
        let panel = PatchPanel {
            id: Uuid::new_v4(),
            name: "配线架 1".to_string(),
            cabinet_id: Uuid::new_v4(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&panel)?;
        let back: PatchPanel = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_patch_panel_with_details_serde_roundtrip() -> Result<(), serde_json::Error> {
        let details = PatchPanelWithDetails {
            id: Uuid::new_v4(),
            name: "配线架 1".to_string(),
            cabinet_id: Uuid::new_v4(),
            cabinet_name: "A 机柜".to_string(),
            room_id: Uuid::new_v4(),
            room_name: Some("机房".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&details)?;
        let back: PatchPanelWithDetails = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_patch_panel_deserialize_from_json() -> Result<(), serde_json::Error> {
        // 从 JSON 文本反序列化并核对字段
        let panel: PatchPanel = serde_json::from_value(serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "配线架 2",
            "cabinet_id": "660e8400-e29b-41d4-a716-446655440000",
            "created_at": "2026-01-01T08:00:00Z",
            "updated_at": "2026-01-01T08:00:00Z"
        }))?;
        assert_eq!(panel.name, "配线架 2");
        assert_eq!(panel.id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
        Ok(())
    }
}
