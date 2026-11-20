//! 厂商 profile 探测：一次探测确定厂商类别、资源组索引与端口清单，
//! 采集循环只对缓存索引做定向 GET。
//!
//! 厂商识别走 [`crate::vendor::identify`]（双证据纯函数），私有 OID 集来自
//! [`crate::vendor`] 注册表；公共 MIB 回退链按平台能力声明（caps）裁剪：
//! Windows 等已知缺失的 MIB 族直接跳过，不再空走（§3.2）。
//! - Cisco：CISCO-PROCESS-MIB / CISCO-MEMORY-POOL-MIB / CISCO-ENVMON-MIB；
//! - 华为：HUAWEI-ENTITY-EXTENT-MIB（详见 vendor/huawei.rs）；
//! - 华三：HH3C-ENTITY-EXT-MIB（详见 vendor/h3c.rs）；
//! - 通用回退：UCD 内存标量（net-snmp 主机，total−available 口径）、
//!   HOST-RESOURCES-MIB（hrProcessorLoad、hrStorage Ram）、
//!   ENTITY-SENSOR-MIB、LM-SENSORS-MIB。
//!
//! 探测失败（系统组无响应）返回 [`ProbeError`]；单项 MIB 缺失只降级为空
//! 来源，不影响设备纳入实时采集。
//!
//! IO 经 [`super::SnmpReader`] 抽象：生产为 async_snmp UDP 客户端，
//! 测试为内存 OID 快照，probe 全链路可 mock（§8）。

use std::collections::BTreeMap;

use async_snmp::{Oid, Value, oid};
use thiserror::Error;

use super::{
    CpuSource, DeviceProfile, EntityMem, MAX_LIVE_PORTS, MemPlan, PortMeta, RawSource, SnmpReader,
    TempSource, TempUnit, cpu_pct_value,
};
use crate::collect::metrics::temp_from_milli;
use crate::vendor::platform::PlatformCaps;
use crate::vendor::{
    self, DiscoveryProfile, MemRule, OidColumn, TempRule, VendorMemOids, VendorOidSet,
};

/// 探测失败：系统组（sysObjectID/sysDescr）无响应，设备不可达。
#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("设备系统组无响应: {0}")]
    SystemGroup(String),
}

// OID 常量（公共 MIB；厂商私有 OID 见 vendor/ 各厂商模块）

fn sys_object_id() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 1, 2, 0)
}

fn sys_descr() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 1, 1, 0)
}

// IF-MIB（ifTable 与 ifXTable 列）
fn if_name() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, 1)
}
fn if_descr() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 2)
}
fn if_type() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 3)
}
fn if_high_speed() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, 15)
}
fn if_speed() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 5)
}
fn if_hc_in() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, 6)
}

// HOST-RESOURCES-MIB
fn hr_processor_load() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 25, 3, 3, 1, 2)
}
fn hr_storage_table() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 25, 2, 3, 1)
}
fn hr_storage_ram_type() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 25, 2, 1, 2)
}

// UCD-SNMP-MIB 内存标量（KB）；可用量优先 memSysAvail(4.27，MemAvailable
// 口径，net-snmp ≥5.8)，缺失回落 memAvailReal(4.6，MemFree 口径，老内核
// 唯一可用，含缓冲缓存会高估使用率数十个百分点)
fn ucd_mem_total() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2021, 4, 5, 0)
}
fn ucd_mem_sys_avail() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2021, 4, 27, 0)
}
fn ucd_mem_avail() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2021, 4, 6, 0)
}

// ENTITY-SENSOR-MIB entSensorTable（整表 walk，后缀 [列号, 实体, 传感器]）
fn entity_sensor_table() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 99, 1, 1, 1)
}

// LM-SENSORS-MIB lmTempSensorsValue（毫摄氏度）
fn lm_temp_value() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2021, 13, 16, 2, 1, 3)
}

// 探测入口

/// 探测设备厂商 profile：系统组 GET + 端口/资源组索引 walk。
///
/// 只读、无副作用；建议每 30 分钟或索引失效时重新调用一次。
pub async fn probe<R: SnmpReader>(reader: &R) -> Result<DeviceProfile, ProbeError> {
    // 系统组两个标量分别 GET：SNMPv1 下任一 OID 不存在会让整包 GET 报错，
    // 分开请求可保留另一字段继续识别；两者都无响应才判定不可达。
    let object_id = get_one(reader, sys_object_id())
        .await
        .and_then(|value| value.as_oid().cloned());
    let descr_value = get_one(reader, sys_descr()).await;
    let descr = descr_value
        .as_ref()
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if object_id.is_none() && descr.is_none() {
        return Err(ProbeError::SystemGroup("系统组 GET 无有效值".to_string()));
    }

    let mut identity = vendor::identify(object_id.as_ref(), descr);
    // 静态证据（前缀/关键词/正则）全未命中 → 主动探测一轮 GET 比对
    //（LibreNMS snmpget 语义，见 vendor/probe.rs；每探测周期仅此一次）
    if identity.vendor_guess.is_none()
        && let Some(probed) = vendor::probe::identify_by_probes(reader).await
    {
        tracing::debug!(
            "SNMP 主动探测识别厂商: {}",
            probed.vendor_guess.unwrap_or_default()
        );
        identity = probed;
    }
    let vendor = vendor::vendor_kind(identity.vendor_guess);
    let oid_set = vendor::oid_set(identity.vendor_guess);
    let discovery = vendor::discovery_profile(identity.vendor_guess);

    let ports = discover_ports(reader).await;
    let use_hc = match ports.first() {
        Some(port) => match reader.get_many(&[if_hc_in().child(port.if_index)]).await {
            Ok(vbs) => vbs.first().is_some_and(|vb| !vb.value.is_exception()),
            Err(_) => false,
        },
        None => false,
    };

    let cpu = discover_cpu(reader, discovery, oid_set, identity.caps).await;
    let mem = discover_mem(reader, discovery, oid_set, identity.caps).await;
    let temp = discover_temp(reader, discovery, oid_set, identity.caps).await;

    tracing::debug!(
        "SNMP 实时 profile 探测完成: vendor={}, platform={}, os_version={}, ports={}, cpu={}, temp={}, mem={}",
        vendor.as_str(),
        identity.platform.unwrap_or("unknown"),
        identity.os_version.as_deref().unwrap_or("-"),
        ports.len(),
        cpu.len(),
        temp.len(),
        mem_kind(&mem)
    );

    Ok(DeviceProfile {
        vendor,
        cpu,
        mem,
        temp,
        ports,
        use_hc,
    })
}

