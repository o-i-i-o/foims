//! IPMI 实时快照类型：秒级路径的内存工作副本（供 SSE/Top/告警评估消费）。
//!
//! 采集循环、错峰与指数退避在 agent-service 侧（`live_poll`），本模块只定义
//! 跨层传递的不可变快照结构；由 [`crate::collect::IpmiReading`] 归一而来。

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::collect::{IpmiReading, SensorValue};

/// 单设备一次 IPMI 实时采集的内存快照（按 device_id 存入 `IpmiLiveRegistry`）。
#[derive(Debug, Clone, Serialize)]
pub struct IpmiLiveSnapshot {
    /// 采集完成时刻（供 SSE 节流与陈旧判定）
    pub collected_at: DateTime<Utc>,
    /// 本轮是否成功收到 BMC 响应（false 表示采集失败，保留上一轮值由 worker 决定）
    pub responded: bool,
    /// 温度类传感器最高读数（无温度传感器为 None）
    pub max_temp: Option<f64>,
    pub fans: Vec<SensorValue>,
    pub volts: Vec<SensorValue>,
    /// 系统电源是否开启（Get Chassis Status）
    pub power_on: bool,
    /// BMC 厂商可读串（写入 monitored_hosts.os 的同一口径）
    pub vendor: String,
}

impl IpmiLiveSnapshot {
    /// 由采集结果构造成功快照。
    #[must_use]
    pub fn from_reading(reading: &IpmiReading, collected_at: DateTime<Utc>) -> Self {
        Self {
            collected_at,
            responded: true,
            max_temp: reading.max_temp,
            fans: reading.fans.clone(),
            volts: reading.volts.clone(),
            power_on: reading.power_on,
            vendor: reading.device.vendor_label(),
        }
    }

    /// 采集失败占位快照（responded=false，指标列置空，供前端渲染 "-"）。
    #[must_use]
    pub fn failure(collected_at: DateTime<Utc>) -> Self {
        Self {
            collected_at,
            responded: false,
            max_temp: None,
            fans: Vec::new(),
            volts: Vec::new(),
            power_on: false,
            vendor: String::new(),
        }
    }
}
