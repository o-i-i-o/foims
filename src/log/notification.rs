use crate::db::DbPool;
use crate::models::{ApiResponse, Notification};
use crate::utils::DEFAULT_PAGE;
use actix_web::{HttpResponse, Result, web};
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

// 获取通知列表（支持搜索和分页）
pub async fn get_notifications(
    pool: web::Data<DbPool>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
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

    let total: i64 = match sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM notifications {where_clause}"
    ))
    .fetch_one(pool.get_conn())
    .await
    {
        Ok(t) => t,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("数据库查询错误: {err}"))));
        }
    };

    let notifications = match sqlx::query_as::<_, Notification>(&format!(
        "SELECT id, user_id, title, content, notification_type, read, created_at::TIMESTAMPTZ FROM notifications {where_clause} ORDER BY created_at DESC LIMIT {page_size} OFFSET {offset}"
    ))
    .fetch_all(pool.get_conn())
    .await
    {
        Ok(notifications) => notifications,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("数据库查询错误: {err}"))));
        }
    };

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

// 标记通知为已读
pub async fn mark_notification_read(
    pool: web::Data<DbPool>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let notification_id = *id;

    // 检查通知是否存在
    let existing_notification =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM notifications WHERE id = $1")
            .bind(notification_id)
            .fetch_optional(pool.get_conn())
            .await
        {
            Ok(notification) => notification,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("数据库查询错误: {err}"))));
            }
        };

    if existing_notification.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("通知不存在")));
    }

    // 标记为已读
    if let Err(err) = sqlx::query("UPDATE notifications SET read = true WHERE id = $1")
        .bind(notification_id)
        .execute(pool.get_conn())
        .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库更新错误: {err}"))));
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "通知已标记为已读")))
}

// 标记所有通知为已读
pub async fn mark_all_notifications_read(pool: web::Data<DbPool>) -> Result<HttpResponse> {
    if let Err(err) = sqlx::query("UPDATE notifications SET read = true")
        .execute(pool.get_conn())
        .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库更新错误: {err}"))));
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "所有通知已标记为已读")))
}

// 创建新通知
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
