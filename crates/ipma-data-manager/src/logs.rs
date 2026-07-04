use crate::types::{ApiResponse, ClearLogsRequest, DataError, DataProvider, DataResult};
use actix_web::{HttpResponse, web};
use serde_json::json;
use validator::Validate;

pub async fn clear_logs<P: DataProvider>(
    provider: P,
    req: web::Json<ClearLogsRequest>,
) -> DataResult<HttpResponse> {
    let req = req.into_inner();
    req.validate()?;
    let days = req.days.unwrap_or(0);

    if days < 0 {
        return Err(DataError::Validation("保留天数不能为负数".to_string()));
    }

    let pool = provider.pool()?;

    let result = match req.log_type.as_str() {
        "operation" => {
            if days == 0 {
                sqlx::query("DELETE FROM operation_logs")
                    .execute(&pool)
                    .await
            } else {
                sqlx::query(
                    "DELETE FROM operation_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&pool)
                .await
            }
        }
        "login" => {
            if days == 0 {
                sqlx::query("DELETE FROM login_logs").execute(&pool).await
            } else {
                sqlx::query(
                    "DELETE FROM login_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&pool)
                .await
            }
        }
        "notification" => {
            if days == 0 {
                sqlx::query("DELETE FROM notifications")
                    .execute(&pool)
                    .await
            } else {
                sqlx::query(
                    "DELETE FROM notifications WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&pool)
                .await
            }
        }
        "all" => {
            let mut deleted = 0u64;

            if days == 0 {
                if let Ok(r) = sqlx::query("DELETE FROM operation_logs")
                    .execute(&pool)
                    .await
                {
                    deleted += r.rows_affected();
                }
                if let Ok(r) = sqlx::query("DELETE FROM login_logs").execute(&pool).await {
                    deleted += r.rows_affected();
                }
                if let Ok(r) = sqlx::query("DELETE FROM notifications")
                    .execute(&pool)
                    .await
                {
                    deleted += r.rows_affected();
                }
            } else {
                if let Ok(r) = sqlx::query(
                    "DELETE FROM operation_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&pool)
                .await
                {
                    deleted += r.rows_affected();
                }
                if let Ok(r) = sqlx::query(
                    "DELETE FROM login_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&pool)
                .await
                {
                    deleted += r.rows_affected();
                }
                if let Ok(r) = sqlx::query(
                    "DELETE FROM notifications WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&pool)
                .await
                {
                    deleted += r.rows_affected();
                }
            }

            return Ok(HttpResponse::Ok().json(ApiResponse::success(
                json!({ "deleted": deleted }),
                &format!("成功清理 {deleted} 条日志记录"),
            )));
        }
        _ => {
            return Err(DataError::Validation("无效的日志类型".to_string()));
        }
    };

    let r = result.map_err(DataError::from)?;
    let deleted = r.rows_affected();
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
