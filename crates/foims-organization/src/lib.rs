//! 组织与人员业务模块（与资源管理 `resource` 平级）：组织树、员工与
//! 组织类型模板；本文件仅做模块组装与路径再导出。

// 测试代码允许 unwrap/expect（AGENTS.md「测试代码除外」）；生产代码仍由
// workspace lints + CI `-D warnings` 强制禁止。cfg(test) 仅作用于测试构建。
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod employee;
pub mod org_template;
pub mod organization;

pub use employee::*;
pub use org_template::*;
pub use organization::*;

// 员工与组织模块共用的空串归一化辅助（保持 `crate::blank_to_none` 路径）
pub(crate) use organization::blank_to_none;
