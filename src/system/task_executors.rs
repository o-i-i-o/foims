use async_trait::async_trait;
use ipma_scheduler::{SchedulerError, SchedulerResult, TaskContext, TaskExecutor};
use uuid::Uuid;

/// 数据库备份任务执行器
pub struct BackupTaskExecutor;

#[async_trait]
impl TaskExecutor for BackupTaskExecutor {
    fn task_type(&self) -> &str {
        "backup"
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String> {
        let db_config = ctx.db_config.clone();

        let result = tokio::task::spawn_blocking(move || {
            let backup_dir = "/var/lib/ipma/backups";
            let path = ipma_data_manager::backup_to_file(&db_config, backup_dir, "ipma_backup")?;
            ipma_data_manager::cleanup_old_backup_files(backup_dir, 7)?;
            let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            Ok::<String, ipma_data_manager::DataError>(format!(
                "备份成功: {path} ({:.2} MB)",
                file_size as f64 / (1024.0 * 1024.0)
            ))
        })
        .await;

        match result {
            Ok(Ok(msg)) => Ok(msg),
            Ok(Err(e)) => Err(SchedulerError::Execution(e.to_string())),
            Err(e) => Err(SchedulerError::Execution(format!("备份任务执行异常: {e}"))),
        }
    }
}

/// 过期 Token 清理任务执行器
pub struct TokenCleanupTaskExecutor;

#[async_trait]
impl TaskExecutor for TokenCleanupTaskExecutor {
    fn task_type(&self) -> &str {
        "token_cleanup"
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String> {
        let count = crate::utils::cleanup_expired_revoked_tokens(&ctx.pool)
            .await
            .map_err(|e| SchedulerError::Execution(format!("Token清理失败: {e}")))?;
        Ok(format!("清理了 {count} 个过期token"))
    }
}

/// Token 使用记录清理任务执行器
pub struct TokenUsageCleanupTaskExecutor;

#[async_trait]
impl TaskExecutor for TokenUsageCleanupTaskExecutor {
    fn task_type(&self) -> &str {
        "token_usage_cleanup"
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String> {
        let days = ctx
            .config
            .get("days")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(30) as i32;

        let count = crate::utils::cleanup_old_token_usage(&ctx.pool, days)
            .await
            .map_err(|e| SchedulerError::Execution(format!("Token使用记录清理失败: {e}")))?;
        Ok(format!("清理了 {count} 条{days}天前的token_usage记录"))
    }
}

/// 日志清理任务执行器
pub struct LogCleanupTaskExecutor;

#[async_trait]
impl TaskExecutor for LogCleanupTaskExecutor {
    fn task_type(&self) -> &str {
        "log_cleanup"
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String> {
        let days = ctx
            .config
            .get("days")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(30) as i32;

        let deleted = ipma_data_manager::clear_logs_core(&ctx.pool, days, "all")
            .await
            .map_err(|e| SchedulerError::Execution(e.to_string()))?;

        Ok(format!("清理了 {deleted} 条日志记录"))
    }
}

/// MAC 同步任务执行器
pub struct MacSyncTaskExecutor;

#[async_trait]
impl TaskExecutor for MacSyncTaskExecutor {
    fn task_type(&self) -> &str {
        "mac_sync"
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String> {
        let device_id = ctx
            .config
            .get("device_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        let network_id = ctx
            .config
            .get("network_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        match (device_id, network_id) {
            (Some(device_id), Some(network_id)) => {
                crate::resource::ip::pull_ip_managers_internal(&ctx.pool, device_id, network_id)
                    .await
                    .map(|()| "MAC同步成功".to_string())
                    .map_err(|e| SchedulerError::Execution(format!("MAC同步失败: {e}")))
            }
            _ => Err(SchedulerError::Validation(
                "MAC同步任务需要配置device_id和network_id".to_string(),
            )),
        }
    }
}
