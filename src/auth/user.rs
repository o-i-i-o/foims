//! 用户管理 CRUD。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::auth::utils::hash_password;
use crate::error::{AppError, msg};
use crate::models::{User, UserCreate, UserUpdate};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::{Pagination, paged_response};
use ipma_common::log_info;

pub async fn get_users(
    _secadmin: crate::auth::extractor::SecAdminUser,
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
        ("username", "desc") => "ORDER BY username DESC",
        ("username", _) => "ORDER BY username ASC",
        ("email", "desc") => "ORDER BY email DESC",
        ("email", _) => "ORDER BY email ASC",
        ("role", "desc") => "ORDER BY role DESC, username ASC",
        ("role", _) => "ORDER BY role ASC, username ASC",
        ("status", "desc") => "ORDER BY status DESC, username ASC",
        ("status", _) => "ORDER BY status ASC, username ASC",
        ("two_factor_enabled", "desc") => "ORDER BY two_factor_enabled DESC, username ASC",
        ("two_factor_enabled", _) => "ORDER BY two_factor_enabled ASC, username ASC",
        ("created_at", "asc") => "ORDER BY created_at ASC",
        _ => "ORDER BY created_at DESC",
    };

    let search_pattern = format!("%{search}%");
    let conn = state.pool()?.get_conn();

    let (total, users) = if search.is_empty() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&conn)
            .await?;

        let users = sqlx::query_as::<_, User>(sqlx::AssertSqlSafe(format!(
            "SELECT id, username, email, role, status, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users {order_clause} LIMIT $1 OFFSET $2"
        )))
        .bind(page_size)
        .bind(offset)
        .fetch_all(&conn)
        .await?;

        (total, users)
    } else {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE username ILIKE $1 OR email ILIKE $1",
        )
        .bind(&search_pattern)
        .fetch_one(&conn)
        .await?;

        let users = sqlx::query_as::<_, User>(sqlx::AssertSqlSafe(format!(
            "SELECT id, username, email, role, status, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE username ILIKE $1 OR email ILIKE $1 {order_clause} LIMIT $2 OFFSET $3"
        )))
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&conn)
        .await?;

        (total, users)
    };

    Ok(crate::error::ok_json(
        paged_response(users, total, &pagination),
        "server.user.list_retrieved",
    ))
}

pub async fn create_user(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _secadmin: crate::auth::extractor::SecAdminUser,
    AppJson(req): AppJson<UserCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let conn = state.pool()?.get_conn();

    let existing_user = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE username = $1")
        .bind(&req.username)
        .fetch_optional(&conn)
        .await?;

    if existing_user.is_some() {
        return Err(AppError::Conflict(msg("server.user.username_exists")));
    }

    let existing_email = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE email = $1")
        .bind(&req.email)
        .fetch_optional(&conn)
        .await?;

    if existing_email.is_some() {
        return Err(AppError::Conflict(msg("server.user.email_exists")));
    }

    // 等保密码策略：复杂度校验（新用户无历史记录可查）
    crate::auth::password_policy::validate_complexity(&state.pool()?.get_conn(), &req.password)
        .await?;

    let hashed_password = hash_password(&req.password).await?;

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO users (id, username, password_hash, email, role, status, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(&req.username)
    .bind(&hashed_password)
    .bind(&req.email)
    .bind(&req.role)
    .bind(true)
    .bind(now)
    .bind(now)
    .execute(&conn)
    .await?;

    // 等保密码策略：记录密码历史（供后续改密时的重复使用检查）
    crate::auth::password_policy::record_history(&conn, id, &hashed_password).await;

    let details = json!({"username": req.username, "email": req.email, "role": req.role});
    log_op_best_effort(&conn, &meta, "create_user", "user", Some(&id), &details).await;
    log_info!("log.user.created", username = req.username, id = id);

    let user = User {
        id,
        username: req.username.clone(),
        email: req.email.clone(),
        role: req.role.clone(),
        status: true,
        two_factor_enabled: false,
        two_factor_verified: false,
        created_at: now,
        updated_at: now,
    };

    Ok(crate::error::ok_json(user, "server.user.created"))
}

pub async fn get_user(
    _secadmin: crate::auth::extractor::SecAdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, email, role, status, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&conn)
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.user.not_found")))?;

    Ok(crate::error::ok_json(user, "server.user.retrieved"))
}

pub async fn update_user(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _secadmin: crate::auth::extractor::SecAdminUser,
    Path(id): Path<Uuid>,
    AppJson(req): AppJson<UserUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let conn = state.pool()?.get_conn();

    let existing_user = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&conn)
        .await?;

    if existing_user.is_none() {
        return Err(AppError::NotFound(msg("server.user.not_found")));
    }

    let now = Utc::now();

    // 权限或启用状态变更时，同语句吊销历史令牌（强制重新登录）：
    // 拆成两条语句时第二条失败会导致降权已生效但旧令牌未被强制下线（D-2）
    sqlx::query(
        "UPDATE users SET
         email = COALESCE($1, email),
         role = COALESCE($2, role),
         status = COALESCE($3, status),
         tokens_invalidated_at = CASE WHEN $2::VARCHAR IS NOT NULL OR $3::BOOLEAN IS FALSE THEN NOW() ELSE tokens_invalidated_at END,
         updated_at = $4
         WHERE id = $5",
    )
    .bind(&req.email)
    .bind(&req.role)
    .bind(req.status)
    .bind(now)
    .bind(id)
    .execute(&conn)
    .await?;

    let details = json!({"email": req.email, "role": req.role, "status": req.status});
    log_op_best_effort(&conn, &meta, "update_user", "user", Some(&id), &details).await;
    log_info!("log.user.updated", id = id);

    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, email, role, status, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE id = $1"
    )
    .bind(id)
    .fetch_one(&conn)
    .await?;

    Ok(crate::error::ok_json(user, "server.user.updated"))
}

pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _secadmin: crate::auth::extractor::SecAdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let existing_user = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&conn)
        .await?;

    if existing_user.is_none() {
        return Err(AppError::NotFound(msg("server.user.not_found")));
    }

    let mut tx = conn.begin().await?;

    // operation_logs.user_id ON DELETE SET NULL — 保留日志，user_id 置 NULL
    // notifications/user_tokens ON DELETE CASCADE — 自动级联删除
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = json!({});
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete_user",
        "user",
        Some(&id),
        &details,
    )
    .await;
    log_info!("log.user.deleted", id = id);

    Ok(crate::error::ok_json((), "server.user.deleted"))
}
