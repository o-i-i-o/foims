//! 温度族执行器（docs/SNMP模块重构方案.md §5.6）：值列（divisor/multiplier/
//! unit 换算）与 ENTITY-SENSOR-MIB 标准表（内建吸收）。

use crate::vendor::model;
use async_snmp::Value;
use std::collections::BTreeMap;

use super::{
    CandidateFailure, ExecArgs, ExecOutput, RowMeta, SelectionRows, get_one, get_rows, numeric,
    transform, walk_single_arc,
};
use model::{CandidateKind, Column};

pub(super) async fn exec<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
) -> ExecOutput<super::TempReading> {
    match &args.candidate.kind {
        CandidateKind::Columns(col) => exec_column(args, col).await,
        CandidateKind::EntitySensors => {
            let readings =
                super::sensors::exec_entity_sensors(args, Some(model::SensorKind::Temperature))
                    .await?;
            // 传感器行 → 温度行（entity_sensors 的 temperature 行即温度读数）
            let temps = readings
                .0
                .into_iter()
                .map(|s| super::TempReading {
                    index: s.index,
                    name: s.name,
                    celsius: s.value,
                })
                .collect();
            Ok((temps, readings.1))
        }
        _ => Err(CandidateFailure::Unsupported),
    }
}

/// 值列候选：raw × multiplier / divisor → 单位 → 摄氏度，业务区间 (0, 200]℃。
async fn exec_column<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
    col: &Column,
) -> ExecOutput<super::TempReading> {
    let to_c = |index: u32, value: Value, name: Option<String>| -> Option<super::TempReading> {
        let raw = numeric(&value)?;
        let celsius = transform::to_celsius(raw, col.unit, col.divisor, col.multiplier)?;
        Some(super::TempReading {
            index: index.to_string(),
            name,
            celsius,
        })
    };

    // 定向采样：缓存行索引 GET（列缓存元数据 Plain；行名不重采）
    if let Some(selection) = args.selection {
        let values = get_rows(
            args.reader,
            &[col.oid.clone()],
            selection.rows.keys().copied(),
        )
        .await?;
        let readings: Vec<_> = values
            .into_iter()
            .filter_map(|((_, index), value)| to_c(index, value, None))
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
            // skip 过滤（value 维度，行名缺失）
            if transform::skip_hit(
                &args.candidate.skip,
                None,
                Some(value.to_string()).as_deref(),
            ) {
                continue;
            }
            cached.insert(index, RowMeta::Plain);
            if let Some(reading) = to_c(index, value, None) {
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
        let Some(reading) = to_c(0, value, None) else {
            return Err(CandidateFailure::Invalid);
        };
        Ok((vec![reading], None))
    }
}
