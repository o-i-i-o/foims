//! 机柜机位（positions）资源管理。
//!
//! 机位是机柜内的 U 位区间，可被设备占用并绑定 IP。列表过滤使用
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
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::{Pagination, paged_response};
use ipma_common::{AppError, msg};
use ipma_models::{
    CabinetPosition, CabinetPositionCreate, CabinetPositionUpdate, CabinetPositionWithDetails,
    IpManager,
};

/// 追加机位列表过滤条件（关键字 + 机柜 + 机房），供 COUNT 与数据查询共用。
fn push_position_filters(
    builder: &mut QueryBuilder<Postgres>,
    search_pattern: Option<&str>,
    cabinet_id: Option<Uuid>,
    room_id: Option<Uuid>,
) {
    let mut first = true;
    if let Some(pattern) = search_pattern {
        builder
            .push(" WHERE (p.name ILIKE ")
            .push_bind(pattern)
            .push(" OR p.description ILIKE ")
            .push_bind(pattern)
            .push(")");
        first = false;
    }
    if let Some(cabinet_id) = cabinet_id {
        builder
            .push(if first { " WHERE " } else { " AND " })
            .push("p.cabinet_id = ")
            .push_bind(cabinet_id);
        first = false;
    }
    if let Some(room_id) = room_id {
        builder
            .push(if first { " WHERE " } else { " AND " })
            .push("c.room_id = ")
            .push_bind(room_id);
    }
}

/// 分页获取机位列表（支持关键字、机柜、机房过滤与白名单排序）。
pub async fn get_positions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let search = query.get("search").cloned().unwrap_or_default();
    let cabinet_id = query
        .get("cabinet_id")
        .and_then(|id| Uuid::parse_str(id).ok());
    let room_id = query.get("room_id").and_then(|id| Uuid::parse_str(id).ok());
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());

    // ORDER BY 白名单，未匹配时回落默认序，避免注入
    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY name DESC",
        ("cabinet_name", "desc") => "ORDER BY cabinet_name DESC, name ASC",
        ("cabinet_name", _) => "ORDER BY cabinet_name ASC, name ASC",
        ("start_u", "desc") => "ORDER BY start_u DESC, name ASC",
        ("start_u", _) => "ORDER BY start_u ASC, name ASC",
        ("created_at", "desc") => "ORDER BY created_at DESC",
        ("created_at", _) => "ORDER BY created_at ASC",
        _ => "ORDER BY name ASC",
    };

    let search_pattern = (!search.is_empty()).then(|| crate::utils::escape_like(&search));

    let mut count_builder = QueryBuilder::<Postgres>::new(
        "SELECT COUNT(*) FROM positions p LEFT JOIN cabinets c ON p.cabinet_id = c.id",
    );
    push_position_filters(
        &mut count_builder,
        search_pattern.as_deref(),
        cabinet_id,
        room_id,
    );
    let total: i64 = count_builder
        .build_query_scalar()
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let mut data_builder = QueryBuilder::<Postgres>::new(
        r"SELECT p.id, p.name, p.cabinet_id,
                  c.name as cabinet_name, c.room_id, r.name as room_name,
                  p.start_u, p.end_u, p.description,
                  p.created_at::TIMESTAMPTZ as created_at, p.updated_at::TIMESTAMPTZ as updated_at
           FROM positions p
           LEFT JOIN cabinets c ON p.cabinet_id = c.id
           LEFT JOIN rooms r ON c.room_id = r.id",
    );
    push_position_filters(
        &mut data_builder,
        search_pattern.as_deref(),
        cabinet_id,
        room_id,
    );
    data_builder
        .push(" ")
        .push(order_clause)
        .push(" LIMIT ")
        .push_bind(pagination.page_size)
        .push(" OFFSET ")
        .push_bind(pagination.offset);
    let items = data_builder
        .build_query_as::<CabinetPositionWithDetails>()
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    Ok(ipma_common::ok_json(
        paged_response(items, total, &pagination),
        "server.position.list_retrieved",
    ))
}

