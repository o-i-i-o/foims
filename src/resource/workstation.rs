use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    ApiResponse, IpManager, Workstation, WorkstationCreate,
    WorkstationUpdate, WorkstationWithDetails,
};
use crate::resource::ip::detect_ip_version;
use crate::utils::pagination::DEFAULT_PAGE;
use crate::utils::{log_system_operation, OperationLogParams, validate_ip_in_cidr, get_room_id_by_workstation};
use tracing::warn;
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

pub async fn get_workstations(
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
        ("name", "desc") => "ORDER BY w.name DESC",
        ("room_name", "desc") => "ORDER BY room_name DESC, w.name ASC",
        ("room_name", _) => "ORDER BY room_name ASC, w.name ASC",
        ("manager", "desc") => "ORDER BY w.manager DESC, w.name ASC",
        ("manager", _) => "ORDER BY w.manager ASC, w.name ASC",
        ("created_at", "desc") => "ORDER BY w.created_at DESC",
        ("created_at", _) => "ORDER BY w.created_at ASC",
        _ => "ORDER BY w.name ASC",
    };

    let (total, workstations_basic) = if search.is_empty() && parsed_room_id.is_none() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workstations w")
            .fetch_one(&state.pool()?.get_conn())
            .await?;

        let workstations_basic = sqlx::query(&format!(
            "SELECT w.id, w.name, w.room_id,
                    COALESCE((SELECT r.name FROM rooms r WHERE r.id = w.room_id), '未知房间') as room_name,
                    w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ
             FROM workstations w {order_clause} LIMIT $1 OFFSET $2"
        ))
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, workstations_basic)
    } else if parsed_room_id.is_some() && search.is_empty() {
        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM workstations w WHERE w.room_id = $1")
                .bind(parsed_room_id)
                .fetch_one(&state.pool()?.get_conn())
                .await?;

        let workstations_basic = sqlx::query(&format!(
            "SELECT w.id, w.name, w.room_id,
                    COALESCE((SELECT r.name FROM rooms r WHERE r.id = w.room_id), '未知房间') as room_name,
                    w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ
             FROM workstations w WHERE w.room_id = $1 {order_clause} LIMIT $2 OFFSET $3"
        ))
        .bind(parsed_room_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, workstations_basic)
    } else if parsed_room_id.is_some() {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM workstations w WHERE w.room_id = $1 AND (w.name ILIKE $2 OR w.manager ILIKE $2 OR w.description ILIKE $2)"
        )
        .bind(parsed_room_id)
        .bind(&search_pattern)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

        let workstations_basic = sqlx::query(&format!(
            "SELECT w.id, w.name, w.room_id,
                    COALESCE((SELECT r.name FROM rooms r WHERE r.id = w.room_id), '未知房间') as room_name,
                    w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ
             FROM workstations w WHERE w.room_id = $1 AND (w.name ILIKE $2 OR w.manager ILIKE $2 OR w.description ILIKE $2) {order_clause} LIMIT $3 OFFSET $4"
        ))
        .bind(parsed_room_id)
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, workstations_basic)
    } else {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM workstations w WHERE w.name ILIKE $1 OR w.manager ILIKE $1 OR w.description ILIKE $1"
        )
        .bind(&search_pattern)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

        let workstations_basic = sqlx::query(&format!(
            "SELECT w.id, w.name, w.room_id,
                    COALESCE((SELECT r.name FROM rooms r WHERE r.id = w.room_id), '未知房间') as room_name,
                    w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ
             FROM workstations w WHERE w.name ILIKE $1 OR w.manager ILIKE $1 OR w.description ILIKE $1 {order_clause} LIMIT $2 OFFSET $3"
        ))
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, workstations_basic)
    };

    let mut workstation_ids = Vec::new();
    for row in &workstations_basic {
        let id: Uuid = row.get("id");
        workstation_ids.push(id);
    }

    let mut workstations_with_details = Vec::new();

    for row in workstations_basic {
        let id: Uuid = row.get("id");
        let name: String = row.get("name");
        let room_id: Uuid = row
            .get::<Option<Uuid>, _>("room_id")
            .unwrap_or_else(Uuid::nil);
        let room_name: String = row
            .get::<Option<String>, _>("room_name")
            .unwrap_or_else(|| "未知房间".to_string());
        let manager: Option<String> = row.get("manager");
        let description: Option<String> = row.get("description");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

        let workstation_with_details = WorkstationWithDetails {
            id,
            name,
            room_id,
            room_name,
            manager,
            ips: Vec::new(),
            description,
            created_at,
            updated_at,
        };

        workstations_with_details.push(workstation_with_details);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": workstations_with_details,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "工位获取成功",
    )))
}

