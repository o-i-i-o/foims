use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    ApiResponse, CabinetPosition, CabinetPositionCreate, CabinetPositionUpdate,
    CabinetPositionWithDetails, IpManager,
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

pub async fn get_positions(
    state: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;
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

    let mut conditions: Vec<String> = Vec::new();
    let mut param_idx = 1;

    if !search.is_empty() {
        conditions.push(format!(
            "(p.name ILIKE ${param_idx} OR p.description ILIKE ${param_idx})"
        ));
        param_idx += 1;
    }
    if cabinet_id.is_some() {
        conditions.push(format!("p.cabinet_id = ${param_idx}"));
        param_idx += 1;
    }
    if room_id.is_some() {
        conditions.push(format!("c.room_id = ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let search_pattern = format!("%{search}%");
    let limit_idx = param_idx;
    let offset_idx = param_idx + 1;

    let count_sql = sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM positions p LEFT JOIN cabinets c ON p.cabinet_id = c.id {where_clause}"
    ));
    let data_sql = sqlx::AssertSqlSafe(format!(
        r"SELECT p.id, p.name, p.cabinet_id,
                  (SELECT c2.name FROM cabinets c2 WHERE c2.id = p.cabinet_id) as cabinet_name,
                  c.room_id,
                  (SELECT r.name FROM rooms r WHERE r.id = c.room_id) as room_name,
                  p.start_u, p.end_u, p.description,
                  p.created_at::TIMESTAMPTZ as created_at, p.updated_at::TIMESTAMPTZ as updated_at
           FROM positions p
           LEFT JOIN cabinets c ON p.cabinet_id = c.id
           {where_clause}
           {order_clause} LIMIT ${limit_idx} OFFSET ${offset_idx}"
    ));

    let mut count_query = sqlx::query_scalar::<_, i64>(count_sql);
    let mut data_query = sqlx::query(data_sql);

    if !search.is_empty() {
        count_query = count_query.bind(&search_pattern);
        data_query = data_query.bind(&search_pattern);
    }
    if let Some(cid) = cabinet_id {
        count_query = count_query.bind(cid);
        data_query = data_query.bind(cid);
    }
    if let Some(rid) = room_id {
        count_query = count_query.bind(rid);
        data_query = data_query.bind(rid);
    }

    count_query = count_query.bind(page_size).bind(offset);
    data_query = data_query.bind(page_size).bind(offset);

    let total: i64 = count_query.fetch_one(&state.pool()?.get_conn()).await?;
    let positions_basic = data_query.fetch_all(&state.pool()?.get_conn()).await?;

    let mut positions_with_details = Vec::new();

    for row in positions_basic {
        let id: Uuid = row.get("id");
        let name: String = row.get("name");
        let cabinet_id: Option<Uuid> = row.get("cabinet_id");
        let cabinet_name: Option<String> = row.get("cabinet_name");
        let room_id: Option<Uuid> = row.get("room_id");
        let room_name: Option<String> = row.get("room_name");
        let start_u: i32 = row.get("start_u");
        let end_u: i32 = row.get("end_u");
        let description: Option<String> = row.get("description");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

        let position_with_details = CabinetPositionWithDetails {
            id,
            name,
            cabinet_id,
            cabinet_name,
            room_id,
            room_name,
            start_u,
            end_u,
            ips: Vec::new(),
            description,
            created_at,
            updated_at,
        };

        positions_with_details.push(position_with_details);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": positions_with_details,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "机位获取成功",
    )))
}

pub async fn create_cabinet_position(
    state: web::Data<AppState>,
    req: web::Json<CabinetPositionCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_position: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM positions WHERE name = $1 AND cabinet_id = $2",
    )
    .bind(&req.name)
    .bind(req.cabinet_id)
    .fetch_optional(&mut *tx)
    .await?;

    if existing_position.is_some() {
        return Err(AppError::Conflict("机位名称已存在".to_string()));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
    )
    .bind(id)
    .bind(&req.name)
    .bind(req.cabinet_id)
    .bind(req.start_u)
    .bind(req.end_u)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx).await?;

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
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
            resource_type: "cabinet_position",
            resource_id: Some(&id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::<CabinetPosition>::success(
            position,
            "机位创建成功",
        )),
    )
}

