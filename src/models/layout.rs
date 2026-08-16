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
