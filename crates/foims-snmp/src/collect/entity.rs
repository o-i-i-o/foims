//! ENTITY-MIB / ENTITY-SENSOR-MIB 采集（物理实体清单 + 传感器读数）。
//!
//! `entPhysicalTable`（1.3.6.1.2.1.47.1.1.1.1）以单一 entPhysicalIndex 为
//! 实例索引，逐列 walk 汇总；`entSensorTable`（1.3.6.1.2.1.99.1.1.1）实例
//! 索引为 (entPhysicalIndex, entSensorIndex) 双弧，取次末弧映射实体、末弧
//! 作传感器实例。两表都以「各列索引并集」驱动遍历：单列缺失只把该字段降级
//! 为 NULL，不丢整行。vendor 差异或缺失列一律降级 NULL/跳过，不阻断同步。

use std::collections::HashMap;
use std::time::Duration;

use async_snmp::{Client, Value, oid};
use tracing::debug;

use foims_models::{EntityRecord, SensorRecord};

use super::ports::{value_as_i32, walk_index_value_map};
use crate::core::{
    SnmpError, SnmpParamsLegacy, build_auth, clean_snmp_text, format_snmp_error, snmp_target,
};

/// entPhysicalClass 数值 → 可读类别名（未知值回退 `class<N>`）。
fn ent_class_name(raw: Option<i32>) -> Option<String> {
    let value = raw?;
    let name = match value {
        1 => "other",
        2 => "unknown",
        3 => "chassis",
        4 => "port",
        5 => "powerSupply",
        6 => "fan",
        7 => "module",
        8 => "card",
        9 => "sensor",
        10 => "tModule",
        11 => "backplane",
        12 => "container",
        _ => return Some(format!("class{value}")),
    };
    Some(name.to_string())
}

/// SNMP TruthValue（1=true/2=false）→ Option<bool>。
fn truth_to_bool(raw: Option<i32>) -> Option<bool> {
    match raw? {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// 取单弧索引列映射里的字符串值（复用 B3 归一：trim/空串/异常字面量 → None）。
fn str_value(map: &HashMap<u32, Value>, index: u32) -> Option<String> {
    clean_snmp_text(map.get(&index).and_then(|v| v.as_str()))
}

/// 采集物理实体清单（类别/名称/型号/序列号/厂商/三级版本/FRU/父实体）。
///
/// 错误仅来自鉴权构造与会话建立；单列 walk 失败只记日志并降级为空值。
pub async fn get_entities_via_snmp(
    params: &SnmpParamsLegacy,
) -> Result<Vec<EntityRecord>, SnmpError> {
    let addr = snmp_target(&params.ip, params.port);
    let timeout = Duration::from_secs(params.timeout_secs);
    let auth = build_auth(params).map_err(SnmpError::Message)?;
    let client = Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    let descr = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 47, 1, 1, 1, 1, 1),
        "log.device.snmp.entity_walk_failed",
    )
    .await;
    let class_map = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 47, 1, 1, 1, 1, 3),
        "log.device.snmp.entity_walk_failed",
    )
    .await;
    let contained = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 47, 1, 1, 1, 1, 4),
        "log.device.snmp.entity_walk_failed",
    )
    .await;
    let name_map = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 47, 1, 1, 1, 1, 5),
        "log.device.snmp.entity_walk_failed",
    )
    .await;
    let hw_rev = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 47, 1, 1, 1, 1, 6),
        "log.device.snmp.entity_walk_failed",
    )
    .await;
    let fw_rev = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 47, 1, 1, 1, 1, 7),
        "log.device.snmp.entity_walk_failed",
    )
    .await;
    let sw_rev = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 47, 1, 1, 1, 1, 8),
        "log.device.snmp.entity_walk_failed",
    )
    .await;
    let serial = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 47, 1, 1, 1, 1, 9),
        "log.device.snmp.entity_walk_failed",
    )
    .await;
    let mfg = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 47, 1, 1, 1, 1, 10),
        "log.device.snmp.entity_walk_failed",
    )
    .await;
    let model = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 47, 1, 1, 1, 1, 11),
        "log.device.snmp.entity_walk_failed",
    )
    .await;
    let is_fru = walk_index_value_map(
        &client,
        oid!(1, 3, 6, 1, 2, 1, 47, 1, 1, 1, 1, 16),
        "log.device.snmp.entity_walk_failed",
    )
    .await;

    let mut entities = Vec::new();
    // 排序去重保证台账输出确定性
    let mut indexes: Vec<u32> = descr
        .keys()
        .chain(name_map.keys())
        .chain(class_map.keys())
        .chain(model.keys())
        .chain(serial.keys())
        .copied()
        .collect();
    indexes.sort_unstable();
    indexes.dedup();
    for index in indexes {
        let index = &index;
        entities.push(EntityRecord {
            ent_index: i32::try_from(*index).unwrap_or(0),
            ent_class: ent_class_name(value_as_i32(class_map.get(index))),
            name: str_value(&name_map, *index),
            descr: str_value(&descr, *index),
            model_name: str_value(&model, *index),
            serial_num: str_value(&serial, *index),
            mfg_name: str_value(&mfg, *index),
            hw_rev: str_value(&hw_rev, *index),
            fw_rev: str_value(&fw_rev, *index),
            sw_rev: str_value(&sw_rev, *index),
            is_fru: truth_to_bool(value_as_i32(is_fru.get(index))),
            parent_index: value_as_i32(contained.get(index)),
        });
    }

    debug!("获取物理实体完成: {} 条记录", entities.len());
    Ok(entities)
}

