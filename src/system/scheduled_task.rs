use actix_web::{HttpResponse, Result, web};
use chrono::Utc;
use serde_json;
use uuid::Uuid;
use validator::Validate;

use crate::db::DbPool;
use crate::models::{ApiResponse, ScheduledTask, ScheduledTaskCreate, ScheduledTaskUpdate};
use crate::system::cron::{calculate_next_run, execute_task_by_type};

pub async fn get_scheduled_tasks(pool: web::Data<DbPool>) -> Result<HttpResponse> {
    let tasks: Vec<ScheduledTask> = sqlx::query_as(
        "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at FROM scheduled_tasks ORDER BY created_at DESC"
    )
    .fetch_all(pool.get_conn())
    .await
    .unwrap_or_default();

    Ok(HttpResponse::Ok().json(ApiResponse::success(tasks, "获取定时任务列表成功")))
}

pub async fn get_scheduled_task(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = path.into_inner();

    let task: Option<ScheduledTask> = sqlx::query_as(
        "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at FROM scheduled_tasks WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool.get_conn())
    .await
    .ok()
    .flatten();

    match task {
        Some(t) => Ok(HttpResponse::Ok().json(ApiResponse::success(t, "获取定时任务成功"))),
        None => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("定时任务不存在"))),
    }
}

pub async fn create_scheduled_task(
    pool: web::Data<DbPool>,
    req: web::Json<ScheduledTaskCreate>,
) -> Result<HttpResponse> {
    if let Err(e) = req.validate() {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error(format!("参数验证失败: {:?}", e))));
    }

    let config = req.config.clone().unwrap_or(serde_json::json!({}));
    let enabled = req.enabled.unwrap_or(true);

    let next_run_at = calculate_next_run(&req.cron_expression).ok();

    let task: Result<ScheduledTask, sqlx::Error> = sqlx::query_as(
        r#"INSERT INTO scheduled_tasks (name, task_type, cron_expression, enabled, config, next_run_at)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at"#
    )
    .bind(&req.name)
    .bind(&req.task_type)
    .bind(&req.cron_expression)
    .bind(enabled)
    .bind(&config)
    .bind(next_run_at)
    .fetch_one(pool.get_conn())
    .await;

    match task {
        Ok(t) => Ok(HttpResponse::Created().json(ApiResponse::success(t, "创建定时任务成功"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建定时任务失败: {}", e)))),
    }
}

pub async fn update_scheduled_task(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    req: web::Json<ScheduledTaskUpdate>,
) -> Result<HttpResponse> {
    if let Err(e) = req.validate() {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error(format!("参数验证失败: {:?}", e))));
    }

    let id = path.into_inner();
    let now = Utc::now();

    if let Some(ref cron_expr) = req.cron_expression
        && let Ok(next_run) = calculate_next_run(cron_expr)
        && let Err(e) = sqlx::query("UPDATE scheduled_tasks SET next_run_at = $1 WHERE id = $2")
            .bind(next_run)
            .bind(id)
            .execute(pool.get_conn())
            .await
    {
        tracing::warn!("更新下次运行时间失败: {}", e);
    }

    let result = sqlx::query(
        r#"UPDATE scheduled_tasks SET
           name = COALESCE($1, name),
           task_type = COALESCE($2, task_type),
           cron_expression = COALESCE($3, cron_expression),
           enabled = COALESCE($4, enabled),
           config = COALESCE($5, config),
           updated_at = $6
           WHERE id = $7"#,
    )
    .bind(&req.name)
    .bind(&req.task_type)
    .bind(&req.cron_expression)
    .bind(req.enabled)
    .bind(&req.config)
    .bind(now)
    .bind(id)
    .execute(pool.get_conn())
    .await;

    match result {
        Ok(res) if res.rows_affected() > 0 => {
            let task: Result<ScheduledTask, sqlx::Error> = sqlx::query_as(
                "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at FROM scheduled_tasks WHERE id = $1"
            )
            .bind(id)
            .fetch_one(pool.get_conn())
            .await;

            match task {
                Ok(t) => Ok(HttpResponse::Ok().json(ApiResponse::success(t, "更新定时任务成功"))),
                Err(e) => Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "查询更新后的任务失败: {}",
                        e
                    ))),
                ),
            }
        }
        Ok(_) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("定时任务不存在"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("更新定时任务失败: {}", e)))),
    }
}

