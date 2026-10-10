//! FOIMS Agent 库入口：指标模型、采集器框架与各采集器实现。
//!
//! 移植约定（详见 docs/agent-porting.md）：
//! - 指标名称/标签/类型与 node_exporter 严格对齐，便于逐项对照验证；
//! - 只读 /proc、/sys 与少量 syscalls（经 rustix 安全封装，crate 内零 unsafe）；
//! - 采集器一律支持 `with_root` 构造，测试可用 node_exporter 的 fixture 数据。

pub mod collector;
pub mod collectors;
pub mod metric;
pub mod reporter;

pub use collector::{Collector, CollectorError, default_collectors, scrape};
pub use metric::{MetricFamily, MetricType, encode_text_all};

/// Agent 版本：独立自管理（取 crate 自身版本），经 --version 与上报快照
/// agent_version 仅作展示/运维核对，不再与主程序版本绑定。
/// 分发门控（agent 版本不低于主程序）比较的是 dist/agents/manifest.json
/// 的清单版本（构建时对齐的服务端版本），与本值无关。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
