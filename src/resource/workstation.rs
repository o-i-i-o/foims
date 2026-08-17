//! 工位（workstations）资源管理。
//!
//! 工位是房间内的人员/终端位置，可绑定设备与 IP。列表过滤使用
//! sqlx `QueryBuilder` 动态拼接（关键字经 `escape_like` 转义、排序走
//! 白名单），行映射统一为 `query_as` + `FromRow` 强类型结构。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use sqlx::{PgExecutor, Postgres, QueryBuilder};
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    IpManager, Workstation, WorkstationCreate, WorkstationUpdate, WorkstationWithDetails,
};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::{Pagination, paged_response};

/// 工位基础查询列（含房间名联表），列表与单条查询共用。
const WORKSTATION_COLUMNS: &str = "w.id, w.name, w.room_id,
        r.name as room_name,
        w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ";

/// 追加工位列表过滤条件（关键字 + 机房），供 COUNT 与数据查询共用。
fn push_workstation_filters(
    builder: &mut QueryBuilder<Postgres>,
    search_pattern: Option<&str>,
    room_id: Option<Uuid>,
) {
    let mut first = true;
    if let Some(pattern) = search_pattern {
        builder
            .push(" WHERE (w.name ILIKE ")
            .push_bind(pattern)
            .push(" OR w.manager ILIKE ")
            .push_bind(pattern)
            .push(" OR w.description ILIKE ")
            .push_bind(pattern)
            .push(")");
        first = false;
    }
    if let Some(room_id) = room_id {
        builder
            .push(if first { " WHERE " } else { " AND " })
            .push("w.room_id = ")
            .push_bind(room_id);
    }
}

/// 分页获取工位列表（支持关键字、机房过滤与白名单排序）。
pub async fn get_workstations(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let search = query.get("search").cloned().unwrap_or_default();
    let room_id = query.get("room_id").cloned();
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());

    let search_pattern = (!search.is_empty()).then(|| crate::utils::escape_like(&search));
    let parsed_room_id = room_id.as_ref().and_then(|id| Uuid::parse_str(id).ok());

    // ORDER BY 白名单，未匹配时回落默认序，避免注入
    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY w.name DESC",
        ("room_name", "desc") => "ORDER BY room_name DESC, w.name ASC",
        ("room_name", _) => "ORDER BY room_name ASC, w.name ASC",
        ("manager", "desc") => "ORDER BY w.manager DESC, w.name ASC",
        ("manager", _) => "ORDER BY w.manager ASC, w.name ASC",
        ("created_at", "desc") => "ORDER BY w.created_at DESC",
        ("created_at", _) => "ORDER BY w.created_at ASC",
        _ => "ORDER BY w.name ASC",
    };

    let mut count_builder = QueryBuilder::<Postgres>::new("SELECT COUNT(*) FROM workstations w");
    let mut data_builder = QueryBuilder::<Postgres>::new(format!(
        "SELECT {WORKSTATION_COLUMNS}
        FROM workstations w
        LEFT JOIN rooms r ON w.room_id = r.id"
    ));

    push_workstation_filters(
        &mut count_builder,
        search_pattern.as_deref(),
        parsed_room_id,
    );
    push_workstation_filters(&mut data_builder, search_pattern.as_deref(), parsed_room_id);

    let total: i64 = count_builder
        .build_query_scalar()
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    data_builder
        .push(" ")
        .push(order_clause)
        .push(" LIMIT ")
        .push_bind(pagination.page_size)
        .push(" OFFSET ")
        .push_bind(pagination.offset);
    let items = data_builder
        .build_query_as::<WorkstationWithDetails>()
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    Ok(crate::error::ok_json(
        paged_response(items, total, &pagination),
        "工位获取成功",
    ))
}

