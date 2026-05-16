// IPMA - IP/MAC Address Management System
// Copyright (c) 2024-2025 oi-io <boss@oi-io.cc>
// SPDX-License-Identifier: MIT

// 国际化支持
#[macro_use]
extern crate rust_i18n;

// 初始化国际化支持
i18n!("src/i18n");

pub mod app_state;
pub mod auth;
pub mod config;
pub mod crypto;
pub mod db;
pub mod error;
pub mod init;
pub mod log;
pub mod models;
pub mod resource;
pub mod routes;
pub mod system;
pub mod utils;
