use crate::app_state::AppState;
use crate::crypto::encrypt_password_async;
use crate::error::AppError;
use crate::models::{
    ApiResponse, Device, DeviceConnectRequest, DeviceCreate, DeviceUpdate, DeviceWithDetails,
    IpManager, IpManagerCreate,
};
use crate::resource::ip::detect_ip_version;
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

/// Valid device types matching the database CHECK constraint
const VALID_DEVICE_TYPES: [&str; 9] = [
    "pc",
    "laptop",
    "printer",
    "server",
    "network_device",
    "switch",
    "camera",
    "phone",
    "other",
];

fn validate_device_type(device_type: &str) -> Result<(), AppError> {
    if VALID_DEVICE_TYPES.contains(&device_type) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "设备类型必须是以下之一: {}",
            VALID_DEVICE_TYPES.join(", ")
        )))
    }
}

pub async fn get_devices(
    state: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();
    let workstation_id = query
        .get("workstation_id")
        .and_then(|id| Uuid::parse_str(id).ok());
    let position_id = query
        .get("position_id")
        .and_then(|id| Uuid::parse_str(id).ok());
    let device_type = query.get("device_type").cloned();
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
        ("name", "desc") => "ORDER BY d.name DESC",
        ("device_type", "desc") => "ORDER BY d.device_type DESC, d.name ASC",
        ("device_type", _) => "ORDER BY d.device_type ASC, d.name ASC",
        ("created_at", "desc") => "ORDER BY d.created_at DESC",
        ("created_at", _) => "ORDER BY d.created_at ASC",
        _ => "ORDER BY d.name ASC",
    };

    // Build dynamic WHERE conditions
    let mut where_parts: Vec<String> = Vec::new();
    let mut param_idx = 1;

    if !search.is_empty() {
        where_parts.push(format!(
            "(d.name ILIKE ${param_idx} OR d.brand ILIKE ${param_idx} OR d.model ILIKE ${param_idx} OR d.serial_number ILIKE ${param_idx} OR d.description ILIKE ${param_idx})"
        ));
        param_idx += 1;
    }

    if workstation_id.is_some() {
        where_parts.push(format!("d.workstation_id = ${param_idx}"));
        param_idx += 1;
    }

    if position_id.is_some() {
        where_parts.push(format!("d.position_id = ${param_idx}"));
        param_idx += 1;
    }

    if device_type.as_ref().is_some() {
        where_parts.push(format!("d.device_type = ${param_idx}"));
        param_idx += 1;
    }

    if room_id.is_some() {
        where_parts.push(format!(
            "(d.workstation_id IN (SELECT w.id FROM workstations w WHERE w.room_id = ${param_idx}) OR d.position_id IN (SELECT p.id FROM positions p JOIN cabinets c ON p.cabinet_id = c.id WHERE c.room_id = ${param_idx}))"
        ));
        param_idx += 1;
    }

    let where_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_parts.join(" AND "))
    };

    let search_pattern = format!("%{search}%");

    let count_sql = sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM devices_with_details d {where_clause}"
    ));
    let data_sql = sqlx::AssertSqlSafe(format!(
        "SELECT d.id, d.name, d.device_type, d.brand, d.model, d.serial_number,
                d.workstation_id, d.position_id, d.access_point_id, d.switch_port_id,
                d.template_id, d.vendor, d.location,
                d.snmp_version, d.snmp_community, d.snmp_username,
                d.snmp_auth_protocol, d.snmp_auth_password,
                d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
                d.description,
                d.workstation_name, d.room_id, d.room_name, d.cabinet_id, d.cabinet_name,
                d.start_u, d.end_u, d.access_point_name, d.access_point_type,
                d.connected_switch_port, d.connected_switch_name, d.template_name,
                d.created_at::TIMESTAMPTZ, d.updated_at::TIMESTAMPTZ
         FROM devices_with_details d
         {where_clause}
         {order_clause}
         LIMIT ${param_idx} OFFSET ${}",
        param_idx + 1
    ));

    // Build and execute count query with bound params
    let mut count_query = sqlx::query_scalar::<_, i64>(count_sql);
    let mut data_query = sqlx::query_as::<_, DeviceWithDetails>(data_sql);

    if !search.is_empty() {
        count_query = count_query.bind(&search_pattern);
        data_query = data_query.bind(&search_pattern);
    }

    if let Some(ws_id) = workstation_id {
        count_query = count_query.bind(ws_id);
        data_query = data_query.bind(ws_id);
    }

    if let Some(pos_id) = position_id {
        count_query = count_query.bind(pos_id);
        data_query = data_query.bind(pos_id);
    }

    if let Some(ref dt) = device_type {
        count_query = count_query.bind(dt);
        data_query = data_query.bind(dt);
    }

    if let Some(r_id) = room_id {
        count_query = count_query.bind(r_id);
        data_query = data_query.bind(r_id);
    }

    count_query = count_query.bind(page_size).bind(offset);
    data_query = data_query.bind(page_size).bind(offset);

    let total: i64 = count_query.fetch_one(&state.pool()?.get_conn()).await?;

    let devices = data_query.fetch_all(&state.pool()?.get_conn()).await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": devices,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "设备列表获取成功",
    )))
}