// 端口清单

/// 物理以太网 + 聚合口类型（ethernetCsmacd/fastEthernet/gigabitEthernet/ieee8023adLag）。
const LIVE_IF_TYPES: [u32; 4] = [6, 62, 117, 161];

/// 端口静态清单：ifName/ifDescr、ifType、ifHighSpeed/ifSpeed。
async fn discover_ports<R: SnmpReader>(reader: &R) -> Vec<PortMeta> {
    let names = walk_column(reader, if_name()).await;
    let descrs = walk_column(reader, if_descr()).await;
    let types = walk_column(reader, if_type()).await;
    let high_speeds = walk_column(reader, if_high_speed()).await;
    let speeds = walk_column(reader, if_speed()).await;

    let mut indexes: Vec<u32> = names
        .keys()
        .chain(descrs.keys())
        .copied()
        .filter(|idx| {
            types
                .get(idx)
                .and_then(Value::as_u32)
                .is_some_and(|t| LIVE_IF_TYPES.contains(&t))
        })
        .collect();
    indexes.sort_unstable();
    indexes.dedup();

    let mut ports: Vec<PortMeta> = indexes
        .into_iter()
        .filter_map(|if_index| {
            let name = names
                .get(&if_index)
                .or_else(|| descrs.get(&if_index))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())?
                .to_string();
            let speed_bps = match high_speeds.get(&if_index).and_then(Value::as_u64) {
                Some(mbps) if mbps > 0 => Some(mbps.saturating_mul(1_000_000)),
                _ => speeds.get(&if_index).and_then(Value::as_u64),
            };
            Some(PortMeta {
                if_index,
                name,
                speed_bps,
            })
        })
        .collect();
    if ports.len() > MAX_LIVE_PORTS {
        tracing::debug!(
            "SNMP 实时端口数 {} 超过上限 {}，截断（按 ifIndex 排序）",
            ports.len(),
            MAX_LIVE_PORTS
        );
        ports.truncate(MAX_LIVE_PORTS);
    }
    ports
}

// CPU

/// CPU 来源探测：OS 级 LibreNMS 规则 → 品牌私有列 → hrProcessorLoad
/// （caps 裁剪已知缺失平台，如 Windows 未实现 hrProcessorTable，§3.2）。
async fn discover_cpu<R: SnmpReader>(
    reader: &R,
    discovery: Option<&DiscoveryProfile>,
    oid_set: Option<&VendorOidSet>,
    caps: PlatformCaps,
) -> Vec<CpuSource> {
    if let Some(profile) = discovery {
        for rule in profile.cpu {
            let sources = discovery_cpu_sources(reader, rule).await;
            if !sources.is_empty() {
                return sources;
            }
        }
    }
    if let Some(set) = oid_set {
        for column in set.cpu_columns {
            let sources: Vec<CpuSource> = percent_sources(reader, &column())
                .await
                .into_iter()
                .map(CpuSource::plain)
                .collect();
            if !sources.is_empty() {
                return sources;
            }
        }
    }
    if !caps.hr_processor {
        return Vec::new();
    }
    // hrProcessorLoad：每核一行，采集时取最大值
    percent_sources(reader, &hr_processor_load())
        .await
        .into_iter()
        .map(CpuSource::plain)
        .collect()
}

/// LibreNMS os_discovery CPU 列：`indexed` 表列 walk，标量定向 GET。
async fn discovery_cpu_sources<R: SnmpReader>(
    reader: &R,
    rule: &crate::vendor::CpuRule,
) -> Vec<CpuSource> {
    let column = rule.column;
    if column.indexed {
        walk_single_index(reader, &column.oid())
            .await
            .into_iter()
            .filter(|(_, v)| cpu_pct_value(v, rule.precision).is_some())
            .map(|(idx, _)| CpuSource {
                source: RawSource::indexed(column.oid(), idx),
                precision: rule.precision,
            })
            .collect()
    } else {
        let oid = column.oid();
        match get_one(reader, oid.clone()).await {
            Some(value) if cpu_pct_value(&value, rule.precision).is_some() => vec![CpuSource {
                source: RawSource::new(oid, Vec::new()),
                precision: rule.precision,
            }],
            _ => Vec::new(),
        }
    }
}

/// walk 一列百分比值，返回有效行（0..=100）的定向来源。
async fn percent_sources<R: SnmpReader>(reader: &R, column: &Oid) -> Vec<RawSource> {
    walk_column(reader, column.clone())
        .await
        .into_iter()
        .filter(|(_, v)| v.as_u32().is_some_and(|p| p <= 100))
        .map(|(idx, _)| RawSource::indexed(column.clone(), idx))
        .collect()
}

// 内存

