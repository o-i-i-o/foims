use chrono::Utc;
use tracing::{error, warn};
use uuid::Uuid;

use crate::cron::calculate_next_run;
use crate::error::SchedulerResult;
use crate::models::ScheduledTask;

/// 记录任务执行日志
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
        warn!("记录任务日志失败: {}", e);
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
    .map_err(|e| crate::error::SchedulerError::Database(format!("查询用户任务失败: {e}")))?;

    for task in tasks {
        if let Err(e) = update_next_run_at(pool, &task).await {
            error!("更新任务 {} 的下次执行时间失败: {}", task.name, e);
        }
    }

    Ok(())
}

async fn update_next_run_at(pool: &sqlx::PgPool, task: &ScheduledTask) -> SchedulerResult<()> {
    let cron_expr = task.cron_expression.clone();
    let next_run = tokio::task::spawn_blocking(move || calculate_next_run(&cron_expr))
        .await
        .map_err(|e| {
            crate::error::SchedulerError::Internal(format!("计算下次执行时间任务失败: {e}"))
        })??;

    sqlx::query("UPDATE scheduled_tasks SET next_run_at = $1, updated_at = $2 WHERE id = $3")
        .bind(next_run)
        .bind(Utc::now())
        .bind(task.id)
        .execute(pool)
        .await
        .map_err(|e| {
            crate::error::SchedulerError::Database(format!("更新下次执行时间失败: {e}"))
        })?;

    Ok(())
}
