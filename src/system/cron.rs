use chrono::{Datelike, Timelike, Utc};
use std::process::Command;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};
use uuid::Uuid;

use crate::db::DbPool;
use crate::models::ScheduledTask;
use crate::utils::{cleanup_expired_revoked_tokens, cleanup_old_token_usage};

pub struct SchedulerState {
    pub scheduler: JobScheduler,
}

pub async fn start_scheduler(
    pool: Arc<DbPool>,
) -> Result<SchedulerState, Box<dyn std::error::Error>> {
    let scheduler = JobScheduler::new().await?;

    let db_config = pool.db_config.clone();

    let backup_pool = pool.clone();
    let backup_config = db_config.clone();
    let backup_job = Job::new_async("0 0 0 * * *", move |_, _| {
        let pool = backup_pool.clone();
        let config = backup_config.clone();
        Box::pin(async move {
            info!("Running data backup job...");
            let config_move = config.clone();
            let pool_conn = pool.get_conn();
            let result = tokio::task::spawn_blocking(move || {
                execute_database_backup(&config_move, &pool_conn)
            })
            .await;
            match result {
                Ok(Ok(msg)) => {
                    info!("Data backup job completed: {}", msg);
                    log_task_execution(&pool, "system_backup", "success", &msg).await;
                }
                Ok(Err(e)) => {
                    error!("Data backup job failed: {}", e);
                    log_task_execution(&pool, "system_backup", "failed", &e).await;
                }
                Err(e) => {
                    error!("Data backup task join error: {}", e);
                    log_task_execution(
                        &pool,
                        "system_backup",
                        "failed",
                        &format!("任务执行异常: {e}"),
                    )
                    .await;
                }
            }
        })
    })?;
    scheduler.add(backup_job).await?;

    let pool_for_tokens = pool.clone();
    let token_cleanup_job = Job::new_async("0 0 * * * *", move |_, _| {
        let pool = pool_for_tokens.clone();
        Box::pin(async move {
            info!("Running expired token cleanup job...");
            let result = cleanup_expired_revoked_tokens(&pool.get_conn()).await;
            match result {
                Ok(count) => {
                    info!("Token cleanup completed: {} expired tokens removed", count);
                    log_task_execution(
                        &pool,
                        "system_token_cleanup",
                        "success",
                        &format!("清理了 {count} 个过期token"),
                    )
                    .await;
                }
                Err(e) => {
                    error!("Token cleanup failed: {}", e);
                    log_task_execution(
                        &pool,
                        "system_token_cleanup",
                        "failed",
                        &format!("Token清理失败: {e}"),
                    )
                    .await;
                }
            }
        })
    })?;
    scheduler.add(token_cleanup_job).await?;

    let pool_for_usage = pool.clone();
    let usage_cleanup_job = Job::new_async("0 0 2 * * *", move |_, _| {
        let pool = pool_for_usage.clone();
        Box::pin(async move {
            info!("Running old token usage cleanup job...");
            let result = cleanup_old_token_usage(&pool.get_conn(), 30).await;
            match result {
                Ok(count) => {
                    info!(
                        "Token usage cleanup completed: {} old records removed",
                        count
                    );
                    log_task_execution(
                        &pool,
                        "system_usage_cleanup",
                        "success",
                        &format!("清理了 {count} 条30天前的token_usage记录"),
                    )
                    .await;
                }
                Err(e) => {
                    error!("Token usage cleanup failed: {}", e);
                    log_task_execution(
                        &pool,
                        "system_usage_cleanup",
                        "failed",
                        &format!("Token使用记录清理失败: {e}"),
                    )
                    .await;
                }
            }
        })
    })?;
    scheduler.add(usage_cleanup_job).await?;

    let pool_for_tasks = pool.clone();
    let load_tasks_job = Job::new_async("0 */5 * * * *", move |_, _| {
        let pool = pool_for_tasks.clone();
        Box::pin(async move {
            if let Err(e) = sync_user_tasks_from_db(&pool).await {
                error!("Failed to sync user tasks: {}", e);
            }
        })
    })?;
    scheduler.add(load_tasks_job).await?;

    scheduler.start().await?;
    info!("Cron scheduler started successfully");

    Ok(SchedulerState { scheduler })
}

