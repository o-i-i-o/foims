use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{ApiResponse, IpManager, IpManagerCreate, IpManagerUpdate, IpManagerWithNames};
use crate::utils::pagination::{DEFAULT_PAGE, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};
use crate::utils::{
    OperationLogParams, get_room_id_by_position, get_room_id_by_workstation, log_system_operation,
    validate_network_in_room,
};
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use std::net::IpAddr;
use std::str::FromStr;
use tracing::{error, info, warn};
use uuid::Uuid;
use validator::Validate;

type IpMacCurrentInfo = (Option<String>, String, Option<Uuid>, Option<Uuid>);

pub async fn get_ip_managers(
    state: web::Data<AppState>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let search = query.get("search").map_or("", std::string::String::as_str);
    let device_type = query
        .get("device_type")
        .map_or("", std::string::String::as_str);
    let status = query.get("status").map_or("", std::string::String::as_str);
    let device_name = query
        .get("device_name")
        .map_or("", std::string::String::as_str);
    let network = query.get("network").map_or("", std::string::String::as_str);
    let ip_address = query
        .get("ip_address")
        .map_or("", std::string::String::as_str);
    let network_id = query
        .get("network_id")
        .and_then(|s| uuid::Uuid::parse_str(s).ok());
    let page: i64 = query
        .get("page")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PAGE);
    let page_size: i64 = query
        .get("page_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(MAX_PAGE_SIZE);
    let offset = (page - 1) * page_size;

    let mut conditions: Vec<String> = Vec::new();
    let mut param_index = 1;

    let search_param = if search.is_empty() {
        None
    } else {
        let pattern = format!("%{search}%");
        conditions.push(format!(
            "(ip_address::TEXT ILIKE ${} OR mac_address ILIKE ${} OR hostname ILIKE ${} OR workstation_name ILIKE ${} OR cabinet_position_name ILIKE ${} OR network_name ILIKE ${})",
            param_index, param_index + 1, param_index + 2, param_index + 3, param_index + 4, param_index + 5
        ));
        param_index += 6;
        Some(pattern)
    };

    let device_type_param = if device_type.is_empty() {
        None
    } else {
        conditions.push(format!("device_type = ${param_index}"));
        param_index += 1;
        Some(device_type.to_string())
    };

    let status_param = if status.is_empty() {
        None
    } else {
        conditions.push(format!("status = ${param_index}"));
        param_index += 1;
        Some(status.to_string())
    };

    let device_name_param = if device_name.is_empty() {
        None
    } else {
        let pattern = format!("%{device_name}%");
        conditions.push(format!("device_name ILIKE ${param_index}"));
        param_index += 1;
        Some(pattern)
    };

    let network_param = if network.is_empty() {
        None
    } else {
        let pattern = format!("%{network}%");
        conditions.push(format!("network_name ILIKE ${param_index}"));
        param_index += 1;
        Some(pattern)
    };

    let ip_address_param = if ip_address.is_empty() {
        None
    } else {
        let pattern = format!("%{ip_address}%");
        conditions.push(format!("ip_address::TEXT ILIKE ${param_index}"));
        param_index += 1;
        Some(pattern)
    };

    let network_id_param = if let Some(nid) = network_id {
        conditions.push(format!("network_id = ${param_index}"));
        param_index += 1;
        Some(nid)
    } else {
        None
    };

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_query = format!("SELECT COUNT(*) FROM ip_with_details {where_clause}");
    let mut count_sql = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_query));

    if let Some(ref pattern) = search_param {
        for _ in 0..6 {
            count_sql = count_sql.bind(pattern);
        }
    }
    if let Some(ref dt) = device_type_param {
        count_sql = count_sql.bind(dt);
    }
    if let Some(ref st) = status_param {
        count_sql = count_sql.bind(st);
    }
    if let Some(ref pattern) = device_name_param {
        count_sql = count_sql.bind(pattern);
    }
    if let Some(ref pattern) = network_param {
        count_sql = count_sql.bind(pattern);
    }
    if let Some(ref pattern) = ip_address_param {
        count_sql = count_sql.bind(pattern);
    }
    if let Some(ref nid) = network_id_param {
        count_sql = count_sql.bind(nid);
    }

    let total: i64 = count_sql.fetch_one(&state.pool()?.get_conn()).await?;

    let data_query = format!(
        "SELECT id, workstation_id, position_id, switch_port_id, device_id, device_type, device_name, connected_device_name, connected_device_type, access_point_name, peer_access_point_name, network_id, workstation_name, cabinet_position_name, switch_name, switch_port_number, room_name, cabinet_name, org_name, network_name, network_region, ip_address::TEXT as ip_address, ip_version, mac_address, hostname, status, last_seen, last_mac, created_at, updated_at FROM ip_with_details {} ORDER BY updated_at DESC LIMIT ${} OFFSET ${}",
        where_clause,
        param_index,
        param_index + 1
    );

    let mut data_sql = sqlx::query_as::<_, IpManagerWithNames>(sqlx::AssertSqlSafe(data_query));

    if let Some(ref pattern) = search_param {
        for _ in 0..6 {
            data_sql = data_sql.bind(pattern);
        }
    }
    if let Some(ref dt) = device_type_param {
        data_sql = data_sql.bind(dt);
    }
    if let Some(ref st) = status_param {
        data_sql = data_sql.bind(st);
    }
    if let Some(ref pattern) = device_name_param {
        data_sql = data_sql.bind(pattern);
    }
    if let Some(ref pattern) = network_param {
        data_sql = data_sql.bind(pattern);
    }
    if let Some(ref pattern) = ip_address_param {
        data_sql = data_sql.bind(pattern);
    }
    if let Some(ref nid) = network_id_param {
        data_sql = data_sql.bind(nid);
    }
    data_sql = data_sql.bind(page_size as i32).bind(offset as i32);

    let mappings = data_sql.fetch_all(&state.pool()?.get_conn()).await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "data": mappings,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "IP获取成功",
    )))
}

