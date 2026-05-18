use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    ApiResponse, CabinetPosition, CabinetPositionCreate,
    CabinetPositionUpdate, CabinetPositionWithDetails, IpManager,
};
use crate::resource::ip::detect_ip_version;
use crate::utils::pagination::DEFAULT_PAGE;
use crate::utils::{log_system_operation, OperationLogParams, validate_ip_in_cidr, get_room_id_by_position};
use tracing::warn;
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

pub async fn get_positions(
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
    let cabinet_id = query
        .get("cabinet_id")
        .and_then(|id| Uuid::parse_str(id).ok());
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());
    let offset = (page - 1) * page_size;

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

    let total: i64 = if !search.is_empty() || cabinet_id.is_some() {
        if let Some(cid) = cabinet_id {
            if search.is_empty() {
                sqlx::query_scalar("SELECT COUNT(*) FROM positions WHERE cabinet_id = $1")
                    .bind(cid)
                    .fetch_one(&state.pool()?.get_conn())
                    .await?
            } else {
                let pattern = format!("%{search}%");
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM positions WHERE cabinet_id = $1 AND (name ILIKE $2 OR description ILIKE $2)"
                )
                .bind(cid)
                .bind(&pattern)
                .fetch_one(&state.pool()?.get_conn())
                .await?
            }
        } else {
            let pattern = format!("%{search}%");
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM positions WHERE name ILIKE $1 OR description ILIKE $1",
            )
            .bind(&pattern)
            .fetch_one(&state.pool()?.get_conn())
            .await?
        }
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM positions")
            .fetch_one(&state.pool()?.get_conn())
            .await?
    };

    let positions_basic = if !search.is_empty() || cabinet_id.is_some() {
        if let Some(cid) = cabinet_id {
            if search.is_empty() {
                sqlx::query(
                    &format!(
                        r"SELECT p.id, p.name, p.cabinet_id, 
                                  COALESCE((SELECT c.name FROM cabinets c WHERE c.id = p.cabinet_id), '未知机柜') as cabinet_name, 
                                  p.start_u, p.end_u, p.description, 
                                  p.device_type, p.device_id,
                                  p.created_at::TIMESTAMPTZ as created_at, p.updated_at::TIMESTAMPTZ as updated_at
                           FROM positions p 
                           WHERE p.cabinet_id = $1
                           {order_clause} LIMIT $2 OFFSET $3"
                    )
                )
                .bind(cid)
                .bind(page_size)
                .bind(offset)
                .fetch_all(&state.pool()?.get_conn())
                .await?
            } else {
                let pattern = format!("%{search}%");
                sqlx::query(
                    &format!(
                        r"SELECT p.id, p.name, p.cabinet_id, 
                                  COALESCE((SELECT c.name FROM cabinets c WHERE c.id = p.cabinet_id), '未知机柜') as cabinet_name, 
                                  p.start_u, p.end_u, p.description, 
                                  p.device_type, p.device_id,
                                  p.created_at::TIMESTAMPTZ as created_at, p.updated_at::TIMESTAMPTZ as updated_at
                           FROM positions p 
                           WHERE p.cabinet_id = $1 AND (p.name ILIKE $2 OR p.description ILIKE $2)
                           {order_clause} LIMIT $3 OFFSET $4"
                    )
                )
                .bind(cid)
                .bind(&pattern)
                .bind(page_size)
                .bind(offset)
                .fetch_all(&state.pool()?.get_conn())
                .await?
            }
        } else {
            let pattern = format!("%{search}%");
            sqlx::query(
                &format!(
                    r"SELECT p.id, p.name, p.cabinet_id, 
                              COALESCE((SELECT c.name FROM cabinets c WHERE c.id = p.cabinet_id), '未知机柜') as cabinet_name, 
                              p.start_u, p.end_u, p.description, 
                              p.device_type, p.device_id,
                              p.created_at::TIMESTAMPTZ as created_at, p.updated_at::TIMESTAMPTZ as updated_at
                       FROM positions p 
                       WHERE p.name ILIKE $1 OR p.description ILIKE $1
                       {order_clause} LIMIT $2 OFFSET $3"
                )
            )
            .bind(&pattern)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&state.pool()?.get_conn())
            .await?
        }
    } else {
        sqlx::query(
            &format!(
                r"SELECT p.id, p.name, p.cabinet_id, 
                          COALESCE((SELECT c.name FROM cabinets c WHERE c.id = p.cabinet_id), '未知机柜') as cabinet_name, 
                          p.start_u, p.end_u, p.description, 
                          p.device_type, p.device_id,
                          p.created_at::TIMESTAMPTZ as created_at, p.updated_at::TIMESTAMPTZ as updated_at
                   FROM positions p 
                   {order_clause} LIMIT $1 OFFSET $2"
            )
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?
    };

    let mut positions_with_details = Vec::new();

    for row in positions_basic {
        let id: Uuid = row.get("id");
        let name: String = row.get("name");
        let cabinet_id: Option<Uuid> = row.get("cabinet_id");
        let cabinet_name: Option<String> = row.get("cabinet_name");
        let start_u: i32 = row.get("start_u");
        let end_u: i32 = row.get("end_u");
        let device_type: Option<String> = row.get("device_type");
        let description: Option<String> = row.get("description");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");
        let device_id: Option<Uuid> = row.get("device_id");

        let position_with_details = CabinetPositionWithDetails {
            id,
            name,
            cabinet_id,
            cabinet_name,
            start_u,
            end_u,
            device_type,
            device_id,
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
        "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, device_type, device_id, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, 'cabinet_position', NULL, $7, $8)"
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

    let mut ip_count = 0;
    if let Some(ips) = &req.ips {
        let room_id = get_room_id_by_position(tx.as_mut(), id).await?;

        let network_id = if let Some(rid) = room_id {
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
            if device_type != "cabinet_position" || ip.position_id.is_some() {
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
            .bind(ip.workstation_id)
            .bind(Some(id))
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

    let position = CabinetPosition {
        id,
        name: req.name.clone(),
        cabinet_id: req.cabinet_id,
        start_u: req.start_u,
        end_u: req.end_u,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
        device_type: Some("cabinet_position".to_string()),
        device_id: None,
    };

    let details = serde_json::json!({
        "name": position.name,
        "cabinet_id": position.cabinet_id,
        "start_u": position.start_u,
        "end_u": position.end_u,
        "description": position.description,
        "ip_count": ip_count
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
            resource_type: "cabinet_position",
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
        r"SELECT id, name, cabinet_id, start_u, end_u, description, 
                  device_type, device_id,
                  created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ 
           FROM positions WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("机位未找到".to_string()))?;

    let device_type: Option<String> = position_data.get("device_type");
    let device_id: Option<Uuid> = position_data.get("device_id");
    let is_switch = device_type.as_deref() == Some("switch");

    let position_ips = if is_switch {
        sqlx::query(
            r"SELECT 
                m.id, m.workstation_id, m.position_id, m.switch_port_id,
                m.device_type, rn.network_id, 
                host(m.ip_address) as ip_address,
                m.ip_version, m.mac_address, m.hostname,
                m.status, m.last_seen, m.created_at, m.updated_at,
                n.network_region_id, nr.name as network_region
            FROM ips m
            LEFT JOIN positions p ON m.position_id = p.id
            LEFT JOIN cabinets c ON p.cabinet_id = c.id
            LEFT JOIN rooms r ON c.room_id = r.id
            LEFT JOIN room_networks rn ON r.id = rn.room_id
            LEFT JOIN network_cidrs n ON rn.network_id = n.id
            LEFT JOIN network_regions nr ON n.network_region_id = nr.id
            WHERE m.position_id = (SELECT p.id FROM positions p WHERE p.device_type = 'switch' AND p.device_id = $1)
            ORDER BY m.ip_address",
        )
        .bind(device_id)
        .fetch_all(&state.pool()?.get_conn())
        .await?
    } else {
        sqlx::query(
            r"SELECT 
                m.id, m.workstation_id, m.position_id, m.switch_port_id,
                m.device_type, rn.network_id, 
                host(m.ip_address) as ip_address,
                m.ip_version, m.mac_address, m.hostname,
                m.status, m.last_seen, m.created_at, m.updated_at,
                n.network_region_id, nr.name as network_region
            FROM ips m
            LEFT JOIN positions p ON m.position_id = p.id
            LEFT JOIN cabinets c ON p.cabinet_id = c.id
            LEFT JOIN rooms r ON c.room_id = r.id
            LEFT JOIN room_networks rn ON r.id = rn.room_id
            LEFT JOIN network_cidrs n ON rn.network_id = n.id
            LEFT JOIN network_regions nr ON n.network_region_id = nr.id
            WHERE m.position_id = $1
            ORDER BY m.ip_address",
        )
        .bind(id)
        .fetch_all(&state.pool()?.get_conn())
        .await?
    };

    let ips_with_region: Vec<serde_json::Value> = position_ips
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.get::<Uuid, _>(0),
                "workstation_id": row.get::<Option<Uuid>, _>(1),
                "position_id": row.get::<Option<Uuid>, _>(2),
                "switch_port_id": row.get::<Option<Uuid>, _>(3),
                "device_type": row.get::<Option<String>, _>(4),
                "network_id": row.get::<Uuid, _>(5),
                "ip_address": row.get::<String, _>(6),
                "ip_version": row.get::<i16, _>(7),
                "mac_address": row.get::<Option<String>, _>(8),
                "hostname": row.get::<Option<String>, _>(9),
                "status": row.get::<String, _>(10),
                "last_seen": row.get::<chrono::DateTime<chrono::Utc>, _>(11),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>(12),
                "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>(13),
                "network_region_id": row.get::<Option<Uuid>, _>(14),
                "network_region": row.get::<Option<String>, _>(15)
            })
        })
        .collect();

    let cabinet_name: String = if position_data.get::<Option<Uuid>, _>("cabinet_id").is_some() {
        let cabinet_id: Uuid = position_data.get("cabinet_id");
        sqlx::query_scalar::<_, String>("SELECT name FROM cabinets WHERE id = $1")
            .bind(cabinet_id)
            .fetch_optional(&state.pool()?.get_conn())
            .await?
            .unwrap_or_else(|| "未知机柜".to_string())
    } else {
        "未知机柜".to_string()
    };

    let position_with_details = serde_json::json!({
        "id": position_data.get::<Uuid, _>("id"),
        "name": position_data.get::<String, _>("name"),
        "cabinet_id": position_data.get::<Option<Uuid>, _>("cabinet_id"),
        "cabinet_name": cabinet_name,
        "start_u": position_data.get::<i32, _>("start_u"),
        "end_u": position_data.get::<i32, _>("end_u"),
        "device_type": device_type,
        "device_id": device_id,
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

    let position_info: Option<(Uuid, String, Option<Uuid>)> =
        sqlx::query_as::<_, (Uuid, String, Option<Uuid>)>(
            "SELECT id, device_type, device_id FROM positions WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

    let Some((_position_id, device_type, _device_id)) = position_info else {
        return Err(AppError::NotFound("机位未找到".to_string()));
    };

    let is_switch = device_type == "switch";

    if is_switch {
        return Err(AppError::Validation("交换机机位请通过交换机管理页面编辑".to_string()));
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

    if let Some(ips) = &req.ips {
        sqlx::query("DELETE FROM ips WHERE position_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        let room_id = get_room_id_by_position(tx.as_mut(), id).await?;

        let _network_id = if let Some(rid) = room_id {
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
                "INSERT INTO ips (id, position_id, device_type, ip_address, ip_version, mac_address, hostname, switch_port_id, status, last_seen, created_at, updated_at) 
                 VALUES ($1, $2, $3, CAST($4 AS INET), $5, $6, $7, $8, $9, $10, $11, $12)"
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(ip.device_type.as_deref().unwrap_or("cabinet_position"))
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
        "SELECT p.id, p.name, p.cabinet_id, c.name as cabinet_name, p.start_u, p.end_u, p.description, p.device_type, p.device_id, p.created_at::TIMESTAMPTZ, p.updated_at::TIMESTAMPTZ 
        FROM positions p 
        LEFT JOIN cabinets c ON p.cabinet_id = c.id 
        WHERE p.id = $1"
    ).bind(id)
    .fetch_one(&state.pool()?.get_conn()).await?;

    let ips: Vec<IpManager> = sqlx::query_as(
        r"SELECT id, workstation_id, position_id, switch_port_id, device_type,
           host(ip_address) as ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at, last_mac
           FROM ips WHERE position_id = $1"
    ).bind(id)
    .fetch_all(&state.pool()?.get_conn()).await?;

    let result = CabinetPositionWithDetails {
        id: row.get("id"),
        name: row.get("name"),
        cabinet_id: row.get("cabinet_id"),
        cabinet_name: row
            .get::<Option<String>, _>("cabinet_name"),
        start_u: row.get("start_u"),
        end_u: row.get("end_u"),
        device_type: row.get("device_type"),
        device_id: row.get("device_id"),
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
        let existing_switch: Option<Uuid> =
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM switches WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;

        if existing_switch.is_some() {
            return Err(AppError::Validation("该机位是交换机占用的位置，请通过交换机管理页面删除对应的交换机".to_string()));
        }

        return Err(AppError::NotFound("机位未找到".to_string()));
    }

    let has_switch: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM positions p WHERE p.id = $1 AND p.device_type = 'switch')"
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    ?;

    if has_switch {
        return Err(AppError::Validation("该机位已关联交换机，请通过删除交换机来删除机位".to_string()));
    }

    let device_type: Option<String> = sqlx::query_scalar("SELECT device_type FROM positions WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .flatten();

    if device_type.as_deref() == Some("switch") {
        return Err(AppError::Validation("该机位是交换机占用的位置，请通过交换机管理页面删除对应的交换机".to_string()));
    }

    sqlx::query("DELETE FROM ips WHERE position_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

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
            resource_id: &id,
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
