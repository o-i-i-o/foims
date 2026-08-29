//! 设备 LLDP 邻居发现（SNMP 采集与入库）。

use std::collections::HashMap;
use std::sync::Arc;

use async_snmp::{Client, VarBind, oid};
use axum::extract::{Path, State};
use axum::response::Response;
use tracing::debug;
use uuid::Uuid;

use ipma_common::DbProvider;
use ipma_common::log_error;
use ipma_common::{AppError, msg};
use ipma_models::{DeviceLldp, LldpNeighbor};

use super::snmp::{
    DeviceForSnmp, SnmpError, SnmpParamsLegacy, build_auth, format_snmp_error, snmp_target,
    truncate_to_column_width,
};

/// device_lldps.local_port 列宽（VARCHAR(50)，与建表契约一致）
const LOCAL_PORT_MAX_CHARS: usize = 50;

/// 获取设备的 LLDP 邻居（先校验设备/IP/SNMP 配置，再走 SNMP 采集）。
pub async fn get_lldp_neighbors(
    pool: &sqlx::PgPool,
    device_id: &Uuid,
) -> Result<Vec<LldpNeighbor>, AppError> {
    let switch = sqlx::query_as::<_, DeviceForSnmp>(
        r"SELECT
            id, name, snmp_version, snmp_community,
            snmp_username, snmp_auth_protocol,
            snmp_auth_password, snmp_priv_protocol,
            snmp_priv_password, snmp_port
        FROM devices WHERE id = $1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.device.not_found")))?;

    let ip_address: Option<String> = sqlx::query_scalar(
        r"SELECT host(i.ip_address) FROM ips i
           JOIN device_interfaces di ON i.device_interface_id = di.id
           WHERE di.device_id = $1
           ORDER BY i.created_at LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let ip_address = ip_address
        .filter(|ip| !ip.is_empty())
        .ok_or_else(|| AppError::Validation(msg("server.device.no_ip_configured")))?;

    if switch.snmp_community.is_none() && switch.snmp_username.is_none() {
        return Err(AppError::Validation(msg(
            "server.device.snmp.not_configured",
        )));
    }

    let params = switch.to_snmp_params_async(&ip_address).await?;
    get_lldp_neighbors_via_snmp(&params)
        .await
        .map_err(|e| AppError::Snmp(msg("server.device.snmp.lldp_fetch_failed").with("error", e)))
}

async fn snmp_walk<F>(
    client: &Client,
    oid: async_snmp::Oid,
    error_msg: &str,
    mut process: F,
) -> Result<(), SnmpError>
where
    F: FnMut(&VarBind) -> Result<(), SnmpError>,
{
    let mut walk = client
        .walk(oid)
        .map_err(|e| SnmpError::Message(format!("{}: {}", error_msg, format_snmp_error(e))))?;

    while let Some(result) = walk.next().await {
        let vb = result
            .map_err(|e| SnmpError::Message(format!("{}: {}", error_msg, format_snmp_error(e))))?;

        if vb.value.is_exception() {
            if matches!(vb.value, async_snmp::Value::EndOfMibView) {
                break;
            }
            continue;
        }

        process(&vb)?;
    }

    Ok(())
}

pub async fn get_lldp_neighbors_via_snmp(
    params: &SnmpParamsLegacy,
) -> Result<Vec<ipma_models::LldpNeighbor>, SnmpError> {
    // 复用 snmp_target：IPv6 地址须包裹方括号，直接 ip:port 拼接必然解析失败
    let addr = snmp_target(&params.ip, params.port);
    let timeout = std::time::Duration::from_secs(params.timeout_secs);

    let auth = build_auth(params).map_err(SnmpError::Message)?;

    let client = Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    debug!("开始获取LLDP邻居信息");

    let if_name_oid = oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 2);
    let mut if_name_map: HashMap<String, String> = HashMap::new();

    snmp_walk(&client, if_name_oid, "接口名称表walk失败", |vb| {
        let oid_parts = vb.oid.arcs();
        if oid_parts.len() >= 11 {
            let if_index = oid_parts[10].to_string();
            if let Some(name) = vb.value.as_str() {
                if !name.is_empty() {
                    if_name_map.insert(if_index, name.to_string());
                }
            } else if let Some(name) = vb.value.as_bytes() {
                let name_str = String::from_utf8_lossy(name).to_string();
                if !name_str.is_empty() {
                    if_name_map.insert(if_index, name_str);
                }
            }
        }
        Ok(())
    })
    .await?;

    let lldp_loc_port_subtype = oid!(1, 0, 8802, 1, 1, 2, 1, 3, 7, 1, 2);
    let mut loc_port_subtype_map: HashMap<String, u8> = HashMap::new();

    snmp_walk(
        &client,
        lldp_loc_port_subtype,
        "LLDP端口子类型表walk失败",
        |vb| {
            let oid_parts = vb.oid.arcs();
            if oid_parts.len() >= 12 {
                let port_num = oid_parts[11].to_string();
                let subtype = if let Some(s) = vb.value.as_u32() {
                    s as u8
                } else if let Some(bytes) = vb.value.as_bytes() {
                    if bytes.is_empty() { 0 } else { bytes[0] }
                } else {
                    0
                };
                loc_port_subtype_map.insert(port_num, subtype);
            }
            Ok(())
        },
    )
    .await?;

    let lldp_loc_port_id = oid!(1, 0, 8802, 1, 1, 2, 1, 3, 7, 1, 3);
    let mut port_id_map: HashMap<String, String> = HashMap::new();

    snmp_walk(&client, lldp_loc_port_id, "LLDP端口表walk失败", |vb| {
        let oid_parts = vb.oid.arcs();
        if oid_parts.len() >= 12 {
            let port_num = oid_parts[11].to_string();
            if let Some(port_id) = vb.value.as_bytes() {
                let subtype = loc_port_subtype_map.get(&port_num).copied().unwrap_or(0);
                let port_id_str = format_lldp_id(port_id, subtype);
                port_id_map.insert(port_num, port_id_str);
            }
        }
        Ok(())
    })
    .await?;

    let lldp_loc_port_desc = oid!(1, 0, 8802, 1, 1, 2, 1, 3, 7, 1, 4);
    let mut port_desc_map: HashMap<String, String> = HashMap::new();

    snmp_walk(
        &client,
        lldp_loc_port_desc,
        "LLDP端口描述表walk失败",
        |vb| {
            let oid_parts = vb.oid.arcs();
            if oid_parts.len() >= 12 {
                let port_num = oid_parts[11].to_string();
                if let Some(port_desc) = vb.value.as_str() {
                    let desc = port_desc.trim();
                    if !desc.is_empty() && desc != "NULL" {
                        port_desc_map.insert(port_num, desc.to_string());
                    }
                } else if let Some(port_desc) = vb.value.as_bytes() {
                    let desc = String::from_utf8_lossy(port_desc).trim().to_string();
                    if !desc.is_empty() && desc != "NULL" {
                        port_desc_map.insert(port_num, desc);
                    }
                }
            }
            Ok(())
        },
    )
    .await?;

    let lldp_rem_table = oid!(1, 0, 8802, 1, 1, 2, 1, 4, 1, 1);
    let mut neighbor_data: HashMap<(String, String), ipma_models::LldpNeighbor> = HashMap::new();
    let mut chassis_subtype_map: HashMap<(String, String), u8> = HashMap::new();
    let mut port_subtype_map: HashMap<(String, String), u8> = HashMap::new();

    snmp_walk(&client, lldp_rem_table, "LLDP邻居表walk失败", |vb| {
        let oid_parts = vb.oid.arcs();
        if oid_parts.len() >= 14 {
            let field_type = oid_parts[10];
            let local_port_num = oid_parts[12].to_string();
            let rem_index = oid_parts[13].to_string();
            let key = (local_port_num.clone(), rem_index);

            let entry = neighbor_data.entry(key.clone()).or_insert_with(|| {
                let local_port = get_local_port_name(
                    &local_port_num,
                    &if_name_map,
                    &port_id_map,
                    &port_desc_map,
                );
                ipma_models::LldpNeighbor {
                    local_port,
                    neighbor_chassis_id: None,
                    neighbor_port_id: None,
                    neighbor_port_desc: None,
                    neighbor_sys_name: None,
                    neighbor_sys_desc: None,
                }
            });

            match field_type {
                4 => {
                    if let Some(subtype) = vb.value.as_u32() {
                        chassis_subtype_map.insert(key, subtype as u8);
                    } else if let Some(bytes) = vb.value.as_bytes()
                        && !bytes.is_empty()
                    {
                        chassis_subtype_map.insert(key, bytes[0]);
                    }
                }
                5 => {
                    if let Some(chassis_id) = vb.value.as_bytes() {
                        let subtype = chassis_subtype_map.get(&key).copied().unwrap_or(0);
                        let chassis_id_str = format_lldp_id(chassis_id, subtype);
                        entry.neighbor_chassis_id = Some(chassis_id_str);
                    }
                }
                6 => {
                    if let Some(subtype) = vb.value.as_u32() {
                        port_subtype_map.insert(key, subtype as u8);
                    } else if let Some(bytes) = vb.value.as_bytes()
                        && !bytes.is_empty()
                    {
                        port_subtype_map.insert(key, bytes[0]);
                    }
                }
                7 => {
                    if let Some(port_id) = vb.value.as_bytes() {
                        let subtype = port_subtype_map.get(&key).copied().unwrap_or(1);
                        let port_id_str = format_lldp_id(port_id, subtype);
                        debug!(
                            "邻居端口 {:?} subtype={} raw={:?} formatted={}",
                            key, subtype, port_id, port_id_str
                        );
                        entry.neighbor_port_id = Some(port_id_str);
                    }
                }
                8 => {
                    if let Some(port_desc) = vb.value.as_str() {
                        let desc = port_desc.trim();
                        if !desc.is_empty() && desc != "NULL" {
                            entry.neighbor_port_desc = Some(desc.to_string());
                        }
                    } else if let Some(port_desc) = vb.value.as_bytes() {
                        let desc = String::from_utf8_lossy(port_desc).trim().to_string();
                        if !desc.is_empty() && desc != "NULL" {
                            entry.neighbor_port_desc = Some(desc);
                        }
                    }
                }
                9 => {
                    if let Some(sys_name) = vb.value.as_str() {
                        entry.neighbor_sys_name = Some(sys_name.to_string());
                    } else if let Some(sys_name) = vb.value.as_bytes() {
                        entry.neighbor_sys_name =
                            Some(String::from_utf8_lossy(sys_name).to_string());
                    }
                }
                10 => {
                    if let Some(sys_desc) = vb.value.as_str() {
                        entry.neighbor_sys_desc = Some(sys_desc.to_string());
                    } else if let Some(sys_desc) = vb.value.as_bytes() {
                        entry.neighbor_sys_desc =
                            Some(String::from_utf8_lossy(sys_desc).to_string());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    })
    .await?;

    let neighbors: Vec<ipma_models::LldpNeighbor> = neighbor_data
        .into_values()
        .filter(|n| n.neighbor_sys_name.is_some() || n.neighbor_chassis_id.is_some())
        .collect();

    debug!("获取LLDP邻居完成: {} 条记录", neighbors.len());
    Ok(neighbors)
}

fn get_local_port_name(
    local_port_num: &str,
    if_name_map: &HashMap<String, String>,
    port_id_map: &HashMap<String, String>,
    port_desc_map: &HashMap<String, String>,
) -> String {
    if let Some(name) = if_name_map.get(local_port_num) {
        return name.clone();
    }

    let port_id = port_id_map
        .get(local_port_num)
        .cloned()
        .unwrap_or_else(|| local_port_num.to_string());
    let is_mac = is_mac_address(&port_id);

    if is_mac {
        port_desc_map
            .get(local_port_num)
            .cloned()
            .unwrap_or(port_id)
    } else {
        port_id
    }
}

fn is_mac_address(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        return false;
    }
    parts
        .iter()
        .all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()))
}

fn format_lldp_id(bytes: &[u8], subtype: u8) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    match subtype {
        1 | 2 | 5 | 6 | 7 => String::from_utf8_lossy(bytes).to_string(),
        3 | 4 => format_mac_address(bytes),
        _ => {
            if is_printable_string(bytes) {
                String::from_utf8_lossy(bytes).to_string()
            } else {
                format_mac_address(bytes)
            }
        }
    }
}

fn is_printable_string(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|&b| (0x20..=0x7E).contains(&b) || b == b'\t')
}

