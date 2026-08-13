use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{ApiResponse, IpManager, IpManagerCreate, IpManagerWithNames};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::Pagination;
use crate::utils::{get_room_id_by_position, get_room_id_by_workstation, validate_network_in_room};
use chrono::Utc;
use sqlx::Row;
use std::net::IpAddr;
use std::str::FromStr;
use tracing::{error, info, warn};
use uuid::Uuid;
use validator::Validate;

type IpMacCurrentInfo = (Option<String>, Uuid);

pub async fn get_ip_managers(
    State(state): State<Arc<AppState>>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Response, AppError> {
    let search = query.get("search").map_or("", std::string::String::as_str);
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
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;

    let mut conditions: Vec<String> = Vec::new();
    let mut param_index = 1;

    let search_param = if search.is_empty() {
        None
    } else {
        let pattern = crate::utils::escape_like(search);
        conditions.push(format!(
            "(ip_address::TEXT ILIKE ${} OR mac_address ILIKE ${} OR hostname ILIKE ${} OR description ILIKE ${} OR device_name ILIKE ${} OR workstation_name ILIKE ${} OR cabinet_position_name ILIKE ${} OR network_name ILIKE ${})",
            param_index, param_index + 1, param_index + 2, param_index + 3, param_index + 4, param_index + 5, param_index + 6, param_index + 7
        ));
        param_index += 8;
        Some(pattern)
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
        for _ in 0..8 {
            count_sql = count_sql.bind(pattern);
        }
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
        "SELECT id, device_interface_id, device_id, device_type, device_name, interface_name, interface_type, network_id, workstation_name, cabinet_position_name, room_name, cabinet_name, org_name, network_name, network_region, ip_address::TEXT as ip_address, ip_version, mac_address, hostname, description, status, last_seen, last_mac, created_at, updated_at FROM ip_with_details {} ORDER BY updated_at DESC LIMIT ${} OFFSET ${}",
        where_clause,
        param_index,
        param_index + 1
    );

    let mut data_sql = sqlx::query_as::<_, IpManagerWithNames>(sqlx::AssertSqlSafe(data_query));

    if let Some(ref pattern) = search_param {
        for _ in 0..8 {
            data_sql = data_sql.bind(pattern);
        }
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

    Ok(crate::error::ok_json(
        serde_json::json!({
            "data": mappings,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "IP获取成功",
    ))
}

pub async fn get_device_ips(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
        .bind(id)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    if !exists {
        return Err(AppError::NotFound("设备未找到".to_string()));
    }

    let ips: Vec<IpManager> = sqlx::query_as(
        r"SELECT
            m.id, m.device_interface_id, m.device_id, m.network_id,
            host(m.ip_address) as ip_address,
            m.ip_version, m.mac_address, m.hostname, m.description,
            m.status, m.last_seen, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ, m.last_mac
        FROM ips m
        WHERE m.device_id = $1
        ORDER BY m.ip_address",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(crate::error::ok_json(
        serde_json::json!({ "items": ips }),
        "设备IP列表获取成功",
    ))
}

pub async fn create_device_ip(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<IpManagerCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let device_row = sqlx::query("SELECT workstation_id, position_id FROM devices WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("设备未找到".to_string()))?;

    let device_ws_id: Option<Uuid> = device_row.get("workstation_id");
    let device_pos_id: Option<Uuid> = device_row.get("position_id");

    let existing_ip: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM ips WHERE ip_address = CAST($1 AS INET)")
            .bind(&req.ip_address)
            .fetch_optional(&mut *tx)
            .await?;

    if existing_ip.is_some() {
        return Err(AppError::Conflict("IP地址已存在".to_string()));
    }

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
    let now = Utc::now();
    let ip_id = Uuid::new_v4();

    // 解析 device_interface_id：优先使用请求中的；否则使用设备的默认 physical 接口
    let interface_id = match req.device_interface_id {
        Some(iid) => iid,
        None => {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM device_interfaces WHERE device_id = $1 AND interface_type = 'physical' ORDER BY created_at LIMIT 1",
            )
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::Validation(
                "设备没有可用的 physical 接口，请先创建接口或指定 device_interface_id".to_string()
            ))?
        }
    };

    sqlx::query(
        "INSERT INTO ips (id, device_interface_id, device_id, network_id, ip_address, ip_version, description, status, last_seen, created_at, updated_at)
         VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, $8, $9, $10, $11)",
    )
    .bind(ip_id)
    .bind(interface_id)
    .bind(id)
    .bind(network_id)
    .bind(&req.ip_address)
    .bind(ip_version)
    .bind(&req.description)
    .bind("active")
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let mapping = IpManager {
        id: ip_id,
        device_interface_id: interface_id,
        device_id: id,
        network_id,
        ip_address: req.ip_address.clone(),
        ip_version,
        mac_address: None,
        hostname: None,
        description: req.description.clone(),
        status: "active".to_string(),
        last_seen: now,
        last_mac: None,
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "device_id": id.to_string(),
        "ip_address": mapping.ip_address,
        "description": mapping.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create_device_ip",
        "device",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(mapping, "设备IP创建成功"))
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
    let mut tx = pool.begin().await?;

    let network_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM network_cidrs WHERE id = $1)")
            .bind(network_id)
            .fetch_one(&mut *tx)
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
    .fetch_all(&mut *tx)
    .await?;

    if switch_macs.is_empty() {
        let total_macs: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM switch_macs WHERE device_id = $1")
                .bind(device_id)
                .fetch_one(&mut *tx)
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
    let mut pending_notifications: Vec<(Uuid, String, String, String)> = Vec::new();

    for (ip, mac) in &switch_macs {
        let current: Option<IpMacCurrentInfo> = sqlx::query_as(
            "SELECT mac_address, device_id FROM ips WHERE ip_address = CAST($1 AS INET)",
        )
        .bind(ip)
        .fetch_optional(&mut *tx)
        .await?;

        let Some((old_mac, cur_device_id)) = current else {
            continue;
        };

        let mac_conflict: Option<String> = sqlx::query_scalar(
            r"SELECT host(ip_address) FROM ips
               WHERE mac_address = $1
               AND ip_address != CAST($2 AS INET)
               AND device_id != $3
               LIMIT 1",
        )
        .bind(mac)
        .bind(ip)
        .bind(cur_device_id)
        .fetch_optional(&mut *tx)
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
                .execute(&mut *tx)
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
                .execute(&mut *tx)
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
                .execute(&mut *tx)
                .await?;
                updated_count += 1;

                let workstation_id: Option<Uuid> = sqlx::query_scalar::<_, Option<Uuid>>(
                    "SELECT d.workstation_id FROM ips i JOIN devices d ON i.device_id = d.id WHERE i.ip_address = CAST($1 AS INET)"
                )
                .bind(ip)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();

                if let Some(ws_id) = workstation_id {
                    info!("检测到MAC地址变更: IP={}, 旧MAC={}, 新MAC={}", ip, old, mac);
                    pending_notifications.push((ws_id, ip.clone(), old.to_string(), mac.clone()));
                }
            }
        }
    }

    tx.commit().await?;

    for (ws_id, ip, old_mac, new_mac) in pending_notifications {
        match crate::utils::send_mac_change_notification(pool, &ws_id, &ip, &old_mac, &new_mac)
            .await
        {
            Ok(()) => info!("MAC地址变更通知发送成功: IP={}", ip),
            Err(e) => error!("MAC地址变更通知发送失败: IP={}, 错误: {}", ip, e),
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
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<crate::models::PullIpManagersRequest>,
) -> Result<Response, AppError> {
    req.validate()?;

    let result = sync_switch_macs(&state.pool()?.get_conn(), req.device_id, req.network_id).await?;

    if result.switch_macs_empty {
        if result.total_macs_on_switch == 0 {
            return Ok((
                StatusCode::OK,
                Json(ApiResponse::<Vec<IpManager>>::error(
                    "该设备暂无MAC数据，请先在设备管理中同步MAC表",
                )),
            )
                .into_response());
        }
        return Ok(crate::error::ok_json(
            Vec::<IpManager>::new(),
            "未发现属于该网段的已管理IP地址",
        ));
    }

    let results: Vec<IpManager> = sqlx::query_as::<_, IpManager>(
        r"SELECT m.id, m.device_interface_id, m.device_id, m.network_id,
           host(m.ip_address) as ip_address, m.ip_version, m.mac_address, m.hostname, m.description, m.status,
           m.last_seen::TIMESTAMPTZ, m.last_mac, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ
           FROM ips m
           WHERE m.network_id = $1",
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

    Ok(crate::error::ok_json(results, &message))
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

    let is_v6 = matches!(network_cidr, ipnetwork::IpNetwork::V6(_));
    let effective_max = match (max_count, is_v6) {
        (Some(max), _) => max,
        (None, true) => 100,
        (None, false) => 1000,
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
        if available.len() >= effective_max {
            break;
        }
    }
    available
}

pub async fn get_available_ips(
    State(state): State<Arc<AppState>>,
    Path(network_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let network = sqlx::query(crate::utils::NETWORK_QUERY)
        .bind(network_id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .ok_or_else(|| AppError::NotFound("网络未找到".to_string()))
        .and_then(|row| crate::utils::parse_network_from_row(&row))?;

    let used_ips: Vec<String> =
        sqlx::query_scalar("SELECT host(ip_address) FROM ips WHERE network_id = $1")
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

    Ok(crate::error::ok_json(
        serde_json::json!({
            "network_id": network_id,
            "network_name": network.name,
            "available_count": available_ips.len(),
            "available_ips": available_ips
        }),
        "获取可用IP列表成功",
    ))
}

pub async fn auto_assign_ip(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<crate::models::AutoAssignIpRequest>,
) -> Result<Response, AppError> {
    req.validate()?;

    let req_network_id = req.network_id;
    let device_id = req.device_id;
    let device_interface_id = req.device_interface_id;
    let description = req.description.clone();

    let mut tx = state.pool()?.get_conn().begin().await?;

    let device_row = sqlx::query("SELECT workstation_id, position_id FROM devices WHERE id = $1")
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("设备未找到".to_string()))?;

    let device_ws_id: Option<Uuid> = device_row.get("workstation_id");
    let device_pos_id: Option<Uuid> = device_row.get("position_id");

    let room_id = if let Some(ws_id) = device_ws_id {
        get_room_id_by_workstation(&mut *tx, ws_id).await?
    } else if let Some(pos_id) = device_pos_id {
        get_room_id_by_position(&mut *tx, pos_id).await?
    } else {
        None
    };

    if let Some(r_id) = room_id {
        validate_network_in_room(&mut *tx, r_id, Some(req_network_id)).await?;
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

    // 解析 device_interface_id：优先使用请求中的；否则使用设备的默认 physical 接口
    let interface_id = match device_interface_id {
        Some(iid) => iid,
        None => {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM device_interfaces WHERE device_id = $1 AND interface_type = 'physical' ORDER BY created_at LIMIT 1",
            )
            .bind(device_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::Validation(
                "设备没有可用的 physical 接口，请先创建接口或指定 device_interface_id".to_string()
            ))?
        }
    };

    let insert_result = sqlx::query(
        "INSERT INTO ips (id, device_interface_id, device_id, network_id, ip_address, ip_version, description, status, last_seen, created_at, updated_at)
         VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, $8, $9, $10, $11)"
    )
    .bind(id)
    .bind(interface_id)
    .bind(device_id)
    .bind(req_network_id)
    .bind(&assigned_ip)
    .bind(ip_version_num)
    .bind(&description)
    .bind("active")
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await;

    match insert_result {
        Ok(_) => {}
        Err(e) => {
            if let sqlx::Error::Database(ref db_err) = e
                && db_err.code().as_deref() == Some("23505")
            {
                return Err(AppError::Validation("IP地址已被分配，请重试".to_string()));
            }
            return Err(AppError::from(e));
        }
    }

    tx.commit().await?;

    let mapping = IpManager {
        id,
        device_interface_id: interface_id,
        device_id,
        network_id: Some(req_network_id),
        ip_address: assigned_ip.clone(),
        ip_version: ip_version_num,
        mac_address: None,
        hostname: None,
        description,
        status: "active".to_string(),
        last_seen: now,
        last_mac: None,
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "device_id": device_id.to_string(),
        "ip_address": mapping.ip_address,
        "description": mapping.description,
        "auto_assigned": true
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "auto_assign_ip",
        "ip_manager",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(mapping, "IP地址自动分配成功"))
}

pub async fn auto_assign_device_ip(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<crate::models::AutoAssignIpRequest>,
) -> Result<Response, AppError> {
    let mut req = req;
    req.device_id = id;
    auto_assign_ip(State(state), meta, AppJson(req)).await
}

pub async fn batch_create_ip_managers(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<Vec<IpManagerCreate>>,
) -> Result<Response, AppError> {
    let now = Utc::now();
    let mut valid_requests: Vec<(usize, &IpManagerCreate, Uuid, i16)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for (index, ip_req) in req.iter().enumerate() {
        if let Err(e) = ip_req.validate() {
            errors.push(format!("第{}条记录验证失败: {:?}", index + 1, e));
            continue;
        }

        if ip_req.device_id.is_none() {
            errors.push(format!("第{}条记录: device_id不能为空", index + 1));
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

        let device_id = match ip_req.device_id {
            Some(d) => d,
            None => {
                duplicate_errors.push(format!("第{}条记录: device_id不能为空", index + 1));
                continue;
            }
        };

        let interface_id = match ip_req.device_interface_id {
            Some(iid) => iid,
            None => {
                match sqlx::query_scalar::<_, Uuid>(
                    "SELECT id FROM device_interfaces WHERE device_id = $1 AND interface_type = 'physical' ORDER BY created_at LIMIT 1",
                )
                .bind(device_id)
                .fetch_optional(tx.as_mut())
                .await
                {
                    Ok(Some(iid)) => iid,
                    Ok(None) => {
                        duplicate_errors.push(format!(
                            "第{}条记录: 设备没有可用的 physical 接口，请先创建接口或指定 device_interface_id",
                            index + 1
                        ));
                        continue;
                    }
                    Err(err) => {
                        duplicate_errors.push(format!(
                            "第{}条记录: 查询接口失败 - {}",
                            index + 1,
                            err
                        ));
                        continue;
                    }
                }
            }
        };

        if let Err(err) = sqlx::query(
            "INSERT INTO ips (id, device_interface_id, device_id, network_id, ip_address, ip_version, description, status, last_seen, created_at, updated_at)
             VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, $8, $9, $10, $11)",
        )
        .bind(*id)
        .bind(interface_id)
        .bind(device_id)
        .bind(ip_req.network_id)
        .bind(&ip_req.ip_address)
        .bind(*ip_version_num)
        .bind(&ip_req.description)
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
            device_interface_id: interface_id,
            device_id,
            network_id: ip_req.network_id,
            ip_address: ip_req.ip_address.clone(),
            ip_version: *ip_version_num,
            mac_address: None,
            hostname: None,
            description: ip_req.description.clone(),
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
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "batch_create",
        "ip_manager",
        None,
        &details,
    )
    .await;

    Ok(crate::error::ok_json(
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
    ))
}
