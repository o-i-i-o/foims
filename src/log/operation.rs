use crate::db::DbPool;
use crate::models::{ApiResponse, OperationLog};
use crate::utils::DEFAULT_PAGE;
use actix_web::{HttpResponse, Result, web};
use uuid::Uuid;

pub async fn get_operation_logs(
    pool: web::Data<DbPool>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    let resource_type = query.get("resource_type").cloned().unwrap_or_default();
    let resource_id = query.get("resource_id").cloned().unwrap_or_default();
    let user_id = query.get("user_id").cloned().unwrap_or_default();
    let action = query.get("action").cloned().unwrap_or_default();
    let page: i64 = query.get("page").and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_PAGE);
    let page_size: i64 = query.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(50);
    let offset = (page - 1) * page_size;

    let search_pattern = format!("%{}%", action);

    let has_filters = !resource_type.is_empty() 
        || !resource_id.is_empty() 
        || !user_id.is_empty() 
        || !action.is_empty();

    let (total, logs) = if !has_filters {
        let total: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM operation_logs ol")
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

        let logs = match sqlx::query_as::<_, OperationLog>(
            "SELECT ol.id, ol.user_id, COALESCE(u.username, '已删除用户') as username, ol.action, ol.action as operation_type, ol.resource_type, ol.resource_id, ol.details, ol.result, ol.ip_address, ol.created_at::TIMESTAMPTZ FROM operation_logs ol LEFT JOIN users u ON ol.user_id = u.id ORDER BY ol.created_at DESC LIMIT $1 OFFSET $2"
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
        let parsed_resource_id = if resource_id.is_empty() {
            None
        } else {
            Uuid::parse_str(&resource_id).ok()
        };

        let parsed_user_id = if user_id.is_empty() {
            None
        } else {
            Uuid::parse_str(&user_id).ok()
        };

        let total: i64 = match sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM operation_logs ol 
               LEFT JOIN users u ON ol.user_id = u.id 
               WHERE ($1::text = '' OR ol.resource_type = $1)
               AND ($2::uuid IS NULL OR ol.resource_id = $2)
               AND ($3::uuid IS NULL OR ol.user_id = $3)
               AND ($4::text = '' OR ol.action ILIKE $5 OR u.username ILIKE $5 OR ol.ip_address ILIKE $5 OR ol.resource_type ILIKE $5 OR ol.resource_id::TEXT ILIKE $5)"#
        )
        .bind(&resource_type)
        .bind(parsed_resource_id)
        .bind(parsed_user_id)
        .bind(&action)
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

        let logs = match sqlx::query_as::<_, OperationLog>(
            r#"SELECT ol.id, ol.user_id, COALESCE(u.username, '已删除用户') as username, ol.action, ol.action as operation_type, ol.resource_type, ol.resource_id, ol.details, ol.result, ol.ip_address, ol.created_at::TIMESTAMPTZ 
               FROM operation_logs ol 
               LEFT JOIN users u ON ol.user_id = u.id 
               WHERE ($1::text = '' OR ol.resource_type = $1)
               AND ($2::uuid IS NULL OR ol.resource_id = $2)
               AND ($3::uuid IS NULL OR ol.user_id = $3)
               AND ($4::text = '' OR ol.action ILIKE $5 OR u.username ILIKE $5 OR ol.ip_address ILIKE $5 OR ol.resource_type ILIKE $5 OR ol.resource_id::TEXT ILIKE $5)
               ORDER BY ol.created_at DESC 
               LIMIT $6 OFFSET $7"#
        )
        .bind(&resource_type)
        .bind(parsed_resource_id)
        .bind(parsed_user_id)
        .bind(&action)
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
            "data": logs,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "操作日志获取成功",
    )))
}
