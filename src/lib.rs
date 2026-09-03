//! FOIMS 库入口：应用组装层（二进制与其他测试使用）。
//!
//! 业务实现已按领域拆分为独立 crate（依赖方向自上而下）：
//!
//! ```text
//! main（本 crate：路由装配/系统管理/日志/可视化包装/app_state）
//!   ├── foims-resource        资源管理（子网/房间/设备/IP/链路…）
//!   ├── foims-organization    组织管理（组织树/员工/模板）
//!   ├── foims-auth            认证与用户管理（登录/JWT/fail2ban/SMTP/操作日志）
//!   ├── foims-visualization   拓扑与布局计算
//!   ├── foims-data-management CSV 导入导出与备份
//!   ├── foims-scheduler       定时任务调度
//!   ├── foims-init            数据库初始化与校验
//!   ├── foims-x509-management    证书管理
//!   ├── foims-services       systemd 服务管理（foims/nginx）
//!   ├── foims-models          领域模型（请求/响应/行模型）
//!   └── foims-common          共享基础设施（响应/错误/配置/加密/连接池/限流/网络工具）
//! ```
//!
//! 跨 crate 状态访问经依赖倒置：业务 crate 定义/使用 `DbProvider`
//! （连接池）与 `AuthProvider`（认证扩展）trait，由本 crate 的
//! [`app_state::AppState`] 实现。

// FOIMS - Organization IT Information Management System
// Copyright (c) 2024-2025 oi-io <boss@oi-io.cc>
// SPDX-License-Identifier: MIT

// 国际化支持
#[macro_use]
extern crate rust_i18n;

// 初始化国际化支持（日志文案；缺失语言回退英文）
i18n!("src/i18n", fallback = "en");

pub mod app_state;
pub mod log;
pub mod routes;
pub mod shutdown;
pub mod system;
pub mod utils;
