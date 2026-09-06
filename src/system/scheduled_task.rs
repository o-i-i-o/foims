//! 定时任务管理接口。

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use foims_scheduler::{TaskContext, calculate_next_run};
use serde_json;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use foims_auth::extractor::AdminUser;
use foims_common::AppError;
use foims_common::AppJson;
use foims_common::pagination::{Pagination, paged_response};
use foims_common::{log_warn, msg};
use foims_models::{ApiResponse, ScheduledTask, ScheduledTaskCreate, ScheduledTaskUpdate, TaskLog};

/// 允许通过 API 创建/更新的任务类型白名单（与 task_executors 中注册的类型保持一致）
const ALLOWED_TASK_TYPES: &[&str] = &[
    "backup",
    "token_cleanup",
    "log_cleanup",
    "mac_sync",
    "ip_status_sync",
];

/// 校验任务类型是否在白名单内
fn validate_task_type(task_type: &str) -> Result<(), AppError> {
    if ALLOWED_TASK_TYPES.contains(&task_type) {
        Ok(())
    } else {
        Err(AppError::Validation(
            msg("server.task.type_unsupported")
                .with("task_type", task_type)
                .with("allowed", ALLOWED_TASK_TYPES.join(", ")),
        ))
    }
}

/// 需要校验 days 配置的任务（与 task_executors.rs 执行期校验口径一致）
const DAYS_CONFIG_TASK_TYPES: &[&str] = &["log_cleanup", "ip_status_sync"];

/// 天数类任务 days 合法区间（1..=3650）：越界值在执行器处会被拒绝，
/// 创建/更新时同步校验，避免落库后任务每次执行都失败
const DAYS_CONFIG_RANGE: std::ops::RangeInclusive<i64> = 1..=3650;

