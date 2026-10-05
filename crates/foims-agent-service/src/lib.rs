//! FOIMS Agent 服务端 crate。
//!
//! 本 crate 承载 agent 分发与（后续阶段的）接收服务：
//! - [`manifest`]：构建期产物清单（manifest.json）解析与版本门控；
//! - [`packaging`]：按操作系统/架构动态组包（zip / deb / rpm）；
//! - [`api`]：分发下载 Web API（/api/agents/dist、/api/agents/download）。
//!
//! 设计来源：docs/agent-design.md §5.1 / §6。

pub mod api;
pub mod manifest;
pub mod packaging;
