//! 站内通知查询与已读管理。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::utils::pagination::{Pagination, paged_response};
use foims_auth::extractor::AuthUser;
use foims_common::{AppError, msg};
use foims_models::Notification;

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

    let conn = state.pool()?.get_conn();

    // read 状态为白名单映射：仅追加常量条件，user_id 经 push_bind 传参
    let read_clause = match status.as_str() {
        "unread" => " AND read = false",
        "read" => " AND read = true",
        _ => "",
    };

    let mut count_qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT COUNT(*) FROM notifications WHERE user_id = ",
    );
    count_qb.push_bind(user_id);
    count_qb.push(read_clause);
    let total: i64 = count_qb.build_query_scalar().fetch_one(&conn).await?;

    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT id, user_id, title, content, notification_type, read, created_at::TIMESTAMPTZ FROM notifications WHERE user_id = ",
    );
    qb.push_bind(user_id);
    qb.push(read_clause);
    // 排序段为白名单常量，经 push 拼接
    qb.push(" ").push(order_clause);
    qb.push(" LIMIT ").push_bind(page_size);
    qb.push(" OFFSET ").push_bind(offset);
    let notifications = qb.build_query_as::<Notification>().fetch_all(&conn).await?;

    Ok(foims_common::ok_json(
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

    Ok(foims_common::ok_json((), "server.notification.marked_read"))
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

    Ok(foims_common::ok_json(
        (),
        "server.notification.all_marked_read",
    ))
}
