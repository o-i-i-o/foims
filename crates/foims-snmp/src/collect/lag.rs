//! IEEE8023-LAG-MIB 聚合口台账采集（ifTable + dot3adAgg 成员归属列）。
//!
//! 聚合口识别：`ifTable.ifType == 161(ieee8023adLag)`。成员归属按优先级：
//! 1. 标准 `dot3adAggPortAttachedAggID`（1.2.840.10006.300.43.1.1.1.1.13，
//!    dot3adAggPortEntry 列 13）：行=成员端口 ifIndex，值=所属聚合口 ifIndex，
//!    Cisco/H3C 等标准实现走此列；
//! 2. 华为 CE 变体（标准列返回空时启用）：实测华为 CE12812 把成员行挂在
//!    `dot3adAggTable`（1.2.840.10006.300.43.1.2.1.1.13），列 12/13（Selected/
//!    Attached）值=所属聚合口 ifIndex，标准 portEntry 列反而为 0 行。
//!
//! 未附加成员两处均以 0 或 2147483647（0x7FFFFFFF 哨兵）上报，过滤之——
//! 仅收录 LACP 已附加成员，未附加属设备真实状态（如对端未启用 LACP）。
//! 端口名统一取 ifDescr。无 LACP 的设备返回空集。

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_snmp::{Client, oid};
use tracing::debug;

use foims_models::LagRecord;

use foims_common::log_warn;

use super::ports::value_as_i32;
use super::walk::{walk_index_value_map, walk_index_value_map_checked};
use crate::core::{
    SnmpError, SnmpParamsLegacy, build_auth, clean_snmp_text, snmp_target, wrap_snmp_error,
};

/// ifType 常量：ieee8023adLag（LACP 链路聚合口）。
const IF_TYPE_LACP: i32 = 161;

/// 未附加成员哨兵值：华为 CE 以 2147483647（0x7FFFFFFF）上报「未选择/未附加」。
const UNATTACHED_SENTINEL: i32 = i32::MAX;

/// 标准成员列「确认空集」记忆 TTL：实测华为 CE 对标准 portEntry.13 连 GETBULK
/// 探针都不响应（静默丢包），每次同步冷探针即白付 6s 才降级确认空表。MIB 支持
/// 属固件能力，短期不会变化；确认空表后 TTL 内跳过标准列直接走变体列。仅存
/// 进程内，重启后重新确认一次。
const STANDARD_EMPTY_TTL_SECS: u64 = 21600;

/// 标准 portEntry.13 基 OID（写死与采集点一致的文本，供记忆键使用）。
const OID_STANDARD_ATTACHED: &str = "1.2.840.10006.300.43.1.1.1.1.13";

/// 标准成员列「确认空集」记忆表：键=设备地址|OID，值=最近确认空表时刻。
static STANDARD_EMPTY_MEMORY: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn standard_empty_hit(key: &str) -> bool {
    let mut map = STANDARD_EMPTY_MEMORY
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    match map.get(key) {
        Some(at) if at.elapsed() <= Duration::from_secs(STANDARD_EMPTY_TTL_SECS) => true,
        Some(_) => {
            map.remove(key);
            false
        }
        None => false,
    }
}

fn standard_empty_confirm(key: &str) {
    STANDARD_EMPTY_MEMORY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key.to_string(), Instant::now());
}

fn standard_empty_evict(key: &str) {
    STANDARD_EMPTY_MEMORY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(key);
}

/// 成员归属列值有效性：正数且非未附加哨兵。
fn is_attached_agg_id(value: i32) -> bool {
    value > 0 && value != UNATTACHED_SENTINEL
}

