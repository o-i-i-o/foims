//! 厂商 profile 探测：一次 walk 确定厂商类别、资源组索引与端口清单，
//! 采集循环只对缓存索引做定向 GET。
//!
//! 厂商 OID（依据厂商 MIB 文档）：
//! - Cisco：CISCO-PROCESS-MIB cpmCPUTotal1minRev、CISCO-MEMORY-POOL-MIB
//!   used/free、CISCO-ENVMON-MIB 温度；
//! - 华为：HUAWEI-ENTITY-EXTENT-MIB hwEntityCpuUsage(.5)/hwEntityMemUsage(.7)/
//!   hwEntityMemSize(.9，字节)/hwEntityMemSizeMega(.19，MB)/hwEntityTemperature(.11)；
//! - 华三：HH3C-ENTITY-EXT-MIB hh3cEntityExtCpuUsage(.6)/hh3cEntityExtMemUsage(.8)/
//!   hh3cEntityExtMemSize(.10，字节)/hh3cEntityExtTemperature(.12)；
//! - 通用回退：HOST-RESOURCES-MIB（hrProcessorLoad、hrStorage Ram）、
//!   UCD 内存标量、ENTITY-SENSOR-MIB、LM-SENSORS-MIB。
//!
//! 探测失败（系统组无响应）返回 [`ProbeError`]；单项 MIB 缺失只降级为空
//! 来源，不影响设备纳入实时采集。

use std::collections::BTreeMap;

use async_snmp::transport::UdpHandle;
use async_snmp::{Client, Oid, Value, oid};
use thiserror::Error;

use super::{
    DeviceProfile, EntityMem, MAX_LIVE_PORTS, MemPlan, PortMeta, RawSource, TempSource, TempUnit,
    VendorKind,
};
use crate::collect::metrics::{temp_from_milli, walk_all};
use crate::core::format_snmp_error;

/// 探测失败：系统组（sysObjectID/sysDescr）无响应，设备不可达。
#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("设备系统组无响应: {0}")]
    SystemGroup(String),
}

// ============================================================
// OID 常量
// ============================================================

fn sys_object_id() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 1, 2, 0)
}

fn sys_descr() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 1, 1, 0)
}

// IF-MIB / IF-MIB ifXTable
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

// CISCO-PROCESS-MIB
fn cisco_cpm_cpu_1min() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 9, 9, 109, 1, 1, 1, 1, 7)
}
fn cisco_cpm_cpu_5min() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 9, 9, 109, 1, 1, 1, 1, 8)
}
// CISCO-MEMORY-POOL-MIB
fn cisco_mem_pool_name() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 9, 9, 48, 1, 1, 1, 2)
}
fn cisco_mem_pool_used() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 9, 9, 48, 1, 1, 1, 5)
}
fn cisco_mem_pool_free() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 9, 9, 48, 1, 1, 1, 6)
}
// CISCO-ENVMON-MIB
fn cisco_envmon_temp() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 9, 9, 13, 1, 3, 1, 3)
}

// HUAWEI-ENTITY-EXTENT-MIB
fn huawei_cpu() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 5)
}
fn huawei_mem_usage() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 7)
}
fn huawei_mem_size() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 9)
}
fn huawei_mem_size_mega() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 19)
}
fn huawei_temp() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2011, 5, 25, 31, 1, 1, 1, 1, 11)
}

// HH3C-ENTITY-EXT-MIB
fn h3c_cpu() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 25506, 2, 6, 1, 1, 1, 1, 6)
}
fn h3c_mem_usage() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 25506, 2, 6, 1, 1, 1, 1, 8)
}
fn h3c_mem_size() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 25506, 2, 6, 1, 1, 1, 1, 10)
}
fn h3c_temp() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 25506, 2, 6, 1, 1, 1, 1, 12)
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

