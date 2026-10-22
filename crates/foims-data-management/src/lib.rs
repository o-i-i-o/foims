//! FOIMS 数据管理：模块化 CSV 导入导出、日志清理与数据库备份。

// 测试代码允许 unwrap/expect（AGENTS.md「测试代码除外」）；生产代码仍由
// workspace lints + CI `-D warnings` 强制禁止。cfg(test) 仅作用于测试构建。
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod backup;
pub mod export;
pub mod import;
pub mod logs;
pub mod modules;
pub mod names;
pub mod spec;
pub mod types;

pub use backup::*;
pub use export::*;
pub use import::*;
pub use logs::*;
pub use modules::*;
pub use types::*;
