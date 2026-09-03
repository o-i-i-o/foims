//! 布局领域模型：画布布局保存请求与渲染坐标（唯一副本）。

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

/// 布局画布类型白名单（与前端可视化画布类型一一对应）。
pub const LAYOUT_TYPES: [&str; 2] = ["workstation", "cabinet"];

/// 坐标/尺寸绝对值上限：拦截 `1e400` 之类解析为 inf 的极值，
/// 防止写库前才被 i32 饱和钳制（手工录入与拖拽保存同一口径）。
pub const POSITION_COORD_LIMIT: f64 = 1_000_000.0;

/// 旋转角绝对值上限：允许 -360..=360（负角落库时规范化为等价正角），
/// 超出该范围的旋转无渲染意义且会与落库钳制口径不一致。
pub const ROTATION_LIMIT: f64 = 360.0;

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

/// 校验单个坐标值：必须为有限数且绝对值不超上限。
fn validate_coord(value: f64) -> Result<(), validator::ValidationError> {
    if value.is_finite() && value.abs() <= POSITION_COORD_LIMIT {
        Ok(())
    } else {
        Err(validator::ValidationError::new(
            "server.visualization.position_invalid",
        ))
    }
}

/// 校验旋转角：必须为有限数且在 -360..=360 内
/// （落库前经 `rotation_i32` 钳位到 0..=360，保存与回读一致）。
fn validate_rotation(value: f64) -> Result<(), validator::ValidationError> {
    if value.is_finite() && value.abs() <= ROTATION_LIMIT {
        Ok(())
    } else {
        Err(validator::ValidationError::new(
            "server.visualization.position_invalid",
        ))
    }
}

/// 校验布局坐标：x/y/width/height 受坐标上限约束、rotation 受旋转角范围约束，
/// width/height 必须为正（尺寸非正的布局项无渲染意义）。
fn validate_position_coords(position: &Position) -> Result<(), validator::ValidationError> {
    for value in [position.x, position.y, position.width, position.height] {
        validate_coord(value)?;
    }
    validate_rotation(position.rotation)?;
    if position.width > 0.0 && position.height > 0.0 {
        Ok(())
    } else {
        Err(validator::ValidationError::new(
            "server.visualization.position_invalid",
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

    /// 尺寸取整（负值钳制为 0）。
    pub fn width_i32(&self) -> i32 {
        self.width.round().clamp(0.0, i32::MAX as f64) as i32
    }

    pub fn height_i32(&self) -> i32 {
        self.height.round().clamp(0.0, i32::MAX as f64) as i32
    }

    /// 旋转取整：钳位到 0..=360（负角与超上限角一律收敛到有效区间，
    /// 落库前经本方法规范化，写库与回读一致）。
    pub fn rotation_i32(&self) -> i32 {
        (self.rotation.round() as i32).clamp(0, 360)
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
    /// nested 显式开启：validator derive 不自动展开 Vec 元素，
    /// 缺失时 LayoutItem 的坐标/element_type 校验在请求路径不会执行
    #[validate(nested)]
    pub layout: Vec<LayoutItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Validate)]
#[validate(schema(function = validate_layout_item))]
pub struct LayoutItem {
    pub id: Uuid,
    pub position: Position,
    #[validate(length(max = 20, message = "server.visualization.element_type_too_long"))]
    pub element_type: String,
}

/// 布局项整体校验：坐标有限性/范围 + 尺寸为正
fn validate_layout_item(item: &LayoutItem) -> Result<(), validator::ValidationError> {
    validate_position_coords(&item.position)
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
    fn test_layout_save_request_nested_item_validation() -> Result<(), serde_json::Error> {
        // 嵌套校验生效：请求内布局项的非法坐标应使整个请求校验失败
        let req: LayoutSaveRequest = serde_json::from_value(serde_json::json!({
            "type": "workstation",
            "room_id": Uuid::new_v4(),
            "layout": [
                {
                    "id": Uuid::new_v4(),
                    "position": { "x": 2e6, "y": 0.0, "width": 3.0, "height": 4.0, "rotation": 0.0 },
                    "element_type": "workstation"
                }
            ]
        }))?;
        assert!(req.validate().is_err(), "布局项坐标超上限应使请求被拒绝");

        let req: LayoutSaveRequest = serde_json::from_value(serde_json::json!({
            "type": "cabinet",
            "room_id": Uuid::new_v4(),
            "layout": [
                {
                    "id": Uuid::new_v4(),
                    "position": { "x": 0.0, "y": 0.0, "width": 0.0, "height": 4.0, "rotation": 0.0 },
                    "element_type": "cabinet"
                }
            ]
        }))?;
        assert!(req.validate().is_err(), "布局项零尺寸应使请求被拒绝");
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
    fn test_layout_item_coordinate_validation() -> Result<(), serde_json::Error> {
        let base = |x: f64, y: f64, w: f64, h: f64| {
            serde_json::json!({
                "id": Uuid::new_v4(),
                "position": { "x": x, "y": y, "width": w, "height": h, "rotation": 0.0 },
                "element_type": "workstation"
            })
        };

        // 合法：负坐标允许（拖拽画布），绝对值在上限内
        let ok: LayoutItem = serde_json::from_value(base(-500.0, -500.0, 120.0, 60.0))?;
        assert!(ok.validate().is_ok(), "合法坐标应通过");

        // 超上限（写库前才被 i32 饱和钳制的极值）拒绝
        let huge: LayoutItem = serde_json::from_value(base(2e6, 0.0, 10.0, 10.0))?;
        assert!(huge.validate().is_err(), "超上限坐标应被拒绝");

        // 尺寸非正拒绝
        let zero_size: LayoutItem = serde_json::from_value(base(0.0, 0.0, 0.0, 10.0))?;
        assert!(zero_size.validate().is_err(), "零尺寸应被拒绝");

        let negative_size: LayoutItem = serde_json::from_value(base(0.0, 0.0, 10.0, -3.0))?;
        assert!(negative_size.validate().is_err(), "负尺寸应被拒绝");

        // 边界：恰为上限 1_000_000 合法
        let boundary: LayoutItem =
            serde_json::from_value(base(1_000_000.0, -1_000_000.0, 10.0, 10.0))?;
        assert!(boundary.validate().is_ok(), "上限边界坐标应合法");
        Ok(())
    }

    #[test]
    fn test_layout_item_rotation_range() -> Result<(), serde_json::Error> {
        let item = |rotation: f64| {
            serde_json::json!({
                "id": Uuid::new_v4(),
                "position": { "x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0, "rotation": rotation },
                "element_type": "workstation"
            })
        };

        // 合法：-360..=360（负角落库时规范化为等价正角）
        for ok_rotation in [-360.0, -90.0, 0.0, 90.0, 360.0] {
            let req: LayoutItem = serde_json::from_value(item(ok_rotation))?;
            assert!(req.validate().is_ok(), "旋转角 {ok_rotation} 应合法");
        }

        // 非法：超上限 / 低于下限 / 非有限数
        for bad_rotation in [360.1, 400.0, -360.1] {
            let req: LayoutItem = serde_json::from_value(item(bad_rotation))?;
            assert!(req.validate().is_err(), "旋转角 {bad_rotation} 应被拒绝");
        }

        // NaN/inf 不落库（serde_json 不支持 NaN，直接构造结构体验证校验函数）
        assert!(validate_rotation(f64::NAN).is_err());
        assert!(validate_rotation(f64::INFINITY).is_err());
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
