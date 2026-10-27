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

use foims_auth::meta::{RequestMeta, log_op_best_effort};
use foims_common::AppJson;
use foims_common::DbProvider;
use foims_common::pagination::{Pagination, paged_response};
use foims_common::{AppError, msg};
use foims_models::{
    IpDetail, Workstation, WorkstationCreate, WorkstationUpdate, WorkstationWithDetails,
};

/// 工位基础查询列（含房间名与管理人姓名联表），列表与单条查询共用。
const WORKSTATION_COLUMNS: &str = "w.id, w.name, w.room_id,
        r.name as room_name,
        e.name as manager_name, w.manager_employee_id, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ";

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
            .push(" OR e.name ILIKE ")
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
pub async fn get_workstations<P: DbProvider>(
    State(state): State<Arc<P>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let search = query.get("search").cloned().unwrap_or_default();
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());

    let search_pattern = (!search.is_empty()).then(|| foims_common::net::escape_like(&search));
    // 过滤参数非法 UUID 显式 422（与 options.rs/patch_panel.rs 口径一致），
    // 不再静默忽略退化为全量列表；空串视为未提供
    let parsed_room_id = crate::helpers::parse_optional_uuid(&query, "room_id")?;

    // ORDER BY 白名单，未匹配时回落默认序，避免注入
    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY w.name DESC",
        ("room_name", "desc") => "ORDER BY room_name DESC, w.name ASC",
        ("room_name", _) => "ORDER BY room_name ASC, w.name ASC",
        // 管理人可空：未指定的工位无论升降序都排在末尾
        ("manager_name", "desc") => "ORDER BY manager_name DESC NULLS LAST, w.name ASC",
        ("manager_name", _) => "ORDER BY manager_name ASC NULLS LAST, w.name ASC",
        ("created_at", "desc") => "ORDER BY w.created_at DESC",
        ("created_at", _) => "ORDER BY w.created_at ASC",
        _ => "ORDER BY w.name ASC",
    };

    let mut count_builder = QueryBuilder::<Postgres>::new(
        "SELECT COUNT(*) FROM workstations w
        LEFT JOIN employees e ON w.manager_employee_id = e.id",
    );
    let mut data_builder = QueryBuilder::<Postgres>::new(format!(
        "SELECT {WORKSTATION_COLUMNS}
        FROM workstations w
        JOIN rooms r ON w.room_id = r.id
        LEFT JOIN employees e ON w.manager_employee_id = e.id"
    ));

    push_workstation_filters(
        &mut count_builder,
        search_pattern.as_deref(),
        parsed_room_id,
    );
    push_workstation_filters(&mut data_builder, search_pattern.as_deref(), parsed_room_id);

    let total: i64 = count_builder
        .build_query_scalar()
        .persistent(false)
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
        .persistent(false)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    Ok(foims_common::ok_json(
        paged_response(items, total, &pagination),
        "server.workstation.list_retrieved",
    ))
}

/// 创建工位（同房间内名称唯一，检查与写入在同一事务内）。
pub async fn create_workstation<P: DbProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<WorkstationCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    // 房间空间属性（组织 + 房型）与管理人员工同组织校验：
    // 工位只能放在办公类空间；管理人可空（未指定责任人的工位不参与校验），
    // 一旦指定则必须属于房间所属组织
    let space = crate::helpers::load_room_space(&mut *tx, req.room_id).await?;
    if !crate::helpers::room_allows_workstation(&space.room_type) {
        return Err(AppError::Validation(msg(
            "server.room.workstation_not_allowed",
        )));
    }
    crate::helpers::ensure_employee_in_org(
        &mut *tx,
        req.manager_employee_id,
        space.org_id,
        "server.workstation.manager_employee_not_found",
        "server.workstation.manager_org_mismatch",
    )
    .await?;

    crate::helpers::ensure_unique_name(
        &mut *tx,
        "workstations",
        Some("room_id"),
        Some(req.room_id),
        &req.name,
        None,
        "server.workstation.name_exists",
    )
    .await?;

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO workstations (id, name, room_id, manager_employee_id, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(req.room_id)
    .bind(req.manager_employee_id)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // 并发写入竞态兜底：uq_workstations_room_name 冲突映射为 409
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict(msg("server.workstation.name_exists"));
        }
        AppError::from(e)
    })?;

    tx.commit().await?;

    let workstation = Workstation {
        id,
        name: req.name.clone(),
        room_id: req.room_id,
        room_name: None,
        manager_employee_id: req.manager_employee_id,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": workstation.name,
        "room_id": workstation.room_id,
        "manager_employee_id": workstation.manager_employee_id,
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

    Ok(foims_common::ok_json(
        workstation,
        "server.workstation.created",
    ))
}

