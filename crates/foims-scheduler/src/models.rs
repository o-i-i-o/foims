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

/// 数据库连接配置（复用 foims-data-management 的定义）
pub use foims_data_management::DatabaseConfig;

/// 任务执行上下文
pub struct TaskContext {
    pub pool: sqlx::PgPool,
    pub config: serde_json::Value,
    pub db_config: DatabaseConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造固定时间戳，保证序列化输出可精确断言
    fn fixed_time() -> DateTime<Utc> {
        DateTime::from_timestamp(1700000000, 0).unwrap_or_default()
    }

    #[test]
    fn scheduled_task_serde_往返保持字段() {
        let task = ScheduledTask {
            id: uuid::Uuid::nil(),
            name: "备份任务".to_string(),
            task_type: "backup".to_string(),
            cron_expression: "0 */5 * * *".to_string(),
            enabled: true,
            config: serde_json::json!({ "target": "all" }),
            last_run_at: Some(fixed_time()),
            next_run_at: Some(fixed_time()),
            last_result: Some("server.task.ok".to_string()),
            created_at: fixed_time(),
            updated_at: fixed_time(),
        };
        let json = serde_json::to_string(&task).unwrap_or_else(|e| panic!("序列化失败: {e}"));
        let back: ScheduledTask =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(back.id, task.id);
        assert_eq!(back.name, "备份任务");
        assert_eq!(back.task_type, "backup");
        assert_eq!(back.cron_expression, "0 */5 * * *");
        assert!(back.enabled);
        assert_eq!(back.config, serde_json::json!({ "target": "all" }));
        assert_eq!(back.last_run_at, Some(fixed_time()));
        assert_eq!(back.next_run_at, Some(fixed_time()));
        assert_eq!(back.last_result.as_deref(), Some("server.task.ok"));
        assert_eq!(back.created_at, fixed_time());
        assert_eq!(back.updated_at, fixed_time());
    }

    #[test]
    fn task_log_serde_往返保持字段() {
        let log = TaskLog {
            id: uuid::Uuid::nil(),
            task_name: "cleanup".to_string(),
            status: "success".to_string(),
            details: serde_json::json!({ "message": "server.task.ok" }),
            start_time: fixed_time(),
            end_time: Some(fixed_time()),
            duration: Some(42),
        };
        let json = serde_json::to_string(&log).unwrap_or_else(|e| panic!("序列化失败: {e}"));
        let back: TaskLog =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(back.task_name, "cleanup");
        assert_eq!(back.status, "success");
        assert_eq!(
            back.details,
            serde_json::json!({ "message": "server.task.ok" })
        );
        assert_eq!(back.start_time, fixed_time());
        assert_eq!(back.end_time, Some(fixed_time()));
        assert_eq!(back.duration, Some(42));
    }

    #[test]
    fn task_serde_option字段_为空时序列化为null() {
        let task = ScheduledTask {
            id: uuid::Uuid::nil(),
            name: "n".to_string(),
            task_type: "t".to_string(),
            cron_expression: "* * * * *".to_string(),
            enabled: false,
            config: serde_json::json!(null),
            last_run_at: None,
            next_run_at: None,
            last_result: None,
            created_at: fixed_time(),
            updated_at: fixed_time(),
        };
        let json = serde_json::to_string(&task).unwrap_or_else(|e| panic!("序列化失败: {e}"));
        assert!(json.contains(r#""last_run_at":null"#), "序列化输出: {json}");
        assert!(json.contains(r#""next_run_at":null"#), "序列化输出: {json}");
        assert!(json.contains(r#""last_result":null"#), "序列化输出: {json}");
        let back: ScheduledTask =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert!(back.last_run_at.is_none());
        assert!(back.next_run_at.is_none());
        assert!(back.last_result.is_none());
    }
}
