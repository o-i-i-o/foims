//! 任务执行日志记录。

use chrono::Utc;
use foims_common::{log_debug, log_error, log_warn, msg};
use uuid::Uuid;

use crate::cron::calculate_next_run;
use crate::error::{SchedulerError, SchedulerResult};
use crate::models::ScheduledTask;
use crate::scheduler::error_message;

/// 记录任务执行日志
///
/// `started_at` 为执行器开始执行的真实时刻（调用方在执行前取
/// `Utc::now()`），结束时刻与耗时在本函数内按当前时间计算，
/// 保证审计数据（start_time/end_time/duration）真实反映执行区间。
///
/// `details` 为 i18n key（成功时为执行器返回的结果 key，失败时为
/// `key(params)` 形式的诊断串），由前端负责翻译展示。
pub async fn log_task_execution(
    pool: &sqlx::PgPool,
    task_name: &str,
    status: &str,
    details: &str,
    started_at: chrono::DateTime<Utc>,
) {
    let end_time = Utc::now();
    // 时钟回拨等异常情况下不允许出现负耗时；量纲与“立即执行”路径
    // （src/system/scheduled_task.rs）一致，统一为毫秒
    let duration = (end_time - started_at)
        .num_milliseconds()
        .clamp(0, i64::from(i32::MAX)) as i32;
    if let Err(e) = sqlx::query(
        r"INSERT INTO task_logs (id, task_name, status, details, start_time, end_time, duration)
           VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(task_name)
    .bind(status)
    .bind(sqlx::types::Json(serde_json::json!({ "message": details })))
    .bind(started_at)
    .bind(end_time)
    .bind(duration)
    .execute(pool)
    .await
    {
        log_warn!("log.task.log_record_failed", error = e);
    }
}

/// 从数据库同步用户定时任务的 next_run_at
pub async fn sync_user_tasks_from_db(pool: &sqlx::PgPool) -> SchedulerResult<()> {
    let tasks: Vec<ScheduledTask> = sqlx::query_as(
        "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at
         FROM scheduled_tasks WHERE enabled = true",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        SchedulerError::Database(msg("server.task.query_failed").with("error", e))
    })?;

    for task in tasks {
        if let Err(e) = update_next_run_at(pool, &task).await {
            log_error!(
                "log.task.next_run_update_failed",
                name = task.name,
                error = error_message(&e).log_string()
            );
        }
    }

    Ok(())
}

async fn update_next_run_at(pool: &sqlx::PgPool, task: &ScheduledTask) -> SchedulerResult<()> {
    let cron_expr = task.cron_expression.clone();
    let next_run = tokio::task::spawn_blocking(move || calculate_next_run(&cron_expr))
        .await
        .map_err(|e| {
            SchedulerError::Internal(msg("server.task.next_run_calc_task_failed").with("error", e))
        })??;

    // 只更新调度字段：updated_at 语义是"最后配置变更时间"，例行调度
    // 每 5 分钟覆盖会破坏该语义（对照手动编辑路径）
    //
    // 条件前移：仅当现有 next_run_at 为空或晚于本次计算值时才写入，
    // 不允许推后覆盖——本同步与手动触发/到期派发/其他调度器实例并发时，
    // 无条件覆盖可能把一次尚未执行的到期调度推后吞掉
    let updated = sqlx::query(
        "UPDATE scheduled_tasks SET next_run_at = $1
         WHERE id = $2 AND (next_run_at IS NULL OR next_run_at > $1)",
    )
    .bind(next_run)
    .bind(task.id)
    .execute(pool)
    .await
    .map_err(|e| {
        SchedulerError::Database(msg("server.task.next_run_update_failed").with("error", e))
    })?;

    if updated.rows_affected() == 0 {
        // 未生效：现有 next_run_at 不晚于计算值（或任务已被并发停用/删除），保留原值
        log_debug!(
            "log.task.next_run_sync_skipped",
            name = task.name,
            next_run = next_run
        );
    }

    Ok(())
}