/// 查询工位基础信息（含房间名联表）。
async fn fetch_workstation_base(
    executor: impl PgExecutor<'_>,
    id: Uuid,
) -> Result<Option<WorkstationWithDetails>, sqlx::Error> {
    // WORKSTATION_COLUMNS 为服务端常量列清单，id 经 push_bind 传参
    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(format!(
        "SELECT {WORKSTATION_COLUMNS}
        FROM workstations w
        JOIN rooms r ON w.room_id = r.id
        LEFT JOIN employees e ON w.manager_employee_id = e.id
        WHERE w.id = "
    ));
    qb.push_bind(id);
    qb.build_query_as::<WorkstationWithDetails>()
        .fetch_optional(executor)
        .await
}

/// 查询工位上设备绑定的 IP 明细（含网段/区域联表）。
async fn fetch_workstation_ips(
    executor: impl PgExecutor<'_>,
    id: Uuid,
) -> Result<Vec<IpDetail>, sqlx::Error> {
    sqlx::query_as(
        r"SELECT
            m.id, m.device_interface_id, di.device_id, m.subnet_id,
            nc.network_region_id AS network_region_id,
            nc.name AS network_name,
            nr.name AS network_region,
            host(m.ip_address) as ip_address,
            m.ip_version, di.mac_address AS mac_address,
            m.status, m.last_seen, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ
        FROM interface_ips m
        JOIN device_interfaces di ON m.device_interface_id = di.id
        JOIN devices d ON di.device_id = d.id
        LEFT JOIN network_cidrs nc ON m.subnet_id = nc.id
        LEFT JOIN network_regions nr ON nc.network_region_id = nr.id
        WHERE d.workstation_id = $1
        ORDER BY m.ip_address",
    )
    .bind(id)
    .fetch_all(executor)
    .await
}

/// 获取工位详情（含设备 IP 明细）。
pub async fn get_workstation<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();
    let mut workstation = fetch_workstation_base(&conn, id)
        .await?
        .ok_or_else(|| AppError::NotFound(msg("server.workstation.not_found")))?;
    workstation.ips = fetch_workstation_ips(&conn, id).await?;

    Ok(foims_common::ok_json(
        workstation,
        "server.workstation.fetched",
    ))
}