/// 内存方案探测：厂商私有方案优先，公共链按 caps 裁剪
///（Windows 无 UCD，仅 hrStorage；未知平台全链 best-effort，现状等价）。
async fn discover_mem<R: SnmpReader>(
    reader: &R,
    discovery: Option<&DiscoveryProfile>,
    oid_set: Option<&VendorOidSet>,
    caps: PlatformCaps,
) -> MemPlan {
    if let Some(profile) = discovery {
        for rule in profile.mem {
            if let Some(plan) = discover_discovery_mem(reader, rule).await {
                return plan;
            }
        }
    }
    if let Some(set) = oid_set {
        let plan = match &set.mem {
            VendorMemOids::Pool { name, used, free } => {
                discover_pool_mem(reader, name(), used(), free()).await
            }
            VendorMemOids::EntityPercent {
                usage,
                size_mega,
                size_bytes,
            } => {
                let mega = size_mega.map(|(column, factor)| (column(), factor));
                let bytes = size_bytes.map(|column| column());
                let mega_ref = mega.as_ref().map(|(oid, factor)| (oid, *factor));
                discover_entity_mem(reader, &usage(), mega_ref, bytes.as_ref()).await
            }
        };
        if let Some(plan) = plan {
            return plan;
        }
    }
    // net-snmp 主机优先 UCD 标量：used = total − available，与 free 命令/
    // agent/5 分钟轮询口径一致；hrStorage Ram 行的 used 是 total − free 口径
    //（含缓冲缓存），直接使用会把内存使用率高估数十个百分点
    if caps.ucd_mem
        && let Some(plan) = discover_ucd_mem(reader).await
    {
        return plan;
    }
    if caps.hr_storage
        && let Some(plan) = discover_hr_mem(reader).await
    {
        return plan;
    }
    // UCD 标量兜底（探测无响应时仍按 UCD 采集，值缺失自然为空；
    // caps 已裁剪的平台不兜底，避免 Windows 每轮空 GET）
    if caps.ucd_mem {
        MemPlan::Ucd {
            total_kb: RawSource::new(ucd_mem_total(), Vec::new()),
            avail_kb: RawSource::new(ucd_mem_avail(), Vec::new()),
        }
    } else {
        MemPlan::None
    }
}

/// 把一条 LibreNMS 内存规则落成采集计划；标量 GET / 表列 walk 均失败时返回
/// `None`，由调用方继续尝试下一条规则。
async fn discover_discovery_mem<R: SnmpReader>(reader: &R, rule: &MemRule) -> Option<MemPlan> {
    match *rule {
        MemRule::Pool {
            used,
            free,
            precision,
        } => {
            if used.indexed || free.indexed {
                let used_values = walk_single_index(reader, &used.oid()).await;
                let free_values = walk_single_index(reader, &free.oid()).await;
                let index = used_values
                    .iter()
                    .filter_map(|(idx, value)| {
                        let used = value.as_u64()?;
                        let free = free_values.get(idx)?.as_u64()?;
                        Some((*idx, used.saturating_add(free)))
                    })
                    .max_by_key(|(_, total)| *total)?
                    .0;
                Some(MemPlan::Pool {
                    used: RawSource::indexed(used.oid(), index),
                    free: RawSource::indexed(free.oid(), index),
                    scale: precision,
                })
            } else {
                let used_oid = used.oid();
                let free_oid = free.oid();
                let used_value = get_one(reader, used_oid.clone()).await?.as_u64()?;
                let free_value = get_one(reader, free_oid.clone()).await?.as_u64()?;
                (used_value.saturating_add(free_value) > 0).then(|| MemPlan::Pool {
                    used: RawSource::new(used_oid, Vec::new()),
                    free: RawSource::new(free_oid, Vec::new()),
                    scale: precision,
                })
            }
        }
        MemRule::PercentTotal {
            percent,
            total,
            precision,
        } => {
            if percent.indexed || total.indexed {
                let percentages = walk_single_index(reader, &percent.oid()).await;
                let totals = walk_single_index(reader, &total.oid()).await;
                let entities = percentage_entities(&percent, &percentages, |idx| {
                    totals
                        .get(&idx)
                        .and_then(Value::as_u64)
                        .map(|bytes| bytes.saturating_mul(precision))
                        .filter(|bytes| *bytes > 0)
                });
                (!entities.is_empty()).then_some(MemPlan::EntityPercent { entities })
            } else {
                let percent_oid = percent.oid();
                let total_oid = total.oid();
                // 百分比必须存在且在 0–100 内，否则该规则不可用
                get_one(reader, percent_oid.clone())
                    .await?
                    .as_u32()
                    .filter(|p| *p <= 100)?;
                let total_bytes = get_one(reader, total_oid.clone())
                    .await
                    .and_then(|value| value.as_u64())
                    .map(|bytes| bytes.saturating_mul(precision))
                    .filter(|bytes| *bytes > 0);
                Some(MemPlan::EntityPercent {
                    entities: vec![EntityMem {
                        usage: RawSource::new(percent_oid, Vec::new()),
                        total_bytes,
                    }],
                })
            }
        }
        MemRule::Percent { percent } => {
            let entities = if percent.indexed {
                let percentages = walk_single_index(reader, &percent.oid()).await;
                percentage_entities(&percent, &percentages, |_| None)
            } else {
                let oid = percent.oid();
                match get_one(reader, oid.clone()).await {
                    Some(value) if value.as_u32().is_some_and(|p| p <= 100) => vec![EntityMem {
                        usage: RawSource::new(oid, Vec::new()),
                        total_bytes: None,
                    }],
                    _ => Vec::new(),
                }
            };
            (!entities.is_empty()).then_some(MemPlan::EntityPercent { entities })
        }
        MemRule::TotalFree {
            total,
            free,
            precision,
        } => {
            let (total_source, free_source) = pair_plan(reader, &total, &free).await?;
            Some(MemPlan::TotalFree {
                total: total_source,
                free: free_source,
                scale: precision,
            })
        }
        MemRule::UsedTotal {
            total,
            used,
            precision,
        } => {
            let (total_source, used_source) = pair_plan(reader, &total, &used).await?;
            Some(MemPlan::UsedTotal {
                total: total_source,
                used: used_source,
                scale: precision,
            })
        }
    }
}

