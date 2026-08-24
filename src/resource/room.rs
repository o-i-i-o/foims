//! 房间资源管理（含网络绑定与子资源级联）。

use crate::app_state::AppState;
use crate::error::{AppError, msg};
use crate::models::{
    CabinetBrief, NetOutletBrief, NetworkInfo, Room, RoomChildrenSync, RoomCreate,
    RoomNetOutletsSync, RoomUpdate, RoomWithNetworks, WorkstationBrief,
};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::{Pagination, paged_response};
use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
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
        ("name", "desc") => " ORDER BY r.name DESC",
        ("created_at", "desc") => " ORDER BY r.created_at DESC",
        ("created_at", _) => " ORDER BY r.created_at ASC",
        ("room_type", "desc") => " ORDER BY r.room_type DESC, r.name ASC",
        ("room_type", _) => " ORDER BY r.room_type ASC, r.name ASC",
        ("org_name", "desc") => " ORDER BY o.name DESC NULLS LAST, r.name ASC",
        ("org_name", _) => " ORDER BY o.name ASC NULLS LAST, r.name ASC",
        _ => " ORDER BY r.name ASC",
    };

    // 可选过滤：org_id（组织节点）、room_type（逗号分隔的类型集合）
    let org_filter: Option<Uuid> = match query.get("org_id") {
        Some(v) if !v.is_empty() => Some(Uuid::parse_str(v).map_err(|_| {
            AppError::Validation(msg("server.common.invalid_param").with("param", "org_id"))
        })?),
        _ => None,
    };
    let room_types: Vec<String> = query
        .get("room_type")
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_uppercase())
                .collect()
        })
        .unwrap_or_default();

    // 动态拼接过滤条件（count 与列表两侧保持一致）
    let mut count_builder: sqlx::QueryBuilder<sqlx::Postgres> =
        sqlx::QueryBuilder::new("SELECT COUNT(*) FROM rooms r");
    let mut list_builder: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
        "SELECT r.id, r.name, r.room_type, r.org_id, r.description, \
         r.created_at::TIMESTAMPTZ, r.updated_at::TIMESTAMPTZ \
         FROM rooms r LEFT JOIN organizations o ON r.org_id = o.id",
    );

    let mut has_where = false;
    if !search.is_empty() {
        for builder in [&mut count_builder, &mut list_builder] {
            builder.push(" WHERE r.name ILIKE ");
            builder.push_bind(search_pattern.clone());
            builder.push(" OR r.room_type ILIKE ");
            builder.push_bind(search_pattern.clone());
            builder.push(" OR r.description ILIKE ");
            builder.push_bind(search_pattern.clone());
        }
        has_where = true;
    }
    // 各过滤分支独立判断首条件（WHERE）与后续条件（AND），
    // 避免 org_id 与 room_type 组合时拼出双 WHERE
    if let Some(org_id) = org_filter {
        let conjunction = if has_where { " AND" } else { " WHERE" };
        for builder in [&mut count_builder, &mut list_builder] {
            builder.push(conjunction);
            builder.push(" r.org_id = ");
            builder.push_bind(org_id);
        }
        has_where = true;
    }
    if !room_types.is_empty() {
        let conjunction = if has_where { " AND" } else { " WHERE" };
        for builder in [&mut count_builder, &mut list_builder] {
            builder.push(conjunction);
            builder.push(" r.room_type = ANY(");
            builder.push_bind(room_types.clone());
            builder.push(")");
        }
    }

    let total: i64 = count_builder
        .build_query_scalar()
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    list_builder.push(order_clause);
    list_builder.push(" LIMIT ");
    list_builder.push_bind(page_size);
    list_builder.push(" OFFSET ");
    list_builder.push_bind(offset);

    let rooms = list_builder
        .build_query_as::<Room>()
        .fetch_all(&state.pool()?.get_conn())
        .await?;

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
        paged_response(rooms_with_networks, total, &pagination),
        "server.room.list_retrieved",
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
        return Err(AppError::Conflict(msg("server.room.name_exists")));
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
    .bind(req.room_type.to_uppercase())
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

    Ok(crate::error::ok_json(room, "server.room.created"))
}