pub async fn create_workstation(
    state: web::Data<AppState>,
    req: web::Json<WorkstationCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_workstation: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM workstations WHERE name = $1 AND room_id = $2",
    )
    .bind(&req.name)
    .bind(req.room_id)
    .fetch_optional(&mut *tx)
    .await?;

    if existing_workstation.is_some() {
        return Err(AppError::Conflict("工位名称已存在".to_string()));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO workstations (id, name, room_id, manager, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(id)
    .bind(&req.name)
    .bind(req.room_id)
    .bind(&req.manager)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx).await?;

    let mut ip_count = 0;
    if let Some(ips) = &req.ips {
        let network_id = {
            let room_network: Option<Uuid> = sqlx::query_scalar(
                "SELECT network_id FROM room_networks WHERE room_id = $1 LIMIT 1"
            )
            .bind(req.room_id)
            .fetch_optional(&mut *tx)
            .await?;
            room_network
        };

        let network = if let Some(nid) = network_id {
            sqlx::query(crate::utils::NETWORK_QUERY)
                .bind(nid)
                .fetch_optional(&mut *tx)
                .await?
                .map(|row| crate::utils::parse_network_from_row(&row))
        } else {
            None
        };

        for ip in ips {
            let device_type = ip.device_type.as_deref().unwrap_or("");
            if device_type != "workstation" || ip.workstation_id.is_some() {
                return Err(AppError::Validation("设备类型与设备ID不匹配".to_string()));
            }

            let existing_mapping: Option<Uuid> =
                sqlx::query_scalar::<_, Uuid>(
                    "SELECT id FROM ips WHERE ip_address = CAST($1 AS INET)",
                )
                .bind(&ip.ip_address)
                .fetch_optional(&mut *tx)
                .await?;

            if existing_mapping.is_some() {
                return Err(AppError::Conflict("IP地址已存在".to_string()));
            }

            if let Some(ref net) = network {
                let ip_in_cidr = validate_ip_in_cidr(&ip.ip_address, net)?;
                if !ip_in_cidr {
                    return Err(AppError::Validation("IP地址不在所属房间网段内".to_string()));
                }
            }

            let ip_version = detect_ip_version(&ip.ip_address)?;

            sqlx::query(
                "INSERT INTO ips (id, workstation_id, position_id, switch_port_id, device_type, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, CAST($6 AS INET), $7, $8, $9, $10, $11, $12, $13)"
            )
            .bind(Uuid::new_v4())
            .bind(Some(id))
            .bind(ip.position_id)
            .bind(ip.switch_port_id)
            .bind(&ip.device_type)
            .bind(&ip.ip_address)
            .bind(ip_version)
            .bind(&ip.mac_address)
            .bind(&ip.hostname)
            .bind("active")
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&mut *tx).await?;

            ip_count += 1;
        }
    }

    tx.commit().await?;

    let workstation = Workstation {
        id,
        name: req.name.clone(),
        room_id: req.room_id,
        room_name: None,
        manager: req.manager.clone(),
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": workstation.name,
        "room_id": workstation.room_id,
        "manager": workstation.manager,
        "description": workstation.description,
        "ip_count": ip_count
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
            resource_type: "workstation",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<Workstation>::success(
        workstation,
        "工位创建成功",
    )))
}

