use config::Config as ConfigBuilder;
use serde::{Deserialize, Serialize};
use std::path::Path;

// 配置文件搜索路径（按优先级）
pub const CONFIG_PATHS: [&str; 3] = [
    "/etc/ipma/config", // 系统配置目录（生产环境）- 最高优先级
    "/opt/ipma/config", // 应用目录（备用）
    "config",           // 当前目录（开发环境）
];

// 获取配置文件路径
#[must_use] 
pub fn get_config_path() -> &'static str {
    for path in &CONFIG_PATHS {
        if Path::new(&format!("{path}.toml")).exists() {
            return path;
        }
    }
    // 默认返回系统配置目录
    "/etc/ipma/config"
}

// 获取配置文件完整路径
#[must_use] 
pub fn get_config_file_path() -> String {
    format!("{}.toml", get_config_path())
}

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

const fn default_query_timeout() -> u64 {
    30
}

const fn default_slow_query_threshold() -> u64 {
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
    pub public_url: String,           // 服务器公共URL，用于构建重置链接等
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
pub struct RateLimitConfig {
    #[serde(default = "default_ip_limit")]
    pub ip_limit: u32,
    #[serde(default = "default_user_limit")]
    pub user_limit: u32,
    #[serde(default = "default_login_limit")]
    pub login_limit: u32,
    #[serde(default = "default_window_secs")]
    pub window_secs: u64,
    #[serde(default = "default_rate_limit_enabled")]
    pub enabled: bool,
}

const fn default_ip_limit() -> u32 {
    100
}
const fn default_user_limit() -> u32 {
    200
}
const fn default_login_limit() -> u32 {
    5
}
const fn default_window_secs() -> u64 {
    60
}
const fn default_rate_limit_enabled() -> bool {
    true
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            ip_limit: default_ip_limit(),
            user_limit: default_user_limit(),
            login_limit: default_login_limit(),
            window_secs: default_window_secs(),
            enabled: default_rate_limit_enabled(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub jwt: JwtConfig,
    pub init: InitConfig,
    pub i18n: Option<I18nConfig>,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

impl Config {
    pub fn load() -> Result<Self, config::ConfigError> {
        // 构建配置加载器
        let mut builder = ConfigBuilder::builder();

        // 首先加载配置文件（如果存在）
        let config_path = get_config_path();
        let config_file = format!("{config_path}.toml");

        if Path::new(&config_file).exists() {
            builder = builder.add_source(config::File::with_name(config_path));
        }

        // 然后加载环境变量（优先级高于配置文件）
        builder = builder.add_source(
            config::Environment::default()
                .prefix("IPMA")
                .separator("_")
                .try_parsing(true),
        );

        // 构建配置
        let config = builder.build()?;

        // 尝试反序列化
        let config: Self = config.try_deserialize()?;

        // 验证关键配置
        if config.database.host.is_empty() {
            return Err(config::ConfigError::Message(
                "Database host is required".to_string(),
            ));
        }

        if config.jwt.secret.len() < 32 {
            return Err(config::ConfigError::Message(
                "JWT secret must be at least 32 characters long".to_string(),
            ));
        }

        Ok(config)
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
