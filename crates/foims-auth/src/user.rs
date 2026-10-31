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
use foims_models::{User, UserCreate, UserUpdate, is_valid_phone};

/// 管理员删除路径的固定咨询锁键（"FOIMS" 魔数 + 序号 2 的 int64 编码，
/// 仅作为串行化标记，与其他锁键不冲突即可）
const ADMIN_DELETE_ADVISORY_LOCK_KEY: i64 = 0x464F_494D_5300_0002;

/// 手机号格式校验（非空时执行，空串视为未填写；口径同 employees 侧）
fn validate_phone(phone: &Option<String>) -> Result<(), AppError> {
    if let Some(phone) = phone.as_deref().map(str::trim).filter(|s| !s.is_empty())
        && !is_valid_phone(phone)
    {
        return Err(AppError::Validation(msg(
            "server.user.validation.phone_invalid",
        )));
    }
    Ok(())
}

/// 手机号规范化：trim 后空串转 NULL（选填字段不落空串）
fn normalize_phone(phone: Option<String>) -> Option<String> {
    phone
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
}

/// GET /api/users：用户列表（登录即可读：只读系统模块的普通用户/审计员需
/// 展示账户信息；增删改由账户管理员端点约束；search 模糊匹配用户名/邮箱，
/// sort_by/sort_order 走白名单，未知列回落 created_at DESC）。
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
        ("enabled", "desc") => "ORDER BY enabled DESC, username ASC",
        ("enabled", _) => "ORDER BY enabled ASC, username ASC",
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

        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id, username, email, phone, role, enabled, password_expiry_days, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users",
        );
        // 排序段为白名单常量，经 push 拼接
        qb.push(" ").push(order_clause);
        qb.push(" LIMIT ").push_bind(page_size);
        qb.push(" OFFSET ").push_bind(offset);
        let users = qb
            .build_query_as::<User>()
            .persistent(false)
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

        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id, username, email, phone, role, enabled, password_expiry_days, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE username ILIKE ",
        );
        qb.push_bind(&search_pattern)
            .push(" OR email ILIKE ")
            .push_bind(&search_pattern);
        // 排序段为白名单常量，经 push 拼接
        qb.push(" ").push(order_clause);
        qb.push(" LIMIT ").push_bind(page_size);
        qb.push(" OFFSET ").push_bind(offset);
        let users = qb
            .build_query_as::<User>()
            .persistent(false)
            .fetch_all(&conn)
            .await?;

        (total, users)
    };

    Ok(foims_common::ok_json(
        paged_response(users, total, &pagination),
        "server.user.list_retrieved",
    ))
}

/// POST /api/users：创建用户（账户管理员 admin/sysadmin/secadmin 可用，否则
/// 403；用户名或邮箱重复转 409，并发撞唯一约束同样 409；仅超管可授予 admin
/// 角色，否则 403；密码过等保复杂度校验，用户写入与密码历史同事务）。
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

    // 手机号：空串规范化为 NULL，非空时校验字符白名单
    let phone = normalize_phone(req.phone.clone());
    validate_phone(&phone)?;

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
        "INSERT INTO users (id, username, password_hash, email, phone, role, enabled, password_expiry_days, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(&req.username)
    .bind(&hashed_password)
    .bind(&req.email)
    .bind(&phone)
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

    let details = json!({"username": req.username, "email": req.email, "phone": phone, "role": req.role});
    log_op_best_effort(&conn, &meta, "create_user", "user", Some(&id), &details).await;
    log_info!("log.user.created", username = req.username, id = id);

    let user = User {
        id,
        username: req.username.clone(),
        email: req.email.clone(),
        phone,
        role: req.role.clone(),
        enabled: true,
        password_expiry_days: req.password_expiry_days.unwrap_or(0),
        two_factor_enabled: false,
        two_factor_verified: false,
        created_at: now,
        updated_at: now,
    };

    Ok(foims_common::ok_json(user, "server.user.created"))
}

/// GET /api/users/{id}：用户详情（登录即可读；不存在转 404）。
pub async fn get_user<P: AuthProvider>(
    _user: crate::extractor::AuthUser,
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, email, phone, role, enabled, password_expiry_days, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&conn)
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.user.not_found")))?;

    Ok(foims_common::ok_json(user, "server.user.retrieved"))
}

