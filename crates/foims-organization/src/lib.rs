//! 组织与人员模块（组织架构/员工/组织模板）。
//!
//! 与资源管理（`resource`）平级的业务模块：组织树 CRUD 见 `organization`
//! 子模块，员工挂在组织节点下（`employee`），组织类型模板见 `org_template`。
//! 本文件仅做模块组装与路径再导出（`foims_organization::*` 保持不变）。

pub mod employee;
pub mod org_template;
pub mod organization;

pub use employee::*;
pub use org_template::*;
pub use organization::*;

// 员工与组织模块共用的空串归一化辅助（保持 `crate::blank_to_none` 路径）
pub(crate) use organization::blank_to_none;
