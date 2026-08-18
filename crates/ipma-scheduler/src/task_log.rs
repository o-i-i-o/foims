//! 任务执行日志记录。

use chrono::Utc;
use ipma_common::{log_error, log_warn, msg};
use uuid::Uuid;

use crate::cron::calculate_next_run;
use crate::error::{SchedulerError, SchedulerResult};
use crate::models::ScheduledTask;
use crate::scheduler::error_message;

/// 记录任务执行日志
///
/// `details` 为 i18n key（成功时为执行器返回的结果 key，失败时为
/// `key(params)` 形式的诊断串），由前端负责翻译展示。
pub async fn log_task_execution(pool: &sqlx::PgPool, task_name: &str, status: &str, details: &str) {
    if let Err(e) = sqlx::query(
        r"INSERT INTO task_logs (id, task_name, status, details, start_time, end_time, duration)
           VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(task_name)
    .bind(status)
    .bind(sqlx::types::Json(serde_json::json!({ "message": details })))
    .bind(Utc::now())
    .bind(Utc::now())
    .bind(0i32)
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

    sqlx::query("UPDATE scheduled_tasks SET next_run_at = $1, updated_at = $2 WHERE id = $3")
        .bind(next_run)
        .bind(Utc::now())
        .bind(task.id)
        .execute(pool)
        .await
        .map_err(|e| {
            SchedulerError::Database(msg("server.task.next_run_update_failed").with("error", e))
        })?;

    Ok(())
}