pub async fn create_device(
    state: web::Data<AppState>,
    req: web::Json<DeviceCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    // Validate device_type, default to 'other'
    let device_type = match &req.device_type {
        Some(dt) => {
            validate_device_type(dt)?;
            dt.clone()
        }
        None => "other".to_string(),
    };

    // Business validation: workstation_id and position_id cannot both be set
    if req.workstation_id.is_some() && req.position_id.is_some() {
        return Err(AppError::Validation("工位和机位不能同时指定".to_string()));
    }

    // Business validation: access_point_id and switch_port_id cannot both be set
    if req.access_point_id.is_some() && req.switch_port_id.is_some() {
        return Err(AppError::Validation(
            "接入点和交换机端口不能同时指定".to_string(),
        ));
    }

    let mut tx = state.pool()?.get_conn().begin().await?;

    // If workstation_id provided, verify it exists
    if let Some(ws_id) = req.workstation_id {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workstations WHERE id = $1)")
                .bind(ws_id)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(AppError::NotFound("工位未找到".to_string()));
        }
    }

    // If position_id provided, verify it exists
    if let Some(pos_id) = req.position_id {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM positions WHERE id = $1)")
                .bind(pos_id)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(AppError::NotFound("机位未找到".to_string()));
        }
    }

    // If access_point_id provided, verify it exists
    if let Some(ap_id) = req.access_point_id {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM access_points WHERE id = $1)")
                .bind(ap_id)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(AppError::NotFound("接入点未找到".to_string()));
        }
    }

    // If switch_port_id provided, verify it exists
    if let Some(sp_id) = req.switch_port_id {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM switch_ports WHERE id = $1)")
                .bind(sp_id)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(AppError::NotFound("交换机端口未找到".to_string()));
        }
    }

    // Apply template defaults if template_id is provided
    let (final_device_type, final_brand, final_model) = if let Some(tmpl_id) = req.template_id {
        let template_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_templates WHERE id = $1)")
                .bind(tmpl_id)
                .fetch_one(&mut *tx)
                .await?;
        if !template_exists {
            return Err(AppError::NotFound("设备模板未找到".to_string()));
        }

        let tmpl_device_type: Option<String> =
            sqlx::query_scalar("SELECT device_type FROM device_templates WHERE id = $1")
                .bind(tmpl_id)
                .fetch_optional(&mut *tx)
                .await?;
        let tmpl_brand: Option<String> =
            sqlx::query_scalar("SELECT brand FROM device_templates WHERE id = $1")
                .bind(tmpl_id)
                .fetch_optional(&mut *tx)
                .await?;
        let tmpl_model: Option<String> =
            sqlx::query_scalar("SELECT model FROM device_templates WHERE id = $1")
                .bind(tmpl_id)
                .fetch_optional(&mut *tx)
                .await?;

        // Apply template defaults for missing fields
        let dt = req
            .device_type
            .as_deref()
            .or(tmpl_device_type.as_deref())
            .unwrap_or("other")
            .to_string();
        let br = req.brand.as_ref().or(tmpl_brand.as_ref()).cloned();
        let md = req.model.as_ref().or(tmpl_model.as_ref()).cloned();

        (dt, br, md)
    } else {
        (device_type, req.brand.clone(), req.model.clone())
    };

    // Validate the final device type
    validate_device_type(&final_device_type)?;

    let id = Uuid::new_v4();
    let now = Utc::now();

    // Encrypt SNMP sensitive fields
    let encrypted_community = match &req.snmp_community {
        Some(c) if !c.is_empty() => encrypt_password_async(c.clone()).await,
        _ => None,
    };
    let encrypted_auth_password = match &req.snmp_auth_password {
        Some(p) if !p.is_empty() => encrypt_password_async(p.clone()).await,
        _ => None,
    };
    let encrypted_priv_password = match &req.snmp_priv_password {
        Some(p) if !p.is_empty() => encrypt_password_async(p.clone()).await,
        _ => None,
    };

    let snmp_version = req
        .snmp_version
        .clone()
        .unwrap_or_else(|| "v2c".to_string());
    let snmp_port = req.snmp_port.unwrap_or(161);

    sqlx::query(
        "INSERT INTO devices (id, name, device_type, brand, model, serial_number, workstation_id, position_id, access_point_id, switch_port_id, template_id, vendor, location, snmp_version, snmp_community, snmp_username, snmp_auth_protocol, snmp_auth_password, snmp_priv_protocol, snmp_priv_password, snmp_port, description, created_at, updated_at)
	         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&final_device_type)
    .bind(&final_brand)
    .bind(&final_model)
    .bind(&req.serial_number)
    .bind(req.workstation_id)
    .bind(req.position_id)
    .bind(req.access_point_id)
    .bind(req.switch_port_id)
    .bind(req.template_id)
    .bind(&req.vendor)
    .bind(&req.location)
    .bind(&snmp_version)
    .bind(&encrypted_community)
    .bind(&req.snmp_username)
    .bind(&req.snmp_auth_protocol)
    .bind(&encrypted_auth_password)
    .bind(&req.snmp_priv_protocol)
    .bind(&encrypted_priv_password)
    .bind(snmp_port)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let mut ip_count = 0;
    if let Some(ips) = &req.ips {
        // Determine room_id for network validation based on device location
        let room_id = if let Some(ws_id) = req.workstation_id {
            sqlx::query_scalar::<_, Uuid>("SELECT room_id FROM workstations WHERE id = $1")
                .bind(ws_id)
                .fetch_optional(&mut *tx)
                .await?
        } else if let Some(pos_id) = req.position_id {
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

        for ip in ips {
            ip.validate()?;

            // Check for duplicate IP
            let existing_ip: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM ips WHERE ip_address = CAST($1 AS INET)",
            )
            .bind(&ip.ip_address)
            .fetch_optional(&mut *tx)
            .await?;

            if existing_ip.is_some() {
                return Err(AppError::Conflict("IP地址已存在".to_string()));
            }

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
                .bind(&ip.ip_address)
                .fetch_optional(&mut *tx)
                .await?
            } else {
                ip.network_id
            };

            let ip_version = detect_ip_version(&ip.ip_address)?;

            // Determine ip device_type based on device location
            let ip_device_type = if req.workstation_id.is_some() || req.position_id.is_some() {
                "device"
            } else {
                ip.device_type.as_deref().unwrap_or("device")
            };

            sqlx::query(
                "INSERT INTO ips (id, workstation_id, position_id, switch_port_id, device_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)",
            )
            .bind(Uuid::new_v4())
            .bind(req.workstation_id)
            .bind(req.position_id)
            .bind(ip.switch_port_id)
            .bind(Some(id))
            .bind(ip_device_type)
            .bind(network_id)
            .bind(&ip.ip_address)
            .bind(ip_version)
            .bind(&ip.mac_address)
            .bind(&ip.hostname)
            .bind("active")
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            ip_count += 1;
        }
    }

    if req.save_as_template == Some(true) {
        let template_name = req.template_name.as_deref().unwrap_or(&req.name);
        let tmpl_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO device_templates (id, name, device_type, brand, model, description, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(tmpl_id)
        .bind(template_name)
        .bind(&final_device_type)
        .bind(&final_brand)
        .bind(&final_model)
        .bind(&req.description)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e
                && db_err.is_unique_violation()
            {
                return AppError::Conflict("设备模板名称已存在".to_string());
            }
            AppError::from(e)
        })?;

        sqlx::query("UPDATE devices SET template_id = $1 WHERE id = $2")
            .bind(tmpl_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    let device = Device {
        id,
        name: req.name.clone(),
        device_type: final_device_type,
        brand: final_brand,
        model: final_model,
        serial_number: req.serial_number.clone(),
        workstation_id: req.workstation_id,
        position_id: req.position_id,
        access_point_id: req.access_point_id,
        switch_port_id: req.switch_port_id,
        template_id: req.template_id,
        vendor: req.vendor.clone(),
        location: req.location.clone(),
        snmp_version,
        snmp_community: encrypted_community,
        snmp_username: req.snmp_username.clone(),
        snmp_auth_protocol: req.snmp_auth_protocol.clone(),
        snmp_auth_password: encrypted_auth_password,
        snmp_priv_protocol: req.snmp_priv_protocol.clone(),
        snmp_priv_password: encrypted_priv_password,
        snmp_port,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": device.name,
        "device_type": device.device_type,
        "brand": device.brand,
        "model": device.model,
        "serial_number": device.serial_number,
        "description": device.description,
        "ip_count": ip_count
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
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

    Ok(HttpResponse::Ok().json(ApiResponse::<Device>::success(device, "设备创建成功")))
}

pub async fn get_device(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let device = sqlx::query_as::<_, DeviceWithDetails>(
        "SELECT d.id, d.name, d.device_type, d.brand, d.model, d.serial_number,
                d.workstation_id, d.position_id, d.access_point_id, d.switch_port_id,
                d.template_id, d.vendor, d.location,
                d.snmp_version, d.snmp_community, d.snmp_username,
                d.snmp_auth_protocol, d.snmp_auth_password,
                d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
                d.description,
                d.workstation_name, d.room_id, d.room_name, d.cabinet_id, d.cabinet_name,
                d.start_u, d.end_u, d.access_point_name, d.access_point_type,
                d.connected_switch_port, d.connected_switch_name, d.template_name,
                d.created_at::TIMESTAMPTZ, d.updated_at::TIMESTAMPTZ
         FROM devices_with_details d
         WHERE d.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("设备未找到".to_string()))?;

    // Fetch associated IPs
    let device_ips: Vec<IpManager> = sqlx::query_as(
        r"SELECT
            m.id, m.workstation_id, m.position_id, m.switch_port_id,
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

    let mut result = serde_json::to_value(&device)
        .map_err(|e| AppError::Internal(format!("序列化设备数据失败: {e}")))?;
    result["ips"] = serde_json::to_value(device_ips)
        .map_err(|e| AppError::Internal(format!("序列化IP数据失败: {e}")))?;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<serde_json::Value>::success(
            result,
            "设备获取成功",
        )),
    )
}