/// 采集传感器读数（类型/量程标度/精度/当前值/状态，按实体索引归属）。
///
/// 错误仅来自鉴权构造与会话建立；`name` 不在所 walk 的 5 列内，恒为 None。
pub async fn get_sensors_via_snmp(
    params: &SnmpParamsLegacy,
) -> Result<Vec<SensorRecord>, SnmpError> {
    let addr = snmp_target(&params.ip, params.port);
    let timeout = Duration::from_secs(params.timeout_secs);
    let auth = build_auth(params).map_err(SnmpError::Message)?;
    let client = Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    let mut by_key: HashMap<(i32, u32), SensorRecord> = HashMap::new();

    let type_map = walk_sensor_columns(&client, 1).await;
    let scale_map = walk_sensor_columns(&client, 2).await;
    let precision_map = walk_sensor_columns(&client, 3).await;
    let value_map = walk_sensor_columns(&client, 4).await;
    let status_map = walk_sensor_columns(&client, 5).await;

    for key in type_map
        .keys()
        .chain(value_map.keys())
        .chain(scale_map.keys())
        .chain(precision_map.keys())
        .chain(status_map.keys())
        .copied()
    {
        let (ent_index, _) = key;
        by_key.insert(
            key,
            SensorRecord {
                ent_index,
                sensor_type: value_as_i32(type_map.get(&key)),
                sensor_scale: value_as_i32(scale_map.get(&key)),
                sensor_precision: value_as_i32(precision_map.get(&key)),
                sensor_value: value_map.get(&key).and_then(|v| match v {
                    Value::Integer(i) => Some(f64::from(*i)),
                    Value::Gauge32(g) => Some(f64::from(*g)),
                    _ => None,
                }),
                sensor_status: value_as_i32(status_map.get(&key)),
                name: None,
            },
        );
    }

    let sensors: Vec<SensorRecord> = by_key.into_values().collect();
    debug!("获取传感器完成: {} 条记录", sensors.len());
    Ok(sensors)
}

/// walk entSensorTable 单列，返回 `(ent_index, sensor_index) → 值` 映射
///（实例索引双弧：次末弧实体、末弧传感器序号）。
async fn walk_sensor_columns(client: &Client, col: u32) -> HashMap<(i32, u32), async_snmp::Value> {
    let base = match col {
        1 => oid!(1, 3, 6, 1, 2, 1, 99, 1, 1, 1, 1),
        2 => oid!(1, 3, 6, 1, 2, 1, 99, 1, 1, 1, 2),
        3 => oid!(1, 3, 6, 1, 2, 1, 99, 1, 1, 1, 3),
        4 => oid!(1, 3, 6, 1, 2, 1, 99, 1, 1, 1, 4),
        5 => oid!(1, 3, 6, 1, 2, 1, 99, 1, 1, 1, 5),
        // 未知列号直接跳过，不给兜底 OID（否则静默 walk 到错列）
        other => {
            tracing::debug!("未知传感器列号 {other}，跳过");
            return HashMap::new();
        }
    };
    let mut map = HashMap::new();
    let Ok(mut walk) = client.walk(base) else {
        foims_common::log_warn!("log.device.snmp.sensor_walk_failed");
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
        // 前缀 11 弧 + ent_index + sensor_index
        if arcs.len() >= 13 {
            let ent_index = arcs[arcs.len() - 2] as i32;
            let sensor_index = arcs[arcs.len() - 1];
            map.insert((ent_index, sensor_index), vb.value);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ent_class_name_映射已知与未知类别() {
        assert_eq!(ent_class_name(Some(3)).as_deref(), Some("chassis"));
        assert_eq!(ent_class_name(Some(4)).as_deref(), Some("port"));
        assert_eq!(ent_class_name(Some(6)).as_deref(), Some("fan"));
        assert_eq!(ent_class_name(Some(99)).as_deref(), Some("class99"));
        assert_eq!(ent_class_name(None), None);
    }

    #[test]
    fn truth_to_bool_仅识别_1_与_2() {
        assert_eq!(truth_to_bool(Some(1)), Some(true));
        assert_eq!(truth_to_bool(Some(2)), Some(false));
        assert_eq!(truth_to_bool(Some(0)), None);
        assert_eq!(truth_to_bool(Some(3)), None);
        assert_eq!(truth_to_bool(None), None);
    }
}
