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
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}
