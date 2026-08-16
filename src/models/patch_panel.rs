//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ==================== 配线架模型 ====================
// 配线架与信息点是不同概念，隶属机柜，由机柜弹窗内联同步管理

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct PatchPanel {
    pub id: Uuid,
    pub name: String,
    pub cabinet_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct PatchPanelWithDetails {
    pub id: Uuid,
    pub name: String,
    pub cabinet_id: Uuid,
    pub cabinet_name: String,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
