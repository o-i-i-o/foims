//! 定时任务数据模型。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct ScheduledTask {
    pub id: uuid::Uuid,
    pub name: String,
    pub task_type: String,
    pub cron_expression: String,
    pub enabled: bool,
    pub config: serde_json::Value,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_result: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct TaskLog {
    pub id: uuid::Uuid,
    pub task_name: String,
    pub status: String,
    pub details: serde_json::Value,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration: Option<i32>,
}

/// 数据库连接配置（复用 ipma-data-manager 的定义）
pub use ipma_data_manager::DatabaseConfig;

/// 任务执行上下文
pub struct TaskContext {
    pub pool: sqlx::PgPool,
    pub config: serde_json::Value,
    pub db_config: DatabaseConfig,
}
