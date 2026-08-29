//! 员工（employees）资源管理。
//!
//! 员工挂在组织节点下（组织模板不变），供工位管理人下拉选择与
//! 设备 IP 分配后的邮件通知使用。列表过滤使用 sqlx `QueryBuilder`
//! 动态拼接（关键字经 `escape_like` 转义）。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use ipma_common::{AppError, msg};
use ipma_models::{Employee, EmployeeCreate, EmployeeUpdate, is_valid_phone};

/// 员工基础查询列（含组织名联表）
const EMPLOYEE_COLUMNS: &str = "e.id, e.org_id,
        o.name as org_name,
        e.name, e.gender, e.phone, e.email, e.hire_date,
        e.created_at::TIMESTAMPTZ, e.updated_at::TIMESTAMPTZ";

/// 手机号格式校验（非空时执行，空串视为清空）
fn validate_phone(phone: &Option<String>) -> Result<(), AppError> {
    if let Some(phone) = phone.as_deref().map(str::trim).filter(|s| !s.is_empty())
        && !is_valid_phone(phone)
    {
        return Err(AppError::Validation(msg(
            "server.employee.validation.phone_invalid",
        )));
    }
    Ok(())
}

/// 空串转 None（可空文本字段统一处理）
fn blank_to_none(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// 获取员工列表（可选 org_id / search 过滤，登录即可读，供下拉与模态框使用）。
pub async fn get_employees(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let org_id = query.get("org_id").cloned();
    let search = query.get("search").cloned().unwrap_or_default();
    let parsed_org_id = org_id.as_ref().and_then(|id| Uuid::parse_str(id).ok());
    // org_id 参数给了但解析失败时直接返回校验错误（避免误当全量查询）
    if org_id.is_some() && parsed_org_id.is_none() {
        return Err(AppError::Validation(msg(
            "server.common.validation.uuid_invalid",
        )));
    }

    let search_pattern = (!search.is_empty()).then(|| crate::utils::escape_like(&search));

    let mut builder = QueryBuilder::<Postgres>::new(format!(
        "SELECT {EMPLOYEE_COLUMNS}
        FROM employees e
        LEFT JOIN organizations o ON e.org_id = o.id"
    ));

    let mut first = true;
    if let Some(pattern) = search_pattern.as_deref() {
        builder
            .push(" WHERE (e.name ILIKE ")
            .push_bind(pattern)
            .push(" OR e.phone ILIKE ")
            .push_bind(pattern)
            .push(" OR e.email ILIKE ")
            .push_bind(pattern)
            .push(")");
        first = false;
    }
    if let Some(org_id) = parsed_org_id {
        builder
            .push(if first { " WHERE " } else { " AND " })
            .push("e.org_id = ")
            .push_bind(org_id);
    }

    builder.push(" ORDER BY e.name ASC LIMIT 1000");
    let items = builder
        .build_query_as::<Employee>()
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    Ok(ipma_common::ok_json(
        items,
        "server.employee.list_retrieved",
    ))
}

/// 查询单个员工。
pub async fn get_employee(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let employee = sqlx::query_as::<_, Employee>(sqlx::AssertSqlSafe(format!(
        "SELECT {EMPLOYEE_COLUMNS}
        FROM employees e
        LEFT JOIN organizations o ON e.org_id = o.id
        WHERE e.id = $1"
    )))
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.employee.not_found")))?;

    Ok(ipma_common::ok_json(employee, "server.employee.fetched"))
}

/// 创建员工（同组织内名称唯一，检查与写入在同一事务内）。
pub async fn create_employee(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<EmployeeCreate>,
) -> Result<Response, AppError> {
    req.validate()?;
    let phone = blank_to_none(req.phone.clone());
    let email = blank_to_none(req.email.clone());
    validate_phone(&phone)?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let org_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organizations WHERE id = $1)")
            .bind(req.org_id)
            .fetch_one(&mut *tx)
            .await?;
    if !org_exists {
        return Err(AppError::NotFound(msg("server.organization.not_found")));
    }

    let name_conflict: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM employees WHERE name = $1 AND org_id = $2")
            .bind(&req.name)
            .bind(req.org_id)
            .fetch_optional(&mut *tx)
            .await?;
    if name_conflict.is_some() {
        return Err(AppError::Conflict(msg("server.employee.name_exists")));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO employees (id, org_id, name, gender, phone, email, hire_date, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(req.org_id)
    .bind(&req.name)
    .bind(req.normalized_gender())
    .bind(&phone)
    .bind(&email)
    .bind(req.hire_date)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "name": req.name,
        "org_id": req.org_id.to_string(),
        "email": email,
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "employee",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json((), "server.employee.created"))
}

/// 更新员工（字段缺失表示不修改，`Option` 绑定经 COALESCE 保留旧值；
/// 空串语义为清空对应字段）。
pub async fn update_employee(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<EmployeeUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;
    let phone = blank_to_none(req.phone.clone());
    let email = blank_to_none(req.email.clone());
    validate_phone(&phone)?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM employees WHERE id = $1)")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Err(AppError::NotFound(msg("server.employee.not_found")));
    }

    // 名称唯一性：同组织下重名（排除自身）时拒绝
    if let Some(name) = &req.name {
        let conflict: Option<Uuid> = sqlx::query_scalar(
            "SELECT e.id FROM employees e
             WHERE e.name = $1 AND e.org_id = (SELECT org_id FROM employees WHERE id = $2)
               AND e.id <> $2",
        )
        .bind(name)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        if conflict.is_some() {
            return Err(AppError::Conflict(msg("server.employee.name_exists")));
        }
    }

    // 更新语义：name/gender 缺省（null）保留旧值；phone/email/hire_date
    // 以提交值覆盖（前端未填写的可空字段提交空串，规范化为 NULL 即清空）
    sqlx::query(
        "UPDATE employees SET
         name = COALESCE($1, name),
         gender = COALESCE($2, gender),
         phone = $3,
         email = $4,
         hire_date = $5,
         updated_at = $6
         WHERE id = $7",
    )
    .bind(&req.name)
    .bind(req.normalized_gender())
    .bind(&phone)
    .bind(&email)
    .bind(req.hire_date)
    .bind(Utc::now())
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "name": req.name,
        "email": email,
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "employee",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json((), "server.employee.updated"))
}

/// 删除员工（工位上的 manager_employee_id 因 ON DELETE SET NULL 自动解绑）。
pub async fn delete_employee(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let result = sqlx::query("DELETE FROM employees WHERE id = $1")
        .bind(id)
        .execute(&state.pool()?.get_conn())
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(msg("server.employee.not_found")));
    }

    let details = serde_json::json!({ "employee_id": id.to_string() });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "employee",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json((), "server.employee.deleted"))
}
