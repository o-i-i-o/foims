//! Q-BRIDGE-MIB VLAN 台账采集（dot1qVlanStaticTable）。
//!
//! VLAN 名 / 成员（egress）/ 非标记（untagged）端口位图均以 dot1dBasePort
//! 编码，需经 BRIDGE-MIB `dot1dBasePortIfIndex`（1.3.6.1.2.1.17.1.4.1.2）
//! 映射为 ifIndex 后落库。非交换设备无该表，返回空集不报错。

use std::time::Duration;

use async_snmp::{Client, Value, oid};
use tracing::debug;

use foims_models::VlanRecord;

use super::ports::{port_list_members, value_as_i32, walk_index_value_map};
use crate::core::{
    SnmpError, SnmpParamsLegacy, build_auth, clean_snmp_text, format_snmp_error, snmp_target,
};

pub async fn get_vlans_via_snmp(params: &SnmpParamsLegacy) -> Result<Vec<VlanRecord>, SnmpError> {
    let addr = snmp_target(&params.ip, params.port);
    let timeout = Duration::from_secs(params.timeout_secs);
    let auth = build_auth(params).map_err(SnmpError::Message)?;
    let client = Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    // 桥端口(dot1dBasePort) → ifIndex 映射（成员位图换算前提）
    let bp_if_raw = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 17, 1, 4, 1, 2),
        "log.device.snmp.vlan_walk_failed",
    )
    .await;
    let bp_to_if: std::collections::HashMap<u32, u32> = bp_if_raw
        .into_iter()
        .filter_map(|(bp, value)| {
            let if_index = value_as_i32(Some(&value))?;
            u32::try_from(if_index).ok().map(|idx| (bp, idx))
        })
        .collect();

    let names = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 17, 7, 1, 4, 3, 1, 1),
        "log.device.snmp.vlan_walk_failed",
    )
    .await;
    let egress = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 17, 7, 1, 4, 3, 1, 2),
        "log.device.snmp.vlan_walk_failed",
    )
    .await;
    let untagged = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 17, 7, 1, 4, 3, 1, 4),
        "log.device.snmp.vlan_walk_failed",
    )
    .await;

    let to_ifindexes = |value: Option<&Value>| -> Vec<u32> {
        value
            .and_then(|v| v.as_bytes())
            .map(|bytes| {
                port_list_members(bytes)
                    .into_iter()
                    .filter_map(|bp| bp_to_if.get(&bp).copied())
                    .collect()
            })
            .unwrap_or_default()
    };

    let mut vlans = Vec::new();
    // C1：名称/egress/untagged 三列索引并集驱动并排序——此前仅以
    // names.keys() 驱动，不返回名称列的 VLAN（行存在但无 name 列）
    // 会连成员位图一起丢失
    let mut vlan_ids: Vec<u32> = names
        .keys()
        .chain(egress.keys())
        .chain(untagged.keys())
        .copied()
        .collect();
    vlan_ids.sort_unstable();
    vlan_ids.dedup();
    for vlan in vlan_ids {
        let vlan = &vlan;
        if !(1u32..=4094).contains(vlan) {
            continue;
        }
        let name = clean_snmp_text(names.get(vlan).and_then(|v| v.as_str()));
        vlans.push(VlanRecord {
            vlan_id: i32::try_from(*vlan).unwrap_or(0),
            name,
            member_ports: to_ifindexes(egress.get(vlan)),
            untagged_ports: to_ifindexes(untagged.get(vlan)),
        });
    }

    debug!("获取 VLAN 台账完成: {} 条记录", vlans.len());
    Ok(vlans)
}
