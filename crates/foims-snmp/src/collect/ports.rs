//! 设备端口表采集（IF-MIB ifTable/ifXTable + BRIDGE/Q-BRIDGE VLAN 判定）。

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use async_snmp::{Client, Value, oid};
use tracing::debug;

use foims_common::log_warn;
use foims_models::SnmpPort;

use crate::core::{SnmpError, SnmpParamsLegacy, build_auth, format_snmp_error, snmp_target};

/// walk 单弧索引标量表：返回（实例尾弧 → 值）映射。
///
/// 适用于 ifTable（ifIndex）、BRIDGE-MIB（dot1dBasePort）、
/// Q-BRIDGE-MIB 静态 VLAN 表（dot1qVlanIndex）等单弧索引表；
/// walk 建立失败时记告警并返回空映射（字段降级为 NULL，不阻断同步）。
async fn walk_index_value_map(
    client: &Client,
    base: async_snmp::Oid,
    warn_key: &'static str,
) -> HashMap<u32, Value> {
    let mut map = HashMap::new();

    let Ok(mut walk) = client.walk(base) else {
        log_warn!(warn_key);
        return map;
    };

    while let Some(result) = walk.next().await {
        let Ok(vb) = result else {
            continue;
        };
        if vb.value.is_exception() {
            if matches!(vb.value, Value::EndOfMibView) {
                break;
            }
            continue;
        }
        if let Some(&index) = vb.oid.arcs().last() {
            map.insert(index, vb.value);
        }
    }

    map
}

/// SNMP Integer/Gauge32 值统一取 i32（其余类型返回 None）
fn value_as_i32(value: Option<&Value>) -> Option<i32> {
    match value {
        Some(Value::Integer(i)) => Some(*i),
        Some(Value::Gauge32(g)) => i32::try_from(*g).ok(),
        _ => None,
    }
}

