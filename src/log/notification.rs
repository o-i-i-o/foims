//! 站内通知查询与已读管理。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::extractor::AuthUser;
use crate::models::Notification;
use crate::utils::pagination::{Pagination, paged_response};
use ipma_common::{AppError, msg};

pub async fn get_notifications(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let user_id = Uuid::parse_str(&auth.sub)
        .map_err(|_| AppError::Internal(msg("server.common.user_id_invalid")))?;
    let pagination = Pagination::from_query(&query);
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let status = query
        .get("status")
        .cloned()
        .unwrap_or_else(|| "all".to_string());

    let sort_by = query.get("sort_by").cloned().unwrap_or_default();
    let sort_order = query.get("sort_order").cloned().unwrap_or_default();

    // ORDER BY 白名单，未匹配时回落默认序，避免注入
    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("title", "desc") => "ORDER BY title DESC, created_at DESC",
        ("title", _) => "ORDER BY title ASC, created_at DESC",
        ("read", "desc") => "ORDER BY read DESC, created_at DESC",
        ("read", _) => "ORDER BY read ASC, created_at DESC",
        ("created_at", "asc") => "ORDER BY created_at ASC",
        _ => "ORDER BY created_at DESC",
    };

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
        "SELECT id, user_id, title, content, notification_type, read, created_at::TIMESTAMPTZ FROM notifications {where_clause} {order_clause} LIMIT {page_size} OFFSET {offset}"
    )))
    .bind(user_id)
    .fetch_all(&conn)
    .await?;

    Ok(ipma_common::ok_json(
        paged_response(notifications, total, &pagination),
        "server.notification.list_retrieved",
    ))
}

pub async fn mark_notification_read(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(notification_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let user_id = Uuid::parse_str(&auth.sub)
        .map_err(|_| AppError::Internal(msg("server.common.user_id_invalid")))?;
    let conn = state.pool()?.get_conn();

    let existing_notification = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM notifications WHERE id = $1 AND user_id = $2",
    )
    .bind(notification_id)
    .bind(user_id)
    .fetch_optional(&conn)
    .await?;

    if existing_notification.is_none() {
        return Err(AppError::NotFound(msg("server.notification.not_found")));
    }

    sqlx::query("UPDATE notifications SET read = true WHERE id = $1 AND user_id = $2")
        .bind(notification_id)
        .bind(user_id)
        .execute(&conn)
        .await?;

    Ok(ipma_common::ok_json((), "server.notification.marked_read"))
}

pub async fn mark_all_notifications_read(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Response, AppError> {
    let user_id = Uuid::parse_str(&auth.sub)
        .map_err(|_| AppError::Internal(msg("server.common.user_id_invalid")))?;
    let conn = state.pool()?.get_conn();

    sqlx::query("UPDATE notifications SET read = true WHERE user_id = $1")
        .bind(user_id)
        .execute(&conn)
        .await?;

    Ok(ipma_common::ok_json(
        (),
        "server.notification.all_marked_read",
    ))
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
