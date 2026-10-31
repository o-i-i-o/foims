//! FOIMS SNMP 域 crate：SNMP 协议参数与 v1/v2c/v3 鉴权构造、MIB 采集与
//! 性能指标、探测 handler、观测事件与实时快照。
//!
//! 依赖方向约束：仅依赖 foims-common / foims-models / foims-auth，不得依赖
//! foims-resource 与 foims-agent-service（二者反向依赖本 crate）。

// 测试代码允许 unwrap/expect（AGENTS.md「测试代码除外」）；生产代码仍由
// workspace lints + CI `-D warnings` 强制禁止。cfg(test) 仅作用于测试构建。
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod collect;
pub mod core;
pub mod debug;
pub mod events;
pub mod handlers;
pub mod live;

pub use core::config::{SwitchConfigError, get_device_ip_address, get_device_snmp_config};
pub use core::{
    DecryptedSnmpCredentials, DeviceForSnmp, SnmpError, SnmpParamsLegacy, build_auth,
    format_snmp_error, parse_auth_protocol, parse_priv_protocol, snmp_target,
    truncate_to_column_width,
};
