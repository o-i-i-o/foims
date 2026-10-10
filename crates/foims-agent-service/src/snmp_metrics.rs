//! SNMP 性能指标采集（设计 docs/agent-design.md §5.2.1「二期：性能指标与流量曲线」）。
//!
//! 在一期系统组（sysName/sysDescr/sysUpTime）之上扩展 CPU/内存/磁盘/温度/
//! 流量/负载采集，全部基于 net-snmp 约定 OID：
//! - CPU：UCD-SNMP-MIB ssCpuRaw* 计数器（11 个 Counter32）差值算使用率
//!   （net-snmp 5.9 已移除 ssCpu 百分比标量，无法直接 GET）；
//! - 内存/磁盘：HOST-RESOURCES-MIB hrStorageTable（Ram 行 + "Available
//!   memory" 行 / FixedDisk 行），内存两行不全时回落 UCD memTotalReal/memAvailReal；
//! - 温度：LM-SENSORS-MIB lmTempSensorsTable（毫度 ℃，/1000 归一）；
//! - 流量：IF-MIB ifTable + ifXTable（64 位 HC 计数器优先，回落 32 位），
//!   Counter32 回绕校正后按 Δbytes×8/elapsed 算速率；
//! - 负载：UCD laLoad.1/2/3（字符串形式浮点）。
//!
//! 计数器差值依赖上一轮状态：调用方自 agents.raw_metrics 私有键
//! `_snmp_state` 读取 [`SnmpState`]，采集完成后新状态随 raw_metrics 写回
//! （差值算不出时状态仍更新，保证下一轮可用）。
//!
//! 单轮失败容忍：任一指标类采集/解析失败仅该类为空，不影响其他类；
//! agents 热列由 upsert 的 COALESCE 保留上一轮旧值。

use std::collections::BTreeMap;

use async_snmp::value::Value as SnmpValue;
use async_snmp::{Client, oid, transport::UdpHandle};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// 流量速率合理性兜底上限（bit/s）：ifSpeed 未知/过低（虚拟口）时的
/// 校验基准，对应 100 Gbps
const DEFAULT_SPEED_CAP_BPS: u64 = 100_000_000_000;
/// ifSpeed 视为"已知"的下限（bit/s）：低于该值（如虚拟口 10 bit/s）不参与
/// 速率合理性校验，避免把正常速率误杀
const MIN_KNOWN_SPEED_BPS: u64 = 1_000_000;
/// 速率计算所需的最小轮询间隔（秒）：间隔过短时差值抖动过大，不出速率
/// （状态仍更新）
const MIN_RATE_ELAPSED_SECS: i64 = 30;
/// 网卡上报条数上限：按接口名排序后截断，防异常设备返回巨量虚拟接口
const MAX_NET_IFACES: usize = 24;
/// 报文聚合请求的 max-repetitions（bulk walk 单次批量大小）
const WALK_MAX_REPETITIONS: u32 = 20;

// OID 构造（oid! 宏展开为运行期 from_slice，非常量）：UCD-SNMP-MIB
// CPU 计数器 ssCpuRaw*.50~60
fn cpu_raw_oids() -> [async_snmp::Oid; 11] {
    [
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 50), // User
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 51), // Nice
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 52), // System
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 53), // Idle
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 54), // Wait
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 55), // Kernel
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 56), // Interrupt
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 57), // SoftIrq
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 58), // Steal
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 59), // Guest
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 60), // GuestNice
    ]
}

/// CPU Raw 计数器中 Idle 的下标（usage 分子）
const CPU_RAW_IDLE_IDX: usize = 3;

/// hrStorageType 取值：Ram（物理内存行）
fn storage_type_ram() -> async_snmp::Oid {
    oid!(1, 3, 6, 1, 2, 1, 25, 2, 1, 2)
}

/// hrStorageType 取值：FixedDisk（本地固定磁盘行）
fn storage_type_fixed_disk() -> async_snmp::Oid {
    oid!(1, 3, 6, 1, 2, 1, 25, 2, 1, 4)
}
/// net-snmp hrStorageTable 中"物理内存"行的描述串（类型匹配失败时兜底）
const STORAGE_DESCR_PHYS_MEM: &str = "Physical memory";
/// net-snmp hrStorageTable 中"可用内存"行的描述串（精确匹配）
const STORAGE_DESCR_AVAIL_MEM: &str = "Available memory";

// ============================================================
// 上一轮状态（raw_metrics 私有键 _snmp_state）
// ============================================================

/// CPU 计数器快照：ssCpuRaw* 全量求和 + idle 分量（差值算使用率）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CpuCounters {
    pub total: u64,
    pub idle: u64,
}

/// 单网卡收发字节计数器快照（差值算速率）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct IfaceCounters {
    pub in_octets: u64,
    pub out_octets: u64,
}