/// 由 ifAdminStatus/ifOperStatus 推导端口状态。
///
/// 管理态 down 优先（admin-down）；运营态 up(1) → up，其余
///（down/testing/unknown/dormant/notPresent 2..=6）一律视为 down；
/// 数据缺失返回 None（同步时保留原值）。
fn derive_port_status(admin: Option<i32>, oper: Option<i32>) -> Option<String> {
    match admin {
        Some(2) => Some("admin-down".to_string()),
        Some(1) => match oper {
            Some(1) => Some("up".to_string()),
            Some(2..=6) => Some("down".to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// 速率格式化：整千进入 Gbps（1000 → 1Gbps、10000 → 10Gbps），
/// 其余 Mbps（2500 → 2500Mbps）；0 视为未知。
fn format_speed(mbps: u64) -> Option<String> {
    match mbps {
        0 => None,
        n if n >= 1000 && n % 1000 == 0 => Some(format!("{}Gbps", n / 1000)),
        n => Some(format!("{}Mbps", n)),
    }
}

/// 由 ifHighSpeed（Mbps）推导速率，0 或缺失时回退
/// ifSpeed（bps，仅实现 MIB-2 的老设备；Gauge32 上限约 4.29G，
/// 更高速率端口以 ifHighSpeed 为准）。
fn derive_port_speed(high: Option<&Value>, speed: Option<&Value>) -> Option<String> {
    if let Some(Value::Gauge32(mbps)) = high
        && *mbps > 0
    {
        return format_speed(u64::from(*mbps));
    }
    if let Some(Value::Gauge32(bps)) = speed {
        return format_speed(u64::from(*bps) / 1_000_000);
    }
    None
}

/// 解析 PortList 位图（dot1qVlanStaticEgressPorts 值）：
/// 第 p 位（高位在前）代表 dot1dBasePort p，返回置位的桥端口列表。
fn port_list_members(bytes: &[u8]) -> Vec<u32> {
    bytes
        .iter()
        .enumerate()
        .flat_map(|(byte_idx, &b)| {
            (0..8u32)
                .filter(move |&bit| b & (0x80u8 >> bit) != 0)
                .map(move |bit| (byte_idx as u32) * 8 + bit + 1)
        })
        .collect()
}

/// 汇总 VLAN 成员位图：dot1qVlanStaticEgressPorts 按 VLAN 展开位图，
/// 借助 桥端口→ifIndex 映射 聚合出各 ifIndex 所属 VLAN 集合。
async fn collect_vlan_members(
    client: &Client,
    bp_to_if: &HashMap<u32, u32>,
) -> HashMap<u32, HashSet<i32>> {
    let egress = walk_index_value_map(
        client,
        oid!(1, 3, 6, 1, 2, 1, 17, 7, 1, 4, 3, 1, 2),
        "log.device.snmp.vlan_walk_failed",
    )
    .await;

    let mut members: HashMap<u32, HashSet<i32>> = HashMap::new();
    for (vlan, value) in egress {
        // dot1qVlanStaticTable 以 dot1qVlanIndex（即 VLAN ID）为实例索引
        if !(1..=4094).contains(&vlan) {
            continue;
        }
        if let Value::OctetString(bytes) = value {
            for bp in port_list_members(&bytes) {
                if let Some(&if_index) = bp_to_if.get(&bp) {
                    members.entry(if_index).or_default().insert(vlan as i32);
                }
            }
        }
    }
    members
}

pub async fn get_device_ports_via_snmp(
    params: &SnmpParamsLegacy,
) -> Result<Vec<SnmpPort>, SnmpError> {
    let addr = snmp_target(&params.ip, params.port);
    let timeout = Duration::from_secs(params.timeout_secs);

    let auth = build_auth(params).map_err(SnmpError::Message)?;

    let client = Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    // IF-MIB 基础表：名称/管理态/运营态/高速率/速率
    let if_descr = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 2),
        "log.device.snmp.ports_walk_failed",
    )
    .await;
    let if_admin = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 7),
        "log.device.snmp.ports_walk_failed",
    )
    .await;
    let if_oper = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 8),
        "log.device.snmp.ports_walk_failed",
    )
    .await;
    let if_high_speed = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, 15),
        "log.device.snmp.ports_walk_failed",
    )
    .await;
    let if_speed = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 5),
        "log.device.snmp.ports_walk_failed",
    )
    .await;

    // BRIDGE-MIB 桥端口 → ifIndex（VLAN 采集的映射前提，
    // 非交换设备无桥 MIB，相关字段保持 NULL）
    let bp_if_raw = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 17, 1, 4, 1, 2),
        "log.device.snmp.ports_walk_failed",
    )
    .await;
    let bp_to_if: HashMap<u32, u32> = bp_if_raw
        .into_iter()
        .filter_map(|(bp, value)| {
            let if_index = value_as_i32(Some(&value))?;
            u32::try_from(if_index).ok().map(|idx| (bp, idx))
        })
        .collect();

    // Q-BRIDGE-MIB dot1qPvid：桥端口 → 接入 VLAN（PVID）
    let pvid = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 17, 7, 1, 4, 5, 1, 1),
        "log.device.snmp.ports_walk_failed",
    )
    .await;
    let mut pvid_by_if: HashMap<u32, i32> = HashMap::new();
    for (bp, value) in pvid {
        if let (Some(&if_index), Some(vlan)) = (bp_to_if.get(&bp), value_as_i32(Some(&value)))
            && (1..=4094).contains(&vlan)
        {
            pvid_by_if.insert(if_index, vlan);
        }
    }

    let vlan_members = collect_vlan_members(&client, &bp_to_if).await;

    let mut ports = Vec::new();
    for (if_index, value) in if_descr {
        let name = value
            .as_str()
            .map(std::string::ToString::to_string)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| if_index.to_string());

        let status = derive_port_status(
            value_as_i32(if_admin.get(&if_index)),
            value_as_i32(if_oper.get(&if_index)),
        );
        let speed = derive_port_speed(if_high_speed.get(&if_index), if_speed.get(&if_index));
        // VLAN 成员数 >1 判 trunk，=1 判 access；无 VLAN 表（非交换设备）保持 None
        let port_type = match vlan_members.get(&if_index).map(HashSet::len) {
            Some(count) if count > 1 => Some("trunk".to_string()),
            Some(1) => Some("access".to_string()),
            _ => None,
        };

        ports.push(SnmpPort {
            description: Some(name.clone()),
            name,
            port_type,
            vlan_id: pvid_by_if.get(&if_index).copied(),
            status,
            speed,
        });
    }

    debug!("获取接口列表完成: {} 条记录", ports.len());
    Ok(ports)
}