/// 校验天数类任务（log_cleanup / ip_status_sync）的 days 配置：
/// days < 1 会清空全部审计日志（或立即判停全部地址），超大值会被执行器按截断拒绝
fn validate_cleanup_days(task_type: &str, config: &serde_json::Value) -> Result<(), AppError> {
    if !DAYS_CONFIG_TASK_TYPES.contains(&task_type) {
        return Ok(());
    }
    if let Some(days) = config.get("days").and_then(serde_json::Value::as_i64)
        && !DAYS_CONFIG_RANGE.contains(&days)
    {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "days (1-3650)"),
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

    Ok(foims_common::ok_json(tasks, "server.task.list_retrieved"))
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
        Some(t) => Ok(foims_common::ok_json(t, "server.task.retrieved")),
        None => Err(AppError::NotFound(msg("server.task.not_found"))),
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
    validate_cleanup_days(&req.task_type, &config)?;
    let enabled = req.enabled.unwrap_or(true);

    // 校验 cron 表达式，非法时直接拒绝创建（与 update 口径一致），
    // 不再告警后落库 NULL 导致任务静默永不执行
    let cron_expr = req.cron_expression.clone();
    let next_run_at =
        match tokio::task::spawn_blocking(move || calculate_next_run(&cron_expr)).await {
            Ok(Ok(time)) => Some(time),
            Ok(Err(e)) => {
                return Err(AppError::Validation(
                    msg("server.task.cron_invalid").with("error", e),
                ));
            }
            Err(e) => {
                return Err(AppError::Internal(
                    msg("server.task.cron_validate_task_failed").with("error", e),
                ));
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
        Json(ApiResponse::success(task, "server.task.created")),
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
        validate_cleanup_days(&effective_type, config)?;
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
                return Err(AppError::Validation(
                    msg("server.task.cron_invalid").with("error", e),
                ));
            }
            Err(e) => {
                return Err(AppError::Internal(
                    msg("server.task.cron_validate_task_failed").with("error", e),
                ));
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
        return Err(AppError::NotFound(msg("server.task.not_found")));
    }

    let task: ScheduledTask = sqlx::query_as(
        "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at FROM scheduled_tasks WHERE id = $1"
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(foims_common::ok_json(task, "server.task.updated"))
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
        Ok(foims_common::ok_json((), "server.task.deleted"))
    } else {
        Err(AppError::NotFound(msg("server.task.not_found")))
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

        Ok(foims_common::ok_json(task, "server.task.toggled"))
    } else {
        Err(AppError::NotFound(msg("server.task.not_found")))
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
        return Err(AppError::Conflict(msg("server.task.running_conflict")));
    }

    let conn = pool.get_conn();

    let task: Option<ScheduledTask> = sqlx::query_as(
        "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at FROM scheduled_tasks WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&conn)
    .await?;

    let Some(task) = task else {
        return Err(AppError::NotFound(msg("server.task.not_found")));
    };

    let start_time = Utc::now();
    let task_name = task.name.clone();
    let task_type = task.task_type.clone();

    let db_config = pool.db_config.clone();

    let ctx = TaskContext {
        pool: conn.clone(),
        config: task.config.clone(),
        db_config,
    };

    let result = state.task_registry.execute(&task.task_type, &ctx).await;

    let end_time = Utc::now();
    let duration = i32::try_from((end_time - start_time).num_milliseconds()).unwrap_or(i32::MAX);

    let (status, details) = match &result {
        Ok(message) => (
            "success",
            serde_json::json!({ "message": message, "task_type": task_type }),
        ),
        Err(e) => (
            "failed",
            serde_json::json!({
                "error": foims_scheduler::error_message(e).log_string(),
                "task_type": task_type
            }),
        ),
    };

    let cron_expr = task.cron_expression.clone();
    let next_run_at =
        match tokio::task::spawn_blocking(move || calculate_next_run(&cron_expr)).await {
            Ok(Ok(time)) => Some(time),
            Ok(Err(e)) => {
                log_warn!("log.task.next_run_calc_failed", error = e);
                None
            }
            Err(e) => {
                log_warn!("log.task.next_run_calc_task_failed", error = e);
                None
            }
        };

    // cron 解析失败时按 1 小时退避推进 next_run_at：保留过期的旧值会让
    // 任务每分钟重新触发并刷屏（与调度路径同口径）
    let effective_next_run = next_run_at.unwrap_or(start_time + chrono::Duration::hours(1));

    // 日志写入与状态更新（last_run_at/next_run_at/last_result）是与本次
    // 执行同源的业务双写，与 advisory lock 同事务提交：两者原子生效，
    // 提交同时释放锁；任一失败整体回滚（last_run_at 不前进，留痕于日志）
    let mut write_err: Option<sqlx::Error> = None;
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
    .execute(&mut *lock_tx)
    .await
    {
        write_err = Some(e);
    }

    if write_err.is_none()
        && let Err(e) = sqlx::query(
            "UPDATE scheduled_tasks SET last_run_at = $1, next_run_at = $2, last_result = $3, updated_at = $1 WHERE id = $4",
        )
        .bind(start_time)
        .bind(effective_next_run)
        .bind(result.as_ref().ok())
        .bind(id)
        .execute(&mut *lock_tx)
        .await
    {
        write_err = Some(e);
    }

    match write_err {
        None => {
            if let Err(e) = lock_tx.commit().await {
                log_warn!("log.task.lock_release_failed", error = e);
            }
        }
        Some(e) => {
            log_warn!("log.task.run_result_write_failed", error = e);
            if let Err(re) = lock_tx.rollback().await {
                log_warn!("log.task.run_result_rollback_failed", error = re);
            }
        }
    }

    Ok(foims_common::ok_json(
        serde_json::json!({
            "result": result.map_err(|e| {
                foims_scheduler::error_message(&e).key().to_string()
            })
        }),
        "server.task.executed",
    ))
}

pub async fn get_task_logs(
    State(state): State<Arc<AppState>>,
    _viewer: foims_auth::extractor::AdminOrAuditorUser,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let task_name = query.get("task_name").cloned();
    // 与其它列表端点同口径：page/page_size 钳制（1..=1000），
    // 响应为固定五键 items/total/page/page_size/total_pages
    let pagination = Pagination::from_query(&query);
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let conn = state.pool()?.get_conn();

    let (total, logs) = if let Some(name) = &task_name {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_logs WHERE task_name = $1")
            .bind(name)
            .fetch_one(&conn)
            .await?;
        let logs = sqlx::query_as::<_, TaskLog>(
            "SELECT id, task_name, status, details, start_time, end_time, duration FROM task_logs WHERE task_name = $1 ORDER BY start_time DESC LIMIT $2 OFFSET $3",
        )
        .bind(name)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&conn)
        .await?;
        (total, logs)
    } else {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_logs")
            .fetch_one(&conn)
            .await?;
        let logs = sqlx::query_as::<_, TaskLog>(
            "SELECT id, task_name, status, details, start_time, end_time, duration FROM task_logs ORDER BY start_time DESC LIMIT $1 OFFSET $2",
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(&conn)
        .await?;
        (total, logs)
    };

    Ok(foims_common::ok_json(
        paged_response(logs, total, &pagination),
        "server.task.logs_retrieved",
    ))
}