pub async fn create_ip_manager(
    state: web::Data<AppState>,
    req: web::Json<IpManagerCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    let device_type = req.device_type.as_deref().unwrap_or("");
    if !((device_type == "workstation"
        && req.workstation_id.is_some()
        && req.position_id.is_none())
        || (device_type == "cabinet_position"
            && req.workstation_id.is_none()
            && req.position_id.is_some()))
    {
        return Err(AppError::Validation("设备类型与设备ID不匹配".to_string()));
    }

    let existing_mapping =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM ips WHERE ip_address = CAST($1 AS INET)")
            .bind(&req.ip_address)
            .fetch_optional(&state.pool()?.get_conn())
            .await?;

    if existing_mapping.is_some() {
        return Err(AppError::Conflict("该IP地址已存在".to_string()));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    let ip_version_num = detect_ip_version(&req.ip_address)?;

    sqlx::query(
        "INSERT INTO ips (id, workstation_id, position_id, switch_port_id, device_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)"
    )
    .bind(id)
    .bind(req.workstation_id)
    .bind(req.position_id)
    .bind(req.switch_port_id)
    .bind(req.device_id)
    .bind(&req.device_type)
    .bind(req.network_id)
    .bind(&req.ip_address)
    .bind(ip_version_num)
    .bind(&req.mac_address)
    .bind(&req.hostname)
    .bind("active")
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&state.pool()?.get_conn()).await?;

    let mapping = IpManager {
        id,
        workstation_id: req.workstation_id,
        position_id: req.position_id,
        switch_port_id: req.switch_port_id,
        device_id: req.device_id,
        device_type: req.device_type.clone(),
        network_id: req.network_id,
        ip_address: req.ip_address.clone(),
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
        "ip_address": mapping.ip_address,
        "mac_address": mapping.mac_address,
        "hostname": mapping.hostname,
        "ip_version": mapping.ip_version,
        "workstation_id": mapping.workstation_id,
        "position_id": mapping.position_id
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
            resource_type: "ip_manager",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }
    tracing::info!("IP地址 {} 创建成功, ID: {}", mapping.ip_address, id);

    Ok(HttpResponse::Ok().json(ApiResponse::<IpManager>::success(mapping, "IP管理创建成功")))
}

pub async fn get_ip_manager(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mapping = sqlx::query_as::<_, IpManager>(
        r"SELECT m.id, m.workstation_id, m.position_id, m.switch_port_id, m.device_id, m.device_type, 
           m.network_id,
           host(m.ip_address) as ip_address, m.ip_version, m.mac_address, m.hostname, m.status, 
           m.last_seen::TIMESTAMPTZ, m.last_mac, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ 
           FROM ips m
           WHERE m.id = $1"
    ).bind(id)
    .fetch_optional(&state.pool()?.get_conn()).await?
    .ok_or_else(|| AppError::NotFound("IP管理未找到".to_string()))?;

    Ok(HttpResponse::Ok().json(ApiResponse::<IpManager>::success(mapping, "IP管理获取成功")))
}