/// 百分比实体表：只保留 0..=100 的行，总量由调用方按行索引提供。
fn percentage_entities(
    column: &OidColumn,
    values: &BTreeMap<u32, Value>,
    total_for: impl Fn(u32) -> Option<u64>,
) -> Vec<EntityMem> {
    values
        .iter()
        .filter(|(_, value)| value.as_u32().is_some_and(|pct| pct <= 100))
        .map(|(index, _)| EntityMem {
            usage: RawSource::indexed(column.oid(), *index),
            total_bytes: total_for(*index),
        })
        .collect()
}

/// 双列内存（total/free 或 total/used）：表列取总量最大的行（要求 total 列
/// 不小于另一列），标量直接 GET。返回（total 来源, 另一列来源）。
async fn pair_plan<R: SnmpReader>(
    reader: &R,
    total: &OidColumn,
    other: &OidColumn,
) -> Option<(RawSource, RawSource)> {
    if total.indexed || other.indexed {
        let total_values = walk_single_index(reader, &total.oid()).await;
        let other_values = walk_single_index(reader, &other.oid()).await;
        let index = total_values
            .iter()
            .filter_map(|(idx, value)| {
                let total = value.as_u64()?;
                let other = other_values.get(idx)?.as_u64()?;
                (total >= other).then_some((*idx, total))
            })
            .max_by_key(|(_, total)| *total)?
            .0;
        Some((
            RawSource::indexed(total.oid(), index),
            RawSource::indexed(other.oid(), index),
        ))
    } else {
        let total_oid = total.oid();
        let other_oid = other.oid();
        let total_value = get_one(reader, total_oid.clone()).await?.as_u64()?;
        let other_value = get_one(reader, other_oid.clone()).await?.as_u64()?;
        (total_value > 0 && total_value >= other_value).then(|| {
            (
                RawSource::new(total_oid, Vec::new()),
                RawSource::new(other_oid, Vec::new()),
            )
        })
    }
}

/// UCD 内存标量探测：total 与可用量标量 GET 均有效且 total_kb > 0 才采用。
/// 可用量优先 memSysAvail(4.27，MemAvailable 口径，net-snmp ≥5.8)，
/// 缺失回落 memAvailReal(4.6，MemFree 口径，老 net-snmp 唯一可用)。
async fn discover_ucd_mem<R: SnmpReader>(reader: &R) -> Option<MemPlan> {
    let total_oid = ucd_mem_total();
    for avail_oid in [ucd_mem_sys_avail(), ucd_mem_avail()] {
        if let Some(plan) = probe_ucd_pair(reader, &total_oid, &avail_oid).await {
            return Some(plan);
        }
    }
    None
}

/// 双标量探测：GET 均有效且 total_kb > 0 时返回计划（值在采集轮经同一
/// RawSource 读取）。
async fn probe_ucd_pair<R: SnmpReader>(
    reader: &R,
    total_oid: &Oid,
    avail_oid: &Oid,
) -> Option<MemPlan> {
    // 分开启用以保留部分字段：rev1 设备只有一个计数器时不至于整轮失败。
    let total_kb = get_one(reader, total_oid.clone()).await?.as_u64()?;
    get_one(reader, avail_oid.clone())
        .await
        .and_then(|value| value.as_u64())?;
    (total_kb > 0).then(|| MemPlan::Ucd {
        total_kb: RawSource::new(total_oid.clone(), Vec::new()),
        avail_kb: RawSource::new(avail_oid.clone(), Vec::new()),
    })
}

/// CISCO-MEMORY-POOL-MIB 内存池：name 定位 Processor 行（缺失取首行；
/// 名称列整列缺失时以 used 列存在行为准）。
async fn discover_pool_mem<R: SnmpReader>(
    reader: &R,
    name_col: Oid,
    used_col: Oid,
    free_col: Oid,
) -> Option<MemPlan> {
    let names = walk_column(reader, name_col).await;
    let index = match names
        .iter()
        .find(|(_, v)| {
            v.as_str()
                .is_some_and(|s| s.to_ascii_lowercase().contains("processor"))
        })
        .map(|(idx, _)| *idx)
        .or_else(|| names.keys().next().copied())
    {
        Some(index) => index,
        None => walk_column(reader, used_col.clone())
            .await
            .keys()
            .next()
            .copied()?,
    };
    Some(MemPlan::Pool {
        used: RawSource::indexed(used_col, index),
        free: RawSource::indexed(free_col, index),
        scale: 1,
    })
}

/// 华为/华三实体表内存：usage 百分比 + size 列缓存总量（优先 MB 口径）。
async fn discover_entity_mem<R: SnmpReader>(
    reader: &R,
    usage_column: &Oid,
    size_mega: Option<(&Oid, u64)>,
    size_bytes: Option<&Oid>,
) -> Option<MemPlan> {
    let usage = walk_column(reader, usage_column.clone()).await;
    let usage_indexes: Vec<u32> = usage
        .iter()
        .filter(|(_, v)| v.as_u32().is_some_and(|p| p <= 100))
        .map(|(idx, _)| *idx)
        .collect();
    if usage_indexes.is_empty() {
        return None;
    }
    let mega = match size_mega {
        Some((column, factor)) => walk_column(reader, column.clone())
            .await
            .into_iter()
            .filter_map(|(idx, v)| v.as_u64().map(|n| (idx, n.saturating_mul(factor))))
            .collect::<BTreeMap<_, _>>(),
        None => BTreeMap::new(),
    };
    let bytes = match size_bytes {
        Some(column) => walk_column(reader, column.clone())
            .await
            .into_iter()
            .filter_map(|(idx, v)| v.as_u64().map(|n| (idx, n)))
            .collect::<BTreeMap<_, _>>(),
        None => BTreeMap::new(),
    };
    let entities = usage_indexes
        .into_iter()
        .map(|idx| EntityMem {
            usage: RawSource::indexed(usage_column.clone(), idx),
            total_bytes: mega
                .get(&idx)
                .or_else(|| bytes.get(&idx))
                .copied()
                .filter(|b| *b > 0),
        })
        .collect();
    Some(MemPlan::EntityPercent { entities })
}