fn execute_database_backup(
    config: &crate::config::DatabaseConfig,
    _pool: &sqlx::PgPool,
) -> Result<String, String> {
    let backup_dir = "/var/lib/ipma/backups";
    if let Err(e) = std::fs::create_dir_all(backup_dir) {
        return Err(format!("创建备份目录失败: {e}"));
    }

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let backup_file = format!("{backup_dir}/ipma_backup_{timestamp}.sql");

    let output = Command::new("pg_dump")
        .arg("-h")
        .arg(&config.host)
        .arg("-p")
        .arg(config.port.to_string())
        .arg("-U")
        .arg(&config.username)
        .arg("-d")
        .arg(&config.database)
        .arg("--no-owner")
        .arg("--no-acl")
        .arg("--clean")
        .arg("--if-exists")
        .env("PGPASSWORD", &config.password)
        .output()
        .map_err(|e| {
            format!(
                "执行 pg_dump 失败: {e}。请确保系统已安装 postgresql-client。"
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("pg_dump 执行失败: {stderr}"));
    }

    let sql_content = output.stdout;
    if sql_content.is_empty() {
        return Err("导出的 SQL 文件为空".to_string());
    }

    std::fs::write(&backup_file, sql_content).map_err(|e| format!("写入备份文件失败: {e}"))?;

    cleanup_old_backups(backup_dir, 7)?;

    let file_size = std::fs::metadata(&backup_file)
        .map(|m| m.len())
        .unwrap_or(0);

    let file_size_mb = file_size as f64 / (1024.0 * 1024.0);

    Ok(format!(
        "备份成功: {backup_file} ({file_size_mb:.2} MB)"
    ))
}

fn cleanup_old_backups(backup_dir: &str, keep_days: u64) -> Result<(), String> {
    let entries = std::fs::read_dir(backup_dir).map_err(|e| format!("读取备份目录失败: {e}"))?;

    let now = std::time::SystemTime::now();
    let cutoff = std::time::Duration::from_secs(keep_days * 24 * 60 * 60);

    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(filename) = path.file_name().and_then(|f| f.to_str())
            && filename.starts_with("ipma_backup_")
            && filename.ends_with(".sql")
            && let Ok(metadata) = entry.metadata()
            && let Ok(modified) = metadata.modified()
            && let Ok(age) = now.duration_since(modified)
            && age > cutoff
        {
            if let Err(e) = std::fs::remove_file(&path) {
                error!("删除旧备份文件失败: {} - {}", path.display(), e);
            } else {
                info!("删除旧备份文件: {}", path.display());
            }
        }
    }

    Ok(())
}

async fn log_task_execution(pool: &DbPool, task_name: &str, status: &str, details: &str) {
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
    .execute(&pool.get_conn())
    .await
    {
        tracing::warn!("记录任务日志失败: {}", e);
    }
}

async fn sync_user_tasks_from_db(pool: &DbPool) -> Result<(), String> {
    let tasks: Vec<ScheduledTask> = sqlx::query_as(
        "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at 
         FROM scheduled_tasks WHERE enabled = true"
    )
    .fetch_all(&pool.get_conn())
    .await
    .map_err(|e| format!("查询用户任务失败: {e}"))?;

    for task in tasks {
        if let Err(e) = update_next_run_at(pool, &task).await {
            error!("更新任务 {} 的下次执行时间失败: {}", task.name, e);
        }
    }

    Ok(())
}