pub async fn get_workstation(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let workstation = sqlx::query_as::<_, Workstation>(
        "SELECT w.id, w.name, w.room_id, r.name as room_name, w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ FROM workstations w LEFT JOIN rooms r ON w.room_id = r.id WHERE w.id = $1"
    ).bind(id)
    .fetch_optional(&state.pool()?.get_conn()).await?
    .ok_or_else(|| AppError::NotFound("工位未找到".to_string()))?;

    let workstation_ips = sqlx::query_as::<_, IpManager>(
        r"SELECT
            m.id, m.workstation_id, m.position_id, m.switch_port_id,
            m.device_type, rn.network_id,
            host(m.ip_address) as ip_address,
            m.ip_version, m.mac_address, m.hostname,
            m.status, m.last_seen, m.created_at, m.updated_at, m.last_mac
        FROM ips m
        LEFT JOIN workstations w ON m.workstation_id = w.id
        LEFT JOIN rooms r ON w.room_id = r.id
        LEFT JOIN LATERAL (
            SELECT network_id FROM room_networks 
            WHERE room_id = r.id 
            LIMIT 1
        ) rn ON true
        WHERE m.workstation_id = $1
        ORDER BY m.ip_address",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let room_name = sqlx::query_scalar::<_, String>("SELECT COALESCE((SELECT r.name FROM rooms r JOIN workstations w ON r.id = w.room_id WHERE w.id = $1), '未知房间')")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .unwrap_or_else(|| "未知房间".to_string());

    let workstation_with_details = WorkstationWithDetails {
        id: workstation.id,
        name: workstation.name,
        room_id: workstation.room_id,
        room_name,
        manager: workstation.manager.clone(),
        ips: workstation_ips,
        description: workstation.description,
        created_at: workstation.created_at,
        updated_at: workstation.updated_at,
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<WorkstationWithDetails>::success(
            workstation_with_details,
            "工位获取成功",
        )),
    )
}

pub async fn update_workstation(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<WorkstationUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    (*req).validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_workstation: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM workstations WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing_workstation.is_none() {
        return Err(AppError::NotFound("工位未找到".to_string()));
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE workstations SET
         name = COALESCE($1, name),
         room_id = COALESCE($2, room_id),
         manager = COALESCE($3, manager),
         description = COALESCE($4, description),
         updated_at = $5
         WHERE id = $6",
    )
    .bind(&req.name)
    .bind(req.room_id)
    .bind(&req.manager)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    if let Some(ips) = &req.ips {
        sqlx::query("DELETE FROM ips WHERE workstation_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        let ws_room_id = get_room_id_by_workstation(tx.as_mut(), id).await?;

        let _network_id = if let Some(rid) = ws_room_id {
            let room_network: Option<Uuid> = sqlx::query_scalar(
                "SELECT network_id FROM room_networks WHERE room_id = $1 LIMIT 1"
            )
            .bind(rid)
            .fetch_optional(&mut *tx)
            .await?;
            room_network
        } else {
            None
        };

        for ip in ips {
            let ip_version = detect_ip_version(&ip.ip_address)?;

            sqlx::query(
                "INSERT INTO ips (id, workstation_id, device_type, ip_address, ip_version, mac_address, hostname, switch_port_id, status, last_seen, created_at, updated_at)
                 VALUES ($1, $2, $3, CAST($4 AS INET), $5, $6, $7, $8, $9, $10, $11, $12)"
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(ip.device_type.as_deref().unwrap_or("workstation"))
            .bind(&ip.ip_address)
            .bind(ip_version)
            .bind(&ip.mac_address)
            .bind(&ip.hostname)
            .bind(ip.switch_port_id)
            .bind("active")
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&mut *tx).await?;
        }
    }

    tx.commit().await?;

    let row = sqlx::query(
        r"SELECT w.id, w.name, w.room_id, r.name as room_name, w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ
        FROM workstations w
        LEFT JOIN rooms r ON w.room_id = r.id
        WHERE w.id = $1"
    ).bind(id)
    .fetch_one(&state.pool()?.get_conn()).await?;

    let ips: Vec<IpManager> = sqlx::query_as(
        r"SELECT id, workstation_id, position_id, switch_port_id, device_type,
           host(ip_address) as ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at, last_mac
           FROM ips WHERE workstation_id = $1"
    ).bind(id)
    .fetch_all(&state.pool()?.get_conn()).await?;

    let result = WorkstationWithDetails {
        id: row.get("id"),
        name: row.get("name"),
        room_id: row.get("room_id"),
        room_name: row
            .get::<Option<String>, _>("room_name")
            .unwrap_or_default(),
        manager: row.get("manager"),
        ips,
        description: row.get("description"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };

    let details = serde_json::json!({
        "name": result.name,
        "room_id": result.room_id,
        "manager": result.manager,
        "description": result.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
            resource_type: "workstation",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::<WorkstationWithDetails>::success(
            result,
            "工位更新成功",
        )),
    )
}

pub async fn delete_workstation(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_workstation: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM workstations WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing_workstation.is_none() {
        return Err(AppError::NotFound("工位未找到".to_string()));
    }

    sqlx::query("DELETE FROM ips WHERE workstation_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

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
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "workstation",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "工位删除成功")))
}