/// HOST-RESOURCES-MIB：hrStorageTable Ram 行（整表 walk 一次，取 allocation units）。
async fn discover_hr_mem<R: SnmpReader>(reader: &R) -> Option<MemPlan> {
    let varbinds = reader.walk(&hr_storage_table()).await;
    let mut types: BTreeMap<u32, Oid> = BTreeMap::new();
    let mut units: BTreeMap<u32, u64> = BTreeMap::new();
    for vb in &varbinds {
        let arcs = vb.oid.arcs();
        if arcs.len() != 12 {
            continue;
        }
        let (column, index) = (arcs[10], arcs[11]);
        match column {
            2 => {
                if let Some(oid) = vb.value.as_oid() {
                    types.insert(index, oid.clone());
                }
            }
            4 => {
                if let Some(v) = vb.value.as_u64() {
                    units.insert(index, v);
                }
            }
            _ => {}
        }
    }
    let ram = types
        .iter()
        .find(|(_, oid)| **oid == hr_storage_ram_type())
        .map(|(idx, _)| *idx)?;
    let units = units.get(&ram).copied().filter(|u| *u > 0)?;
    Some(MemPlan::HrStorage {
        size: RawSource::indexed(oid!(1, 3, 6, 1, 2, 1, 25, 2, 3, 1, 5), ram),
        used: RawSource::indexed(oid!(1, 3, 6, 1, 2, 1, 25, 2, 3, 1, 6), ram),
        units,
    })
}

// 温度

/// 温度来源探测：厂商私有列 → ENTITY-SENSOR-MIB → LM-SENSORS-MIB，
/// 按 caps 裁剪已知缺失族（任一命中即止）。
async fn discover_temp<R: SnmpReader>(
    reader: &R,
    discovery: Option<&DiscoveryProfile>,
    oid_set: Option<&VendorOidSet>,
    caps: PlatformCaps,
) -> Vec<TempSource> {
    if let Some(profile) = discovery {
        // 单 OS 可能声明几十条传感器（含光模块/电源等），逐条 walk 成本高；
        // 取前若干条，命中即止。未命中的设备仍由品牌库/公共链兜底。
        for rule in profile.temp.iter().take(MAX_DISCOVERY_TEMP_RULES) {
            let sources = discovery_temp_sources(reader, rule).await;
            if !sources.is_empty() {
                return sources;
            }
        }
    }
    if let Some(set) = oid_set {
        for column in set.temp_columns {
            let sources = celsius_sources(reader, &column()).await;
            if !sources.is_empty() {
                return sources;
            }
        }
    }
    if caps.entity_sensors {
        let sensors = discover_entity_sensors(reader).await;
        if !sensors.is_empty() {
            return sensors;
        }
    }
    if caps.lm_sensors {
        discover_lm_sensors(reader).await
    } else {
        Vec::new()
    }
}

/// 单 OS 温度规则最多尝试条数（避免光模块类设备探测期 walk 次数过多）。
const MAX_DISCOVERY_TEMP_RULES: usize = 12;

/// LibreNMS os_discovery 温度列：支持 divisor/multiplier 换算。
async fn discovery_temp_sources<R: SnmpReader>(reader: &R, rule: &TempRule) -> Vec<TempSource> {
    let unit = if rule.divisor == 1 && rule.multiplier == 1 {
        TempUnit::Celsius
    } else {
        TempUnit::Scaled {
            divisor: rule.divisor,
            multiplier: rule.multiplier,
        }
    };
    let column = rule.column;
    if column.indexed {
        walk_single_index(reader, &column.oid())
            .await
            .into_iter()
            .filter(|(_, value)| valid_temp(value, unit))
            .map(|(idx, _)| TempSource {
                source: RawSource::indexed(column.oid(), idx),
                unit,
            })
            .collect()
    } else {
        let oid = column.oid();
        match get_one(reader, oid.clone()).await {
            Some(value) if valid_temp(&value, unit) => vec![TempSource {
                source: RawSource::new(oid, Vec::new()),
                unit,
            }],
            _ => Vec::new(),
        }
    }
}

fn valid_temp(value: &Value, unit: TempUnit) -> bool {
    temp_raw(value)
        .and_then(|raw| apply_temp_unit(raw, unit))
        .is_some_and(|celsius| 0.0 < celsius && celsius <= 200.0)
}

/// 厂商温度列：直接摄氏度，过滤无效读数（0 常为占位、>200 明显越界）。
async fn celsius_sources<R: SnmpReader>(reader: &R, column: &Oid) -> Vec<TempSource> {
    walk_column(reader, column.clone())
        .await
        .into_iter()
        .filter(|(_, v)| temp_value(v).is_some())
        .map(|(idx, _)| TempSource {
            source: RawSource::indexed(column.clone(), idx),
            unit: TempUnit::Celsius,
        })
        .collect()
}

