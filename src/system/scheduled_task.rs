use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use ipma_data_manager::DatabaseConfig;
use ipma_scheduler::{TaskContext, TaskLog, calculate_next_run};
use serde_json;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::auth::extractor::AdminUser;
use crate::error::AppError;
use crate::models::{ApiResponse, ScheduledTask, ScheduledTaskCreate, ScheduledTaskUpdate};
use crate::routes::static_files::AppJson;

/// 允许通过 API 创建/更新的任务类型白名单（与 task_executors 中注册的类型保持一致）
const ALLOWED_TASK_TYPES: &[&str] = &[
    "backup",
    "token_cleanup",
    "token_usage_cleanup",
    "log_cleanup",
    "mac_sync",
];

/// 校验任务类型是否在白名单内
fn validate_task_type(task_type: &str) -> Result<(), AppError> {
    if ALLOWED_TASK_TYPES.contains(&task_type) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "不支持的任务类型: {task_type}（允许: {}）",
            ALLOWED_TASK_TYPES.join(", ")
        )))
    }
}

/// 校验 log_cleanup 任务的 days 配置，禁止 days < 1 导致清空全部审计日志
fn validate_log_cleanup_days(config: &serde_json::Value) -> Result<(), AppError> {
    if let Some(days) = config.get("days").and_then(serde_json::Value::as_i64)
        && days < 1
    {
        return Err(AppError::Validation(
            "log_cleanup 任务的 days 必须 >= 1，禁止清空全部日志".to_string(),
        ));
    }
    Ok(())
}

pub async fn get_scheduled_tasks(
    State(state): State<Arc<AppState>>,
    _admin: AdminUser,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let sort_by = query.get("sort_by").cloned().unwrap_or_default();
    let sort_order = query.get("sort_order").cloned().unwrap_or_default();

    // ORDER BY 白名单，未匹配时回落默认序，避免注入
    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY name DESC",
        ("name", _) => "ORDER BY name ASC",
        ("task_type", "desc") => "ORDER BY task_type DESC, name ASC",
        ("task_type", _) => "ORDER BY task_type ASC, name ASC",
        ("enabled", "desc") => "ORDER BY enabled DESC, name ASC",
        ("enabled", _) => "ORDER BY enabled ASC, name ASC",
        ("last_run_at", "desc") => "ORDER BY last_run_at DESC NULLS LAST, name ASC",
        ("last_run_at", _) => "ORDER BY last_run_at ASC NULLS LAST, name ASC",
        ("last_result", "desc") => "ORDER BY last_result DESC NULLS LAST, name ASC",
        ("last_result", _) => "ORDER BY last_result ASC NULLS LAST, name ASC",
        ("created_at", "asc") => "ORDER BY created_at ASC",
        _ => "ORDER BY created_at DESC",
    };

    let tasks: Vec<ScheduledTask> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at FROM scheduled_tasks {order_clause}"
    )))
    .fetch_all(&state.pool()?.get_conn())
    .await
    ?;

    Ok(crate::error::ok_json(tasks, "获取定时任务列表成功"))
}

pub async fn get_scheduled_task(
    State(state): State<Arc<AppState>>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let task: Option<ScheduledTask> = sqlx::query_as(
        "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at FROM scheduled_tasks WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?;

    match task {
        Some(t) => Ok(crate::error::ok_json(t, "获取定时任务成功")),
        None => Err(AppError::NotFound("定时任务不存在".to_string())),
    }
}

pub async fn create_scheduled_task(
    State(state): State<Arc<AppState>>,
    _admin: AdminUser,
    AppJson(req): AppJson<ScheduledTaskCreate>,
) -> Result<Response, AppError> {
    req.validate()?;
    validate_task_type(&req.task_type)?;
    let config = req.config.clone().unwrap_or_else(|| serde_json::json!({}));
    if req.task_type == "log_cleanup" {
        validate_log_cleanup_days(&config)?;
    }
    let enabled = req.enabled.unwrap_or(true);

    let cron_expr = req.cron_expression.clone();
    let next_run_at =
        match tokio::task::spawn_blocking(move || calculate_next_run(&cron_expr)).await {
            Ok(Ok(time)) => Some(time),
            Ok(Err(e)) => {
                tracing::warn!("计算下次运行时间失败: {}", e);
                None
            }
            Err(e) => {
                tracing::warn!("计算下次运行时间任务失败: {}", e);
                None
            }
        };

    let task: ScheduledTask = sqlx::query_as(
        r"INSERT INTO scheduled_tasks (name, task_type, cron_expression, enabled, config, next_run_at)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at"
    )
    .bind(&req.name)
    .bind(&req.task_type)
    .bind(&req.cron_expression)
    .bind(enabled)
    .bind(&config)
    .bind(next_run_at)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(task, "创建定时任务成功")),
    )
        .into_response())
}

