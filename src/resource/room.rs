use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{ApiResponse, NetworkInfo, Room, RoomCreate, RoomUpdate, RoomWithNetworks};
use crate::utils::pagination::DEFAULT_PAGE;
use crate::utils::log_system_operation;
use tracing::warn;
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

pub async fn get_rooms(
    state: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let page: i64 = query
        .get("page")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PAGE);
    let page_size: i64 = query
        .get("page_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let search = query.get("search").cloned().unwrap_or_default();
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());
    let offset = (page - 1) * page_size;

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
            &format!("SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms {order_clause} LIMIT $1 OFFSET $2")
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
            &format!("SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms WHERE name ILIKE $1 OR room_type ILIKE $1 OR description ILIKE $1 {order_clause} LIMIT $2 OFFSET $3")
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
                .await
                .unwrap_or(0);

        let room_with_networks = RoomWithNetworks {
            id: room.id,
            name: room.name,
            room_type: room.room_type,
            description: room.description,
            networks: room_networks,
            workstation_count,
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
        "INSERT INTO rooms (id, name, room_type, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.room_type)
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
        &http_req,
        &state.config,
        "create",
        "room",
        &id,
        &details,
        true,
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<Room>::success(room, "房间创建成功")))
}

pub async fn get_room(state: web::Data<AppState>, id_path: web::Path<Uuid>) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let room = sqlx::query_as::<_, Room>(
        "SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms WHERE id = $1"
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
            .await
            .unwrap_or(0);

    let room_with_networks = RoomWithNetworks {
        id: room.id,
        name: room.name,
        room_type: room.room_type,
        description: room.description,
        networks: room_networks,
        workstation_count,
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
         description = COALESCE($3, description),
         updated_at = $4
         WHERE id = $5",
    )
    .bind(&req.name)
    .bind(&req.room_type)
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
        "SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms WHERE id = $1"
    ).bind(id)
    .fetch_one(&state.pool()?.get_conn()).await?;

    let details = serde_json::json!({
        "name": room.name,
        "room_type": room.room_type,
        "description": room.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        &http_req,
        &state.config,
        "update",
        "room",
        &id,
        &details,
        true,
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
        return Err(AppError::Validation("该房间已被机柜关联，无法删除".to_string()));
    }

    let workstation_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workstations WHERE room_id = $1")
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    if workstation_count > 0 {
        return Err(AppError::Validation("该房间已被工位关联，无法删除".to_string()));
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
        &http_req,
        &state.config,
        "delete",
        "room",
        &id,
        &details,
        true,
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
