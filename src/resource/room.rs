use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    ApiResponse, CabinetBrief, NetworkInfo, Room, RoomChildrenSync, RoomCreate, RoomUpdate,
    RoomWithNetworks, WorkstationBrief,
};
use crate::utils::pagination::Pagination;
use crate::utils::{OperationLogParams, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use tracing::warn;
use uuid::Uuid;
use validator::Validate;

pub async fn get_rooms(
    state: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
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

    let search_pattern = format!("%{search}%");

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

    let mut rooms_with_networks = Vec::new();

    for room in rooms {
        let room_networks = sqlx::query_as::<_, NetworkInfo>(
            r"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr
               FROM room_networks rn
               JOIN network_cidrs n ON rn.network_id = n.id
               JOIN network_regions nr ON n.network_region_id = nr.id
               WHERE rn.room_id = $1",
        )
        .bind(room.id)
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

        let room_with_networks = RoomWithNetworks {
            id: room.id,
            name: room.name,
            room_type: room.room_type.clone(),
            org_id: room.org_id,
            org_name,
            description: room.description,
            networks: room_networks,
            workstation_count,
            workstations: None,
            cabinets: None,
            created_at: room.created_at,
            updated_at: room.updated_at,
        };

        rooms_with_networks.push(room_with_networks);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": rooms_with_networks,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "房间获取成功",
    )))
}

pub async fn create_room(
    state: web::Data<AppState>,
    req: web::Json<RoomCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    let existing_room = sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE name = $1")
        .bind(&req.name)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

    if existing_room.is_some() {
        return Err(AppError::Conflict("房间名称已存在".to_string()));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

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
    .execute(&state.pool()?.get_conn())
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
        .execute(&state.pool()?.get_conn())
        .await?;
    }

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
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
            resource_type: "room",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<Room>::success(room, "房间创建成功")))
}

pub async fn get_room(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

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
        created_at: room.created_at,
        updated_at: room.updated_at,
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<RoomWithNetworks>::success(
            room_with_networks,
            "房间获取成功",
        )),
    )
}

pub async fn update_room(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<RoomUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    (*req).validate()?;

    let existing_room = sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

    if existing_room.is_none() {
        return Err(AppError::NotFound("房间未找到".to_string()));
    }

    let now = Utc::now();

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
    .execute(&state.pool()?.get_conn())
    .await?;

    if let Some(network_ids) = &req.network_ids {
        sqlx::query("DELETE FROM room_networks WHERE room_id = $1")
            .bind(id)
            .execute(&state.pool()?.get_conn())
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
            .execute(&state.pool()?.get_conn())
            .await?;
        }
    }

    let room = sqlx::query_as::<_, Room>(
        "SELECT id, name, room_type, org_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms WHERE id = $1"
    ).bind(id)
    .fetch_one(&state.pool()?.get_conn()).await?;

    let details = serde_json::json!({
        "name": room.name,
        "room_type": room.room_type,
        "description": room.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
            resource_type: "room",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<Room>::success(room, "房间更新成功")))
}

pub async fn delete_room(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

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

    sqlx::query("DELETE FROM rooms WHERE id = $1")
        .bind(id)
        .execute(&state.pool()?.get_conn())
        .await?;

    let details = serde_json::json!({
        "room_id": id.to_string()
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "room",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "房间删除成功")))
}

pub async fn get_room_networks(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

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

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<NetworkInfo>>::success(
            room_networks,
            "房间网段获取成功",
        )),
    )
}

pub async fn sync_room_children(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<RoomChildrenSync>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    (*req).validate()?;

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
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "sync_children",
            resource_type: "room",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "房间子项同步成功")))
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
