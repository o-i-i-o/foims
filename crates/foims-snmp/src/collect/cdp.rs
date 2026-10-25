//! CISCO-CDP-MIB 邻居采集（cdpCacheTable）与邻居增删事件比对。
//!
//! cdpCacheTable（1.3.6.1.4.1.9.9.23.1.1.1.1）实例索引为
//! (本地 ifIndex, 缓存条目号) 双弧，本地端口名经 ifDescr 表映射。非 Cisco
//! 设备不实现 CDP，walk 建立失败记告警并返回空集。邻居 diff 与第一层
//! `diff_arp_entries` 同构：按 (local_port, neighbor_device_id) 自然键比对新旧，
//! 新增→`cdp_neighbor_added`(info)、消失→`cdp_neighbor_removed`(info)。

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use async_snmp::{Client, Value, oid};
use tracing::debug;

use foims_common::log_warn;
use foims_models::{CdpRecord, DeviceCdp, SNMP_EVENT_CDP_ADDED, SNMP_EVENT_CDP_REMOVED};

use crate::core::{SnmpError, SnmpParamsLegacy, build_auth, format_snmp_error, snmp_target};
use crate::events::ObsEvent;

/// walk cdpCacheTable 单列，返回 `(本地 ifIndex, 缓存条目) → 值` 映射。
async fn walk_cdp_column(client: &Client, col: u32) -> HashMap<(u32, u32), Value> {
    let base = match col {
        4 => oid!(1, 3, 6, 1, 4, 1, 9, 9, 23, 1, 1, 1, 1, 4),
        6 => oid!(1, 3, 6, 1, 4, 1, 9, 9, 23, 1, 1, 1, 1, 6),
        7 => oid!(1, 3, 6, 1, 4, 1, 9, 9, 23, 1, 1, 1, 1, 7),
        _ => oid!(1, 3, 6, 1, 4, 1, 9, 9, 23, 1, 1, 1, 1, 8),
    };
    let mut map = HashMap::new();
    let Ok(mut walk) = client.walk(base) else {
        log_warn!("log.device.snmp.cdp_walk_failed");
        return map;
    };
    while let Some(result) = walk.next().await {
        let Ok(vb) = result else { continue };
        if vb.value.is_exception() {
            if matches!(vb.value, Value::EndOfMibView) {
                break;
            }
            continue;
        }
        let arcs = vb.oid.arcs();
        // 前缀 13 弧 + ifIndex + cacheIndex
        if arcs.len() >= 15 {
            let if_index = arcs[arcs.len() - 2];
            let cache_index = arcs[arcs.len() - 1];
            map.insert((if_index, cache_index), vb.value.clone());
        }
    }
    map
}

/// 值列取字符串（优先 as_str，回退 as_bytes→UTF-8 lossy，去空白）。
fn column_str(map: &HashMap<(u32, u32), Value>, key: &(u32, u32)) -> Option<String> {
    let v = map.get(key)?;
    let s = v
        .as_str()
        .map(str::to_string)
        .or_else(|| v.as_bytes().map(|b| String::from_utf8_lossy(b).to_string()))?;
    let t = s.trim_matches('\0').trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

pub async fn get_cdp_neighbors_via_snmp(
    params: &SnmpParamsLegacy,
) -> Result<Vec<CdpRecord>, SnmpError> {
    let addr = snmp_target(&params.ip, params.port);
    let timeout = Duration::from_secs(params.timeout_secs);
    let auth = build_auth(params).map_err(SnmpError::Message)?;
    let client = Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    // 本地端口名（ifDescr：1.3.6.1.2.1.2.2.1.2，单弧索引 ifIndex）
    let if_descr = super::ports::walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 2),
        "log.device.snmp.cdp_walk_failed",
    )
    .await;

    let address = walk_cdp_column(&client, 4).await;
    let device_id = walk_cdp_column(&client, 6).await;
    let device_port = walk_cdp_column(&client, 7).await;
    let platform = walk_cdp_column(&client, 8).await;

    let mut neighbors = Vec::new();
    for key in device_id.keys() {
        // 无邻居设备 ID 的缓存条目跳过（自然键的一部分，缺失无法判定增删）
        let Some(neighbor_device_id) = column_str(&device_id, key) else {
            continue;
        };
        let if_index = key.0;
        let local_port = if_descr
            .get(&if_index)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| if_index.to_string());
        neighbors.push(CdpRecord {
            local_port,
            neighbor_device_id: Some(neighbor_device_id),
            neighbor_port: column_str(&device_port, key),
            neighbor_address: column_str(&address, key),
            neighbor_platform: column_str(&platform, key),
        });
    }

    debug!("获取 CDP 邻居完成: {} 条记录", neighbors.len());
    Ok(neighbors)
}

