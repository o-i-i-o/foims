//! IPMA 工作区共享基础库。
//!
//! 提供各 crate 复用的统一 API 响应结构（[`ApiResponse`]）、统一 JSON
//! 提取器（[`AppJson`]）、全局错误类型（[`AppError]`）、面向前端的
//! i18n 消息类型（[`AppMessage`]）、PostgreSQL 错误归类映射
//! （[`classify_db_error`]）与多语言日志宏基础设施（[`log_i18n`]）。

mod api;
mod db_error;
mod error;
mod json;
mod log_i18n;
mod msg;
mod validation;

pub mod config;
pub mod crypto;
pub mod db;
pub mod net;
pub mod pagination;
pub mod rate_limit;

pub use api::{ApiResponse, ok_json};
pub use db_error::{DbErrorKind, classify_db_error};
pub use error::AppError;
pub use json::AppJson;
pub use log_i18n::{
    active_log_langs, emit_debug, emit_error, emit_info, emit_warn, set_active_log_langs,
    set_log_translate, translate_for,
};
pub use msg::{AppMessage, msg};
pub use validation::validation_errors_to_message;