pub async fn get_workstation_ips(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let workstation_id = *id_path;

    let ips = sqlx::query_as::<_, IpManagerWithNames>(
        r"SELECT 
            id, workstation_id, position_id, switch_port_id, device_id, device_type, device_name, 
            connected_device_name, connected_device_type, access_point_name, peer_access_point_name,
            network_id, workstation_name, cabinet_position_name, switch_name, switch_port_number, room_name, cabinet_name, org_name, network_name, network_region, 
            ip_address::TEXT as ip_address, ip_version, mac_address, hostname, status, last_seen, last_mac, created_at, updated_at 
        FROM ip_with_details 
        WHERE workstation_id = $1"
    )
    .bind(workstation_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<IpManagerWithNames>>::success(
            ips,
            "获取工位IP列表成功",
        )),
    )
}

pub async fn get_cabinet_position_ips(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let position_id = *id_path;

    let ips = sqlx::query_as::<_, IpManagerWithNames>(
        r"SELECT 
            id, workstation_id, position_id, switch_port_id, device_id, device_type, device_name, 
            connected_device_name, connected_device_type, access_point_name, peer_access_point_name,
            network_id, workstation_name, cabinet_position_name, switch_name, switch_port_number, room_name, cabinet_name, org_name, network_name, network_region, 
            ip_address::TEXT as ip_address, ip_version, mac_address, hostname, status, last_seen, last_mac, created_at, updated_at 
        FROM ip_with_details 
        WHERE position_id = $1"
    )
    .bind(position_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<IpManagerWithNames>>::success(
            ips,
            "获取机位IP列表成功",
        )),
    )
}

pub async fn get_switch_ips(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let device_id = *id_path;

    let position_id: Option<Uuid> =
        sqlx::query_scalar("SELECT position_id FROM devices WHERE id = $1")
            .bind(device_id)
            .fetch_optional(&state.pool()?.get_conn())
            .await?;

    let Some(position_id) = position_id else {
        return Ok(
            HttpResponse::Ok().json(ApiResponse::<Vec<IpManagerWithNames>>::success(
                vec![],
                "获取设备IP列表成功",
            )),
        );
    };

    let ips = sqlx::query_as::<_, IpManagerWithNames>(
        r"SELECT
            id, workstation_id, position_id, switch_port_id, device_id, device_type, device_name,
            connected_device_name, connected_device_type, access_point_name, peer_access_point_name,
            network_id, workstation_name, cabinet_position_name, switch_name, switch_port_number, room_name, cabinet_name, org_name, network_name, network_region,
            ip_address::TEXT as ip_address, ip_version, mac_address, hostname, status, last_seen, last_mac, created_at, updated_at
        FROM ip_with_details
        WHERE position_id = $1"
    )
    .bind(position_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<IpManagerWithNames>>::success(
            ips,
            "获取设备IP列表成功",
        )),
    )
}

