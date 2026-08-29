//! 布局领域模型：画布布局保存请求与渲染坐标（唯一副本）。

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

/// 布局画布类型白名单（与前端可视化画布类型一一对应）。
pub const LAYOUT_TYPES: [&str; 2] = ["workstation", "cabinet"];

/// 校验布局类型取值（`server.visualization.type_unsupported`）。
fn validate_layout_type(value: &str) -> Result<(), validator::ValidationError> {
    if LAYOUT_TYPES.contains(&value) {
        Ok(())
    } else {
        Err(validator::ValidationError::new(
            "server.visualization.type_unsupported",
        ))
    }
}

// ==================== 布局模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
}

impl Position {
    /// SVG 渲染坐标取整（四舍五入并钳制到 i32 范围）。
    pub fn x_i32(&self) -> i32 {
        self.x.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
    }

    pub fn y_i32(&self) -> i32 {
        self.y.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
    }

    /// 尺寸/旋转取整（负值钳制为 0；旋转钳制到 0..=360）。
    pub fn width_i32(&self) -> i32 {
        self.width.round().clamp(0.0, i32::MAX as f64) as i32
    }

    pub fn height_i32(&self) -> i32 {
        self.height.round().clamp(0.0, i32::MAX as f64) as i32
    }

    pub fn rotation_i32(&self) -> i32 {
        self.rotation.round().clamp(0.0, 360.0) as i32
    }
}

/// 布局保存请求（`POST /api/resources/layouts`）。
///
/// 约定：两种画布类型均必需 `room_id`，且布局项非空；`element_type`
/// 长度上限对齐 `element_layouts.element_type VARCHAR(20)`。
#[derive(Debug, Serialize, Deserialize, Clone, Validate)]
pub struct LayoutSaveRequest {
    #[validate(custom(function = validate_layout_type))]
    pub r#type: String,
    pub room_id: Uuid,
    #[validate(length(min = 1, message = "server.visualization.layout_empty"))]
    pub layout: Vec<LayoutItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Validate)]
pub struct LayoutItem {
    pub id: Uuid,
    pub position: Position,
    #[validate(length(max = 20, message = "server.visualization.element_type_too_long"))]
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
        // r#type / room_id / layout 数组均正确解析；未知字段被忽略
        let req: LayoutSaveRequest = serde_json::from_value(serde_json::json!({
            "type": "workstation",
            "room_id": Uuid::new_v4(),
            "cabinet_id": null,
            "layout": [
                {
                    "id": Uuid::new_v4(),
                    "position": { "x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0, "rotation": 0.0 },
                    "element_type": "door"
                }
            ]
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.r#type, "workstation");
        assert_eq!(req.layout.len(), 1);
        Ok(())
    }

    #[test]
    fn test_layout_save_request_rejects_unknown_type() -> Result<(), serde_json::Error> {
        // r#type 不在白名单（workstation/cabinet）时校验失败
        let req: LayoutSaveRequest = serde_json::from_value(serde_json::json!({
            "type": "room",
            "room_id": Uuid::new_v4(),
            "layout": [
                {
                    "id": Uuid::new_v4(),
                    "position": { "x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0, "rotation": 0.0 },
                    "element_type": "workstation"
                }
            ]
        }))?;
        let errors = req.validate().unwrap_err();
        assert!(errors.errors().contains_key("r#type"));
        Ok(())
    }

    #[test]
    fn test_layout_save_request_empty_layout_rejected() -> Result<(), serde_json::Error> {
        // 空布局不再合法：保存即整表 upsert，空列表是无意义写入
        let req: LayoutSaveRequest = serde_json::from_value(serde_json::json!({
            "type": "cabinet",
            "room_id": Uuid::new_v4(),
            "layout": []
        }))?;
        let errors = req.validate().unwrap_err();
        assert!(errors.errors().contains_key("layout"));
        Ok(())
    }

    #[test]
    fn test_layout_save_request_missing_room_id_rejected() {
        // room_id 为必填字段，缺失或 null 时反序列化失败
        let missing: Result<LayoutSaveRequest, _> = serde_json::from_value(serde_json::json!({
            "type": "workstation",
            "layout": []
        }));
        assert!(missing.is_err());

        let null_: Result<LayoutSaveRequest, _> = serde_json::from_value(serde_json::json!({
            "type": "workstation",
            "room_id": null,
            "layout": []
        }));
        assert!(null_.is_err());
    }

    #[test]
    fn test_layout_item_element_type_too_long_rejected() -> Result<(), serde_json::Error> {
        // element_type 上限 20 字符，对齐 element_layouts.element_type VARCHAR(20)
        let item: LayoutItem = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "position": { "x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0, "rotation": 0.0 },
            "element_type": "a".repeat(21)
        }))?;
        let errors = item.validate().unwrap_err();
        assert!(errors.errors().contains_key("element_type"));
        Ok(())
    }

    #[test]
    fn test_layout_save_request_missing_type_rejected() {
        // type 为必填字段，缺失时反序列化失败
        let result: Result<LayoutSaveRequest, _> = serde_json::from_value(serde_json::json!({
            "room_id": Uuid::new_v4(),
            "layout": []
        }));
        assert!(result.is_err());
    }
}
