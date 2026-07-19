use config::Config as ConfigBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::LazyLock;

static DURATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)([smhd])$").expect("failed to compile duration regex"));

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
    #[serde(default = "default_slow_query_threshold")]
    pub slow_query_threshold_ms: u64,
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

const fn default_slow_query_threshold() -> u64 {
    1000
}

const fn default_health_check_interval() -> u64 {
    30
}

/// 监听配置（仅 UDS 模式）
///
/// 架构说明：
/// - actix-web 监听 Unix Domain Socket，由 nginx 反代
/// - 静态文件由 nginx 直接托管，actix 仅服务 API
/// - 调试时手动启动 actix 即可，无需安装 systemd 服务
/// - nginx 与 actix 通过 UDS 通信，性能优于 TCP loopback
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ListenConfig {
    /// UDS socket 文件路径，默认 /run/ipma/api.sock
    /// 调试时可改为 /tmp/ipma-dev.sock 避免权限问题
    #[serde(default = "default_uds_path")]
    pub uds_path: String,
    /// 是否托管静态文件
    /// - false（默认）：由 nginx 托管静态文件，actix 仅服务 API（生产模式）
    /// - true：actix 同时托管静态文件和 API（调试模式，可用 curl 验证）
    #[serde(default = "default_serve_static")]
    pub serve_static: bool,
}

fn default_uds_path() -> String {
    "/run/ipma/api.sock".to_string()
}

const fn default_serve_static() -> bool {
    false
}

impl Default for ListenConfig {
    fn default() -> Self {
        Self {
            uds_path: default_uds_path(),
            serve_static: default_serve_static(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ServerConfig {
    pub host: String,                 // IPv4地址（保留用于公共 URL 解析）
    pub host_ipv6: Option<String>,    // IPv6地址（保留用于公共 URL 解析）
    pub public_url: String,           // 服务器公共URL，用于构建重置链接等
    pub session_timeout: Option<u64>, // 会话超时时间（分钟）
    pub page_timeout: Option<u64>,    // 页面超时时间（分钟）
    #[serde(default)]
    pub cors_allowed_origins: Vec<String>, // CORS允许的源列表
    #[serde(default)]
    pub allow_localhost_cors: bool, // 是否允许localhost/127.0.0.1/[::1]跨域（仅开发环境启用）
    /// 监听配置（UDS 模式）
    #[serde(default)]
    pub listen: ListenConfig,
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
    #[serde(default = "default_email_limit")]
    pub email_limit: u32,
    #[serde(default = "default_email_window_secs")]
    pub email_window_secs: u64,
    #[serde(default = "default_rate_limit_enabled")]
    pub enabled: bool,
}

const fn default_ip_limit() -> u32 {
    1000
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
const fn default_email_limit() -> u32 {
    5
}
const fn default_email_window_secs() -> u64 {
    3600
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
            email_limit: default_email_limit(),
            email_window_secs: default_email_window_secs(),
            enabled: default_rate_limit_enabled(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct SnmpConfig {
    #[serde(default = "default_snmp_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_snmp_retries")]
    pub retries: u32,
    #[serde(default = "default_lldp_timeout")]
    pub lldp_timeout_secs: u64,
    #[serde(default = "default_mac_scan_timeout")]
    pub mac_scan_timeout_secs: u64,
}

const fn default_snmp_timeout() -> u64 {
    5
}

const fn default_snmp_retries() -> u32 {
    3
}

const fn default_lldp_timeout() -> u64 {
    30
}

const fn default_mac_scan_timeout() -> u64 {
    10
}

impl Default for SnmpConfig {
    fn default() -> Self {
        Self {
            timeout_secs: default_snmp_timeout(),
            retries: default_snmp_retries(),
            lldp_timeout_secs: default_lldp_timeout(),
            mac_scan_timeout_secs: default_mac_scan_timeout(),
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
    #[serde(default)]
    pub snmp: SnmpConfig,
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
        // 支持的环境变量：
        // - IPMA_DATABASE__PASSWORD (注意双下划线表示嵌套)
        // - IPMA_DATABASE_PASSWORD
        // - IPMA_JWT__SECRET (注意双下划线表示嵌套)
        // - IPMA_JWT_SECRET
        builder = builder.add_source(
            config::Environment::default()
                .prefix("IPMA")
                .separator("_")
                .try_parsing(true),
        );

        // 构建配置
        let config = builder.build()?;

        // 尝试反序列化
        let mut config: Self = config.try_deserialize()?;

        // 环境变量覆盖敏感信息
        if let Ok(db_password) = std::env::var("IPMA_DATABASE_PASSWORD")
            && !db_password.is_empty()
        {
            config.database.password = db_password;
        }

        if let Ok(jwt_secret) = std::env::var("IPMA_JWT_SECRET")
            && !jwt_secret.is_empty()
        {
            config.jwt.secret = jwt_secret;
        }

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
    if let Some(captures) = DURATION_RE.captures(duration_str) {
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
