use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    Cabinet, CabinetCreate, CabinetPatchPanelsSync, CabinetPositionsSync, CabinetUpdate,
    CabinetWithNetworks, NetOutletBrief, NetworkInfo, PositionBrief, PositionSyncItem,
};
use crate::routes::static_files::AppJson;
use crate::utils::common::{log_op_best_effort, RequestMeta};
use crate::utils::pagination::Pagination;
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

pub async fn get_cabinets(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
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

    let search_pattern = crate::utils::escape_like(&search);
    let parsed_room_id = room_id.as_ref().and_then(|id| Uuid::parse_str(id).ok());

    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY c.name DESC",
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
            sqlx::AssertSqlSafe(format!("SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets c {order_clause} LIMIT $1 OFFSET $2"))
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
            sqlx::AssertSqlSafe(format!("SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets c WHERE c.room_id = $1 {order_clause} LIMIT $2 OFFSET $3"))
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
            sqlx::AssertSqlSafe(format!("SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets c WHERE c.room_id = $1 AND (c.name ILIKE $2 OR c.description ILIKE $2) {order_clause} LIMIT $3 OFFSET $4"))
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
            sqlx::AssertSqlSafe(format!("SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets c WHERE c.name ILIKE $1 OR c.description ILIKE $1 {order_clause} LIMIT $2 OFFSET $3"))
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

    Ok(crate::error::ok_json(
        json!({
            "items": cabinets_with_networks,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "机柜获取成功",
    ))
}

pub async fn get_cabinets_by_network_region(
    State(state): State<Arc<AppState>>,
    Path(region_id_str): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Response, AppError> {
    let Ok(region_id) = Uuid::parse_str(&region_id_str) else {
        return Err(AppError::Validation("无效的网络区域ID".to_string()));
    };

    let network_id_filter = query
        .get("network_id")
        .and_then(|s| Uuid::parse_str(s).ok());

    let cabinets = if let Some(network_id) = network_id_filter {
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

    Ok(crate::error::ok_json(cabinets, "机柜获取成功"))
}

pub async fn create_cabinet(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<CabinetCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let existing_cabinet =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE name = $1 AND room_id = $2")
            .bind(&req.name)
            .bind(req.room_id)
            .fetch_optional(&state.pool()?.get_conn())
            .await?;

    if existing_cabinet.is_some() {
        return Err(AppError::Conflict("机柜名称已存在".to_string()));
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
    .execute(&state.pool()?.get_conn())
    .await?;

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
    log_op_best_effort(&state.pool()?.get_conn(), &meta, "create", "cabinet", Some(&id), &details).await;

    Ok(crate::error::ok_json(cabinet, "机柜创建成功"))
}

pub async fn get_cabinet(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let cabinet = sqlx::query_as::<_, Cabinet>(
        "SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets WHERE id = $1"
    ).bind(id)
    .fetch_optional(&state.pool()?.get_conn()).await?
    .ok_or_else(|| AppError::NotFound("机柜未找到".to_string()))?;

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

    // 加载该机柜下的配线架（net_outlets 中 outlet_type='patch_panel' 的行）
    let pp_rows = sqlx::query(
        "SELECT no.id, no.name, no.outlet_type, no.cabinet_id, cab.name AS cabinet_name \
         FROM net_outlets no \
         LEFT JOIN cabinets cab ON no.cabinet_id = cab.id \
         WHERE no.cabinet_id = $1 AND no.outlet_type = 'patch_panel' \
         ORDER BY no.name",
    )
    .bind(cabinet.id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;
    let patch_panels: Vec<NetOutletBrief> = pp_rows
        .iter()
        .map(|r| NetOutletBrief {
            id: r.get("id"),
            name: r.get("name"),
            outlet_type: r.get("outlet_type"),
            cabinet_id: r.get("cabinet_id"),
            cabinet_name: r.get("cabinet_name"),
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

    Ok(crate::error::ok_json(cabinet_with_networks, "机柜获取成功"))
}

pub async fn update_cabinet(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<CabinetUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let existing_cabinet = sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

    if existing_cabinet.is_none() {
        return Err(AppError::NotFound("机柜未找到".to_string()));
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
    .execute(&state.pool()?.get_conn())
    .await?;

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
    log_op_best_effort(&state.pool()?.get_conn(), &meta, "update", "cabinet", Some(&id), &details).await;

    Ok(crate::error::ok_json(cabinet, "机柜更新成功"))
}

pub async fn delete_cabinet(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let existing_cabinet = sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

    if existing_cabinet.is_none() {
        return Err(AppError::NotFound("机柜未找到".to_string()));
    }

    let position_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM positions WHERE cabinet_id = $1")
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    if position_count > 0 {
        return Err(AppError::Validation(
            "该机柜已被机位关联，无法删除".to_string(),
        ));
    }

    sqlx::query("DELETE FROM cabinets WHERE id = $1")
        .bind(id)
        .execute(&state.pool()?.get_conn())
        .await?;

    let details = serde_json::json!({
        "cabinet_id": id.to_string()
    });
    log_op_best_effort(&state.pool()?.get_conn(), &meta, "delete", "cabinet", Some(&id), &details).await;

    Ok(crate::error::ok_json((), "机柜删除成功"))
}

pub async fn get_cabinet_networks(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let existing_cabinet = sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

    if existing_cabinet.is_none() {
        return Err(AppError::NotFound("机柜未找到".to_string()));
    }

    let cabinet_networks = sqlx::query_as::<_, NetworkInfo>(
        r"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr 
           FROM rooms r 
           JOIN room_networks rn ON r.id = rn.room_id
           JOIN network_cidrs n ON rn.network_id = n.id 
           JOIN network_regions nr ON n.network_region_id = nr.id 
           WHERE r.id = (SELECT room_id FROM cabinets WHERE id = $1) 
           AND r.room_type IN ('DATA_CENTER', 'TELECOM_CLOSET')",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(crate::error::ok_json(cabinet_networks, "机柜网段获取成功"))
}

pub async fn sync_cabinet_positions(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<CabinetPositionsSync>,
) -> Result<Response, AppError> {
    req.validate()?;

    let cabinet_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cabinets WHERE id = $1)")
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;
    if !cabinet_exists {
        return Err(AppError::NotFound("机柜未找到".to_string()));
    }

    let mut tx = state.pool()?.get_conn().begin().await?;
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
                return Err(AppError::Validation(
                    "机位已被设备关联，无法删除".to_string(),
                ));
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
            return Err(AppError::Conflict("机位名称已存在".to_string()));
        }

        if let Some(item_id) = item.id {
            sqlx::query(
                "UPDATE positions SET name = $1, start_u = $2, end_u = $3, description = $4, cabinet_id = $5, updated_at = $6 WHERE id = $7",
            )
            .bind(&item.name)
            .bind(item.start_u)
            .bind(item.end_u)
            .bind(&item.description)
            .bind(id)
            .bind(now)
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
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
    log_op_best_effort(&state.pool()?.get_conn(), &meta, "sync_positions", "cabinet", Some(&id), &details).await;

    Ok(crate::error::ok_json((), "机位同步成功"))
}

/// 同步机柜下的配线架（net_outlets 中 outlet_type='patch_panel' 的行）
/// 参照 room.rs::sync_room_net_outlets 实现，范围为当前机柜的配线架
pub async fn sync_cabinet_net_outlets(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<CabinetPatchPanelsSync>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 取机柜及其所在房间（配线架的 room_id 必须取自机柜所在房间）
    let cabinet_row = sqlx::query("SELECT id, room_id FROM cabinets WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;
    let room_id: Uuid = cabinet_row
        .ok_or_else(|| AppError::NotFound("机柜未找到".to_string()))?
        .get("room_id");

    let items = &req.patch_panels;
    let mut tx = state.pool()?.get_conn().begin().await?;
    let now = Utc::now();

    // 既有配线架 id 范围（仅本机柜的）
    let existing_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM net_outlets WHERE cabinet_id = $1 AND outlet_type = 'patch_panel'")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;

    let request_ids: Vec<Uuid> = items.iter().filter_map(|i| i.id).collect();

    // 删除请求中不存在的配线架（cable_links 的删除保护触发器会阻止被引用的删除）
    for existing_id in &existing_ids {
        if !request_ids.contains(existing_id) {
            sqlx::query("DELETE FROM net_outlets WHERE id = $1")
                .bind(existing_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    if let sqlx::Error::Database(db_err) = &e {
                        let msg = db_err.message();
                        if msg.contains("cable_links") {
                            return AppError::Validation(
                                "配线架已被线路引用，无法删除".to_string(),
                            );
                        }
                    }
                    AppError::from(e)
                })?;
        }
    }

    // 新增或更新（强制 outlet_type='patch_panel'、cabinet_id=机柜、room_id=机柜房间）
    for item in items {
        // 名称唯一性（UNIQUE(room_id, name)）：在整个房间范围内唯一
        let dup: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM net_outlets WHERE room_id = $1 AND name = $2 AND ($3::uuid IS NULL OR id != $3)",
        )
        .bind(room_id)
        .bind(&item.name)
        .bind(item.id)
        .fetch_optional(&mut *tx)
        .await?;
        if dup.is_some() {
            return Err(AppError::Conflict("配线架名称在该房间内已存在".to_string()));
        }

        if let Some(item_id) = item.id {
            sqlx::query(
                "UPDATE net_outlets SET name = $1, outlet_type = 'patch_panel', cabinet_id = $2, room_id = $3, updated_at = $4 WHERE id = $5",
            )
            .bind(&item.name)
            .bind(id)
            .bind(room_id)
            .bind(now)
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
        } else {
            let new_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO net_outlets (id, name, outlet_type, room_id, cabinet_id, created_at, updated_at) VALUES ($1, $2, 'patch_panel', $3, $4, $5, $6)",
            )
            .bind(new_id)
            .bind(&item.name)
            .bind(room_id)
            .bind(id)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    let details = serde_json::json!({
        "cabinet_id": id.to_string(),
        "patch_panel_count": items.len()
    });
    log_op_best_effort(&state.pool()?.get_conn(), &meta, "sync_net_outlets", "cabinet", Some(&id), &details).await;

    Ok(crate::error::ok_json((), "机柜配线架同步成功"))
}
