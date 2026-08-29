//! 操作审计日志查询。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::Response;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::utils::pagination::{Pagination, paged_response};
use ipma_common::{AppError, msg};
use ipma_models::OperationLog;

pub async fn get_operation_logs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let resource_type = query.get("resource_type").cloned().unwrap_or_default();
    let resource_id = query.get("resource_id").cloned().unwrap_or_default();
    let user_id = query.get("user_id").cloned().unwrap_or_default();
    let action = query.get("action").cloned().unwrap_or_default();
    let pagination = Pagination::from_query(&query);
    let page_size = pagination.page_size;
    let offset = pagination.offset;

    let sort_by = query.get("sort_by").cloned().unwrap_or_default();
    let sort_order = query.get("sort_order").cloned().unwrap_or_default();

    // ORDER BY 白名单，未匹配时回落默认序，避免注入
    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("username", "desc") => "ORDER BY u.username DESC NULLS LAST, ol.created_at DESC",
        ("username", _) => "ORDER BY u.username ASC NULLS LAST, ol.created_at DESC",
        ("operation_type", "desc") => "ORDER BY ol.action DESC, ol.created_at DESC",
        ("operation_type", _) => "ORDER BY ol.action ASC, ol.created_at DESC",
        ("resource_type", "desc") => {
            "ORDER BY ol.resource_type DESC NULLS LAST, ol.created_at DESC"
        }
        ("resource_type", _) => "ORDER BY ol.resource_type ASC NULLS LAST, ol.created_at DESC",
        ("result", "desc") => "ORDER BY ol.result DESC, ol.created_at DESC",
        ("result", _) => "ORDER BY ol.result ASC, ol.created_at DESC",
        ("ip_address", "desc") => "ORDER BY ol.ip_address DESC NULLS LAST, ol.created_at DESC",
        ("ip_address", _) => "ORDER BY ol.ip_address ASC NULLS LAST, ol.created_at DESC",
        ("created_at", "asc") => "ORDER BY ol.created_at ASC",
        _ => "ORDER BY ol.created_at DESC",
    };

    let search_pattern = crate::utils::escape_like(&action);

    let has_filters = !resource_type.is_empty()
        || !resource_id.is_empty()
        || !user_id.is_empty()
        || !action.is_empty();

    let conn = state.pool()?.get_conn();

    let (total, logs) = if has_filters {
        let parsed_resource_id = if resource_id.is_empty() {
            None
        } else {
            Some(Uuid::parse_str(&resource_id).map_err(|_| {
                AppError::Validation(
                    msg("server.logs.invalid_resource_id").with("value", &resource_id),
                )
            })?)
        };

        let parsed_user_id = if user_id.is_empty() {
            None
        } else {
            Some(Uuid::parse_str(&user_id).map_err(|_| {
                AppError::Validation(msg("server.logs.invalid_user_id").with("value", &user_id))
            })?)
        };

        let total: i64 = sqlx::query_scalar::<_, i64>(
            r"SELECT COUNT(*) FROM operation_logs ol 
               LEFT JOIN users u ON ol.user_id = u.id 
               WHERE ($1::text = '' OR ol.resource_type = $1)
               AND ($2::uuid IS NULL OR ol.resource_id = $2)
               AND ($3::uuid IS NULL OR ol.user_id = $3)
               AND ($4::text = '' OR ol.action ILIKE $5 OR u.username ILIKE $5 OR ol.ip_address ILIKE $5 OR ol.resource_type ILIKE $5 OR ol.resource_id::TEXT ILIKE $5)"
        )
        .bind(&resource_type)
        .bind(parsed_resource_id)
        .bind(parsed_user_id)
        .bind(&action)
        .bind(&search_pattern)
        .fetch_one(&conn)
        .await?;

        let logs = sqlx::query_as::<_, OperationLog>(sqlx::AssertSqlSafe(format!(
            r"SELECT ol.id, ol.user_id, u.username, ol.action, ol.action as operation_type, ol.resource_type, ol.resource_id, ol.details, ol.result, ol.ip_address, ol.created_at::TIMESTAMPTZ
               FROM operation_logs ol
               LEFT JOIN users u ON ol.user_id = u.id
               WHERE ($1::text = '' OR ol.resource_type = $1)
               AND ($2::uuid IS NULL OR ol.resource_id = $2)
               AND ($3::uuid IS NULL OR ol.user_id = $3)
               AND ($4::text = '' OR ol.action ILIKE $5 OR u.username ILIKE $5 OR ol.ip_address ILIKE $5 OR ol.resource_type ILIKE $5 OR ol.resource_id::TEXT ILIKE $5)
               {order_clause}
               LIMIT $6 OFFSET $7"
        )))
        .bind(&resource_type)
        .bind(parsed_resource_id)
        .bind(parsed_user_id)
        .bind(&action)
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&conn)
        .await?;

        (total, logs)
    } else {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operation_logs ol")
            .fetch_one(&conn)
            .await?;

        let logs = sqlx::query_as::<_, OperationLog>(sqlx::AssertSqlSafe(format!(
            "SELECT ol.id, ol.user_id, u.username, ol.action, ol.action as operation_type, ol.resource_type, ol.resource_id, ol.details, ol.result, ol.ip_address, ol.created_at::TIMESTAMPTZ FROM operation_logs ol LEFT JOIN users u ON ol.user_id = u.id {order_clause} LIMIT $1 OFFSET $2"
        )))
        .bind(page_size)
        .bind(offset)
        .fetch_all(&conn)
        .await?;

        (total, logs)
    };

    Ok(ipma_common::ok_json(
        paged_response(logs, total, &pagination),
        "server.logs.operation_retrieved",
    ))
}