/// 房间编辑回显专用轻量端点：仅基础字段 + 网络绑定（GET /{id}/brief）。
///
/// 完整详情接口为组装视图需 7 次串行查询，而编辑弹窗只需要
/// name/room_type/org_id/description/networks，此前一并拉取了
/// 工位/机柜/信息点等无关数据。
pub async fn get_room_brief(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();
    let (room, room_networks) = tokio::join!(
        sqlx::query_as::<_, Room>(
            "SELECT id, name, room_type, org_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&conn),
        sqlx::query_as::<_, NetworkInfo>(
            r"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr
               FROM room_networks rn
               JOIN network_cidrs n ON rn.network_id = n.id
               JOIN network_regions nr ON n.network_region_id = nr.id
               WHERE rn.room_id = $1",
        )
        .bind(id)
        .fetch_all(&conn)
    );

    let room = room?.ok_or_else(|| AppError::NotFound(msg("server.room.not_found")))?;
    let room_networks = room_networks?;

    Ok(crate::error::ok_json(
        serde_json::json!({
            "id": room.id,
            "name": room.name,
            "room_type": room.room_type,
            "org_id": room.org_id,
            "description": room.description,
            "networks": room_networks,
        }),
        "server.room.fetched",
    ))
}

pub async fn get_room(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let room = sqlx::query_as::<_, Room>(
        "SELECT id, name, room_type, org_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms WHERE id = $1"
    ).bind(id)
    .fetch_optional(&state.pool()?.get_conn()).await?
    .ok_or_else(|| AppError::NotFound(msg("server.room.not_found")))?;

    // 以下查询相互独立，并行执行替代原先的串行等待
    let conn = state.pool()?.get_conn();
    let is_office = room.room_type == "OFFICE";
    let is_dc = room.room_type == "DATA_CENTER" || room.room_type == "TELECOM_CLOSET";

    let (room_networks, workstation_count, org_name_opt, ws_rows, cab_rows, no_rows) =
        tokio::join!(
            sqlx::query_as::<_, NetworkInfo>(
                r"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr
                   FROM room_networks rn
                   JOIN network_cidrs n ON rn.network_id = n.id
                   JOIN network_regions nr ON n.network_region_id = nr.id
                   WHERE rn.room_id = $1",
            )
            .bind(room.id)
            .fetch_all(&conn),
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workstations WHERE room_id = $1")
                .bind(room.id)
                .fetch_one(&conn),
            async {
                match room.org_id {
                    Some(oid) => sqlx::query_scalar::<_, String>(
                        "SELECT name FROM organizations WHERE id = $1",
                    )
                    .bind(oid)
                    .fetch_optional(&conn)
                    .await
                    .map_err(AppError::from),
                    None => Ok(None),
                }
            },
            async {
                if is_office {
                    sqlx::query(
                        "SELECT id, name, manager, manager_employee_id FROM workstations WHERE room_id = $1 ORDER BY name",
                    )
                    .bind(room.id)
                    .fetch_all(&conn)
                    .await
                    .map_err(AppError::from)
                } else {
                    Ok(Vec::new())
                }
            },
            async {
                if is_dc {
                    sqlx::query(
                        "SELECT id, name, capacity FROM cabinets WHERE room_id = $1 ORDER BY name",
                    )
                    .bind(room.id)
                    .fetch_all(&conn)
                    .await
                    .map_err(AppError::from)
                } else {
                    Ok(Vec::new())
                }
            },
            sqlx::query("SELECT id, name FROM net_outlets WHERE room_id = $1 ORDER BY name")
                .bind(room.id)
                .fetch_all(&conn)
        );

    let workstation_count = workstation_count?;
    let org_name = org_name_opt?;

    let workstations = if is_office {
        Some(
            ws_rows?
                .iter()
                .map(|r| WorkstationBrief {
                    id: r.get("id"),
                    name: r.get("name"),
                    manager: r.get("manager"),
                    manager_employee_id: r.get("manager_employee_id"),
                })
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    let cabinets = if is_dc {
        Some(
            cab_rows?
                .iter()
                .map(|r| CabinetBrief {
                    id: r.get("id"),
                    name: r.get("name"),
                    capacity: r.get("capacity"),
                })
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    let net_outlets: Vec<NetOutletBrief> = no_rows?
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
        networks: room_networks?,
        workstation_count,
        workstations,
        cabinets,
        net_outlets,
        created_at: room.created_at,
        updated_at: room.updated_at,
    };

    Ok(crate::error::ok_json(
        room_with_networks,
        "server.room.fetched",
    ))
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
        return Err(AppError::NotFound(msg("server.room.not_found")));
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
    .bind(req.room_type.as_ref().map(|t| t.to_uppercase()))
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

    Ok(crate::error::ok_json(room, "server.room.updated"))
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
        return Err(AppError::NotFound(msg("server.room.not_found")));
    }

    let cabinet_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cabinets WHERE room_id = $1")
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    if cabinet_count > 0 {
        return Err(AppError::Validation(msg("server.room.has_cabinets")));
    }

    let workstation_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workstations WHERE room_id = $1")
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    if workstation_count > 0 {
        return Err(AppError::Validation(msg("server.room.has_workstations")));
    }

    let net_outlet_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM net_outlets WHERE room_id = $1")
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    if net_outlet_count > 0 {
        return Err(AppError::Validation(msg("server.room.has_net_outlets")));
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

    Ok(crate::error::ok_json((), "server.room.deleted"))
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
        return Err(AppError::NotFound(msg("server.room.not_found")));
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

    Ok(crate::error::ok_json(
        room_networks,
        "server.room.networks_retrieved",
    ))
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
        .ok_or_else(|| AppError::NotFound(msg("server.room.not_found")))?;

    let mut tx = state.pool()?.get_conn().begin().await?;
    let now = Utc::now();

    // 按房型性质同步子项：办公类（办公室/大厅/前台）同步工位；
    // 机房类（机房/弱电井）同步机柜；"其他"无固定性质，两类均同步
    let sync_workstations = matches!(
        room_type.as_str(),
        "OFFICE" | "LOBBY" | "RECEPTION" | "OTHER"
    );
    let sync_cabinets = matches!(
        room_type.as_str(),
        "DATA_CENTER" | "TELECOM_CLOSET" | "OTHER"
    );

    if sync_workstations {
        let items = req
            .workstations
            .as_ref()
            .ok_or_else(|| AppError::Validation(msg("server.room.office_requires_workstations")))?;
        sync_workstation_children(&mut tx, id, items, now).await?;
    }

    if sync_cabinets {
        let items = req
            .cabinets
            .as_ref()
            .ok_or_else(|| AppError::Validation(msg("server.room.datacenter_requires_cabinets")))?;
        sync_cabinet_children(&mut tx, id, items, now).await?;
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

    Ok(crate::error::ok_json((), "server.room.children_synced"))
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
        return Err(AppError::NotFound(msg("server.room.not_found")));
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
                        let msg_text = db_err.message();
                        if msg_text.contains("cable_links") {
                            return AppError::Validation(msg("server.net_outlet.in_use"));
                        }
                    }
                    AppError::from(e)
                })?;
        }
    }

    // 新增或更新
    for item in items {
        validate_net_outlet_name(&mut tx, &item.name, item.id).await?;
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

    Ok(crate::error::ok_json((), "server.room.net_outlets_synced"))
}

/// 同步房间工位：删除请求中缺失的（被设备引用的拒绝），再新增/更新。
async fn sync_workstation_children(
    tx: &mut sqlx::PgConnection,
    room_id: Uuid,
    items: &[crate::models::WorkstationSyncItem],
    now: chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    let existing_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM workstations WHERE room_id = $1")
            .bind(room_id)
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
                return Err(AppError::Validation(msg(
                    "server.workstation.in_use_by_device",
                )));
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
        validate_workstation_name(tx, &item.name, room_id, item.id).await?;
        if let Some(item_id) = item.id {
            sqlx::query(
                "UPDATE workstations SET name = $1,
                    manager = COALESCE((SELECT name FROM employees WHERE id = $2), $3),
                    manager_employee_id = $2,
                    room_id = $4, updated_at = $5
                 WHERE id = $6",
            )
            .bind(&item.name)
            .bind(item.manager_employee_id)
            .bind(&item.manager)
            .bind(room_id)
            .bind(now)
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
        } else {
            let new_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO workstations (id, name, room_id, manager, manager_employee_id, created_at, updated_at)
                 VALUES ($1, $2, $3, COALESCE((SELECT name FROM employees WHERE id = $4), $5), $4, $6, $7)",
            )
            .bind(new_id)
            .bind(&item.name)
            .bind(room_id)
            .bind(item.manager_employee_id)
            .bind(&item.manager)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }
    Ok(())
}

