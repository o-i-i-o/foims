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
//! 计数器差值依赖上一轮状态：调用方自 monitored_hosts.raw_metrics 私有键
//! `_snmp_state` 读取 [`SnmpState`]，采集完成后新状态随 raw_metrics 写回
//! （差值算不出时状态仍更新，保证下一轮可用）。
//!
//! 单轮失败容忍：任一指标类采集/解析失败仅该类为空，不影响其他类；
//! monitored_hosts 热列由 upsert 的 COALESCE 保留上一轮旧值。

use std::collections::BTreeMap;

use async_snmp::value::Value as SnmpValue;
use async_snmp::{Client, oid, transport::UdpHandle};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::events::ObsEvent;
use foims_models::{
    SNMP_EVENT_DEVICE_REBOOT, SNMP_EVENT_LINK_FLIP, SNMP_SEVERITY_CRITICAL, SNMP_SEVERITY_INFO,
    SNMP_SEVERITY_WARNING,
};

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
// CPU 计数器 ssCpuRaw*.50~60（SNMP 标量必须带 .0 实例后缀，缺失会
// 返回 No Such Instance 导致使用率永远为空）
fn cpu_raw_oids() -> [async_snmp::Oid; 11] {
    [
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 50, 0), // User
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 51, 0), // Nice
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 52, 0), // System
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 53, 0), // Idle
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 54, 0), // Wait
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 55, 0), // Kernel
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 56, 0), // Interrupt
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 57, 0), // SoftIrq
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 58, 0), // Steal
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 59, 0), // Guest
        oid!(1, 3, 6, 1, 4, 1, 2021, 11, 60, 0), // GuestNice
    ]
}

/// CPU Raw 计数器中 Idle 的下标（usage 分子）
const CPU_RAW_IDLE_IDX: usize = 3;

/// Wait（iowait）下标：计入 total 但归入非 busy 侧（B5：与 top 口径一致，
/// I/O 等待不算 CPU 使用率）
const CPU_RAW_WAIT_IDX: usize = 4;

/// total 计入口径下标（B5）：User/Nice/System/Idle/Wait/Interrupt/SoftIrq/Steal。
/// 排除 Kernel（net-snmp 中 Kernel=System+Interrupt+SoftIrq 的重叠和，
/// 一并求和会双倍计入内核时间）与 Guest/GuestNice（guest 时间已包含
/// 在 User/Nice 内，另属虚拟化客户负载的重复计数）。
const CPU_RAW_TOTAL_IDXS: [usize; 8] = [0, 1, 2, 3, 4, 6, 7, 8];

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

// 上一轮状态（raw_metrics 私有键 _snmp_state）

/// CPU 计数器快照：ssCpuRaw* 按 B5 口径求和 + 非 busy（Idle+Wait）分量
///（差值算使用率）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CpuCounters {
    pub total: u64,
    pub idle: u64,
}

/// 单网卡收发字节与错误/丢弃计数器快照（差值算速率与本轮错误增量）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct IfaceCounters {
    pub in_octets: u64,
    pub out_octets: u64,
    /// 入向错误计数（Counter32；旧状态缺字段降级 None）
    #[serde(default)]
    pub in_errors: Option<u64>,
    #[serde(default)]
    pub out_errors: Option<u64>,
    #[serde(default)]
    pub in_discards: Option<u64>,
    #[serde(default)]
    pub out_discards: Option<u64>,
}

/// 上一轮 SNMP 采集状态（monitored_hosts.raw_metrics 私有键 `_snmp_state`）。
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
    /// 上一轮各网口 ifOperStatus（IF-MIB 编码：1=up/2=down/…），供链路翻动检测
    #[serde(default)]
    pub if_status: BTreeMap<String, i32>,
    /// 上一轮各网口 ifAdminStatus，用于 admin-down 降噪标注
    #[serde(default)]
    pub if_admin: BTreeMap<String, i32>,
    /// 上一轮 sysUpTime（秒），供重启检测（回退即重启）
    #[serde(default)]
    pub last_boot_uptime: Option<i64>,
    /// 上一轮采集时刻（unix 秒），供重启估算停机时长与轮询间隔
    #[serde(default)]
    pub last_poll_ts: Option<i64>,
}

// 采集结果

