//! IEEE8023-LAG-MIB 聚合口台账采集（dot3adAggPortTable + ifTable）。
//!
//! 聚合口识别：`ifTable.ifType == 161(ieee8023adLap)`；成员归属：
//! `dot3adAggPortAttachedAggID`（1.3.6.1.2.1.34.2.4.1.7）值即聚合口 ifIndex，
//! 实例末弧为成员 ifIndex。端口名统一取 ifDescr。无 LACP 的设备返回空集。

use std::collections::HashMap;
use std::time::Duration;

use async_snmp::{Client, oid};
use tracing::debug;

use foims_models::LagRecord;

use super::ports::{value_as_i32, walk_index_value_map};
use crate::core::{SnmpError, SnmpParamsLegacy, build_auth, format_snmp_error, snmp_target};

/// ifType 常量：ieee8023adLap（LACP 链路聚合口）。
const IF_TYPE_LACP: i32 = 161;

pub async fn get_lags_via_snmp(params: &SnmpParamsLegacy) -> Result<Vec<LagRecord>, SnmpError> {
    let addr = snmp_target(&params.ip, params.port);
    let timeout = Duration::from_secs(params.timeout_secs);
    let auth = build_auth(params).map_err(SnmpError::Message)?;
    let client = Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

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

    // 聚合口集合：ifType==161 的 ifIndex
    let aggregators: Vec<u32> = if_type
        .iter()
        .filter(|(_, v)| value_as_i32(Some(v)) == Some(IF_TYPE_LACP))
        .map(|(idx, _)| *idx)
        .collect();
    if aggregators.is_empty() {
        debug!("设备无 LACP 聚合口，返回空集");
        return Ok(Vec::new());
    }

    // 成员 ifIndex → 聚合口 ifIndex（AttachedAggID 值）
    let attached = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 34, 2, 4, 1, 7),
        "log.device.snmp.lag_walk_failed",
    )
    .await;
    let members_by_agg: HashMap<u32, Vec<u32>> = attached
        .iter()
        .filter_map(|(member, value)| {
            let agg = value_as_i32(Some(value))?;
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
                if_descr
                    .get(m)
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| m.to_string())
            })
            .collect();
        lags.push(LagRecord {
            agg_ifindex: i32::try_from(agg).unwrap_or(0),
            agg_name: if_descr
                .get(&agg)
                .and_then(|v| v.as_str())
                .map(str::to_string),
            member_ifindexes,
            member_names,
            lacp_state: None,
        });
    }

    debug!("获取 LAG 聚合口完成: {} 条记录", lags.len());
    Ok(lags)
}
