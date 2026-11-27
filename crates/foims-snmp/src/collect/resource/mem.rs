//! 内存族执行器（docs/SNMP模块重构方案.md §5.6）：池角色列（used+free /
//! total−free / total−used）、实体表（usage×size）、百分比（+total 缓存）、
//! hrStorage Ram 行兜底。

use crate::vendor::model;
use std::collections::BTreeMap;

use async_snmp::Value;

use super::{
    CandidateFailure, ExecArgs, ExecOutput, RowMeta, SelectionRows, get_one, get_rows, numeric_u64,
    transform, varbind_text, walk_single_arc,
};
use model::{CandidateKind, Column, PoolFields, StorageType};

pub(super) async fn exec<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
) -> ExecOutput<super::MemReading> {
    match &args.candidate.kind {
        CandidateKind::Pool(_) => exec_pool(args).await,
        CandidateKind::Entity(_) => exec_entity(args).await,
        CandidateKind::Percent(_) => exec_percent(args).await,
        CandidateKind::HrStorage { storage_type } => {
            if *storage_type == StorageType::Ram {
                super::storage::exec_hr_storage(args, *storage_type).await
            } else {
                Err(CandidateFailure::Unsupported)
            }
        }
        _ => Err(CandidateFailure::Unsupported),
    }
}

/// 池候选：name/used/free/total 角色列按索引对齐，语义按出现角色推导
///（used+free → total=used+free；total+free → used=total−free；
/// total+used → free=total−used）。行选择：skip 过滤后优先含
/// "processor" 的行，否则首行（CISCO 池口径）。
async fn exec_pool<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
) -> ExecOutput<super::MemReading> {
    let CandidateKind::Pool(fields) = &args.candidate.kind else {
        return Err(CandidateFailure::Unsupported);
    };
    let roles: [(&str, &Option<Column>); 4] = [
        ("name", &fields.name),
        ("used", &fields.used),
        ("free", &fields.free),
        ("total", &fields.total),
    ];
    let value_cols: Vec<&Column> = [&fields.used, &fields.free, &fields.total]
        .into_iter()
        .flatten()
        .collect();
    let name_col = fields.name.as_ref();

    // 定向采样：对已选行 GET 值列（表列取子实例，标量列直取）
    if let Some(selection) = args.selection {
        let index = selection.rows.keys().copied().next();
        let Some(index) = index else {
            return Err(CandidateFailure::Empty);
        };
        let mut values: Vec<Option<Value>> = Vec::new();
        for col in &value_cols {
            let oid = if col.indexed {
                super::child_oid(&col.oid, index)
            } else {
                col.oid.clone()
            };
            values.push(get_one(args.reader, &oid).await);
        }
        return match compute_pool(&roles, &value_cols, &values) {
            Some(reading) => Ok((vec![reading], None)),
            None => Err(CandidateFailure::Empty),
        };
    }

    // 全量发现：walk 全部表列 + 标量列直取，按索引对齐
    if value_cols.iter().all(|c| !c.indexed) && name_col.is_none_or(|c| !c.indexed) {
        // 标量池：无行选择、无缓存
        let mut values = Vec::new();
        for col in &value_cols {
            values.push(get_one(args.reader, &col.oid).await);
        }
        return match compute_pool(&roles, &value_cols, &values) {
            Some(mut reading) => {
                reading.index = "scalar".to_string();
                Ok((vec![reading], None))
            }
            None => Err(CandidateFailure::Invalid),
        };
    }
    let mut maps: Vec<(&str, BTreeMap<u32, Value>)> = Vec::new();
    let mut scalars: Vec<(&str, Value)> = Vec::new();
    let mut any_indexed = false;
    for (role, col) in roles
        .iter()
        .filter_map(|(r, c)| c.as_ref().map(|col| (*r, col)))
    {
        if col.indexed {
            let rows = walk_single_arc(args.reader, &col.oid).await;
            any_indexed = any_indexed || !rows.is_empty();
            maps.push((role, rows));
        } else if let Some(value) = get_one(args.reader, &col.oid).await {
            scalars.push((role, value));
        }
    }
    if !any_indexed {
        return Err(CandidateFailure::Empty);
    }
    // 行索引并集
    let mut indexes: Vec<u32> = Vec::new();
    for (_, rows) in &maps {
        indexes.extend(rows.keys().copied());
    }
    indexes.sort_unstable();
    indexes.dedup();

    let mut chosen: Option<(u32, super::MemReading)> = None;
    for index in indexes {
        let mut values: Vec<Option<Value>> = Vec::new();
        let mut name: Option<String> = None;
        for (role, col) in roles
            .iter()
            .filter_map(|(r, c)| c.as_ref().map(|col| (*r, col)))
        {
            let value = if col.indexed {
                maps.iter()
                    .find(|(r, _)| *r == role)
                    .and_then(|(_, rows)| rows.get(&index))
                    .cloned()
            } else {
                scalars
                    .iter()
                    .find(|(r, _)| *r == role)
                    .map(|(_, v)| v.clone())
            };
            if role == "name" {
                name = value.as_ref().and_then(|v| {
                    varbind_text(&async_snmp::VarBind::new(col.oid.clone(), v.clone()))
                });
            }
            values.push(value);
        }
        // skip 过滤（行名/原始值）
        let raw_text = values
            .get(1)
            .and_then(|v| v.as_ref())
            .map(|v| v.to_string());
        if transform::skip_hit(&args.candidate.skip, name.as_deref(), raw_text.as_deref()) {
            continue;
        }
        let Some(mut reading) = compute_pool(&roles, &value_cols, &values) else {
            continue;
        };
        reading.name = name.clone();
        // processor 行优先（Cisco 池多行时取 Processor 池，否则首行）
        if name
            .as_deref()
            .is_some_and(|n| n.to_lowercase().contains("processor"))
        {
            return Ok((
                vec![reading],
                Some(SelectionRows::from([(index, RowMeta::Plain)])),
            ));
        }
        if chosen.is_none() {
            chosen = Some((index, reading));
        }
    }
    match chosen {
        Some((index, reading)) => Ok((
            vec![reading],
            Some(SelectionRows::from([(index, RowMeta::Plain)])),
        )),
        None => Err(CandidateFailure::Invalid),
    }
}

