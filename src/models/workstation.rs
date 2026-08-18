//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use super::ip::IpManager;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== 工位模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Workstation {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub manager: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct WorkstationWithDetails {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub manager: Option<String>,
    /// IP 明细不从 SQL 映射（列表不携带、详情单独查询后手动填充）
    #[sqlx(skip)]
    pub ips: Vec<IpManager>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct WorkstationCreate {
    #[validate(length(
        min = 1,
        max = 50,
        message = "server.workstation.validation.name_length"
    ))]
    pub name: String,
    pub room_id: Uuid,
    #[validate(length(max = 50, message = "server.workstation.validation.manager_length"))]
    pub manager: Option<String>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct WorkstationUpdate {
    #[validate(length(
        min = 1,
        max = 50,
        message = "server.workstation.validation.name_length"
    ))]
    pub name: Option<String>,
    pub room_id: Option<Uuid>,
    pub manager: Option<String>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

// ==================== 批量同步模型 ====================

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct WorkstationSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(
        min = 1,
        max = 50,
        message = "server.workstation.validation.name_length"
    ))]
    pub name: String,
    #[validate(length(max = 50, message = "server.workstation.validation.manager_length"))]
    pub manager: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 50, message = "server.cabinet.validation.name_length"))]
    pub name: String,
    #[validate(range(
        min = 1,
        max = 48,
        message = "server.cabinet.validation.capacity_range"
    ))]
    pub capacity: i32,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetOutletSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(
        min = 1,
        max = 100,
        message = "server.net_outlet.validation.name_length"
    ))]
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RoomNetOutletsSync {
    pub net_outlets: Vec<NetOutletSyncItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetOutletBrief {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PatchPanelBrief {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct PositionSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 50, message = "server.position.validation.name_length"))]
    pub name: String,
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
pub struct RoomChildrenSync {
    pub workstations: Option<Vec<WorkstationSyncItem>>,
    pub cabinets: Option<Vec<CabinetSyncItem>>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetPositionsSync {
    pub positions: Vec<PositionSyncItem>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct PatchPanelSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(
        min = 1,
        max = 100,
        message = "server.patch_panel.validation.name_length"
    ))]
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetPatchPanelsSync {
    pub patch_panels: Vec<PatchPanelSyncItem>,
}
