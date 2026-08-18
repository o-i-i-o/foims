//! 登录日志查询。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::Response;

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::LoginLog;
use crate::utils::pagination::{Pagination, paged_response};

pub async fn get_login_logs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();

    let sort_by = query.get("sort_by").cloned().unwrap_or_default();
    let sort_order = query.get("sort_order").cloned().unwrap_or_default();

    // ORDER BY 白名单，未匹配时回落默认序，避免注入
    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("username", "desc") => "ORDER BY username DESC, created_at DESC",
        ("username", _) => "ORDER BY username ASC, created_at DESC",
        ("ip_address", "desc") => "ORDER BY ip_address DESC, created_at DESC",
        ("ip_address", _) => "ORDER BY ip_address ASC, created_at DESC",
        ("success", "desc") => "ORDER BY success DESC, created_at DESC",
        ("success", _) => "ORDER BY success ASC, created_at DESC",
        ("created_at", "asc") => "ORDER BY created_at ASC",
        _ => "ORDER BY created_at DESC",
    };

    let search_pattern = crate::utils::escape_like(&search);
    let conn = state.pool()?.get_conn();

    let (total, logs) = if search.is_empty() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM login_logs")
            .fetch_one(&conn)
            .await?;

        let logs = sqlx::query_as::<_, LoginLog>(sqlx::AssertSqlSafe(format!(
            "SELECT id, username, ip_address, user_agent, success, error_message, created_at::TIMESTAMPTZ FROM login_logs {order_clause} LIMIT $1 OFFSET $2"
        )))
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

        let logs = sqlx::query_as::<_, LoginLog>(sqlx::AssertSqlSafe(format!(
            "SELECT id, username, ip_address, user_agent, success, error_message, created_at::TIMESTAMPTZ FROM login_logs WHERE username ILIKE $1 OR ip_address ILIKE $1 OR user_agent ILIKE $1 {order_clause} LIMIT $2 OFFSET $3"
        )))
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&conn)
        .await?;

        (total, logs)
    };

    Ok(crate::error::ok_json(
        paged_response(logs, total, &pagination),
        "server.logs.login_retrieved",
    ))
}
