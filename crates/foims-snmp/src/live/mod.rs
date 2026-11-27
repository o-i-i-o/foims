//! 实时采集调度层：探测（profile::probe）→ 单设备采集器（LiveCollector）
//! → 快照（LiveSnapshot）。
//!
//! 按 docs/SNMP模块重构方案.md §5.1，本层只剩调度与快照，不含 OID 知识：
//! 资源组（CPU/内存/温度）解释 [`crate::collect::resource::ResourceReader`]，
//! 候选选择与行索引缓存在探测期建立的 [`crate::collect::resource::IndexCache`]
//! 中跨轮复用。SNMP 读操作抽象（[`SnmpReader`]）由 core 提供，
//! 生产实现为 async_snmp UDP 客户端，测试实现为内存 OID 快照。

pub mod collector;
pub mod profile;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::collect::resource::IndexCache;

pub use collector::LiveCollector;
pub use crate::core::{SnmpReader, get_many_tolerant};
pub use profile::{ProbeError, probe};

/// 单请求 OID 上限：20 个 OID × 约 30B + v3 authPriv 头约 150B ≈ 750B，
/// 低于 IPv6 最小 MTU 1280 对应的 UDP 安全载荷 1232B。
pub const MAX_OIDS_PER_REQUEST: usize = 20;
/// 实时采集端口上限：异常设备（大量虚拟接口）下限制单设备 OID 规模。
pub const MAX_LIVE_PORTS: usize = 128;

/// 端口静态元数据（探测期缓存，采集期按 ifIndex 定向 GET）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortMeta {
    pub if_index: u32,
    pub name: String,
    pub speed_bps: Option<u64>,
}

/// 设备实时画像：识别结果（profile/brand/caps + 解析后采集计划）、
/// 行索引缓存、端口清单。探测一次、采集循环反复使用。
#[derive(Debug)]
pub struct DeviceProfile {
    pub identity: crate::vendor::Identity,
    /// 探测期初始化的候选/索引缓存（资源组解释器跨轮复用）。
    pub cache: IndexCache,
    pub ports: Vec<PortMeta>,
    /// 设备是否支持 ifXTable 64 位计数器（探测期单点验证）。
    pub use_hc: bool,
}

/// 单轮采样范围：由调用方按「当前是否被观察」动态选择，避免为无人查看的
/// 设备持续下发全端口状态与流量 OID。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveSampleSpec {
    /// 是否读取全端口 admin/oper 状态（2×N 个 OID，成本随端口数线性增长）
    pub ports: bool,
    /// 是否读取采集端口计数器（2 个 OID）
    pub traffic: bool,
}

impl LiveSampleSpec {
    /// 无人观察的基线轮次：只更新 CPU/内存/温度/uptime 等资源组。
    pub const BASELINE: Self = Self {
        ports: false,
        traffic: false,
    };
    /// 主机列表观察：列表只展示资源组，不读端口状态与流量。
    pub const WATCH: Self = Self {
        ports: false,
        traffic: false,
    };
    /// 仪表盘卡片：需要流量 Top，但不展示端口明细。
    pub const OVERVIEW: Self = Self {
        ports: false,
        traffic: true,
    };
    /// 选中设备详情：资源组 + 流量 + 全端口状态。
    pub const DETAIL: Self = Self {
        ports: true,
        traffic: true,
    };

    /// 本轮是否包含端口状态 OID。
    pub fn includes_ports(self) -> bool {
        self.ports
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
    /// 识别品牌（profile.brand 展示口径，generic 为兜底）。
    pub brand: String,
    pub cpu_usage_pct: Option<f64>,
    pub mem_total: Option<u64>,
    pub mem_used: Option<u64>,
    pub mem_usage_pct: Option<f64>,
    pub temperature_c: Option<f64>,
    pub uptime_secs: Option<i64>,
    /// 本轮实际采集流量的端口名（配置端口未命中时回落探测结果首端口）。
    pub monitor_if: Option<String>,
    pub ports: Vec<LivePortSnapshot>,
    /// 采集端口的入/出向速率（bit/s），不再是全设备求和。
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
