//! FOIMS IPMI 域 crate：BMC RMCP+ 会话（ruipmi）、SDR/传感器采集、连通性探测
//! handler 与实时快照类型。
//!
//! 依赖方向约束：仅依赖 foims-common / foims-models / foims-auth，不得依赖
//! foims-resource 与 foims-agent-service（二者反向依赖本 crate）。
//!
//! 真机约束（受 ruipmi 0.6.2 实现所限，已在设备列设计上收敛）：
//! - 会话端口固定 623、特权固定 Administrator、超时/重试固定；
//! - 纯 IPv6 目标：ruipmi 内部绑定 `0.0.0.0:0`，对 IPv6 建会话可能失败，
//!   管理网推荐 IPv4 BMC 地址。

// 测试代码允许 unwrap/expect（AGENTS.md「测试代码除外」）；生产代码仍由
// workspace lints + CI `-D warnings` 强制禁止。cfg(test) 仅作用于测试构建。
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod collect;
pub mod core;
pub mod handlers;
pub mod live;

pub use collect::{IpmiReading, SensorValue, collect};
pub use core::{
    DecryptedIpmiCredentials, DeviceForIpmi, DeviceIdInfo, IPMI_AUTH_SHA1, IPMI_AUTH_SHA256,
    IPMI_LAN_PORT, IpmiConfigError, IpmiErrorExt, IpmiParams, format_ipmi_error,
    get_device_ip_address, get_device_ipmi_config, open_session, test_ipmi, truncate_ipmi_text,
};
pub use live::IpmiLiveSnapshot;
