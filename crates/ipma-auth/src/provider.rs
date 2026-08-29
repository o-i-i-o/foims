//! 认证模块的状态抽象（依赖倒置边界）。
//!
//! 本 crate 的 handler 面向 [`AuthProvider`] 泛型编写；主程序 crate 中的
//! `AppState` 提供连接池（经 [`DbProvider`] 超 trait）、JWT 工具与应用配置，
//! 从而避免 ipma-auth 反向依赖主程序。

use ipma_common::DbProvider;
use ipma_common::config::Config;

use crate::utils::JwtUtils;

pub trait AuthProvider: DbProvider {
    /// JWT 签发与校验工具。
    fn jwt_utils(&self) -> &JwtUtils;

    /// 应用配置。
    fn config(&self) -> &Config;
}