pub async fn update_ip_manager(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<IpManagerUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    (*req).validate()?;

    let existing_mapping = sqlx::query_as::<_, IpManager>(
        r"SELECT m.id, m.workstation_id, m.position_id, m.switch_port_id, m.device_type, 
           m.network_id,
           host(m.ip_address) as ip_address, m.ip_version, m.mac_address, m.hostname, m.status, 
           m.last_seen::TIMESTAMPTZ, m.last_mac, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ 
           FROM ips m
           WHERE m.id = $1"
    ).bind(id)
    .fetch_optional(&state.pool()?.get_conn()).await?
    .ok_or_else(|| AppError::NotFound("IP管理未找到".to_string()))?;

    let ip_address = req
        .ip_address
        .clone()
        .unwrap_or_else(|| existing_mapping.ip_address.clone());

    if req.ip_address.is_some() {
        let existing_ip = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM ips WHERE ip_address = CAST($1 AS INET) AND id != $2",
        )
        .bind(&ip_address)
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

        if existing_ip.is_some() {
            return Err(AppError::Conflict("该IP地址已存在".to_string()));
        }
    }

    let now = Utc::now();

    let ip_version_num = if let Some(ip_address) = &req.ip_address {
        detect_ip_version(ip_address)?
    } else {
        if let Some(ip_version) = &req.ip_version {
            *ip_version
        } else {
            detect_ip_version(&existing_mapping.ip_address)?
        }
    };

    if let Some(device_type) = &req.device_type
        && !((device_type == "workstation"
            && req.workstation_id.is_some()
            && req.position_id.is_none())
            || (device_type == "cabinet_position"
                && req.workstation_id.is_none()
                && req.position_id.is_some()))
    {
        return Err(AppError::Validation("设备类型与设备ID不匹配".to_string()));
    }

    let effective_device_type = req
        .device_type
        .as_deref()
        .or(existing_mapping.device_type.as_deref());
    let effective_workstation_id = req.workstation_id.or(existing_mapping.workstation_id);
    let effective_position_id = req.position_id.or(existing_mapping.position_id);

    if effective_device_type == Some("workstation") && effective_workstation_id.is_none() {
        return Err(AppError::Validation("工位IP必须关联工位".to_string()));
    }
    if effective_device_type != Some("workstation") && effective_position_id.is_none() {
        return Err(AppError::Validation("机位IP必须关联机位".to_string()));
    }

    sqlx::query(
        "UPDATE ips SET 
         workstation_id = $1, 
         position_id = $2,
         switch_port_id = $3,
         device_id = COALESCE($4, device_id),
         device_type = COALESCE($5, device_type),
         ip_address = COALESCE(CAST($6 AS INET), ip_address), 
         mac_address = COALESCE($7, mac_address), 
         hostname = COALESCE($8, hostname), 
         status = COALESCE($9, status), 
         ip_version = $10, 
         updated_at = $11 
         WHERE id = $12",
    )
    .bind(req.workstation_id)
    .bind(req.position_id)
    .bind(req.switch_port_id)
    .bind(req.device_id)
    .bind(&req.device_type)
    .bind(&req.ip_address)
    .bind(&req.mac_address)
    .bind(&req.hostname)
    .bind(&req.status)
    .bind(ip_version_num)
    .bind(now)
    .bind(id)
    .execute(&state.pool()?.get_conn())
    .await?;

    let mapping = sqlx::query_as::<_, IpManager>(
        r"SELECT m.id, m.workstation_id, m.position_id, m.switch_port_id, m.device_id, m.device_type, 
           m.network_id,
           host(m.ip_address) as ip_address, m.ip_version, m.mac_address, m.hostname, m.status, 
           m.last_seen::TIMESTAMPTZ, m.last_mac, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ 
           FROM ips m
           WHERE m.id = $1"
    ).bind(id)
    .fetch_one(&state.pool()?.get_conn()).await?;

    let details = serde_json::json!({
        "ip_address": mapping.ip_address,
        "mac_address": mapping.mac_address,
        "hostname": mapping.hostname,
        "ip_version": mapping.ip_version,
        "status": mapping.status,
        "workstation_id": mapping.workstation_id,
        "position_id": mapping.position_id
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
            resource_type: "ip_manager",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }
    tracing::info!("IP地址 {} 更新成功, ID: {}", mapping.ip_address, id);

    Ok(HttpResponse::Ok().json(ApiResponse::<IpManager>::success(mapping, "IP管理更新成功")))
}

pub async fn delete_ip_manager(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let existing_mapping = sqlx::query_scalar::<_, Uuid>("SELECT id FROM ips WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

    if existing_mapping.is_none() {
        return Err(AppError::NotFound("IP管理未找到".to_string()));
    }

    sqlx::query("DELETE FROM ips WHERE id = $1")
        .bind(id)
        .execute(&state.pool()?.get_conn())
        .await?;

    let details = serde_json::json!({
        "ip_manager_id": id.to_string()
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "ip_manager",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }
    tracing::info!("IP地址删除成功, ID: {}", id);

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "IP管理删除成功")))
}

struct MacSyncResult {
    updated_count: usize,
    unchanged_count: usize,
    skipped_count: usize,
    switch_macs_empty: bool,
    total_macs_on_switch: i64,
}

