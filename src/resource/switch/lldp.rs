use std::collections::HashMap;

use actix_web::{HttpResponse, Result, web};
use async_snmp::{Client, oid, VarBind};
use tracing::debug;
use uuid::Uuid;

use crate::db::DbPool;
use crate::models::{ApiResponse, LldpNeighbor, Switch};

use super::snmp::{SnmpError, SnmpParamsLegacy, build_auth, format_snmp_error, DecryptedSnmpCredentials};

pub async fn get_lldp_neighbors(
    pool: &sqlx::PgPool,
    switch_id: &Uuid,
) -> Result<Vec<LldpNeighbor>, SnmpError> {
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

    let creds = DecryptedSnmpCredentials::from_switch(&switch);
    let params = SnmpParamsLegacy {
        ip: ip_address.clone(),
        port: switch.snmp_port,
        version: switch.snmp_version.clone(),
        community: creds.community,
        username: switch.snmp_username.clone(),
        auth_proto: switch.snmp_auth_protocol.clone(),
        auth_pass: creds.auth_password,
        priv_proto: switch.snmp_priv_protocol.clone(),
        priv_pass: creds.priv_password,
    };
    get_lldp_neighbors_via_snmp(&params).await
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
        let vb = result.map_err(|e| SnmpError::Message(format!("{}: {}", error_msg, format_snmp_error(e))))?;
        
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
) -> Result<Vec<crate::models::LldpNeighbor>, SnmpError> {
    let addr = format!("{}:{}", params.ip, params.port);
    let timeout = std::time::Duration::from_secs(30);

    let auth = build_auth(params).map_err(SnmpError::Message)?;

    let client = Client::builder(&addr, auth)
        .timeout(timeout)
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
    }).await?;

    let lldp_loc_port_subtype = oid!(1, 0, 8802, 1, 1, 2, 1, 3, 7, 1, 2);
    let mut loc_port_subtype_map: HashMap<String, u8> = HashMap::new();
    
    snmp_walk(&client, lldp_loc_port_subtype, "LLDP端口子类型表walk失败", |vb| {
        let oid_parts = vb.oid.arcs();
        if oid_parts.len() >= 12 {
            let port_num = oid_parts[11].to_string();
            let subtype = if let Some(s) = vb.value.as_u32() {
                s as u8
            } else if let Some(bytes) = vb.value.as_bytes() {
                if !bytes.is_empty() { bytes[0] } else { 0 }
            } else {
                0
            };
            loc_port_subtype_map.insert(port_num, subtype);
        }
        Ok(())
    }).await?;

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
    }).await?;

    let lldp_loc_port_desc = oid!(1, 0, 8802, 1, 1, 2, 1, 3, 7, 1, 4);
    let mut port_desc_map: HashMap<String, String> = HashMap::new();
    
    snmp_walk(&client, lldp_loc_port_desc, "LLDP端口描述表walk失败", |vb| {
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
    }).await?;

    let lldp_rem_table = oid!(1, 0, 8802, 1, 1, 2, 1, 4, 1, 1);
    let mut neighbor_data: HashMap<(String, String), crate::models::LldpNeighbor> = HashMap::new();
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
                    &port_desc_map
                );
                crate::models::LldpNeighbor {
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
                        debug!("邻居端口 {:?} subtype={} raw={:?} formatted={}", key, subtype, port_id, port_id_str);
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
                        entry.neighbor_sys_name = Some(String::from_utf8_lossy(sys_name).to_string());
                    }
                }
                10 => {
                    if let Some(sys_desc) = vb.value.as_str() {
                        entry.neighbor_sys_desc = Some(sys_desc.to_string());
                    } else if let Some(sys_desc) = vb.value.as_bytes() {
                        entry.neighbor_sys_desc = Some(String::from_utf8_lossy(sys_desc).to_string());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }).await?;

    let neighbors: Vec<crate::models::LldpNeighbor> = neighbor_data
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
    
    let port_id = port_id_map.get(local_port_num).cloned().unwrap_or_else(|| local_port_num.to_string());
    let is_mac = is_mac_address(&port_id);
    
    if is_mac {
        port_desc_map.get(local_port_num).cloned().unwrap_or(port_id)
    } else {
        port_id
    }
}

fn is_mac_address(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        return false;
    }
    parts.iter().all(|part| {
        part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit())
    })
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
    bytes.iter().all(|&b| (0x20..=0x7E).contains(&b) || b == b'\t')
}

fn format_mac_address(bytes: &[u8]) -> String {
    if bytes.len() == 6 {
        format!(
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            bytes[0], bytes[1], bytes[2],
            bytes[3], bytes[4], bytes[5]
        )
    } else {
        bytes.iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(":")
    }
}

pub async fn get_switch_lldp_neighbors(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();

    match get_lldp_neighbors(pool.get_conn(), &switch_id).await {
        Ok(neighbors) => Ok(HttpResponse::Ok().json(ApiResponse::success(neighbors, "获取LLDP邻居成功"))),
        Err(e) => Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error(format!("获取LLDP邻居失败: {}", e)))),
    }
}