pub async fn update_scheduled_task(
    State(state): State<Arc<AppState>>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
    AppJson(req): AppJson<ScheduledTaskUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;
    if let Some(ref task_type) = req.task_type {
        validate_task_type(task_type)?;
    }
    if let Some(ref config) = req.config {
        let task_type = req.task_type.as_deref().unwrap_or("");
        // 类型未在本请求中变更时，需要读取已有类型来判断
        let effective_type = if task_type.is_empty() {
            sqlx::query_scalar::<_, String>("SELECT task_type FROM scheduled_tasks WHERE id = $1")
                .bind(id)
                .fetch_optional(&state.pool()?.get_conn())
                .await?
                .unwrap_or_default()
        } else {
            task_type.to_string()
        };
        if effective_type == "log_cleanup" {
            validate_log_cleanup_days(config)?;
        }
    }

    let now = Utc::now();
    let conn = state.pool()?.get_conn();
    let mut tx = conn.begin().await?;

    // 校验 cron 表达式，失败时拒绝写入
    let next_run_at = if let Some(ref cron_expr) = req.cron_expression {
        let cron_expr_clone = cron_expr.clone();
        match tokio::task::spawn_blocking(move || calculate_next_run(&cron_expr_clone)).await {
            Ok(Ok(next_run)) => Some(next_run),
            Ok(Err(e)) => {
                return Err(AppError::Validation(format!("cron表达式无效: {e}")));
            }
            Err(e) => {
                return Err(AppError::Internal(format!("cron校验任务失败: {e}")));
            }
        }
    } else {
        None
    };

    let result = sqlx::query(
        r"UPDATE scheduled_tasks SET
           name = COALESCE($1, name),
           task_type = COALESCE($2, task_type),
           cron_expression = COALESCE($3, cron_expression),
           enabled = COALESCE($4, enabled),
           config = COALESCE($5, config),
           next_run_at = COALESCE($6, next_run_at),
           updated_at = $7
           WHERE id = $8",
    )
    .bind(&req.name)
    .bind(&req.task_type)
    .bind(&req.cron_expression)
    .bind(req.enabled)
    .bind(&req.config)
    .bind(next_run_at)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("定时任务不存在".to_string()));
    }

    let task: ScheduledTask = sqlx::query_as(
        "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at FROM scheduled_tasks WHERE id = $1"
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(crate::error::ok_json(task, "更新定时任务成功"))
}

pub async fn delete_scheduled_task(
    State(state): State<Arc<AppState>>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let result = sqlx::query("DELETE FROM scheduled_tasks WHERE id = $1")
        .bind(id)
        .execute(&state.pool()?.get_conn())
        .await?;

    if result.rows_affected() > 0 {
        Ok(crate::error::ok_json((), "删除定时任务成功"))
    } else {
        Err(AppError::NotFound("定时任务不存在".to_string()))
    }
}

pub async fn toggle_scheduled_task(
    State(state): State<Arc<AppState>>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let result = sqlx::query(
        "UPDATE scheduled_tasks SET enabled = NOT enabled, updated_at = $1 WHERE id = $2",
    )
    .bind(Utc::now())
    .bind(id)
    .execute(&conn)
    .await?;

    if result.rows_affected() > 0 {
        let task: ScheduledTask = sqlx::query_as(
            "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at FROM scheduled_tasks WHERE id = $1"
        )
        .bind(id)
        .fetch_one(&conn)
        .await?;

        Ok(crate::error::ok_json(task, "切换定时任务状态成功"))
    } else {
        Err(AppError::NotFound("定时任务不存在".to_string()))
    }
}

