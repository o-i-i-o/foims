//! 机柜资源管理。

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;

use chrono::Utc;
use ipma_auth::meta::{RequestMeta, log_op_best_effort};
use ipma_common::AppError;
use ipma_common::AppJson;
use ipma_common::DbProvider;
use ipma_common::msg;
use ipma_common::pagination::{Pagination, paged_response};
use ipma_models::{
    Cabinet, CabinetCreate, CabinetPositionsSync, CabinetUpdate, CabinetWithNetworks, NetworkInfo,
    PatchPanelBrief, PositionBrief, PositionSyncItem,
};
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

pub async fn get_cabinets<P: DbProvider>(
    State(state): State<Arc<P>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page_size = pagination.page_size;
    let offset = pagination.offset;
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

    let search_pattern = ipma_common::net::escape_like(&search);
    // 非法 UUID 显式 422（与 network.rs 口径一致），不静默退化为全量列表
    let parsed_room_id = room_id
        .as_ref()
        .map(|id| {
            Uuid::parse_str(id).map_err(|_| {
                AppError::Validation(msg("server.common.invalid_param").with("param", "room_id"))
            })
        })
        .transpose()?;

    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY c.name DESC",
        ("room_name", "desc") => "ORDER BY rm.name DESC NULLS LAST, c.name ASC",
        ("room_name", _) => "ORDER BY rm.name ASC NULLS LAST, c.name ASC",
        ("capacity", "desc") => "ORDER BY c.capacity DESC, c.name ASC",
        ("capacity", _) => "ORDER BY c.capacity ASC, c.name ASC",
        ("created_at", "desc") => "ORDER BY c.created_at DESC",
        ("created_at", _) => "ORDER BY c.created_at ASC",
        _ => "ORDER BY c.name ASC",
    };

    let (total, cabinets) = if search.is_empty() && parsed_room_id.is_none() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cabinets c")
            .fetch_one(&state.pool()?.get_conn())
            .await?;

        let cabinets = sqlx::query_as::<_, Cabinet>(
            sqlx::AssertSqlSafe(format!("SELECT c.id, c.name, c.room_id, c.capacity, c.description, c.created_at::TIMESTAMPTZ, c.updated_at::TIMESTAMPTZ FROM cabinets c LEFT JOIN rooms rm ON c.room_id = rm.id {order_clause} LIMIT $1 OFFSET $2"))
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, cabinets)
    } else if parsed_room_id.is_some() && search.is_empty() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cabinets c WHERE c.room_id = $1")
            .bind(parsed_room_id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

        let cabinets = sqlx::query_as::<_, Cabinet>(
            sqlx::AssertSqlSafe(format!("SELECT c.id, c.name, c.room_id, c.capacity, c.description, c.created_at::TIMESTAMPTZ, c.updated_at::TIMESTAMPTZ FROM cabinets c LEFT JOIN rooms rm ON c.room_id = rm.id WHERE c.room_id = $1 {order_clause} LIMIT $2 OFFSET $3"))
        )
        .bind(parsed_room_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, cabinets)
    } else if parsed_room_id.is_some() {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cabinets c WHERE c.room_id = $1 AND (c.name ILIKE $2 OR c.description ILIKE $2)"
        )
        .bind(parsed_room_id)
        .bind(&search_pattern)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

        let cabinets = sqlx::query_as::<_, Cabinet>(
            sqlx::AssertSqlSafe(format!("SELECT c.id, c.name, c.room_id, c.capacity, c.description, c.created_at::TIMESTAMPTZ, c.updated_at::TIMESTAMPTZ FROM cabinets c LEFT JOIN rooms rm ON c.room_id = rm.id WHERE c.room_id = $1 AND (c.name ILIKE $2 OR c.description ILIKE $2) {order_clause} LIMIT $3 OFFSET $4"))
        )
        .bind(parsed_room_id)
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, cabinets)
    } else {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cabinets c WHERE c.name ILIKE $1 OR c.description ILIKE $1",
        )
        .bind(&search_pattern)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

        let cabinets = sqlx::query_as::<_, Cabinet>(
            sqlx::AssertSqlSafe(format!("SELECT c.id, c.name, c.room_id, c.capacity, c.description, c.created_at::TIMESTAMPTZ, c.updated_at::TIMESTAMPTZ FROM cabinets c LEFT JOIN rooms rm ON c.room_id = rm.id WHERE c.name ILIKE $1 OR c.description ILIKE $1 {order_clause} LIMIT $2 OFFSET $3"))
        )
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, cabinets)
    };

    let mut cabinets_with_networks = Vec::new();

    for cabinet in cabinets {
        let position_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM positions WHERE cabinet_id = $1")
                .bind(cabinet.id)
                .fetch_one(&state.pool()?.get_conn())
                .await?;

        let room_name: Option<String> = sqlx::query_scalar("SELECT name FROM rooms WHERE id = $1")
            .bind(cabinet.room_id)
            .fetch_optional(&state.pool()?.get_conn())
            .await?;

        let cabinet_with_networks = CabinetWithNetworks {
            id: cabinet.id,
            name: cabinet.name,
            room_id: cabinet.room_id,
            room_name,
            capacity: cabinet.capacity,
            position_count,
            positions: None,
            patch_panels: None,
            description: cabinet.description,
            created_at: cabinet.created_at,
            updated_at: cabinet.updated_at,
        };

        cabinets_with_networks.push(cabinet_with_networks);
    }

    Ok(ipma_common::ok_json(
        paged_response(cabinets_with_networks, total, &pagination),
        "server.cabinet.fetched",
    ))
}

