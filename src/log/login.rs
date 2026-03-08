use crate::db::DbPool;
use crate::models::{ApiResponse, LoginLog};
use actix_web::{HttpResponse, Result, web};

pub async fn get_login_logs(
    pool: web::Data<DbPool>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    let page: i64 = query.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = query.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(50);
    let search = query.get("search").cloned().unwrap_or_default();
    let offset = (page - 1) * page_size;

    let search_pattern = format!("%{}%", search);

    let (total, logs) = if search.is_empty() {
        let total: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM login_logs")
            .fetch_one(pool.get_conn())
            .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {}", err),
                )));
            }
        };

        let logs = match sqlx::query_as::<_, LoginLog>(
            "SELECT id, username, ip_address, user_agent, success, error_message, created_at::TIMESTAMPTZ FROM login_logs ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
        {
            Ok(l) => l,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {}", err),
                )));
            }
        };

        (total, logs)
    } else {
        let total: i64 = match sqlx::query_scalar(
            "SELECT COUNT(*) FROM login_logs WHERE username ILIKE $1 OR ip_address ILIKE $1 OR user_agent ILIKE $1"
        )
        .bind(&search_pattern)
        .fetch_one(pool.get_conn())
        .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {}", err),
                )));
            }
        };

        let logs = match sqlx::query_as::<_, LoginLog>(
            "SELECT id, username, ip_address, user_agent, success, error_message, created_at::TIMESTAMPTZ FROM login_logs WHERE username ILIKE $1 OR ip_address ILIKE $1 OR user_agent ILIKE $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
        {
            Ok(l) => l,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {}", err),
                )));
            }
        };

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