pub async fn run_scheduled_task_now(
    State(state): State<Arc<AppState>>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let pool = state.pool()?;

    // 使用事务级 advisory lock 防止同一任务并发执行
    // 事务提交或回滚时自动释放锁,避免 panic/取消导致锁泄漏
    let mut lock_tx = pool.begin().await?;
    let lock_key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        id.hash(&mut hasher);
        hasher.finish() as i64
    };
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
        .bind(lock_key)
        .fetch_one(&mut *lock_tx)
        .await?;

    if !locked {
        return Err(AppError::Conflict("任务正在执行中，请稍后再试".to_string()));
    }

    let conn = pool.get_conn();

    let task: Option<ScheduledTask> = sqlx::query_as(
        "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at FROM scheduled_tasks WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&conn)
    .await?;

    let Some(task) = task else {
        return Err(AppError::NotFound("定时任务不存在".to_string()));
    };

    let start_time = Utc::now();
    let task_name = task.name.clone();
    let task_type = task.task_type.clone();

    let db_config = DatabaseConfig {
        host: pool.db_config.host.clone(),
        port: pool.db_config.port,
        database: pool.db_config.database.clone(),
        username: pool.db_config.username.clone(),
        password: pool.db_config.password.clone(),
    };

    let ctx = TaskContext {
        pool: conn.clone(),
        config: task.config.clone(),
        db_config,
    };

    let result = state.task_registry.execute(&task.task_type, &ctx).await;

    let end_time = Utc::now();
    let duration = i32::try_from((end_time - start_time).num_milliseconds()).unwrap_or(i32::MAX);

    let (status, details) = match &result {
        Ok(msg) => (
            "success",
            serde_json::json!({ "message": msg, "task_type": task_type }),
        ),
        Err(e) => (
            "failed",
            serde_json::json!({ "error": e.to_string(), "task_type": task_type }),
        ),
    };

    if let Err(e) = sqlx::query(
        r"INSERT INTO task_logs (id, task_name, status, details, start_time, end_time, duration)
           VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(&task_name)
    .bind(status)
    .bind(sqlx::types::Json(details))
    .bind(start_time)
    .bind(end_time)
    .bind(duration)
    .execute(&conn)
    .await
    {
        tracing::warn!("记录任务日志失败: {}", e);
    }

    let cron_expr = task.cron_expression.clone();
    let next_run_at =
        match tokio::task::spawn_blocking(move || calculate_next_run(&cron_expr)).await {
            Ok(Ok(time)) => Some(time),
            Ok(Err(e)) => {
                tracing::warn!("计算下次运行时间失败: {}", e);
                None
            }
            Err(e) => {
                tracing::warn!("计算下次运行时间任务失败: {}", e);
                None
            }
        };

    let update_query = if let Some(next_run) = next_run_at {
        sqlx::query(
            "UPDATE scheduled_tasks SET last_run_at = $1, next_run_at = $2, last_result = $3, updated_at = $1 WHERE id = $4"
        )
        .bind(start_time)
        .bind(next_run)
        .bind(result.as_ref().ok())
        .bind(id)
    } else {
        sqlx::query(
            "UPDATE scheduled_tasks SET last_run_at = $1, last_result = $2, updated_at = $1 WHERE id = $3"
        )
        .bind(start_time)
        .bind(result.as_ref().ok())
        .bind(id)
    };

    if let Err(e) = update_query.execute(&conn).await {
        tracing::warn!("更新定时任务执行结果失败: {}", e);
    }

    // 提交事务以释放 advisory lock
    if let Err(e) = lock_tx.commit().await {
        tracing::warn!("释放任务锁失败: {}", e);
    }

    Ok(crate::error::ok_json(
        serde_json::json!({"result": result.map_err(|e| e.to_string())}),
        "执行定时任务成功",
    ))
}

pub async fn get_task_logs(
    State(state): State<Arc<AppState>>,
    _admin: AdminUser,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let task_name = query.get("task_name").cloned();
    let limit: i64 = query
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let conn = state.pool()?.get_conn();

    let logs = if let Some(name) = task_name {
        sqlx::query_as::<_, TaskLog>(
            "SELECT id, task_name, status, details, start_time, end_time, duration FROM task_logs WHERE task_name = $1 ORDER BY start_time DESC LIMIT $2"
        )
        .bind(&name)
        .bind(limit)
        .fetch_all(&conn)
        .await
        ?
    } else {
        sqlx::query_as::<_, TaskLog>(
            "SELECT id, task_name, status, details, start_time, end_time, duration FROM task_logs ORDER BY start_time DESC LIMIT $1"
        )
        .bind(limit)
        .fetch_all(&conn)
        .await
        ?
    };

    Ok(crate::error::ok_json(logs, "获取任务日志成功"))
}