/// 由角色值合成池读数（发现与定向采样共用）：值序与 `value_cols` 一致。
fn compute_pool(
    roles: &[(&str, &Option<Column>); 4],
    value_cols: &[&Column],
    values: &[Option<Value>],
) -> Option<super::MemReading> {
    let get = |role: &str| -> Option<u64> {
        let pos = roles.iter().position(|(r, _)| *r == role)?;
        let value = values.get(pos)?.as_ref()?;
        let col = roles[pos].1.as_ref()?;
        Some(transform::to_bytes(
            numeric_u64(value)?,
            col.unit,
            col.precision,
        ))
    };
    let used = get("used");
    let free = get("free");
    let total = get("total");
    let (used, free, total) = match (used, free, total) {
        (Some(u), Some(f), None) => (u, f, u.checked_add(f)?),
        (None, Some(f), Some(t)) => (t.checked_sub(f)?, f, t),
        (Some(u), None, Some(t)) => (u, t.checked_sub(u)?, t),
        _ => return None,
    };
    if total == 0 {
        return None;
    }
    Some(super::MemReading {
        index: String::new(),
        name: None,
        used: Some(used),
        free: Some(free),
        total: Some(total),
        usage_percent: Some(100.0 * used as f64 / total as f64),
    })
}

/// 实体候选：usage（0-100）+ size（MB 优先、字节兜底），多行求和
///（多 DIMM 累计口径）。
async fn exec_entity<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
) -> ExecOutput<super::MemReading> {
    let CandidateKind::Entity(fields) = &args.candidate.kind else {
        return Err(CandidateFailure::Unsupported);
    };
    // 定向采样：usage 列 + 缓存 total
    if let Some(selection) = args.selection {
        let values = get_rows(
            args.reader,
            &[fields.usage.oid.clone()],
            selection.rows.keys().copied(),
        )
        .await?;
        let mut total = 0u64;
        let mut used = 0u64;
        for ((_, index), value) in values {
            let Some(RowMeta::Total(bytes)) = selection.rows.get(&index) else {
                continue;
            };
            let Some(pct) = value.as_i32().map(f64::from) else {
                continue;
            };
            if !(0.0..=100.0).contains(&pct) {
                continue;
            }
            total += bytes;
            used += (pct / 100.0 * *bytes as f64) as u64;
        }
        return finish_sum(total, used);
    }

    // 全量发现：walk usage + size 列，join 后过滤
    let usage_rows = walk_single_arc(args.reader, &fields.usage.oid).await;
    if usage_rows.is_empty() {
        return Err(CandidateFailure::Empty);
    }
    let (size_col, size_unit) = match (fields.size_mega.as_ref(), fields.size_bytes.as_ref()) {
        (Some(c), _) => (c, 1024u64 * 1024),
        (None, Some(c)) => (c, 1),
        (None, None) => return Err(CandidateFailure::Unsupported),
    };
    let sizes = if size_col.indexed {
        walk_single_arc(args.reader, &size_col.oid).await
    } else {
        let mut map = BTreeMap::new();
        if let Some(value) = get_one(args.reader, &size_col.oid).await {
            map.insert(0u32, value);
        }
        map
    };
    let mut cached = SelectionRows::new();
    let mut total = 0u64;
    let mut used = 0u64;
    for (index, value) in usage_rows {
        let Some(pct) = value.as_i32().map(f64::from) else {
            continue;
        };
        if !(0.0..=100.0).contains(&pct) {
            continue;
        }
        let Some(size_raw) = sizes.get(&index).and_then(numeric_u64) else {
            continue;
        };
        let bytes = size_raw.saturating_mul(size_unit);
        if bytes == 0 {
            continue;
        }
        cached.insert(index, RowMeta::Total(bytes));
        total += bytes;
        used += (pct / 100.0 * bytes as f64) as u64;
    }
    if total == 0 {
        return Err(CandidateFailure::Invalid);
    }
    Ok((vec![summed(total, used)], Some(cached)))
}