/// 单网卡速率结果（bit/s；差值不可得时为 None，与 agent 上报口径一致）
#[derive(Debug, Clone, Default)]
pub struct NetRate {
    pub iface: String,
    pub rx_bps: Option<f64>,
    pub tx_bps: Option<f64>,
    /// ifOperStatus 文本（up/down/…）
    pub status: Option<String>,
    /// ifAdminStatus 文本（up/down/…）
    pub admin_status: Option<String>,
    /// 链路速率（Mbps，ifSpeed/1e6 归一，未知为 None）
    pub speed_mbps: Option<u64>,
    /// 本轮入向错误增量（Counter32 回绕校正；首轮/无增量为 None）
    pub err_in_delta: Option<u64>,
    pub err_out_delta: Option<u64>,
    pub disc_in_delta: Option<u64>,
    pub disc_out_delta: Option<u64>,
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

// 采集入口

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
    let (nets, state_ifaces, state_if_status, state_if_admin) =
        collect_nets(client, prev, now_unix).await;
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
            if_status: state_if_status,
            if_admin: state_if_admin,
            // 重启检测所需的 sysUpTime 由调用方（snmp_poll 读 system 组）回填
            last_boot_uptime: None,
            last_poll_ts: Some(now_unix),
        },
    }
}

// 纯函数（差值/归一/聚合，单测覆盖）

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

/// 网卡速率（bit/s）：Δbytes×8/elapsed，按计数器位宽分道回绕处理。
///
/// 合理性校验：Δbytes ≤ elapsed×cap/8×1.1（cap 取 ifSpeed，低于
/// [`MIN_KNOWN_SPEED_BPS`] 视为未知用 100 Gbps 兜底）；超限（计数器未校正/
/// 设备计数清零）→ None，防止出现虚假峰值。
///
/// B2：ifHCInOctets（Counter64）与 ifInOctets（Counter32）共用本函数，
/// cur<prev 时仅当 prev 落在 32 位计数空间才按 2^32 回绕校正；
/// prev 超过 u32 最大值说明本轮使用的是 64 位计数器，其 cur<prev 只
/// 可能是设备重启计数清零（2^64 回绕在现实流量下不可达），无法可信
/// 差值 → None；无差别加 2^32 会产出虚假峰值，且 prev−cur>2^32 时
/// u64 减法下溢。
pub fn iface_rate_bps(
    prev: u64,
    cur: u64,
    elapsed_secs: i64,
    speed_bps: Option<u64>,
) -> Option<f64> {
    if elapsed_secs <= 0 {
        return None;
    }
    let delta = if cur >= prev {
        cur - prev
    } else if prev <= u64::from(u32::MAX) {
        // Counter32 回绕校正，32 位空间内不会下溢
        (u64::from(u32::MAX) - prev) + cur + 1
    } else {
        return None;
    };
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

/// 重启判定的最小上机时长（秒）：上一轮 uptime 低于此值时不据此判重启
/// （刚上线/采集初期满载时钟不具参考性）。
const REBOOT_MIN_PREV_UPTIME_SECS: i64 = 300;

/// 基于上一轮与本轮 ifOperStatus 比对，检出链路翻动（up↔down）事件（纯函数）。
///
/// - 基线：`prev.if_status` 为空（首轮/旧数据）时不发事件；
/// - 方向：本轮 1=up、2=down 区分文案，其余编码归为 `changed`；
/// - 降噪：admin-down（ifAdminStatus=2）导致的 oper down 标注 `admin_down=true`
///   并降为 info（管理员主动关停不视为故障）；非 admin-down 的 down 为 critical，
///   up 恢复为 info。
pub fn detect_link_flips(
    prev: &SnmpState,
    cur_status: &BTreeMap<String, i32>,
    cur_admin: &BTreeMap<String, i32>,
) -> Vec<ObsEvent> {
    let mut events = Vec::new();
    if prev.if_status.is_empty() {
        return events;
    }
    for (iface, &cur) in cur_status {
        let Some(&old) = prev.if_status.get(iface) else {
            continue;
        };
        if old == cur {
            continue;
        }
        let direction = match cur {
            1 => "up",
            2 => "down",
            _ => "changed",
        };
        let admin_down = cur == 2 && cur_admin.get(iface) == Some(&2);
        let severity = if direction == "up" || admin_down {
            SNMP_SEVERITY_INFO
        } else {
            SNMP_SEVERITY_CRITICAL
        };
        events.push(ObsEvent::new(
            SNMP_EVENT_LINK_FLIP,
            Some(iface.clone()),
            severity,
            json!({ "direction": direction, "admin_down": admin_down }),
        ));
    }
    // 接口消失补报：上轮 up 而本轮 ifTable 不再报告该接口（板卡拔出/
    // 子接口删除），标 vanished 并取 info 控制告警面
    for (iface, &old) in &prev.if_status {
        if old == 1 && !cur_status.contains_key(iface) {
            events.push(ObsEvent::new(
                SNMP_EVENT_LINK_FLIP,
                Some(iface.clone()),
                SNMP_SEVERITY_INFO,
                json!({ "direction": "down", "vanished": true }),
            ));
        }
    }
    events
}

/// 基于 sysUpTime 回退检出设备重启（纯函数）。
///
/// `prev_uptime`/`cur_uptime` 为秒；本轮 < 上轮且上轮 > 300s 时判为重启，
/// 估算停机时长 = 轮询间隔 `gap_secs` − 当前 uptime（负值钳 0）。
pub fn detect_reboot(
    prev_uptime: Option<i64>,
    cur_uptime: Option<i64>,
    gap_secs: i64,
) -> Option<ObsEvent> {
    let (prev, cur) = match (prev_uptime, cur_uptime) {
        (Some(p), Some(c)) => (p, c),
        _ => return None,
    };
    if cur >= prev || prev <= REBOOT_MIN_PREV_UPTIME_SECS {
        return None;
    }
    let downtime = (gap_secs - cur).max(0);
    Some(ObsEvent::new(
        SNMP_EVENT_DEVICE_REBOOT,
        None,
        SNMP_SEVERITY_WARNING,
        json!({ "downtime": format_secs(downtime), "downtime_secs": downtime }),
    ))
}

/// 秒数格式化为 `Xh Ym Zs`（供重启通知文案 uptime 回退后估算停机时长）。
fn format_secs(secs: i64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h}h {m}m {s}s")
}