pub async fn get_cabinets_by_network_region<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(region_id_str): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Response, AppError> {
    let Ok(region_id) = Uuid::parse_str(&region_id_str) else {
        return Err(AppError::Validation(msg(
            "server.network.region_id_invalid",
        )));
    };

    let network_id_filter = query
        .get("network_id")
        .and_then(|s| Uuid::parse_str(s).ok());

    let cabinets = if let Some(network_id) = network_id_filter {
        // 归属校验：提供的网段必须属于路径中的区域，防止跨区域越权枚举
        let network_in_region: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM network_cidrs WHERE id = $1 AND network_region_id = $2)",
        )
        .bind(network_id)
        .bind(region_id)
        .fetch_one(&state.pool()?.get_conn())
        .await?;
        if !network_in_region {
            return Err(AppError::Validation(
                msg("server.common.invalid_param").with("param", "network_id"),
            ));
        }
        sqlx::query_as::<_, Cabinet>(
            r"SELECT DISTINCT c.id, c.name, c.room_id, c.capacity, c.description, c.created_at, c.updated_at
               FROM cabinets c
               LEFT JOIN rooms r ON c.room_id = r.id
               LEFT JOIN room_networks rn ON r.id = rn.room_id
               WHERE rn.network_id = $1
               ORDER BY c.name"
        )
        .bind(network_id)
        .fetch_all(&state.pool()?.get_conn())
        .await?
    } else {
        sqlx::query_as::<_, Cabinet>(
            r"SELECT DISTINCT c.id, c.name, c.room_id, c.capacity, c.description, c.created_at, c.updated_at
               FROM cabinets c
               LEFT JOIN rooms r ON c.room_id = r.id
               LEFT JOIN room_networks rn ON r.id = rn.room_id
               LEFT JOIN network_cidrs nc ON rn.network_id = nc.id
               WHERE nc.network_region_id = $1
               ORDER BY c.name"
        )
        .bind(region_id)
        .fetch_all(&state.pool()?.get_conn())
        .await?
    };

    Ok(ipma_common::ok_json(cabinets, "server.cabinet.fetched"))
}

pub async fn create_cabinet<P: DbProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<CabinetCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 重名预检、引用存在性校验与写入放同一事务，避免 TOCTOU 与残缺写入
    let mut tx = state.pool()?.get_conn().begin().await?;

    // room_id 引用存在性校验（非法引用返回校验错误而非 FK 500 兜底）
    let room_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rooms WHERE id = $1)")
        .bind(req.room_id)
        .fetch_one(&mut *tx)
        .await?;
    if !room_exists {
        return Err(AppError::Validation(msg("server.room.not_found")));
    }

    let existing_cabinet =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE name = $1 AND room_id = $2")
            .bind(&req.name)
            .bind(req.room_id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing_cabinet.is_some() {
        return Err(AppError::Conflict(msg("server.cabinet.name_exists")));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO cabinets (id, name, room_id, capacity, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(req.room_id)
    .bind(req.capacity)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // 并发写入竞态兜底：uq_cabinets_room_name 冲突映射为 409
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict(msg("server.cabinet.name_exists"));
        }
        AppError::from(e)
    })?;

    tx.commit().await?;

    let cabinet = Cabinet {
        id,
        name: req.name.clone(),
        room_id: req.room_id,
        capacity: req.capacity,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": cabinet.name,
        "room_id": cabinet.room_id,
        "capacity": cabinet.capacity,
        "description": cabinet.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "cabinet",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json(cabinet, "server.cabinet.created"))
}