/// 百分比候选：percent（0-100）+ 可选 total（缓存），多行求和。
async fn exec_percent<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
) -> ExecOutput<super::MemReading> {
    let CandidateKind::Percent(fields) = &args.candidate.kind else {
        return Err(CandidateFailure::Unsupported);
    };
    // 定向采样
    if let Some(selection) = args.selection {
        let values = get_rows(
            args.reader,
            &[fields.percent.oid.clone()],
            selection.rows.keys().copied(),
        )
        .await?;
        let mut total = 0u64;
        let mut used = 0u64;
        for ((_, index), value) in values {
            let Some(RowMeta::Total(bytes)) = selection.rows.get(&index) else {
                continue;
            };
            let Some(pct) = value.as_i32().map(f64::from) else {
                continue;
            };
            if !(0.0..=100.0).contains(&pct) {
                continue;
            }
            total += bytes;
            used += (pct / 100.0 * *bytes as f64) as u64;
        }
        return finish_sum(total, used);
    }

    // 全量发现：walk percent 列 + total 列（表列 walk、标量 GET 共用索引 0）
    let percent_rows = walk_single_arc(args.reader, &fields.percent.oid).await;
    if percent_rows.is_empty() {
        return Err(CandidateFailure::Empty);
    }
    let Some(total_col) = fields.total.as_ref() else {
        return Err(CandidateFailure::Unsupported);
    };
    let totals = if total_col.indexed {
        walk_single_arc(args.reader, &total_col.oid).await
    } else {
        let mut map = BTreeMap::new();
        if let Some(value) = get_one(args.reader, &total_col.oid).await {
            map.insert(0u32, value);
        }
        map
    };
    let mut cached = SelectionRows::new();
    let mut sum_total = 0u64;
    let mut sum_used = 0u64;
    for (index, value) in percent_rows {
        let Some(pct) = value.as_i32().map(f64::from) else {
            continue;
        };
        if !(0.0..=100.0).contains(&pct) {
            continue;
        }
        let Some(raw) = totals.get(&index).and_then(numeric_u64) else {
            continue;
        };
        let bytes = transform::to_bytes(raw, total_col.unit, total_col.precision);
        if bytes == 0 {
            continue;
        }
        cached.insert(index, RowMeta::Total(bytes));
        sum_total += bytes;
        sum_used += (pct / 100.0 * bytes as f64) as u64;
    }
    if sum_total == 0 {
        return Err(CandidateFailure::Invalid);
    }
    Ok((vec![summed(sum_total, sum_used)], Some(cached)))
}

fn summed(total: u64, used: u64) -> super::MemReading {
    super::MemReading {
        index: String::new(),
        name: None,
        used: Some(used),
        free: Some(total.saturating_sub(used)),
        total: Some(total),
        usage_percent: Some(100.0 * used as f64 / total as f64),
    }
}

fn finish_sum(total: u64, used: u64) -> ExecOutput<super::MemReading> {
    if total == 0 {
        return Err(CandidateFailure::Empty);
    }
    Ok((vec![summed(total, used)], None))
}
