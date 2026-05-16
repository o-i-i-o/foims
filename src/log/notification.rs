use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{ApiResponse, Notification};
use crate::utils::DEFAULT_PAGE;
use actix_web::{HttpResponse, web};
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

pub async fn get_notifications(
    state: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let page: i64 = query
        .get("page")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PAGE);
    let page_size: i64 = query
        .get("page_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let status = query
        .get("status")
        .cloned()
        .unwrap_or_else(|| "all".to_string());
    let offset = (page - 1) * page_size;

    let mut where_conditions = Vec::new();

    match status.as_str() {
        "unread" => where_conditions.push("read = false".to_string()),
        "read" => where_conditions.push("read = true".to_string()),
        _ => {}
    }

    let where_clause = if where_conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_conditions.join(" AND "))
    };

    let conn = state.pool()?.get_conn();

    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM notifications {where_clause}"
    ))
    .fetch_one(&conn)
    .await?;

    let notifications = sqlx::query_as::<_, Notification>(&format!(
        "SELECT id, user_id, title, content, notification_type, read, created_at::TIMESTAMPTZ FROM notifications {where_clause} ORDER BY created_at DESC LIMIT {page_size} OFFSET {offset}"
    ))
    .fetch_all(&conn)
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": notifications,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "通知列表获取成功",
    )))
}

pub async fn mark_notification_read(
    state: web::Data<AppState>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let notification_id = *id;
    let conn = state.pool()?.get_conn();

    let existing_notification =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM notifications WHERE id = $1")
            .bind(notification_id)
            .fetch_optional(&conn)
            .await?;

    if existing_notification.is_none() {
        return Err(AppError::NotFound("通知不存在".to_string()));
    }

    sqlx::query("UPDATE notifications SET read = true WHERE id = $1")
        .bind(notification_id)
        .execute(&conn)
        .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "通知已标记为已读")))
}

pub async fn mark_all_notifications_read(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let conn = state.pool()?.get_conn();

    sqlx::query("UPDATE notifications SET read = true")
        .execute(&conn)
        .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "所有通知已标记为已读")))
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
