//! 登录日志查询。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::Response;

use crate::app_state::AppState;
use crate::utils::pagination::{Pagination, paged_response};
use foims_common::AppError;
use foims_models::LoginLog;

pub async fn get_login_logs(
    State(state): State<Arc<AppState>>,
    // 与 admin_guard_middleware 的 /api/logs/** 角色矩阵一致：admin 或 auditor
    _viewer: foims_auth::extractor::AdminOrAuditorUser,
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

        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id, username, ip_address, user_agent, success, error_message, created_at::TIMESTAMPTZ FROM login_logs",
        );
        // 排序段为白名单常量，经 push 拼接
        qb.push(" ").push(order_clause);
        qb.push(" LIMIT ").push_bind(page_size);
        qb.push(" OFFSET ").push_bind(offset);
        let logs = qb.build_query_as::<LoginLog>().fetch_all(&conn).await?;

        (total, logs)
    } else {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM login_logs WHERE username ILIKE $1 OR ip_address ILIKE $1 OR user_agent ILIKE $1"
        )
        .bind(&search_pattern)
        .fetch_one(&conn)
        .await?;

        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id, username, ip_address, user_agent, success, error_message, created_at::TIMESTAMPTZ FROM login_logs WHERE username ILIKE ",
        );
        qb.push_bind(&search_pattern)
            .push(" OR ip_address ILIKE ")
            .push_bind(&search_pattern)
            .push(" OR user_agent ILIKE ")
            .push_bind(&search_pattern);
        // 排序段为白名单常量，经 push 拼接
        qb.push(" ").push(order_clause);
        qb.push(" LIMIT ").push_bind(page_size);
        qb.push(" OFFSET ").push_bind(offset);
        let logs = qb.build_query_as::<LoginLog>().fetch_all(&conn).await?;

        (total, logs)
    };

    Ok(foims_common::ok_json(
        paged_response(logs, total, &pagination),
        "server.logs.login_retrieved",
    ))
}
