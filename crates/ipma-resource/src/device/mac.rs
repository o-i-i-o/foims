//! 设备 MAC 地址表管理。

use std::collections::HashMap;
use std::sync::Arc;

use async_snmp::{Client, oid};
use axum::extract::{Path, State};
use axum::response::Response;
use chrono::Utc;
use tracing::debug;
use uuid::Uuid;

use ipma_common::DbProvider;
use ipma_common::{AppError, msg};
use ipma_common::{log_error, log_warn};
use ipma_models::{ArpEntry, DeviceMac};

use super::snmp::{
    SnmpError, SnmpParamsLegacy, build_auth, format_snmp_error, get_device_snmp_config,
    snmp_target, truncate_to_column_width,
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
        log_warn!("log.device.snmp.if_name_walk_failed");
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
        log_warn!("log.device.snmp.vlan_walk_failed");
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
        // 前缀 1.3.6.1.2.1.17.7.1.2.2.1.2 共 13 弧（下标 0..=12），
        // 实例索引从下标 13 开始：Q-BRIDGE-MIB 为 VLAN 编码（多数实现
        // 为单弧）+ 6 字节 MAC。MAC 取末 6 弧以兼容变长 VLAN 编码
        let n = oid_parts.len();
        // 最小合法实例：13 前缀弧 + 1 VLAN 弧 + 6 MAC 弧 = 20
        if n >= 20 {
            let vlan_id = oid_parts[13] as i32;
            let mac_parts = &oid_parts[n - 6..];
            let mac = format!(
                "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                mac_parts[0], mac_parts[1], mac_parts[2], mac_parts[3], mac_parts[4], mac_parts[5]
            );
            map.insert(mac, vlan_id);
        }
    }

    debug!("获取VLAN映射: {} 条", map.len());
    map
}

/// 解析 ipNetToPhysicalPhysAddress（1.3.6.1.2.1.4.35.1.4）行 OID 的索引部分。
///
/// 该表索引为 (ifIndex, addrType, addr)（RFC 4293）：addr 是变长
/// OCTET STRING，SNMP 实例标识按「类型弧 + 长度弧 + 地址字节」编码，
/// 故完整布局为 `[base_len]=ifIndex`、`[base_len+1]=addrType`、
/// `[base_len+2]=addr_len`、`[base_len+3..]` 为地址字节
///（实测 snmpwalk 输出形如 `.ifIndex.2.16.<16 字节>`，总弧长 29）。
/// IPv4（addr_len=4）与 IPv6（addr_len=16）行均解析，其余返回 None。
fn parse_ipnet_to_physical_index(oid_parts: &[u32]) -> Option<(u32, String)> {
    let base_len = 10;
    if oid_parts.len() < base_len + 3 {
        return None;
    }
    let if_index = oid_parts[base_len];
    let addr_len = oid_parts[base_len + 2] as usize;
    let addr_start = base_len + 3;
    if oid_parts.len() < addr_start + addr_len {
        return None;
    }
    let addr_bytes: Vec<u8> = oid_parts[addr_start..addr_start + addr_len]
        .iter()
        .map(|&b| b as u8)
        .collect();
    let ip_addr = match addr_len {
        4 => format!(
            "{}.{}.{}.{}",
            addr_bytes[0], addr_bytes[1], addr_bytes[2], addr_bytes[3]
        ),
        16 => {
            let parts: Vec<String> = (0..8)
                .map(|i| format!("{:02x}{:02x}", addr_bytes[i * 2], addr_bytes[i * 2 + 1]))
                .collect();
            simplify_ipv6(&parts.join(":"))
        }
        _ => return None,
    };
    Some((if_index, ip_addr))
}

pub async fn get_arp_table_via_snmp(params: &SnmpParamsLegacy) -> Result<Vec<ArpEntry>, SnmpError> {
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
        // ipNetToPhysicalPhysAddress 行：ifIndex/addrLen/地址均取自索引弧
        //（addrType 位于 ifIndex 与 addrLen 之间，仅作索引用途）
        if let Some((if_index, ip_addr)) = parse_ipnet_to_physical_index(oid_parts)
            && let Some(bytes) = vb.value.as_bytes()
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

    debug!("获取MAC表完成: {} 条记录 (IPv4 + IPv6)", entries.len());
    Ok(entries)
}

fn simplify_ipv6(ip: &str) -> String {
    ip.parse::<std::net::Ipv6Addr>()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|_| ip.to_string())
}

/// device_macs.interface 列宽（VARCHAR(50)，与建表契约一致）
const INTERFACE_MAX_CHARS: usize = 50;

