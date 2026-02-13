// 国际化支持
#[macro_use]
extern crate rust_i18n;

// 初始化国际化支持
i18n!("src/i18n");

pub mod auth;
pub mod config;
pub mod crypto;
pub mod db;
pub mod log;
pub mod models;
pub mod resource;
pub mod routes;
pub mod system;
pub mod utils;