/// 上一轮 SNMP 采集状态（agents.raw_metrics 私有键 `_snmp_state`）。
///
/// 字段全部 `default`：旧版本 raw_metrics 无此键、或历史数据缺字段时
/// 反序列化降级为 None，首轮不出差值指标。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SnmpState {
    /// 采集时刻（unix 秒，差值时段分母）
    pub ts: i64,
    #[serde(default)]
    pub cpu: Option<CpuCounters>,
    #[serde(default)]
    pub ifaces: BTreeMap<String, IfaceCounters>,
}

// ============================================================
// 采集结果
// ============================================================

/// 单网卡速率结果（bit/s；差值不可得时为 None，与 agent 上报口径一致）
pub struct NetRate {
    pub iface: String,
    pub rx_bps: Option<f64>,
    pub tx_bps: Option<f64>,
}

/// 单磁盘/文件系统条目（字节；SNMP 无 IOPS/繁忙度）
pub struct DiskRow {
    pub device: String,
    pub mount: String,
    pub total: u64,
    pub used: u64,
}

/// 单温度传感器读数（℃，归一后业务区间内）
pub struct SensorRow {
    pub label: String,
    pub value: f64,
}

/// 单轮 SNMP 指标采集结果：上报 JSON 明细 + 热列输入 + 写回状态
pub struct SnmpSnapshot {
    pub cpu_usage_pct: Option<f64>,
    pub load: (Option<f64>, Option<f64>, Option<f64>),
    /// 内存（字节，used = total − available）
    pub mem_total: Option<u64>,
    pub mem_used: Option<u64>,
    /// 交换分区（字节；设备未配置/不支持时为 None）
    pub swap_total: Option<u64>,
    pub swap_used: Option<u64>,
    pub disks: Vec<DiskRow>,
    pub sensors: Vec<SensorRow>,
    pub nets: Vec<NetRate>,
    /// 本轮新状态（写回 raw_metrics._snmp_state）
    pub state: SnmpState,
}

// ============================================================
// 采集入口
// ============================================================

/// 采集单台设备全部性能指标并计算差值类指标（CPU 使用率/网卡速率）。
///
/// `prev` 为上一轮状态（无历史时传 None，首轮差值指标为 None 但状态
/// 照常写入）；`now` 为本轮采集时刻（状态时间戳与报告 collected_at 同源）。
pub async fn collect(
    client: &Client<UdpHandle>,
    prev: Option<&SnmpState>,
    now: DateTime<Utc>,
) -> SnmpSnapshot {
    let now_unix = now.timestamp();
    let (cpu_counters, cpu_usage_pct) = collect_cpu(client, prev.and_then(|p| p.cpu)).await;
    // hrStorageTable walk 一次，内存与磁盘共用
    let storage = walk_hr_storage(client).await;
    let mut mem = memory_from_storage(&storage);
    if mem.is_none() {
        mem = ucd_memory(client).await;
    }
    let disks = disks_from_storage(&storage);
    let swap = ucd_swap(client).await;
    let sensors = collect_sensors(client).await;
    let (nets, state_ifaces) = collect_nets(client, prev, now_unix).await;
    let load = collect_load(client).await;

    SnmpSnapshot {
        cpu_usage_pct,
        load,
        mem_total: mem.as_ref().map(|(t, _)| *t),
        mem_used: mem.as_ref().map(|(_, u)| *u),
        swap_total: swap.0,
        swap_used: swap.1,
        disks,
        sensors,
        nets,
        state: SnmpState {
            ts: now_unix,
            cpu: cpu_counters,
            ifaces: state_ifaces,
        },
    }
}

// ============================================================
// 纯函数（差值/归一/聚合，单测覆盖）
// ============================================================

/// Counter32 差值（回绕校正：cur < prev 时按 2^32 计数空间回绕）
fn delta32(prev: u64, cur: u64) -> u64 {
    if cur >= prev {
        cur - prev
    } else {
        cur + (1u64 << 32) - prev
    }
}

/// CPU 使用率（%）：100×(1−Δidle/Δtotal)；Δtotal≤0（时段过短/计数器
/// 回绕）时 None，结果钳制 [0, 100]。
pub fn cpu_usage_pct(prev: CpuCounters, cur: CpuCounters) -> Option<f64> {
    let total = delta32(prev.total, cur.total);
    let idle = delta32(prev.idle, cur.idle);
    if total == 0 {
        return None;
    }
    Some((100.0 * (1.0 - idle as f64 / total as f64)).clamp(0.0, 100.0))
}

/// 网卡速率（bit/s）：Δbytes×8/elapsed，Counter32 回绕校正。
///
/// 合理性校验：Δbytes ≤ elapsed×cap/8×1.1（cap 取 ifSpeed，低于
/// [`MIN_KNOWN_SPEED_BPS`] 视为未知用 100 Gbps 兜底）；超限（计数器回绕
/// 未校正/设备计数清零）→ None，防止出现虚假峰值。
pub fn iface_rate_bps(
    prev: u64,
    cur: u64,
    elapsed_secs: i64,
    speed_bps: Option<u64>,
) -> Option<f64> {
    if elapsed_secs <= 0 {
        return None;
    }
    let delta = delta32(prev, cur);
    let cap = match speed_bps {
        Some(s) if s >= MIN_KNOWN_SPEED_BPS => s,
        _ => DEFAULT_SPEED_CAP_BPS,
    };
    let max_bytes = elapsed_secs as f64 * cap as f64 / 8.0 * 1.1;
    if delta as f64 > max_bytes {
        return None;
    }
    Some(delta as f64 * 8.0 / elapsed_secs as f64)
}

