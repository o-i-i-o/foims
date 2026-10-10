//! FOIMS Agent 服务端 crate。
//!
//! 本 crate 承载 agent 分发与监控闭环（设计 docs/agent-design.md §3.3/§5）：
//! - [`manifest`]：构建期产物清单（manifest.json）解析与版本门控；
//! - [`packaging`]：按操作系统/架构动态组包（zip / deb / rpm）；
//! - [`api`]：分发下载 Web API（/api/agents/dist、/api/agents/download）；
//! - [`cert`]：Agent 服务端/客户端证书签发（站点 CA 签发，mTLS 物料）；
//! - [`report_server`]：QUIC/HTTP3 指标接收监听（强制客户端证书）；
//! - [`ingest`]：上报鉴权、校验与入库（agents + agent_metrics_history）；
//! - [`agents_api`]：Agent 列表/详情/历史/管理的 Web API；
//! - [`tasks`]：离线判定与历史清理调度任务执行器。

/// 站点 CA 公钥路径（公开物料，作 agent 信任锚，设计 §3.1）
pub const SITE_CA_PATH: &str = "/etc/ssl/foims-ca/ca.pem";
/// 站点 CA 私钥路径（Agent 证书签发需要，由证书管理页生成）
pub const SITE_CA_KEY_PATH: &str = "/etc/ssl/foims-ca/ca.key";

pub mod agents_api;
pub mod api;
pub mod cert;
pub mod ingest;
pub mod manifest;
pub mod packaging;
pub mod report_server;
pub mod tasks;
