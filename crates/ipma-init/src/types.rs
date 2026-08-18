//! 初始化模块请求/响应类型。

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const VERIFICATION_CODE_EXPIRY_SECS: u64 = 15 * 60;
pub const BCRYPT_COST: u32 = 12;

#[derive(Debug, Clone)]
pub struct VerificationCode {
    pub code: String,
    pub created_at: u64,
}

impl VerificationCode {
    #[must_use]
    pub fn new(code: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|e| {
                ipma_common::log_warn!("log.init.system_time_warning", error = e);
                std::time::Duration::from_secs(0)
            })
            .as_secs();
        Self {
            code,
            created_at: now,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, validator::Validate)]
pub struct InitRequest {
    #[validate(length(min = 3, max = 50, message = "server.init.validation.username_length"))]
    pub username: String,
    #[validate(length(min = 8, message = "server.init.validation.password_length"))]
    pub password: String,
    #[validate(email(message = "server.init.validation.email_invalid"))]
    pub email: String,
    #[validate(length(min = 1, max = 20, message = "server.init.validation.role_length"))]
    pub role: String,
    #[validate(length(
        min = 16,
        max = 16,
        message = "server.init.validation.verification_length"
    ))]
    pub verification: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDatabaseRequest {
    pub verification: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportDatabaseRequest {
    pub verification: String,
}

#[derive(Debug, Serialize)]
pub struct CreateDatabaseResponse {
    pub backup_file: Option<String>,
    /// 消息 key（由前端翻译），与外层 ApiResponse.message 一致
    pub message: String,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_acquire_timeout")]
    pub acquire_timeout_secs: u64,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
    #[serde(default = "default_max_lifetime")]
    pub max_lifetime_secs: u64,
    #[serde(default = "default_query_timeout")]
    pub query_timeout_secs: u64,
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval_secs: u64,
}

const fn default_max_connections() -> u32 {
    10
}

const fn default_min_connections() -> u32 {
    5
}

const fn default_acquire_timeout() -> u64 {
    15
}

const fn default_idle_timeout() -> u64 {
    60
}

const fn default_max_lifetime() -> u64 {
    1800
}

const fn default_query_timeout() -> u64 {
    30
}

const fn default_health_check_interval() -> u64 {
    30
}