/// LM-Sensors 毫度 ℃ 归一（Gauge32 毫度 → ℃）：0 无效；换算后超出
/// [-100, 250]℃ 业务区间剔除（与 ingest 温度校验口径一致）。
pub fn temp_from_milli(raw: u64) -> Option<f64> {
    if raw == 0 {
        return None;
    }
    let c = raw as f64 / 1000.0;
    (-100.0..=250.0).contains(&c).then_some(c)
}

/// 内存使用率（%）：total 缺失/为 0 → None，结果钳制 [0, 100]。
pub fn mem_usage_pct(mem: Option<(u64, u64)>) -> Option<f64> {
    let (total, used) = mem?;
    if total == 0 {
        return None;
    }
    Some((used as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
}

/// 磁盘使用率（%）：各盘 used/total 取最大值，无有效项 → None。
pub fn disk_usage_pct(disks: &[DiskRow]) -> Option<f64> {
    disks
        .iter()
        .filter(|d| d.total > 0)
        .map(|d| (d.used as f64 / d.total as f64 * 100.0).clamp(0.0, 100.0))
        .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |m| m.max(v))))
}

/// 最高温度：无有效读数 → None。
pub fn max_temp(sensors: &[SensorRow]) -> Option<f64> {
    sensors
        .iter()
        .map(|s| s.value)
        .filter(|v| v.is_finite())
        .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |m| m.max(v))))
}

/// 流量合计（bit/s）：各网卡速率求和，全 None → None（历史快照曲线键）。
pub fn net_sums(nets: &[NetRate]) -> (Option<f64>, Option<f64>) {
    let mut rx = None::<f64>;
    let mut tx = None::<f64>;
    for n in nets {
        if let Some(v) = n.rx_bps {
            rx = Some(rx.unwrap_or(0.0) + v);
        }
        if let Some(v) = n.tx_bps {
            tx = Some(tx.unwrap_or(0.0) + v);
        }
    }
    (rx, tx)
}

// ============================================================
// 采集实现
// ============================================================

/// CPU：GET ssCpuRaw* 11 个计数器 → 求和与 idle 分量 → 与上一轮差值算
/// 使用率。Idle 缺失时整体 None（无法定义使用率），其余缺失按 0 计入。
async fn collect_cpu(
    client: &Client<UdpHandle>,
    prev: Option<CpuCounters>,
) -> (Option<CpuCounters>, Option<f64>) {
    let counters = match client.get_many(&cpu_raw_oids()).await {
        Ok(resp) => {
            // 与请求等长有序（FixedCardinality）；异常值（No Such Instance
            // 等）按缺失处理：Idle 缺失整体 None，其余缺失计 0
            let vals: Vec<Option<u64>> = resp
                .varbinds
                .iter()
                .map(|vb| {
                    if vb.value.is_exception() {
                        None
                    } else {
                        vb.value.as_u64()
                    }
                })
                .collect();
            if vals.len() != 11 {
                None
            } else {
                let idle = vals.get(CPU_RAW_IDLE_IDX).copied().flatten();
                let total: u64 = vals.iter().flatten().sum();
                idle.map(|idle| CpuCounters { total, idle })
            }
        }
        Err(e) => {
            tracing::debug!("ssCpuRaw 计数器读取失败（本轮无 CPU 指标）: {e}");
            None
        }
    };
    let usage = match (prev, counters) {
        (Some(prev), Some(cur)) => cpu_usage_pct(prev, cur),
        _ => None,
    };
    (counters, usage)
}

/// hrStorageTable 行解析结果
struct StorageRow {
    /// hrStorageType OID 值（Ram / FixedDisk 判定依据）
    row_type: Option<async_snmp::Oid>,
    descr: String,
    units: i64,
    size: i64,
    used: i64,
}