/// 采集 LACP 聚合口台账（聚合口 + 成员端口名/ifIndex）。
///
/// 错误仅来自鉴权构造与会话建立；无 LACP 的设备返回空集。
/// `lacp_state` 本采集器不填（恒为 None）。
pub async fn get_lags_via_snmp(params: &SnmpParamsLegacy) -> Result<Vec<LagRecord>, SnmpError> {
    let addr = snmp_target(&params.ip, params.port);
    let timeout = Duration::from_secs(params.timeout_secs);
    let auth = build_auth(params).map_err(SnmpError::Message)?;
    let client = Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .retry(params.retry())
        .connect()
        .await
        .map_err(|e| wrap_snmp_error("server.snmp.error.session_create_failed", e))?;

    let if_descr = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 2),
        "log.device.snmp.lag_walk_failed",
    )
    .await;
    let if_type = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 3),
        "log.device.snmp.lag_walk_failed",
    )
    .await;

    // 排序保证台账输出确定性
    let mut aggregators: Vec<u32> = if_type
        .iter()
        .filter(|(_, v)| value_as_i32(Some(v)) == Some(IF_TYPE_LACP))
        .map(|(idx, _)| *idx)
        .collect();
    aggregators.sort_unstable();
    if aggregators.is_empty() {
        debug!("设备无 LACP 聚合口，返回空集");
        return Ok(Vec::new());
    }

    // 成员归属：标准 dot3adAggPortAttachedAggID 优先；为空（华为 CE 等非标
    // 实现）回退华为变体列。两列行语义一致：末弧=成员端口 ifIndex，值=聚合口。
    // 标准列采集失败（设备故障语义，非空表）不记忆直接走变体列并留痕告警；
    // 确认空表写入记忆，TTL 内跳过标准列白付的探针等待
    let memory_key = format!("{}|{OID_STANDARD_ATTACHED}", client.peer_addr());
    let attached = if standard_empty_hit(&memory_key) {
        walk_index_value_map(
            &client,
            oid!(1, 2, 840, 10006, 300, 43, 1, 2, 1, 1, 13),
            "log.device.snmp.lag_walk_failed",
        )
        .await
    } else {
        match walk_index_value_map_checked(
            &client,
            oid!(1, 2, 840, 10006, 300, 43, 1, 1, 1, 1, 13),
        )
        .await
        {
            Ok(map) if !map.is_empty() => {
                standard_empty_evict(&memory_key);
                map
            }
            Ok(_) => {
                standard_empty_confirm(&memory_key);
                walk_index_value_map(
                    &client,
                    oid!(1, 2, 840, 10006, 300, 43, 1, 2, 1, 1, 13),
                    "log.device.snmp.lag_walk_failed",
                )
                .await
            }
            Err(e) => {
                log_warn!(
                    "log.device.snmp.lag_walk_failed",
                    error = e.message().log_string()
                );
                walk_index_value_map(
                    &client,
                    oid!(1, 2, 840, 10006, 300, 43, 1, 2, 1, 1, 13),
                    "log.device.snmp.lag_walk_failed",
                )
                .await
            }
        }
    };
    let members_by_agg: HashMap<u32, Vec<u32>> = attached
        .iter()
        .filter_map(|(member, value)| {
            let agg = value_as_i32(Some(value))?;
            if !is_attached_agg_id(agg) {
                return None;
            }
            u32::try_from(agg).ok().map(|a| (a, *member))
        })
        .fold(
            HashMap::new(),
            |mut acc: HashMap<u32, Vec<u32>>, (agg, member)| {
                acc.entry(agg).or_default().push(member);
                acc
            },
        );

    let mut lags = Vec::new();
    for agg in aggregators {
        let mut member_ifindexes = members_by_agg.get(&agg).cloned().unwrap_or_default();
        member_ifindexes.sort_unstable();
        let member_names = member_ifindexes
            .iter()
            .map(|m| {
                clean_snmp_text(if_descr.get(m).and_then(|v| v.as_str()))
                    .unwrap_or_else(|| m.to_string())
            })
            .collect();
        lags.push(LagRecord {
            agg_ifindex: i32::try_from(agg).unwrap_or(0),
            agg_name: clean_snmp_text(if_descr.get(&agg).and_then(|v| v.as_str())),
            member_ifindexes,
            member_names,
            lacp_state: None,
        });
    }

    debug!("获取 LAG 聚合口完成: {} 条记录", lags.len());
    Ok(lags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_attached_agg_id_哨兵与非法值过滤() {
        assert!(is_attached_agg_id(511));
        assert!(!is_attached_agg_id(0)); // 未附加
        assert!(!is_attached_agg_id(-1)); // 负值
        assert!(!is_attached_agg_id(i32::MAX)); // 华为未附加哨兵 2147483647
    }
}
