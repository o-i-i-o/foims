//! 用户管理 CRUD。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;
use validator::Validate;

use crate::jwt::hash_password;
use crate::meta::{RequestMeta, log_op_best_effort};
use crate::provider::AuthProvider;
use foims_common::AppJson;
use foims_common::log_info;
use foims_common::pagination::{Pagination, paged_response};
use foims_common::{AppError, msg};
use foims_models::{User, UserCreate, UserUpdate};

/// 管理员删除路径的固定咨询锁键（"FOIMS" 魔数 + 序号 2 的 int64 编码，
/// 仅作为串行化标记，与其他锁键不冲突即可）
const ADMIN_DELETE_ADVISORY_LOCK_KEY: i64 = 0x464F_494D_5300_0002;

/// 用户列表（登录即可读：只读系统模块的普通用户/审计员需展示账户信息；
/// 增删改由下方账户管理员端点约束）
pub async fn get_users<P: AuthProvider>(
    _user: crate::extractor::AuthUser,
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

    let search_pattern = foims_common::net::escape_like(&search);
    let conn = state.pool()?.get_conn();

    let (total, users) = if search.is_empty() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&conn)
            .await?;

        let users = sqlx::query_as::<_, User>(sqlx::AssertSqlSafe(format!(
            "SELECT id, username, email, role, status, password_expiry_days, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users {order_clause} LIMIT $1 OFFSET $2"
        )))
        .persistent(false)
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
            "SELECT id, username, email, role, status, password_expiry_days, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE username ILIKE $1 OR email ILIKE $1 {order_clause} LIMIT $2 OFFSET $3"
        )))
        .persistent(false)
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
    account_admin: crate::extractor::AccountAdminUser,
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

    // 提权防护：仅超管可授予 admin 角色（sysadmin/secadmin 共管其余账户）
    if req.role == "admin" && account_admin.role != "admin" {
        return Err(AppError::Forbidden(msg(
            "server.user.admin_role_admin_only",
        )));
    }

    // 等保密码策略：复杂度校验（新用户无历史记录可查）
    crate::password_policy::validate_complexity(&state.pool()?.get_conn(), &req.password).await?;

    let hashed_password = hash_password(&req.password).await?;

    let id = Uuid::new_v4();
    let now = Utc::now();

    // 用户写入与密码历史同事务：避免「用户已建但历史缺失」弱化重复使用检查
    let mut tx = conn.begin().await?;

    let insert_result = sqlx::query(
        "INSERT INTO users (id, username, password_hash, email, role, status, password_expiry_days, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(&req.username)
    .bind(&hashed_password)
    .bind(&req.email)
    .bind(&req.role)
    .bind(true)
    .bind(req.password_expiry_days.unwrap_or(0))
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
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
    crate::password_policy::record_history(
        &state.pool()?.get_conn(),
        &mut tx,
        id,
        &hashed_password,
    )
    .await
    .map_err(AppError::from)?;

    tx.commit().await?;

    let details = json!({"username": req.username, "email": req.email, "role": req.role});
    log_op_best_effort(&conn, &meta, "create_user", "user", Some(&id), &details).await;
    log_info!("log.user.created", username = req.username, id = id);

    let user = User {
        id,
        username: req.username.clone(),
        email: req.email.clone(),
        role: req.role.clone(),
        status: true,
        password_expiry_days: req.password_expiry_days.unwrap_or(0),
        two_factor_enabled: false,
        two_factor_verified: false,
        created_at: now,
        updated_at: now,
    };

    Ok(foims_common::ok_json(user, "server.user.created"))
}

