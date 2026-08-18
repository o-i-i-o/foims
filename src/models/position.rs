//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use super::ip::IpManager;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== 机位模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct CabinetPosition {
    pub id: Uuid,
    pub name: String,
    pub cabinet_id: Option<Uuid>,
    pub start_u: i32,
    pub end_u: i32,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct CabinetPositionWithDetails {
    pub id: Uuid,
    pub name: String,
    pub cabinet_id: Option<Uuid>,
    pub cabinet_name: Option<String>,
    pub room_id: Option<Uuid>,
    pub room_name: Option<String>,
    pub start_u: i32,
    pub end_u: i32,
    /// IP 明细不从 SQL 映射（列表不携带、详情单独查询后手动填充）
    #[sqlx(skip)]
    pub ips: Vec<IpManager>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetPositionCreate {
    #[validate(length(min = 1, max = 50, message = "server.position.validation.name_length"))]
    pub name: String,
    pub cabinet_id: Option<Uuid>,
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
pub struct CabinetPositionUpdate {
    #[validate(length(min = 1, max = 50, message = "server.position.validation.name_length"))]
    pub name: Option<String>,
    pub cabinet_id: Option<Uuid>,
    #[validate(range(
        min = 1,
        max = 48,
        message = "server.position.validation.start_u_range"
    ))]
    pub start_u: Option<i32>,
    #[validate(range(min = 1, max = 48, message = "server.position.validation.end_u_range"))]
    pub end_u: Option<i32>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}
