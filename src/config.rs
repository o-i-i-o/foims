use config::Config as ConfigBuilder;
use regex;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub max_connections: u32,
    #[serde(default = "default_query_timeout")]
    pub query_timeout_secs: u64,
    #[serde(default = "default_slow_query_threshold")]
    pub slow_query_threshold_ms: u64,
}

fn default_query_timeout() -> u64 {
    30
}

fn default_slow_query_threshold() -> u64 {
    1000
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ServerConfig {
    pub host: String,              // IPv4地址
    pub host_ipv6: Option<String>, // IPv6地址
    pub http_enabled: Option<bool>,
    pub http_port: Option<u16>,
    pub https_enabled: Option<bool>,
    pub https_port: Option<u16>,
    pub auto_https: Option<bool>,
    pub http_version: Option<String>,
    pub cert_type: Option<String>,
    pub public_url: String,         // 服务器公共URL，用于构建重置链接等
    pub session_timeout: Option<u64>, // 会话超时时间（分钟）
    pub page_timeout: Option<u64>,    // 页面超时时间（分钟）
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct JwtConfig {
    pub secret: String,
    pub access_token_expiry: String, // 带单位的过期时间，如"15m"表示15分钟
    pub refresh_token_expiry: String, // 带单位的过期时间，如"7d"表示7天
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct InitConfig {
    pub enabled: bool,
}



#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct I18nConfig {
    pub default_language: String,
    pub supported_languages: Vec<String>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub jwt: JwtConfig,
    pub init: InitConfig,
    pub i18n: Option<I18nConfig>,
}

impl Config {
    pub fn load() -> Result<Self, config::ConfigError> {
        // 检查是否存在 .env 文件
        let env_file = Path::new(".env");
        let builder = if env_file.exists() {
            ConfigBuilder::builder().add_source(config::Environment::default())
        } else {
            ConfigBuilder::builder()
                .add_source(config::File::with_name("config"))
                .add_source(config::Environment::default())
        };

        let config = builder.build()?;

        // 直接尝试反序列化
        config.try_deserialize()
    }
}

// 解析带单位的时间字符串为秒数
pub fn parse_duration(duration_str: &str) -> Result<u64, String> {
    let re = regex::Regex::new(r"^(\d+)([smhd])$").unwrap();
    if let Some(captures) = re.captures(duration_str) {
        let value: u64 = captures[1].parse().map_err(|_| "Invalid duration value")?;
        let unit = &captures[2];

        match unit {
            "s" => Ok(value),         // 秒
            "m" => Ok(value * 60),    // 分钟
            "h" => Ok(value * 3600),  // 小时
            "d" => Ok(value * 86400), // 天
            _ => Err("Invalid time unit".to_string()),
        }
    } else {
        Err("Invalid duration format".to_string())
    }
}
