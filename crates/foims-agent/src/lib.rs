//! FOIMS Agent 库入口：指标模型、采集器框架与各采集器实现。
//!
//! 移植约定（详见 docs/agent-porting.md）：
//! - 指标名称/标签/类型与 node_exporter 严格对齐，便于逐项对照验证；
//! - 只读 /proc、/sys 与少量 syscalls（经 rustix 安全封装，crate 内零 unsafe）；
//! - 采集器一律支持 `with_root` 构造，测试可用 node_exporter 的 fixture 数据。

pub mod collector;
pub mod collectors;
pub mod metric;

pub use collector::{Collector, CollectorError, default_collectors, scrape};
pub use metric::{MetricFamily, MetricType, encode_text_all};

/// Agent 版本：构建期由 build-agent.sh 以 FOIMS_AGENT_VERSION 注入主程序
/// 版本（agent 版本 = 主程序版本，满足「agent 版本不低于 foims」门控）；
/// 未注入（如开发机直编）时回退 crate 自身版本。
pub const VERSION: &str = match option_env!("FOIMS_AGENT_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};
