//! FOIMS 数据采集服务 crate（agent 分发/接收 + SNMP 采集，设计
//! docs/agent-design.md §3.3/§5/§6）。
//!
//! 本 crate 承载服务端数据采集面：
//! - [`manifest`]：构建期产物清单（manifest.json）解析与版本门控；
//! - [`packaging`]：按操作系统/架构动态组包（zip / deb / rpm）；
//! - [`api`]：分发下载 Web API（/api/agents/dist、/api/agents/download）；
//! - [`cert`]：Agent 服务端/客户端证书签发与续期（站点 CA 签发，mTLS 物料）；
//! - [`report_server`]：QUIC/HTTP3 指标接收监听（强制客户端证书，
//!   同时承载 [`renew`] 续期端点的路由分发）；
//! - [`renew`]：客户端证书续期处理（POST /agent/v1/renew，mTLS 鉴权）；
//! - [`ingest`]：上报鉴权、校验与入库（agents + agent_metrics_history）；
//! - [`alerts`]：主机资源告警（全局阈值 + 状态翻转评估 + 站内通知）；
//! - [`agents_api`]：Agent 列表/详情/历史/管理的 Web API；
//! - [`snmp_poll`]：SNMP 设备轮询采集（纳入主机监控页，source='snmp'）；
//! - [`snmp_metrics`]：SNMP 性能指标采集（CPU/内存/磁盘/温度/流量/负载
//!   与差值计算，二期）；
//! - [`trap`]：SNMP Trap/Inform 接收（自资源管理模块迁入，采集服务）；
//! - [`tasks`]：离线判定、SNMP 轮询与历史清理调度任务执行器。

/// 站点 CA 公钥路径（公开物料，作 agent 信任锚，设计 §3.1）
pub const SITE_CA_PATH: &str = "/etc/ssl/foims-ca/ca.pem";
/// 站点 CA 私钥路径（Agent 证书签发需要，由证书管理页生成）
pub const SITE_CA_KEY_PATH: &str = "/etc/ssl/foims-ca/ca.key";

pub mod agents_api;
pub mod alerts;
pub mod api;
pub mod cert;
pub mod ingest;
pub mod manifest;
pub mod packaging;
pub mod renew;
pub mod report_server;
pub mod snmp_metrics;
pub mod snmp_poll;
pub mod tasks;
pub mod trap;

pub use trap::start_trap_receiver;
