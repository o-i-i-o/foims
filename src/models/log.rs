//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// ==================== 日志模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct OperationLog {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub username: Option<String>,
    pub action: String,
    pub operation_type: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub details: Option<serde_json::Value>,
    pub result: bool,
    pub ip_address: String,
    pub created_at: DateTime<Utc>,
}

// ==================== 定时任务模型 ====================

pub use ipma_scheduler::{ScheduledTask, TaskLog};

#[derive(Debug, Serialize, Deserialize, Clone, Validate)]
pub struct ScheduledTaskCreate {
    #[validate(length(min = 1, max = 100, message = "任务名称长度必须在1到100个字符之间"))]
    pub name: String,
    #[validate(length(min = 1, max = 50, message = "任务类型长度必须在1到50个字符之间"))]
    pub task_type: String,
    #[validate(length(min = 1, max = 100, message = "cron表达式长度必须在1到100个字符之间"))]
    pub cron_expression: String,
    pub enabled: Option<bool>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Validate)]
pub struct ScheduledTaskUpdate {
    #[validate(length(min = 1, max = 100, message = "任务名称长度必须在1到100个字符之间"))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 50, message = "任务类型长度必须在1到50个字符之间"))]
    pub task_type: Option<String>,
    #[validate(length(min = 1, max = 100, message = "cron表达式长度必须在1到100个字符之间"))]
    pub cron_expression: Option<String>,
    pub enabled: Option<bool>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct LoginLog {
    pub id: Uuid,
    pub username: String,
    pub ip_address: String,
    pub user_agent: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ==================== 通知模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub title: String,
    pub content: String,
    pub notification_type: String,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}
