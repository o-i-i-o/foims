use std::collections::HashMap;

use actix_web::{HttpResponse, Result, web};
use async_snmp::{Client, oid};
use chrono::Utc;
use tracing::{debug, error, warn};
use uuid::Uuid;

use crate::db::DbPool;
use crate::models::{ApiResponse, ArpEntry, SwitchMac};

use super::snmp::{SnmpError, SnmpParamsLegacy, SwitchForSnmp, build_auth, format_snmp_error};

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
        return map;
    };

    while let Some(result) = walk.next().await {
        let Ok(vb) = result else { continue };
        if vb.value.is_exception() {
            if matches!(vb.value, async_snmp::Value::EndOfMibView) {
                break;
            }
            continue;
        }
        let oid_parts = vb.oid.arcs();
        if let Some(&if_index) = oid_parts.last() {
            let name = vb.value.as_str().map(|s| s.to_string()).unwrap_or_default();
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
        return map;
    };

    while let Some(result) = walk.next().await {
        let Ok(vb) = result else { continue };
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
    let timeout = std::time::Duration::from_secs(10);

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

fn simplify_ipv6(ipv6: &str) -> String {
    let parts: Vec<&str> = ipv6.split(':').collect();
    let mut result = String::new();
    let mut zero_start = None;
    let mut zero_len = 0;
    let mut current_zero_len = 0;
    let mut current_zero_start = None;

    for (i, part) in parts.iter().enumerate() {
        if *part == "0000" || *part == "0" {
            if current_zero_start.is_none() {
                current_zero_start = Some(i);
            }
            current_zero_len += 1;
        } else {
            if current_zero_len > zero_len {
                zero_start = current_zero_start;
                zero_len = current_zero_len;
            }
            current_zero_start = None;
            current_zero_len = 0;
        }
    }
    if current_zero_len > zero_len {
        zero_start = current_zero_start;
        zero_len = current_zero_len;
    }

    for (i, part) in parts.iter().enumerate() {
        if let Some(start) = zero_start {
            if i == start && zero_len > 1 {
                result.push_str("::");
                continue;
            }
            if i > start && i < start + zero_len {
                continue;
            }
        }
        if !result.is_empty() && !result.ends_with(':') {
            result.push(':');
        }
        let val = u16::from_str_radix(part, 16).unwrap_or(0);
        result.push_str(&format!("{:x}", val));
    }

    result
}

pub async fn batch_get_mac_via_snmp(
    pool: &sqlx::PgPool,
    ips: &[String],
) -> HashMap<String, Option<String>> {
    let mut results: HashMap<String, Option<String>> = HashMap::new();
    results.reserve(ips.len());

    let switches = match sqlx::query_as::<_, SwitchForSnmp>(
        r#"SELECT
            id, name, snmp_version, snmp_community,
            snmp_username, snmp_auth_protocol,
            snmp_auth_password, snmp_priv_protocol,
            snmp_priv_password, snmp_port
        FROM switches WHERE snmp_community IS NOT NULL OR snmp_username IS NOT NULL"#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(s) => s,
        Err(e) => {
            error!("查询交换机列表失败: {}", e);
            ips.iter().for_each(|ip| {
                results.insert(ip.clone(), None);
            });
            return results;
        }
    };

    if switches.is_empty() {
        warn!("没有配置SNMP的交换机");
        ips.iter().for_each(|ip| {
            results.insert(ip.clone(), None);
        });
        return results;
    }

    let mut all_arp_entries: HashMap<String, String> = HashMap::new();

    for switch in &switches {
        if let Err(e) = fetch_switch_arp(pool, switch, &mut all_arp_entries).await {
            warn!("从交换机 {} 获取ARP表失败: {}", switch.name, e);
        }
    }

    for ip in ips {
        let mac = all_arp_entries.get(ip).cloned();
        results.insert(ip.clone(), mac);
    }

    results
}

async fn fetch_switch_arp(
    pool: &sqlx::PgPool,
    switch: &SwitchForSnmp,
    arp_entries: &mut HashMap<String, String>,
) -> Result<(), SnmpError> {
    let ip_address: Option<String> = sqlx::query_scalar(
        r#"SELECT host(ip_address) FROM ip_managers
           WHERE switch_id = $1 AND device_type = 'switch'
           ORDER BY created_at LIMIT 1"#,
    )
    .bind(switch.id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SnmpError::Message(e.to_string()))?
    .flatten();

    let ip_address = match ip_address {
        Some(ref ip) if !ip.is_empty() => ip,
        _ => return Ok(()),
    };

    let params = switch.to_snmp_params(ip_address);

    match get_arp_table_via_snmp(&params).await {
        Ok(entries) => {
            for entry in entries {
                arp_entries.insert(entry.ip_address, entry.mac_address);
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

pub async fn get_mac_from_switch(
    pool: &sqlx::PgPool,
    switch_id: &uuid::Uuid,
    ips: &[String],
) -> Result<HashMap<String, Option<String>>, SnmpError> {
    let switch = sqlx::query_as::<_, SwitchForSnmp>(
        r#"SELECT
            id, name, snmp_version, snmp_community,
            snmp_username, snmp_auth_protocol,
            snmp_auth_password, snmp_priv_protocol,
            snmp_priv_password, snmp_port
        FROM switches WHERE id = $1"#,
    )
    .bind(switch_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SnmpError::Message(e.to_string()))?
    .ok_or_else(|| SnmpError::Message("交换机不存在".to_string()))?;

    let ip_address: Option<String> = sqlx::query_scalar(
        r#"SELECT host(ip_address) FROM ip_managers
           WHERE switch_id = $1 AND device_type = 'switch'
           ORDER BY created_at LIMIT 1"#,
    )
    .bind(switch_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SnmpError::Message(e.to_string()))?
    .flatten();

    let ip_address = ip_address
        .filter(|ip| !ip.is_empty())
        .ok_or_else(|| SnmpError::Message("交换机没有配置IP地址".to_string()))?;

    if switch.snmp_community.is_none() && switch.snmp_username.is_none() {
        return Err(SnmpError::Message("该交换机未配置SNMP".to_string()));
    }

    let params = switch.to_snmp_params(&ip_address);

    let entries = get_arp_table_via_snmp(&params).await?;

    let arp_map: HashMap<String, String> = entries
        .into_iter()
        .map(|e| (e.ip_address, e.mac_address))
        .collect();

    let mut results = HashMap::with_capacity(ips.len());
    for ip in ips {
        let mac = arp_map.get(ip).cloned();
        results.insert(ip.clone(), mac);
    }

    Ok(results)
}

pub async fn get_all_arp_entries(
    pool: &sqlx::PgPool,
    switch_id: &uuid::Uuid,
) -> Result<Vec<ArpEntry>, SnmpError> {
    let switch = sqlx::query_as::<_, SwitchForSnmp>(
        r#"SELECT
            id, name, snmp_version, snmp_community,
            snmp_username, snmp_auth_protocol,
            snmp_auth_password, snmp_priv_protocol,
            snmp_priv_password, snmp_port
        FROM switches WHERE id = $1"#,
    )
    .bind(switch_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SnmpError::Message(e.to_string()))?
    .ok_or_else(|| SnmpError::Message("交换机不存在".to_string()))?;

    let ip_address: Option<String> = sqlx::query_scalar(
        r#"SELECT host(ip_address) FROM ip_managers
           WHERE switch_id = $1 AND device_type = 'switch'
           ORDER BY created_at LIMIT 1"#,
    )
    .bind(switch_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SnmpError::Message(e.to_string()))?
    .flatten();

    let ip_address = ip_address
        .filter(|ip| !ip.is_empty())
        .ok_or_else(|| SnmpError::Message("交换机没有配置IP地址".to_string()))?;

    if switch.snmp_community.is_none() && switch.snmp_username.is_none() {
        return Err(SnmpError::Message("该交换机未配置SNMP".to_string()));
    }

    let params = switch.to_snmp_params(&ip_address);

    let entries = get_arp_table_via_snmp(&params).await?;
    Ok(entries)
}

pub async fn get_switch_mac_table(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();

    let switch = sqlx::query_as::<_, SwitchForSnmp>(
        r#"SELECT
            id, name, snmp_version, snmp_community,
            snmp_username, snmp_auth_protocol,
            snmp_auth_password, snmp_priv_protocol,
            snmp_priv_password, snmp_port
        FROM switches WHERE id = $1"#,
    )
    .bind(switch_id)
    .fetch_optional(pool.get_conn())
    .await;

    let switch = match switch {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在")));
        }
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("查询交换机失败: {}", e))));
        }
    };

    let ip_address: Option<String> = sqlx::query_scalar(
        r#"SELECT host(ip_address) FROM ip_managers
           WHERE switch_id = $1 AND device_type = 'switch'
           ORDER BY created_at LIMIT 1"#,
    )
    .bind(switch_id)
    .fetch_optional(pool.get_conn())
    .await
    .ok()
    .flatten();

    let ip_address = match ip_address {
        Some(ref ip) if !ip.is_empty() => ip,
        _ => {
            return Ok(
                HttpResponse::BadRequest().json(ApiResponse::<()>::error("交换机没有配置IP地址"))
            );
        }
    };

    let snmp_params = switch.to_snmp_params(ip_address);

    let entries = match get_arp_table_via_snmp(&snmp_params).await {
        Ok(e) => e,
        Err(e) => {
            return Ok(HttpResponse::BadRequest()
                .json(ApiResponse::<()>::error(format!("获取ARP表失败: {}", e))));
        }
    };

    let now = Utc::now();
    let mut upserted_count = 0usize;

    for entry in &entries {
        let id = Uuid::new_v4();
        let result = sqlx::query(
            r#"INSERT INTO switch_macs (id, switch_id, ip_address, mac_address, interface, vlan_id, created_at, updated_at)
               VALUES ($1, $2, CAST($3 AS INET), $4, $5, $6, $7, $7)
               ON CONFLICT (switch_id, ip_address)
               DO UPDATE SET mac_address = EXCLUDED.mac_address,
                             interface = COALESCE(EXCLUDED.interface, switch_macs.interface),
                             vlan_id = COALESCE(EXCLUDED.vlan_id, switch_macs.vlan_id),
                             updated_at = EXCLUDED.updated_at"#,
        )
        .bind(id)
        .bind(switch_id)
        .bind(&entry.ip_address)
        .bind(&entry.mac_address)
        .bind(&entry.interface)
        .bind(entry.vlan_id)
        .bind(now)
        .execute(pool.get_conn())
        .await;

        if let Ok(r) = result
            && r.rows_affected() > 0
        {
            upserted_count += 1;
        }
    }

    let saved_macs: Vec<SwitchMac> = sqlx::query_as::<_, SwitchMac>(
        r#"SELECT id, switch_id, host(ip_address) as ip_address, mac_address, interface, vlan_id, created_at, updated_at
           FROM switch_macs WHERE switch_id = $1 ORDER BY ip_address"#,
    )
    .bind(switch_id)
    .fetch_all(pool.get_conn())
    .await
    .unwrap_or_default();

    let message = format!("同步 {} 条 MAC 记录", upserted_count);

    Ok(HttpResponse::Ok().json(ApiResponse::success(saved_macs, &message)))
}

pub async fn get_switch_macs_from_db(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM switches WHERE id = $1)")
        .bind(switch_id)
        .fetch_one(pool.get_conn())
        .await
        .unwrap_or(false);

    if !exists {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在")));
    }

    let macs_result = sqlx::query_as::<_, SwitchMac>(
        r#"SELECT id, switch_id, host(ip_address) as ip_address, mac_address, interface, vlan_id, created_at, updated_at
           FROM switch_macs WHERE switch_id = $1 ORDER BY ip_address"#,
    )
    .bind(switch_id)
    .fetch_all(pool.get_conn())
    .await;

    let macs = match macs_result {
        Ok(m) => m,
        Err(e) => {
            error!("查询MAC表失败: {}", e);
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<Vec<SwitchMac>>::error("查询MAC表失败")));
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(macs, "获取MAC表成功")))
}
