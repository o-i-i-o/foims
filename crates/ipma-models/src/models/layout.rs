//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

// ==================== 布局模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct LayoutSaveRequest {
    pub r#type: String,
    pub room_id: Option<Uuid>,
    pub network_region_id: Option<Uuid>,
    pub cabinet_id: Option<Uuid>,
    pub layout: Vec<LayoutItem>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct LayoutItem {
    pub id: Uuid,
    pub position: Position,
    pub element_type: String,
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_serde_roundtrip() -> Result<(), serde_json::Error> {
        let pos = Position {
            x: 1.5,
            y: -2.25,
            width: 100.0,
            height: 40.5,
            rotation: 90.0,
        };
        let first = serde_json::to_value(&pos)?;
        let back: Position = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_position_deserialize_from_json() -> Result<(), serde_json::Error> {
        let pos: Position = serde_json::from_value(serde_json::json!({
            "x": 0.0,
            "y": 0.0,
            "width": 10.0,
            "height": 10.0,
            "rotation": 0.0
        }))?;
        assert_eq!(pos.x, 0.0);
        assert_eq!(pos.rotation, 0.0);
        Ok(())
    }

    #[test]
    fn test_layout_item_serde_roundtrip() -> Result<(), serde_json::Error> {
        let item = LayoutItem {
            id: Uuid::new_v4(),
            position: Position {
                x: 3.0,
                y: 4.0,
                width: 5.0,
                height: 6.0,
                rotation: 0.0,
            },
            element_type: "workstation".to_string(),
        };
        let first = serde_json::to_value(&item)?;
        let back: LayoutItem = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_layout_save_request_deserialize() -> Result<(), serde_json::Error> {
        // r#type / room_id / layout 数组均正确解析
        let req: LayoutSaveRequest = serde_json::from_value(serde_json::json!({
            "type": "room",
            "room_id": Uuid::new_v4(),
            "cabinet_id": null,
            "layout": [
                {
                    "id": Uuid::new_v4(),
                    "position": { "x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0, "rotation": 0.0 },
                    "element_type": "cabinet"
                }
            ]
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.r#type, "room");
        assert_eq!(req.layout.len(), 1);
        assert_eq!(req.network_region_id, None);
        Ok(())
    }

    #[test]
    fn test_layout_save_request_empty_layout() -> Result<(), serde_json::Error> {
        // 空布局（清空画布场景）合法
        let req: LayoutSaveRequest = serde_json::from_value(serde_json::json!({
            "type": "room",
            "layout": []
        }))?;
        assert!(req.validate().is_ok());
        assert!(req.layout.is_empty());
        assert_eq!(req.room_id, None);
        Ok(())
    }

    #[test]
    fn test_layout_save_request_missing_type_rejected() {
        // type 为必填字段，缺失时反序列化失败
        let result: Result<LayoutSaveRequest, _> = serde_json::from_value(serde_json::json!({
            "layout": []
        }));
        assert!(result.is_err());
    }
}
