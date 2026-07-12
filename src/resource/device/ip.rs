use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{ApiResponse, IpManager, IpManagerCreate};
use crate::resource::ip::detect_ip_version;
use crate::utils::{OperationLogParams, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use tracing::warn;
use uuid::Uuid;
use validator::Validate;

pub async fn get_device_ips(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    // Verify device exists
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
        .bind(id)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    if !exists {
        return Err(AppError::NotFound("设备未找到".to_string()));
    }

    let ips: Vec<IpManager> = sqlx::query_as(
        r"SELECT
            m.id, m.workstation_id, m.position_id, m.device_port_id,
            m.device_id, m.device_type, m.network_id,
            host(m.ip_address) as ip_address,
            m.ip_version, m.mac_address, m.hostname,
            m.status, m.last_seen, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ, m.last_mac
        FROM ips m
        WHERE m.device_id = $1
        ORDER BY m.ip_address",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({ "items": ips }),
        "设备IP列表获取成功",
    )))
}

pub async fn create_device_ip(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<IpManagerCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    (*req).validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    // Verify device exists and get its location info
    let device_row = sqlx::query("SELECT workstation_id, position_id FROM devices WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("设备未找到".to_string()))?;

    let device_ws_id: Option<Uuid> = device_row.get("workstation_id");
    let device_pos_id: Option<Uuid> = device_row.get("position_id");

    // Check for duplicate IP
    let existing_ip: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM ips WHERE ip_address = CAST($1 AS INET)")
            .bind(&req.ip_address)
            .fetch_optional(&mut *tx)
            .await?;

    if existing_ip.is_some() {
        return Err(AppError::Conflict("IP地址已存在".to_string()));
    }

    // Determine room_id for network validation
    let room_id = if let Some(ws_id) = device_ws_id {
        sqlx::query_scalar::<_, Uuid>("SELECT room_id FROM workstations WHERE id = $1")
            .bind(ws_id)
            .fetch_optional(&mut *tx)
            .await?
    } else if let Some(pos_id) = device_pos_id {
        sqlx::query_scalar::<_, Option<Uuid>>(
            "SELECT c.room_id FROM positions p LEFT JOIN cabinets c ON p.cabinet_id = c.id WHERE p.id = $1",
        )
        .bind(pos_id)
        .fetch_optional(&mut *tx)
        .await?
        .flatten()
    } else {
        None
    };

    // Find network_id from room networks
    let network_id: Option<Uuid> = if let Some(r_id) = room_id {
        sqlx::query_scalar(
            r"SELECT nc.id
            FROM room_networks rn
            JOIN network_cidrs nc ON rn.network_id = nc.id
            WHERE rn.room_id = $1
            AND (
                (nc.ipv4_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv4_cidr::inet)
                OR (nc.ipv6_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv6_cidr::inet)
            )
            LIMIT 1",
        )
        .bind(r_id)
        .bind(&req.ip_address)
        .fetch_optional(&mut *tx)
        .await?
    } else {
        req.network_id
    };

    let ip_version = detect_ip_version(&req.ip_address)?;

    // Determine ip device_type
    let ip_device_type = if device_ws_id.is_some() || device_pos_id.is_some() {
        "device"
    } else {
        req.device_type.as_deref().unwrap_or("device")
    };

    let now = Utc::now();
    let ip_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO ips (id, workstation_id, position_id, device_port_id, device_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)",
    )
    .bind(ip_id)
    .bind(device_ws_id)
    .bind(device_pos_id)
    .bind(req.device_port_id)
    .bind(Some(id))
    .bind(ip_device_type)
    .bind(network_id)
    .bind(&req.ip_address)
    .bind(ip_version)
    .bind(&req.mac_address)
    .bind(&req.hostname)
    .bind("active")
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let mapping = IpManager {
        id: ip_id,
        workstation_id: device_ws_id,
        position_id: device_pos_id,
        device_port_id: req.device_port_id,
        device_id: Some(id),
        device_type: Some(ip_device_type.to_string()),
        network_id,
        ip_address: req.ip_address.clone(),
        ip_version,
        mac_address: req.mac_address.clone(),
        hostname: req.hostname.clone(),
        status: "active".to_string(),
        last_seen: now,
        last_mac: None,
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "device_id": id.to_string(),
        "ip_address": mapping.ip_address,
        "mac_address": mapping.mac_address,
        "hostname": mapping.hostname
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create_device_ip",
            resource_type: "device",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(mapping, "设备IP创建成功")))
}