fn format_mac_address(bytes: &[u8]) -> String {
    if bytes.len() == 6 {
        format!(
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
        )
    } else {
        bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":")
    }
}

pub async fn get_device_lldp_neighbors<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
        .bind(device_id)
        .fetch_one(&conn)
        .await?;

    if !exists {
        return Err(AppError::NotFound(msg("server.device.not_found")));
    }

    let lldps: Vec<DeviceLldp> = sqlx::query_as::<_, DeviceLldp>(
        "SELECT * FROM device_lldps WHERE device_id = $1 ORDER BY local_port",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    Ok(ipma_common::ok_json(lldps, "server.device.lldp.fetched"))
}

pub async fn sync_lldp_from_snmp<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    // 邻居采集失败（含设备/IP/SNMP 配置问题）已带语义化错误，直接透传
    let neighbors = get_lldp_neighbors(&conn, &device_id).await?;

    let now = chrono::Utc::now();
    let mut synced_count = 0usize;
    let mut failed_count = 0usize;
    // 截断后的本地端口名：入库与过期清理必须使用同一口径
    let mut local_ports: Vec<String> = Vec::with_capacity(neighbors.len());

    // 在同一事务内 upsert 全部邻居并清理已消失的邻居，避免逐条 autocommit 造成部分写入与数据漂移
    let mut tx = conn.begin().await?;

    for neighbor in &neighbors {
        // local_port 列宽 VARCHAR(50)：入库前按字符截断（超宽截断优于必败写入）
        let local_port = truncate_to_column_width(&neighbor.local_port, LOCAL_PORT_MAX_CHARS);
        local_ports.push(local_port.clone());
        // 逐行 SAVEPOINT：单行 upsert 失败仅回滚该行（ROLLBACK TO + RELEASE），
        // 事务保持可用后继续处理后续行；单行失败不再令 PostgreSQL 事务
        // 进入 aborted 状态导致整单 500
        let mut sp = sqlx::Acquire::begin(&mut *tx).await?;
        let result = sqlx::query(
            r"INSERT INTO device_lldps (id, device_id, local_port, neighbor_chassis_id, neighbor_port_id, neighbor_port_desc, neighbor_sys_name, neighbor_sys_desc, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
               ON CONFLICT (device_id, local_port) DO UPDATE SET
                    neighbor_chassis_id = EXCLUDED.neighbor_chassis_id,
                    neighbor_port_id = EXCLUDED.neighbor_port_id,
                    neighbor_port_desc = EXCLUDED.neighbor_port_desc,
                    neighbor_sys_name = EXCLUDED.neighbor_sys_name,
                    neighbor_sys_desc = EXCLUDED.neighbor_sys_desc,
                    updated_at = EXCLUDED.updated_at",
        )
        .bind(Uuid::new_v4())
        .bind(device_id)
        .bind(&local_port)
        .bind(&neighbor.neighbor_chassis_id)
        .bind(&neighbor.neighbor_port_id)
        .bind(&neighbor.neighbor_port_desc)
        .bind(&neighbor.neighbor_sys_name)
        .bind(&neighbor.neighbor_sys_desc)
        .bind(now)
        .execute(&mut *sp)
        .await;

        match result {
            Ok(_) => {
                sp.commit().await?;
                synced_count += 1;
            }
            Err(e) => {
                // 回滚到 SAVEPOINT，恢复本事务可用状态后计入失败并继续
                sp.rollback().await?;
                failed_count += 1;
                log_error!(
                    "log.device.lldp.record_write_failed",
                    port = neighbor.local_port,
                    error = e
                );
            }
        }
    }

    // 清理本次未发现的旧邻居记录（防止数据漂移）；
    // 空采集结果（seen 集为空）时跳过 DELETE 保留存量记录——空结果
    // 视为采集异常而非"邻居全部消失"，与 mac.rs 的 seen_ips 空分支同口径；
    // 清理失败经 ? 向上传播并回滚事务，避免 removed 计数失真与半提交状态
    let stale_removed: i64 = if local_ports.is_empty() {
        0
    } else {
        sqlx::query_scalar(
            "WITH deleted AS (
                DELETE FROM device_lldps WHERE device_id = $1 AND NOT (local_port = ANY($2)) RETURNING 1
             ) SELECT COUNT(*) FROM deleted",
        )
        .bind(device_id)
        .bind(&local_ports)
        .fetch_one(&mut *tx)
        .await?
    };

    tx.commit().await?;

    let saved_lldps: Vec<DeviceLldp> = sqlx::query_as::<_, DeviceLldp>(
        "SELECT * FROM device_lldps WHERE device_id = $1 ORDER BY local_port",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    // 按同步结果构造消息：有失败（部分成功）/ 有变化 / 无变化
    let message = if failed_count > 0 {
        msg("server.device.lldp.sync_partial")
            .with("synced", synced_count)
            .with("failed", failed_count)
            .with("removed", stale_removed)
    } else if synced_count > 0 {
        msg("server.device.lldp.sync_changed")
            .with("synced", synced_count)
            .with("removed", stale_removed)
    } else {
        msg("server.device.lldp.sync_unchanged").with("removed", stale_removed)
    };

    Ok(ipma_common::ok_json(saved_lldps, message))
}
