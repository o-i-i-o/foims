//! 存储族执行器：HOST-RESOURCES hrStorageTable 行（FixedDisk / Ram）。
//! 表列 walk 一次（type/descr/units/size/used），按类型过滤行；units 列
//! 缓存后每轮仅 GET size/used。

use crate::vendor::model;

use async_snmp::{Oid, Value};

use super::{
    CandidateFailure, ExecArgs, ExecOutput, RowMeta, SelectionRows, get_rows, numeric_u64,
    varbind_text, walk_single_arc,
};
use model::{CandidateKind, StorageType};

/// hrStorageTable 列基 OID（1.3.6.1.2.1.25.2.3.1.x）：
/// Index(1)/Type(2)/Descr(3)/AllocationUnits(4)/Size(5)/Used(6)。
const HR_TABLE: [u32; 10] = [1, 3, 6, 1, 2, 1, 25, 2, 3, 1];
const COL_TYPE: usize = 2;
const COL_DESCR: usize = 3;
const COL_UNITS: usize = 4;
const COL_SIZE: usize = 5;
const COL_USED: usize = 6;

fn col_oid(col: usize) -> Oid {
    let mut arcs = HR_TABLE.to_vec();
    arcs.push(col as u32);
    Oid::new(arcs)
}

pub(super) async fn exec<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
) -> ExecOutput<super::MemReading> {
    match &args.candidate.kind {
        CandidateKind::HrStorage { storage_type } => exec_hr_storage(args, *storage_type).await,
        _ => Err(CandidateFailure::Unsupported),
    }
}

/// hrStorage 行采集（mem 族的 Ram 行与 storage 族的 FixedDisk 行共用）。
pub(super) async fn exec_hr_storage<R: crate::core::SnmpReader>(
    args: ExecArgs<'_, '_, R>,
    storage_type: StorageType,
) -> ExecOutput<super::MemReading> {
    let type_value_arcs: [u32; 10] = [1, 3, 6, 1, 2, 1, 25, 2, 1, storage_type.type_arc()];

    // 定向采样：缓存行索引（Units 元数据）→ GET size/used
    if let Some(selection) = args.selection {
        let cols = [col_oid(COL_SIZE), col_oid(COL_USED)];
        let values = get_rows(args.reader, &cols, selection.rows.keys().copied()).await?;
        let mut readings = Vec::new();
        for (index, meta) in selection.rows.iter() {
            let RowMeta::Units(units) = meta else {
                continue;
            };
            let size = values
                .get(&(0, *index))
                .and_then(numeric_u64)
                .map(|v| v.saturating_mul(*units));
            let used = values
                .get(&(1, *index))
                .and_then(numeric_u64)
                .map(|v| v.saturating_mul(*units));
            if let (Some(total), Some(used)) = (size, used)
                && total > 0
            {
                readings.push(super::MemReading {
                    index: index.to_string(),
                    name: None,
                    used: Some(used),
                    free: Some(total.saturating_sub(used)),
                    total: Some(total),
                    usage_percent: Some(100.0 * used as f64 / total as f64),
                });
            }
        }
        return Ok((readings, None));
    }

    // 全量发现：walk 五列（type/descr/units/size/used），按索引 join
    let types = walk_single_arc(args.reader, &col_oid(COL_TYPE)).await;
    if types.is_empty() {
        return Err(CandidateFailure::Empty);
    }
    let descrs = walk_single_arc(args.reader, &col_oid(COL_DESCR)).await;
    let units = walk_single_arc(args.reader, &col_oid(COL_UNITS)).await;
    let sizes = walk_single_arc(args.reader, &col_oid(COL_SIZE)).await;
    let useds = walk_single_arc(args.reader, &col_oid(COL_USED)).await;

    let mut cached = SelectionRows::new();
    let mut readings = Vec::new();
    for (index, type_value) in types {
        // 类型过滤：hrStorageType 值 OID（Ram=…25.2.1.2 / FixedDisk=…25.2.1.4）
        let Value::ObjectIdentifier(oid) = type_value else {
            continue;
        };
        if oid.arcs() != type_value_arcs {
            continue;
        }
        let Some(units) = units.get(&index).and_then(numeric_u64) else {
            continue;
        };
        if units == 0 {
            continue;
        }
        let Some(size_raw) = sizes.get(&index).and_then(numeric_u64) else {
            continue;
        };
        let Some(used_raw) = useds.get(&index).and_then(numeric_u64) else {
            continue;
        };
        let name = descrs
            .get(&index)
            .and_then(|v| varbind_text(&async_snmp::VarBind::new(col_oid(COL_DESCR), v.clone())));
        // skip 过滤（descr 行名）
        if super::transform::skip_hit(&args.candidate.skip, name.as_deref(), None) {
            continue;
        }
        let total = size_raw.saturating_mul(units);
        let used = used_raw.saturating_mul(units);
        if total == 0 {
            continue;
        }
        cached.insert(index, RowMeta::Units(units));
        readings.push(super::MemReading {
            index: index.to_string(),
            name,
            used: Some(used),
            free: Some(total.saturating_sub(used)),
            total: Some(total),
            usage_percent: Some(100.0 * used as f64 / total as f64),
        });
    }
    if readings.is_empty() {
        return Err(CandidateFailure::Invalid);
    }
    Ok((readings, Some(cached)))
}
