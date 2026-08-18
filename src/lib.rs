//! IPMA 库入口：汇聚全部子模块供二进制与其他测试使用。

// IPMA - IP/DEVICE Address Management System
// Copyright (c) 2024-2025 oi-io <boss@oi-io.cc>
// SPDX-License-Identifier: MIT

// 国际化支持
#[macro_use]
extern crate rust_i18n;

// 初始化国际化支持（日志文案；缺失语言回退英文）
i18n!("src/i18n", fallback = "en");

pub mod app_state;
pub mod auth;
pub mod config;
pub mod crypto;
pub mod db;
pub mod error;
pub mod log;
pub mod models;
pub mod resource;
pub mod routes;
pub mod shutdown;
pub mod system;
pub mod utils;