/// walk hrStorageTable 并按行索引合并各列（一次 walk 供内存/磁盘共用）。
/// 列：2 hrStorageType / 3 hrStorageDescr / 4 hrStorageAllocationUnits /
/// 5 hrStorageSize / 6 hrStorageUsed。
async fn walk_hr_storage(client: &Client<UdpHandle>) -> Vec<StorageRow> {
    let base = oid!(1, 3, 6, 1, 2, 1, 25, 2, 3, 1);
    let varbinds = walk_all(client, base).await;
    let mut types: BTreeMap<u32, async_snmp::Oid> = BTreeMap::new();
    let mut descrs: BTreeMap<u32, String> = BTreeMap::new();
    let mut units: BTreeMap<u32, i64> = BTreeMap::new();
    let mut sizes: BTreeMap<u32, i64> = BTreeMap::new();
    let mut useds: BTreeMap<u32, i64> = BTreeMap::new();
    for vb in &varbinds {
        let Some((col, idx)) = table_parts(&vb.oid, 10) else {
            continue;
        };
        match col {
            2 => {
                if let Some(o) = vb.value.as_oid() {
                    types.insert(idx, o.clone());
                }
            }
            3 => {
                if let Some(s) = vb.value.as_str() {
                    descrs.insert(idx, s.trim().to_string());
                }
            }
            4 => {
                if let Some(v) = int_value(&vb.value) {
                    units.insert(idx, v);
                }
            }
            5 => {
                if let Some(v) = int_value(&vb.value) {
                    sizes.insert(idx, v);
                }
            }
            6 => {
                if let Some(v) = int_value(&vb.value) {
                    useds.insert(idx, v);
                }
            }
            _ => {}
        }
    }
    types
        .keys()
        .filter_map(|idx| {
            let row_type = types.get(idx).cloned();
            let descr = descrs.get(idx)?.clone();
            Some(StorageRow {
                row_type,
                descr,
                units: units.get(idx).copied().unwrap_or(0),
                size: sizes.get(idx).copied().unwrap_or(0),
                used: useds.get(idx).copied().unwrap_or(0),
            })
        })
        .collect()
}

/// 内存（字节）：hrStorageTable Ram 行 total + "Available memory" 行
/// avail → used = total − avail（饱和）；两行不全时由调用方回落 UCD。
/// size ≤ 0（>8TB 32 位溢出为负）的行视为无效。
fn memory_from_storage(rows: &[StorageRow]) -> Option<(u64, u64)> {
    let ram = rows
        .iter()
        .find(|r| r.row_type.as_ref() == Some(&storage_type_ram()))
        .or_else(|| {
            rows.iter()
                .find(|r| r.descr.eq_ignore_ascii_case(STORAGE_DESCR_PHYS_MEM))
        })?;
    let avail = rows
        .iter()
        .find(|r| r.descr.eq_ignore_ascii_case(STORAGE_DESCR_AVAIL_MEM))?;
    if ram.units <= 0 || ram.size <= 0 || avail.units <= 0 || avail.size < 0 {
        return None;
    }
    let total = (ram.size as u64).saturating_mul(ram.units as u64);
    let avail_bytes = (avail.size as u64).saturating_mul(avail.units as u64);
    let used = total.saturating_sub(avail_bytes);
    (total > 0).then_some((total, used))
}

/// 磁盘：hrStorageTable 中 FixedDisk 行（本地固定盘）；size ≤ 0 跳过
/// （32 位容量溢出），device/mount 同取 hrStorageDescr（挂载点）。
fn disks_from_storage(rows: &[StorageRow]) -> Vec<DiskRow> {
    rows.iter()
        .filter(|r| r.row_type.as_ref() == Some(&storage_type_fixed_disk()))
        .filter(|r| r.units > 0 && r.size > 0 && !r.descr.is_empty())
        .map(|r| {
            let total = (r.size as u64).saturating_mul(r.units as u64);
            let used = (r.used.max(0) as u64)
                .saturating_mul(r.units as u64)
                .min(total);
            DiskRow {
                device: r.descr.clone(),
                mount: r.descr.clone(),
                total,
                used,
            }
        })
        .collect()
}

/// UCD 内存回落：memTotalReal(4.5)/memAvailReal(4.6)，KB → 字节；
/// 仅在 hrStorage 两行不全时调用。
async fn ucd_memory(client: &Client<UdpHandle>) -> Option<(u64, u64)> {
    let total_kb = get_nonneg_scalar(client, oid!(1, 3, 6, 1, 4, 1, 2021, 4, 5, 0)).await?;
    let avail_kb = get_nonneg_scalar(client, oid!(1, 3, 6, 1, 4, 1, 2021, 4, 6, 0)).await?;
    let total = total_kb.saturating_mul(1024);
    let used = total_kb.saturating_sub(avail_kb).saturating_mul(1024);
    (total > 0).then_some((total, used))
}

/// UCD 交换分区：memTotalSwap(4.3)/memAvailSwap(4.4)，KB → 字节；
/// 未配置（total=0）或不支持 → (None, None)。
async fn ucd_swap(client: &Client<UdpHandle>) -> (Option<u64>, Option<u64>) {
    let Some(total_kb) = get_nonneg_scalar(client, oid!(1, 3, 6, 1, 4, 1, 2021, 4, 3, 0)).await
    else {
        return (None, None);
    };
    let Some(avail_kb) = get_nonneg_scalar(client, oid!(1, 3, 6, 1, 4, 1, 2021, 4, 4, 0)).await
    else {
        return (None, None);
    };
    if total_kb == 0 {
        return (None, None);
    }
    let total = total_kb.saturating_mul(1024);
    let used = total_kb.saturating_sub(avail_kb).saturating_mul(1024);
    (Some(total), Some(used))
}

