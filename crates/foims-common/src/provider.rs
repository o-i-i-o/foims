//! 数据访问边界 trait（依赖倒置）。
//!
//! 业务 crate 的 handler 面向 [`DbProvider`] 泛型编写，由主程序的
//! `AppState` 实现以提供连接池，避免业务 crate 反向依赖主程序。

use crate::db::DbPool;
use crate::error::AppError;

pub trait DbProvider: Clone + Send + Sync + 'static {
    /// 应用数据库连接池（未初始化时返回错误）。
    fn pool(&self) -> Result<&DbPool, AppError>;
}