pub async fn update_device(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<DeviceUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    (*req).validate()?;

    // Validate device_type if provided
    if let Some(ref dt) = req.device_type {
        validate_device_type(dt)?;
    }

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM devices WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_none() {
        return Err(AppError::NotFound("设备未找到".to_string()));
    }

    // Fetch current device data for business validations
    let current_row = sqlx::query(
        "SELECT workstation_id, position_id, access_point_id, switch_port_id FROM devices WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;

    let current_ws_id: Option<Uuid> = current_row.get("workstation_id");
    let current_pos_id: Option<Uuid> = current_row.get("position_id");
    let current_ap_id: Option<Uuid> = current_row.get("access_point_id");
    let current_sp_id: Option<Uuid> = current_row.get("switch_port_id");

    // Resolve the final values for Option<Option<Uuid>> fields
    let resolved_workstation_id = match &req.workstation_id {
        None => current_ws_id,
        Some(Some(ws_id)) => {
            // Verify the workstation exists
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workstations WHERE id = $1)")
                    .bind(ws_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if !exists {
                return Err(AppError::NotFound("工位未找到".to_string()));
            }
            Some(*ws_id)
        }
        Some(None) => None,
    };

    let resolved_position_id = match &req.position_id {
        None => current_pos_id,
        Some(Some(pos_id)) => {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM positions WHERE id = $1)")
                    .bind(pos_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if !exists {
                return Err(AppError::NotFound("机位未找到".to_string()));
            }
            Some(*pos_id)
        }
        Some(None) => None,
    };

    let resolved_access_point_id = match &req.access_point_id {
        None => current_ap_id,
        Some(Some(ap_id)) => {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM access_points WHERE id = $1)")
                    .bind(ap_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if !exists {
                return Err(AppError::NotFound("接入点未找到".to_string()));
            }
            Some(*ap_id)
        }
        Some(None) => None,
    };

    let resolved_switch_port_id = match &req.switch_port_id {
        None => current_sp_id,
        Some(Some(sp_id)) => {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM switch_ports WHERE id = $1)")
                    .bind(sp_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if !exists {
                return Err(AppError::NotFound("交换机端口未找到".to_string()));
            }
            Some(*sp_id)
        }
        Some(None) => None,
    };

    // Business validation: workstation_id and position_id cannot both be set
    if resolved_workstation_id.is_some() && resolved_position_id.is_some() {
        return Err(AppError::Validation("工位和机位不能同时指定".to_string()));
    }

    // Business validation: access_point_id and switch_port_id cannot both be set
    if resolved_access_point_id.is_some() && resolved_switch_port_id.is_some() {
        return Err(AppError::Validation(
            "接入点和交换机端口不能同时指定".to_string(),
        ));
    }

    let now = Utc::now();

    // Encrypt SNMP sensitive fields if provided
    let encrypted_community = match &req.snmp_community {
        Some(c) if !c.is_empty() => encrypt_password_async(c.clone()).await,
        _ => None,
    };
    let encrypted_auth_password = match &req.snmp_auth_password {
        Some(p) if !p.is_empty() => encrypt_password_async(p.clone()).await,
        _ => None,
    };
    let encrypted_priv_password = match &req.snmp_priv_password {
        Some(p) if !p.is_empty() => encrypt_password_async(p.clone()).await,
        _ => None,
    };

    sqlx::query(
        "UPDATE devices SET
         name = COALESCE($1, name),
         device_type = COALESCE($2, device_type),
         brand = COALESCE($3, brand),
         model = COALESCE($4, model),
         serial_number = COALESCE($5, serial_number),
         workstation_id = CASE WHEN $6::boolean THEN $7 ELSE workstation_id END,
         position_id = CASE WHEN $8::boolean THEN $9 ELSE position_id END,
         access_point_id = CASE WHEN $10::boolean THEN $11 ELSE access_point_id END,
         switch_port_id = CASE WHEN $12::boolean THEN $13 ELSE switch_port_id END,
         vendor = COALESCE($14, vendor),
         location = COALESCE($15, location),
         snmp_version = COALESCE($16, snmp_version),
         snmp_community = CASE WHEN $17::boolean THEN $18 ELSE snmp_community END,
         snmp_username = COALESCE($19, snmp_username),
         snmp_auth_protocol = COALESCE($20, snmp_auth_protocol),
         snmp_auth_password = CASE WHEN $21::boolean THEN $22 ELSE snmp_auth_password END,
         snmp_priv_protocol = COALESCE($23, snmp_priv_protocol),
         snmp_priv_password = CASE WHEN $24::boolean THEN $25 ELSE snmp_priv_password END,
         snmp_port = COALESCE($26, snmp_port),
         description = COALESCE($27, description),
         updated_at = $28
         WHERE id = $29",
    )
    .bind(&req.name)
    .bind(&req.device_type)
    .bind(&req.brand)
    .bind(&req.model)
    .bind(&req.serial_number)
    // workstation_id: CASE WHEN provided THEN resolved_value ELSE current
    .bind(req.workstation_id.is_some())
    .bind(resolved_workstation_id)
    // position_id
    .bind(req.position_id.is_some())
    .bind(resolved_position_id)
    // access_point_id
    .bind(req.access_point_id.is_some())
    .bind(resolved_access_point_id)
    // switch_port_id
    .bind(req.switch_port_id.is_some())
    .bind(resolved_switch_port_id)
    // vendor, location
    .bind(&req.vendor)
    .bind(&req.location)
    // snmp_version, snmp_community (CASE WHEN provided)
    .bind(&req.snmp_version)
    .bind(req.snmp_community.is_some())
    .bind(&encrypted_community)
    // snmp_username, snmp_auth_protocol, snmp_auth_password (CASE WHEN provided)
    .bind(&req.snmp_username)
    .bind(&req.snmp_auth_protocol)
    .bind(req.snmp_auth_password.is_some())
    .bind(&encrypted_auth_password)
    // snmp_priv_protocol, snmp_priv_password (CASE WHEN provided), snmp_port
    .bind(&req.snmp_priv_protocol)
    .bind(req.snmp_priv_password.is_some())
    .bind(&encrypted_priv_password)
    .bind(req.snmp_port)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    // Handle IP replacement if ips are provided
    if let Some(ips) = &req.ips {
        // Determine room_id for network validation based on resolved location
        let room_id = if let Some(ws_id) = resolved_workstation_id {
            sqlx::query_scalar::<_, Uuid>("SELECT room_id FROM workstations WHERE id = $1")
                .bind(ws_id)
                .fetch_optional(&mut *tx)
                .await?
        } else if let Some(pos_id) = resolved_position_id {
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

        // Delete old IPs associated with this device
        sqlx::query("DELETE FROM ips WHERE device_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        for ip in ips {
            ip.validate()?;

            // Check for duplicate IP
            let existing_ip: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM ips WHERE ip_address = CAST($1 AS INET)",
            )
            .bind(&ip.ip_address)
            .fetch_optional(&mut *tx)
            .await?;

            if existing_ip.is_some() {
                return Err(AppError::Conflict("IP地址已存在".to_string()));
            }

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
                .bind(&ip.ip_address)
                .fetch_optional(&mut *tx)
                .await?
            } else {
                ip.network_id
            };

            let ip_version = detect_ip_version(&ip.ip_address)?;

            // Determine ip device_type based on device location
            let ip_device_type =
                if resolved_workstation_id.is_some() || resolved_position_id.is_some() {
                    "device"
                } else {
                    ip.device_type.as_deref().unwrap_or("device")
                };

            sqlx::query(
                "INSERT INTO ips (id, workstation_id, position_id, switch_port_id, device_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)",
            )
            .bind(Uuid::new_v4())
            .bind(resolved_workstation_id)
            .bind(resolved_position_id)
            .bind(ip.switch_port_id)
            .bind(Some(id))
            .bind(ip_device_type)
            .bind(network_id)
            .bind(&ip.ip_address)
            .bind(ip_version)
            .bind(&ip.mac_address)
            .bind(&ip.hostname)
            .bind("active")
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }

    if req.save_as_template == Some(true) {
        let template_name = req
            .template_name
            .as_deref()
            .unwrap_or(req.name.as_deref().unwrap_or(""));
        let tmpl_id = Uuid::new_v4();

        let current_device_type: String =
            sqlx::query_scalar("SELECT device_type FROM devices WHERE id = $1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        let current_brand: Option<String> =
            sqlx::query_scalar("SELECT brand FROM devices WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
        let current_model: Option<String> =
            sqlx::query_scalar("SELECT model FROM devices WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
        let current_description: Option<String> =
            sqlx::query_scalar("SELECT description FROM devices WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();

        sqlx::query(
            "INSERT INTO device_templates (id, name, device_type, brand, model, description, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(tmpl_id)
        .bind(template_name)
        .bind(&current_device_type)
        .bind(&current_brand)
        .bind(&current_model)
        .bind(&current_description)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e
                && db_err.is_unique_violation()
            {
                return AppError::Conflict("设备模板名称已存在".to_string());
            }
            AppError::from(e)
        })?;

        sqlx::query("UPDATE devices SET template_id = $1 WHERE id = $2")
            .bind(tmpl_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    // Fetch updated device with details
    let updated_device = sqlx::query_as::<_, DeviceWithDetails>(
        "SELECT d.id, d.name, d.device_type, d.brand, d.model, d.serial_number,
                d.workstation_id, d.position_id, d.access_point_id, d.switch_port_id,
                d.template_id, d.vendor, d.location,
                d.snmp_version, d.snmp_community, d.snmp_username,
                d.snmp_auth_protocol, d.snmp_auth_password,
                d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
                d.description,
                d.workstation_name, d.room_id, d.room_name, d.cabinet_id, d.cabinet_name,
                d.start_u, d.end_u, d.access_point_name, d.access_point_type,
                d.connected_switch_port, d.connected_switch_name, d.template_name,
                d.created_at::TIMESTAMPTZ, d.updated_at::TIMESTAMPTZ
         FROM devices_with_details d
         WHERE d.id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    // Fetch updated IPs
    let device_ips: Vec<IpManager> = sqlx::query_as(
        r"SELECT
            m.id, m.workstation_id, m.position_id, m.switch_port_id,
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

    let mut result = serde_json::to_value(&updated_device)
        .map_err(|e| AppError::Internal(format!("序列化设备数据失败: {e}")))?;
    result["ips"] = serde_json::to_value(device_ips)
        .map_err(|e| AppError::Internal(format!("序列化IP数据失败: {e}")))?;

    let details = serde_json::json!({
        "name": updated_device.name,
        "device_type": updated_device.device_type,
        "brand": updated_device.brand,
        "model": updated_device.model,
        "description": updated_device.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
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

    Ok(
        HttpResponse::Ok().json(ApiResponse::<serde_json::Value>::success(
            result,
            "设备更新成功",
        )),
    )
}

pub async fn delete_device(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM devices WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_none() {
        return Err(AppError::NotFound("设备未找到".to_string()));
    }

    // 对于同时关联了 workstation/position 的 IP，仅解除 device_id 关联（保留 IP）
    // 对于仅通过 device_id 关联的 IP，由 ON DELETE CASCADE 自动删除
    sqlx::query(
        "UPDATE ips SET device_id = NULL WHERE device_id = $1 AND (workstation_id IS NOT NULL OR position_id IS NOT NULL)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM devices WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "device_id": id.to_string()
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
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

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "设备删除成功")))
}

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
            m.id, m.workstation_id, m.position_id, m.switch_port_id,
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
        "INSERT INTO ips (id, workstation_id, position_id, switch_port_id, device_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)",
    )
    .bind(ip_id)
    .bind(device_ws_id)
    .bind(device_pos_id)
    .bind(req.switch_port_id)
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
        switch_port_id: req.switch_port_id,
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
        .map(|row| crate::utils::parse_network_from_row(&row))
        .ok_or_else(|| AppError::NotFound("网络未找到".to_string()))?;

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
        "INSERT INTO ips (id, workstation_id, position_id, switch_port_id, device_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)",
    )
    .bind(ip_id)
    .bind(device_ws_id)
    .bind(device_pos_id)
    .bind(req.switch_port_id)
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
        switch_port_id: req.switch_port_id,
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

pub async fn connect_device(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<DeviceConnectRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mut tx = state.pool()?.get_conn().begin().await?;

    // Verify device exists
    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM devices WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_none() {
        return Err(AppError::NotFound("设备未找到".to_string()));
    }

    // Resolve final values
    let resolved_ap_id = match &req.access_point_id {
        Some(Some(ap_id)) => {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM access_points WHERE id = $1)")
                    .bind(ap_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if !exists {
                return Err(AppError::NotFound("接入点未找到".to_string()));
            }
            Some(*ap_id)
        }
        Some(None) => None,
        None => {
            // Keep current value
            let current: Option<Uuid> =
                sqlx::query_scalar("SELECT access_point_id FROM devices WHERE id = $1")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
            current
        }
    };

    let resolved_sp_id = match &req.switch_port_id {
        Some(Some(sp_id)) => {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM switch_ports WHERE id = $1)")
                    .bind(sp_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if !exists {
                return Err(AppError::NotFound("交换机端口未找到".to_string()));
            }
            // 检查端口是否已被其他设备占用
            let occupied_by: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM devices WHERE switch_port_id = $1 AND id != $2")
                    .bind(sp_id)
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?;
            if occupied_by.is_some() {
                return Err(AppError::Conflict(
                    "该交换机端口已被其他设备占用".to_string(),
                ));
            }
            Some(*sp_id)
        }
        Some(None) => None,
        None => {
            let current: Option<Uuid> =
                sqlx::query_scalar("SELECT switch_port_id FROM devices WHERE id = $1")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
            current
        }
    };

    // Business validation: access_point_id and switch_port_id cannot both be set
    if resolved_ap_id.is_some() && resolved_sp_id.is_some() {
        return Err(AppError::Validation(
            "接入点和交换机端口不能同时指定".to_string(),
        ));
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE devices SET
         access_point_id = CASE WHEN $1::boolean THEN $2 ELSE access_point_id END,
         switch_port_id = CASE WHEN $3::boolean THEN $4 ELSE switch_port_id END,
         updated_at = $5
         WHERE id = $6",
    )
    .bind(req.access_point_id.is_some())
    .bind(resolved_ap_id)
    .bind(req.switch_port_id.is_some())
    .bind(resolved_sp_id)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // Fetch updated device with details
    let updated_device = sqlx::query_as::<_, DeviceWithDetails>(
        "SELECT d.id, d.name, d.device_type, d.brand, d.model, d.serial_number,
                d.workstation_id, d.position_id, d.access_point_id, d.switch_port_id,
                d.template_id, d.vendor, d.location,
                d.snmp_version, d.snmp_community, d.snmp_username,
                d.snmp_auth_protocol, d.snmp_auth_password,
                d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
                d.description,
                d.workstation_name, d.room_id, d.room_name, d.cabinet_id, d.cabinet_name,
                d.start_u, d.end_u, d.access_point_name, d.access_point_type,
                d.connected_switch_port, d.connected_switch_name, d.template_name,
                d.created_at::TIMESTAMPTZ, d.updated_at::TIMESTAMPTZ
         FROM devices_with_details d
         WHERE d.id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "access_point_id": updated_device.access_point_id,
        "switch_port_id": updated_device.switch_port_id
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "connect",
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

    Ok(
        HttpResponse::Ok().json(ApiResponse::<DeviceWithDetails>::success(
            updated_device,
            "设备连接设置成功",
        )),
    )
}

pub async fn disconnect_device(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mut tx = state.pool()?.get_conn().begin().await?;

    // Verify device exists
    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM devices WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_none() {
        return Err(AppError::NotFound("设备未找到".to_string()));
    }

    // Query old values before update for logging
    let old_row = sqlx::query("SELECT access_point_id, switch_port_id FROM devices WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let old_access_point_id: Option<Uuid> = old_row.get("access_point_id");
    let old_switch_port_id: Option<Uuid> = old_row.get("switch_port_id");

    let now = Utc::now();

    sqlx::query(
        "UPDATE devices SET access_point_id = NULL, switch_port_id = NULL, updated_at = $1 WHERE id = $2",
    )
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // Fetch updated device with details
    let updated_device = sqlx::query_as::<_, DeviceWithDetails>(
        "SELECT d.id, d.name, d.device_type, d.brand, d.model, d.serial_number,
                d.workstation_id, d.position_id, d.access_point_id, d.switch_port_id,
                d.template_id, d.vendor, d.location,
                d.snmp_version, d.snmp_community, d.snmp_username,
                d.snmp_auth_protocol, d.snmp_auth_password,
                d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
                d.description,
                d.workstation_name, d.room_id, d.room_name, d.cabinet_id, d.cabinet_name,
                d.start_u, d.end_u, d.access_point_name, d.access_point_type,
                d.connected_switch_port, d.connected_switch_name, d.template_name,
                d.created_at::TIMESTAMPTZ, d.updated_at::TIMESTAMPTZ
         FROM devices_with_details d
         WHERE d.id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "disconnected": true,
        "previous_access_point_id": old_access_point_id,
        "previous_switch_port_id": old_switch_port_id
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "disconnect",
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

    Ok(
        HttpResponse::Ok().json(ApiResponse::<DeviceWithDetails>::success(
            updated_device,
            "设备断开连接成功",
        )),
    )
}