async fn sync_switch_macs(
    pool: &sqlx::PgPool,
    device_id: Uuid,
    network_id: Uuid,
) -> Result<MacSyncResult, AppError> {
    let network_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM network_cidrs WHERE id = $1)")
            .bind(network_id)
            .fetch_one(pool)
            .await?;

    if !network_exists {
        return Err(AppError::Validation("未找到网段信息".to_string()));
    }

    let switch_macs: Vec<(String, String)> = sqlx::query_as(
        r"SELECT host(sm.ip_address), sm.mac_address FROM switch_macs sm
          INNER JOIN ips i ON sm.ip_address = i.ip_address
          WHERE sm.device_id = $1 AND i.network_id = $2",
    )
    .bind(device_id)
    .bind(network_id)
    .fetch_all(pool)
    .await?;

    if switch_macs.is_empty() {
        let total_macs: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM switch_macs WHERE device_id = $1")
                .bind(device_id)
                .fetch_one(pool)
                .await?;

        return Ok(MacSyncResult {
            updated_count: 0,
            unchanged_count: 0,
            skipped_count: 0,
            switch_macs_empty: true,
            total_macs_on_switch: total_macs,
        });
    }

    let now = Utc::now();
    let mut updated_count = 0usize;
    let mut unchanged_count = 0usize;
    let mut skipped_count = 0usize;

    for (ip, mac) in &switch_macs {
        let current: Option<IpMacCurrentInfo> = sqlx::query_as(
            "SELECT mac_address, device_type, workstation_id, position_id FROM ips WHERE ip_address = CAST($1 AS INET)"
        )
        .bind(ip)
        .fetch_optional(pool)
        .await?;

        let Some((old_mac, device_type, ws_id, pos_id)) = current else {
            continue;
        };

        let mac_conflict: Option<String> = sqlx::query_scalar(
            r"SELECT host(ip_address) FROM ips
               WHERE mac_address = $1 
               AND ip_address != CAST($2 AS INET)
               AND (
                   device_type != $3
                   OR workstation_id IS DISTINCT FROM $4
                   OR position_id IS DISTINCT FROM $5
               )
               LIMIT 1",
        )
        .bind(mac)
        .bind(ip)
        .bind(&device_type)
        .bind(ws_id)
        .bind(pos_id)
        .fetch_optional(pool)
        .await?
        .flatten();

        if let Some(conflict_ip) = mac_conflict {
            warn!(
                "MAC冲突: {} 已被不同设备的 IP {} 使用，跳过更新",
                mac, conflict_ip
            );
            skipped_count += 1;
            continue;
        }

        match old_mac.as_deref() {
            None | Some("") => {
                sqlx::query(
                    r"UPDATE ips 
                       SET mac_address = $1, last_seen = $2, updated_at = $2
                       WHERE ip_address = CAST($3 AS INET)",
                )
                .bind(mac)
                .bind(now)
                .bind(ip)
                .execute(pool)
                .await?;
                updated_count += 1;
                info!("MAC地址写入: IP={}, MAC={}", ip, mac);
            }
            Some(old) if old == mac => {
                sqlx::query(
                    r"UPDATE ips SET last_seen = $1, updated_at = $1 WHERE ip_address = CAST($2 AS INET)"
                )
                .bind(now)
                .bind(ip)
                .execute(pool)
                .await?;
                unchanged_count += 1;
            }
            Some(old) => {
                sqlx::query(
                    r"UPDATE ips 
                       SET last_mac = $1, mac_address = $2, last_seen = $3, updated_at = $3
                       WHERE ip_address = CAST($4 AS INET)",
                )
                .bind(old)
                .bind(mac)
                .bind(now)
                .bind(ip)
                .execute(pool)
                .await?;
                updated_count += 1;

                let workstation_id: Option<Uuid> = sqlx::query_scalar::<_, Option<Uuid>>(
                    "SELECT COALESCE(i.workstation_id, p.workstation_id) FROM ips i LEFT JOIN positions p ON i.position_id = p.id WHERE i.ip_address = CAST($1 AS INET)"
                )
                .bind(ip)
                .fetch_optional(pool)
                .await?
                .flatten();

                if let Some(ws_id) = workstation_id {
                    info!("检测到MAC地址变更: IP={}, 旧MAC={}, 新MAC={}", ip, old, mac);
                    match crate::utils::send_mac_change_notification(pool, &ws_id, ip, old, mac)
                        .await
                    {
                        Ok(()) => info!("MAC地址变更通知发送成功: IP={}", ip),
                        Err(e) => error!("MAC地址变更通知发送失败: IP={}, 错误: {}", ip, e),
                    }
                }
            }
        }
    }

    Ok(MacSyncResult {
        updated_count,
        unchanged_count,
        skipped_count,
        switch_macs_empty: false,
        total_macs_on_switch: 0,
    })
}

