//! FOIMS 数据采集服务 crate（agent 接收 + SNMP 采集）。
//!
//! 本 crate 承载服务端数据采集面：
//! - [`packaging`]：配置包现场组装（zip / deb / rpm，仅证书与配置文件，
//!   二进制安装包由 CI 直发 OS 系统包）；
//! - [`cert`]：Agent 服务端/客户端证书签发与续期（站点 CA 签发，mTLS 物料）；
//! - [`report_server`]：QUIC/HTTP3 指标接收监听（强制客户端证书，
//!   同时承载 [`renew`] 续期端点的路由分发）；
//! - [`renew`]：客户端证书续期处理（POST /agent/v1/renew，mTLS 鉴权）；
//! - [`ingest`]：上报鉴权、校验与入库（monitored_hosts + host_metrics_history）；
//! - [`alerts`]：主机资源告警配置面（全局阈值 + 抖动抑制窗口参数读写校验）；
//! - [`alert_episode`]：告警 episode 运行时（持续超标状态机 + 落库 + 站内通知）；
//! - [`agents_api`]：Agent 列表/详情/历史/管理的 Web API；
//! - [`snmp_poll`]：SNMP 设备轮询采集（纳入主机监控页，source='snmp'），
//!   性能指标采集本体在 foims-snmp::collect::metrics；
//! - [`live_poll`]：SNMP 实时采集（秒级定向 GET + 内存快照注册表，不落库，
//!   供实时展示与 Top 榜；趋势仍由 snmp_poll 落库）；
//! - [`ipmi_poll`]：IPMI 带外轮询采集（纳入主机监控页，source='ipmi'），
//!   SDR/传感器采集本体在 foims-ipmi::collect；
//! - [`live_api`]：实时快照 Web API（全量列表 + SSE 变更推送）；
//! - [`trap`]：SNMP Trap/Inform 接收（属采集服务，非资源管理业务）；
//! - [`tasks`]：离线判定、SNMP 轮询与历史清理调度任务执行器。

// 测试代码允许 unwrap/expect（AGENTS.md「测试代码除外」）；生产代码仍由
// workspace lints + CI `-D warnings` 强制禁止。cfg(test) 仅作用于测试构建。
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

/// 站点 CA 公钥路径（公开物料，作 agent 信任锚，设计 §3.1）
pub const SITE_CA_PATH: &str = "/etc/ssl/foims-ca/ca.pem";
/// 站点 CA 私钥路径（Agent 证书签发需要，由证书管理页生成）
pub const SITE_CA_KEY_PATH: &str = "/etc/ssl/foims-ca/ca.key";

pub mod agents_api;
pub mod alert_episode;
pub mod alerts;
pub mod api;
pub mod cert;
pub mod ingest;
pub mod ipmi_poll;
pub mod live_api;
pub mod live_poll;
pub mod packaging;
pub mod renew;
pub mod report_server;
pub mod snmp_poll;
pub mod tasks;
pub mod trap;

pub use alert_episode::{AlertProvider, AlertRuntime, HostRef};
pub use live_api::LiveProvider;
pub use live_poll::{
    IpmiLiveDeviceSnapshot, IpmiLiveRegistry, LiveDeviceSnapshot, LiveRegistry,
    start_ipmi_live_poller, start_live_poller,
};
pub use trap::start_trap_receiver;
