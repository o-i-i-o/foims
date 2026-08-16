//! IPMA 工作区共享基础库。
//!
//! 提供各 crate 复用的统一 API 响应结构（[`ApiResponse`]）与 PostgreSQL
//! 错误归类映射（[`classify_db_error`]）。二者此前在多个 crate 中各存一份
//! 逐字相同的副本，收敛于此以避免行为漂移。

mod api;
mod db_error;

pub use api::{ApiResponse, ok_json};
pub use db_error::{DbErrorKind, classify_db_error};
