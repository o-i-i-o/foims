//! CPU 族执行器（docs/SNMP模块重构方案.md §5.6）：值列（percent）与
//! UCD ssCpuRaw 计数器差值（需上一轮状态）。
//!
//! 执行器返回「读数 + 可选行缓存元数据」；FamilySelection（含候选下标）
//! 由驱动层组装，执行器不感知自身下标。

use crate::vendor::model;
use async_snmp::{Oid, Value};

use super::ExecOutput;
use super::transform;
use super::{
    CandidateFailure, ExecArgs, RowMeta, SelectionRows, get_one, get_rows, numeric_u64,
    walk_single_arc,
};
use model::{CandidateKind, Column};

/// UCD-SNMP-MIB ssCpuRaw 计数器（1.3.6.1.4.1.2021.11），标量带 .0 实例后缀；
/// 编号：User(50)/Nice(51)/System(52)/Idle(53)/Wait(54)/Interrupt(56)。
/// SoftIRQ(61)/Steal(64)/Guest(65) 不参与求和（子集/重叠/客态）。
const UCD_CPU_ARCS: [u32; 6] = [50, 51, 52, 53, 54, 56];
/// Idle / Wait（iowait）下标：idle 分量 = Idle + Wait，与 top 口径一致
///（I/O 等待不算 CPU 使用率）。
const UCD_CPU_IDLE_IDX: usize = 3;
const UCD_CPU_WAIT_IDX: usize = 4;

/// UCD 计数器快照：total = 六列求和，idle = Idle + Wait（差值算使用率）。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CpuCounters {
    pub total: u64,
    pub idle: u64,
}

/// Counter32 差值（回绕校正：cur < prev 时按 2^32 计数空间回绕）。
pub fn delta32(prev: u64, cur: u64) -> u64 {
    if cur >= prev {
        cur - prev
    } else {
        cur + (1u64 << 32) - prev
    }
}

/// CPU 使用率（%）：100×(1−Δidle/Δtotal)；Δtotal≤0（时段过短/计数器回绕）
/// 时 None，结果钳制 [0, 100]。
pub fn cpu_usage_pct(prev: CpuCounters, cur: CpuCounters) -> Option<f64> {
    let total = delta32(prev.total, cur.total);
    let idle = delta32(prev.idle, cur.idle);
    if total == 0 {
        return None;
    }
    Some((100.0 * (1.0 - idle as f64 / total as f64)).clamp(0.0, 100.0))
}

pub(super) async fn exec<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
) -> ExecOutput<super::CpuReading> {
    match &args.candidate.kind {
        CandidateKind::Columns(col) => exec_column(args, col).await,
        CandidateKind::UcdCpuCounters => exec_counters(args).await,
        _ => Err(CandidateFailure::Unsupported),
    }
}

/// 值列候选：LibreNMS processor_precision 语义（负值 = 上游上报空闲率）。
/// CPU 列无行名，skip 仅 value 维度；空列返回 Invalid（数值全无效）。
async fn exec_column<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
    col: &Column,
) -> ExecOutput<super::CpuReading> {
    let to_pct = |index: u32, value: Value| -> Option<super::CpuReading> {
        let pct = transform::cpu_pct_value(&value, col.precision.unwrap_or(1))?;
        Some(super::CpuReading {
            index: index.to_string(),
            usage_pct: pct,
        })
    };

    // 定向采样：缓存行索引 GET
    if let Some(selection) = args.selection {
        let values = get_rows(
            args.reader,
            &[col.oid.clone()],
            selection.rows.keys().copied(),
        )
        .await?;
        let readings: Vec<_> = values
            .into_iter()
            .filter_map(|((_, index), value)| to_pct(index, value))
            .collect();
        return Ok((readings, None));
    }

    if col.indexed {
        // 全量发现：walk 列
        let rows = walk_single_arc(args.reader, &col.oid).await;
        if rows.is_empty() {
            return Err(CandidateFailure::Empty);
        }
        let mut readings = Vec::new();
        let mut cached = SelectionRows::new();
        for (index, value) in rows {
            cached.insert(index, RowMeta::Plain);
            if let Some(reading) = to_pct(index, value) {
                readings.push(reading);
            }
        }
        if readings.is_empty() {
            return Err(CandidateFailure::Invalid);
        }
        Ok((readings, Some(cached)))
    } else {
        // 标量（OID 含实例后缀）：单 GET，无需缓存
        let Some(value) = get_one(args.reader, &col.oid).await else {
            return Err(CandidateFailure::Unsupported);
        };
        let Some(reading) = to_pct(0, value) else {
            return Err(CandidateFailure::Invalid);
        };
        Ok((vec![reading], None))
    }
}

/// UCD 计数器候选：GET 六个计数器 → 差值算使用率；新计数器写回缓存
///（趋势路径下一轮 take 消费）。
async fn exec_counters<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
) -> ExecOutput<super::CpuReading> {
    let Some(prev) = args.prev_cpu else {
        return Err(CandidateFailure::Unsupported);
    };
    let base = Oid::new([1, 3, 6, 1, 4, 1, 2021, 11]);
    let oids: Vec<Oid> = UCD_CPU_ARCS
        .iter()
        .map(|arc| {
            let mut arcs = base.arcs().to_vec();
            arcs.push(*arc);
            arcs.push(0);
            Oid::new(arcs)
        })
        .collect();
    let binds = args
        .reader
        .get_many(&oids)
        .await
        .map_err(CandidateFailure::Error)?;
    let mut values = [0u64; 6];
    let mut any = false;
    for (i, vb) in binds.iter().enumerate() {
        if let Some(v) = numeric_u64(&vb.value) {
            values[i] = v;
            any = true;
        }
    }
    if !any {
        return Err(CandidateFailure::Unsupported);
    }
    // net-snmp 实现 UCD 全六列；缺列（SNMPv1 老代理）按 0 参与，差值口径
    // 与历史版本一致；total==0（计数器未变/设备未初始化）由 cpu_usage_pct
    // 的 None 分支兜住
    let total: u64 = values.iter().sum();
    let idle = values[UCD_CPU_IDLE_IDX] + values[UCD_CPU_WAIT_IDX];
    let cur = CpuCounters { total, idle };
    let Some(pct) = cpu_usage_pct(*prev, cur) else {
        return Err(CandidateFailure::Invalid);
    };
    if let Some(cache) = args.cache {
        cache.set_cpu_counters(cur);
    }
    Ok((
        vec![super::CpuReading {
            index: "ucd".to_string(),
            usage_pct: pct,
        }],
        None,
    ))
}
