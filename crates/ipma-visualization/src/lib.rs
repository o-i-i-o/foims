//! IPMA 可视化：机房布局与拓扑数据管理。
//!
//! `layout` / `topology` 为业务与数据层；`http` 为 axum handler 层
//! （面向 `P: DbProvider` 泛型，由主程序 AppState 实现），经模块路径
//! 访问以避免与业务函数同名冲突。

pub mod http;
pub mod layout;
pub mod topology;

pub use layout::*;
pub use topology::*;