async fn update_next_run_at(pool: &DbPool, task: &ScheduledTask) -> Result<(), String> {
    let next_run = calculate_next_run(&task.cron_expression)?;

    sqlx::query("UPDATE scheduled_tasks SET next_run_at = $1, updated_at = $2 WHERE id = $3")
        .bind(next_run)
        .bind(Utc::now())
        .bind(task.id)
        .execute(&pool.get_conn())
        .await
        .map_err(|e| format!("更新下次执行时间失败: {e}"))?;

    Ok(())
}

pub fn calculate_next_run(cron_expression: &str) -> Result<chrono::DateTime<Utc>, String> {
    let parts: Vec<&str> = cron_expression.split_whitespace().collect();

    if parts.len() != 5 && parts.len() != 6 {
        return Err(format!("无效的cron表达式: {cron_expression}"));
    }

    let now = Utc::now();
    let mut next = now;

    for _ in 0..366 * 24 * 60 {
        next += chrono::Duration::minutes(1);

        let (sec, min, hour, day, month, weekday) = (
            next.second() as i32,
            next.minute() as i32,
            next.hour() as i32,
            next.day() as i32,
            next.month() as i32,
            next.weekday().num_days_from_monday() as i32,
        );

        let cron_parts = if parts.len() == 6 {
            parts.clone()
        } else {
            vec!["0", parts[0], parts[1], parts[2], parts[3], parts[4]]
        };

        if matches_cron_field(cron_parts[0], sec).is_ok()
            && matches_cron_field(cron_parts[1], min).is_ok()
            && matches_cron_field(cron_parts[2], hour).is_ok()
            && matches_cron_field(cron_parts[3], day).is_ok()
            && matches_cron_field(cron_parts[4], month).is_ok()
            && matches_cron_field(cron_parts[5], weekday).is_ok()
        {
            return Ok(next);
        }
    }

    Err("无法计算下次执行时间".to_string())
}