/// 温度：walk lmTempSensorsTable（列 2 device / 3 value 毫度），
/// 按行索引配对后归一；无效读数剔除。
async fn collect_sensors(client: &Client<UdpHandle>) -> Vec<SensorRow> {
    let varbinds = walk_all(client, oid!(1, 3, 6, 1, 4, 1, 2021, 13, 16, 2, 1)).await;
    let mut devices: BTreeMap<u32, String> = BTreeMap::new();
    let mut values: BTreeMap<u32, u64> = BTreeMap::new();
    for vb in &varbinds {
        let Some((col, idx)) = table_parts(&vb.oid, 11) else {
            continue;
        };
        match col {
            2 => {
                if let Some(s) = vb.value.as_str() {
                    let s = s.trim();
                    if !s.is_empty() {
                        devices.insert(idx, s.to_string());
                    }
                }
            }
            3 => {
                if let Some(v) = vb.value.as_u64() {
                    values.insert(idx, v);
                }
            }
            _ => {}
        }
    }
    values
        .into_iter()
        .filter_map(|(idx, raw)| {
            let label = devices.get(&idx)?;
            temp_from_milli(raw).map(|value| SensorRow {
                label: label.clone(),
                value,
            })
        })
        .collect()
}

/// 流量：bulk walk ifTable + ifXTable 按行索引合并 → 过滤 up 且非环回 →
/// 按接口名排序截断 → 与上一轮差值算速率。HC 64 位计数器优先，回落 32 位。
async fn collect_nets(
    client: &Client<UdpHandle>,
    prev: Option<&SnmpState>,
    now_unix: i64,
) -> (Vec<NetRate>, BTreeMap<String, IfaceCounters>) {
    // ifTable（base 1.3.6.1.2.1.2.2.1 共 9 分量；col2 descr / col3 type /
    // col5 speed / col8 oper / col10 inOctets / col14 outOctets）
    let mut descrs: BTreeMap<u32, String> = BTreeMap::new();
    let mut types: BTreeMap<u32, u64> = BTreeMap::new();
    let mut speeds: BTreeMap<u32, u64> = BTreeMap::new();
    let mut opers: BTreeMap<u32, u64> = BTreeMap::new();
    let mut in32: BTreeMap<u32, u64> = BTreeMap::new();
    let mut out32: BTreeMap<u32, u64> = BTreeMap::new();
    for vb in &walk_all(client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1)).await {
        let Some((col, idx)) = table_parts(&vb.oid, 9) else {
            continue;
        };
        match col {
            2 => str_map(&vb.value, &mut descrs, idx),
            3 => num_map(&vb.value, &mut types, idx),
            5 => num_map(&vb.value, &mut speeds, idx),
            8 => num_map(&vb.value, &mut opers, idx),
            10 => num_map(&vb.value, &mut in32, idx),
            14 => num_map(&vb.value, &mut out32, idx),
            _ => {}
        }
    }
    // ifXTable（base 1.3.6.1.2.1.31.1.1.1 共 10 分量；col1 ifName /
    // col6 ifHCInOctets / col10 ifHCOutOctets）
    let mut names: BTreeMap<u32, String> = BTreeMap::new();
    let mut hc_in: BTreeMap<u32, u64> = BTreeMap::new();
    let mut hc_out: BTreeMap<u32, u64> = BTreeMap::new();
    for vb in &walk_all(client, oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1)).await {
        let Some((col, idx)) = table_parts(&vb.oid, 10) else {
            continue;
        };
        match col {
            1 => str_map(&vb.value, &mut names, idx),
            6 => num_map(&vb.value, &mut hc_in, idx),
            10 => num_map(&vb.value, &mut hc_out, idx),
            _ => {}
        }
    }

    let elapsed = prev.map_or(0, |p| now_unix - p.ts);
    // 合并行键（两表索引对齐），过滤 up 且非环回（type 24），命名 ifName 优先
    let mut idxs: Vec<u32> = opers
        .keys()
        .filter(|idx| {
            opers.get(idx) == Some(&1)
                && types.get(idx) != Some(&24)
                && (names.contains_key(idx) || descrs.contains_key(idx))
        })
        .copied()
        .collect();
    idxs.sort_by_cached_key(|idx| {
        names
            .get(idx)
            .or_else(|| descrs.get(idx))
            .cloned()
            .unwrap_or_default()
    });

    let mut nets = Vec::new();
    let mut state_ifaces = BTreeMap::new();
    for idx in idxs.into_iter().take(MAX_NET_IFACES) {
        let Some(name) = names
            .get(&idx)
            .or_else(|| descrs.get(&idx))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let cur_in = hc_in.get(&idx).copied().or_else(|| in32.get(&idx).copied());
        let cur_out = hc_out
            .get(&idx)
            .copied()
            .or_else(|| out32.get(&idx).copied());
        let speed = speeds.get(&idx).copied();
        let (rx_bps, tx_bps) = match prev.and_then(|p| p.ifaces.get(&name)) {
            Some(p) if elapsed >= MIN_RATE_ELAPSED_SECS => (
                cur_in.and_then(|c| iface_rate_bps(p.in_octets, c, elapsed, speed)),
                cur_out.and_then(|c| iface_rate_bps(p.out_octets, c, elapsed, speed)),
            ),
            _ => (None, None),
        };
        if let (Some(ci), Some(co)) = (cur_in, cur_out) {
            state_ifaces.insert(
                name.clone(),
                IfaceCounters {
                    in_octets: ci,
                    out_octets: co,
                },
            );
        }
        nets.push(NetRate {
            iface: name,
            rx_bps,
            tx_bps,
        });
    }
    (nets, state_ifaces)
}