pub async fn auto_assign_device_ip(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<crate::models::AutoAssignIpRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    req.validate()?;

    let req_network_id = req.network_id;

    let mut tx = state.pool()?.get_conn().begin().await?;

    // Verify device exists and get its location info
    let device_row = sqlx::query("SELECT workstation_id, position_id FROM devices WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("设备未找到".to_string()))?;

    let device_ws_id: Option<Uuid> = device_row.get("workstation_id");
    let device_pos_id: Option<Uuid> = device_row.get("position_id");

    // Device-associated IPs always use "device" as device_type
    let device_type = "device".to_string();

    // Determine room_id for network validation
    let room_id = if let Some(ws_id) = device_ws_id {
        crate::utils::get_room_id_by_workstation(&mut *tx, ws_id).await?
    } else if let Some(pos_id) = device_pos_id {
        crate::utils::get_room_id_by_position(&mut *tx, pos_id).await?
    } else {
        None
    };

    if let Some(r_id) = room_id {
        crate::utils::validate_network_in_room(&mut *tx, r_id, Some(req_network_id)).await?;
    }

    let network = sqlx::query(crate::utils::NETWORK_QUERY)
        .bind(req_network_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("网络未找到".to_string()))
        .and_then(|row| crate::utils::parse_network_from_row(&row))?;

    let used_ips: Vec<String> =
        sqlx::query_scalar("SELECT host(ip_address) FROM ips WHERE network_id = $1")
            .bind(req_network_id)
            .fetch_all(&mut *tx)
            .await?;

    let used_set: std::collections::HashSet<String> = used_ips.into_iter().collect();

    let assigned_ip = network
        .ipv4_cidr
        .as_ref()
        .and_then(|cidr| {
            crate::resource::ip::find_available_ips_in_cidr(
                cidr,
                network.ipv4_gateway.as_ref(),
                &used_set,
                Some(1),
            )
            .into_iter()
            .next()
        })
        .or_else(|| {
            network.ipv6_cidr.as_ref().and_then(|cidr| {
                crate::resource::ip::find_available_ips_in_cidr(
                    cidr,
                    network.ipv6_gateway.as_ref(),
                    &used_set,
                    Some(1),
                )
                .into_iter()
                .next()
            })
        })
        .ok_or_else(|| AppError::Validation("该网络没有可用的IP地址".to_string()))?;

    let now = Utc::now();
    let ip_id = Uuid::new_v4();
    let ip_version_num = detect_ip_version(&assigned_ip)?;

    sqlx::query(
        "INSERT INTO ips (id, workstation_id, position_id, device_port_id, device_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)",
    )
    .bind(ip_id)
    .bind(device_ws_id)
    .bind(device_pos_id)
    .bind(req.device_port_id)
    .bind(Some(id))
    .bind(&device_type)
    .bind(req_network_id)
    .bind(&assigned_ip)
    .bind(ip_version_num)
    .bind(&req.mac_address)
    .bind(&req.hostname)
    .bind("active")
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let mapping = IpManager {
        id: ip_id,
        workstation_id: device_ws_id,
        position_id: device_pos_id,
        device_port_id: req.device_port_id,
        device_id: Some(id),
        device_type: Some(device_type),
        network_id: Some(req_network_id),
        ip_address: assigned_ip.clone(),
        ip_version: ip_version_num,
        mac_address: req.mac_address.clone(),
        hostname: req.hostname.clone(),
        status: "active".to_string(),
        last_seen: now,
        last_mac: None,
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "device_id": id.to_string(),
        "ip_address": mapping.ip_address,
        "mac_address": mapping.mac_address,
        "hostname": mapping.hostname,
        "auto_assigned": true
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "auto_assign_device_ip",
            resource_type: "device",
            resource_id: &ip_id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(mapping, "设备IP自动分配成功")))
}