pub async fn get_device_mac_table<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let (switch, ip_address) = get_device_snmp_config(&conn, &device_id).await?;

    let ip_address =
        ip_address.ok_or_else(|| AppError::Validation(msg("server.device.no_ip_configured")))?;

    let snmp_params = switch.to_snmp_params_async(&ip_address).await?;

    let entries = get_arp_table_via_snmp(&snmp_params)
        .await
        .map_err(|e| AppError::Snmp(msg("server.device.snmp.arp_fetch_failed").with("error", e)))?;

    let now = Utc::now();
    let mut upserted_count = 0usize;
    let mut failed_count = 0usize;
    let mut seen_ips: Vec<String> = Vec::new();

    let mut tx = conn.begin().await?;

    for entry in &entries {
        let id = Uuid::new_v4();
        seen_ips.push(entry.ip_address.clone());
        // interface 列宽 VARCHAR(50)：入库前按字符截断（超宽截断优于必败写入）
        let interface = entry
            .interface
            .as_deref()
            .map(|iface| truncate_to_column_width(iface, INTERFACE_MAX_CHARS));
        // 逐行 SAVEPOINT：单行 upsert 失败仅回滚该行（ROLLBACK TO + RELEASE），
        // 事务保持可用后继续处理后续行，恢复"sync_partial 部分成功"语义；
        // 此前任一行失败会令 PostgreSQL 事务进入 aborted 状态，后续语句全部失败
        let mut sp = sqlx::Acquire::begin(&mut *tx).await?;
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
        .bind(&interface)
        .bind(entry.vlan_id)
        .bind(now)
        .execute(&mut *sp)
        .await;

        match result {
            Ok(r) => {
                sp.commit().await?;
                if r.rows_affected() > 0 {
                    upserted_count += 1;
                }
            }
            Err(e) => {
                // 回滚到 SAVEPOINT，恢复本事务可用状态后计入失败并继续
                sp.rollback().await?;
                failed_count += 1;
                log_error!(
                    "log.device.mac.record_write_failed",
                    ip = entry.ip_address,
                    mac = entry.mac_address,
                    error = e
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

    // 按同步结果构造消息：有失败 / 全部成功
    let message = if failed_count > 0 {
        msg("server.device.mac.sync_partial")
            .with("synced", upserted_count)
            .with("failed", failed_count)
    } else {
        msg("server.device.mac.synced").with("count", upserted_count)
    };

    Ok(ipma_common::ok_json(saved_macs, message))
}

pub async fn get_device_macs_from_db<P: DbProvider>(
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

    let macs: Vec<DeviceMac> = sqlx::query_as::<_, DeviceMac>(
        r"SELECT id, device_id, host(ip_address) as ip_address, mac_address, interface, vlan_id, created_at, updated_at
           FROM device_macs WHERE device_id = $1 ORDER BY ip_address",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await
    .map_err(|e| {
        log_error!("log.device.mac.query_failed", error = e);
        AppError::Database(msg("server.error.database"))
    })?;

    Ok(ipma_common::ok_json(macs, "server.device.mac.fetched"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造 ipNetToPhysicalPhysAddress 行 OID：基弧(10) + ifIndex + addrType
    /// + addrLen + 地址字节，与 RFC 4293 索引编码一致。
    fn neighbor_oid(if_index: u32, addr_type: u32, addr: &[u8]) -> Vec<u32> {
        let mut oid: Vec<u32> = vec![1, 3, 6, 1, 2, 1, 4, 35, 1, 4];
        oid.push(if_index);
        oid.push(addr_type);
        oid.push(addr.len() as u32);
        oid.extend(addr.iter().map(|&b| b as u32));
        oid
    }

    #[test]
    fn test_parse_neighbor_index_ipv6() {
        // 29 弧长的真实 IPv6 邻居行：fe80::1（ifIndex=3, addrType=2, addrLen=16）
        let addr = [0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        let oid = neighbor_oid(3, 2, &addr);
        assert_eq!(oid.len(), 29, "IPv6 邻居行应为 29 弧长");
        let (if_index, ip) = parse_ipnet_to_physical_index(&oid)
            .unwrap_or_else(|| panic!("IPv6 邻居 OID 应解析成功"));
        assert_eq!(if_index, 3);
        assert_eq!(ip, "fe80::1");
    }

    #[test]
    fn test_parse_neighbor_index_ipv4() {
        // 同表也携带 IPv4 行（ifIndex=2, addrType=1, addrLen=4）
        let oid = neighbor_oid(2, 1, &[10, 0, 0, 6]);
        let (if_index, ip) = parse_ipnet_to_physical_index(&oid)
            .unwrap_or_else(|| panic!("IPv4 邻居 OID 应解析成功"));
        assert_eq!(if_index, 2);
        assert_eq!(ip, "10.0.0.6");
    }

    #[test]
    fn test_parse_neighbor_index_rejects_bad_shapes() {
        // 不足基弧 + ifIndex + addrType + addrLen
        assert!(parse_ipnet_to_physical_index(&[1, 3, 6, 1, 2, 1, 4, 35, 1, 4, 2, 2]).is_none());
        // 地址字节被截断：声明 16 字节仅给 2 字节
        let truncated: Vec<u32> = vec![1, 3, 6, 1, 2, 1, 4, 35, 1, 4, 2, 2, 16, 0xfe, 0x80];
        assert!(parse_ipnet_to_physical_index(&truncated).is_none());
        // 非法地址长度（如 6 字节）不解析
        let odd = neighbor_oid(2, 2, &[1, 2, 3, 4, 5, 6]);
        assert!(parse_ipnet_to_physical_index(&odd).is_none());
    }
}