/// ENTITY-SENSOR-MIB：整表 walk，按 status=ok 且换算后温度合理的行建来源。
async fn discover_entity_sensors<R: SnmpReader>(reader: &R) -> Vec<TempSource> {
    let varbinds = reader.walk(&entity_sensor_table()).await;
    let mut scales: BTreeMap<u32, i32> = BTreeMap::new();
    let mut precisions: BTreeMap<u32, i32> = BTreeMap::new();
    let mut values: BTreeMap<u32, u64> = BTreeMap::new();
    let mut statuses: BTreeMap<u32, u32> = BTreeMap::new();
    for vb in &varbinds {
        let arcs = vb.oid.arcs();
        // 表基 10 弧 + entry(1) + 列号 + entPhysicalIndex（RFC 3433 单弧索引）
        if arcs.len() != 13 {
            continue;
        }
        let key = arcs[12];
        match arcs[11] {
            2 => {
                if let Some(v) = vb.value.as_u32() {
                    scales.insert(key, v as i32);
                }
            }
            3 => {
                if let Some(v) = vb.value.as_u32() {
                    precisions.insert(key, v as i32);
                }
            }
            4 => {
                if let Some(v) = vb.value.as_u64() {
                    values.insert(key, v);
                }
            }
            5 => {
                if let Some(v) = vb.value.as_u32() {
                    statuses.insert(key, v);
                }
            }
            _ => {}
        }
    }
    values
        .into_iter()
        .filter(|(key, _)| statuses.get(key) == Some(&1))
        .filter_map(|(key, raw)| {
            let scale = scales.get(&key).copied()?;
            let precision = precisions.get(&key).copied().unwrap_or(0);
            let unit = TempUnit::EntitySensor { scale, precision };
            let celsius = apply_temp_unit(raw as f64, unit)?;
            (0.0 < celsius && celsius <= 200.0).then_some(TempSource {
                source: RawSource::indexed(oid!(1, 3, 6, 1, 2, 1, 99, 1, 1, 1, 4), key),
                unit,
            })
        })
        .collect()
}

/// LM-SENSORS-MIB：毫摄氏度列。
async fn discover_lm_sensors<R: SnmpReader>(reader: &R) -> Vec<TempSource> {
    walk_column(reader, lm_temp_value())
        .await
        .into_iter()
        .filter(|(_, v)| v.as_u64().and_then(temp_from_milli).is_some())
        .map(|(idx, _)| TempSource {
            source: RawSource::indexed(lm_temp_value(), idx),
            unit: TempUnit::MilliCelsius,
        })
        .collect()
}

// 工具

/// walk 一列并返回 `行索引 → 值`（异常值跳过）。
async fn walk_column<R: SnmpReader>(reader: &R, base: Oid) -> BTreeMap<u32, Value> {
    let mut map = BTreeMap::new();
    for vb in reader.walk(&base).await {
        if vb.value.is_exception() {
            continue;
        }
        if let Some(&index) = vb.oid.arcs().last() {
            map.insert(index, vb.value);
        }
    }
    map
}

/// 温度原始值（可能为负或带小数换算系数，不在本函数过滤范围）。
fn temp_raw(value: &Value) -> Option<f64> {
    value
        .as_i32()
        .map(f64::from)
        .or_else(|| value.as_u64().map(|v| v as f64))
}

/// 整数温度值（摄氏度）转 f64：0/越界视为无效。
fn temp_value(value: &Value) -> Option<f64> {
    let raw = temp_raw(value)?;
    (0.0 < raw && raw <= 200.0).then_some(raw)
}

/// 定向 GET 单值（异常值视为缺失）。
async fn get_one<R: SnmpReader>(reader: &R, oid: Oid) -> Option<Value> {
    reader
        .get_many(&[oid])
        .await
        .ok()?
        .into_iter()
        .next()
        .filter(|vb| !vb.value.is_exception())
        .map(|vb| vb.value)
}

/// walk 一列，只接受「基 OID + 单弧索引」的行；复合索引整行跳过，
/// 避免把多弧实例误当成单弧索引后又构造出错误 OID。
async fn walk_single_index<R: SnmpReader>(reader: &R, base: &Oid) -> BTreeMap<u32, Value> {
    let base_len = base.arcs().len();
    let mut map = BTreeMap::new();
    for vb in reader.walk(base).await {
        if vb.value.is_exception() {
            continue;
        }
        let arcs = vb.oid.arcs();
        if arcs.len() == base_len + 1
            && let Some(&index) = arcs.last()
        {
            map.insert(index, vb.value);
        }
    }
    map
}

/// ENTITY-SENSOR-MIB 换算：value × 10^(scale-9) / 10^precision。
pub(crate) fn apply_temp_unit(raw: f64, unit: TempUnit) -> Option<f64> {
    let celsius = match unit {
        TempUnit::Celsius => raw,
        TempUnit::MilliCelsius => raw / 1000.0,
        TempUnit::EntitySensor { scale, precision } => {
            let factor = entity_scale_factor(scale)? / 10f64.powi(precision);
            raw * factor
        }
        TempUnit::Scaled {
            divisor,
            multiplier,
        } => {
            if divisor == 0 {
                return None;
            }
            raw * multiplier as f64 / divisor as f64
        }
    };
    celsius.is_finite().then_some(celsius)
}

/// ENTITY-SENSOR-MIB entSensorScale（1=yocto … 9=units … 17=yotta）指数因子。
///
/// 枚举按 3 的步长递增：units(9)=10^0、milli(8)=10^-3、kilo(10)=10^3。
fn entity_scale_factor(scale: i32) -> Option<f64> {
    let exponent = 3 * (scale - 9);
    (-24..=24).contains(&exponent).then(|| 10f64.powi(exponent))
}

