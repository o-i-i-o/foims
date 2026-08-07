use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use serde_json::json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::extractor::AuthUser;
use crate::error::AppError;
use crate::models::Notification;
use crate::utils::pagination::Pagination;

pub async fn get_notifications(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let user_id =
        Uuid::parse_str(&auth.sub).map_err(|_| AppError::Internal("无效的用户ID".to_string()))?;
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let status = query
        .get("status")
        .cloned()
        .unwrap_or_else(|| "all".to_string());

    let mut where_conditions = vec!["user_id = $1".to_string()];

    match status.as_str() {
        "unread" => where_conditions.push("read = false".to_string()),
        "read" => where_conditions.push("read = true".to_string()),
        _ => {}
    }

    let where_clause = format!("WHERE {}", where_conditions.join(" AND "));

    let conn = state.pool()?.get_conn();

    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM notifications {where_clause}"
    )))
    .bind(user_id)
    .fetch_one(&conn)
    .await?;

    let notifications = sqlx::query_as::<_, Notification>(sqlx::AssertSqlSafe(format!(
        "SELECT id, user_id, title, content, notification_type, read, created_at::TIMESTAMPTZ FROM notifications {where_clause} ORDER BY created_at DESC LIMIT {page_size} OFFSET {offset}"
    )))
    .bind(user_id)
    .fetch_all(&conn)
    .await?;

    Ok(crate::error::ok_json(
        json!({
            "items": notifications,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "通知列表获取成功",
    ))
}

pub async fn mark_notification_read(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(notification_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let user_id =
        Uuid::parse_str(&auth.sub).map_err(|_| AppError::Internal("无效的用户ID".to_string()))?;
    let conn = state.pool()?.get_conn();

    let existing_notification = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM notifications WHERE id = $1 AND user_id = $2",
    )
    .bind(notification_id)
    .bind(user_id)
    .fetch_optional(&conn)
    .await?;

    if existing_notification.is_none() {
        return Err(AppError::NotFound("通知不存在".to_string()));
    }

    sqlx::query("UPDATE notifications SET read = true WHERE id = $1 AND user_id = $2")
        .bind(notification_id)
        .bind(user_id)
        .execute(&conn)
        .await?;

    Ok(crate::error::ok_json((), "通知已标记为已读"))
}

pub async fn mark_all_notifications_read(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Response, AppError> {
    let user_id =
        Uuid::parse_str(&auth.sub).map_err(|_| AppError::Internal("无效的用户ID".to_string()))?;
    let conn = state.pool()?.get_conn();

    sqlx::query("UPDATE notifications SET read = true WHERE user_id = $1")
        .bind(user_id)
        .execute(&conn)
        .await?;

    Ok(crate::error::ok_json((), "所有通知已标记为已读"))
}

pub async fn create_notification(
    pool: &sqlx::PgPool,
    title: &str,
    content: &str,
    notification_type: &str,
    user_id: Option<&Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(r"INSERT INTO notifications (id, user_id, title, content, notification_type, read, created_at) 
           VALUES ($1, $2, $3, $4, $5, $6, $7)")
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(title)
        .bind(content)
        .bind(notification_type)
        .bind(false)
        .bind(chrono::Utc::now())
        .execute(pool)
        .await?;
    Ok(())
}
