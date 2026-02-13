use crate::db::DbPool;
use crate::models::{ApiResponse, Notification};
use actix_web::{HttpResponse, Result, web};
use uuid::Uuid;

// 获取通知列表
pub async fn get_notifications(
    pool: web::Data<DbPool>,
    query: web::Query<serde_json::Value>,
) -> Result<HttpResponse> {
    let status = query
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("all");

    let notifications = match status {
        "unread" => {
            sqlx::query_as::<_, Notification>(
                "SELECT id, user_id, title, content, notification_type, read, created_at::TIMESTAMPTZ FROM notifications 
                 WHERE read = false ORDER BY created_at DESC"
            )
        },
        "read" => {
            sqlx::query_as::<_, Notification>(
                "SELECT id, user_id, title, content, notification_type, read, created_at::TIMESTAMPTZ FROM notifications 
                 WHERE read = true ORDER BY created_at DESC"
            )
        },
        _ => {
            sqlx::query_as::<_, Notification>(
                "SELECT id, user_id, title, content, notification_type, read, created_at::TIMESTAMPTZ FROM notifications 
                 ORDER BY created_at DESC"
            )
        }
    };

    let notifications = match notifications.fetch_all(pool.get_conn()).await {
        Ok(notifications) => notifications,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
        }
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<Notification>>::success(
            notifications,
            "通知列表获取成功",
        )),
    )
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
                    .json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
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
            .json(ApiResponse::<()>::error(format!("数据库更新错误: {}", err))));
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
            .json(ApiResponse::<()>::error(format!("数据库更新错误: {}", err))));
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
    sqlx::query(r#"INSERT INTO notifications (id, user_id, title, content, notification_type, read, created_at) 
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#)
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