fn mem_kind(plan: &MemPlan) -> &'static str {
    match plan {
        MemPlan::None => "none",
        MemPlan::Pool { .. } => "pool",
        MemPlan::EntityPercent { .. } => "entity-percent",
        MemPlan::TotalFree { .. } => "total-free",
        MemPlan::UsedTotal { .. } => "used-total",
        MemPlan::HrStorage { .. } => "hr-storage",
        MemPlan::Ucd { .. } => "ucd",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::VendorKind;
    use async_snmp::{Oid, Value, VarBind};

    /// 内存 OID 快照 mock（§8）：get 精确匹配，walk 返回子树（含子弧）。
    struct MockSnmpReader {
        data: Vec<(Oid, Value)>,
    }

    impl MockSnmpReader {
        fn new(data: Vec<(Oid, Value)>) -> Self {
            Self { data }
        }
    }

    impl SnmpReader for MockSnmpReader {
        async fn get_many(&self, oids: &[Oid]) -> Result<Vec<VarBind>, String> {
            oids.iter()
                .map(|oid| {
                    self.data
                        .iter()
                        .find(|(o, _)| o == oid)
                        .map(|(o, v)| VarBind::new(o.clone(), v.clone()))
                        .ok_or_else(|| "noSuchName".to_string())
                })
                .collect()
        }

        async fn walk(&self, base: &Oid) -> Vec<VarBind> {
            self.data
                .iter()
                .filter(|(o, _)| o.starts_with(base))
                .map(|(o, v)| VarBind::new(o.clone(), v.clone()))
                .collect()
        }
    }

    fn oid(s: &str) -> Oid {
        Oid::parse(s).unwrap()
    }

    fn snmp_str(s: &str) -> Value {
        Value::from(s)
    }

    #[tokio::test]
    async fn 探测_华为设备全链路() {
        let data = vec![
            (
                oid("1.3.6.1.2.1.1.2.0"),
                Value::ObjectIdentifier(oid("1.3.6.1.4.1.2011.2.23.1")),
            ),
            (
                oid("1.3.6.1.2.1.1.1.0"),
                snmp_str("Huawei S5720 Versatile Routing Platform Software"),
            ),
            // 端口：ifName/ifType/ifHighSpeed
            (oid("1.3.6.1.2.1.31.1.1.1.1.1"), snmp_str("GE0/0/1")),
            (oid("1.3.6.1.2.1.2.2.1.3.1"), Value::Integer(6)),
            (oid("1.3.6.1.2.1.31.1.1.1.15.1"), Value::Gauge32(1000)),
            // hwEntityCpuUsage(.5)/MemUsage(.7)/MemSizeMega(.19)/Temperature(.11)
            (
                oid("1.3.6.1.4.1.2011.5.25.31.1.1.1.1.5.9"),
                Value::Integer(12),
            ),
            (
                oid("1.3.6.1.4.1.2011.5.25.31.1.1.1.1.7.9"),
                Value::Integer(45),
            ),
            (
                oid("1.3.6.1.4.1.2011.5.25.31.1.1.1.1.19.9"),
                Value::Integer(1024),
            ),
            (
                oid("1.3.6.1.4.1.2011.5.25.31.1.1.1.1.11.9"),
                Value::Integer(38),
            ),
        ];
        let profile = probe(&MockSnmpReader::new(data)).await.unwrap();
        assert_eq!(profile.vendor, VendorKind::Huawei);
        assert_eq!(profile.ports.len(), 1);
        assert_eq!(profile.ports[0].name, "GE0/0/1");
        assert!(!profile.cpu.is_empty());
        assert!(!profile.temp.is_empty());
        assert!(matches!(profile.mem, MemPlan::EntityPercent { .. }));
    }

    #[tokio::test]
    async fn 内存规则_total_free标量取两列() {
        let data = vec![
            (oid("1.3.6.1.4.1.9.9.48.1.1.1.5.0"), Value::Gauge32(1000)),
            (oid("1.3.6.1.4.1.9.9.48.1.1.1.6.0"), Value::Gauge32(400)),
        ];
        let reader = MockSnmpReader::new(data);
        let total = OidColumn {
            base: &[1, 3, 6, 1, 4, 1, 9, 9, 48, 1, 1, 1, 5, 0],
            indexed: false,
        };
        let free = OidColumn {
            base: &[1, 3, 6, 1, 4, 1, 9, 9, 48, 1, 1, 1, 6, 0],
            indexed: false,
        };
        let (total_source, free_source) = pair_plan(&reader, &total, &free).await.unwrap();
        assert_eq!(total_source.oid(), total.oid());
        assert_eq!(free_source.oid(), free.oid());
    }

    #[tokio::test]
    async fn 探测_非品牌os走librenms采集规则() {
        // Fortigate 不在内置品牌库中，CPU/内存/温度全部来自 os_discovery
        let data = vec![
            (
                oid("1.3.6.1.2.1.1.2.0"),
                Value::ObjectIdentifier(oid("1.3.6.1.4.1.12356.101.1.1")),
            ),
            (
                oid("1.3.6.1.2.1.1.1.0"),
                snmp_str("Fortinet FortiGate-100F v7.4"),
            ),
            // fgSysCpuUsage(.4.1.3.0)
            (oid("1.3.6.1.4.1.12356.101.4.1.3.0"), Value::Integer(23)),
            // fgSysMemUsage(.4.1.4.0) / fgSysMemCapacity(.4.1.5.0)，单位 KB
            (oid("1.3.6.1.4.1.12356.101.4.1.4.0"), Value::Integer(40)),
            (oid("1.3.6.1.4.1.12356.101.4.1.5.0"), Value::Integer(1000)),
            // fgHwSensorEntValue(.4.3.2.1.3.1)
            (oid("1.3.6.1.4.1.12356.101.4.3.2.1.3.1"), Value::Integer(41)),
        ];
        let profile = probe(&MockSnmpReader::new(data)).await.unwrap();
        assert_eq!(profile.vendor, VendorKind::Generic);
        assert_eq!(profile.cpu.len(), 1);
        assert_eq!(profile.cpu[0].precision, 1);
        assert_eq!(profile.temp.len(), 1);
        match profile.mem {
            MemPlan::EntityPercent { entities } => {
                assert_eq!(entities.len(), 1);
                // 1000 KB × 1024 = 1,024,000 bytes
                assert_eq!(entities[0].total_bytes, Some(1_024_000));
            }
            other => panic!("应使用 os_discovery 百分比内存方案: {other:?}"),
        }
    }

    #[tokio::test]
    async fn 探测_cisco设备内存池() {
        let data = vec![
            (
                oid("1.3.6.1.2.1.1.2.0"),
                Value::ObjectIdentifier(oid("1.3.6.1.4.1.9.1.1208")),
            ),
            (
                oid("1.3.6.1.2.1.1.1.0"),
                snmp_str("Cisco IOS Software, C2960"),
            ),
            // cpmCPUTotal1minRev / ENVMON 温度
            (oid("1.3.6.1.4.1.9.9.109.1.1.1.1.7.1"), Value::Integer(20)),
            (oid("1.3.6.1.4.1.9.9.13.1.3.1.3.1"), Value::Integer(30)),
            // ciscoMemoryPoolTable：name/used/free
            (oid("1.3.6.1.4.1.9.9.48.1.1.1.2.1"), snmp_str("Processor")),
            (oid("1.3.6.1.4.1.9.9.48.1.1.1.5.1"), Value::Gauge32(1000)),
            (oid("1.3.6.1.4.1.9.9.48.1.1.1.6.1"), Value::Gauge32(2000)),
        ];
        let profile = probe(&MockSnmpReader::new(data)).await.unwrap();
        assert_eq!(profile.vendor, VendorKind::Cisco);
        assert!(!profile.cpu.is_empty());
        assert!(!profile.temp.is_empty());
        assert!(matches!(profile.mem, MemPlan::Pool { .. }));
    }

    #[tokio::test]
    async fn 探测_linux主机走通用链() {
        let data = vec![
            (
                oid("1.3.6.1.2.1.1.2.0"),
                Value::ObjectIdentifier(oid("1.3.6.1.4.1.8072.3.2.10")),
            ),
            (
                oid("1.3.6.1.2.1.1.1.0"),
                snmp_str("Linux web01 5.15.0-91-generic x86_64"),
            ),
            // 端口 + ifHCInOctets（use_hc 探测单点）
            (oid("1.3.6.1.2.1.31.1.1.1.1.2"), snmp_str("eth0")),
            (oid("1.3.6.1.2.1.2.2.1.3.2"), Value::Integer(6)),
            (oid("1.3.6.1.2.1.31.1.1.1.6.2"), Value::Counter32(100)),
            // hrProcessorLoad 回退
            (oid("1.3.6.1.2.1.25.3.3.1.2.1"), Value::Integer(7)),
            // UCD 内存标量
            (oid("1.3.6.1.4.1.2021.4.5.0"), Value::Integer(8000000)),
            (oid("1.3.6.1.4.1.2021.4.27.0"), Value::Integer(4000000)),
        ];
        let profile = probe(&MockSnmpReader::new(data)).await.unwrap();
        assert_eq!(profile.vendor, VendorKind::Generic);
        assert_eq!(profile.ports.len(), 1);
        assert!(profile.use_hc);
        assert!(!profile.cpu.is_empty(), "hrProcessorLoad 回退生效");
        assert!(matches!(profile.mem, MemPlan::Ucd { .. }));
    }

    #[tokio::test]
    async fn 探测_windows仅hr_storage() {
        let data = vec![
            (
                oid("1.3.6.1.2.1.1.2.0"),
                Value::ObjectIdentifier(oid("1.3.6.1.4.1.311.1.1.3.1")),
            ),
            (
                oid("1.3.6.1.2.1.1.1.0"),
                snmp_str("Microsoft Windows Version 10.0 (Build 19045)"),
            ),
            // hrStorageTable：type(Ram) + allocationUnits
            (
                oid("1.3.6.1.2.1.25.2.3.1.2.1"),
                Value::ObjectIdentifier(oid("1.3.6.1.2.1.25.2.1.2")),
            ),
            (oid("1.3.6.1.2.1.25.2.3.1.4.1"), Value::Integer(1024)),
        ];
        let profile = probe(&MockSnmpReader::new(data)).await.unwrap();
        assert_eq!(profile.vendor, VendorKind::Generic);
        // caps 裁剪：无 UCD/lm_sensors/hrProcessor，内存仅 hrStorage
        assert!(matches!(profile.mem, MemPlan::HrStorage { .. }));
        assert!(profile.cpu.is_empty(), "Windows 无 hrProcessor，不得空探测");
        assert!(profile.temp.is_empty());
    }

    #[tokio::test]
    async fn 探测_系统组无响应报错() {
        let result = probe(&MockSnmpReader::new(vec![])).await;
        assert!(matches!(result, Err(ProbeError::SystemGroup(_))));
    }

    #[test]
    fn 传感器换算_毫与实体比例() {
        assert_eq!(apply_temp_unit(45000.0, TempUnit::MilliCelsius), Some(45.0));
        // scale=8(milli) precision=0 → 45000 × 10^-3 = 45
        assert_eq!(
            apply_temp_unit(
                45000.0,
                TempUnit::EntitySensor {
                    scale: 8,
                    precision: 0
                }
            ),
            Some(45.0)
        );
        // scale=9(units) precision=1 → 4550 / 10 = 455
        assert_eq!(
            apply_temp_unit(
                4550.0,
                TempUnit::EntitySensor {
                    scale: 9,
                    precision: 1
                }
            ),
            Some(455.0)
        );
        // 越界 scale 拒绝
        assert_eq!(
            apply_temp_unit(
                1.0,
                TempUnit::EntitySensor {
                    scale: 99,
                    precision: 0
                }
            ),
            None
        );
    }

    #[test]
    fn 温度读数_过滤零与越界() {
        assert_eq!(temp_value(&Value::Integer(45)), Some(45.0));
        assert_eq!(temp_value(&Value::Integer(0)), None);
        assert_eq!(temp_value(&Value::Integer(65535)), None);
        assert_eq!(temp_value(&Value::Integer(-5)), None);
        assert_eq!(temp_value(&Value::Gauge32(52)), Some(52.0));
    }
}