pub async fn get_cabinet_position(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let position_data = sqlx::query(
        r"SELECT p.id, p.name, p.cabinet_id, p.start_u, p.end_u, p.description,
                  p.created_at::TIMESTAMPTZ, p.updated_at::TIMESTAMPTZ,
                  c.room_id
           FROM positions p
           LEFT JOIN cabinets c ON p.cabinet_id = c.id
           WHERE p.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("机位未找到".to_string()))?;

    let position_ips = sqlx::query(
        r"SELECT
            m.id, m.device_interface_id, m.device_id, m.network_id,
            host(m.ip_address) as ip_address,
            m.ip_version, m.mac_address, m.hostname, m.description,
            m.status, m.last_seen, m.created_at, m.updated_at,
            n.network_region_id, nr.name as network_region
        FROM ips m
        JOIN devices d ON m.device_id = d.id
        LEFT JOIN network_cidrs n ON m.network_id = n.id
        LEFT JOIN network_regions nr ON n.network_region_id = nr.id
        WHERE d.position_id = $1
        ORDER BY m.ip_address",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let ips_with_region: Vec<serde_json::Value> = position_ips
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.get::<Uuid, _>(0),
                "device_interface_id": row.get::<Uuid, _>(1),
                "device_id": row.get::<Uuid, _>(2),
                "network_id": row.get::<Option<Uuid>, _>(3),
                "ip_address": row.get::<String, _>(4),
                "ip_version": row.get::<i16, _>(5),
                "mac_address": row.get::<Option<String>, _>(6),
                "hostname": row.get::<Option<String>, _>(7),
                "description": row.get::<Option<String>, _>(8),
                "status": row.get::<String, _>(9),
                "last_seen": row.get::<chrono::DateTime<chrono::Utc>, _>(10),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>(11),
                "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>(12),
                "network_region_id": row.get::<Option<Uuid>, _>(13),
                "network_region": row.get::<Option<String>, _>(14)
            })
        })
        .collect();

    let cabinet_name: Option<String> =
        if position_data.get::<Option<Uuid>, _>("cabinet_id").is_some() {
            let cabinet_id: Uuid = position_data.get("cabinet_id");
            sqlx::query_scalar::<_, String>("SELECT name FROM cabinets WHERE id = $1")
                .bind(cabinet_id)
                .fetch_optional(&state.pool()?.get_conn())
                .await?
        } else {
            None
        };

    let position_with_details = serde_json::json!({
        "id": position_data.get::<Uuid, _>("id"),
        "name": position_data.get::<String, _>("name"),
        "cabinet_id": position_data.get::<Option<Uuid>, _>("cabinet_id"),
        "cabinet_name": cabinet_name,
        "room_id": position_data.get::<Option<Uuid>, _>("room_id"),
        "start_u": position_data.get::<i32, _>("start_u"),
        "end_u": position_data.get::<i32, _>("end_u"),
        "ips": ips_with_region,
        "description": position_data.get::<Option<String>, _>("description"),
        "created_at": position_data.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updated_at": position_data.get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
    });

    Ok(
        HttpResponse::Ok().json(ApiResponse::<serde_json::Value>::success(
            position_with_details,
            "机位获取成功",
        )),
    )
}

pub async fn update_cabinet_position(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<CabinetPositionUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    (*req).validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let position_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM positions WHERE id = $1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    if !position_exists {
        return Err(AppError::NotFound("机位未找到".to_string()));
    }

    let now = Utc::now();

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
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let row = sqlx::query(
        "SELECT p.id, p.name, p.cabinet_id, c.name as cabinet_name, c.room_id, r.name as room_name, p.start_u, p.end_u, p.description, p.created_at::TIMESTAMPTZ, p.updated_at::TIMESTAMPTZ 
        FROM positions p 
        LEFT JOIN cabinets c ON p.cabinet_id = c.id 
        LEFT JOIN rooms r ON c.room_id = r.id 
        WHERE p.id = $1"
    ).bind(id)
    .fetch_one(&state.pool()?.get_conn()).await?;

    let ips: Vec<IpManager> = sqlx::query_as(
        r"SELECT m.id, m.device_interface_id, m.device_id, m.network_id,
           host(m.ip_address) as ip_address, m.ip_version, m.mac_address, m.hostname, m.description,
           m.status, m.last_seen, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ, m.last_mac
           FROM ips m JOIN devices d ON m.device_id = d.id WHERE d.position_id = $1",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let result = CabinetPositionWithDetails {
        id: row.get("id"),
        name: row.get("name"),
        cabinet_id: row.get("cabinet_id"),
        cabinet_name: row.get::<Option<String>, _>("cabinet_name"),
        room_id: row.get("room_id"),
        room_name: row.get("room_name"),
        start_u: row.get("start_u"),
        end_u: row.get("end_u"),
        ips,
        description: row.get("description"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };

    let details = serde_json::json!({
        "name": result.name,
        "cabinet_id": result.cabinet_id,
        "start_u": result.start_u,
        "end_u": result.end_u,
        "description": result.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
            resource_type: "cabinet_position",
            resource_id: Some(&id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::<CabinetPositionWithDetails>::success(
            result,
            "机位更新成功",
        )),
    )
}

pub async fn delete_cabinet_position(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_position: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM positions WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing_position.is_none() {
        return Err(AppError::NotFound("机位未找到".to_string()));
    }

    let device_using_position: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM devices WHERE position_id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if device_using_position.is_some() {
        return Err(AppError::Validation(
            "该机位被设备占用，请先删除对应的设备".to_string(),
        ));
    }

    sqlx::query("DELETE FROM positions WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "position_id": id.to_string()
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "cabinet_position",
            resource_id: Some(&id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "机位删除成功")))
}
