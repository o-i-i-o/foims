//! 设备 MAC 地址表管理。

use std::collections::HashMap;
use std::sync::Arc;

use async_snmp::{Client, oid};
use axum::extract::{Path, State};
use axum::response::Response;
use chrono::Utc;
use tracing::{debug, error};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{ArpEntry, DeviceMac};

use super::snmp::{
    SnmpError, SnmpParamsLegacy, build_auth, format_snmp_error, get_device_snmp_config,
};

fn parse_vlan_from_interface(iface: &str) -> Option<i32> {
    let iface_lower = iface.to_lowercase();
    let prefixes = ["vlan-interface", "vlan", "ve", "bvi", "irb", "svi"];
    for prefix in prefixes {
        if let Some(rest) = iface_lower.strip_prefix(prefix)
            && let Ok(vlan) = rest.parse::<i32>()
        {
            return Some(vlan);
        }
    }
    None
}

async fn walk_if_name_map(client: &Client) -> HashMap<u32, String> {
    let if_name_oid = oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, 1);
    let mut map = HashMap::new();

    let Ok(mut walk) = client.walk(if_name_oid) else {
        tracing::warn!("SNMP walk 接口名称映射失败");
        return map;
    };

    while let Some(result) = walk.next().await {
        let Ok(vb) = result else {
            tracing::debug!("SNMP walk 接口名称迭代错误，跳过");
            continue;
        };
        if vb.value.is_exception() {
            if matches!(vb.value, async_snmp::Value::EndOfMibView) {
                break;
            }
            continue;
        }
        let oid_parts = vb.oid.arcs();
        if let Some(&if_index) = oid_parts.last() {
            let name = vb
                .value
                .as_str()
                .map(std::string::ToString::to_string)
                .unwrap_or_default();
            if !name.is_empty() {
                map.insert(if_index, name);
            }
        }
    }

    debug!("获取接口名称映射: {} 条", map.len());
    map
}

async fn walk_vlan_map(client: &Client) -> HashMap<String, i32> {
    let dot1q_tp_fdb_port_oid = oid!(1, 3, 6, 1, 2, 1, 17, 7, 1, 2, 2, 1, 2);
    let mut map = HashMap::new();

    let Ok(mut walk) = client.walk(dot1q_tp_fdb_port_oid) else {
        tracing::warn!("SNMP walk VLAN映射失败");
        return map;
    };

    while let Some(result) = walk.next().await {
        let Ok(vb) = result else {
            tracing::debug!("SNMP walk VLAN映射迭代错误，跳过");
            continue;
        };
        if vb.value.is_exception() {
            if matches!(vb.value, async_snmp::Value::EndOfMibView) {
                break;
            }
            continue;
        }
        let oid_parts = vb.oid.arcs();
        if oid_parts.len() >= 13 {
            let vlan_id = oid_parts[12] as i32;
            let mac_parts = &oid_parts[13..];
            if mac_parts.len() == 6 {
                let mac = format!(
                    "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                    mac_parts[0],
                    mac_parts[1],
                    mac_parts[2],
                    mac_parts[3],
                    mac_parts[4],
                    mac_parts[5]
                );
                map.insert(mac, vlan_id);
            }
        }
    }

    debug!("获取VLAN映射: {} 条", map.len());
    map
}

