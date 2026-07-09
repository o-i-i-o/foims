use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{ApiResponse, LoginLog};
use crate::utils::pagination::Pagination;
use actix_web::{HttpResponse, web};

pub async fn get_login_logs(
    state: web::Data<AppState>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();

    let search_pattern = format!("%{search}%");
    let conn = state.pool()?.get_conn();

    let (total, logs) = if search.is_empty() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM login_logs")
            .fetch_one(&conn)
            .await?;

        let logs = sqlx::query_as::<_, LoginLog>(
            "SELECT id, username, ip_address, user_agent, success, error_message, created_at::TIMESTAMPTZ FROM login_logs ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(&conn)
        .await?;

        (total, logs)
    } else {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM login_logs WHERE username ILIKE $1 OR ip_address ILIKE $1 OR user_agent ILIKE $1"
        )
        .bind(&search_pattern)
        .fetch_one(&conn)
        .await?;

        let logs = sqlx::query_as::<_, LoginLog>(
            "SELECT id, username, ip_address, user_agent, success, error_message, created_at::TIMESTAMPTZ FROM login_logs WHERE username ILIKE $1 OR ip_address ILIKE $1 OR user_agent ILIKE $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&conn)
        .await?;

        (total, logs)
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "items": logs,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "登录日志获取成功",
    )))
}