pub async fn get_cabinet<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let cabinet = sqlx::query_as::<_, Cabinet>(
        "SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets WHERE id = $1"
    ).bind(id)
    .fetch_optional(&state.pool()?.get_conn()).await?
    .ok_or_else(|| AppError::NotFound(msg("server.cabinet.not_found")))?;

    let position_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM positions WHERE cabinet_id = $1")
            .bind(cabinet.id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    let room_name: Option<String> = sqlx::query_scalar("SELECT name FROM rooms WHERE id = $1")
        .bind(cabinet.room_id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

    let pos_rows = sqlx::query(
        "SELECT id, name, start_u, end_u, description FROM positions WHERE cabinet_id = $1 ORDER BY start_u, name",
    )
    .bind(cabinet.id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;
    let positions: Vec<PositionBrief> = pos_rows
        .iter()
        .map(|r| PositionBrief {
            id: r.get("id"),
            name: r.get("name"),
            start_u: r.get("start_u"),
            end_u: r.get("end_u"),
            description: r.get("description"),
        })
        .collect();

    // 加载该机柜下的配线架（独立表，隶属机柜）
    let pp_rows =
        sqlx::query("SELECT id, name FROM patch_panels WHERE cabinet_id = $1 ORDER BY name")
            .bind(cabinet.id)
            .fetch_all(&state.pool()?.get_conn())
            .await?;
    let patch_panels: Vec<PatchPanelBrief> = pp_rows
        .iter()
        .map(|r| PatchPanelBrief {
            id: r.get("id"),
            name: r.get("name"),
        })
        .collect();

    let cabinet_with_networks = CabinetWithNetworks {
        id: cabinet.id,
        name: cabinet.name,
        room_id: cabinet.room_id,
        room_name,
        capacity: cabinet.capacity,
        position_count,
        positions: Some(positions),
        patch_panels: Some(patch_panels),
        description: cabinet.description,
        created_at: cabinet.created_at,
        updated_at: cabinet.updated_at,
    };

    Ok(ipma_common::ok_json(
        cabinet_with_networks,
        "server.cabinet.fetched",
    ))
}

pub async fn update_cabinet<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<CabinetUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 预检（存在性/重名/引用）与写入放同一事务，避免 TOCTOU
    let mut tx = state.pool()?.get_conn().begin().await?;

    let current_room_id: Uuid = sqlx::query_scalar("SELECT room_id FROM cabinets WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(msg("server.cabinet.not_found")))?;

    // room_id 引用存在性校验（缺省沿用现值）
    if let Some(room_id) = req.room_id {
        let room_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rooms WHERE id = $1)")
                .bind(room_id)
                .fetch_one(&mut *tx)
                .await?;
        if !room_exists {
            return Err(AppError::Validation(msg("server.room.not_found")));
        }
    }
    let effective_room_id = req.room_id.unwrap_or(current_room_id);

    // 重名预检：同房间内（UNIQUE(room_id, name)）名称冲突
    if let Some(name) = &req.name {
        let duplicate: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM cabinets WHERE name = $1 AND room_id = $2 AND id != $3",
        )
        .bind(name)
        .bind(effective_room_id)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        if duplicate.is_some() {
            return Err(AppError::Conflict(msg("server.cabinet.name_exists")));
        }
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE cabinets SET
         name = COALESCE($1, name),
         room_id = COALESCE($2, room_id),
         capacity = COALESCE($3, capacity),
         description = COALESCE($4, description),
         updated_at = $5
         WHERE id = $6",
    )
    .bind(&req.name)
    .bind(req.room_id)
    .bind(req.capacity)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // 并发写入竞态兜底：uq_cabinets_room_name 冲突映射为 409
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict(msg("server.cabinet.name_exists"));
        }
        AppError::from(e)
    })?;

    tx.commit().await?;

    let cabinet = sqlx::query_as::<_, Cabinet>(
        "SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets WHERE id = $1"
    ).bind(id)
    .fetch_one(&state.pool()?.get_conn()).await?;

    let details = serde_json::json!({
        "name": cabinet.name,
        "room_id": cabinet.room_id,
        "capacity": cabinet.capacity,
        "description": cabinet.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "cabinet",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json(cabinet, "server.cabinet.updated"))
}

