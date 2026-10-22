//! FOIMS 工作区共享基础库。
//!
//! 提供各 crate 复用的统一 API 响应结构（[`ApiResponse`]）、统一 JSON
//! 提取器（[`AppJson`]）、全局错误类型（[`AppError]`）、面向前端的
//! i18n 消息类型（[`AppMessage`]）、PostgreSQL 错误归类映射
//! （[`classify_db_error`]）与多语言日志宏基础设施（[`log_i18n`]）。

// 测试代码允许 unwrap/expect（AGENTS.md「测试代码除外」）；生产代码仍由
// workspace lints + CI `-D warnings` 强制禁止。cfg(test) 仅作用于测试构建。
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod api;
mod db_error;
mod error;
mod json;
mod log_i18n;
mod msg;

pub mod config;
pub mod crypto;
pub mod db;
pub mod net;
pub mod pagination;
pub mod pgpass;
pub mod provider;
pub mod rate_limit;
pub mod report;
pub mod validation;
pub mod x509;

pub use api::{ApiResponse, ok_json};
pub use config::{ArcSwap, Config, SharedConfig};
pub use db_error::{DbErrorKind, classify_db_error};
pub use error::AppError;
pub use json::AppJson;
pub use log_i18n::{
    active_log_langs, emit_debug, emit_error, emit_info, emit_warn, set_active_log_langs,
    set_log_translate, translate_for,
};
pub use msg::{AppMessage, msg};
pub use provider::{ConfigProvider, DbProvider};
pub use validation::validation_errors_to_message;
