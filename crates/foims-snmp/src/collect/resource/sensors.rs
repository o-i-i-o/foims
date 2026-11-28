//! 传感器族执行器：ENTITY-SENSOR-MIB 标准表（内建吸收）与值列候选。

use crate::vendor::model;

use async_snmp::{Oid, Value};

use super::{
    CandidateFailure, ExecArgs, ExecOutput, RowMeta, SelectionRows, get_one, get_rows, numeric,
    transform, varbind_text, walk_single_arc,
};
use model::{CandidateKind, Column, SensorKind};

/// entPhySensorTable 列基 OID（1.3.6.1.2.1.99.7.1.1.1.x）：
/// Type(1)/Scale(2)/Precision(3)/Value(4)/OperStatus(5)/UnitsDisplay(6)。
const SENSOR_TABLE: [u32; 8] = [1, 3, 6, 1, 2, 1, 99, 7];
const COL_TYPE: usize = 1;
const COL_SCALE: usize = 2;
const COL_PRECISION: usize = 3;
const COL_VALUE: usize = 4;
const COL_STATUS: usize = 5;

fn col_oid(col: usize) -> Oid {
    let mut arcs = SENSOR_TABLE.to_vec();
    arcs.push(1);
    arcs.push(col as u32);
    Oid::new(arcs)
}

/// entPhySensorType 枚举（IANA-ENTITY-MIB）→ 传感器类型映射。
fn sensor_kind_of(type_value: i32) -> Option<SensorKind> {
    Some(match type_value {
        3 => SensorKind::Voltage,     // volts
        4 => SensorKind::Current,     // amperes
        5 => SensorKind::Power,       // watts
        6 => SensorKind::Frequency,   // hertz
        7 => SensorKind::Temperature, // celsius
        8 => SensorKind::Percent,     // percentRelativeHumidity
        9 => SensorKind::Fan,         // rpm
        _ => return None,
    })
}

pub(super) async fn exec<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
) -> ExecOutput<super::SensorReading> {
    match &args.candidate.kind {
        CandidateKind::EntitySensors => {
            let filter = args.candidate.sensor;
            exec_entity_sensors(args, filter).await
        }
        CandidateKind::Columns(col) => exec_column(args, col).await,
        _ => Err(CandidateFailure::Unsupported),
    }
}

/// ENTITY-SENSOR 标准表：walk Type/Scale/Precision/Value/Status 五列，
/// Status=ok(1) 行换算 `raw × 10^scale / 10^precision`。
/// `filter` 为 Some 时只保留该类型（温度族复用）；None = 全类型。
pub(super) async fn exec_entity_sensors<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
    filter: Option<SensorKind>,
) -> ExecOutput<super::SensorReading> {
    // 定向采样：缓存行（scale/precision 元数据）→ GET value
    if let Some(selection) = args.selection {
        let values = get_rows(
            args.reader,
            &[col_oid(COL_VALUE)],
            selection.rows.keys().copied(),
        )
        .await?;
        let mut readings = Vec::new();
        for ((_, index), value) in values {
            let Some(RowMeta::Sensor { scale, precision }) = selection.rows.get(&index) else {
                continue;
            };
            if let Some(reading) = sensor_reading(
                index,
                None,
                SensorKind::Temperature,
                &value,
                *scale,
                *precision,
            ) {
                readings.push(reading);
            }
        }
        return Ok((readings, None));
    }

    let types = walk_single_arc(args.reader, &col_oid(COL_TYPE)).await;
    if types.is_empty() {
        return Err(CandidateFailure::Empty);
    }
    let scales = walk_single_arc(args.reader, &col_oid(COL_SCALE)).await;
    let precisions = walk_single_arc(args.reader, &col_oid(COL_PRECISION)).await;
    let values = walk_single_arc(args.reader, &col_oid(COL_VALUE)).await;
    let statuses = walk_single_arc(args.reader, &col_oid(COL_STATUS)).await;

    let mut cached = SelectionRows::new();
    let mut readings = Vec::new();
    for (index, type_value) in types {
        // 类型过滤 + 状态过滤（ok=1）
        let Some(kind_value) = type_value.as_i32() else {
            continue;
        };
        let Some(kind) = sensor_kind_of(kind_value) else {
            continue;
        };
        if let Some(filter) = filter
            && kind != filter
        {
            continue;
        }
        if statuses
            .get(&index)
            .and_then(|v| v.as_i32())
            .is_none_or(|status| status != 1)
        {
            continue;
        }
        let Some(value) = values.get(&index) else {
            continue;
        };
        let scale = scales.get(&index).and_then(|v| v.as_i32()).unwrap_or(9);
        let precision = precisions.get(&index).and_then(|v| v.as_i32()).unwrap_or(0);
        cached.insert(index, RowMeta::Sensor { scale, precision });
        if let Some(reading) = sensor_reading(index, None, kind, value, scale, precision) {
            readings.push(reading);
        }
    }
    if readings.is_empty() {
        return Err(CandidateFailure::Invalid);
    }
    Ok((readings, Some(cached)))
}

