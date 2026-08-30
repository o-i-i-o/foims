//! 用户管理 CRUD。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;
use validator::Validate;

use crate::meta::{RequestMeta, log_op_best_effort};
use crate::provider::AuthProvider;
use crate::utils::hash_password;
use foims_common::AppJson;
use foims_common::log_info;
use foims_common::pagination::{Pagination, paged_response};
use foims_common::{AppError, msg};
use foims_models::{User, UserCreate, UserUpdate};

/// 管理员删除路径的固定咨询锁键（"FOIMS" 魔数 + 序号 2 的 int64 编码，
/// 仅作为串行化标记，与其他锁键不冲突即可）
const ADMIN_DELETE_ADVISORY_LOCK_KEY: i64 = 0x464F_494D_5300_0002;

pub async fn get_users<P: AuthProvider>(
    _secadmin: crate::extractor::SecAdminUser,
    State(state): State<Arc<P>>,
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

    Ok(foims_common::ok_json(
        paged_response(users, total, &pagination),
        "server.user.list_retrieved",
    ))
}

pub async fn create_user<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    _secadmin: crate::extractor::SecAdminUser,
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
    crate::password_policy::validate_complexity(&state.pool()?.get_conn(), &req.password).await?;

    let hashed_password = hash_password(&req.password).await?;

    let id = Uuid::new_v4();
    let now = Utc::now();

    let insert_result = sqlx::query(
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
    .await;

    if let Err(e) = insert_result {
        // 并发同名/同邮箱的 TOCTOU 兜底：预检通过后仍可能撞唯一约束
        //（23505），映射为 409 Conflict 而非 500
        if let Some(db_err) = e.as_database_error()
            && db_err.is_unique_violation()
        {
            let conflict_msg = if db_err
                .constraint()
                .map(|name| name.contains("email"))
                .unwrap_or(false)
            {
                msg("server.user.email_exists")
            } else {
                msg("server.user.username_exists")
            };
            return Err(AppError::Conflict(conflict_msg));
        }
        return Err(e.into());
    }

    // 等保密码策略：记录密码历史（供后续改密时的重复使用检查）
    crate::password_policy::record_history(&conn, id, &hashed_password).await;

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

    Ok(foims_common::ok_json(user, "server.user.created"))
}

pub async fn get_user<P: AuthProvider>(
    _secadmin: crate::extractor::SecAdminUser,
    State(state): State<Arc<P>>,
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

    Ok(foims_common::ok_json(user, "server.user.retrieved"))
}

pub async fn update_user<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    _secadmin: crate::extractor::SecAdminUser,
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

    Ok(foims_common::ok_json(user, "server.user.updated"))
}

pub async fn delete_user<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    secadmin: crate::extractor::SecAdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    // 禁止删除当前登录账户：误操作自删会立即把自己登出且无法恢复
    if secadmin.sub == id.to_string() {
        return Err(AppError::Conflict(msg("server.user.cannot_delete_self")));
    }

    let existing_user = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&conn)
        .await?;

    if existing_user.is_none() {
        return Err(AppError::NotFound(msg("server.user.not_found")));
    }

    let mut tx = conn.begin().await?;

    // 管理员删除路径串行化：先取固定键的事务级咨询锁。仅靠目标行的
    // FOR UPDATE 无法保护 COUNT——普通 MVCC 快照读看不到并发事务未提交
    // 的删除，两个 secadmin 并发各删一个 admin 时双方计数均为 1，最终
    // 管理员归零。咨询锁使并发的「检查+删除」整体串行执行
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ADMIN_DELETE_ADVISORY_LOCK_KEY)
        .execute(&mut *tx)
        .await?;

    // 末位管理员保护：目标为 admin/secadmin 且删除后管理员数量归零时拒绝。
    // 统计与删除置于同一事务（统计行加锁语义由删除目标的 FOR UPDATE 保证），
    // 防止并发删除两个管理员时双双通过检查
    let (target_role,): (String,) =
        sqlx::query_as("SELECT role FROM users WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    if target_role == "admin" || target_role == "secadmin" {
        let other_admins: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE role IN ('admin', 'secadmin') AND id <> $1",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if other_admins == 0 {
            return Err(AppError::Conflict(msg("server.user.last_admin_protected")));
        }
    }

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

    Ok(foims_common::ok_json((), "server.user.deleted"))
}