pub async fn get_arp_table_via_snmp(params: &SnmpParamsLegacy) -> Result<Vec<ArpEntry>, SnmpError> {
    let addr = format!("{}:{}", params.ip, params.port);
    let timeout = std::time::Duration::from_secs(params.timeout_secs);

    let auth = build_auth(params).map_err(SnmpError::Message)?;

    let client = Client::builder(&addr, auth)
        .timeout(timeout)
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    let if_name_map = walk_if_name_map(&client).await;
    let vlan_map = walk_vlan_map(&client).await;

    let mut entries = Vec::new();
    let mut seen_ips = std::collections::HashSet::new();

    debug!("尝试获取IPv4 ARP表");
    let arp_oid = oid!(1, 3, 6, 1, 2, 1, 4, 22, 1, 2);

    let mut walk = client
        .walk(arp_oid)
        .map_err(|e| SnmpError::Message(format!("创建SNMP walk失败: {}", format_snmp_error(e))))?;

    while let Some(result) = walk.next().await {
        let vb = result
            .map_err(|e| SnmpError::Message(format!("SNMP walk失败: {}", format_snmp_error(e))))?;

        if vb.value.is_exception() {
            if matches!(vb.value, async_snmp::Value::EndOfMibView) {
                break;
            }
            continue;
        }

        let oid_parts = vb.oid.arcs();
        if oid_parts.len() >= 14 {
            let if_index = oid_parts[oid_parts.len() - 5];
            let ip_addr = format!(
                "{}.{}.{}.{}",
                oid_parts[oid_parts.len() - 4],
                oid_parts[oid_parts.len() - 3],
                oid_parts[oid_parts.len() - 2],
                oid_parts[oid_parts.len() - 1]
            );

            if let Some(bytes) = vb.value.as_bytes()
                && bytes.len() >= 6
            {
                let mac_addr = format!(
                    "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
                );

                if mac_addr != "00:00:00:00:00:00" && !seen_ips.contains(&ip_addr) {
                    seen_ips.insert(ip_addr.clone());
                    let interface = if_name_map.get(&if_index).cloned();
                    let vlan_id = vlan_map.get(&mac_addr).copied().or_else(|| {
                        interface
                            .as_ref()
                            .and_then(|i| parse_vlan_from_interface(i))
                    });
                    entries.push(ArpEntry {
                        ip_address: ip_addr,
                        mac_address: mac_addr,
                        interface,
                        vlan_id,
                    });
                }
            }
        }
    }

    debug!("尝试获取IPv6 MAC表");
    let ip_mib_oid = oid!(1, 3, 6, 1, 2, 1, 4, 35, 1, 4);

    let mut walk = client
        .walk(ip_mib_oid)
        .map_err(|e| SnmpError::Message(format!("创建SNMP walk失败: {}", format_snmp_error(e))))?;

    while let Some(result) = walk.next().await {
        let vb = result
            .map_err(|e| SnmpError::Message(format!("SNMP walk失败: {}", format_snmp_error(e))))?;

        if vb.value.is_exception() {
            if matches!(vb.value, async_snmp::Value::EndOfMibView) {
                debug!("IPv6 MAC表walk完成: 到达MIB视图末尾");
                break;
            }
            continue;
        }

        let oid_parts = vb.oid.arcs();
        if oid_parts.len() >= 14 {
            let base_len = 10;
            let if_index = oid_parts[base_len];
            let addr_len = oid_parts[base_len + 2] as usize;

            if addr_len == 16 && oid_parts.len() >= base_len + 3 + addr_len {
                let addr_start = base_len + 3;
                let addr_bytes: Vec<u8> = oid_parts[addr_start..addr_start + addr_len]
                    .iter()
                    .map(|&b| b as u8)
                    .collect();

                let parts: Vec<String> = (0..8)
                    .map(|i| format!("{:02x}{:02x}", addr_bytes[i * 2], addr_bytes[i * 2 + 1]))
                    .collect();
                let ipv6 = parts.join(":");
                let ip_addr = simplify_ipv6(&ipv6);

                if let Some(bytes) = vb.value.as_bytes()
                    && bytes.len() >= 6
                {
                    let mac_addr = format!(
                        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
                    );

                    if mac_addr != "00:00:00:00:00:00" && !seen_ips.contains(&ip_addr) {
                        seen_ips.insert(ip_addr.clone());
                        let interface = if_name_map.get(&if_index).cloned();
                        let vlan_id = vlan_map.get(&mac_addr).copied().or_else(|| {
                            interface
                                .as_ref()
                                .and_then(|i| parse_vlan_from_interface(i))
                        });
                        entries.push(ArpEntry {
                            ip_address: ip_addr,
                            mac_address: mac_addr,
                            interface,
                            vlan_id,
                        });
                    }
                }
            }
        }
    }

    debug!("获取MAC表完成: {} 条记录 (IPv4 + IPv6)", entries.len());
    Ok(entries)
}