/// 创建机位（同机柜内名称唯一，检查与写入在同一事务内）。
pub async fn create_cabinet_position(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<CabinetPositionCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 机位必须隶属机柜：cabinet_id 可空时既不受 UNIQUE(cabinet_id,name) 保护，
    // 也不受 U 位重叠触发器保护（db-schema-review R8，已实测确认）
    let Some(cabinet_id) = req.cabinet_id else {
        return Err(AppError::Validation(msg(
            "server.position.cabinet_required",
        )));
    };

    let mut tx = state.pool()?.get_conn().begin().await?;

    let cabinet_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cabinets WHERE id = $1)")
            .bind(cabinet_id)
            .fetch_one(&mut *tx)
            .await?;
    if !cabinet_exists {
        return Err(AppError::NotFound(msg("server.cabinet.not_found")));
    }

    let existing_position: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM positions WHERE name = $1 AND cabinet_id = $2",
    )
    .bind(&req.name)
    .bind(req.cabinet_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing_position.is_some() {
        return Err(AppError::Conflict(msg("server.position.name_exists")));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(req.cabinet_id)
    .bind(req.start_u)
    .bind(req.end_u)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let position = CabinetPosition {
        id,
        name: req.name.clone(),
        cabinet_id: req.cabinet_id,
        start_u: req.start_u,
        end_u: req.end_u,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": position.name,
        "cabinet_id": position.cabinet_id,
        "start_u": position.start_u,
        "end_u": position.end_u,
        "description": position.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "cabinet_position",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json(position, "server.position.created"))
}

/// 查询机位基础信息（含机柜/机房名称联表）。
async fn fetch_position_base(
    executor: impl PgExecutor<'_>,
    id: Uuid,
) -> Result<Option<CabinetPositionWithDetails>, sqlx::Error> {
    sqlx::query_as::<_, CabinetPositionWithDetails>(
        r"SELECT p.id, p.name, p.cabinet_id,
                  c.name as cabinet_name, c.room_id, r.name as room_name,
                  p.start_u, p.end_u, p.description,
                  p.created_at::TIMESTAMPTZ, p.updated_at::TIMESTAMPTZ
           FROM positions p
           LEFT JOIN cabinets c ON p.cabinet_id = c.id
           LEFT JOIN rooms r ON c.room_id = r.id
           WHERE p.id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
}

/// 查询占用机位的设备所绑定的 IP 明细。
async fn fetch_position_ips(
    executor: impl PgExecutor<'_>,
    id: Uuid,
) -> Result<Vec<IpManager>, sqlx::Error> {
    sqlx::query_as(
        r"SELECT m.id, m.device_interface_id, di.device_id, m.network_id,
           nc.network_region_id AS network_region_id,
           nc.name AS network_name, nr.name AS network_region,
           host(m.ip_address) as ip_address, m.ip_version, di.mac_address AS mac_address, m.description,
           m.status, m.last_seen, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ
           FROM ips m
           JOIN device_interfaces di ON m.device_interface_id = di.id
           JOIN devices d ON di.device_id = d.id
           LEFT JOIN network_cidrs nc ON m.network_id = nc.id
           LEFT JOIN network_regions nr ON nc.network_region_id = nr.id
           WHERE d.position_id = $1
           ORDER BY m.ip_address",
    )
    .bind(id)
    .fetch_all(executor)
    .await
}

/// 获取机位详情（含占用设备的 IP 明细）。
pub async fn get_cabinet_position(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();
    let mut position = fetch_position_base(&conn, id)
        .await?
        .ok_or_else(|| AppError::NotFound(msg("server.position.not_found")))?;
    position.ips = fetch_position_ips(&conn, id).await?;

    Ok(ipma_common::ok_json(position, "server.position.fetched"))
}

/// 更新机位（字段缺失表示不修改，`Option` 绑定经 COALESCE 保留旧值）。
pub async fn update_cabinet_position(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<CabinetPositionUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let position_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM positions WHERE id = $1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if !position_exists {
        return Err(AppError::NotFound(msg("server.position.not_found")));
    }

    sqlx::query(
        "UPDATE positions SET
         name = COALESCE($1, name),
         cabinet_id = COALESCE($2, cabinet_id),
         start_u = COALESCE($3, start_u),
         end_u = COALESCE($4, end_u),
         description = COALESCE($5, description),
         updated_at = $6
         WHERE id = $7",
    )
    .bind(&req.name)
    .bind(req.cabinet_id)
    .bind(req.start_u)
    .bind(req.end_u)
    .bind(&req.description)
    .bind(Utc::now())
    .bind(id)
    .execute(&mut *tx)
    .await?;

    let mut result = fetch_position_base(&mut *tx, id)
        .await?
        .ok_or_else(|| AppError::Internal(msg("server.position.detail_query_failed")))?;
    result.ips = fetch_position_ips(&mut *tx, id).await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "name": result.name,
        "cabinet_id": result.cabinet_id,
        "start_u": result.start_u,
        "end_u": result.end_u,
        "description": result.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "cabinet_position",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json(result, "server.position.updated"))
}

/// 删除机位（被设备占用时拒绝删除）。
pub async fn delete_cabinet_position(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_position: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM positions WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if existing_position.is_none() {
        return Err(AppError::NotFound(msg("server.position.not_found")));
    }

    let device_using_position: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM devices WHERE position_id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if device_using_position.is_some() {
        return Err(AppError::Validation(msg(
            "server.position.occupied_by_device",
        )));
    }

    sqlx::query("DELETE FROM positions WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "position_id": id.to_string()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "cabinet_position",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json((), "server.position.deleted"))
}
