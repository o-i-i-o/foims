use std::collections::HashMap;

use actix_web::{HttpResponse, Result, web};
use async_snmp::{Client, oid};
use chrono::Utc;
use tracing::{debug, error, warn};
use uuid::Uuid;

use crate::db::DbPool;
use crate::models::{ApiResponse, ArpEntry, Switch};

use super::snmp::{SnmpError, SnmpParamsLegacy, build_auth, format_snmp_error};

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
    use crate::models::Switch;
    
    let mut results: HashMap<String, Option<String>> = HashMap::new();
    results.reserve(ips.len());

    let switches = match sqlx::query_as::<_, Switch>(
        r#"SELECT 
            id, name, network_region_id, network_id,
            model, vendor, 
            location, snmp_version, 
            snmp_community, 
            snmp_username, snmp_auth_protocol, 
            snmp_auth_password, 
            snmp_priv_protocol, 
            snmp_priv_password, 
            snmp_port, 
            parent_switch_id, parent_port_id, description, created_at, updated_at 
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
    switch: &crate::models::Switch,
    arp_entries: &mut HashMap<String, String>,
) -> Result<(), SnmpError> {
    use crate::crypto::decrypt_password;
    
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

    let decrypted_community = switch.snmp_community.as_ref().map(|v| decrypt_password(v));
    let decrypted_auth_pass = switch.snmp_auth_password.as_ref().map(|v| decrypt_password(v));
    let decrypted_priv_pass = switch.snmp_priv_password.as_ref().map(|v| decrypt_password(v));

    let params = SnmpParamsLegacy {
        ip: ip_address.to_string(),
        port: switch.snmp_port,
        version: switch.snmp_version.clone(),
        community: decrypted_community,
        username: switch.snmp_username.clone(),
        auth_proto: switch.snmp_auth_protocol.clone(),
        auth_pass: decrypted_auth_pass,
        priv_proto: switch.snmp_priv_protocol.clone(),
        priv_pass: decrypted_priv_pass,
    };

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
    use crate::models::Switch;
    use crate::crypto::decrypt_password;

    let switch = sqlx::query_as::<_, Switch>(
        r#"SELECT 
            id, name, network_region_id, network_id,
            model, vendor, 
            location, snmp_version, 
            snmp_community, 
            snmp_username, snmp_auth_protocol, 
            snmp_auth_password, 
            snmp_priv_protocol, 
            snmp_priv_password, 
            snmp_port, 
            parent_switch_id, parent_port_id, description, created_at, updated_at 
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

    let decrypted_community = switch.snmp_community.as_ref().map(|v| decrypt_password(v));
    let decrypted_auth_pass = switch.snmp_auth_password.as_ref().map(|v| decrypt_password(v));
    let decrypted_priv_pass = switch.snmp_priv_password.as_ref().map(|v| decrypt_password(v));

    let params = SnmpParamsLegacy {
        ip: ip_address.clone(),
        port: switch.snmp_port,
        version: switch.snmp_version.clone(),
        community: decrypted_community,
        username: switch.snmp_username.clone(),
        auth_proto: switch.snmp_auth_protocol.clone(),
        auth_pass: decrypted_auth_pass,
        priv_proto: switch.snmp_priv_protocol.clone(),
        priv_pass: decrypted_priv_pass,
    };

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
    use crate::models::Switch;
    use crate::crypto::decrypt_password;

    let switch = sqlx::query_as::<_, Switch>(
        r#"SELECT 
            id, name, network_region_id, network_id,
            model, vendor, 
            location, snmp_version, 
            snmp_community, 
            snmp_username, snmp_auth_protocol, 
            snmp_auth_password, 
            snmp_priv_protocol, 
            snmp_priv_password, 
            snmp_port, 
            parent_switch_id, parent_port_id, description, created_at, updated_at 
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

    let decrypted_community = switch.snmp_community.as_ref().map(|v| decrypt_password(v));
    let decrypted_auth_pass = switch.snmp_auth_password.as_ref().map(|v| decrypt_password(v));
    let decrypted_priv_pass = switch.snmp_priv_password.as_ref().map(|v| decrypt_password(v));

    let params = SnmpParamsLegacy {
        ip: ip_address.clone(),
        port: switch.snmp_port,
        version: switch.snmp_version.clone(),
        community: decrypted_community,
        username: switch.snmp_username.clone(),
        auth_proto: switch.snmp_auth_protocol.clone(),
        auth_pass: decrypted_auth_pass,
        priv_proto: switch.snmp_priv_protocol.clone(),
        priv_pass: decrypted_priv_pass,
    };

    let entries = get_arp_table_via_snmp(&params).await?;
    Ok(entries)
}

pub async fn get_switch_mac_table(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();

    let switch = sqlx::query_as::<_, Switch>(
        r#"SELECT 
            id, name, network_region_id, network_id,
            model, vendor, 
            location, snmp_version, 
            snmp_community, 
            snmp_username, snmp_auth_protocol, 
            snmp_auth_password, 
            snmp_priv_protocol, 
            snmp_priv_password, 
            snmp_port, 
            parent_switch_id, parent_port_id, description, created_at, updated_at 
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

    let network_id = switch.network_id;

    if network_id.is_none() {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error("交换机没有关联网段，无法获取MAC表")));
    }

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

    match get_arp_table_via_snmp(&snmp_params).await {
        Ok(entries) => {
            if !entries.is_empty() {
                let net_id = network_id.unwrap();
                let now = Utc::now();
                
                for entry in &entries {
                    let ip_version = if entry.ip_address.contains(':') { 6i16 } else { 4i16 };
                    let mac_opt = Some(entry.mac_address.clone());
                    
                    let _ = sqlx::query(
                        r#"INSERT INTO ip_managers (id, switch_id, device_type, network_id, ip_address, ip_version, mac_address, status, last_seen, created_at, updated_at) 
                           VALUES (gen_random_uuid(), $1, 'switch_port', $2, CAST($3 AS INET), $4, $5, 'active', $6, $6, $6)
                           ON CONFLICT (ip_address, network_id) 
                           DO UPDATE SET 
                               mac_address = EXCLUDED.mac_address,
                               switch_id = EXCLUDED.switch_id,
                               status = 'active',
                               last_seen = EXCLUDED.last_seen,
                               updated_at = EXCLUDED.updated_at"#
                    )
                    .bind(switch_id)
                    .bind(net_id)
                    .bind(&entry.ip_address)
                    .bind(ip_version)
                    .bind(&mac_opt)
                    .bind(now)
                    .execute(pool.get_conn())
                    .await;
                }
            }
            
            Ok(HttpResponse::Ok().json(ApiResponse::success(entries, "获取ARP表成功")))
        }
        Err(e) => Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error(format!("获取ARP表失败: {}", e)))),
    }
}