/// PUT /api/users/{id}：更新用户（账户管理员；目标不存在 404；目标为超管
/// 而操作者非超管、或非超管授予 admin 角色转 403；自改角色、重新启用已过期
/// 账户转 409；邮箱撞唯一约束转 409；角色、禁用或重置密码会吊销历史令牌）。
pub async fn update_user<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    account_admin: crate::extractor::AccountAdminUser,
    Path(id): Path<Uuid>,
    AppJson(req): AppJson<UserUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 手机号三态：字段缺失不修改 / null 清空 / 有值更新；空串规范化为 NULL
    let phone_set = req.phone.is_some();
    let phone = normalize_phone(req.phone.clone().flatten());
    validate_phone(&phone)?;

    let conn = state.pool()?.get_conn();

    // 提权防护与启用防护判定需要目标当前角色/状态/有效期
    let (current_role, current_enabled, current_expiry_days, password_changed_at): (
        String,
        bool,
        i32,
        chrono::DateTime<Utc>,
    ) = sqlx::query_as(
        "SELECT role, enabled, password_expiry_days, password_changed_at FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&conn)
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.user.not_found")))?;

    // 重新启用防护：仅把到期被禁用户翻回启用而不重置密码或调整有效期时，
    // password_changed_at 仍早于到期点，下轮定时扫描或下次登录会立即再次
    // 禁用（静默回滚）。此处显式 409 引导管理员先重置密码或调整有效期；
    // 同请求重置密码视为已处理过期问题，不再拦截
    let effective_expiry_days = req.password_expiry_days.unwrap_or(current_expiry_days);
    if req.password.is_none()
        && req.enabled == Some(true)
        && !current_enabled
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

    // 管理员重置密码：写入前过等保策略（复杂度 + 与当前密码及历史不重复）
    // 并预哈希；新密码生效即过期作废，同请求重新启用不再触发上方 409
    let new_password_hash = match req.password.as_deref() {
        Some(pw) => {
            crate::password_policy::validate_password(&conn, id, pw).await?;
            Some(hash_password(pw).await?)
        }
        None => None,
    };

    // 仅在实际降权/提权、实际禁用或重置密码时吊销历史令牌（强制重新登录）。
    // 吊销条件必须对比现值：编辑表单总是提交 role，若以"参数是否提供"
    // 判定会每次编辑都吊销——管理员编辑自己的邮箱/密码有效期时会把
    // 自己踢下线，后续请求 401 且 refresh 同样被拒，表现为强制跳登录页
    let role_changed = req.role.as_deref().is_some_and(|r| r != current_role);
    let disabling = req.enabled == Some(false) && current_enabled;
    let password_reset = new_password_hash.is_some();

    // 权限/启用状态/有效期/密码单语句完成：状态或角色变更时同语句吊销
    // 历史令牌（强制重新登录）；拆成两条语句时第二条失败会导致降权已
    // 生效但旧令牌未被强制下线（D-2）。
    // 用户信息更新与密码历史记录同事务：保证「新密码生效 ⇔ 历史已记录」，
    // 失败时整体回滚，避免出现未入历史的密码变更弱化重复使用检查
    let mut tx = conn.begin().await?;
    sqlx::query(
        "UPDATE users SET
         email = COALESCE($1, email),
         role = COALESCE($2, role),
         enabled = COALESCE($3, enabled),
         password_expiry_days = COALESCE($4, password_expiry_days),
         phone = CASE WHEN $5::BOOLEAN THEN $6 ELSE phone END,
         password_hash = COALESCE($7, password_hash),
         password_changed_at = COALESCE($8, password_changed_at),
         tokens_invalidated_at = CASE WHEN $9::BOOLEAN THEN NOW() ELSE tokens_invalidated_at END,
         updated_at = $10
         WHERE id = $11",
    )
    .bind(&req.email)
    .bind(&req.role)
    .bind(req.enabled)
    .bind(req.password_expiry_days)
    .bind(phone_set)
    .bind(&phone)
    .bind(&new_password_hash)
    .bind(password_reset.then_some(now))
    .bind(role_changed || disabling || password_reset)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // 并发同邮箱的 TOCTOU 兜底：预检（create_user 同款）不可覆盖编辑路径
        // 撞 users.email 唯一约束（23505），映射为 409 而非 500
        if let Some(db_err) = e.as_database_error()
            && db_err.is_unique_violation()
        {
            return AppError::Conflict(msg("server.user.email_exists"));
        }
        AppError::from(e)
    })?;

    if let Some(hashed) = &new_password_hash {
        crate::password_policy::record_history(&conn, &mut tx, id, hashed)
            .await
            .map_err(AppError::from)?;
    }

    tx.commit().await?;

    let details = json!({"email": req.email, "phone": phone, "role": req.role, "enabled": req.enabled, "password_expiry_days": req.password_expiry_days, "password_reset": req.password.is_some()});
    log_op_best_effort(&conn, &meta, "update_user", "user", Some(&id), &details).await;
    log_info!("log.user.updated", id = id);

    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, email, phone, role, enabled, password_expiry_days, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE id = $1"
    )
    .bind(id)
    .fetch_one(&conn)
    .await?;

    Ok(foims_common::ok_json(user, "server.user.updated"))
}

/// DELETE /api/users/{id}：删除用户（账户管理员可用；自删转 409；目标不存在
/// 404；超管账户仅超管可删，否则 403；admin/secadmin 末位保护，删除后管理员
/// 归零转 409；咨询锁串行化「检查+删除」防并发双删）。
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
