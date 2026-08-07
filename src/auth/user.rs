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
use crate::error::AppError;
use crate::models::{User, UserCreate, UserUpdate};
use crate::routes::static_files::AppJson;
use crate::utils::common::RequestMeta;
use crate::utils::pagination::Pagination;
use crate::utils::{OperationLogParams, log_system_operation};

pub async fn get_users(
    _admin: crate::auth::extractor::AdminUser,
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();

    let search_pattern = format!("%{search}%");
    let conn = state.pool()?.get_conn();

    let (total, users) = if search.is_empty() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&conn)
            .await?;

        let users = sqlx::query_as::<_, User>(
            "SELECT id, username, email, role, status, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        )
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

        let users = sqlx::query_as::<_, User>(
            "SELECT id, username, email, role, status, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE username ILIKE $1 OR email ILIKE $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&conn)
        .await?;

        (total, users)
    };

    Ok(crate::error::ok_json(
        json!({
            "items": users,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "用户获取成功",
    ))
}

pub async fn create_user(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _admin: crate::auth::extractor::AdminUser,
    AppJson(req): AppJson<UserCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let conn = state.pool()?.get_conn();

    let existing_user = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE username = $1")
        .bind(&req.username)
        .fetch_optional(&conn)
        .await?;

    if existing_user.is_some() {
        return Err(AppError::Conflict("用户名已存在".to_string()));
    }

    let existing_email = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE email = $1")
        .bind(&req.email)
        .fetch_optional(&conn)
        .await?;

    if existing_email.is_some() {
        return Err(AppError::Conflict("邮箱已存在".to_string()));
    }

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

    let details = json!({"username": req.username, "email": req.email, "role": req.role});
    if let Err(e) = log_system_operation(
        &conn,
        OperationLogParams {
            ip_address: &meta.ip_address,
            user_id: meta.user_id(),
            action: "create_user",
            resource_type: "user",
            resource_id: Some(&id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        tracing::warn!("记录操作日志失败: {}", e);
    }
    tracing::info!("用户 {} 创建成功, ID: {}", req.username, id);

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

    Ok(crate::error::ok_json(user, "用户创建成功"))
}

pub async fn get_user(
    _admin: crate::auth::extractor::AdminUser,
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
    .ok_or_else(|| AppError::NotFound("用户未找到".to_string()))?;

    Ok(crate::error::ok_json(user, "用户获取成功"))
}

pub async fn update_user(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _admin: crate::auth::extractor::AdminUser,
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
        return Err(AppError::NotFound("用户未找到".to_string()));
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE users SET 
         email = COALESCE($1, email), 
         role = COALESCE($2, role), 
         status = COALESCE($3, status), 
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
    if let Err(e) = log_system_operation(
        &conn,
        OperationLogParams {
            ip_address: &meta.ip_address,
            user_id: meta.user_id(),
            action: "update_user",
            resource_type: "user",
            resource_id: Some(&id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        tracing::warn!("记录操作日志失败: {}", e);
    }
    tracing::info!("用户更新成功, ID: {}", id);

    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, email, role, status, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE id = $1"
    )
    .bind(id)
    .fetch_one(&conn)
    .await?;

    Ok(crate::error::ok_json(user, "用户更新成功"))
}

pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _admin: crate::auth::extractor::AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let existing_user = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&conn)
        .await?;

    if existing_user.is_none() {
        return Err(AppError::NotFound("用户未找到".to_string()));
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
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            ip_address: &meta.ip_address,
            user_id: meta.user_id(),
            action: "delete_user",
            resource_type: "user",
            resource_id: Some(&id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        tracing::warn!("记录操作日志失败: {}", e);
    }
    tracing::info!("用户删除成功, ID: {}", id);

    Ok(crate::error::ok_json((), "用户删除成功"))
}
