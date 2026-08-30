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

pub use foims_scheduler::{ScheduledTask, TaskLog};

#[derive(Debug, Serialize, Deserialize, Clone, Validate)]
pub struct ScheduledTaskCreate {
    #[validate(length(min = 1, max = 100, message = "server.task.validation.name_length"))]
    pub name: String,
    #[validate(length(min = 1, max = 50, message = "server.task.validation.type_length"))]
    pub task_type: String,
    #[validate(length(min = 1, max = 100, message = "server.task.validation.cron_length"))]
    pub cron_expression: String,
    pub enabled: Option<bool>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Validate)]
pub struct ScheduledTaskUpdate {
    #[validate(length(min = 1, max = 100, message = "server.task.validation.name_length"))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 50, message = "server.task.validation.type_length"))]
    pub task_type: Option<String>,
    #[validate(length(min = 1, max = 100, message = "server.task.validation.cron_length"))]
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

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    // ---------- 定时任务请求 ----------

    #[test]
    fn test_scheduled_task_create_valid() -> Result<(), serde_json::Error> {
        let req: ScheduledTaskCreate = serde_json::from_value(serde_json::json!({
            "name": "IP 存活探测",
            "task_type": "ip_scan",
            "cron_expression": "0 */5 * * * *",
            "enabled": true,
            "config": { "timeout_ms": 500 }
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.enabled, Some(true));
        Ok(())
    }

    #[test]
    fn test_scheduled_task_create_invalid() -> Result<(), serde_json::Error> {
        // 空名称 / 空类型 / 空 cron 均拒绝
        let req: ScheduledTaskCreate = serde_json::from_value(serde_json::json!({
            "name": "",
            "task_type": "",
            "cron_expression": ""
        }))?;
        let Err(errors) = req.validate() else {
            panic!("空字段应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        assert!(errors.errors().contains_key("task_type"));
        assert!(errors.errors().contains_key("cron_expression"));
        Ok(())
    }

    #[test]
    fn test_scheduled_task_update_valid_and_invalid() -> Result<(), serde_json::Error> {
        let ok: ScheduledTaskUpdate = serde_json::from_value(serde_json::json!({
            "name": "新任务名",
            "enabled": false
        }))?;
        assert!(ok.validate().is_ok());

        let bad: ScheduledTaskUpdate = serde_json::from_value(serde_json::json!({ "name": "" }))?;
        let Err(errors) = bad.validate() else {
            panic!("空任务名应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));

        // 全缺省通过
        let empty: ScheduledTaskUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert!(empty.validate().is_ok());
        Ok(())
    }

    // ---------- 日志实体序列化往返 ----------

    #[test]
    fn test_operation_log_serde_roundtrip() -> Result<(), serde_json::Error> {
        let log = OperationLog {
            id: Uuid::new_v4(),
            user_id: Some(Uuid::new_v4()),
            username: Some("admin".to_string()),
            action: "create".to_string(),
            operation_type: "create".to_string(),
            resource_type: "device".to_string(),
            resource_id: Some(Uuid::new_v4()),
            details: Some(serde_json::json!({ "name": "核心交换机" })),
            result: true,
            ip_address: "192.168.1.10".to_string(),
            created_at: Utc::now(),
        };
        let first = serde_json::to_value(&log)?;
        let back: OperationLog = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_login_log_serde_roundtrip() -> Result<(), serde_json::Error> {
        let log = LoginLog {
            id: Uuid::new_v4(),
            username: "admin".to_string(),
            ip_address: "192.168.1.10".to_string(),
            user_agent: Some("Mozilla/5.0".to_string()),
            success: false,
            error_message: Some("密码错误".to_string()),
            created_at: Utc::now(),
        };
        let first = serde_json::to_value(&log)?;
        let back: LoginLog = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_notification_serde_roundtrip() -> Result<(), serde_json::Error> {
        let n = Notification {
            id: Uuid::new_v4(),
            user_id: None,
            title: "MAC 地址变更".to_string(),
            content: r#"{"key":"server.notification.mac_change.body"}"#.to_string(),
            notification_type: "mac_change".to_string(),
            read: false,
            created_at: Utc::now(),
        };
        let first = serde_json::to_value(&n)?;
        let back: Notification = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }
}