// 采集实现

/// CPU：GET ssCpuRaw* 11 个计数器 → 按 [`CPU_RAW_TOTAL_IDXS`] 口径求和与
/// 非 busy（Idle+Wait）分量 → 与上一轮差值算使用率。Idle 缺失时整体
/// None（无法定义使用率），其余缺失按 0 计入。
async fn collect_cpu(
    client: &Client<UdpHandle>,
    prev: Option<CpuCounters>,
) -> (Option<CpuCounters>, Option<f64>) {
    let counters = match client.get_many(&cpu_raw_oids()).await {
        Ok(resp) => {
            // 响应与请求等长且有序（FixedCardinality）；异常值
            //（No Such Instance 等）归为缺失
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
                let wait = vals
                    .get(CPU_RAW_WAIT_IDX)
                    .copied()
                    .flatten()
                    .unwrap_or_default();
                let total: u64 = CPU_RAW_TOTAL_IDXS
                    .iter()
                    .filter_map(|i| vals.get(*i).copied().flatten())
                    .sum();
                idle.map(|idle| CpuCounters {
                    total,
                    idle: idle + wait,
                })
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
/// 按接口名排序截断 → 与上一轮差值算速率与错误增量。HC 64 位计数器优先，回落 32 位。
///
/// 同时为全部非环回命名接口记录 ifOperStatus/ifAdminStatus（含 down），
/// 供上层 [`detect_link_flips`] 比对上一轮状态检出链路翻动。
async fn collect_nets(
    client: &Client<UdpHandle>,
    prev: Option<&SnmpState>,
    now_unix: i64,
) -> (
    Vec<NetRate>,
    BTreeMap<String, IfaceCounters>,
    BTreeMap<String, i32>,
    BTreeMap<String, i32>,
) {
    // ifTable（base 1.3.6.1.2.1.2.2.1 共 9 分量；col2 descr / col3 type /
    // col5 speed / col7 admin / col8 oper / col10 ifInOctets / col16 ifOutOctets，
    // 错误/丢弃：col13 ifInDiscards / col14 ifInErrors / col19 ifOutDiscards /
    // col20 ifOutErrors）
    let mut descrs: BTreeMap<u32, String> = BTreeMap::new();
    let mut types: BTreeMap<u32, u64> = BTreeMap::new();
    let mut speeds: BTreeMap<u32, u64> = BTreeMap::new();
    let mut admins: BTreeMap<u32, u64> = BTreeMap::new();
    let mut opers: BTreeMap<u32, u64> = BTreeMap::new();
    let mut in32: BTreeMap<u32, u64> = BTreeMap::new();
    let mut out32: BTreeMap<u32, u64> = BTreeMap::new();
    // 错误/丢弃计数器（与 octets 同回落 32 位表）
    let mut in_err: BTreeMap<u32, u64> = BTreeMap::new();
    let mut out_err: BTreeMap<u32, u64> = BTreeMap::new();
    let mut in_disc: BTreeMap<u32, u64> = BTreeMap::new();
    let mut out_disc: BTreeMap<u32, u64> = BTreeMap::new();
    for vb in &walk_all(client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1)).await {
        let Some((col, idx)) = table_parts(&vb.oid, 9) else {
            continue;
        };
        match col {
            2 => str_map(&vb.value, &mut descrs, idx),
            3 => num_map(&vb.value, &mut types, idx),
            5 => num_map(&vb.value, &mut speeds, idx),
            7 => num_map(&vb.value, &mut admins, idx),
            8 => num_map(&vb.value, &mut opers, idx),
            10 => num_map(&vb.value, &mut in32, idx),
            13 => num_map(&vb.value, &mut in_disc, idx),
            14 => num_map(&vb.value, &mut in_err, idx),
            16 => num_map(&vb.value, &mut out32, idx),
            19 => num_map(&vb.value, &mut out_disc, idx),
            20 => num_map(&vb.value, &mut out_err, idx),
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

    // 命名解析：ifName 优先、回落 ifDescr；非环回（type 24）且有 oper 的
    // 全部接口计入状态图（含 down），供链路翻动检测
    let name_of = |idx: &u32| -> Option<String> {
        names
            .get(idx)
            .or_else(|| descrs.get(idx))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let mut if_status: BTreeMap<String, i32> = BTreeMap::new();
    let mut if_admin: BTreeMap<String, i32> = BTreeMap::new();
    for idx in opers.keys() {
        if types.get(idx) == Some(&24) {
            continue;
        }
        let Some(name) = name_of(idx) else {
            continue;
        };
        if let Some(op) = opers.get(idx) {
            if_status.insert(name.clone(), *op as i32);
        }
        if let Some(ad) = admins.get(idx) {
            if_admin.insert(name, *ad as i32);
        }
    }

    let elapsed = prev.map_or(0, |p| now_unix - p.ts);
    // 速率/错误仅统计 up 且非环回接口（与既有展示口径一致），按名排序截断
    let mut idxs: Vec<u32> = opers
        .keys()
        .filter(|idx| {
            opers.get(idx) == Some(&1)
                && types.get(idx) != Some(&24)
                && (names.contains_key(idx) || descrs.contains_key(idx))
        })
        .copied()
        .collect();
    idxs.sort_by_cached_key(|idx| name_of(idx).unwrap_or_default());

    let mut nets = Vec::new();
    let mut state_ifaces = BTreeMap::new();
    for idx in idxs.into_iter().take(MAX_NET_IFACES) {
        let Some(name) = name_of(&idx) else {
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
        // 错误/丢弃本轮增量：Counter32 回绕校正；上一轮无该字段时 None
        let prev_c = prev.and_then(|p| p.ifaces.get(&name));
        let err_delta = |prev_v: Option<u64>, cur_v: Option<u64>| -> Option<u64> {
            match (prev_v, cur_v) {
                (Some(pv), Some(cv)) if elapsed >= MIN_RATE_ELAPSED_SECS => Some(delta32(pv, cv)),
                _ => None,
            }
        };
        let err_in_delta = err_delta(prev_c.and_then(|c| c.in_errors), in_err.get(&idx).copied());
        let err_out_delta = err_delta(
            prev_c.and_then(|c| c.out_errors),
            out_err.get(&idx).copied(),
        );
        let disc_in_delta = err_delta(
            prev_c.and_then(|c| c.in_discards),
            in_disc.get(&idx).copied(),
        );
        let disc_out_delta = err_delta(
            prev_c.and_then(|c| c.out_discards),
            out_disc.get(&idx).copied(),
        );

        if let (Some(ci), Some(co)) = (cur_in, cur_out) {
            state_ifaces.insert(
                name.clone(),
                IfaceCounters {
                    in_octets: ci,
                    out_octets: co,
                    in_errors: in_err.get(&idx).copied(),
                    out_errors: out_err.get(&idx).copied(),
                    in_discards: in_disc.get(&idx).copied(),
                    out_discards: out_disc.get(&idx).copied(),
                },
            );
        }
        nets.push(NetRate {
            speed_mbps: speed.map(|s| s / 1_000_000),
            status: opers.get(&idx).map(|c| if_status_text(*c).to_string()),
            admin_status: admins.get(&idx).map(|c| if_admin_text(*c).to_string()),
            err_in_delta,
            err_out_delta,
            disc_in_delta,
            disc_out_delta,
            iface: name,
            rx_bps,
            tx_bps,
        });
    }
    (nets, state_ifaces, if_status, if_admin)
}

/// ifOperStatus 文本（1 up/2 down/3 testing/4 unknown/5 dormant/6 notPresent/7 lowerLayerUp）
fn if_status_text(code: u64) -> &'static str {
    match code {
        1 => "up",
        2 => "down",
        3 => "testing",
        4 => "unknown",
        5 => "dormant",
        6 => "notPresent",
        7 => "lowerLayerUp",
        _ => "unknown",
    }
}

/// ifAdminStatus 文本（1 up/2 down/3 testing）
fn if_admin_text(code: u64) -> &'static str {
    match code {
        1 => "up",
        2 => "down",
        3 => "testing",
        _ => "unknown",
    }
}

/// 负载：GET laLoad.1/2/3（字符串形式浮点），解析失败容忍缺失
async fn collect_load(client: &Client<UdpHandle>) -> (Option<f64>, Option<f64>, Option<f64>) {
    let load1 = get_f64_string(client, oid!(1, 3, 6, 1, 4, 1, 2021, 10, 1, 3, 1)).await;
    let load5 = get_f64_string(client, oid!(1, 3, 6, 1, 4, 1, 2021, 10, 1, 3, 2)).await;
    let load15 = get_f64_string(client, oid!(1, 3, 6, 1, 4, 1, 2021, 10, 1, 3, 3)).await;
    (load1, load5, load15)
}

// 采集工具函数

/// bulk walk 全量收集（单条 walk 错误即中断，保留已收条目；
/// 流建立失败返回空，由调用方按"该类缺失"处理）
pub(crate) async fn walk_all(
    client: &Client<UdpHandle>,
    base: async_snmp::Oid,
) -> Vec<async_snmp::VarBind> {
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

// 上报 JSON 合成

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
            "status": n.status,
            "admin_status": n.admin_status,
            "speed_mbps": n.speed_mbps,
            "errors": {
                "in": n.err_in_delta,
                "out": n.err_out_delta,
                "in_discards": n.disc_in_delta,
                "out_discards": n.disc_out_delta,
            },
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
        // 低速虚拟口（10 bit/s）不作为校验基准
        assert!(
            iface_rate_bps(0, 37_500_000_000, 300, Some(10)).is_some(),
            "ifSpeed 过低应视为未知"
        );
        // 32 位回绕：prev=MAX−149 回绕归零 → Δ=150 字节 → 4 bit/s
        let rate = iface_rate_bps(u64::from(u32::MAX) - 149, 0, 300, Some(1_000_000)).unwrap();
        assert!((rate - 4.0).abs() < 0.01);
        // B2 回归：prev 超出 32 位空间（Counter64）时 cur<prev 属计数清零
        assert_eq!(
            iface_rate_bps(40_000_000_000, 100, 300, Some(1_000_000_000)),
            None,
            "64 位计数器 cur<prev 应判 None"
        );
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
        assert_eq!(mem_usage_pct(None), None);
        assert_eq!(mem_usage_pct(Some((0, 100))), None, "total=0 应为 None");
        assert_eq!(mem_usage_pct(Some((1000, 250))), Some(25.0));
        assert_eq!(mem_usage_pct(Some((1000, 2000))), Some(100.0), "越界钳制");
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
                ..Default::default()
            },
            NetRate {
                iface: "eth1".into(),
                rx_bps: Some(50.0),
                tx_bps: Some(20.0),
                ..Default::default()
            },
        ];
        let (rx, tx) = net_sums(&nets);
        assert_eq!(rx, Some(150.0));
        assert_eq!(tx, Some(20.0));
        let empty = [NetRate {
            iface: "eth0".into(),
            rx_bps: None,
            tx_bps: None,
            ..Default::default()
        }];
        assert_eq!(net_sums(&empty), (None, None));
    }

    #[test]
    fn 状态反序列化_缺字段降级与完整解析() {
        let full = r#"{"ts":1000,"cpu":{"total":10,"idle":8},"ifaces":{"eth0":{"in_octets":1,"out_octets":2}}}"#;
        let state: SnmpState = serde_json::from_str(full).unwrap_or_default();
        assert_eq!(state.ts, 1000);
        assert_eq!(state.cpu, Some(CpuCounters { total: 10, idle: 8 }));
        assert_eq!(state.ifaces.get("eth0").map(|c| c.in_octets), Some(1));
        // 缺字段（旧数据）→ default 降级
        let partial: SnmpState = serde_json::from_str(r#"{"ts":1000}"#).unwrap_or_default();
        assert_eq!(partial.cpu, None);
        assert!(partial.ifaces.is_empty());
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
                status: Some("up".into()),
                err_in_delta: Some(3),
                ..Default::default()
            }],
            state: SnmpState {
                ts: 1728,
                cpu: Some(CpuCounters { total: 10, idle: 8 }),
                ifaces: BTreeMap::new(),
                ..Default::default()
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
        assert_eq!(report["nets"][0]["status"], "up");
        assert_eq!(report["nets"][0]["errors"]["in"], 3);
        assert_eq!(report["sensors"][0]["kind"], "temp");
        assert_eq!(report["sensors"][0]["value"], 54.0);
        assert_eq!(report["_snmp_state"]["cpu"]["total"], 10);
        assert!(report["collected_at"].is_string());
    }

    fn state_with_status(pairs: &[(&str, i32)]) -> SnmpState {
        let mut s = SnmpState::default();
        for (k, v) in pairs {
            s.if_status.insert((*k).to_string(), *v);
        }
        s
    }

    #[test]
    fn 链路翻动_基线不发事件与方向区分() {
        let prev = SnmpState::default();
        let cur: BTreeMap<String, i32> = [("ge0/1".to_string(), 1)].into_iter().collect();
        let admin: BTreeMap<String, i32> = BTreeMap::new();
        assert!(detect_link_flips(&prev, &cur, &admin).is_empty());

        // 上一轮 up(1)，本轮 down(2) 且 admin up(1)
        let prev = state_with_status(&[("ge0/1", 1)]);
        let cur: BTreeMap<String, i32> = [("ge0/1".to_string(), 2)].into_iter().collect();
        let admin: BTreeMap<String, i32> = [("ge0/1".to_string(), 1)].into_iter().collect();
        let events = detect_link_flips(&prev, &cur, &admin);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "link_flip");
        assert_eq!(events[0].severity, "critical");
        assert_eq!(events[0].detail["direction"], "down");
        assert_eq!(events[0].detail["admin_down"], false);

        let prev = state_with_status(&[("ge0/1", 2)]);
        let cur: BTreeMap<String, i32> = [("ge0/1".to_string(), 1)].into_iter().collect();
        let events = detect_link_flips(&prev, &cur, &BTreeMap::new());
        assert_eq!(events[0].severity, "info");
        assert_eq!(events[0].detail["direction"], "up");
    }

    #[test]
    fn 链路翻动_admin_down降噪为info() {
        // oper down(2) 且 admin down(2)
        let prev = state_with_status(&[("ge0/1", 1)]);
        let cur: BTreeMap<String, i32> = [("ge0/1".to_string(), 2)].into_iter().collect();
        let admin: BTreeMap<String, i32> = [("ge0/1".to_string(), 2)].into_iter().collect();
        let events = detect_link_flips(&prev, &cur, &admin);
        assert_eq!(events[0].severity, "info");
        assert_eq!(events[0].detail["admin_down"], true);
    }

    #[test]
    fn 重启检测_回退与边界() {
        assert!(detect_reboot(None, Some(100), 300).is_none());
        assert!(detect_reboot(Some(1000), None, 300).is_none());
        assert!(detect_reboot(Some(1000), Some(1300), 300).is_none());
        // 上一轮 uptime 200s 未过 REBOOT_MIN_PREV_UPTIME_SECS 门槛
        assert!(detect_reboot(Some(200), Some(50), 300).is_none());
        // 回退：上轮 3600s、本轮 60s、间隔 300s → 停机 = 300-60 = 240s
        let ev = detect_reboot(Some(3600), Some(60), 300).unwrap_or_else(|| panic!("应检出重启"));
        assert_eq!(ev.event_type, "device_reboot");
        assert_eq!(ev.detail["downtime_secs"], 240);
        let ev = detect_reboot(Some(3600), Some(300), 100).unwrap_or_else(|| panic!("应检出重启"));
        assert_eq!(ev.detail["downtime_secs"], 0);
    }
}