/// 更新工位（名称/房间/管理人缺省保留旧值，管理人可显式清空；
/// 房间变更时同事务清理工位布局，布局坐标随房间失效）。
pub async fn update_workstation<P: DbProvider>(
    State(state): State<Arc<P>>,
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
        return Err(AppError::NotFound(msg("server.workstation.not_found")));
    }

    // 旧房间 id：room_id 变更时用于清理工位布局（布局坐标随房间失效）
    let old_room_id: Uuid = sqlx::query_scalar("SELECT room_id FROM workstations WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;

    // 空间属性（缺省沿用现值）与管理人同组织校验：换房间时房型必须
    // 容纳工位；管理人三态（缺省沿用现值 / null 清空 / 值换人）中的有效值
    // 必须属于最终生效房间的组织
    let effective_room_id = req.room_id.unwrap_or(old_room_id);
    let space = crate::helpers::load_room_space(&mut *tx, effective_room_id).await?;
    if !crate::helpers::room_allows_workstation(&space.room_type) {
        return Err(AppError::Validation(msg(
            "server.room.workstation_not_allowed",
        )));
    }
    let effective_manager_id: Option<Uuid> = match req.manager_employee_id {
        Some(manager_employee_id) => manager_employee_id,
        None => {
            sqlx::query_scalar("SELECT manager_employee_id FROM workstations WHERE id = $1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?
        }
    };
    crate::helpers::ensure_employee_in_org(
        &mut *tx,
        effective_manager_id,
        space.org_id,
        "server.workstation.manager_employee_not_found",
        "server.workstation.manager_org_mismatch",
    )
    .await?;

    // 重名预检：以最终生效房间为口径（UNIQUE(room_id, name)）
    if let Some(name) = &req.name {
        let effective_room_id: Uuid = req.room_id.unwrap_or(old_room_id);
        crate::helpers::ensure_unique_name(
            &mut *tx,
            "workstations",
            Some("room_id"),
            Some(effective_room_id),
            name,
            Some(id),
            "server.workstation.name_exists",
        )
        .await?;
    }

    // 三态更新（QueryBuilder 动态拼接，None 不进 SET）：
    // - name/room_id：单层 Option，缺省保留旧值
    // - manager_employee_id：双层 Option，Some(None) SET NULL（清空管理人）、
    //   Some(Some(id)) 换人、缺省保留旧值
    // - description：双层 Option，Some(None) SET NULL、Some(Some(v)) SET v
    let mut builder = QueryBuilder::new("UPDATE workstations SET updated_at = ");
    builder.push_bind(Utc::now());
    if let Some(name) = &req.name {
        builder.push(", name = ").push_bind(name);
    }
    if let Some(room_id) = req.room_id {
        builder.push(", room_id = ").push_bind(room_id);
    }
    if let Some(manager_employee_id) = req.manager_employee_id {
        builder
            .push(", manager_employee_id = ")
            .push_bind(manager_employee_id);
    }
    if let Some(description) = &req.description {
        builder.push(", description = ").push_bind(description);
    }
    builder.push(" WHERE id = ").push_bind(id);

    builder
        .build()
        .persistent(false)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            // 并发写入竞态兜底：uq_workstations_room_name 冲突映射为 409
            if let sqlx::Error::Database(ref db_err) = e
                && db_err.is_unique_violation()
            {
                return AppError::Conflict(msg("server.workstation.name_exists"));
            }
            AppError::from(e)
        })?;

    // 房间变更时布局坐标随房间失效：同一事务内清理工位布局；
    // 工位上的设备跟随换房（设备位置不变量 I1）：父侧 workstations 已落库，
    // 设备行的 BEFORE 触发器回查的就是新房间；同时清理不再归属新房间的 IP
    if req.room_id.is_some_and(|room_id| room_id != old_room_id) {
        sqlx::query("DELETE FROM workstation_layouts WHERE workstation_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        crate::helpers::sync_devices_room(
            &mut tx,
            crate::helpers::DeviceScope::Workstation(id),
            effective_room_id,
        )
        .await?;
    }

    let mut result = fetch_workstation_base(&mut *tx, id)
        .await?
        .ok_or_else(|| AppError::Internal(msg("server.workstation.detail_query_failed")))?;
    result.ips = fetch_workstation_ips(&mut *tx, id).await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "name": result.name,
        "room_id": result.room_id,
        "manager_employee_id": result.manager_employee_id,
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

    Ok(foims_common::ok_json(result, "server.workstation.updated"))
}

/// 删除工位（连同布局数据一并清理，同一事务内完成）。
pub async fn delete_workstation<P: DbProvider>(
    State(state): State<Arc<P>>,
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
        return Err(AppError::NotFound(msg("server.workstation.not_found")));
    }

    // 设备仍占用该工位时拒绝删除：FK ON DELETE SET NULL 会静默清空
    // 设备归属，与同步路径（sync_workstation_children）的删除保护口径一致
    let device_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE workstation_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if device_count > 0 {
        return Err(AppError::Validation(msg(
            "server.workstation.in_use_by_device",
        )));
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

    Ok(foims_common::ok_json((), "server.workstation.deleted"))
}
