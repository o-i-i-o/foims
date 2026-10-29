//! SNMP 实时采集层：厂商 profile 探测 + 定向 OID GET，输出内存快照。
//!
//! 与 [`crate::collect::metrics`] 的全量 walk（5 分钟、落库）互补：本模块
//! 只做「按设备已探测的索引精确 GET」，单轮请求量固定为
//! `4×端口数 + 资源组`，不随 ifTable/存储表规模膨胀；结果仅供实时展示、
//! Top 榜与告警实时评估，不写数据库（趋势仍由全量轮询落库）。
//!
//! 关键约束：
//! - IPv6 管理地址：单个 GET 的 OID 数上限 [`MAX_OIDS_PER_REQUEST`]，
//!   使 UDP 载荷（含 v3 头）留在 1232 字节安全线内，避免分片丢包；
//! - 会话复用由上层（agent-service）负责，本模块只接收 `&Client`；
//! - 厂商差异集中在 [`profile::probe`]：探测一次缓存，采集循环不再 walk；
//! - 温度回退顺序：厂商 MIB → ENTITY-SENSOR-MIB → LM-SENSORS，任一命中
//!   即止（交换机优先厂商 MIB，Linux 主机回退 LM-SENSORS）。

pub mod collector;
pub mod profile;

use std::collections::BTreeMap;

use async_snmp::Oid;
use chrono::{DateTime, Utc};
use serde::Serialize;

pub use collector::LiveCollector;
pub use profile::{ProbeError, probe};

/// 单请求 OID 上限：20 个 OID × 约 30B + v3 authPriv 头约 150B ≈ 750B，
/// 低于 IPv6 最小 MTU 1280 对应的 UDP 安全载荷 1232B。
pub const MAX_OIDS_PER_REQUEST: usize = 20;
/// 实时采集端口上限：异常设备（大量虚拟接口）下限制单设备 OID 规模。
pub const MAX_LIVE_PORTS: usize = 128;

/// 厂商类别（决定资源组 OID 与温度回退顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VendorKind {
    Cisco,
    Huawei,
    H3c,
    Generic,
}

impl VendorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            VendorKind::Cisco => "cisco",
            VendorKind::Huawei => "huawei",
            VendorKind::H3c => "h3c",
            VendorKind::Generic => "generic",
        }
    }
}

/// 带实例后缀的 OID 来源（列 OID + 实例弧）。
///
/// 实例用 `Vec<u32>` 而非单个 `u32`：ENTITY-SENSOR-MIB 等双弧索引表
/// （entPhysicalIndex, entSensorIndex）需要完整后缀。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSource {
    pub base: Oid,
    pub instance: Vec<u32>,
}

impl RawSource {
    pub fn new(base: Oid, instance: Vec<u32>) -> Self {
        Self { base, instance }
    }

    /// 单弧实例便捷构造（IF-MIB / 实体 CPU/内存/温度表）。
    pub fn indexed(base: Oid, index: u32) -> Self {
        Self {
            base,
            instance: vec![index],
        }
    }

    pub fn oid(&self) -> Oid {
        Oid::new(
            self.base
                .arcs()
                .iter()
                .copied()
                .chain(self.instance.iter().copied()),
        )
    }
}

/// 内存采集方案（探测期已确定索引/单位，采集期只 GET 需要的列）。
#[derive(Debug, Clone)]
pub enum MemPlan {
    None,
    /// CISCO-MEMORY-POOL-MIB：used/free 两列（字节），total = used + free。
    Pool {
        used: RawSource,
        free: RawSource,
    },
    /// 华为/华三实体表：usage 百分比 + 探测期缓存的实体内存总量（字节）。
    EntityPercent {
        entities: Vec<EntityMem>,
    },
    /// HOST-RESOURCES-MIB：Ram 行 size/used（单位块 × units）。
    HrStorage {
        size: RawSource,
        used: RawSource,
        units: u64,
    },
    /// UCD-SNMP-MIB：KB 标量（net-snmp 主机回退）。
    Ucd {
        total_kb: RawSource,
        avail_kb: RawSource,
    },
}

/// 单个实体的内存使用率来源与总量缓存。
#[derive(Debug, Clone)]
pub struct EntityMem {
    pub usage: RawSource,
    pub total_bytes: Option<u64>,
}

/// 温度值单位/换算方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempUnit {
    /// 厂商 MIB 直接摄氏度（Cisco ENVMON / 华为 / 华三）。
    Celsius,
    /// LM-SENSORS：毫摄氏度。
    MilliCelsius,
    /// ENTITY-SENSOR-MIB：value × 10^(scale-9) / 10^precision。
    EntitySensor { scale: i32, precision: i32 },
}

/// 单个温度传感器来源。
#[derive(Debug, Clone)]
pub struct TempSource {
    pub source: RawSource,
    pub unit: TempUnit,
}

/// 端口静态元数据（探测期缓存，采集期按 ifIndex 定向 GET）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortMeta {
    pub if_index: u32,
    pub name: String,
    pub speed_bps: Option<u64>,
}

/// 设备厂商 profile：探测一次、采集循环反复使用。
#[derive(Debug, Clone)]
pub struct DeviceProfile {
    pub vendor: VendorKind,
    /// CPU 使用率来源（整数百分比，多个实体取最大值）。
    pub cpu: Vec<RawSource>,
    pub mem: MemPlan,
    pub temp: Vec<TempSource>,
    pub ports: Vec<PortMeta>,
    /// 设备是否支持 ifXTable 64 位计数器（探测期单点验证）。
    pub use_hc: bool,
}

impl DeviceProfile {
    /// 单轮状态/流量组的 OID 数量（不含资源组），供容量评估与日志。
    pub fn base_oid_count(&self) -> usize {
        1 + self.ports.len() * 4
    }
}

/// 单端口实时快照。
#[derive(Debug, Clone, Serialize)]
pub struct LivePortSnapshot {
    pub if_index: u32,
    pub name: String,
    pub speed_bps: Option<u64>,
    pub admin_up: Option<bool>,
    pub oper_up: Option<bool>,
    /// 入向速率（bit/s；首轮/间隔过短/计数清零时为 null）。
    pub rx_bps: Option<f64>,
    pub tx_bps: Option<f64>,
}

/// 单设备实时快照（只进内存，不落库）。
#[derive(Debug, Clone, Serialize)]
pub struct LiveSnapshot {
    pub collected_at: DateTime<Utc>,
    pub vendor: VendorKind,
    pub cpu_usage_pct: Option<f64>,
    pub mem_total: Option<u64>,
    pub mem_used: Option<u64>,
    pub mem_usage_pct: Option<f64>,
    pub temperature_c: Option<f64>,
    pub uptime_secs: Option<i64>,
    pub ports: Vec<LivePortSnapshot>,
    /// 全设备入/出向速率合计（bit/s）。
    pub rx_bps: Option<f64>,
    pub tx_bps: Option<f64>,
    /// 本轮是否收到过任何有效 varbind（全部分块失败时为 false，
    /// 上层据此判定设备不可达并触发退避/重连）。
    pub responded: bool,
    /// 是否存在失败分块（部分数据缺失仍可展示）。
    pub partial: bool,
}

/// 上一轮计数器状态（差值算速率，仅内存）。
#[derive(Debug, Clone, Default)]
pub(crate) struct LiveCounters {
    pub ts: i64,
    pub ports: BTreeMap<u32, PortCounters>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PortCounters {
    pub in_octets: u64,
    pub out_octets: u64,
}
