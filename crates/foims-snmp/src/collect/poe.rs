//! PFN-MIB PoE 供电端口采集（pethPsePortTable）与供电中断事件比对。
//!
//! pethPsePortTable（1.3.6.1.2.1.105.1.3.1）实例索引为
//! (pethPsePortGroupIndex, pethPsePortIndex) 双弧，`pethPsePortIfIndex`(.7)
//! 关联真实 ifIndex，端口名经 ifDescr 表映射。实测功率 best-effort 取 Cisco
//! `cpeExtPsePortPwrConsumption`（CISCO-PHYSICAL-ADDTL-STATS-MIB，0.1W→W
//! 换算），缺失 NULL。
//!
//! 供电中断 diff 与第一层 ARP 同构：某端口 `detectionStatus` 由 delivering(3)
//! 转入 disabled(1)/fault(4)/test(5)/other(6) 之一即产出 `poe_port_off`(warning)。

use std::collections::HashMap;
use std::time::Duration;

use async_snmp::{Client, Value, oid};
use tracing::debug;

use foims_common::log_warn;
use foims_models::{DevicePoePort, PoeRecord, SNMP_EVENT_POE_OFF, SNMP_SEVERITY_WARNING};

use super::ports::value_as_i32;
use crate::core::{
    SnmpError, SnmpParamsLegacy, build_auth, clean_snmp_text, format_snmp_error, snmp_target,
};
use crate::events::ObsEvent;

/// PoE 检测态：deliveringPower(3)。
const PSE_DELIVERING: i32 = 3;

/// walk pethPsePortTable 单列，返回 `(group, port) → 值` 映射。
async fn walk_poe_column(client: &Client, col: u32) -> HashMap<(u32, u32), Value> {
    let base = match col {
        2 => oid!(1, 3, 6, 1, 2, 1, 105, 1, 3, 1, 2),
        5 => oid!(1, 3, 6, 1, 2, 1, 105, 1, 3, 1, 5),
        6 => oid!(1, 3, 6, 1, 2, 1, 105, 1, 3, 1, 6),
        7 => oid!(1, 3, 6, 1, 2, 1, 105, 1, 3, 1, 7),
        // C2：越界列号显式拒绝，禁止兜底分支静默 walk 错列
        other => {
            tracing::debug!("未知 PoE 列号 {other}，跳过");
            return HashMap::new();
        }
    };
    let mut map = HashMap::new();
    let Ok(mut walk) = client.walk(base) else {
        log_warn!("log.device.snmp.poe_walk_failed");
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
        // 前缀 10 弧 + group + port
        if arcs.len() >= 12 {
            let group = arcs[arcs.len() - 2];
            let port = arcs[arcs.len() - 1];
            map.insert((group, port), vb.value.clone());
        }
    }
    map
}

fn i32_at(map: &HashMap<(u32, u32), Value>, key: &(u32, u32)) -> Option<i32> {
    map.get(key).and_then(|v| value_as_i32(Some(v)))
}

/// walk cpeExtPsePortPwrConsumption（CISCO-PHYSICAL-ADDTL-STATS-MIB
/// 1.3.6.1.4.1.9.9.16.1.1.3.1.4，单弧 ifIndex 索引，单位 0.1W）。
/// B6：既有注释宣称实测功率 best-effort，此前实现缺失（硬编码
/// None）；非 Cisco 设备不实现该表，walk 失败记告警返回空集（NULL）。
async fn walk_poe_power_consumption(client: &Client) -> HashMap<u32, f64> {
    let mut map = HashMap::new();
    let Ok(mut walk) = client.walk(oid!(1, 3, 6, 1, 4, 1, 9, 9, 16, 1, 1, 3, 1, 4)) else {
        log_warn!("log.device.snmp.poe_walk_failed");
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
        if let (Some(&if_index), Some(raw)) = (vb.oid.arcs().last(), value_as_i32(Some(&vb.value)))
        {
            // 0.1W → W 换算
            map.insert(if_index, f64::from(raw) / 10.0);
        }
    }
    map
}

