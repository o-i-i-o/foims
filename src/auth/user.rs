use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{ApiResponse, User, UserCreate, UserUpdate};
use crate::utils::{DEFAULT_PAGE, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, web};
use bcrypt::{DEFAULT_COST, hash};
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

pub async fn get_users(
    state: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let page: i64 = query
        .get("page")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PAGE);
    let page_size: i64 = query
        .get("page_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let search = query.get("search").cloned().unwrap_or_default();
    let offset = (page - 1) * page_size;

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

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": users,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "用户获取成功",
    )))
}

pub async fn create_user(
    state: web::Data<AppState>,
    http_req: HttpRequest,
    req: web::Json<UserCreate>,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    let conn = state.pool()?.get_conn();

    let existing_user =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE username = $1")
            .bind(&req.username)
            .fetch_optional(&conn)
            .await?;

    if existing_user.is_some() {
        return Err(AppError::Conflict("用户名已存在".to_string()));
    }

    let existing_email =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE email = $1")
            .bind(&req.email)
            .fetch_optional(&conn)
            .await?;

    if existing_email.is_some() {
        return Err(AppError::Conflict("邮箱已存在".to_string()));
    }

    let hashed_password = hash(&req.password, DEFAULT_COST)
        .map_err(|err| AppError::Internal(format!("密码哈希错误: {err}")))?;

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
        &http_req,
        &state.config,
        "create_user",
        "user",
        &id,
        &details,
        true,
    )
    .await
    {
        tracing::warn!("记录操作日志失败: {}", e);
    }

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

    Ok(HttpResponse::Ok().json(ApiResponse::<User>::success(user, "用户创建成功")))
}

pub async fn get_user(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    let conn = state.pool()?.get_conn();

    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, email, role, status, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&conn)
    .await?
    .ok_or_else(|| AppError::NotFound("用户未找到".to_string()))?;

    Ok(HttpResponse::Ok().json(ApiResponse::<User>::success(user, "用户获取成功")))
}

pub async fn update_user(
    state: web::Data<AppState>,
    http_req: HttpRequest,
    id_path: web::Path<Uuid>,
    req: web::Json<UserUpdate>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    (*req).validate()?;

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
        &http_req,
        &state.config,
        "update_user",
        "user",
        &id,
        &details,
        true,
    )
    .await
    {
        tracing::warn!("记录操作日志失败: {}", e);
    }

    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, email, role, status, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE id = $1"
    )
    .bind(id)
    .fetch_one(&conn)
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::<User>::success(user, "用户更新成功")))
}

pub async fn delete_user(
    state: web::Data<AppState>,
    http_req: HttpRequest,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    let conn = state.pool()?.get_conn();

    let existing_user = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&conn)
        .await?;

    if existing_user.is_none() {
        return Err(AppError::NotFound("用户未找到".to_string()));
    }

    let mut tx = conn.begin().await?;

    sqlx::query("DELETE FROM operation_logs WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM notifications WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = json!({});
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        &http_req,
        &state.config,
        "delete_user",
        "user",
        &id,
        &details,
        true,
    )
    .await
    {
        tracing::warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "用户删除成功")))
}