/// 负载：GET laLoad.1/2/3（字符串形式浮点），解析失败容忍缺失
async fn collect_load(client: &Client<UdpHandle>) -> (Option<f64>, Option<f64>, Option<f64>) {
    let load1 = get_f64_string(client, oid!(1, 3, 6, 1, 4, 1, 2021, 10, 1, 3, 1)).await;
    let load5 = get_f64_string(client, oid!(1, 3, 6, 1, 4, 1, 2021, 10, 1, 3, 2)).await;
    let load15 = get_f64_string(client, oid!(1, 3, 6, 1, 4, 1, 2021, 10, 1, 3, 3)).await;
    (load1, load5, load15)
}

// ============================================================
// 采集工具函数
// ============================================================

/// bulk walk 全量收集（单条 walk 错误即中断，保留已收条目；
/// 流建立失败返回空，由调用方按"该类缺失"处理）
async fn walk_all(client: &Client<UdpHandle>, base: async_snmp::Oid) -> Vec<async_snmp::VarBind> {
    let mut stream = match client.bulk_walk(base, WALK_MAX_REPETITIONS) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!("SNMP walk 流建立失败: {e}");
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    while let Some(item) = stream.next().await {
        match item {
            Ok(vb) => out.push(vb),
            Err(e) => {
                tracing::debug!("SNMP walk 提前终止（保留部分结果）: {e}");
                break;
            }
        }
    }
    out
}

/// 表行列号提取：`oid` 长于 base（hrStorage/ifXTable 10 分量、ifTable
/// 9 分量等）且余下恰好为 [列号, 行索引] 时返回之。
fn table_parts(oid: &async_snmp::Oid, base_len: usize) -> Option<(u32, u32)> {
    if oid.len() <= base_len {
        return None;
    }
    let suffix = oid.suffix(oid.len() - base_len)?;
    if suffix.len() != 2 {
        return None;
    }
    Some((suffix[0], suffix[1]))
}

/// 整数值提取：Integer32 优先，其次无符号类型（Gauge/Counter），负值
/// 由调用方语义处理
fn int_value(v: &SnmpValue) -> Option<i64> {
    if let Some(i) = v.as_i32() {
        return Some(i64::from(i));
    }
    v.as_u64().and_then(|u| i64::try_from(u).ok())
}

/// 非负标量 GET（UCD 内存/交换分区，KB）
async fn get_nonneg_scalar(client: &Client<UdpHandle>, oid_val: async_snmp::Oid) -> Option<u64> {
    let resp = client.get(&oid_val).await.ok()?;
    let vb = resp.varbinds.first()?;
    if vb.value.is_exception() {
        return None;
    }
    int_value(&vb.value)?.try_into().ok()
}

/// 字符串标量 GET 后解析浮点（UCD laLoad 为字符串形式）
async fn get_f64_string(client: &Client<UdpHandle>, oid_val: async_snmp::Oid) -> Option<f64> {
    let resp = client.get(&oid_val).await.ok()?;
    let vb = resp.varbinds.first()?;
    if vb.value.is_exception() {
        return None;
    }
    vb.value.as_str()?.trim().parse::<f64>().ok()
}

/// 字符串列入表（trim 后非空才收录）
fn str_map(value: &SnmpValue, map: &mut BTreeMap<u32, String>, idx: u32) {
    if let Some(s) = value.as_str() {
        let s = s.trim();
        if !s.is_empty() {
            map.insert(idx, s.to_string());
        }
    }
}

/// 数值列入表（异常值天然被 as_u64 拒绝）
fn num_map(value: &SnmpValue, map: &mut BTreeMap<u32, u64>, idx: u32) {
    if let Some(v) = value.as_u64() {
        map.insert(idx, v);
    }
}

// ============================================================
// 上报 JSON 合成
// ============================================================

