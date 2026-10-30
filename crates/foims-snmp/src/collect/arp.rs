//! ARP/FDB 表纯采集（IPv4 arpEntries + IPv6 ipNetToPhysical 双表 + VLAN 映射）。
//!
//! 采集返回结构化条目，snmp_arp_entries 落库 SQL 与 handler 留在资源域调用方。

use std::collections::{HashMap, HashSet};

use async_snmp::{Client, oid};
use tracing::debug;

use foims_common::log_warn;
use foims_models::ArpEntry;

use crate::core::{
    SnmpError, SnmpParamsLegacy, build_auth, clean_snmp_text, format_snmp_error, snmp_target,
};
use crate::events::ObsEvent;

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
            if let Some(name) = clean_snmp_text(vb.value.as_str()) {
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

/// 采集 ARP/FDB 表：IPv4 arpEntries 与 IPv6 ipNetToPhysical 双表 walk 后
/// 按 (ip, mac) 去重合并；接口名取 ifName，VLAN 由 Q-BRIDGE 表按 MAC 归属。
///
/// 任一 walk 迭代出错即整体失败（区别于 [`walk_if_name_map`] 等辅助表的
/// 缺失降级），避免半张 ARP 表被当成完整快照参与差异比对。
pub async fn get_arp_table_via_snmp(params: &SnmpParamsLegacy) -> Result<Vec<ArpEntry>, SnmpError> {
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
    // 去重键为 (ip, mac)：IPv4 arpEntries 与 ipNetToPhysical 双表存在重叠，
    // 按 IP 去重会把「同 IP 多 MAC」在采集层丢弃，使冲突检测永不成真（A1）
    let mut seen_pairs: HashSet<(String, String)> = HashSet::new();

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

                if mac_addr != "00:00:00:00:00:00"
                    && seen_pairs.insert((ip_addr.clone(), mac_addr.clone()))
                {
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
        if let Some((if_index, ip_addr)) = parse_ipnet_to_physical_index(oid_parts)
            && let Some(bytes) = vb.value.as_bytes()
            && bytes.len() >= 6
        {
            let mac_addr = format!(
                "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
            );

            if mac_addr != "00:00:00:00:00:00"
                && seen_pairs.insert((ip_addr.clone(), mac_addr.clone()))
            {
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

/// 比对上一轮与本轮 ARP/FDB 条目，产出新增/移除/冲突/未知设备观测事件（纯函数）。
///
/// - `old`：设备现有 snmp_arp_entries 行（`(ip_host, mac)` 列表）；
/// - `new`：本轮 SNMP 采集条目；
/// - `known_macs`：已登记设备网口 MAC（[`crate::core::normalize_mac`] 归一集，
///   MAC 为物理实体、IP 为虚拟实体：未知与否按 MAC 判定，不看不登记的 IP）；
/// - `baseline`：设备此前无任何条目（首轮同步）为 true——仅逐条落 `arp_added`
///   （severity=info），不发未知设备/冲突事件，避免首次全量同步造成通知风暴。
pub fn diff_arp_entries(
    old: &[(String, String)],
    new: &[ArpEntry],
    known_macs: &HashSet<String>,
    baseline: bool,
) -> Vec<ObsEvent> {
    use foims_models::{
        SNMP_EVENT_ARP_ADDED, SNMP_EVENT_ARP_CONFLICT, SNMP_EVENT_ARP_REMOVED,
        SNMP_EVENT_UNKNOWN_DEVICE, SNMP_SEVERITY_INFO, SNMP_SEVERITY_WARNING,
    };

    let mut events = Vec::new();

    // 基线只建立观测面、不判定接入合法性，故未知 MAC 也按 arp_added 落表
    if baseline {
        for entry in new {
            events.push(ObsEvent::new(
                SNMP_EVENT_ARP_ADDED,
                entry.interface.clone(),
                SNMP_SEVERITY_INFO,
                serde_json::json!({ "ip": entry.ip_address, "mac": entry.mac_address }),
            ));
        }
        return events;
    }

    let old_ips: HashSet<&str> = old.iter().map(|(ip, _)| ip.as_str()).collect();
    let new_ips: HashSet<&str> = new.iter().map(|e| e.ip_address.as_str()).collect();

    // 应管尽管：未登记设备接入才报未知，已登记 MAC 的漂移 IP 归 arp_added，
    // 其 MAC 移动由拉取侧 IP 变更检测覆盖
    // 同一新增 MAC 多 IP 时仅按首个条目产出一条事件，避免风暴（冲突另有专门事件）
    let mut handled_new_macs: HashSet<&str> = HashSet::new();
    for entry in new {
        if old_ips.contains(entry.ip_address.as_str())
            || !handled_new_macs.insert(entry.mac_address.as_str())
        {
            continue;
        }
        if known_macs.contains(&crate::core::normalize_mac(&entry.mac_address)) {
            events.push(ObsEvent::new(
                SNMP_EVENT_ARP_ADDED,
                entry.interface.clone(),
                SNMP_SEVERITY_INFO,
                serde_json::json!({ "ip": entry.ip_address, "mac": entry.mac_address }),
            ));
        } else {
            events.push(ObsEvent::new(
                SNMP_EVENT_UNKNOWN_DEVICE,
                entry.interface.clone(),
                SNMP_SEVERITY_WARNING,
                serde_json::json!({
                    "ip": entry.ip_address,
                    "mac": entry.mac_address,
                    "vlan_id": entry.vlan_id,
                }),
            ));
        }
    }

    for (ip, mac) in old {
        if !new_ips.contains(ip.as_str()) {
            events.push(ObsEvent::new(
                SNMP_EVENT_ARP_REMOVED,
                None,
                SNMP_SEVERITY_INFO,
                serde_json::json!({ "ip": ip, "mac": mac }),
            ));
        }
    }

    // 冲突：同一 IP 本轮学到多个不同 MAC（MAC 列表排序去重，保证事件确定性）
    let mut macs_by_ip: HashMap<&str, Vec<&str>> = HashMap::new();
    for entry in new {
        macs_by_ip
            .entry(entry.ip_address.as_str())
            .or_default()
            .push(entry.mac_address.as_str());
    }
    for (ip, mut macs) in macs_by_ip {
        macs.sort_unstable();
        macs.dedup();
        if macs.len() > 1 {
            events.push(ObsEvent::new(
                SNMP_EVENT_ARP_CONFLICT,
                None,
                SNMP_SEVERITY_WARNING,
                serde_json::json!({ "ip": ip, "macs": macs }),
            ));
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn arp_entry(ip: &str, mac: &str) -> ArpEntry {
        ArpEntry {
            ip_address: ip.to_string(),
            mac_address: mac.to_string(),
            interface: Some("ge0/0/1".to_string()),
            vlan_id: Some(10),
        }
    }

    #[test]
    fn diff_baseline_only_records_added() {
        let new = vec![arp_entry("10.0.0.5", "AA:BB:CC:DD:EE:01")];
        let known = HashSet::new();
        let events = diff_arp_entries(&[], &new, &known, true);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "arp_added");
        assert_eq!(events[0].severity, "info");
    }

    #[test]
    fn diff_added_removed_and_unknown() {
        // 旧表：10.0.0.1（MAC 已知）、10.0.0.9（本轮消失）
        let old = vec![
            ("10.0.0.1".to_string(), "AA:01".to_string()),
            ("10.0.0.9".to_string(), "AA:09".to_string()),
        ];
        // 本轮：10.0.0.1（未变）、新增已知 MAC 10.0.0.2、新增未知 MAC 10.0.0.3
        let new = vec![
            arp_entry("10.0.0.1", "AA:01"),
            arp_entry("10.0.0.2", "AA:02"),
            arp_entry("10.0.0.3", "AA:03"),
        ];
        let known: HashSet<String> = ["AA:01", "AA:02"]
            .iter()
            .map(|s| crate::core::normalize_mac(s))
            .collect();
        let events = diff_arp_entries(&old, &new, &known, false);
        let types: Vec<&str> = events.iter().map(|e| e.event_type).collect();
        assert!(types.contains(&"arp_added"), "应有已知 MAC 新增");
        assert!(types.contains(&"unknown_device"), "应有未知 MAC 新增");
        assert!(types.contains(&"arp_removed"), "应有移除");
        assert_eq!(
            events
                .iter()
                .filter(|e| e.detail["ip"] == "10.0.0.1")
                .count(),
            0,
            "未变 IP 不应产生事件"
        );
    }

    #[test]
    fn diff_known_mac_at_new_ip_is_not_unknown_device() {
        let old = vec![("10.0.0.1".to_string(), "AA:01".to_string())];
        let new = vec![
            arp_entry("10.0.0.1", "AA:01"),
            arp_entry("10.0.0.2", "AA:01"),
        ];
        let known: HashSet<String> = [crate::core::normalize_mac("AA:01")].into_iter().collect();
        let events = diff_arp_entries(&old, &new, &known, false);
        assert!(
            events.iter().all(|e| e.event_type != "unknown_device"),
            "已知 MAC 不应报未知设备"
        );
        let added: Vec<&ObsEvent> = events
            .iter()
            .filter(|e| e.event_type == "arp_added")
            .collect();
        assert_eq!(added.len(), 1);
    }

    #[test]
    fn diff_detects_conflict() {
        // 同一 IP 本轮学到两个不同 MAC → 冲突（其中未知 MAC 另报未知设备）
        let old = vec![("10.0.0.1".to_string(), "AA:01".to_string())];
        let new = vec![
            arp_entry("10.0.0.1", "AA:01"),
            arp_entry("10.0.0.1", "BB:02"),
        ];
        let known: HashSet<String> = [crate::core::normalize_mac("AA:01")].into_iter().collect();
        let events = diff_arp_entries(&old, &new, &known, false);
        assert!(
            events
                .iter()
                .any(|e| e.event_type == "arp_conflict" && e.severity == "warning"),
            "应检出同 IP 多 MAC 冲突"
        );
    }
}
