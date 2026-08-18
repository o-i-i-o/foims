//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use super::network::NetworkInfo;
use super::workstation::NetOutletBrief;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== 房间模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Room {
    pub id: Uuid,
    pub name: String,
    pub room_type: String,
    pub org_id: Option<Uuid>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct RoomNetwork {
    pub id: Uuid,
    pub room_id: Uuid,
    pub network_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct RoomNetworkDetail {
    pub room_id: Uuid,
    pub room_name: String,
    pub room_type: String,
    pub description: Option<String>,
    pub network_id: Option<Uuid>,
    pub network_name: Option<String>,
    pub network_region: Option<String>,
    pub network_region_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RoomWithNetworks {
    pub id: Uuid,
    pub name: String,
    pub room_type: String,
    pub org_id: Option<Uuid>,
    pub org_name: Option<String>,
    pub description: Option<String>,
    pub networks: Vec<NetworkInfo>,
    pub workstation_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workstations: Option<Vec<WorkstationBrief>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cabinets: Option<Vec<CabinetBrief>>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub net_outlets: Vec<NetOutletBrief>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkstationBrief {
    pub id: Uuid,
    pub name: String,
    pub manager: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CabinetBrief {
    pub id: Uuid,
    pub name: String,
    pub capacity: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PositionBrief {
    pub id: Uuid,
    pub name: String,
    pub start_u: i32,
    pub end_u: i32,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RoomCreate {
    #[validate(length(min = 1, max = 50, message = "server.room.validation.name_length"))]
    pub name: String,
    #[validate(custom(
        function = "crate::models::validate_room_type_string",
        message = "server.room.validation.type_invalid"
    ))]
    pub room_type: String,
    pub org_id: Option<Uuid>,
    pub network_ids: Vec<Uuid>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RoomUpdate {
    #[validate(length(min = 1, max = 50, message = "server.room.validation.name_length"))]
    pub name: Option<String>,
    #[validate(custom(
        function = "crate::models::validate_room_type_option",
        message = "server.room.validation.type_invalid"
    ))]
    pub room_type: Option<String>,
    pub org_id: Option<Option<Uuid>>,
    pub network_ids: Option<Vec<Uuid>>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}
