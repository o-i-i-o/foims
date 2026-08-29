//! 工具模块（业务侧通用函数）。
//!
//! 纯网络/HTTP 工具、分页与限流已下沉至 ipma-common，此处经模块再导出
//! 保持 `crate::utils::...` 调用路径稳定；业务耦合部分见 [`common`]。

pub mod common;

pub use common::*;
pub use ipma_common::net::*;
pub use ipma_common::pagination;
pub use ipma_common::rate_limit;
