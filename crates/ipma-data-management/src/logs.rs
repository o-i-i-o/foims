//! 日志清理与统计。

use crate::types::{ClearLogsRequest, DataError, DataProvider, DataResult, ok_json};
use axum::response::Response;
use ipma_common::msg;
use serde_json::json;
use sqlx::PgPool;
use validator::Validate;

/// 核心日志清理逻辑，返回删除的行数
pub async fn clear_logs_core(pool: &PgPool, days: i32, log_type: &str) -> DataResult<u64> {
    if days < 0 {
        return Err(DataError::Validation(msg("server.logs.days_invalid")));
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
            let mut tx = pool.begin().await.map_err(DataError::from)?;
            let mut deleted = 0u64;

            let op_result = if days == 0 {
                sqlx::query("DELETE FROM operation_logs")
                    .execute(&mut *tx)
                    .await
            } else {
                sqlx::query(
                    "DELETE FROM operation_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&mut *tx)
                .await
            };
            deleted += op_result.map_err(DataError::from)?.rows_affected();

            let login_result = if days == 0 {
                sqlx::query("DELETE FROM login_logs")
                    .execute(&mut *tx)
                    .await
            } else {
                sqlx::query(
                    "DELETE FROM login_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&mut *tx)
                .await
            };
            deleted += login_result.map_err(DataError::from)?.rows_affected();

            let notif_result = if days == 0 {
                sqlx::query("DELETE FROM notifications")
                    .execute(&mut *tx)
                    .await
            } else {
                sqlx::query(
                    "DELETE FROM notifications WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&mut *tx)
                .await
            };
            deleted += notif_result.map_err(DataError::from)?.rows_affected();

            tx.commit().await.map_err(DataError::from)?;
            Ok(deleted)
        }
        _ => Err(DataError::Validation(msg("server.logs.type_invalid"))),
    }
}

/// API 端点：清理日志
pub async fn clear_logs<P: DataProvider>(
    provider: P,
    req: ClearLogsRequest,
) -> DataResult<Response> {
    req.validate()?;
    // 0 语义为"删除全部"，属危险操作，缺省时必须拒绝而非静默回退
    let Some(days) = req.days else {
        return Err(DataError::Validation(msg("server.logs.days_required")));
    };
    let pool = provider.pool()?;

    let deleted = clear_logs_core(&pool, days, &req.log_type).await?;

    Ok(ok_json(
        json!({ "deleted": deleted }),
        msg("server.logs.cleared").with("count", deleted),
    ))
}

pub async fn get_logs_stats<P: DataProvider>(provider: P) -> DataResult<Response> {
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

    Ok(ok_json(
        json!({
            "operation_logs": { "count": operation_count, "oldest": operation_oldest },
            "login_logs": { "count": login_count, "oldest": login_oldest },
            "notifications": { "count": notification_count }
        }),
        "server.logs.stats_retrieved",
    ))
}
