//! 系统管理模块（配置/证书/定时任务/fail2ban）。

// 核心功能模块
pub mod app_fail2ban;
pub mod certificate;
pub mod config;
pub mod scheduled_task;
pub mod smtp;
pub mod task_executors;