fn simplify_ipv6(ip: &str) -> String {
    ip.parse::<std::net::Ipv6Addr>()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|_| ip.to_string())
}

pub async fn get_device_mac_table(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let (switch, ip_address) = get_device_snmp_config(&conn, &device_id).await?;

    let ip_address =
        ip_address.ok_or_else(|| AppError::Validation("设备没有配置IP地址".to_string()))?;

    let snmp_params = switch.to_snmp_params_async(&ip_address).await?;

    let entries = get_arp_table_via_snmp(&snmp_params)
        .await
        .map_err(|e| AppError::Snmp(format!("获取ARP表失败: {e}")))?;

    let now = Utc::now();
    let mut upserted_count = 0usize;
    let mut failed_count = 0usize;
    let mut seen_ips: Vec<String> = Vec::new();

    let mut tx = conn.begin().await?;

    for entry in &entries {
        let id = Uuid::new_v4();
        seen_ips.push(entry.ip_address.clone());
        let result = sqlx::query(
            r"INSERT INTO device_macs (id, device_id, ip_address, mac_address, interface, vlan_id, created_at, updated_at)
               VALUES ($1, $2, CAST($3 AS INET), $4, $5, $6, $7, $7)
               ON CONFLICT (device_id, ip_address)
               DO UPDATE SET mac_address = EXCLUDED.mac_address,
                             interface = COALESCE(EXCLUDED.interface, device_macs.interface),
                             vlan_id = COALESCE(EXCLUDED.vlan_id, device_macs.vlan_id),
                             updated_at = EXCLUDED.updated_at",
        )
        .bind(id)
        .bind(device_id)
        .bind(&entry.ip_address)
        .bind(&entry.mac_address)
        .bind(&entry.interface)
        .bind(entry.vlan_id)
        .bind(now)
        .execute(&mut *tx)
        .await;

        match result {
            Ok(r) if r.rows_affected() > 0 => {
                upserted_count += 1;
            }
            Ok(_) => {}
            Err(e) => {
                failed_count += 1;
                tracing::error!(
                    "MAC记录写入失败 (ip={}, mac={}): {}",
                    entry.ip_address,
                    entry.mac_address,
                    e
                );
            }
        }
    }

    // 删除本次同步中未出现的陈旧记录
    if !seen_ips.is_empty() {
        sqlx::query(
            "DELETE FROM device_macs WHERE device_id = $1 AND host(ip_address) NOT IN (SELECT unnest($2::text[]))",
        )
        .bind(device_id)
        .bind(&seen_ips)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let saved_macs: Vec<DeviceMac> = sqlx::query_as::<_, DeviceMac>(
        r"SELECT id, device_id, host(ip_address) as ip_address, mac_address, interface, vlan_id, created_at, updated_at
           FROM device_macs WHERE device_id = $1 ORDER BY ip_address",
    )
    .bind(device_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let message = if failed_count > 0 {
        format!("同步 {upserted_count} 条 MAC 记录，{failed_count} 条失败")
    } else {
        format!("同步 {upserted_count} 条 MAC 记录")
    };

    Ok(crate::error::ok_json(saved_macs, &message))
}

pub async fn get_device_macs_from_db(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
        .bind(device_id)
        .fetch_one(&conn)
        .await?;

    if !exists {
        return Err(AppError::NotFound("设备不存在".to_string()));
    }

    let macs: Vec<DeviceMac> = sqlx::query_as::<_, DeviceMac>(
        r"SELECT id, device_id, host(ip_address) as ip_address, mac_address, interface, vlan_id, created_at, updated_at
           FROM device_macs WHERE device_id = $1 ORDER BY ip_address",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await
    .map_err(|e| {
        error!("查询MAC表失败: {}", e);
        AppError::Database("查询MAC表失败".to_string())
    })?;

    Ok(crate::error::ok_json(macs, "获取MAC表成功"))
}