// UCD-SNMP-MIB 内存标量（KB）
fn ucd_mem_total() -> Oid {
    oid!(1, 3, 6, 1, 4, 1, 2021, 4, 5, 0)
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

// ============================================================
// 探测入口
// ============================================================

/// 探测设备厂商 profile：系统组 GET + 端口/资源组索引 walk。
///
/// 只读、无副作用；建议每 30 分钟或索引失效时重新调用一次。
pub async fn probe(client: &Client<UdpHandle>) -> Result<DeviceProfile, ProbeError> {
    let response = client
        .get_many(&[sys_object_id(), sys_descr()])
        .await
        .map_err(|e| ProbeError::SystemGroup(format_snmp_error(e)))?;
    let object_id = response
        .varbinds
        .iter()
        .find(|vb| vb.oid == sys_object_id())
        .and_then(|vb| vb.value.as_oid().cloned());
    let descr = response
        .varbinds
        .iter()
        .find(|vb| vb.oid == sys_descr())
        .and_then(|vb| vb.value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let vendor = detect_vendor(object_id.as_ref(), descr);
    let ports = discover_ports(client).await;
    let use_hc = match ports.first() {
        Some(port) => match client.get(&if_hc_in().child(port.if_index)).await {
            Ok(resp) => resp
                .varbinds
                .first()
                .is_some_and(|vb| !vb.value.is_exception()),
            Err(_) => false,
        },
        None => false,
    };

    let cpu = discover_cpu(client, vendor).await;
    let mem = discover_mem(client, vendor).await;
    let temp = discover_temp(client, vendor).await;

    tracing::debug!(
        "SNMP 实时 profile 探测完成: vendor={}, ports={}, cpu={}, temp={}, mem={}",
        vendor.as_str(),
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

/// 厂商识别：优先 sysObjectID 企业号，回落 sysDescr 关键字。
fn detect_vendor(object_id: Option<&Oid>, descr: Option<&str>) -> VendorKind {
    if let Some(oid) = object_id {
        let arcs = oid.arcs();
        if arcs.len() >= 7 && arcs.starts_with(&[1, 3, 6, 1, 4, 1]) {
            match arcs[6] {
                9 => return VendorKind::Cisco,
                2011 => return VendorKind::Huawei,
                25506 => return VendorKind::H3c,
                // HPE/H3C Comware 7 设备部分沿用惠普企业号 11，需描述串确认
                11 => {
                    let d = descr.unwrap_or_default().to_ascii_lowercase();
                    if d.contains("comware") || d.contains("h3c") {
                        return VendorKind::H3c;
                    }
                }
                _ => {}
            }
        }
    }
    let d = descr.unwrap_or_default().to_ascii_lowercase();
    if d.contains("cisco") {
        VendorKind::Cisco
    } else if d.contains("huawei") || d.contains("versatile routing platform") {
        VendorKind::Huawei
    } else if d.contains("h3c") || d.contains("comware") {
        VendorKind::H3c
    } else {
        VendorKind::Generic
    }
}

// ============================================================
// 端口清单
// ============================================================

/// 物理以太网 + 聚合口类型（ethernetCsmacd/fastEthernet/gigabitEthernet/ieee8023adLag）。
const LIVE_IF_TYPES: [u32; 4] = [6, 62, 117, 161];

/// 端口静态清单：ifName/ifDescr、ifType、ifHighSpeed/ifSpeed。
async fn discover_ports(client: &Client<UdpHandle>) -> Vec<PortMeta> {
    let names = walk_column(client, if_name()).await;
    let descrs = walk_column(client, if_descr()).await;
    let types = walk_column(client, if_type()).await;
    let high_speeds = walk_column(client, if_high_speed()).await;
    let speeds = walk_column(client, if_speed()).await;

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

// ============================================================
// CPU
// ============================================================

async fn discover_cpu(client: &Client<UdpHandle>, vendor: VendorKind) -> Vec<RawSource> {
    let vendor_columns: &[Oid] = match vendor {
        VendorKind::Cisco => &[cisco_cpm_cpu_1min(), cisco_cpm_cpu_5min()],
        VendorKind::Huawei => &[huawei_cpu()],
        VendorKind::H3c => &[h3c_cpu()],
        VendorKind::Generic => &[],
    };
    for column in vendor_columns {
        let sources = percent_sources(client, column).await;
        if !sources.is_empty() {
            return sources;
        }
    }
    // 通用回退：hrProcessorLoad（每核一行，采集时取最大值）
    percent_sources(client, &hr_processor_load()).await
}

/// walk 一列百分比值，返回有效行（0..=100）的定向来源。
async fn percent_sources(client: &Client<UdpHandle>, column: &Oid) -> Vec<RawSource> {
    walk_column(client, column.clone())
        .await
        .into_iter()
        .filter(|(_, v)| v.as_u32().is_some_and(|p| p <= 100))
        .map(|(idx, _)| RawSource::indexed(column.clone(), idx))
        .collect()
}

// ============================================================
// 内存
// ============================================================

async fn discover_mem(client: &Client<UdpHandle>, vendor: VendorKind) -> MemPlan {
    let memory = match vendor {
        VendorKind::Cisco => discover_cisco_mem(client).await,
        VendorKind::Huawei => {
            let mega = huawei_mem_size_mega();
            let bytes = huawei_mem_size();
            discover_entity_mem(
                client,
                &huawei_mem_usage(),
                Some((&mega, 1_048_576)),
                Some(&bytes),
            )
            .await
        }
        VendorKind::H3c => {
            let bytes = h3c_mem_size();
            discover_entity_mem(client, &h3c_mem_usage(), None, Some(&bytes)).await
        }
        VendorKind::Generic => None,
    };
    if let Some(plan) = memory {
        return plan;
    }
    if let Some(plan) = discover_hr_mem(client).await {
        return plan;
    }
    // UCD 标量（net-snmp 主机）：始终可用，无需探测
    MemPlan::Ucd {
        total_kb: RawSource::new(ucd_mem_total(), Vec::new()),
        avail_kb: RawSource::new(ucd_mem_avail(), Vec::new()),
    }
}

async fn discover_cisco_mem(client: &Client<UdpHandle>) -> Option<MemPlan> {
    let names = walk_column(client, cisco_mem_pool_name()).await;
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
        // 名称列缺失时以 used 列存在的行为准
        None => walk_column(client, cisco_mem_pool_used())
            .await
            .keys()
            .next()
            .copied()?,
    };
    Some(MemPlan::Pool {
        used: RawSource::indexed(cisco_mem_pool_used(), index),
        free: RawSource::indexed(cisco_mem_pool_free(), index),
    })
}

/// 华为/华三实体表内存：usage 百分比 + size 列缓存总量（优先 MB 口径）。
async fn discover_entity_mem(
    client: &Client<UdpHandle>,
    usage_column: &Oid,
    size_mega: Option<(&Oid, u64)>,
    size_bytes: Option<&Oid>,
) -> Option<MemPlan> {
    let usage = walk_column(client, usage_column.clone()).await;
    let usage_indexes: Vec<u32> = usage
        .iter()
        .filter(|(_, v)| v.as_u32().is_some_and(|p| p <= 100))
        .map(|(idx, _)| *idx)
        .collect();
    if usage_indexes.is_empty() {
        return None;
    }
    let mega = match size_mega {
        Some((column, factor)) => walk_column(client, column.clone())
            .await
            .into_iter()
            .filter_map(|(idx, v)| v.as_u64().map(|n| (idx, n.saturating_mul(factor))))
            .collect::<BTreeMap<_, _>>(),
        None => BTreeMap::new(),
    };
    let bytes = match size_bytes {
        Some(column) => walk_column(client, column.clone())
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
async fn discover_hr_mem(client: &Client<UdpHandle>) -> Option<MemPlan> {
    let varbinds = walk_all(client, hr_storage_table()).await;
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

// ============================================================
// 温度
// ============================================================

async fn discover_temp(client: &Client<UdpHandle>, vendor: VendorKind) -> Vec<TempSource> {
    let vendor_columns: &[Oid] = match vendor {
        VendorKind::Cisco => &[cisco_envmon_temp()],
        VendorKind::Huawei => &[huawei_temp()],
        VendorKind::H3c => &[h3c_temp()],
        VendorKind::Generic => &[],
    };
    for column in vendor_columns {
        let sources = celsius_sources(client, column).await;
        if !sources.is_empty() {
            return sources;
        }
    }
    // 回退 1：ENTITY-SENSOR-MIB（交换机通用传感器表）
    let sensors = discover_entity_sensors(client).await;
    if !sensors.is_empty() {
        return sensors;
    }
    // 回退 2：LM-SENSORS-MIB（Linux 主机）
    discover_lm_sensors(client).await
}

/// 厂商温度列：直接摄氏度，过滤无效读数（0 常为占位、>200 明显越界）。
async fn celsius_sources(client: &Client<UdpHandle>, column: &Oid) -> Vec<TempSource> {
    walk_column(client, column.clone())
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
async fn discover_entity_sensors(client: &Client<UdpHandle>) -> Vec<TempSource> {
    let varbinds = walk_all(client, entity_sensor_table()).await;
    let mut scales: BTreeMap<(u32, u32), i32> = BTreeMap::new();
    let mut precisions: BTreeMap<(u32, u32), i32> = BTreeMap::new();
    let mut values: BTreeMap<(u32, u32), u64> = BTreeMap::new();
    let mut statuses: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for vb in &varbinds {
        let arcs = vb.oid.arcs();
        // 表基 10 弧 + entry(1) + 列号 + entPhysicalIndex + entSensorIndex
        if arcs.len() != 14 {
            continue;
        }
        let key = (arcs[12], arcs[13]);
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
                source: RawSource::new(oid!(1, 3, 6, 1, 2, 1, 99, 1, 1, 1, 4), vec![key.0, key.1]),
                unit,
            })
        })
        .collect()
}

/// LM-SENSORS-MIB：毫摄氏度列。
async fn discover_lm_sensors(client: &Client<UdpHandle>) -> Vec<TempSource> {
    walk_column(client, lm_temp_value())
        .await
        .into_iter()
        .filter(|(_, v)| v.as_u64().and_then(temp_from_milli).is_some())
        .map(|(idx, _)| TempSource {
            source: RawSource::indexed(lm_temp_value(), idx),
            unit: TempUnit::MilliCelsius,
        })
        .collect()
}

// ============================================================
// 工具
// ============================================================

/// walk 一列并返回 `行索引 → 值`（异常值跳过）。
async fn walk_column(client: &Client<UdpHandle>, base: Oid) -> BTreeMap<u32, Value> {
    let mut map = BTreeMap::new();
    for vb in walk_all(client, base).await {
        if vb.value.is_exception() {
            continue;
        }
        if let Some(&index) = vb.oid.arcs().last() {
            map.insert(index, vb.value);
        }
    }
    map
}

/// 整数温度值（摄氏度）转 f64：0/越界视为无效。
fn temp_value(value: &Value) -> Option<f64> {
    let raw = value
        .as_i32()
        .map(f64::from)
        .or_else(|| value.as_u64().map(|v| v as f64))?;
    (0.0 < raw && raw <= 200.0).then_some(raw)
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
        MemPlan::Pool { .. } => "cisco-pool",
        MemPlan::EntityPercent { .. } => "entity-percent",
        MemPlan::HrStorage { .. } => "hr-storage",
        MemPlan::Ucd { .. } => "ucd",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 厂商识别_企业号优先描述串兜底() {
        let cisco = Oid::parse("1.3.6.1.4.1.9.1.1208").unwrap();
        assert_eq!(
            detect_vendor(Some(&cisco), Some("some text")),
            VendorKind::Cisco
        );
        let huawei = Oid::parse("1.3.6.1.4.1.2011.2.23.1").unwrap();
        assert_eq!(detect_vendor(Some(&huawei), None), VendorKind::Huawei);
        let h3c = Oid::parse("1.3.6.1.4.1.25506.1.1").unwrap();
        assert_eq!(detect_vendor(Some(&h3c), None), VendorKind::H3c);
        // 惠普企业号需 Comware 描述串确认，否则不误判为华三
        let hp = Oid::parse("1.3.6.1.4.1.11.2.3.7").unwrap();
        assert_eq!(
            detect_vendor(Some(&hp), Some("HP Comware Platform Software")),
            VendorKind::H3c
        );
        assert_eq!(
            detect_vendor(Some(&hp), Some("HP LaserJet")),
            VendorKind::Generic
        );
        assert_eq!(detect_vendor(None, Some("Huawei VRP")), VendorKind::Huawei);
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
