//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::{Validate, ValidationError};

// ==================== 组织管理模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct OrgTemplate {
    pub id: Uuid,
    pub name: String,
    pub levels: serde_json::Value,
    pub icons: serde_json::Value,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct OrgTemplateSummary {
    pub id: Uuid,
    pub name: String,
    pub levels: serde_json::Value,
    pub icons: serde_json::Value,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct OrgTemplateCreate {
    #[validate(length(min = 1, max = 100, message = "模板名称长度必须在1到100个字符之间"))]
    pub name: String,
    pub levels: serde_json::Value,
    pub icons: Option<serde_json::Value>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct OrgTemplateUpdate {
    #[validate(length(min = 1, max = 100, message = "模板名称长度必须在1到100个字符之间"))]
    pub name: Option<String>,
    pub levels: Option<serde_json::Value>,
    pub icons: Option<serde_json::Value>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub type_path: String,
    pub parent_id: Option<Uuid>,
    pub description: Option<String>,
    pub template_id: Option<Uuid>,
    pub level_index: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrganizationTreeNode {
    pub id: Uuid,
    pub name: String,
    pub org_type: String,
    pub parent_id: Option<Uuid>,
    pub description: Option<String>,
    pub template_id: Option<Uuid>,
    pub level_index: i32,
    pub children: Vec<OrganizationTreeNode>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrganizationWithChildren {
    pub id: Uuid,
    pub name: String,
    pub org_type: String,
    pub parent_id: Option<Uuid>,
    pub parent_name: Option<String>,
    pub description: Option<String>,
    pub template_id: Option<Uuid>,
    pub level_index: i32,
    pub children: Vec<Organization>,
    pub child_count: i64,
    pub room_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 校验类型路径：非空且格式合法（点分隔的数字索引）
pub fn validate_type_path(type_path: &str) -> Result<(), ValidationError> {
    if type_path.trim().is_empty() {
        return Err(ValidationError::new("类型路径不能为空"));
    }
    for segment in type_path.split('.') {
        if segment.parse::<usize>().is_err() {
            return Err(ValidationError::new("类型路径格式错误"));
        }
    }
    Ok(())
}

pub fn validate_type_path_option(type_path: &&String) -> Result<(), ValidationError> {
    validate_type_path(type_path)
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct OrganizationCreate {
    #[validate(length(min = 1, max = 100, message = "组织名称长度必须在1到100个字符之间"))]
    pub name: String,
    #[validate(custom(
        function = "crate::models::validate_type_path",
        message = "类型路径格式错误"
    ))]
    pub type_path: String,
    pub parent_id: Option<Uuid>,
    pub template_id: Option<Uuid>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct OrganizationUpdate {
    #[validate(length(min = 1, max = 100, message = "组织名称长度必须在1到100个字符之间"))]
    pub name: Option<String>,
    #[validate(custom(
        function = "crate::models::validate_type_path_option",
        message = "类型路径格式错误"
    ))]
    pub type_path: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}