pub async fn pull_ip_managers(
    state: web::Data<AppState>,
    req: web::Json<crate::models::PullIpManagersRequest>,
) -> Result<HttpResponse, AppError> {
    req.validate()?;

    let result = sync_switch_macs(&state.pool()?.get_conn(), req.device_id, req.network_id).await?;

    if result.switch_macs_empty {
        if result.total_macs_on_switch == 0 {
            return Ok(
                HttpResponse::Ok().json(ApiResponse::<Vec<IpManager>>::error(
                    "该设备暂无MAC数据，请先在设备管理中同步MAC表",
                )),
            );
        }
        return Ok(
            HttpResponse::Ok().json(ApiResponse::<Vec<IpManager>>::success(
                vec![],
                "未发现属于该网段的已管理IP地址",
            )),
        );
    }

    let results: Vec<IpManager> = sqlx::query_as::<_, IpManager>(
        r"SELECT m.id, m.workstation_id, m.position_id, m.switch_port_id, m.device_id, m.device_type, 
           m.network_id,
           host(m.ip_address) as ip_address, m.ip_version, m.mac_address, m.hostname, m.status, 
           m.last_seen::TIMESTAMPTZ, m.last_mac, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ 
           FROM ips m
           WHERE m.network_id = $1"
    )
    .bind(req.network_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let mut message_parts = Vec::new();
    if result.updated_count > 0 {
        message_parts.push(format!("更新 {} 条MAC地址", result.updated_count));
    }
    if result.unchanged_count > 0 {
        message_parts.push(format!("{} 条MAC无变化", result.unchanged_count));
    }
    if result.skipped_count > 0 {
        message_parts.push(format!("{} 条MAC冲突跳过", result.skipped_count));
    }

    let message = if message_parts.is_empty() {
        "MAC地址无变化".to_string()
    } else {
        message_parts.join("，")
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(results, &message)))
}

pub async fn pull_ip_managers_internal(
    pool: &sqlx::PgPool,
    device_id: Uuid,
    network_id: Uuid,
) -> Result<(), String> {
    let result = sync_switch_macs(pool, device_id, network_id)
        .await
        .map_err(|e| format!("MAC同步失败: {e}"))?;

    if result.switch_macs_empty {
        return Err("未发现属于该网段的已管理IP地址".to_string());
    }

    Ok(())
}

pub fn detect_ip_version(ip: &str) -> Result<i16, AppError> {
    match IpAddr::from_str(ip) {
        Ok(IpAddr::V6(_)) => Ok(6),
        Ok(IpAddr::V4(_)) => Ok(4),
        Err(e) => Err(AppError::Validation(format!(
            "IP地址格式无效 '{}': {}",
            ip, e
        ))),
    }
}

pub fn find_available_ips_in_cidr(
    cidr_str: &str,
    gateway: Option<&String>,
    used_ips: &std::collections::HashSet<String>,
    max_count: Option<usize>,
) -> Vec<String> {
    let Ok(network_cidr) = ipnetwork::IpNetwork::from_str(cidr_str) else {
        tracing::warn!("CIDR格式无效，无法查找可用IP: '{}'", cidr_str);
        return Vec::new();
    };

    let network_addr = network_cidr.network();
    let broadcast_addr = match network_cidr {
        ipnetwork::IpNetwork::V4(v4) => Some(v4.broadcast().to_string()),
        ipnetwork::IpNetwork::V6(_) => None,
    };

    let mut available = Vec::new();
    for ip in &network_cidr {
        let ip_str = ip.to_string();
        if used_ips.contains(&ip_str) {
            continue;
        }
        if gateway.is_some_and(|g| g == &ip_str) {
            continue;
        }
        if ip.to_string() == network_addr.to_string() {
            continue;
        }
        if broadcast_addr.as_ref() == Some(&ip_str) {
            continue;
        }
        available.push(ip_str);
        if max_count.is_some_and(|max| available.len() >= max) {
            break;
        }
    }
    available
}

pub async fn get_available_ips(
    state: web::Data<AppState>,
    network_id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let network_id = *network_id_path;

    let network = sqlx::query(crate::utils::NETWORK_QUERY)
        .bind(network_id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .map(|row| crate::utils::parse_network_from_row(&row))
        .ok_or_else(|| AppError::NotFound("网络未找到".to_string()))?;

    let used_ips: Vec<String> = sqlx::query_scalar(
        r"SELECT host(i.ip_address)::TEXT 
              FROM ips i
              WHERE i.id IN (
                  SELECT i2.id FROM ips i2
                  LEFT JOIN workstations w ON i2.workstation_id = w.id
                  LEFT JOIN positions p ON i2.position_id = p.id
                  LEFT JOIN cabinets c ON p.cabinet_id = c.id
                  WHERE EXISTS (
                      SELECT 1 FROM room_networks rn 
                      WHERE rn.network_id = $1 
                      AND rn.room_id = COALESCE(w.room_id, c.room_id)
                  )
              )",
    )
    .bind(network_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let used_set: std::collections::HashSet<String> = used_ips.into_iter().collect();

    let mut available_ips = Vec::new();

    if let Some(ipv4_cidr) = &network.ipv4_cidr {
        available_ips.extend(find_available_ips_in_cidr(
            ipv4_cidr,
            network.ipv4_gateway.as_ref(),
            &used_set,
            Some(256),
        ));
    }

    if let Some(ipv6_cidr) = &network.ipv6_cidr {
        available_ips.extend(find_available_ips_in_cidr(
            ipv6_cidr,
            network.ipv6_gateway.as_ref(),
            &used_set,
            Some(256),
        ));
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "network_id": network_id,
            "network_name": network.name,
            "available_count": available_ips.len(),
            "available_ips": available_ips
        }),
        "获取可用IP列表成功",
    )))
}

