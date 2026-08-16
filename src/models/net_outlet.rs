//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== 信息点模型 ====================
// 信息点特指网络插座，仅隶属房间；配线架见下方独立模型

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct NetOutlet {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct NetOutletWithDetails {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetOutletCreate {
    #[validate(length(min = 1, max = 100, message = "信息点名称长度必须在1到100个字符之间"))]
    pub name: String,
    pub room_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetOutletUpdate {
    #[validate(length(min = 1, max = 100, message = "信息点名称长度必须在1到100个字符之间"))]
    pub name: Option<String>,
    pub room_id: Option<Uuid>,
}