pub async fn get_poe_ports_via_snmp(
    params: &SnmpParamsLegacy,
) -> Result<Vec<PoeRecord>, SnmpError> {
    let addr = snmp_target(&params.ip, params.port);
    let timeout = Duration::from_secs(params.timeout_secs);
    let auth = build_auth(params).map_err(SnmpError::Message)?;
    let client = Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    let if_descr = super::ports::walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 2),
        "log.device.snmp.poe_walk_failed",
    )
    .await;

    let admin = walk_poe_column(&client, 2).await;
    let detection = walk_poe_column(&client, 5).await;
    let class_map = walk_poe_column(&client, 6).await;
    let if_index_map = walk_poe_column(&client, 7).await;
    let power_map = walk_poe_power_consumption(&client).await;

    // 端口名与实测功率均按 ifIndex 对齐（B6：cpdExt 实测功率
    // 非 Cisco 缺失时 NULL，不阻断其余字段）
    let mut keys: Vec<&(u32, u32)> = if_index_map.keys().collect();
    keys.sort_unstable();
    let mut ports = Vec::new();
    for key in keys {
        let Some(if_index) = value_as_i32(if_index_map.get(key)) else {
            continue;
        };
        let if_name = clean_snmp_text(if_descr.get(&(if_index as u32)).and_then(|v| v.as_str()));
        let measured_power_w = if if_index >= 0 {
            power_map.get(&(if_index as u32)).copied()
        } else {
            None
        };
        ports.push(PoeRecord {
            if_index,
            if_name,
            admin_status: i32_at(&admin, key),
            detection_status: i32_at(&detection, key),
            power_class: i32_at(&class_map, key),
            measured_power_w,
        });
    }

    debug!("获取 PoE 端口完成: {} 条记录", ports.len());
    Ok(ports)
}

/// 比对上一轮与本轮 PoE 端口，检出供电中断（`delivering(3)` 转入非供电态）。
///
/// 纯函数：`prev` 为设备现有 device_poe_ports 行，`cur` 为本轮采集记录，按
/// `if_index` 对齐；首轮基线（prev 空）不发事件。detail 含端口名/类别/前后状态。
#[must_use]
pub fn diff_poe_power(prev: &[DevicePoePort], cur: &[PoeRecord]) -> Vec<ObsEvent> {
    let prev_by_if: HashMap<i32, Option<i32>> = prev
        .iter()
        .map(|r| (r.if_index, r.detection_status))
        .collect();

    let mut events = Vec::new();
    for r in cur {
        // 仅"上一轮在供电、本轮掉电"才告警；searching(2) 视为过渡态不告警
        let Some(PSE_DELIVERING) = prev_by_if.get(&r.if_index).copied().flatten() else {
            continue;
        };
        if let Some(status) = r.detection_status
            && status != PSE_DELIVERING
            && status != 2
        {
            events.push(ObsEvent::new(
                SNMP_EVENT_POE_OFF,
                r.if_name.clone(),
                SNMP_SEVERITY_WARNING,
                serde_json::json!({
                    "interface": r.if_name,
                    "power_class": r.power_class,
                    "from": "delivering",
                    "to": status,
                }),
            ));
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn cur(if_index: i32, status: i32) -> PoeRecord {
        PoeRecord {
            if_index,
            if_name: Some(format!("Gi0/{if_index}")),
            admin_status: Some(1),
            detection_status: Some(status),
            power_class: Some(2),
            measured_power_w: None,
        }
    }

    fn prev(if_index: i32, status: i32) -> DevicePoePort {
        DevicePoePort {
            id: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            if_index,
            if_name: Some(format!("Gi0/{if_index}")),
            admin_status: Some(1),
            detection_status: Some(status),
            power_class: Some(2),
            measured_power_w: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn baseline_no_events() {
        // 首轮无 prev：即便本轮有掉电端口也不告警（无参照）
        let events = diff_poe_power(&[], &[cur(1, 4)]);
        assert!(events.is_empty(), "首轮基线不发供电中断事件");
    }

    #[test]
    fn delivering_to_off_alerts() {
        let prev_rows = vec![prev(1, PSE_DELIVERING)];
        // 端口 1 由 delivering 转 fault(4) → 告警
        let events = diff_poe_power(&prev_rows, &[cur(1, 4)]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, SNMP_EVENT_POE_OFF);
        assert_eq!(events[0].severity, "warning");
    }

    #[test]
    fn still_delivering_no_alert() {
        let prev_rows = vec![prev(1, PSE_DELIVERING)];
        assert!(diff_poe_power(&prev_rows, &[cur(1, PSE_DELIVERING)]).is_empty());
    }

    #[test]
    fn searching_transition_not_alerted() {
        // delivering → searching(2) 过渡态不告警
        let prev_rows = vec![prev(1, PSE_DELIVERING)];
        assert!(diff_poe_power(&prev_rows, &[cur(1, 2)]).is_empty());
    }

    #[test]
    fn was_off_now_off_no_alert() {
        // 上轮就非供电(4)，本轮仍非供电：无"掉电"跳变，不告警
        let prev_rows = vec![prev(1, 4)];
        assert!(diff_poe_power(&prev_rows, &[cur(1, 4)]).is_empty());
    }
}
