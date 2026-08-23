//! IP 地址管理：列表检索、设备 IP 绑定与 MAC 同步。

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
use crate::utils::pagination::{Pagination, paged_response};
use crate::utils::validate_network_in_room;
use chrono::Utc;
use ipma_common::{AppMessage, log_error, log_info, log_warn, msg};
use std::net::IpAddr;
use std::str::FromStr;
use uuid::Uuid;
use validator::Validate;

/// IP 列表过滤条件（经解析与转义后的形态）。
struct IpListFilters {
    /// 全局搜索关键字（转义 ILIKE 通配符）。
    search: Option<String>,
    status: Option<String>,
    device_name: Option<String>,
    network: Option<String>,
    ip_address: Option<String>,
    network_id: Option<Uuid>,
    /// 机位 ID 批量过滤（机柜可视化分批拉取机位 IP）。
    position_ids: Vec<Uuid>,
}

/// 追加 IP 列表过滤条件，供 COUNT 与数据查询共用。
///
/// 所有 ILIKE 模式均经 `escape_like` 转义（`%`/`_`/`\` 按字面匹配），
/// 值通过 `push_bind` 参数绑定。
fn push_ip_filters(builder: &mut sqlx::QueryBuilder<sqlx::Postgres>, filters: &IpListFilters) {
    let mut first = true;
    let next = |builder: &mut sqlx::QueryBuilder<sqlx::Postgres>, first: &mut bool| {
        let prefix = if *first {
            *first = false;
            " WHERE ("
        } else {
            " AND ("
        };
        builder.push(prefix);
    };

    if let Some(pattern) = &filters.search {
        next(builder, &mut first);
        builder
            .push("ip_address::TEXT ILIKE ")
            .push_bind(pattern)
            .push(" OR mac_address ILIKE ")
            .push_bind(pattern)
            .push(" OR hostname ILIKE ")
            .push_bind(pattern)
            .push(" OR description ILIKE ")
            .push_bind(pattern)
            .push(" OR device_name ILIKE ")
            .push_bind(pattern)
            .push(" OR workstation_name ILIKE ")
            .push_bind(pattern)
            .push(" OR cabinet_position_name ILIKE ")
            .push_bind(pattern)
            .push(" OR network_name ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
    if let Some(status) = &filters.status {
        next(builder, &mut first);
        builder.push("status = ").push_bind(status).push(")");
    }
    if let Some(pattern) = &filters.device_name {
        next(builder, &mut first);
        builder
            .push("device_name ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
    if let Some(pattern) = &filters.network {
        next(builder, &mut first);
        builder
            .push("network_name ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
    if let Some(pattern) = &filters.ip_address {
        next(builder, &mut first);
        builder
            .push("ip_address::TEXT ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
    if let Some(network_id) = filters.network_id {
        next(builder, &mut first);
        builder
            .push("network_id = ")
            .push_bind(network_id)
            .push(")");
    }
    if !filters.position_ids.is_empty() {
        next(builder, &mut first);
        builder
            .push("position_id = ANY(")
            .push_bind(filters.position_ids.clone())
            .push("))");
    }
}

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
    let pagination = Pagination::from_query(&query);

    let filters = IpListFilters {
        search: (!search.is_empty()).then(|| crate::utils::escape_like(search)),
        status: (!status.is_empty()).then(|| status.to_string()),
        device_name: (!device_name.is_empty()).then(|| crate::utils::escape_like(device_name)),
        network: (!network.is_empty()).then(|| crate::utils::escape_like(network)),
        ip_address: (!ip_address.is_empty()).then(|| crate::utils::escape_like(ip_address)),
        network_id: query
            .get("network_id")
            .and_then(|s| uuid::Uuid::parse_str(s).ok()),
        // 逗号分隔的机位 ID 列表，非法片段直接忽略
        position_ids: query
            .get("position_ids")
            .map(|s| {
                s.split(',')
                    .filter_map(|part| uuid::Uuid::parse_str(part.trim()).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    };

    let sort_by = query.get("sort_by").cloned().unwrap_or_default();
    let sort_order = query.get("sort_order").cloned().unwrap_or_default();

    // ORDER BY 白名单，未匹配时回落默认序，避免注入。
    // 视图列 ip_address 为 host() 输出的 TEXT，直排是字典序（"10.0.0.10" <
    // "10.0.0.2"）；转回 inet 后按数值大小排序（db-schema-review 第九节）
    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("device_name", "desc") => "ORDER BY device_name DESC NULLS LAST, updated_at DESC",
        ("device_name", _) => "ORDER BY device_name ASC NULLS LAST, updated_at DESC",
        ("device_type", "desc") => "ORDER BY device_type DESC, updated_at DESC",
        ("device_type", _) => "ORDER BY device_type ASC, updated_at DESC",
        ("network_name", "desc") => "ORDER BY network_name DESC NULLS LAST, updated_at DESC",
        ("network_name", _) => "ORDER BY network_name ASC NULLS LAST, updated_at DESC",
        ("ip_address", "desc") => "ORDER BY ip_with_details.ip_address::inet DESC, updated_at DESC",
        ("ip_address", _) => "ORDER BY ip_with_details.ip_address::inet ASC, updated_at DESC",
        ("mac_address", "desc") => "ORDER BY mac_address DESC NULLS LAST, updated_at DESC",
        ("mac_address", _) => "ORDER BY mac_address ASC NULLS LAST, updated_at DESC",
        ("hostname", "desc") => "ORDER BY hostname DESC NULLS LAST, updated_at DESC",
        ("hostname", _) => "ORDER BY hostname ASC NULLS LAST, updated_at DESC",
        ("status", "desc") => "ORDER BY status DESC, updated_at DESC",
        ("status", _) => "ORDER BY status ASC, updated_at DESC",
        ("last_seen", "desc") => "ORDER BY last_seen DESC NULLS LAST",
        ("last_seen", _) => "ORDER BY last_seen ASC NULLS LAST",
        ("created_at", "desc") => "ORDER BY created_at DESC",
        ("created_at", _) => "ORDER BY created_at ASC",
        ("updated_at", "asc") => "ORDER BY updated_at ASC",
        _ => "ORDER BY updated_at DESC",
    };

    let mut count_builder =
        sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT COUNT(*) FROM ip_with_details");
    push_ip_filters(&mut count_builder, &filters);
    let total: i64 = count_builder
        .build_query_scalar()
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let mut data_builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT id, device_interface_id, device_id, device_type, device_name, interface_name, physical_type, interface_role, network_id, workstation_name, cabinet_position_name, room_name, cabinet_name, org_name, network_name, network_region, ip_address::TEXT as ip_address, ip_version, mac_address, hostname, description, status, last_seen, created_at, updated_at, position_id FROM ip_with_details",
    );
    push_ip_filters(&mut data_builder, &filters);
    data_builder
        .push(" ")
        .push(order_clause)
        .push(" LIMIT ")
        .push_bind(pagination.page_size as i32)
        .push(" OFFSET ")
        .push_bind(pagination.offset as i32);
    let mappings = data_builder
        .build_query_as::<IpManagerWithNames>()
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    Ok(crate::error::ok_json(
        paged_response(mappings, total, &pagination),
        "server.ip.fetched",
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
        return Err(AppError::NotFound(
            msg("server.device.not_found").with("id", id),
        ));
    }

    let ips: Vec<IpManager> = sqlx::query_as(
        r"SELECT
            m.id, m.device_interface_id, di.device_id, m.network_id,
            nc.network_region_id AS network_region_id,
            nc.name AS network_name,
            nr.name AS network_region,
            host(m.ip_address) as ip_address,
            m.ip_version, di.mac_address AS mac_address, m.description,
            m.status, m.last_seen, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ
        FROM ips m
        JOIN device_interfaces di ON m.device_interface_id = di.id
        LEFT JOIN network_cidrs nc ON m.network_id = nc.id
        LEFT JOIN network_regions nr ON nc.network_region_id = nr.id
        WHERE di.device_id = $1
        ORDER BY m.ip_address",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(crate::error::ok_json(
        serde_json::json!({ "items": ips }),
        "server.ip.device_list_fetched",
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

    let room_id: Uuid = sqlx::query_scalar("SELECT room_id FROM devices WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(msg("server.device.not_found").with("id", id)))?;

    let existing_ip: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM ips WHERE ip_address = CAST($1 AS INET)")
            .bind(&req.ip_address)
            .fetch_optional(&mut *tx)
            .await?;

    if existing_ip.is_some() {
        return Err(AppError::Conflict(
            msg("server.ip.already_exists").with("ip", &req.ip_address),
        ));
    }

    let network_id: Option<Uuid> = sqlx::query_scalar(
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
    .bind(room_id)
    .bind(&req.ip_address)
    .fetch_optional(&mut *tx)
    .await?;

    // 未命中房间网段时回落到请求指定的网段，但必须属于该房间
    let network_id = match network_id {
        Some(nid) => Some(nid),
        None => match req.network_id {
            Some(nid) => {
                validate_network_in_room(&mut *tx, room_id, Some(nid)).await?;
                Some(nid)
            }
            None => None,
        },
    };

    let ip_version = detect_ip_version(&req.ip_address)?;
    let now = Utc::now();
    let ip_id = Uuid::new_v4();

    // 解析 device_interface_id：优先使用请求中的（须属于该设备）；
    // 否则使用设备的默认物理接口
    let interface_id = match req.device_interface_id {
        Some(iid) => {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM device_interfaces WHERE id = $1 AND device_id = $2",
            )
            .bind(iid)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| {
                AppError::Validation(msg("server.ip.interface_invalid"))
            })?
        }
        None => {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM device_interfaces WHERE device_id = $1 AND physical_type <> 'virtual' ORDER BY created_at LIMIT 1",
            )
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| {
                AppError::Validation(msg("server.ip.no_physical_interface"))
            })?
        }
    };

    sqlx::query(
        "INSERT INTO ips (id, device_interface_id, network_id, ip_address, ip_version, description, status, last_seen, created_at, updated_at)
         VALUES ($1, $2, $3, CAST($4 AS INET), $5, $6, $7, $8, $9, $10)",
    )
    .bind(ip_id)
    .bind(interface_id)
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

    let (network_name, network_region, network_region_id): (Option<String>, Option<String>, Option<Uuid>) = match network_id {
        Some(nid) => sqlx::query_as(
            "SELECT nc.name, nr.name, nc.network_region_id FROM network_cidrs nc LEFT JOIN network_regions nr ON nc.network_region_id = nr.id WHERE nc.id = $1",
        )
        .bind(nid)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .map_or((None, None, None), |(name, region, region_id)| {
            (Some(name), Some(region), Some(region_id))
        }),
        None => (None, None, None),
    };

    let mapping = IpManager {
        id: ip_id,
        device_interface_id: interface_id,
        device_id: id,
        network_id,
        network_region_id,
        network_name,
        network_region,
        ip_address: req.ip_address.clone(),
        ip_version,
        mac_address: None,
        description: req.description.clone(),
        status: "active".to_string(),
        last_seen: now,
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

    Ok(crate::error::ok_json(mapping, "server.ip.created"))
}

struct MacSyncResult {
    updated_count: usize,
    unchanged_count: usize,
    skipped_count: usize,
    switch_macs_empty: bool,
    total_macs_on_switch: i64,
}

/// 单网口的同步聚合状态：同一网口多 IP 时按网口去重，
/// 仅执行一次 MAC 更新与一次变更通知。
struct InterfaceSyncState {
    device_id: Uuid,
    workstation_id: Option<Uuid>,
    /// 同步前的网口 MAC 快照（等价旧 ips.mac_address 语义）
    old_mac: Option<String>,
    /// 本次 SNMP 观测到的 MAC
    new_mac: String,
    /// 该网口本次观测到的 IP 行（用于刷新 last_seen 与通知内容）
    observed_ip_ids: Vec<Uuid>,
    observed_ip_addrs: Vec<String>,
}

/// SNMP 观测行：IP 与观测 MAC，连同其所属网口的当前快照。
#[derive(Debug, sqlx::FromRow)]
struct ObservedMacRow {
    ip: String,
    mac: String,
    ip_row_id: Uuid,
    iface_id: Uuid,
    iface_mac: Option<String>,
    iface_device_id: Uuid,
    workstation_id: Option<Uuid>,
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
        return Err(AppError::Validation(msg("server.network.not_found")));
    }

    // 一次性取出观测数据与网口快照
    let observed: Vec<ObservedMacRow> = sqlx::query_as(
        r"SELECT host(sm.ip_address) AS ip, sm.mac_address AS mac,
                  i.id AS ip_row_id, di.id AS iface_id, di.mac_address AS iface_mac,
                  di.device_id AS iface_device_id, dv.workstation_id AS workstation_id
          FROM device_macs sm
          INNER JOIN ips i ON sm.ip_address = i.ip_address AND i.network_id = $2
          INNER JOIN device_interfaces di ON i.device_interface_id = di.id
          INNER JOIN devices dv ON di.device_id = dv.id
          WHERE sm.device_id = $1",
    )
    .bind(device_id)
    .bind(network_id)
    .fetch_all(&mut *tx)
    .await?;

    if observed.is_empty() {
        let total_macs: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM device_macs WHERE device_id = $1")
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
    let mut interfaces: std::collections::HashMap<Uuid, InterfaceSyncState> =
        std::collections::HashMap::new();
    let mut pending_notifications: Vec<(Uuid, String, String, String)> = Vec::new();

    for row in observed {
        let ObservedMacRow {
            ip,
            mac,
            ip_row_id,
            iface_id,
            iface_mac,
            iface_device_id,
            workstation_id,
        } = row;
        let entry = interfaces
            .entry(iface_id)
            .or_insert_with(|| InterfaceSyncState {
                device_id: iface_device_id,
                workstation_id,
                old_mac: iface_mac,
                new_mac: mac.clone(),
                observed_ip_ids: Vec::new(),
                observed_ip_addrs: Vec::new(),
            });

        // 同一网口在一次观测中出现不同 MAC 属异常数据，按冲突跳过处理
        if entry.new_mac != mac {
            log_warn!(
                "log.ip.mac_observation_conflict",
                interface = iface_id,
                existing_mac = entry.new_mac,
                new_mac = mac,
                ip = ip
            );
            skipped_count += 1;
            continue;
        }

        entry.observed_ip_ids.push(ip_row_id);
        entry.observed_ip_addrs.push(ip.clone());
    }

    for (iface_id, iface) in &interfaces {
        // MAC 冲突检测：同一 MAC 已被其他设备的网口使用则跳过
        let mac_conflict: Option<String> = sqlx::query_scalar(
            r"SELECT host(i.ip_address) FROM ips i
               JOIN device_interfaces di ON i.device_interface_id = di.id
               WHERE di.mac_address = $1
               AND di.device_id != $2
               AND di.id != $3
               LIMIT 1",
        )
        .bind(&iface.new_mac)
        .bind(iface.device_id)
        .bind(iface_id)
        .fetch_optional(&mut *tx)
        .await?
        .flatten();

        if let Some(conflict_ip) = mac_conflict {
            log_warn!("log.ip.mac_conflict", mac = iface.new_mac, ip = conflict_ip);
            skipped_count += iface.observed_ip_ids.len();
            continue;
        }

        // 观测到的 IP 行刷新 last_seen（updated_at 由触发器维护）
        sqlx::query("UPDATE ips SET last_seen = $1 WHERE id = ANY($2)")
            .bind(now)
            .bind(&iface.observed_ip_ids)
            .execute(&mut *tx)
            .await?;

        match iface.old_mac.as_deref() {
            None | Some("") => {
                sqlx::query("UPDATE device_interfaces SET mac_address = $1 WHERE id = $2")
                    .bind(&iface.new_mac)
                    .bind(iface_id)
                    .execute(&mut *tx)
                    .await?;
                updated_count += iface.observed_ip_ids.len();
                log_info!(
                    "log.ip.mac_written",
                    interface = iface_id,
                    mac = iface.new_mac
                );
            }
            Some(old) if old == iface.new_mac => {
                unchanged_count += iface.observed_ip_ids.len();
            }
            Some(old) => {
                sqlx::query("UPDATE device_interfaces SET mac_address = $1 WHERE id = $2")
                    .bind(&iface.new_mac)
                    .bind(iface_id)
                    .execute(&mut *tx)
                    .await?;
                updated_count += iface.observed_ip_ids.len();

                if let Some(ws_id) = iface.workstation_id {
                    // 多个 IP 以逗号连接作为通知参数值，避免语言相关分隔符
                    let ips = iface.observed_ip_addrs.join(", ");
                    log_info!(
                        "log.ip.mac_changed",
                        interface = iface_id,
                        ip = ips,
                        old_mac = old,
                        new_mac = iface.new_mac
                    );
                    pending_notifications.push((
                        ws_id,
                        ips,
                        old.to_string(),
                        iface.new_mac.clone(),
                    ));
                }
            }
        }
    }

    tx.commit().await?;

    for (ws_id, ip, old_mac, new_mac) in pending_notifications {
        match crate::utils::send_mac_change_notification(pool, &ws_id, &ip, &old_mac, &new_mac)
            .await
        {
            Ok(()) => log_info!("log.ip.mac_change_notification_sent", ip = ip),
            Err(e) => {
                log_error!("log.ip.mac_change_notification_failed", ip = ip, error = e)
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
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<crate::models::PullIpManagersRequest>,
) -> Result<Response, AppError> {
    req.validate()?;

    let result = sync_switch_macs(&state.pool()?.get_conn(), req.device_id, req.network_id).await?;

    if result.switch_macs_empty {
        if result.total_macs_on_switch == 0 {
            return Ok((
                StatusCode::OK,
                Json(ApiResponse::<Vec<IpManager>>::error(msg(
                    "server.ip.no_mac_data",
                ))),
            )
                .into_response());
        }
        return Ok(crate::error::ok_json(
            Vec::<IpManager>::new(),
            "server.ip.no_managed_ips",
        ));
    }

    let results: Vec<IpManager> = sqlx::query_as::<_, IpManager>(
        r"SELECT m.id, m.device_interface_id, di.device_id, m.network_id,
           nc.network_region_id AS network_region_id,
           nc.name AS network_name, nr.name AS network_region,
           host(m.ip_address) as ip_address, m.ip_version, di.mac_address AS mac_address, m.description, m.status,
           m.last_seen::TIMESTAMPTZ, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ
           FROM ips m
           JOIN device_interfaces di ON m.device_interface_id = di.id
           LEFT JOIN network_cidrs nc ON m.network_id = nc.id
           LEFT JOIN network_regions nr ON nc.network_region_id = nr.id
           WHERE m.network_id = $1",
    )
    .bind(req.network_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    // 同步结果以 key + 计数参数返回，由前端按语言翻译汇总文案
    let message =
        if result.updated_count == 0 && result.unchanged_count == 0 && result.skipped_count == 0 {
            msg("server.ip.mac_sync_no_change")
        } else {
            msg("server.ip.mac_sync_completed")
                .with("updated", result.updated_count)
                .with("unchanged", result.unchanged_count)
                .with("skipped", result.skipped_count)
        };

    Ok(crate::error::ok_json(results, message))
}

pub async fn pull_ip_managers_internal(
    pool: &sqlx::PgPool,
    device_id: Uuid,
    network_id: Uuid,
) -> Result<(), String> {
    let result = sync_switch_macs(pool, device_id, network_id)
        .await
        .map_err(|e| {
            log_error!("log.ip.mac_sync_failed", error = e);
            "server.ip.mac_sync_failed".to_string()
        })?;

    if result.switch_macs_empty {
        return Err("server.ip.no_managed_ips".to_string());
    }

    Ok(())
}

pub fn detect_ip_version(ip: &str) -> Result<i16, AppError> {
    match IpAddr::from_str(ip) {
        Ok(IpAddr::V6(_)) => Ok(6),
        Ok(IpAddr::V4(_)) => Ok(4),
        Err(e) => Err(AppError::Validation(
            msg("server.ip.invalid_ip").with("ip", ip).with("error", e),
        )),
    }
}

pub fn find_available_ips_in_cidr(
    cidr_str: &str,
    gateway: Option<&String>,
    used_ips: &std::collections::HashSet<String>,
    max_count: Option<usize>,
) -> Vec<String> {
    let Ok(network_cidr) = ipnetwork::IpNetwork::from_str(cidr_str) else {
        log_warn!("log.ip.invalid_cidr", cidr = cidr_str);
        return Vec::new();
    };

    let is_v6 = matches!(network_cidr, ipnetwork::IpNetwork::V6(_));
    let effective_max = match (max_count, is_v6) {
        (Some(max), _) => max,
        (None, true) => 100,
        (None, false) => 1000,
    };
    // max_count=Some(0) 语义为不需要任何地址：此前循环先压入再判长度，
    // 仍会返回 1 个地址（security-review 第六节）
    if effective_max == 0 {
        return Vec::new();
    }

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
        .ok_or_else(|| AppError::NotFound(msg("server.network.not_found")))
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
            "network_region": network.network_region,
            "available_count": available_ips.len(),
            "available_ips": available_ips
        }),
        "server.ip.available_fetched",
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

    let room_id: Uuid = sqlx::query_scalar("SELECT room_id FROM devices WHERE id = $1")
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(msg("server.device.not_found").with("id", device_id)))?;

    validate_network_in_room(&mut *tx, room_id, Some(req_network_id)).await?;

    let network = sqlx::query(crate::utils::NETWORK_QUERY)
        .bind(req_network_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(msg("server.network.not_found")))
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
        .ok_or_else(|| AppError::Validation(msg("server.ip.no_available_ips")))?;

    let id = Uuid::new_v4();
    let now = Utc::now();
    let ip_version_num = detect_ip_version(&assigned_ip)?;

    // 解析 device_interface_id：优先使用请求中的（须属于该设备）；
    // 否则使用设备的默认物理接口
    let interface_id = match device_interface_id {
        Some(iid) => {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM device_interfaces WHERE id = $1 AND device_id = $2",
            )
            .bind(iid)
            .bind(device_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| {
                AppError::Validation(msg("server.ip.interface_invalid"))
            })?
        }
        None => {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM device_interfaces WHERE device_id = $1 AND physical_type <> 'virtual' ORDER BY created_at LIMIT 1",
            )
            .bind(device_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| {
                AppError::Validation(msg("server.ip.no_physical_interface"))
            })?
        }
    };

    let insert_result = sqlx::query(
        "INSERT INTO ips (id, device_interface_id, network_id, ip_address, ip_version, description, status, last_seen, created_at, updated_at)
         VALUES ($1, $2, $3, CAST($4 AS INET), $5, $6, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(interface_id)
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
                return Err(AppError::Validation(msg("server.ip.already_assigned")));
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
        network_region_id: None,
        network_name: Some(network.name.clone()),
        network_region: Some(network.network_region.clone()),
        ip_address: assigned_ip.clone(),
        ip_version: ip_version_num,
        mac_address: None,
        description,
        status: "active".to_string(),
        last_seen: now,
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

    Ok(crate::error::ok_json(mapping, "server.ip.auto_assigned"))
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
    // 逐条错误以「key + 参数」记录，随响应返回由前端翻译
    let mut errors: Vec<AppMessage> = Vec::new();

    for (index, ip_req) in req.iter().enumerate() {
        if let Err(e) = ip_req.validate() {
            log_warn!("log.ip.batch_record_failed", index = index + 1, error = e);
            errors.push(msg("server.ip.batch_record_validation_failed").with("index", index + 1));
            continue;
        }

        if ip_req.device_id.is_none() {
            errors.push(msg("server.ip.batch_record_device_id_required").with("index", index + 1));
            continue;
        }

        let id = Uuid::new_v4();
        let ip_version_num = match detect_ip_version(&ip_req.ip_address) {
            Ok(v) => v,
            Err(e) => {
                log_warn!("log.ip.batch_record_failed", index = index + 1, error = e);
                errors.push(msg("server.ip.batch_record_invalid_ip").with("index", index + 1));
                continue;
            }
        };
        valid_requests.push((index, ip_req, id, ip_version_num));
    }

    if valid_requests.is_empty() {
        return Err(AppError::Validation(msg(
            "server.ip.batch_no_valid_records",
        )));
    }

    let mut tx = state.pool()?.get_conn().begin().await?;

    // 批量预取网段与区域名称，保证返回的每条 IP 都带所属网段/区域
    /// 网段摘要：(网段名, 区域名, 区域ID)
    type NetworkSummary = (Option<String>, Option<String>, Option<Uuid>);
    let network_ids: Vec<Uuid> = valid_requests
        .iter()
        .filter_map(|(_, ip_req, _, _)| ip_req.network_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let mut network_names: std::collections::HashMap<Uuid, NetworkSummary> =
        std::collections::HashMap::new();
    for nid in network_ids {
        let row: Option<(String, String, Uuid)> = sqlx::query_as(
            "SELECT nc.name, nr.name, nc.network_region_id FROM network_cidrs nc LEFT JOIN network_regions nr ON nc.network_region_id = nr.id WHERE nc.id = $1",
        )
        .bind(nid)
        .fetch_optional(tx.as_mut())
        .await?;
        if let Some((name, region, region_id)) = row {
            network_names.insert(nid, (Some(name), Some(region), Some(region_id)));
        }
    }

    let mut created_ips = Vec::new();
    let mut duplicate_errors: Vec<AppMessage> = Vec::new();

    for (index, ip_req, id, ip_version_num) in &valid_requests {
        let existing: Option<Uuid> =
            match sqlx::query_scalar("SELECT id FROM ips WHERE ip_address = CAST($1 AS INET)")
                .bind(&ip_req.ip_address)
                .fetch_optional(tx.as_mut())
                .await
            {
                Ok(opt) => opt,
                Err(err) => {
                    log_warn!("log.ip.batch_record_failed", index = index + 1, error = err);
                    duplicate_errors
                        .push(msg("server.ip.batch_record_query_failed").with("index", index + 1));
                    continue;
                }
            };

        if existing.is_some() {
            duplicate_errors.push(
                msg("server.ip.batch_record_ip_exists")
                    .with("index", index + 1)
                    .with("ip", &ip_req.ip_address),
            );
            continue;
        }

        let device_id = match ip_req.device_id {
            Some(d) => d,
            None => {
                duplicate_errors.push(
                    msg("server.ip.batch_record_device_id_required").with("index", index + 1),
                );
                continue;
            }
        };

        let interface_id = match ip_req.device_interface_id {
            Some(iid) => {
                match sqlx::query_scalar::<_, Uuid>(
                    "SELECT id FROM device_interfaces WHERE id = $1 AND device_id = $2",
                )
                .bind(iid)
                .bind(device_id)
                .fetch_optional(tx.as_mut())
                .await
                {
                    Ok(Some(found)) => found,
                    Ok(None) => {
                        duplicate_errors.push(
                            msg("server.ip.batch_record_interface_invalid")
                                .with("index", index + 1),
                        );
                        continue;
                    }
                    Err(err) => {
                        log_warn!("log.ip.batch_record_failed", index = index + 1, error = err);
                        duplicate_errors.push(
                            msg("server.ip.batch_record_query_failed").with("index", index + 1),
                        );
                        continue;
                    }
                }
            }
            None => {
                match sqlx::query_scalar::<_, Uuid>(
                    "SELECT id FROM device_interfaces WHERE device_id = $1 AND physical_type <> 'virtual' ORDER BY created_at LIMIT 1",
                )
                .bind(device_id)
                .fetch_optional(tx.as_mut())
                .await
                {
                    Ok(Some(iid)) => iid,
                    Ok(None) => {
                        duplicate_errors.push(
                            msg("server.ip.batch_record_no_physical_interface")
                                .with("index", index + 1),
                        );
                        continue;
                    }
                    Err(err) => {
                        log_warn!("log.ip.batch_record_failed", index = index + 1, error = err);
                        duplicate_errors.push(
                            msg("server.ip.batch_record_query_failed").with("index", index + 1),
                        );
                        continue;
                    }
                }
            }
        };

        if let Err(err) = sqlx::query(
            "INSERT INTO ips (id, device_interface_id, network_id, ip_address, ip_version, description, status, last_seen, created_at, updated_at)
             VALUES ($1, $2, $3, CAST($4 AS INET), $5, $6, $7, $8, $9, $10)",
        )
        .bind(*id)
        .bind(interface_id)
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
            log_warn!("log.ip.batch_record_failed", index = index + 1, error = err);
            duplicate_errors.push(
                msg("server.ip.batch_record_insert_failed").with("index", index + 1),
            );
            continue;
        }

        created_ips.push(IpManager {
            id: *id,
            device_interface_id: interface_id,
            device_id,
            network_id: ip_req.network_id,
            network_region_id: ip_req
                .network_id
                .and_then(|nid| network_names.get(&nid))
                .and_then(|(_, _, region_id)| *region_id),
            network_name: ip_req
                .network_id
                .and_then(|nid| network_names.get(&nid))
                .and_then(|(name, _, _)| name.clone()),
            network_region: ip_req
                .network_id
                .and_then(|nid| network_names.get(&nid))
                .and_then(|(_, region, _)| region.clone()),
            ip_address: ip_req.ip_address.clone(),
            ip_version: *ip_version_num,
            mac_address: None,
            description: ip_req.description.clone(),
            status: "active".to_string(),
            last_seen: now,
            created_at: now,
            updated_at: now,
        });
    }

    tx.commit().await?;

    errors.extend(duplicate_errors);

    // 操作日志中的错误明细以 key(k=v) 诊断串形式记录，便于排查
    let details = serde_json::json!({
        "created_count": created_ips.len(),
        "error_count": errors.len(),
        "errors": errors.iter().map(AppMessage::log_string).collect::<Vec<_>>()
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

    // 响应中的错误列表序列化为 {key, params} 结构，由前端翻译展示
    let error_items: Vec<serde_json::Value> = errors
        .iter()
        .map(|e| serde_json::json!({ "key": e.key(), "params": e.params_map() }))
        .collect();

    Ok(crate::error::ok_json(
        serde_json::json!({
            "created": created_ips,
            "created_count": created_ips.len(),
            "errors": error_items,
            "error_count": errors.len()
        }),
        msg("server.ip.batch_created")
            .with("created", created_ips.len())
            .with("failed", errors.len()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // ==================== IP 版本识别 ====================

    #[test]
    fn test_detect_ip_version_ipv4_valid() {
        for ip in [
            "192.168.1.1",
            "10.0.0.1",
            "172.16.254.254",
            "0.0.0.0",
            "255.255.255.255",
        ] {
            let version =
                detect_ip_version(ip).unwrap_or_else(|e| panic!("合法 IPv4 {ip} 应识别成功: {e}"));
            assert_eq!(version, 4, "IPv4 地址应返回版本 4: {ip}");
        }
    }

    #[test]
    fn test_detect_ip_version_ipv6_valid() {
        for ip in [
            "::1",
            "2001:db8::1",
            "fe80::1",
            "::",
            "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
        ] {
            let version =
                detect_ip_version(ip).unwrap_or_else(|e| panic!("合法 IPv6 {ip} 应识别成功: {e}"));
            assert_eq!(version, 6, "IPv6 地址应返回版本 6: {ip}");
        }
    }

    #[test]
    fn test_detect_ip_version_ipv4_mapped_ipv6_is_v6() {
        // IPv4 映射的 IPv6 地址按 IPv6 处理
        let version = detect_ip_version("::ffff:192.168.1.1")
            .unwrap_or_else(|e| panic!("映射地址应识别成功: {e}"));
        assert_eq!(version, 6);
    }

    #[test]
    fn test_detect_ip_version_invalid_inputs() {
        // 各类非法输入应返回 Validation 错误
        for invalid in [
            "",               // 空串
            "abc",            // 非数字
            "192.168.1",      // IPv4 缺段
            "1.2.3.4.5",      // IPv4 多段
            "300.1.1.1",      // 超出 0-255
            "192.168.1.1/24", // 带掩码后缀不被接受
            "2001:db8:",      // IPv6 截断
            "::gg::",         // 非法十六进制
            "192.168.1.1 ",   // 带空白
        ] {
            let result = detect_ip_version(invalid);
            let err = result
                .err()
                .unwrap_or_else(|| panic!("非法输入 {invalid:?} 应被拒绝"));
            assert!(
                matches!(err, AppError::Validation(_)),
                "应返回 Validation 错误，实际: {err}"
            );
        }
    }

    // ==================== 网段可用 IP 计算 ====================

    #[test]
    fn test_find_available_invalid_cidr_returns_empty() {
        // 非法 CIDR 返回空列表而非 panic
        for bad in ["not-a-cidr", "192.168.1.1", "", "10.0.0.0/33", "::/129"] {
            let result = find_available_ips_in_cidr(bad, None, &HashSet::new(), None);
            assert!(result.is_empty(), "非法 CIDR {bad:?} 应返回空列表");
        }
    }

    #[test]
    fn test_find_available_ipv4_excludes_network_and_broadcast() {
        // /24 网段应排除网络地址与广播地址
        let available = find_available_ips_in_cidr("192.168.1.0/24", None, &HashSet::new(), None);
        assert_eq!(available.len(), 254, "/24 应有 254 个可用地址");
        assert_eq!(available.first().map(String::as_str), Some("192.168.1.1"));
        assert_eq!(available.last().map(String::as_str), Some("192.168.1.254"));
        assert!(
            !available.contains(&"192.168.1.0".to_string()),
            "网络地址应被排除"
        );
        assert!(
            !available.contains(&"192.168.1.255".to_string()),
            "广播地址应被排除"
        );
    }

    #[test]
    fn test_find_available_excludes_used_and_gateway() {
        let mut used = HashSet::new();
        used.insert("10.0.0.1".to_string());
        let gateway = "10.0.0.2".to_string();
        let available = find_available_ips_in_cidr("10.0.0.0/24", Some(&gateway), &used, None);
        // 已占用与网关均应被跳过，首个可用为 .3
        assert!(
            !available.contains(&"10.0.0.1".to_string()),
            "已占用 IP 应被排除"
        );
        assert!(
            !available.contains(&"10.0.0.2".to_string()),
            "网关 IP 应被排除"
        );
        assert_eq!(available.first().map(String::as_str), Some("10.0.0.3"));
        assert_eq!(available.len(), 252, "254 - 1(占用) - 1(网关) = 252");
    }

    #[test]
    fn test_find_available_respects_max_count() {
        // max_count 截断结果数量
        let available = find_available_ips_in_cidr("10.0.0.0/24", None, &HashSet::new(), Some(3));
        assert_eq!(available.len(), 3);
        assert_eq!(
            available,
            vec![
                "10.0.0.1".to_string(),
                "10.0.0.2".to_string(),
                "10.0.0.3".to_string()
            ]
        );
    }

    #[test]
    fn test_find_available_ipv4_default_cap_1000() {
        // 未指定上限时 IPv4 默认最多 1000 个：/22 覆盖验证
        let available = find_available_ips_in_cidr("10.1.0.0/22", None, &HashSet::new(), None);
        assert_eq!(available.len(), 1000, "IPv4 默认上限应为 1000");
    }

    #[test]
    fn test_find_available_ipv6_no_broadcast_and_default_cap_100() {
        // IPv6 无广播地址概念，仅排除网络地址；默认上限 100
        let available = find_available_ips_in_cidr("2001:db8::/64", None, &HashSet::new(), None);
        assert_eq!(available.len(), 100, "IPv6 默认上限应为 100");
        assert_eq!(available.first().map(String::as_str), Some("2001:db8::1"));
        assert!(
            !available.contains(&"2001:db8::".to_string()),
            "网络地址应被排除"
        );
    }

    #[test]
    fn test_find_available_ipv6_excludes_used_and_gateway() {
        let mut used = HashSet::new();
        used.insert("2001:db8::1".to_string());
        let gateway = "2001:db8::2".to_string();
        let available = find_available_ips_in_cidr("2001:db8::/64", Some(&gateway), &used, Some(5));
        assert_eq!(
            available,
            vec![
                "2001:db8::3".to_string(),
                "2001:db8::4".to_string(),
                "2001:db8::5".to_string(),
                "2001:db8::6".to_string(),
                "2001:db8::7".to_string(),
            ]
        );
    }

    #[test]
    fn test_find_available_prefix_boundaries() {
        let empty = HashSet::new();
        // /32：唯一地址既是网络地址也是广播地址，无可用 IP
        assert!(
            find_available_ips_in_cidr("192.168.7.5/32", None, &empty, None).is_empty(),
            "/32 应无可用地址"
        );
        // /31：仅有的两个地址分别为网络/广播地址，无可用 IP
        assert!(
            find_available_ips_in_cidr("192.168.7.0/31", None, &empty, None).is_empty(),
            "/31 应无可用地址"
        );
        // /30：中间两个主机地址可用
        let slash30 = find_available_ips_in_cidr("192.168.7.0/30", None, &empty, None);
        assert_eq!(slash30.len(), 2, "/30 应有 2 个可用地址");
        assert!(slash30.contains(&"192.168.7.1".to_string()));
        assert!(slash30.contains(&"192.168.7.2".to_string()));
        // /0 前缀下应受默认 1000 上限约束（不遍历全部地址）
        let slash0 = find_available_ips_in_cidr("0.0.0.0/0", None, &empty, None);
        assert_eq!(slash0.len(), 1000, "/0 受默认上限约束");
        assert_eq!(slash0.first().map(String::as_str), Some("0.0.0.1"));
    }

    #[test]
    fn test_find_available_full_subnet_returns_empty() {
        // 所有主机地址均被占用时应返回空列表
        let mut used: HashSet<String> = HashSet::new();
        for i in 1..=2 {
            used.insert(format!("192.168.9.{i}"));
        }
        let available = find_available_ips_in_cidr("192.168.9.0/30", None, &used, None);
        assert!(available.is_empty(), "全部占用后应无可用地址");
    }
}
