//! 采集器实现集合。
//!
//! 每个文件对应 node_exporter 的一个同名采集器；`build_defaults` 返回
//! 默认启用集合（默认开关状态对齐原版，随移植进度逐步扩充）。

pub mod boottime;
pub mod cpu;
pub mod cpufreq;
pub mod diskstats;
pub mod filesystem;
pub mod hwmon;
pub mod loadavg;
pub mod meminfo;
pub mod netdev;
pub mod os;
pub mod thermal_zone;
pub mod time;
pub mod uname;

use crate::collector::Collector;

/// 构建默认启用的采集器实例集合（按名称字母序，输出顺序稳定便于对照）。
pub fn build_defaults() -> Vec<Box<dyn Collector>> {
    vec![
        Box::new(boottime::BootTimeCollector::default()),
        Box::new(cpu::CpuCollector::default()),
        Box::new(cpufreq::CpufreqCollector::default()),
        Box::new(diskstats::DiskstatsCollector::default()),
        Box::new(filesystem::FilesystemCollector::default()),
        Box::new(hwmon::HwmonCollector::default()),
        Box::new(loadavg::LoadavgCollector::default()),
        Box::new(meminfo::MeminfoCollector::default()),
        Box::new(netdev::NetdevCollector::default()),
        Box::new(os::OsCollector::default()),
        Box::new(thermal_zone::ThermalZoneCollector::default()),
        Box::new(time::TimeCollector::default()),
        Box::new(uname::UnameCollector),
    ]
}
