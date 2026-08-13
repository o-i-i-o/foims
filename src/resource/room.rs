use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    CabinetBrief, NetOutletBrief, NetworkInfo, Room, RoomChildrenSync, RoomCreate,
    RoomNetOutletsSync, RoomUpdate, RoomWithNetworks, WorkstationBrief,
};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::Pagination;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

pub async fn get_rooms(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());

    let search_pattern = crate::utils::escape_like(&search);

    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY name DESC",
        ("created_at", "desc") => "ORDER BY created_at DESC",
        ("created_at", _) => "ORDER BY created_at ASC",
        ("room_type", "desc") => "ORDER BY room_type DESC, name ASC",
        ("room_type", _) => "ORDER BY room_type ASC, name ASC",
        _ => "ORDER BY name ASC",
    };

    let (total, rooms) = if search.is_empty() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms")
            .fetch_one(&state.pool()?.get_conn())
            .await?;

        let rooms = sqlx::query_as::<_, Room>(
            sqlx::AssertSqlSafe(format!("SELECT id, name, room_type, org_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms {order_clause} LIMIT $1 OFFSET $2"))
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, rooms)
    } else {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM rooms WHERE name ILIKE $1 OR room_type ILIKE $1 OR description ILIKE $1"
        )
        .bind(&search_pattern)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

        let rooms = sqlx::query_as::<_, Room>(
            sqlx::AssertSqlSafe(format!("SELECT id, name, room_type, org_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms WHERE name ILIKE $1 OR room_type ILIKE $1 OR description ILIKE $1 {order_clause} LIMIT $2 OFFSET $3"))
        )
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, rooms)
    };

    // 批量查询（避免逐房间 N+1）：一次取全部房间的网络、工位数、组织名
    let conn = state.pool()?.get_conn();
    let room_ids: Vec<Uuid> = rooms.iter().map(|r| r.id).collect();
    let org_ids: Vec<Uuid> = rooms.iter().filter_map(|r| r.org_id).collect();

    let mut networks_map: HashMap<Uuid, Vec<NetworkInfo>> = HashMap::new();
    let network_rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, Uuid, Option<String>, Option<String>)>(
        r"SELECT rn.room_id, n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text, n.ipv6_cidr::text
           FROM room_networks rn
           JOIN network_cidrs n ON rn.network_id = n.id
           JOIN network_regions nr ON n.network_region_id = nr.id
           WHERE rn.room_id = ANY($1)",
    )
    .bind(&room_ids)
    .fetch_all(&conn)
    .await?;
    for (room_id, id, name, network_region, network_region_id, ipv4_cidr, ipv6_cidr) in network_rows
    {
        networks_map.entry(room_id).or_default().push(NetworkInfo {
            id,
            name,
            network_region,
            network_region_id,
            ipv4_cidr,
            ipv6_cidr,
        });
    }

    let ws_count_map: HashMap<Uuid, i64> = sqlx::query_as::<_, (Uuid, i64)>(
        "SELECT room_id, COUNT(*) FROM workstations WHERE room_id = ANY($1) GROUP BY room_id",
    )
    .bind(&room_ids)
    .fetch_all(&conn)
    .await?
    .into_iter()
    .collect();

    let org_name_map: HashMap<Uuid, String> = if org_ids.is_empty() {
        HashMap::new()
    } else {
        sqlx::query_as::<_, (Uuid, String)>("SELECT id, name FROM organizations WHERE id = ANY($1)")
            .bind(&org_ids)
            .fetch_all(&conn)
            .await?
            .into_iter()
            .collect()
    };

    let mut rooms_with_networks = Vec::new();
    for room in rooms {
        let room_networks = networks_map.remove(&room.id).unwrap_or_default();
        let workstation_count = ws_count_map.get(&room.id).copied().unwrap_or(0);
        let org_name = room.org_id.and_then(|oid| org_name_map.get(&oid).cloned());

        let room_with_networks = RoomWithNetworks {
            id: room.id,
            name: room.name,
            room_type: room.room_type.clone(),
            org_id: room.org_id,
            org_name,
            description: room.description.clone(),
            networks: room_networks,
            workstation_count,
            workstations: None,
            cabinets: None,
            net_outlets: Vec::new(),
            created_at: room.created_at,
            updated_at: room.updated_at,
        };

        rooms_with_networks.push(room_with_networks);
    }

    Ok(crate::error::ok_json(
        json!({
            "items": rooms_with_networks,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "房间获取成功",
    ))
}

pub async fn create_room(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<RoomCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let existing_room = sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE name = $1")
        .bind(&req.name)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

    if existing_room.is_some() {
        return Err(AppError::Conflict("房间名称已存在".to_string()));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    // 房间与其网络关联必须在同一事务内写入，避免中途失败导致网络关联残缺
    let mut tx = state.pool()?.get_conn().begin().await?;

    sqlx::query(
        "INSERT INTO rooms (id, name, room_type, org_id, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.room_type)
    .bind(req.org_id)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    for network_id in &req.network_ids {
        sqlx::query(
            "INSERT INTO room_networks (id, room_id, network_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(id)
        .bind(network_id)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let room = Room {
        id,
        name: req.name.clone(),
        room_type: req.room_type.clone(),
        org_id: req.org_id,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": room.name,
        "room_type": room.room_type,
        "description": room.description,
        "network_count": req.network_ids.len()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "room",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(room, "房间创建成功"))
}

pub async fn get_room(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let room = sqlx::query_as::<_, Room>(
        "SELECT id, name, room_type, org_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms WHERE id = $1"
    ).bind(id)
    .fetch_optional(&state.pool()?.get_conn()).await?
    .ok_or_else(|| AppError::NotFound("房间未找到".to_string()))?;

    let room_networks = sqlx::query_as::<_, NetworkInfo>(
        r"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr
           FROM room_networks rn
           JOIN network_cidrs n ON rn.network_id = n.id
           JOIN network_regions nr ON n.network_region_id = nr.id
           WHERE rn.room_id = $1",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let workstation_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM workstations WHERE room_id = $1")
            .bind(room.id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    let org_name: Option<String> = if let Some(oid) = room.org_id {
        sqlx::query_scalar("SELECT name FROM organizations WHERE id = $1")
            .bind(oid)
            .fetch_optional(&state.pool()?.get_conn())
            .await?
    } else {
        None
    };

    let (workstations, cabinets) = if room.room_type == "OFFICE" {
        let ws_rows = sqlx::query(
            "SELECT id, name, manager FROM workstations WHERE room_id = $1 ORDER BY name",
        )
        .bind(room.id)
        .fetch_all(&state.pool()?.get_conn())
        .await?;
        let ws_list: Vec<WorkstationBrief> = ws_rows
            .iter()
            .map(|r| WorkstationBrief {
                id: r.get("id"),
                name: r.get("name"),
                manager: r.get("manager"),
            })
            .collect();
        (Some(ws_list), None)
    } else if room.room_type == "DATA_CENTER" || room.room_type == "TELECOM_CLOSET" {
        let cab_rows =
            sqlx::query("SELECT id, name, capacity FROM cabinets WHERE room_id = $1 ORDER BY name")
                .bind(room.id)
                .fetch_all(&state.pool()?.get_conn())
                .await?;
        let cab_list: Vec<CabinetBrief> = cab_rows
            .iter()
            .map(|r| CabinetBrief {
                id: r.get("id"),
                name: r.get("name"),
                capacity: r.get("capacity"),
            })
            .collect();
        (None, Some(cab_list))
    } else {
        (None, None)
    };

    // 信息点：所有房型都支持（特指网络插座，仅隶属房间）
    let no_rows = sqlx::query("SELECT id, name FROM net_outlets WHERE room_id = $1 ORDER BY name")
        .bind(room.id)
        .fetch_all(&state.pool()?.get_conn())
        .await?;
    let net_outlets: Vec<NetOutletBrief> = no_rows
        .iter()
        .map(|r| NetOutletBrief {
            id: r.get("id"),
            name: r.get("name"),
        })
        .collect();

    let room_with_networks = RoomWithNetworks {
        id: room.id,
        name: room.name,
        room_type: room.room_type,
        org_id: room.org_id,
        org_name,
        description: room.description,
        networks: room_networks,
        workstation_count,
        workstations,
        cabinets,
        net_outlets,
        created_at: room.created_at,
        updated_at: room.updated_at,
    };

    Ok(crate::error::ok_json(room_with_networks, "房间获取成功"))
}

pub async fn update_room(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<RoomUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let existing_room = sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

    if existing_room.is_none() {
        return Err(AppError::NotFound("房间未找到".to_string()));
    }

    let now = Utc::now();

    // 房间字段更新与网络关联的「删除+重建」必须在同一事务内，避免中途失败丢失全部网络关联
    let mut tx = state.pool()?.get_conn().begin().await?;

    sqlx::query(
        "UPDATE rooms SET
         name = COALESCE($1, name),
         room_type = COALESCE($2, room_type),
         org_id = CASE WHEN $3::boolean THEN $4 ELSE org_id END,
         description = COALESCE($5, description),
         updated_at = $6
         WHERE id = $7",
    )
    .bind(&req.name)
    .bind(&req.room_type)
    .bind(req.org_id.is_some())
    .bind(req.org_id.flatten())
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    if let Some(network_ids) = &req.network_ids {
        sqlx::query("DELETE FROM room_networks WHERE room_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        for network_id in network_ids {
            sqlx::query(
                "INSERT INTO room_networks (id, room_id, network_id, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(network_id)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    let room = sqlx::query_as::<_, Room>(
        "SELECT id, name, room_type, org_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms WHERE id = $1"
    ).bind(id)
    .fetch_one(&state.pool()?.get_conn()).await?;

    let details = serde_json::json!({
        "name": room.name,
        "room_type": room.room_type,
        "description": room.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "room",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(room, "房间更新成功"))
}

pub async fn delete_room(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let existing_room = sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

    if existing_room.is_none() {
        return Err(AppError::NotFound("房间未找到".to_string()));
    }

    let cabinet_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cabinets WHERE room_id = $1")
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    if cabinet_count > 0 {
        return Err(AppError::Validation(
            "该房间已被机柜关联，无法删除".to_string(),
        ));
    }

    let workstation_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workstations WHERE room_id = $1")
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    if workstation_count > 0 {
        return Err(AppError::Validation(
            "该房间已被工位关联，无法删除".to_string(),
        ));
    }

    let net_outlet_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM net_outlets WHERE room_id = $1")
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    if net_outlet_count > 0 {
        return Err(AppError::Validation(
            "该房间已被信息点关联，无法删除".to_string(),
        ));
    }

    sqlx::query("DELETE FROM rooms WHERE id = $1")
        .bind(id)
        .execute(&state.pool()?.get_conn())
        .await?;

    let details = serde_json::json!({
        "room_id": id.to_string()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "room",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json((), "房间删除成功"))
}

pub async fn get_room_networks(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let existing_room = sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

    if existing_room.is_none() {
        return Err(AppError::NotFound("房间未找到".to_string()));
    }

    let room_networks = sqlx::query_as::<_, NetworkInfo>(
        r"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr
           FROM room_networks rn
           JOIN network_cidrs n ON rn.network_id = n.id
           JOIN network_regions nr ON n.network_region_id = nr.id
           WHERE rn.room_id = $1",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(crate::error::ok_json(room_networks, "房间网段获取成功"))
}

pub async fn sync_room_children(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<RoomChildrenSync>,
) -> Result<Response, AppError> {
    req.validate()?;

    let room_type: String = sqlx::query_scalar("SELECT room_type FROM rooms WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .ok_or_else(|| AppError::NotFound("房间未找到".to_string()))?;

    let mut tx = state.pool()?.get_conn().begin().await?;
    let now = Utc::now();

    if room_type == "OFFICE" {
        let items = req
            .workstations
            .as_ref()
            .ok_or_else(|| AppError::Validation("办公室房间需要工位数据".to_string()))?;

        let existing_ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM workstations WHERE room_id = $1")
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;

        let request_ids: Vec<Uuid> = items.iter().filter_map(|i| i.id).collect();

        for existing_id in &existing_ids {
            if !request_ids.contains(existing_id) {
                let device_count: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE workstation_id = $1")
                        .bind(existing_id)
                        .fetch_one(&mut *tx)
                        .await?;
                if device_count > 0 {
                    return Err(AppError::Validation(
                        "工位已被设备关联，无法删除".to_string(),
                    ));
                }
                sqlx::query("DELETE FROM workstation_layouts WHERE workstation_id = $1")
                    .bind(existing_id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("DELETE FROM workstations WHERE id = $1")
                    .bind(existing_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        for item in items {
            validate_workstation_name(&mut tx, &item.name, id, item.id).await?;
            if let Some(item_id) = item.id {
                sqlx::query(
                    "UPDATE workstations SET name = $1, manager = $2, room_id = $3, updated_at = $4 WHERE id = $5",
                )
                .bind(&item.name)
                .bind(&item.manager)
                .bind(id)
                .bind(now)
                .bind(item_id)
                .execute(&mut *tx)
                .await?;
            } else {
                let new_id = Uuid::new_v4();
                sqlx::query(
                    "INSERT INTO workstations (id, name, room_id, manager, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6)",
                )
                .bind(new_id)
                .bind(&item.name)
                .bind(id)
                .bind(&item.manager)
                .bind(now)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            }
        }
    } else if room_type == "DATA_CENTER" || room_type == "TELECOM_CLOSET" {
        let items = req
            .cabinets
            .as_ref()
            .ok_or_else(|| AppError::Validation("机房需要机柜数据".to_string()))?;

        let existing_ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM cabinets WHERE room_id = $1")
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;

        let request_ids: Vec<Uuid> = items.iter().filter_map(|i| i.id).collect();

        for existing_id in &existing_ids {
            if !request_ids.contains(existing_id) {
                let position_count: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM positions WHERE cabinet_id = $1")
                        .bind(existing_id)
                        .fetch_one(&mut *tx)
                        .await?;
                if position_count > 0 {
                    return Err(AppError::Validation(
                        "机柜已被机位关联，无法删除".to_string(),
                    ));
                }
                sqlx::query("DELETE FROM cabinet_layouts WHERE cabinet_id = $1")
                    .bind(existing_id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("DELETE FROM cabinets WHERE id = $1")
                    .bind(existing_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        for item in items {
            validate_cabinet_name(&mut tx, &item.name, id, item.id).await?;
            if let Some(item_id) = item.id {
                sqlx::query(
                    "UPDATE cabinets SET name = $1, capacity = $2, room_id = $3, updated_at = $4 WHERE id = $5",
                )
                .bind(&item.name)
                .bind(item.capacity)
                .bind(id)
                .bind(now)
                .bind(item_id)
                .execute(&mut *tx)
                .await?;
            } else {
                let new_id = Uuid::new_v4();
                sqlx::query(
                    "INSERT INTO cabinets (id, name, room_id, capacity, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6)",
                )
                .bind(new_id)
                .bind(&item.name)
                .bind(id)
                .bind(item.capacity)
                .bind(now)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    tx.commit().await?;

    let details = serde_json::json!({
        "room_id": id.to_string(),
        "room_type": room_type,
        "workstation_count": req.workstations.as_ref().map(|v| v.len()).unwrap_or(0),
        "cabinet_count": req.cabinets.as_ref().map(|v| v.len()).unwrap_or(0)
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "sync_children",
        "room",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json((), "房间子项同步成功"))
}

pub async fn sync_room_net_outlets(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<RoomNetOutletsSync>,
) -> Result<Response, AppError> {
    req.validate()?;

    let room_exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM rooms WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;
    if room_exists.is_none() {
        return Err(AppError::NotFound("房间未找到".to_string()));
    }

    let items = &req.net_outlets;
    let mut tx = state.pool()?.get_conn().begin().await?;
    let now = Utc::now();

    let existing_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM net_outlets WHERE room_id = $1")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;

    let request_ids: Vec<Uuid> = items.iter().filter_map(|i| i.id).collect();

    // 删除请求中不存在的信息点（cable_links 的删除保护触发器会阻止被引用的删除）
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
                                "信息点已被线路引用，无法删除".to_string(),
                            );
                        }
                    }
                    AppError::from(e)
                })?;
        }
    }

    // 新增或更新
    for item in items {
        validate_net_outlet_name(&mut tx, &item.name, id, item.id).await?;
        if let Some(item_id) = item.id {
            sqlx::query(
                "UPDATE net_outlets SET name = $1, room_id = $2, updated_at = $3 WHERE id = $4",
            )
            .bind(&item.name)
            .bind(id)
            .bind(now)
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
        } else {
            let new_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO net_outlets (id, name, room_id, created_at, updated_at) VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(new_id)
            .bind(&item.name)
            .bind(id)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    let details = serde_json::json!({
        "room_id": id.to_string(),
        "net_outlet_count": items.len()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "sync_net_outlets",
        "room",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json((), "房间信息点同步成功"))
}

async fn validate_net_outlet_name(
    conn: &mut sqlx::PgConnection,
    name: &str,
    room_id: Uuid,
    exclude_id: Option<Uuid>,
) -> Result<(), AppError> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM net_outlets WHERE name = $1 AND room_id = $2 AND ($3::uuid IS NULL OR id != $3)",
    )
    .bind(name)
    .bind(room_id)
    .bind(exclude_id)
    .fetch_optional(conn)
    .await?;
    if existing.is_some() {
        return Err(AppError::Conflict("该房间下信息点名称已存在".to_string()));
    }
    Ok(())
}

async fn validate_workstation_name(
    conn: &mut sqlx::PgConnection,
    name: &str,
    room_id: Uuid,
    exclude_id: Option<Uuid>,
) -> Result<(), AppError> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM workstations WHERE name = $1 AND room_id = $2 AND ($3::uuid IS NULL OR id != $3)",
    )
    .bind(name)
    .bind(room_id)
    .bind(exclude_id)
    .fetch_optional(conn)
    .await?;
    if existing.is_some() {
        return Err(AppError::Conflict("工位名称已存在".to_string()));
    }
    Ok(())
}

async fn validate_cabinet_name(
    conn: &mut sqlx::PgConnection,
    name: &str,
    room_id: Uuid,
    exclude_id: Option<Uuid>,
) -> Result<(), AppError> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM cabinets WHERE name = $1 AND room_id = $2 AND ($3::uuid IS NULL OR id != $3)",
    )
    .bind(name)
    .bind(room_id)
    .bind(exclude_id)
    .fetch_optional(conn)
    .await?;
    if existing.is_some() {
        return Err(AppError::Conflict("机柜名称已存在".to_string()));
    }
    Ok(())
}