fn matches_cron_field(field: &str, value: i32) -> Result<bool, String> {
    if field == "*" {
        return Ok(true);
    }

    if field.contains(',') {
        for part in field.split(',') {
            if matches_cron_field(part, value)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }

    if field.contains('/') {
        let parts: Vec<&str> = field.split('/').collect();
        if parts.len() != 2 {
            return Err(format!("无效的cron字段: {field}"));
        }
        let step: i32 = parts[1]
            .parse()
            .map_err(|_| format!("无效的步长: {}", parts[1]))?;
        let base_field = parts[0];

        if base_field == "*" {
            return Ok(value % step == 0);
        } else {
            let base: i32 = base_field
                .parse()
                .map_err(|_| format!("无效的基础值: {base_field}"))?;
            return Ok((value - base) % step == 0 && value >= base);
        }
    }

    if field.contains('-') {
        let parts: Vec<&str> = field.split('-').collect();
        if parts.len() != 2 {
            return Err(format!("无效的cron字段: {field}"));
        }
        let start: i32 = parts[0]
            .parse()
            .map_err(|_| format!("无效的范围起始: {}", parts[0]))?;
        let end: i32 = parts[1]
            .parse()
            .map_err(|_| format!("无效的范围结束: {}", parts[1]))?;
        return Ok(value >= start && value <= end);
    }

    let field_value: i32 = field
        .parse()
        .map_err(|_| format!("无效的cron字段值: {field}"))?;
    Ok(value == field_value)
}

pub async fn execute_task_by_type(
    pool: &sqlx::PgPool,
    task_type: &str,
    config: &serde_json::Value,
    db_config: &crate::config::DatabaseConfig,
) -> Result<String, String> {
    match task_type {
        "mac_sync" => execute_mac_sync(pool, config).await,
        "token_cleanup" => execute_token_cleanup(pool).await,
        "backup" => {
            let db_config = db_config.clone();
            tokio::task::spawn_blocking(move || execute_backup_task(&db_config))
                .await
                .unwrap_or_else(|e| Err(format!("备份任务执行异常: {e}")))
        }
        "log_cleanup" => execute_log_cleanup(pool, config).await,
        _ => Err(format!("未知的任务类型: {task_type}")),
    }
}

async fn execute_mac_sync(
    pool: &sqlx::PgPool,
    config: &serde_json::Value,
) -> Result<String, String> {
    let switch_id = config
        .get("switch_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());
    let network_id = config
        .get("network_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    match (switch_id, network_id) {
        (Some(switch_id), Some(network_id)) => {
            crate::resource::ip::pull_ip_managers_internal(pool, switch_id, network_id)
                .await
                .map(|()| "MAC同步成功".to_string())
                .map_err(|e| format!("MAC同步失败: {e}"))
        }
        _ => Err("MAC同步任务需要配置switch_id和network_id".to_string()),
    }
}

async fn execute_token_cleanup(pool: &sqlx::PgPool) -> Result<String, String> {
    let count = crate::utils::cleanup_expired_revoked_tokens(pool)
        .await
        .map_err(|e| format!("Token清理失败: {e}"))?;
    Ok(format!("清理了 {count} 个过期token"))
}

fn execute_backup_task(db_config: &crate::config::DatabaseConfig) -> Result<String, String> {
    let backup_dir = "/var/lib/ipma/backups";
    if let Err(e) = std::fs::create_dir_all(backup_dir) {
        return Err(format!("创建备份目录失败: {e}"));
    }

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let backup_file = format!("{backup_dir}/ipma_manual_backup_{timestamp}.sql");

    let output = Command::new("pg_dump")
        .arg("-h")
        .arg(&db_config.host)
        .arg("-p")
        .arg(db_config.port.to_string())
        .arg("-U")
        .arg(&db_config.username)
        .arg("-d")
        .arg(&db_config.database)
        .arg("--no-owner")
        .arg("--no-acl")
        .arg("--clean")
        .arg("--if-exists")
        .env("PGPASSWORD", &db_config.password)
        .output()
        .map_err(|e| format!("执行 pg_dump 失败: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("pg_dump 执行失败: {stderr}"));
    }

    let sql_content = output.stdout;
    if sql_content.is_empty() {
        return Err("导出的 SQL 文件为空".to_string());
    }

    std::fs::write(&backup_file, sql_content).map_err(|e| format!("写入备份文件失败: {e}"))?;

    let file_size = std::fs::metadata(&backup_file)
        .map(|m| m.len())
        .unwrap_or(0);

    let file_size_mb = file_size as f64 / (1024.0 * 1024.0);

    Ok(format!(
        "备份成功: {backup_file} ({file_size_mb:.2} MB)"
    ))
}

async fn execute_log_cleanup(
    pool: &sqlx::PgPool,
    config: &serde_json::Value,
) -> Result<String, String> {
    let days = config.get("days").and_then(serde_json::Value::as_i64).unwrap_or(30) as i32;

    if days < 0 {
        return Err("保留天数不能为负数".to_string());
    }

    let mut deleted = 0u64;

    if days == 0 {
        if let Ok(r) = sqlx::query("DELETE FROM operation_logs")
            .execute(pool)
            .await
        {
            deleted += r.rows_affected();
        }
        if let Ok(r) = sqlx::query("DELETE FROM login_logs").execute(pool).await {
            deleted += r.rows_affected();
        }
        if let Ok(r) = sqlx::query("DELETE FROM notifications").execute(pool).await {
            deleted += r.rows_affected();
        }
    } else {
        if let Ok(r) = sqlx::query(
            "DELETE FROM operation_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
        )
        .bind(days)
        .execute(pool)
        .await
        {
            deleted += r.rows_affected();
        }
        if let Ok(r) =
            sqlx::query("DELETE FROM login_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1")
                .bind(days)
                .execute(pool)
                .await
        {
            deleted += r.rows_affected();
        }
        if let Ok(r) = sqlx::query(
            "DELETE FROM notifications WHERE created_at < NOW() - INTERVAL '1 day' * $1",
        )
        .bind(days)
        .execute(pool)
        .await
        {
            deleted += r.rows_affected();
        }
    }

    Ok(format!("清理了 {deleted} 条日志记录"))
}