pub async fn delete_cabinet<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    // 计数检查与 DELETE 放同一事务，避免检查后被并发写入绕过、
    // 触发 CASCADE 静默清空设备关联
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_cabinet = sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

    if existing_cabinet.is_none() {
        return Err(AppError::NotFound(msg("server.cabinet.not_found")));
    }

    let position_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM positions WHERE cabinet_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    if position_count > 0 {
        return Err(AppError::Validation(msg("server.cabinet.has_positions")));
    }

    // 双保险：机位上仍挂着设备时拒绝删除（positions 级联删除会把
    // devices.position_id 置空，静默清空设备归属）
    let device_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM devices d \
         JOIN positions p ON d.position_id = p.id \
         WHERE p.cabinet_id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if device_count > 0 {
        return Err(AppError::Validation(msg("server.cabinet.position_in_use")));
    }

    // 配线架被线路引用时禁止删除（未引用的配线架随外键级联删除）
    let linked_pp_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM patch_panels pp \
         WHERE pp.cabinet_id = $1 \
         AND EXISTS ( \
             SELECT 1 FROM cable_links cl \
             WHERE (cl.a_endpoint_type = 'patch_panel' AND cl.a_endpoint_id = pp.id) \
                OR (cl.b_endpoint_type = 'patch_panel' AND cl.b_endpoint_id = pp.id) \
         )",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if linked_pp_count > 0 {
        return Err(AppError::Validation(msg(
            "server.cabinet.patch_panel_linked",
        )));
    }

    sqlx::query("DELETE FROM cabinets WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "cabinet_id": id.to_string()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "cabinet",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json((), "server.cabinet.deleted"))
}

pub async fn get_cabinet_networks<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let existing_cabinet = sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

    if existing_cabinet.is_none() {
        return Err(AppError::NotFound(msg("server.cabinet.not_found")));
    }

    let cabinet_networks = sqlx::query_as::<_, NetworkInfo>(
        r"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr 
           FROM rooms r 
           JOIN room_networks rn ON r.id = rn.room_id
           JOIN network_cidrs n ON rn.network_id = n.id 
           JOIN network_regions nr ON n.network_region_id = nr.id 
           WHERE r.id = (SELECT room_id FROM cabinets WHERE id = $1) 
           -- 房型口径与 sync_room_children 对齐：OTHER 房型的机柜同样参与机柜网络（R7）
           AND r.room_type IN ('DATA_CENTER', 'TELECOM_CLOSET', 'OTHER')",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(ipma_common::ok_json(
        cabinet_networks,
        "server.cabinet.networks_fetched",
    ))
}

pub async fn sync_cabinet_positions<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<CabinetPositionsSync>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 机柜存在性检查放进同步事务内，避免事务外的快照读与后续写入
    // 之间出现 TOCTOU（与 sync_room_children 同口径）
    let mut tx = state.pool()?.get_conn().begin().await?;

    let cabinet_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cabinets WHERE id = $1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if !cabinet_exists {
        return Err(AppError::NotFound(msg("server.cabinet.not_found")));
    }

    let now = Utc::now();

    let items: &Vec<PositionSyncItem> = &req.positions;
    let existing_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM positions WHERE cabinet_id = $1")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;

    let request_ids: Vec<Uuid> = items.iter().filter_map(|i| i.id).collect();

    for existing_id in &existing_ids {
        if !request_ids.contains(existing_id) {
            let device_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE position_id = $1")
                    .bind(existing_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if device_count > 0 {
                return Err(AppError::Validation(msg("server.cabinet.position_in_use")));
            }
            sqlx::query("DELETE FROM positions WHERE id = $1")
                .bind(existing_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    for item in items {
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM positions WHERE name = $1 AND cabinet_id = $2 AND ($3::uuid IS NULL OR id != $3)",
        )
        .bind(&item.name)
        .bind(id)
        .bind(item.id)
        .fetch_optional(&mut *tx)
        .await?;
        if existing.is_some() {
            return Err(AppError::Conflict(msg(
                "server.cabinet.position_name_exists",
            )));
        }

        if let Some(item_id) = item.id {
            // 归属校验：仅允许更新当前机柜下的机位，
            // 携带其他机柜的 id 时按未找到处理，避免跨父资源静默搬移
            let updated = sqlx::query(
                "UPDATE positions SET name = $1, start_u = $2, end_u = $3, description = $4, cabinet_id = $5, updated_at = $6 WHERE id = $7 AND cabinet_id = $8",
            )
            .bind(&item.name)
            .bind(item.start_u)
            .bind(item.end_u)
            .bind(&item.description)
            .bind(id)
            .bind(now)
            .bind(item_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() == 0 {
                return Err(AppError::NotFound(msg("server.cabinet.not_found")));
            }
        } else {
            let new_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(new_id)
            .bind(&item.name)
            .bind(id)
            .bind(item.start_u)
            .bind(item.end_u)
            .bind(&item.description)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    let details = serde_json::json!({
        "cabinet_id": id.to_string(),
        "position_count": items.len()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "sync_positions",
        "cabinet",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json((), "server.cabinet.positions_synced"))
}
