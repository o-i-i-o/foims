use std::collections::HashMap;

use actix_web::{HttpResponse, Result, web};
use async_snmp::{Client, oid};
use chrono::Utc;
use tracing::{debug, error, warn};
use uuid::Uuid;

use crate::db::DbPool;
use crate::models::{ApiResponse, ArpEntry, SwitchMac};

use super::snmp::{SnmpError, SnmpParamsLegacy, SwitchForSnmp, build_auth, format_snmp_error};

pub async fn get_arp_table_via_snmp(
    params: &SnmpParamsLegacy,
) -> Result<Vec<ArpEntry>, SnmpError> {
    let addr = format!("{}:{}", params.ip, params.port);
    let timeout = std::time::Duration::from_secs(10);

    let auth = build_auth(params).map_err(SnmpError::Message)?;

    let client = Client::builder(&addr, auth)
        .timeout(timeout)
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    let mut entries = Vec::new();
    let mut seen_ips = std::collections::HashSet::new();

    debug!("尝试获取IPv4 ARP表");
    let arp_oid = oid!(1, 3, 6, 1, 2, 1, 4, 22, 1, 2);
    
    let mut walk = client
        .walk(arp_oid)
        .map_err(|e| SnmpError::Message(format!("创建SNMP walk失败: {}", format_snmp_error(e))))?;

    while let Some(result) = walk.next().await {
        let vb = result.map_err(|e| SnmpError::Message(format!("SNMP walk失败: {}", format_snmp_error(e))))?;

        if vb.value.is_exception() {
            if matches!(vb.value, async_snmp::Value::EndOfMibView) {
                break;
            }
            continue;
        }

        let oid_parts = vb.oid.arcs();
        if oid_parts.len() >= 14 {
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
                    bytes[0], bytes[1], bytes[2],
                    bytes[3], bytes[4], bytes[5]
                );

                if mac_addr != "00:00:00:00:00:00" && !seen_ips.contains(&ip_addr) {
                    seen_ips.insert(ip_addr.clone());
                    entries.push(ArpEntry {
                        ip_address: ip_addr,
                        mac_address: mac_addr,
                        interface: None,
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
        let vb = result.map_err(|e| SnmpError::Message(format!("SNMP walk失败: {}", format_snmp_error(e))))?;

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
                        bytes[0], bytes[1], bytes[2],
                        bytes[3], bytes[4], bytes[5]
                    );

                    if mac_addr != "00:00:00:00:00:00" && !seen_ips.contains(&ip_addr) {
                        seen_ips.insert(ip_addr.clone());
                        entries.push(ArpEntry {
                            ip_address: ip_addr,
                            mac_address: mac_addr,
                            interface: None,
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
        FROM switches WHERE snmp_community IS NOT NULL OR snmp_username IS NOT NULL"#
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
           ORDER BY created_at LIMIT 1"#
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
        FROM switches WHERE id = $1"#
    )
    .bind(switch_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SnmpError::Message(e.to_string()))?
    .ok_or_else(|| SnmpError::Message("交换机不存在".to_string()))?;

    let ip_address: Option<String> = sqlx::query_scalar(
        r#"SELECT host(ip_address) FROM ip_managers 
           WHERE switch_id = $1 AND device_type = 'switch' 
           ORDER BY created_at LIMIT 1"#
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
        FROM switches WHERE id = $1"#
    )
    .bind(switch_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SnmpError::Message(e.to_string()))?
    .ok_or_else(|| SnmpError::Message("交换机不存在".to_string()))?;

    let ip_address: Option<String> = sqlx::query_scalar(
        r#"SELECT host(ip_address) FROM ip_managers 
           WHERE switch_id = $1 AND device_type = 'switch' 
           ORDER BY created_at LIMIT 1"#
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
           ORDER BY created_at LIMIT 1"#
    )
    .bind(switch_id)
    .fetch_optional(pool.get_conn())
    .await
    .ok()
    .flatten();

    let ip_address = match ip_address {
        Some(ref ip) if !ip.is_empty() => ip,
        _ => return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("交换机没有配置IP地址"))),
    };

    let snmp_params = switch.to_snmp_params(ip_address);

    let entries = match get_arp_table_via_snmp(&snmp_params).await {
        Ok(e) => e,
        Err(e) => return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error(format!("获取ARP表失败: {}", e)))),
    };

    let now = Utc::now();
    let mut saved_count = 0usize;
    let mut updated_count = 0usize;

    for entry in &entries {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM switch_macs WHERE switch_id = $1 AND ip_address = $2)"
        )
        .bind(switch_id)
        .bind(&entry.ip_address)
        .fetch_one(pool.get_conn())
        .await
        .unwrap_or(false);

        if exists {
            let result = sqlx::query(
                r#"UPDATE switch_macs SET 
                    mac_address = $1, 
                    interface = COALESCE($2, interface),
                    updated_at = $3
                WHERE switch_id = $4 AND ip_address = $5"#
            )
            .bind(&entry.mac_address)
            .bind(&entry.interface)
            .bind(now)
            .bind(switch_id)
            .bind(&entry.ip_address)
            .execute(pool.get_conn())
            .await;

            if result.is_ok() {
                updated_count += 1;
            }
        } else {
            let id = Uuid::new_v4();
            let result = sqlx::query(
                r#"INSERT INTO switch_macs (id, switch_id, ip_address, mac_address, interface, created_at, updated_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $6)"#
            )
            .bind(id)
            .bind(switch_id)
            .bind(&entry.ip_address)
            .bind(&entry.mac_address)
            .bind(&entry.interface)
            .bind(now)
            .execute(pool.get_conn())
            .await;

            if result.is_ok() {
                saved_count += 1;
            }
        }
    }

    let saved_macs: Vec<SwitchMac> = sqlx::query_as::<_, SwitchMac>(
        "SELECT * FROM switch_macs WHERE switch_id = $1 ORDER BY ip_address"
    )
    .bind(switch_id)
    .fetch_all(pool.get_conn())
    .await
    .unwrap_or_default();

    let message = if saved_count > 0 && updated_count > 0 {
        format!("新增 {} 条，更新 {} 条 MAC 记录", saved_count, updated_count)
    } else if saved_count > 0 {
        format!("新增 {} 条 MAC 记录", saved_count)
    } else if updated_count > 0 {
        format!("更新 {} 条 MAC 记录", updated_count)
    } else {
        "MAC 数据无变化".to_string()
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(saved_macs, &message)))
}

pub async fn get_switch_macs_from_db(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();

    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM switches WHERE id = $1)"
    )
    .bind(switch_id)
    .fetch_one(pool.get_conn())
    .await
    .unwrap_or(false);

    if !exists {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在")));
    }

    let macs: Vec<SwitchMac> = sqlx::query_as::<_, SwitchMac>(
        "SELECT * FROM switch_macs WHERE switch_id = $1 ORDER BY ip_address"
    )
    .bind(switch_id)
    .fetch_all(pool.get_conn())
    .await
    .unwrap_or_default();

    Ok(HttpResponse::Ok().json(ApiResponse::success(macs, "获取MAC表成功")))
}
