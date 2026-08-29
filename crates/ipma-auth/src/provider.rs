//! 认证模块的状态抽象（依赖倒置边界）。
//!
//! 本 crate 的 handler 面向 [`AuthProvider`] 泛型编写；主程序 crate 中的
//! `AppState` 提供数据库连接池、JWT 工具与应用配置并实现本 trait，
//! 从而避免 ipma-auth 反向依赖主程序。

use ipma_common::AppError;
use ipma_common::config::Config;
use ipma_common::db::DbPool;

use crate::utils::JwtUtils;

pub trait AuthProvider: Clone + Send + Sync + 'static {
    /// 应用数据库连接池（未初始化时返回错误）。
    fn pool(&self) -> Result<&DbPool, AppError>;

    /// JWT 签发与校验工具。
    fn jwt_utils(&self) -> &JwtUtils;

    /// 应用配置。
    fn config(&self) -> &Config;
}
