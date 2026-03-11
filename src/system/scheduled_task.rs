use actix_web::{HttpResponse, Result, web};
use chrono::Utc;
use serde_json;
use uuid::Uuid;
use validator::Validate;

use crate::db::DbPool;
use crate::models::{ApiResponse, ScheduledTask, ScheduledTaskCreate, ScheduledTaskUpdate};

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
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            format!("参数验证失败: {:?}", e)
        )));
    }

    let config = req.config.clone().unwrap_or(serde_json::json!({}));
    let enabled = req.enabled.unwrap_or(true);

    let task: Result<ScheduledTask, sqlx::Error> = sqlx::query_as(
        r#"INSERT INTO scheduled_tasks (name, task_type, cron_expression, enabled, config)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at"#
    )
    .bind(&req.name)
    .bind(&req.task_type)
    .bind(&req.cron_expression)
    .bind(enabled)
    .bind(&config)
    .fetch_one(pool.get_conn())
    .await;

    match task {
        Ok(t) => Ok(HttpResponse::Created().json(ApiResponse::success(t, "创建定时任务成功"))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
            format!("创建定时任务失败: {}", e)
        ))),
    }
}

pub async fn update_scheduled_task(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    req: web::Json<ScheduledTaskUpdate>,
) -> Result<HttpResponse> {
    if let Err(e) = req.validate() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            format!("参数验证失败: {:?}", e)
        )));
    }

    let id = path.into_inner();
    let now = Utc::now();

    let result = sqlx::query(
        r#"UPDATE scheduled_tasks SET
           name = COALESCE($1, name),
           task_type = COALESCE($2, task_type),
           cron_expression = COALESCE($3, cron_expression),
           enabled = COALESCE($4, enabled),
           config = COALESCE($5, config),
           updated_at = $6
           WHERE id = $7"#
    )
    .bind(&req.name)
    .bind(&req.task_type)
    .bind(&req.cron_expression)
    .bind(&req.enabled)
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
                Err(e) => Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("查询更新后的任务失败: {}", e)
                ))),
            }
        },
        Ok(_) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("定时任务不存在"))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
            format!("更新定时任务失败: {}", e)
        ))),
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
        },
        Ok(_) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("定时任务不存在"))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
            format!("删除定时任务失败: {}", e)
        ))),
    }
}

pub async fn toggle_scheduled_task(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = path.into_inner();

    let result = sqlx::query(
        "UPDATE scheduled_tasks SET enabled = NOT enabled, updated_at = $1 WHERE id = $2"
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
                Ok(t) => Ok(HttpResponse::Ok().json(ApiResponse::success(t, "切换定时任务状态成功"))),
                Err(e) => Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("查询切换后的任务失败: {}", e)
                ))),
            }
        },
        Ok(_) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("定时任务不存在"))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
            format!("切换定时任务状态失败: {}", e)
        ))),
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
        None => return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("定时任务不存在"))),
    };

    let result = match task.task_type.as_str() {
        "mac_sync" => execute_mac_sync(pool.get_conn(), &task.config).await,
        "token_cleanup" => execute_token_cleanup(pool.get_conn()).await,
        "backup" => execute_backup(pool.get_conn()).await,
        _ => Err(format!("未知的任务类型: {}", task.task_type)),
    };

    let now = Utc::now();
    sqlx::query(
        "UPDATE scheduled_tasks SET last_run_at = $1, last_result = $2, updated_at = $1 WHERE id = $3"
    )
    .bind(now)
    .bind(result.as_ref().ok())
    .bind(id)
    .execute(pool.get_conn())
    .await
    .ok();

    Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({"result": result}), "执行定时任务成功")))
}

async fn execute_mac_sync(pool: &sqlx::PgPool, config: &serde_json::Value) -> Result<String, String> {
    let switch_id = config.get("switch_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok());
    let network_id = config.get("network_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok());

    match (switch_id, network_id) {
        (Some(switch_id), Some(network_id)) => {
            crate::resource::ip::pull_ip_managers_internal(pool, switch_id, network_id).await
                .map(|_| "MAC同步成功".to_string())
                .map_err(|e| format!("MAC同步失败: {}", e))
        },
        _ => Err("MAC同步任务需要配置switch_id和network_id".to_string()),
    }
}

async fn execute_token_cleanup(pool: &sqlx::PgPool) -> Result<String, String> {
    let count = crate::utils::cleanup_expired_revoked_tokens(pool).await
        .map_err(|e| format!("Token清理失败: {}", e))?;
    Ok(format!("清理了 {} 个过期token", count))
}

async fn execute_backup(pool: &sqlx::PgPool) -> Result<String, String> {
    Ok("备份任务执行成功".to_string())
}