pub async fn delete_scheduled_task(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = path.into_inner();

    let result = sqlx::query("DELETE FROM scheduled_tasks WHERE id = $1")
        .bind(id)
        .execute(pool.get_conn())
        .await;

    match result {
        Ok(res) if res.rows_affected() > 0 => {
            Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "删除定时任务成功")))
        }
        Ok(_) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("定时任务不存在"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("删除定时任务失败: {}", e)))),
    }
}

pub async fn toggle_scheduled_task(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = path.into_inner();

    let result = sqlx::query(
        "UPDATE scheduled_tasks SET enabled = NOT enabled, updated_at = $1 WHERE id = $2",
    )
    .bind(Utc::now())
    .bind(id)
    .execute(pool.get_conn())
    .await;

    match result {
        Ok(res) if res.rows_affected() > 0 => {
            let task: Result<ScheduledTask, sqlx::Error> = sqlx::query_as(
                "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at FROM scheduled_tasks WHERE id = $1"
            )
            .bind(id)
            .fetch_one(pool.get_conn())
            .await;

            match task {
                Ok(t) => {
                    Ok(HttpResponse::Ok().json(ApiResponse::success(t, "切换定时任务状态成功")))
                }
                Err(e) => Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "查询切换后的任务失败: {}",
                        e
                    ))),
                ),
            }
        }
        Ok(_) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("定时任务不存在"))),
        Err(e) => Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "切换定时任务状态失败: {}",
                e
            ))),
        ),
    }
}

pub async fn run_scheduled_task_now(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = path.into_inner();

    let task: Option<ScheduledTask> = sqlx::query_as(
        "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at FROM scheduled_tasks WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool.get_conn())
    .await
    .ok()
    .flatten();

    let task = match task {
        Some(t) => t,
        None => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("定时任务不存在")));
        }
    };

    let start_time = Utc::now();
    let task_name = task.name.clone();
    let task_type = task.task_type.clone();

    let db_config = pool.db_config.clone();
    let result =
        execute_task_by_type(pool.get_conn(), &task.task_type, &task.config, &db_config).await;

    let end_time = Utc::now();
    let duration = (end_time - start_time).num_milliseconds() as i32;

    let (status, details) = match &result {
        Ok(msg) => (
            "success",
            serde_json::json!({ "message": msg, "task_type": task_type }),
        ),
        Err(e) => (
            "failed",
            serde_json::json!({ "error": e, "task_type": task_type }),
        ),
    };

    if let Err(e) = sqlx::query(
        r#"INSERT INTO task_logs (id, task_name, status, details, start_time, end_time, duration)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(Uuid::new_v4())
    .bind(&task_name)
    .bind(status)
    .bind(sqlx::types::Json(details))
    .bind(start_time)
    .bind(end_time)
    .bind(duration)
    .execute(pool.get_conn())
    .await
    {
        tracing::warn!("记录任务日志失败: {}", e);
    }

    let next_run_at = calculate_next_run(&task.cron_expression).ok();

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

    update_query.execute(pool.get_conn()).await.ok();

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({"result": result}),
        "执行定时任务成功",
    )))
}

pub async fn get_task_logs(
    pool: web::Data<DbPool>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    let task_name = query.get("task_name").cloned();
    let limit: i64 = query
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    let logs = if let Some(name) = task_name {
        sqlx::query_as::<_, crate::models::TaskLog>(
            "SELECT id, task_name, status, details, start_time, end_time, duration FROM task_logs WHERE task_name = $1 ORDER BY start_time DESC LIMIT $2"
        )
        .bind(&name)
        .bind(limit)
        .fetch_all(pool.get_conn())
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as::<_, crate::models::TaskLog>(
            "SELECT id, task_name, status, details, start_time, end_time, duration FROM task_logs ORDER BY start_time DESC LIMIT $1"
        )
        .bind(limit)
        .fetch_all(pool.get_conn())
        .await
        .unwrap_or_default()
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(logs, "获取任务日志成功")))
}