pub async fn get_user<P: AuthProvider>(
    _user: crate::extractor::AuthUser,
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, email, role, status, password_expiry_days, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE id = $1"
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
    account_admin: crate::extractor::AccountAdminUser,
    Path(id): Path<Uuid>,
    AppJson(req): AppJson<UserUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let conn = state.pool()?.get_conn();

    // 提权防护与启用防护判定需要目标当前角色/状态/有效期
    let (current_role, current_status, current_expiry_days, password_changed_at): (
        String,
        bool,
        i32,
        chrono::DateTime<Utc>,
    ) = sqlx::query_as(
        "SELECT role, status, password_expiry_days, password_changed_at FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&conn)
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.user.not_found")))?;

    // 重新启用防护：仅把到期被禁用户翻回启用而不重置密码或调整有效期时，
    // password_changed_at 仍早于到期点，下轮定时扫描或下次登录会立即再次
    // 禁用（静默回滚）。此处显式 409 引导管理员先重置密码或调整有效期
    let effective_expiry_days = req.password_expiry_days.unwrap_or(current_expiry_days);
    if req.status == Some(true)
        && !current_status
        && effective_expiry_days > 0
        && Utc::now() > password_changed_at + chrono::Duration::days(effective_expiry_days as i64)
    {
        return Err(AppError::Conflict(msg(
            "server.user.reenable_password_expired",
        )));
    }

    // 禁止自改角色：编辑表单总是提交 role，仅在实际变更时拒绝
    if account_admin.sub == id.to_string() && req.role.as_deref().is_some_and(|r| r != current_role)
    {
        return Err(AppError::Conflict(msg(
            "server.user.self_role_change_forbidden",
        )));
    }

    // 超管账户仅超管可管理（改邮箱/禁用/降权等一律拒绝）
    if current_role == "admin" && account_admin.role != "admin" {
        return Err(AppError::Forbidden(msg(
            "server.user.admin_account_admin_only",
        )));
    }

    // 仅超管可授予 admin 角色（防 sysadmin/secadmin 自我或他人提权）
    if req.role.as_deref() == Some("admin") && account_admin.role != "admin" {
        return Err(AppError::Forbidden(msg(
            "server.user.admin_role_admin_only",
        )));
    }

    let now = Utc::now();

    // 仅在实际降权/提权或实际禁用时吊销历史令牌（强制重新登录）。
    // 吊销条件必须对比现值：编辑表单总是提交 role，若以"参数是否提供"
    // 判定会每次编辑都吊销——管理员编辑自己的邮箱/密码有效期时会把
    // 自己踢下线，后续请求 401 且 refresh 同样被拒，表现为强制跳登录页
    let role_changed = req.role.as_deref().is_some_and(|r| r != current_role);
    let disabling = req.status == Some(false) && current_status;

    // 权限/启用状态/有效期变更单语句完成：状态或角色变更时同语句吊销
    // 历史令牌（强制重新登录）；拆成两条语句时第二条失败会导致降权已
    // 生效但旧令牌未被强制下线（D-2）
    sqlx::query(
        "UPDATE users SET
         email = COALESCE($1, email),
         role = COALESCE($2, role),
         status = COALESCE($3, status),
         password_expiry_days = COALESCE($6, password_expiry_days),
         tokens_invalidated_at = CASE WHEN $7::BOOLEAN THEN NOW() ELSE tokens_invalidated_at END,
         updated_at = $4
         WHERE id = $5",
    )
    .bind(&req.email)
    .bind(&req.role)
    .bind(req.status)
    .bind(now)
    .bind(id)
    .bind(req.password_expiry_days)
    .bind(role_changed || disabling)
    .execute(&conn)
    .await?;

    let details = json!({"email": req.email, "role": req.role, "status": req.status, "password_expiry_days": req.password_expiry_days});
    log_op_best_effort(&conn, &meta, "update_user", "user", Some(&id), &details).await;
    log_info!("log.user.updated", id = id);

    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, email, role, status, password_expiry_days, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE id = $1"
    )
    .bind(id)
    .fetch_one(&conn)
    .await?;

    Ok(foims_common::ok_json(user, "server.user.updated"))
}

pub async fn delete_user<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    account_admin: crate::extractor::AccountAdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    // 禁止删除当前登录账户：误操作自删会立即把自己登出且无法恢复
    if account_admin.sub == id.to_string() {
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
        // 超管账户仅超管可删除
        if target_role == "admin" && account_admin.role != "admin" {
            return Err(AppError::Forbidden(msg(
                "server.user.admin_account_admin_only",
            )));
        }

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
