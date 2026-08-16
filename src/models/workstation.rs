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
    #[validate(length(min = 1, max = 50, message = "工位名称长度必须在1到50个字符之间"))]
    pub name: String,
    pub room_id: Uuid,
    #[validate(length(max = 50, message = "管理人长度不能超过50个字符"))]
    pub manager: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct WorkstationUpdate {
    #[validate(length(min = 1, max = 50, message = "工位名称长度必须在1到50个字符之间"))]
    pub name: Option<String>,
    pub room_id: Option<Uuid>,
    pub manager: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

// ==================== 批量同步模型 ====================

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct WorkstationSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 50, message = "工位名称长度必须在1到50个字符之间"))]
    pub name: String,
    #[validate(length(max = 50, message = "管理人长度不能超过50个字符"))]
    pub manager: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 50, message = "机柜名称长度必须在1到50个字符之间"))]
    pub name: String,
    #[validate(range(min = 1, max = 48, message = "机柜容量必须在1到48U之间"))]
    pub capacity: i32,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetOutletSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 100, message = "信息点名称长度必须在1到100个字符之间"))]
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
    #[validate(length(min = 1, max = 50, message = "机位名称长度必须在1到50个字符之间"))]
    pub name: String,
    #[validate(range(min = 1, max = 48, message = "起始U位必须在1到48之间"))]
    pub start_u: i32,
    #[validate(range(min = 1, max = 48, message = "结束U位必须在1到48之间"))]
    pub end_u: i32,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
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
    #[validate(length(min = 1, max = 100, message = "配线架名称长度必须在1到100个字符之间"))]
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetPatchPanelsSync {
    pub patch_panels: Vec<PatchPanelSyncItem>,
}
