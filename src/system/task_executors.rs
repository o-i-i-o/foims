//! 定时任务执行器实现（绑定具体业务任务类型）。

use async_trait::async_trait;
use foims_scheduler::{SchedulerError, SchedulerResult, TaskContext, TaskExecutor};
use uuid::Uuid;

use foims_common::{AppMessage, log_debug, log_info, msg};

/// 提取数据层错误内部的 i18n 消息（避免拼接中文前缀导致文案泄漏）
pub(crate) fn data_error_message(e: foims_data_management::DataError) -> AppMessage {
    match e {
        foims_data_management::DataError::Database(m)
        | foims_data_management::DataError::NotFound(m)
        | foims_data_management::DataError::Validation(m)
        | foims_data_management::DataError::Conflict(m)
        | foims_data_management::DataError::Internal(m) => m,
    }
}

/// 天数类任务（log_cleanup / ip_status_sync）days 的
/// 合法区间（1..=3650，约 10 年）。与 scheduled_task.rs 的创建/更新校验口径一致：
/// i64 → i32 直接 `as` 截断会把超大值变成负数（SQL 阈值落到未来导致
/// 全表误删）或 0（清空全部审计日志），必须先做范围校验。
const DAYS_CONFIG_RANGE: std::ops::RangeInclusive<i64> = 1..=3650;

/// 解析天数类任务（log_cleanup / ip_status_sync）的
/// days 配置，越界或缺失时的处理：缺失回落默认 30 天，越界返回校验错误。
fn parse_days_config(config: &serde_json::Value) -> Result<i32, SchedulerError> {
    let days = config
        .get("days")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(30);
    if !DAYS_CONFIG_RANGE.contains(&days) {
        return Err(SchedulerError::Validation(
            msg("server.common.invalid_param").with("param", "days (1-3650)"),
        ));
    }
    // 已通过 1..=3650 校验，转换不会截断
    i32::try_from(days).map_err(|e| {
        SchedulerError::Validation(
            msg("server.common.invalid_param").with("param", format!("days: {e}")),
        )
    })
}

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
            let backup_dir = "/var/lib/foims/backups";
            let path =
                foims_data_management::backup_to_file(&db_config, backup_dir, "foims_backup")?;
            foims_data_management::cleanup_old_backup_files(backup_dir, 7)?;
            let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            Ok::<(String, String), foims_data_management::DataError>((
                path,
                format!("{:.2}", file_size as f64 / (1024.0 * 1024.0)),
            ))
        })
        .await;

        match result {
            // 备份路径与体积写入运维日志，任务结果仅保留消息 key
            Ok(Ok((path, size_mb))) => {
                log_info!("log.task.backup_completed", path = path, size_mb = size_mb);
                Ok("server.task.backup_completed".to_string())
            }
            Ok(Err(e)) => Err(SchedulerError::Execution(data_error_message(e))),
            Err(e) => Err(SchedulerError::Execution(
                msg("server.task.backup_task_failed").with("error", e),
            )),
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

    /// 每小时例行运行且几乎总有产出，例行成功日志降为 debug 避免刷屏
    fn debug_routine_logs(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String> {
        let count = foims_auth::jwt::cleanup_expired_revoked_tokens(&ctx.pool)
            .await
            .map_err(|e| {
                SchedulerError::Execution(msg("server.task.token_cleanup_failed").with("error", e))
            })?;
        log_debug!("log.task.token_cleanup_completed", count = count);
        Ok("server.task.token_cleanup_completed".to_string())
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
        let days = parse_days_config(&ctx.config)?;

        let deleted = foims_data_management::clear_logs_core(&ctx.pool, days, "all")
            .await
            .map_err(|e| SchedulerError::Execution(data_error_message(e)))?;

        log_info!(
            "log.task.log_cleanup_completed",
            count = deleted,
            days = days
        );
        Ok("server.task.log_cleanup_completed".to_string())
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
        // subnet_id 可选：缺省（含旧配置缺失/损坏）时同步该设备的全部子网
        let subnet_id = ctx
            .config
            .get("subnet_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        match device_id {
            Some(device_id) => {
                foims_resource::ip::pull_ip_details_internal(&ctx.pool, device_id, subnet_id)
                    .await
                    .map(|()| "server.task.mac_sync_completed".to_string())
                    .map_err(|e| SchedulerError::Execution(foims_common::AppMessage::new(e)))
            }
            None => Err(SchedulerError::Validation(msg(
                "server.task.mac_sync_missing_config",
            ))),
        }
    }
}

/// IP 状态同步任务执行器：按 last_seen 新鲜度自动翻转 active/inactive，
/// 使仪表盘 IP 状态分布反映地址观测活性。days 为判停阈值（1..=3650，默认 30）。
pub struct IpStatusSyncTaskExecutor;

#[async_trait]
impl TaskExecutor for IpStatusSyncTaskExecutor {
    fn task_type(&self) -> &str {
        "ip_status_sync"
    }

    /// 每日例行运行且多数轮次无翻转，例行成功日志降为 debug 避免刷屏
    fn debug_routine_logs(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String> {
        let days = parse_days_config(&ctx.config)?;

        let (stale, recovered) = foims_resource::ip::sync_ip_status_by_staleness(&ctx.pool, days)
            .await
            .map_err(|e| {
                SchedulerError::Execution(msg("server.task.ip_status_sync_failed").with("error", e))
            })?;

        log_debug!(
            "log.task.ip_status_sync_completed",
            stale = stale,
            recovered = recovered,
            days = days
        );
        Ok("server.task.ip_status_sync_completed".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days配置_缺失回落默认30() {
        assert_eq!(
            parse_days_config(&serde_json::json!({})).map_err(|_| ()),
            Ok(30)
        );
        assert_eq!(
            parse_days_config(&serde_json::json!({"other": 1})).map_err(|_| ()),
            Ok(30)
        );
    }

    #[test]
    fn days配置_区间内合法() {
        assert_eq!(
            parse_days_config(&serde_json::json!({"days": 1})).map_err(|_| ()),
            Ok(1)
        );
        assert_eq!(
            parse_days_config(&serde_json::json!({"days": 3650})).map_err(|_| ()),
            Ok(3650)
        );
    }

    #[test]
    fn days配置_越界拒绝() {
        for bad in [0, -1, 3651, i64::MAX] {
            assert!(
                parse_days_config(&serde_json::json!({ "days": bad })).is_err(),
                "days={bad} 应被拒绝"
            );
        }
    }
}