/// 换算单行：`raw × 10^scale / 10^precision`；状态类保持原值。
fn sensor_reading(
    index: u32,
    name: Option<String>,
    kind: SensorKind,
    value: &Value,
    scale: i32,
    precision: i32,
) -> Option<super::SensorReading> {
    let raw = numeric(value)?;
    let scale = transform::entity_scale_factor(scale).unwrap_or(1.0);
    let precision = 10f64.powi(-precision.clamp(-9, 9));
    let value = raw * scale * precision;
    let value = if kind == SensorKind::Temperature {
        transform::to_celsius(value, None, 1, 1)?
    } else if kind == SensorKind::Status {
        // 状态类不换算，走 states 映射在调用方
        raw
    } else {
        transform::sensor_value(value, None)?
    };
    Some(super::SensorReading {
        index: index.to_string(),
        kind,
        name,
        value,
    })
}

/// 值列传感器候选：sensor 类型标记 + 单位换算 + 状态映射。
async fn exec_column<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
    col: &Column,
) -> ExecOutput<super::SensorReading> {
    let Some(kind) = args.candidate.sensor else {
        return Err(CandidateFailure::Unsupported);
    };
    let to_reading = |index: u32, value: Value| -> Option<super::SensorReading> {
        if kind == SensorKind::Status {
            // 状态类：states 数值映射（未命中回退原值十进制）
            let raw = numeric(&value)?;
            let text = match value {
                Value::OctetString(_) => {
                    varbind_text(&async_snmp::VarBind::new(col.oid.clone(), value.clone()))?
                }
                _ => transform::map_state(&args.candidate.states, raw as i64),
            };
            let value = text.parse::<f64>().unwrap_or(0.0);
            return Some(super::SensorReading {
                index: index.to_string(),
                kind,
                name: None,
                value,
            });
        }
        let raw = numeric(&value)?;
        let value =
            transform::sensor_value(raw * col.multiplier as f64 / col.divisor as f64, col.unit)?;
        Some(super::SensorReading {
            index: index.to_string(),
            kind,
            name: None,
            value,
        })
    };

    if let Some(selection) = args.selection {
        let values = get_rows(
            args.reader,
            std::slice::from_ref(&col.oid),
            selection.rows.keys().copied(),
        )
        .await?;
        let readings: Vec<_> = values
            .into_iter()
            .filter_map(|((_, index), value)| to_reading(index, value))
            .collect();
        return Ok((readings, None));
    }

    if col.indexed {
        let rows = walk_single_arc(args.reader, &col.oid).await;
        if rows.is_empty() {
            return Err(CandidateFailure::Empty);
        }
        let mut readings = Vec::new();
        let mut cached = SelectionRows::new();
        for (index, value) in rows {
            cached.insert(index, RowMeta::Plain);
            if let Some(reading) = to_reading(index, value) {
                readings.push(reading);
            }
        }
        if readings.is_empty() {
            return Err(CandidateFailure::Invalid);
        }
        Ok((readings, Some(cached)))
    } else {
        let Some(value) = get_one(args.reader, &col.oid).await else {
            return Err(CandidateFailure::Unsupported);
        };
        let Some(reading) = to_reading(0, value) else {
            return Err(CandidateFailure::Invalid);
        };
        Ok((vec![reading], None))
    }
}
