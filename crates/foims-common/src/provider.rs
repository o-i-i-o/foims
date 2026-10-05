//! 数据访问边界 trait（依赖倒置）。
//!
//! 业务 crate 的 handler 面向 [`DbProvider`] 泛型编写，由主程序的
//! `AppState` 实现以提供连接池，避免业务 crate 反向依赖主程序。

use std::sync::Arc;

use crate::config::Config;
use crate::db::DbPool;
use crate::error::AppError;

pub trait DbProvider: Clone + Send + Sync + 'static {
    /// 应用数据库连接池（未初始化时返回错误）。
    fn pool(&self) -> Result<&DbPool, AppError>;
}

/// 配置访问边界 trait（依赖倒置）。
///
/// 与 [`DbProvider`] 同理：业务 crate 的 handler 经此 trait 读取共享配置槽
/// 最新快照与主程序版本，避免业务 crate 反向依赖主程序。
pub trait ConfigProvider: DbProvider {
    /// 共享配置槽最新快照（写盘端点刷新后即为最新）。
    fn config(&self) -> Arc<Config>;

    /// 主程序版本（`env!("CARGO_PKG_VERSION")`，由主 crate 实现时展开），
    /// 供 agent 分发的版本门控比较。
    fn server_version(&self) -> &'static str;
}