pub async fn auto_assign_ip(
    state: web::Data<AppState>,
    req: web::Json<crate::models::AutoAssignIpRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    req.validate()?;

    let req_network_id = req.network_id;
    let workstation_id = req.workstation_id;
    let position_id = req.position_id;
    let switch_port_id = req.switch_port_id;
    let mac_address = req.mac_address.clone();
    let hostname = req.hostname.clone();

    let device_type = if workstation_id.is_some() && position_id.is_none() {
        "workstation".to_string()
    } else if workstation_id.is_none() && position_id.is_some() {
        let pos_device_type: Option<String> =
            sqlx::query_scalar("SELECT device_type FROM positions WHERE id = $1")
                .bind(position_id)
                .fetch_optional(&state.pool()?.get_conn())
                .await?;

        match pos_device_type.as_deref() {
            Some("switch") => "switch".to_string(),
            _ => "cabinet_position".to_string(),
        }
    } else {
        return Err(AppError::Validation(
            "必须指定一个设备ID（workstation_id或position_id）".to_string(),
        ));
    };

    let room_id = if device_type == "workstation" {
        if let Some(ws_id) = workstation_id {
            get_room_id_by_workstation(&state.pool()?.get_conn(), ws_id).await?
        } else {
            None
        }
    } else if let Some(pos_id) = position_id {
        get_room_id_by_position(&state.pool()?.get_conn(), pos_id).await?
    } else {
        None
    };

    if let Some(rid) = room_id {
        validate_network_in_room(&state.pool()?.get_conn(), rid, Some(req_network_id)).await?;
    }

    let network = sqlx::query(crate::utils::NETWORK_QUERY)
        .bind(req_network_id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .map(|row| crate::utils::parse_network_from_row(&row))
        .ok_or_else(|| AppError::NotFound("网络未找到".to_string()))?;

    let used_ips: Vec<String> =
        sqlx::query_scalar("SELECT host(ip_address) FROM ips WHERE network_id = $1")
            .bind(req_network_id)
            .fetch_all(&state.pool()?.get_conn())
            .await?;

    let used_set: std::collections::HashSet<String> = used_ips.into_iter().collect();

    let assigned_ip = network
        .ipv4_cidr
        .as_ref()
        .and_then(|cidr| {
            find_available_ips_in_cidr(cidr, network.ipv4_gateway.as_ref(), &used_set, Some(1))
                .into_iter()
                .next()
        })
        .or_else(|| {
            network.ipv6_cidr.as_ref().and_then(|cidr| {
                find_available_ips_in_cidr(cidr, network.ipv6_gateway.as_ref(), &used_set, Some(1))
                    .into_iter()
                    .next()
            })
        })
        .ok_or_else(|| AppError::Validation("该网络没有可用的IP地址".to_string()))?;

    let id = Uuid::new_v4();
    let now = Utc::now();
    let ip_version_num = detect_ip_version(&assigned_ip)?;

    sqlx::query(
        "INSERT INTO ips (id, workstation_id, position_id, switch_port_id, device_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)"
    )
    .bind(id)
    .bind(workstation_id)
    .bind(position_id)
    .bind(switch_port_id)
    .bind(req.device_id)
    .bind(&device_type)
    .bind(req_network_id)
    .bind(&assigned_ip)
    .bind(ip_version_num)
    .bind(&mac_address)
    .bind(&hostname)
    .bind("active")
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&state.pool()?.get_conn()).await?;

    let mapping = IpManager {
        id,
        workstation_id,
        position_id,
        switch_port_id,
        device_id: req.device_id,
        device_type: Some(device_type),
        network_id: Some(req_network_id),
        ip_address: assigned_ip.clone(),
        ip_version: ip_version_num,
        mac_address,
        hostname,
        status: "active".to_string(),
        last_seen: now,
        last_mac: None,
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "ip_address": mapping.ip_address,
        "mac_address": mapping.mac_address,
        "hostname": mapping.hostname,
        "auto_assigned": true
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "auto_assign_ip",
            resource_type: "ip_manager",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(mapping, "IP地址自动分配成功")))
}

