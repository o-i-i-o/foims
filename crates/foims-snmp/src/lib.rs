//! FOIMS SNMP 域 crate。
//!
//! 本 crate 承载 SNMP 协议的完整能力面，自 foims-resource 与
//! foims-agent-service 中的寄居代码抽取而来（纯移动、零行为变更）：
//! - [`core`]：协议参数层——设备凭据解密、SNMP 参数构造、v1/v2c/v3 鉴权
//!   组装、目标地址格式化、错误文案；
//! - [`core::config`]：设备 SNMP 配置与 IP 的只读查询；
//! - [`collect`]：采集实现——系统信息（sysDescr/sysName）、端口表（IF-MIB +
//!   BRIDGE/Q-BRIDGE）、LLDP 邻居、ARP/FDB 表、性能指标（CPU/内存/磁盘/
//!   温度/流量/负载与差值计算）；
//! - [`handlers`]：SNMP 探测与信息采集 Axum handler（test-snmp / snmp-info）；
//! - [`events`]：观测事件（snmp_events 落库、站内通知与查询）；
//! - [`live`]：实时采集——厂商 profile 探测 + 定向 OID GET，输出内存快照
//!   （供 SSE 推送与 Top 榜，不落库）。
//!
//! 依赖方向约束：仅依赖 foims-common / foims-models / foims-auth，
//! 不得依赖 foims-resource 与 foims-agent-service（二者反向依赖本 crate）。

// 测试代码允许 unwrap/expect（AGENTS.md「测试代码除外」）；生产代码仍由
// workspace lints + CI `-D warnings` 强制禁止。cfg(test) 仅作用于测试构建。
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod collect;
pub mod core;
pub mod events;
pub mod handlers;
pub mod live;

pub use core::config::{SwitchConfigError, get_device_ip_address, get_device_snmp_config};
pub use core::{
    DecryptedSnmpCredentials, DeviceForSnmp, SnmpError, SnmpParamsLegacy, build_auth,
    format_snmp_error, parse_auth_protocol, parse_priv_protocol, snmp_target,
    truncate_to_column_width,
};
