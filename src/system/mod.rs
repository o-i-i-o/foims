//! 系统管理模块（配置/证书/服务管理/定时任务；应用层 fail2ban 见 foims-auth）。

// 核心功能模块
pub mod certificate;
pub mod config;
pub mod scheduled_task;
pub mod services;
pub mod task_executors;