/// 同步房间机柜：删除请求中缺失的（含机位的拒绝），再新增/更新。
async fn sync_cabinet_children(
    tx: &mut sqlx::PgConnection,
    room_id: Uuid,
    items: &[crate::models::CabinetSyncItem],
    now: chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    let existing_ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM cabinets WHERE room_id = $1")
        .bind(room_id)
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
                return Err(AppError::Validation(msg(
                    "server.cabinet.in_use_by_positions",
                )));
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
        validate_cabinet_name(tx, &item.name, room_id, item.id).await?;
        if let Some(item_id) = item.id {
            sqlx::query(
                "UPDATE cabinets SET name = $1, capacity = $2, room_id = $3, updated_at = $4 WHERE id = $5",
            )
            .bind(&item.name)
            .bind(item.capacity)
            .bind(room_id)
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
            .bind(room_id)
            .bind(item.capacity)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }
    Ok(())
}

async fn validate_net_outlet_name(
    conn: &mut sqlx::PgConnection,
    name: &str,
    exclude_id: Option<Uuid>,
) -> Result<(), AppError> {
    // 信息点名称在所有房间范围内唯一（不再限定同一房间）
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM net_outlets WHERE name = $1 AND ($2::uuid IS NULL OR id != $2)",
    )
    .bind(name)
    .bind(exclude_id)
    .fetch_optional(conn)
    .await?;
    if existing.is_some() {
        return Err(AppError::Conflict(msg("server.net_outlet.name_exists")));
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
        return Err(AppError::Conflict(msg("server.workstation.name_exists")));
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
        return Err(AppError::Conflict(msg("server.cabinet.name_exists")));
    }
    Ok(())
}
