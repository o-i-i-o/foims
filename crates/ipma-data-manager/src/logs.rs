use crate::types::{ApiResponse, ClearLogsRequest, DataError, DataProvider, DataResult};
use actix_web::{HttpResponse, web};
use serde_json::json;
use sqlx::PgPool;
use validator::Validate;

/// 核心日志清理逻辑，返回删除的行数
pub async fn clear_logs_core(pool: &PgPool, days: i32, log_type: &str) -> DataResult<u64> {
    if days < 0 {
        return Err(DataError::Validation("保留天数不能为负数".to_string()));
    }

    match log_type {
        "operation" => {
            let result = if days == 0 {
                sqlx::query("DELETE FROM operation_logs")
                    .execute(pool)
                    .await
            } else {
                sqlx::query(
                    "DELETE FROM operation_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(pool)
                .await
            };
            Ok(result.map_err(DataError::from)?.rows_affected())
        }
        "login" => {
            let result = if days == 0 {
                sqlx::query("DELETE FROM login_logs").execute(pool).await
            } else {
                sqlx::query(
                    "DELETE FROM login_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(pool)
                .await
            };
            Ok(result.map_err(DataError::from)?.rows_affected())
        }
        "notification" => {
            let result = if days == 0 {
                sqlx::query("DELETE FROM notifications").execute(pool).await
            } else {
                sqlx::query(
                    "DELETE FROM notifications WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(pool)
                .await
            };
            Ok(result.map_err(DataError::from)?.rows_affected())
        }
        "all" => {
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
                if let Ok(r) = sqlx::query(
                    "DELETE FROM login_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
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

            Ok(deleted)
        }
        _ => Err(DataError::Validation("无效的日志类型".to_string())),
    }
}

/// API 端点：清理日志
pub async fn clear_logs<P: DataProvider>(
    provider: P,
    req: web::Json<ClearLogsRequest>,
) -> DataResult<HttpResponse> {
    let req = req.into_inner();
    req.validate()?;
    let days = req.days.unwrap_or(0);
    let pool = provider.pool()?;

    let deleted = clear_logs_core(&pool, days, &req.log_type).await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({ "deleted": deleted }),
        &format!("成功清理 {deleted} 条日志记录"),
    )))
}

pub async fn get_logs_stats<P: DataProvider>(provider: P) -> DataResult<HttpResponse> {
    let pool = provider.pool()?;

    let operation_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operation_logs")
        .fetch_one(&pool)
        .await
        .map_err(DataError::from)?;

    let login_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM login_logs")
        .fetch_one(&pool)
        .await
        .map_err(DataError::from)?;

    let notification_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications")
        .fetch_one(&pool)
        .await
        .map_err(DataError::from)?;

    let operation_oldest: Option<String> = sqlx::query_scalar(
        "SELECT created_at::text FROM operation_logs ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(DataError::from)?;

    let login_oldest: Option<String> = sqlx::query_scalar(
        "SELECT created_at::text FROM login_logs ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(DataError::from)?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "operation_logs": { "count": operation_count, "oldest": operation_oldest },
            "login_logs": { "count": login_count, "oldest": login_oldest },
            "notifications": { "count": notification_count }
        }),
        "日志统计获取成功",
    )))
}
