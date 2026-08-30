//! FOIMS 领域模型库。
//!
//! 汇聚全部业务资源域的请求/响应模型与数据库行模型（`foims_models::X`），
//! 由主程序与各业务 crate 共享；内部结构保持原 `models` 模块布局，
//! serde 校验函数路径（`crate::models::...`）无需调整。

pub mod models;

pub use models::*;