/// 合成与 AgentReport 同形的上报 JSON（前端主机详情弹窗无需改动即可
/// 渲染；agent_version 缺省、kernel/arch 等设备侧不可得字段置 null，
/// 前端回落 "-"）。附加私有键 `_snmp_state` 随 raw_metrics 持久化，
/// 供下一轮差值计算（前端忽略未知键）。
pub fn build_report_json(
    machine_id: &str,
    hostname: &str,
    os: Option<&str>,
    uptime_secs: Option<i64>,
    snap: &SnmpSnapshot,
    now: DateTime<Utc>,
) -> Value {
    let (load1, load5, load15) = snap.load;
    json!({
        "machine_id": machine_id,
        "hostname": hostname,
        "collected_at": now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "system": {
            "os": os,
            "kernel": Value::Null,
            "arch": Value::Null,
            "uptime_secs": uptime_secs,
        },
        "cpu": {
            "usage_pct": snap.cpu_usage_pct,
            "cores": Value::Null,
            "load1": load1,
            "load5": load5,
            "load15": load15,
        },
        "memory": {
            "total": snap.mem_total,
            "used": snap.mem_used,
            "swap_total": snap.swap_total,
            "swap_used": snap.swap_used,
        },
        "disks": snap.disks.iter().map(|d| json!({
            "device": d.device,
            "mount": d.mount,
            "total": d.total,
            "used": d.used,
            "read_iops": Value::Null,
            "write_iops": Value::Null,
            "util_pct": Value::Null,
        })).collect::<Vec<Value>>(),
        "nets": snap.nets.iter().map(|n| json!({
            "iface": n.iface,
            "rx_bps": n.rx_bps,
            "tx_bps": n.tx_bps,
            "errors": Value::Null,
        })).collect::<Vec<Value>>(),
        "sensors": snap.sensors.iter().map(|s| json!({
            "label": s.label,
            "kind": "temp",
            "value": s.value,
        })).collect::<Vec<Value>>(),
        "_snmp_state": snap.state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter32差值_正常与回绕() {
        assert_eq!(delta32(100, 250), 150, "正常递增");
        assert_eq!(delta32(0, 0), 0, "无变化");
        // 回绕：prev=2^32−101 处回绕到 50，实际步进 = 100(到 MAX) + 1(归零) + 50
        assert_eq!(delta32(u64::from(u32::MAX) - 100, 50), 151);
    }

    #[test]
    fn cpu使用率_差值计算与边界() {
        let prev = CpuCounters {
            total: 10_000,
            idle: 8_000,
        };
        let cur = CpuCounters {
            total: 20_000,
            idle: 17_000,
        };
        // Δtotal=10000 Δidle=9000 → 10%（浮点容差比较）
        let usage = cpu_usage_pct(prev, cur).unwrap();
        assert!((usage - 10.0).abs() < 1e-9);
        // Δtotal=0 → None
        assert_eq!(cpu_usage_pct(cur, cur), None, "同值差值应判 None");
        // idle 零增长 → 100%
        let busy = CpuCounters {
            total: 30_000,
            idle: 17_000,
        };
        assert_eq!(cpu_usage_pct(cur, busy), Some(100.0));
        // 回绕场景：total/idle 同步回绕，使用率不变
        // Δtotal = 999+2^32−MAX = 1000；Δidle = 759+2^32−(MAX−120) = 880 → 12%
        let wrap_prev = CpuCounters {
            total: u64::from(u32::MAX),
            idle: u64::from(u32::MAX) - 120,
        };
        let wrap_cur = CpuCounters {
            total: 999,
            idle: 759,
        };
        assert_eq!(cpu_usage_pct(wrap_prev, wrap_cur), Some(12.0));
    }

    #[test]
    fn 网卡速率_计算与合理性校验() {
        // 300s 内走 37.5GB → 恰好 1 Gbps
        let rate = iface_rate_bps(0, 37_500_000_000, 300, Some(1_000_000_000)).unwrap();
        assert!((rate - 1_000_000_000.0).abs() < 1.0);
        // 超速（计数器清零类异常）：cap 1Gbps，1.1 倍容差外拒绝
        assert_eq!(
            iface_rate_bps(0, 500_000_000_000, 300, Some(1_000_000_000)),
            None,
            "超出链路速率容差应判 None"
        );
        // 速率未知用 100Gbps 兜底
        let rate = iface_rate_bps(0, 3_750_000_000_000, 300, None).unwrap();
        assert!((rate - 100_000_000_000.0).abs() < 1.0);
        // 低速虚拟口（10 bit/s）不作为校验基准，走兜底
        assert!(
            iface_rate_bps(0, 37_500_000_000, 300, Some(10)).is_some(),
            "ifSpeed 过低应视为未知"
        );
        // 32 位回绕：prev=MAX−149 回绕归零 → Δ=150 字节 → 4 bit/s
        let rate = iface_rate_bps(u64::from(u32::MAX) - 149, 0, 300, Some(1_000_000)).unwrap();
        assert!((rate - 4.0).abs() < 0.01);
        // 时段非法
        assert_eq!(iface_rate_bps(0, 100, 0, None), None);
        assert_eq!(iface_rate_bps(0, 100, -5, None), None);
    }

    #[test]
    fn 温度归一_毫度转摄氏与越界剔除() {
        assert_eq!(temp_from_milli(54_000), Some(54.0));
        assert_eq!(temp_from_milli(54_500), Some(54.5));
        assert_eq!(temp_from_milli(0), None, "0 视为无效读数");
        assert_eq!(temp_from_milli(250_001), None, "超上限剔除");
        assert_eq!(temp_from_milli(300_000), None);
    }

    #[test]
    fn 使用率聚合_内存磁盘温度与流量求和() {
        // 内存
        assert_eq!(mem_usage_pct(None), None);
        assert_eq!(mem_usage_pct(Some((0, 100))), None, "total=0 应为 None");
        assert_eq!(mem_usage_pct(Some((1000, 250))), Some(25.0));
        assert_eq!(mem_usage_pct(Some((1000, 2000))), Some(100.0), "越界钳制");
        // 磁盘取最大
        let disks = vec![
            DiskRow {
                device: "a".into(),
                mount: "/a".into(),
                total: 1000,
                used: 100,
            },
            DiskRow {
                device: "b".into(),
                mount: "/b".into(),
                total: 2000,
                used: 1000,
            },
            DiskRow {
                device: "c".into(),
                mount: "/c".into(),
                total: 0,
                used: 500,
            },
        ];
        assert_eq!(disk_usage_pct(&disks), Some(50.0));
        assert_eq!(disk_usage_pct(&[]), None);
        // 温度取最大
        let sensors = vec![
            SensorRow {
                label: "a".into(),
                value: 45.0,
            },
            SensorRow {
                label: "b".into(),
                value: 62.5,
            },
        ];
        assert_eq!(max_temp(&sensors), Some(62.5));
        assert_eq!(max_temp(&[]), None);
        // 流量求和：None 不计入，全 None → None
        let nets = vec![
            NetRate {
                iface: "eth0".into(),
                rx_bps: Some(100.0),
                tx_bps: None,
            },
            NetRate {
                iface: "eth1".into(),
                rx_bps: Some(50.0),
                tx_bps: Some(20.0),
            },
        ];
        let (rx, tx) = net_sums(&nets);
        assert_eq!(rx, Some(150.0));
        assert_eq!(tx, Some(20.0));
        let empty = [NetRate {
            iface: "eth0".into(),
            rx_bps: None,
            tx_bps: None,
        }];
        assert_eq!(net_sums(&empty), (None, None));
    }

    #[test]
    fn 状态反序列化_缺字段降级与完整解析() {
        // 完整状态
        let full = r#"{"ts":1000,"cpu":{"total":10,"idle":8},"ifaces":{"eth0":{"in_octets":1,"out_octets":2}}}"#;
        let state: SnmpState = serde_json::from_str(full).unwrap_or_default();
        assert_eq!(state.ts, 1000);
        assert_eq!(state.cpu, Some(CpuCounters { total: 10, idle: 8 }));
        assert_eq!(state.ifaces.get("eth0").map(|c| c.in_octets), Some(1));
        // 缺字段（旧数据）→ default 降级
        let partial: SnmpState = serde_json::from_str(r#"{"ts":1000}"#).unwrap_or_default();
        assert_eq!(partial.cpu, None);
        assert!(partial.ifaces.is_empty());
        // 空对象
        let empty: SnmpState = serde_json::from_str("{}").unwrap_or_default();
        assert_eq!(empty.ts, 0);
    }

    #[test]
    fn 上报json合成_与agent同形且含状态() {
        let snap = SnmpSnapshot {
            cpu_usage_pct: Some(12.5),
            load: (Some(0.5), Some(0.4), Some(0.3)),
            mem_total: Some(1000),
            mem_used: Some(250),
            swap_total: None,
            swap_used: None,
            disks: vec![DiskRow {
                device: "/".into(),
                mount: "/".into(),
                total: 1000,
                used: 100,
            }],
            sensors: vec![SensorRow {
                label: "coretemp".into(),
                value: 54.0,
            }],
            nets: vec![NetRate {
                iface: "eth0".into(),
                rx_bps: None,
                tx_bps: Some(8000.0),
            }],
            state: SnmpState {
                ts: 1728,
                cpu: Some(CpuCounters { total: 10, idle: 8 }),
                ifaces: BTreeMap::new(),
            },
        };
        let now = Utc::now();
        let report = build_report_json(
            "snmp:1",
            "dev-01",
            Some("Linux dev 5.9"),
            Some(3600),
            &snap,
            now,
        );
        // 前端渲染依赖键存在
        assert_eq!(report["machine_id"], "snmp:1");
        assert_eq!(report["hostname"], "dev-01");
        assert_eq!(report["system"]["os"], "Linux dev 5.9");
        assert_eq!(report["cpu"]["usage_pct"], 12.5);
        assert_eq!(report["memory"]["used"], 250);
        assert_eq!(report["disks"][0]["total"], 1000);
        assert!(report["disks"][0]["util_pct"].is_null());
        assert_eq!(report["nets"][0]["tx_bps"], 8000.0);
        assert!(report["nets"][0]["errors"].is_null());
        assert_eq!(report["sensors"][0]["kind"], "temp");
        assert_eq!(report["sensors"][0]["value"], 54.0);
        // 私有状态键随 JSON 写回
        assert_eq!(report["_snmp_state"]["cpu"]["total"], 10);
        assert!(report["collected_at"].is_string());
    }
}