pub async fn batch_create_ip_managers(
    state: web::Data<AppState>,
    req: web::Json<Vec<IpManagerCreate>>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let now = Utc::now();
    let mut valid_requests: Vec<(usize, &IpManagerCreate, Uuid, i16)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for (index, ip_req) in req.iter().enumerate() {
        if let Err(e) = ip_req.validate() {
            errors.push(format!("第{}条记录验证失败: {:?}", index + 1, e));
            continue;
        }

        let device_type = ip_req.device_type.as_deref().unwrap_or("");
        let device_valid = (device_type == "workstation"
            && ip_req.workstation_id.is_some()
            && ip_req.position_id.is_none())
            || (device_type == "cabinet_position"
                && ip_req.workstation_id.is_none()
                && ip_req.position_id.is_some());

        if !device_valid {
            errors.push(format!("第{}条记录: 设备类型与设备ID不匹配", index + 1));
            continue;
        }

        let id = Uuid::new_v4();
        let ip_version_num = match detect_ip_version(&ip_req.ip_address) {
            Ok(v) => v,
            Err(e) => {
                errors.push(format!("第{}条记录: {}", index + 1, e));
                continue;
            }
        };
        valid_requests.push((index, ip_req, id, ip_version_num));
    }

    if valid_requests.is_empty() {
        let error_msg = if errors.is_empty() {
            "没有有效的记录".to_string()
        } else {
            errors.join("; ")
        };
        return Err(AppError::Validation(error_msg));
    }

    let mut tx = state.pool()?.get_conn().begin().await?;

    let mut created_ips = Vec::new();
    let mut duplicate_errors = Vec::new();

    for (index, ip_req, id, ip_version_num) in &valid_requests {
        let existing: Option<Uuid> =
            match sqlx::query_scalar("SELECT id FROM ips WHERE ip_address = CAST($1 AS INET)")
                .bind(&ip_req.ip_address)
                .fetch_optional(tx.as_mut())
                .await
            {
                Ok(opt) => opt,
                Err(err) => {
                    duplicate_errors.push(format!(
                        "第{}条记录: 数据库查询错误 - {}",
                        index + 1,
                        err
                    ));
                    continue;
                }
            };

        if existing.is_some() {
            duplicate_errors.push(format!(
                "第{}条记录: IP地址 {} 已存在",
                index + 1,
                ip_req.ip_address
            ));
            continue;
        }

        if let Err(err) = sqlx::query(
            "INSERT INTO ips (id, workstation_id, position_id, switch_port_id, device_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
             VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)"
        )
        .bind(*id)
        .bind(ip_req.workstation_id)
        .bind(ip_req.position_id)
        .bind(ip_req.switch_port_id)
        .bind(ip_req.device_id)
        .bind(&ip_req.device_type)
        .bind(ip_req.network_id)
        .bind(&ip_req.ip_address)
        .bind(*ip_version_num)
        .bind(&ip_req.mac_address)
        .bind(&ip_req.hostname)
        .bind("active")
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(tx.as_mut())
        .await
        {
            duplicate_errors.push(format!("第{}条记录: 插入失败 - {}", index + 1, err));
            continue;
        }

        created_ips.push(IpManager {
            id: *id,
            workstation_id: ip_req.workstation_id,
            position_id: ip_req.position_id,
            switch_port_id: ip_req.switch_port_id,
            device_id: ip_req.device_id,
            device_type: ip_req.device_type.clone(),
            network_id: ip_req.network_id,
            ip_address: ip_req.ip_address.clone(),
            ip_version: *ip_version_num,
            mac_address: ip_req.mac_address.clone(),
            hostname: ip_req.hostname.clone(),
            status: "active".to_string(),
            last_seen: now,
            last_mac: None,
            created_at: now,
            updated_at: now,
        });
    }

    tx.commit().await?;

    errors.extend(duplicate_errors);

    let details = serde_json::json!({
        "created_count": created_ips.len(),
        "error_count": errors.len(),
        "errors": errors
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "batch_create",
            resource_type: "ip_manager",
            resource_id: &Uuid::nil(),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "created": created_ips,
            "created_count": created_ips.len(),
            "errors": errors,
            "error_count": errors.len()
        }),
        &format!(
            "批量创建完成，成功 {} 条，失败 {} 条",
            created_ips.len(),
            errors.len()
        ),
    )))
}
