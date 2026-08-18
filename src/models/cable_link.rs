//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== 物理链路模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct CableLink {
    pub id: Uuid,
    pub a_endpoint_type: String,
    pub a_endpoint_id: Uuid,
    pub b_endpoint_type: String,
    pub b_endpoint_id: Uuid,
    pub link_type: String,
    pub cable_label: Option<String>,
    pub length_m: Option<f64>,
    pub tested: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct CableLinkWithDetails {
    pub id: Uuid,
    pub a_endpoint_type: String,
    pub a_endpoint_id: Uuid,
    pub a_endpoint_label: Option<String>,
    /// A 端点所属作用域（用于编辑时回填级联选择器）
    pub a_room_id: Option<Uuid>,
    pub a_cabinet_id: Option<Uuid>,
    pub a_device_id: Option<Uuid>,
    pub b_endpoint_type: String,
    pub b_endpoint_id: Uuid,
    pub b_endpoint_label: Option<String>,
    /// B 端点所属作用域（用于编辑时回填级联选择器）
    pub b_room_id: Option<Uuid>,
    pub b_cabinet_id: Option<Uuid>,
    pub b_device_id: Option<Uuid>,
    pub link_type: String,
    pub cable_label: Option<String>,
    pub length_m: Option<f64>,
    pub tested: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CableLinkCreate {
    pub a_endpoint_type: String,
    pub a_endpoint_id: Uuid,
    pub b_endpoint_type: String,
    pub b_endpoint_id: Uuid,
    pub link_type: Option<String>,
    #[validate(length(max = 50, message = "server.cable_link.validation.cable_label_length"))]
    pub cable_label: Option<String>,
    pub length_m: Option<f64>,
    pub tested: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CableLinkUpdate {
    pub link_type: Option<String>,
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    pub cable_label: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    pub length_m: Option<Option<f64>>,
    pub tested: Option<bool>,
    // 端点可选更新（编辑模态框复用新建表单时一并提交）
    pub a_endpoint_type: Option<String>,
    pub a_endpoint_id: Option<Uuid>,
    pub b_endpoint_type: Option<String>,
    pub b_endpoint_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CablePathNode {
    pub hop_idx: i32,
    pub node_type: String,
    pub node_id: Uuid,
    pub node_label: Option<String>,
    pub cable_id: Option<Uuid>,
    pub cable_label: Option<String>,
    pub hop_type: String,
}
