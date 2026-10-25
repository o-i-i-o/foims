//! SNMP 采集实现：系统信息、端口表、LLDP 邻居、ARP/FDB 表、性能指标，
//! 以及第二层网络侧台账（物理实体/传感器/VLAN/CDP/LAG/PoE）。

pub mod arp;
pub mod cdp;
pub mod entity;
pub mod lag;
pub mod lldp;
pub mod metrics;
pub mod poe;
pub mod ports;
pub mod sysinfo;
pub mod vlan;
