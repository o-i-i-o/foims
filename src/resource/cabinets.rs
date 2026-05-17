use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    ApiResponse, Cabinet, CabinetCreate, CabinetUpdate, CabinetWithNetworks, NetworkInfo,
};
use crate::utils::pagination::DEFAULT_PAGE;
use crate::utils::{log_system_operation, OperationLogParams};
use tracing::warn;
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

pub async fn get_cabinets(
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
    let room_id = query.get("room_id").cloned();
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
            &format!("SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets c {order_clause} LIMIT $1 OFFSET $2")
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, cabinets)
    } else if parsed_room_id.is_some() && search.is_empty() {
        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cabinets c WHERE c.room_id = $1")
                .bind(parsed_room_id)
                .fetch_one(&state.pool()?.get_conn())
                .await?;

        let cabinets = sqlx::query_as::<_, Cabinet>(
            &format!("SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets c WHERE c.room_id = $1 {order_clause} LIMIT $2 OFFSET $3")
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
            &format!("SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets c WHERE c.room_id = $1 AND (c.name ILIKE $2 OR c.description ILIKE $2) {order_clause} LIMIT $3 OFFSET $4")
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
            &format!("SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets c WHERE c.name ILIKE $1 OR c.description ILIKE $1 {order_clause} LIMIT $2 OFFSET $3")
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
                .await
                .unwrap_or(0);

        let room_name: Option<String> = sqlx::query_scalar("SELECT name FROM rooms WHERE id = $1")
            .bind(cabinet.room_id)
            .fetch_optional(&state.pool()?.get_conn())
            .await
            .unwrap_or(None);

        let cabinet_with_networks = CabinetWithNetworks {
            id: cabinet.id,
            name: cabinet.name,
            room_id: cabinet.room_id,
            room_name,
            capacity: cabinet.capacity,
            position_count,
            description: cabinet.description,
            created_at: cabinet.created_at,
            updated_at: cabinet.updated_at,
        };

        cabinets_with_networks.push(cabinet_with_networks);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": cabinets_with_networks,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "机柜获取成功",
    )))
}

pub async fn get_cabinets_by_network_region(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let region_id_str = path.into_inner();

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

    Ok(HttpResponse::Ok().json(ApiResponse::success(cabinets, "机柜获取成功")))
}

pub async fn create_cabinet(
    state: web::Data<AppState>,
    req: web::Json<CabinetCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    let existing_cabinet = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM cabinets WHERE name = $1 AND room_id = $2",
    )
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
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
            resource_type: "cabinet",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<Cabinet>::success(cabinet, "机柜创建成功")))
}

pub async fn get_cabinet(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let cabinet = sqlx::query_as::<_, Cabinet>(
        "SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets WHERE id = $1"
    ).bind(id)
    .fetch_optional(&state.pool()?.get_conn()).await?
    .ok_or_else(|| AppError::NotFound("机柜未找到".to_string()))?;

    let position_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM positions WHERE cabinet_id = $1")
            .bind(cabinet.id)
            .fetch_one(&state.pool()?.get_conn())
            .await
            .unwrap_or(0);

    let room_name: Option<String> = sqlx::query_scalar("SELECT name FROM rooms WHERE id = $1")
        .bind(cabinet.room_id)
        .fetch_optional(&state.pool()?.get_conn())
        .await
        .unwrap_or(None);

    let cabinet_with_networks = CabinetWithNetworks {
        id: cabinet.id,
        name: cabinet.name,
        room_id: cabinet.room_id,
        room_name,
        capacity: cabinet.capacity,
        position_count,
        description: cabinet.description,
        created_at: cabinet.created_at,
        updated_at: cabinet.updated_at,
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<CabinetWithNetworks>::success(
            cabinet_with_networks,
            "机柜获取成功",
        )),
    )
}

pub async fn update_cabinet(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<CabinetUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    (*req).validate()?;

    let existing_cabinet =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
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
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
            resource_type: "cabinet",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<Cabinet>::success(cabinet, "机柜更新成功")))
}

pub async fn delete_cabinet(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let existing_cabinet =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
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
        return Err(AppError::Validation("该机柜已被机位关联，无法删除".to_string()));
    }

    sqlx::query("DELETE FROM cabinets WHERE id = $1")
        .bind(id)
        .execute(&state.pool()?.get_conn())
        .await?;

    let details = serde_json::json!({
        "cabinet_id": id.to_string()
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "cabinet",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "机柜删除成功")))
}

pub async fn get_cabinet_networks(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let existing_cabinet =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
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
           AND r.room_type = 'DATA_CENTER'",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<NetworkInfo>>::success(
            cabinet_networks,
            "机柜网段获取成功",
        )),
    )
}