/// 比对新旧 CDP 邻居，产出新增/消失事件（纯函数）。
///
/// - `prev`：设备现有 device_cdps 行；`cur`：本轮 SNMP 采集记录；
/// - 自然键 `(local_port, neighbor_device_id)`：新增落 `cdp_neighbor_added`、
///   消失落 `cdp_neighbor_removed`，均为 info（控噪，不发通知）；
/// - 首轮基线（prev 空）逐条落 added，与 ARP 基线口径一致。
#[must_use]
pub fn diff_cdp_neighbors(prev: &[DeviceCdp], cur: &[CdpRecord]) -> Vec<ObsEvent> {
    let key_of = |port: &str, dev: Option<&str>| (port.to_string(), dev.unwrap_or("").to_string());

    let prev_keys: HashSet<(String, String)> = prev
        .iter()
        .map(|r| key_of(&r.local_port, r.neighbor_device_id.as_deref()))
        .collect();
    let cur_keys: HashSet<(String, String)> = cur
        .iter()
        .filter_map(|r| {
            r.neighbor_device_id
                .as_deref()
                .map(|d| key_of(&r.local_port, Some(d)))
        })
        .collect();

    let mut events = Vec::new();
    for r in cur {
        let Some(dev) = r.neighbor_device_id.as_deref() else {
            continue;
        };
        if !prev_keys.contains(&(r.local_port.to_string(), dev.to_string())) {
            events.push(ObsEvent::new(
                SNMP_EVENT_CDP_ADDED,
                Some(r.local_port.clone()),
                foims_models::SNMP_SEVERITY_INFO,
                serde_json::json!({ "neighbor": dev, "address": r.neighbor_address }),
            ));
        }
    }
    for r in prev {
        let Some(dev) = r.neighbor_device_id.as_deref() else {
            continue;
        };
        if !cur_keys.contains(&(r.local_port.to_string(), dev.to_string())) {
            events.push(ObsEvent::new(
                SNMP_EVENT_CDP_REMOVED,
                Some(r.local_port.clone()),
                foims_models::SNMP_SEVERITY_INFO,
                serde_json::json!({ "neighbor": dev }),
            ));
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn cur(port: &str, dev: &str) -> CdpRecord {
        CdpRecord {
            local_port: port.to_string(),
            neighbor_device_id: Some(dev.to_string()),
            neighbor_port: None,
            neighbor_address: Some("10.0.0.1".to_string()),
            neighbor_platform: None,
        }
    }

    fn prev(port: &str, dev: &str) -> DeviceCdp {
        DeviceCdp {
            id: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            local_port: port.to_string(),
            neighbor_device_id: Some(dev.to_string()),
            neighbor_port: None,
            neighbor_address: None,
            neighbor_platform: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn baseline_records_all_added() {
        let events = diff_cdp_neighbors(&[], &[cur("Gi0/1", "SW-A"), cur("Gi0/2", "SW-B")]);
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e.event_type == SNMP_EVENT_CDP_ADDED));
        assert!(events.iter().all(|e| e.severity == "info"));
    }

    #[test]
    fn added_and_removed_detected() {
        // 旧：Gi0/1->SW-A、Gi0/2->SW-B；新：Gi0/1->SW-A（不变）、Gi0/3->SW-C（新增）
        let prev_rows = vec![prev("Gi0/1", "SW-A"), prev("Gi0/2", "SW-B")];
        let cur_rows = vec![cur("Gi0/1", "SW-A"), cur("Gi0/3", "SW-C")];
        let events = diff_cdp_neighbors(&prev_rows, &cur_rows);
        let added: Vec<&str> = events
            .iter()
            .filter(|e| e.event_type == SNMP_EVENT_CDP_ADDED)
            .map(|e| e.interface.as_deref().unwrap_or(""))
            .collect();
        let removed: Vec<&str> = events
            .iter()
            .filter(|e| e.event_type == SNMP_EVENT_CDP_REMOVED)
            .map(|e| e.interface.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(added, vec!["Gi0/3"], "仅新端口邻居算新增");
        assert_eq!(removed, vec!["Gi0/2"], "消失的邻居算移除");
    }

    #[test]
    fn unchanged_yields_no_events() {
        let prev_rows = vec![prev("Gi0/1", "SW-A")];
        let cur_rows = vec![cur("Gi0/1", "SW-A")];
        assert!(diff_cdp_neighbors(&prev_rows, &cur_rows).is_empty());
    }
}