/// 创建工位（同房间内名称唯一，检查与写入在同一事务内）。
pub async fn create_workstation(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<WorkstationCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_workstation: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM workstations WHERE name = $1 AND room_id = $2",
    )
    .bind(&req.name)
    .bind(req.room_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing_workstation.is_some() {
        return Err(AppError::Conflict("工位名称已存在".to_string()));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO workstations (id, name, room_id, manager, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(req.room_id)
    .bind(&req.manager)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let workstation = Workstation {
        id,
        name: req.name.clone(),
        room_id: req.room_id,
        room_name: None,
        manager: req.manager.clone(),
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": workstation.name,
        "room_id": workstation.room_id,
        "manager": workstation.manager,
        "description": workstation.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "workstation",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(workstation, "工位创建成功"))
}

/// 查询工位基础信息（含房间名联表）。
async fn fetch_workstation_base(
    executor: impl PgExecutor<'_>,
    id: Uuid,
) -> Result<Option<WorkstationWithDetails>, sqlx::Error> {
    sqlx::query_as::<_, WorkstationWithDetails>(sqlx::AssertSqlSafe(format!(
        "SELECT {WORKSTATION_COLUMNS}
        FROM workstations w
        LEFT JOIN rooms r ON w.room_id = r.id
        WHERE w.id = $1"
    )))
    .bind(id)
    .fetch_optional(executor)
    .await
}

/// 查询工位上设备绑定的 IP 明细（含网段/区域联表）。
async fn fetch_workstation_ips(
    executor: impl PgExecutor<'_>,
    id: Uuid,
) -> Result<Vec<IpManager>, sqlx::Error> {
    sqlx::query_as(
        r"SELECT
            m.id, m.device_interface_id, di.device_id, m.network_id,
            nc.network_region_id AS network_region_id,
            nc.name AS network_name,
            nr.name AS network_region,
            host(m.ip_address) as ip_address,
            m.ip_version, di.mac_address AS mac_address, m.description,
            m.status, m.last_seen, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ
        FROM ips m
        JOIN device_interfaces di ON m.device_interface_id = di.id
        JOIN devices d ON di.device_id = d.id
        LEFT JOIN network_cidrs nc ON m.network_id = nc.id
        LEFT JOIN network_regions nr ON nc.network_region_id = nr.id
        WHERE d.workstation_id = $1
        ORDER BY m.ip_address",
    )
    .bind(id)
    .fetch_all(executor)
    .await
}

/// 获取工位详情（含设备 IP 明细）。
pub async fn get_workstation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();
    let mut workstation = fetch_workstation_base(&conn, id)
        .await?
        .ok_or_else(|| AppError::NotFound("工位未找到".to_string()))?;
    workstation.ips = fetch_workstation_ips(&conn, id).await?;

    Ok(crate::error::ok_json(workstation, "工位获取成功"))
}

/// 更新工位（字段缺失表示不修改，`Option` 绑定经 COALESCE 保留旧值）。
pub async fn update_workstation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<WorkstationUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workstations WHERE id = $1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if !exists {
        return Err(AppError::NotFound("工位未找到".to_string()));
    }

    sqlx::query(
        "UPDATE workstations SET
         name = COALESCE($1, name),
         room_id = COALESCE($2, room_id),
         manager = COALESCE($3, manager),
         description = COALESCE($4, description),
         updated_at = $5
         WHERE id = $6",
    )
    .bind(&req.name)
    .bind(req.room_id)
    .bind(&req.manager)
    .bind(&req.description)
    .bind(Utc::now())
    .bind(id)
    .execute(&mut *tx)
    .await?;

    let mut result = fetch_workstation_base(&mut *tx, id)
        .await?
        .ok_or_else(|| AppError::Internal("工位更新后查询详情失败".to_string()))?;
    result.ips = fetch_workstation_ips(&mut *tx, id).await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "name": result.name,
        "room_id": result.room_id,
        "manager": result.manager,
        "description": result.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "workstation",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(result, "工位更新成功"))
}

/// 删除工位（连同布局数据一并清理，同一事务内完成）。
pub async fn delete_workstation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let mut tx = state.pool()?.get_conn().begin().await?;

    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workstations WHERE id = $1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if !exists {
        return Err(AppError::NotFound("工位未找到".to_string()));
    }

    sqlx::query("DELETE FROM workstation_layouts WHERE workstation_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM workstations WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "workstation_id": id.to_string()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "workstation",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json((), "工位删除成功"))
}
