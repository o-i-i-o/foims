//! 应用配置加载（多路径搜索、环境变量覆盖、时长解析）。

use config::Config as ConfigBuilder;
use serde::{Deserialize, Serialize};
use std::path::Path;

// 配置文件搜索路径（按优先级）
pub const CONFIG_PATHS: [&str; 3] = [
    "/etc/foims/config", // 系统配置目录（生产环境）- 最高优先级
    "/opt/foims/config", // 应用目录（备用）
    "config",            // 当前目录（开发环境）
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
    "/etc/foims/config"
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

/// 监听配置（UDS + h2c 模式）
///
/// 架构说明：
/// - axum 监听 Unix Domain Socket，使用 h2c (HTTP/2 cleartext) 协议
/// - nginx 通过 `proxy_http_version 2.0` 以 h2c 反代到 axum
/// - 静态文件由 nginx 直接托管，axum 仅服务 API
/// - 调试时手动启动 axum 即可，无需安装 systemd 服务
/// - nginx 与 axum 通过 UDS + h2c 通信，多路复用 + keepalive 性能优于 HTTP/1.1
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ListenConfig {
    /// UDS socket 文件路径，默认 /run/foims/api.sock
    /// 调试时可改为 /tmp/foims-dev.sock 避免权限问题
    #[serde(default = "default_uds_path")]
    pub uds_path: String,
    /// UDS socket 属组（反代进程所属组，如 nginx 的 www-data）。
    /// socket 权限固定 0660：仅属主与属组可访问，防止本机任意进程
    /// 直连伪造 X-Real-IP 头访问初始化接口（见 security-review I-1）。
    #[serde(default = "default_uds_group")]
    pub uds_group: String,
    /// 是否托管静态文件
    /// - false（默认）：由 nginx 托管静态文件，axum 仅服务 API（生产模式）
    /// - true：axum 同时托管静态文件和 API（调试模式，可用 curl 验证）
    #[serde(default = "default_serve_static")]
    pub serve_static: bool,
}

fn default_uds_path() -> String {
    "/run/foims/api.sock".to_string()
}

fn default_uds_group() -> String {
    "www-data".to_string()
}

const fn default_serve_static() -> bool {
    false
}

impl Default for ListenConfig {
    fn default() -> Self {
        Self {
            uds_path: default_uds_path(),
            uds_group: default_uds_group(),
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

fn default_log_language() -> String {
    "en".to_string()
}

fn default_supported_languages() -> Vec<String> {
    vec!["zh".to_string(), "en".to_string()]
}

/// i18n / 日志语言配置。
///
/// - `log_language`：控制台日志语言，缺省为 `"en"`；
/// - `supported_languages`：系统支持的语言集合（同时约束日志语言取值）；
/// - `logfiles_i18n_out`：日志文件输出的语言集合（每个语言一个独立文件），
///   缺省时跟随 `log_language`；取值必须是 `supported_languages` 的子集，
///   否则视为配置错误，控制台与日志文件均回退为仅输出英文日志。
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct I18nConfig {
    #[serde(default = "default_log_language")]
    pub log_language: String,
    #[serde(default = "default_supported_languages")]
    pub supported_languages: Vec<String>,
    #[serde(default)]
    pub logfiles_i18n_out: Option<Vec<String>>,
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

/// SNMP Trap/Inform 接收的 v3 USM 用户凭据。
///
/// `auth_protocol` 为空表示 noAuth，`priv_protocol` 为空表示 noPriv；
/// 配置加密协议时必须同时配置认证协议。
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct SnmpTrapUsmUser {
    pub username: String,
    #[serde(default)]
    pub auth_protocol: String,
    #[serde(default)]
    pub auth_password: String,
    #[serde(default)]
    pub priv_protocol: String,
    #[serde(default)]
    pub priv_password: String,
}

/// SNMP Trap/Inform 接收配置。
///
/// - `communities`：v1/v2c community 白名单，空列表表示接受任意 community；
/// - `users`：v3 USM 用户表，为空时拒绝全部 v3 通知；
/// - `cooldown_secs`：同一来源地址的通知冷却窗口，窗口内只记服务日志不重复写站内通知。
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct SnmpTrapConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_trap_bind_addr")]
    pub bind_addr: String,
    #[serde(default)]
    pub communities: Vec<String>,
    #[serde(default)]
    pub users: Vec<SnmpTrapUsmUser>,
    #[serde(default = "default_trap_cooldown_secs")]
    pub cooldown_secs: u64,
}

fn default_trap_bind_addr() -> String {
    "0.0.0.0:162".to_string()
}

const fn default_trap_cooldown_secs() -> u64 {
    30
}

impl Default for SnmpTrapConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_addr: default_trap_bind_addr(),
            communities: Vec::new(),
            users: Vec::new(),
            cooldown_secs: default_trap_cooldown_secs(),
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
    /// Trap/Inform 接收配置（缺省关闭）
    #[serde(default)]
    pub trap: SnmpTrapConfig,
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
            trap: SnmpTrapConfig::default(),
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

/// 进程内共享配置槽：启动时装入初始配置，各「写盘」端点成功落盘后
/// 调用 `store` 刷新，读取方经 `load` / `load_full` 始终拿到最新快照，
/// 消除「启动内存快照与磁盘配置脱节」问题。
///
/// `ArcSwap` 经本模块再导出，主程序 crate 无需声明 arc-swap 直接依赖
/// 即可使用该类型（跨 crate 复用类型按规范统一放 foims-common）。
pub type SharedConfig = std::sync::Arc<ArcSwap<Config>>;

pub use arc_swap::ArcSwap;

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
        // - FOIMS_DATABASE__PASSWORD (注意双下划线表示嵌套)
        // - FOIMS_DATABASE_PASSWORD
        // - FOIMS_JWT__SECRET (注意双下划线表示嵌套)
        // - FOIMS_JWT_SECRET
        builder = builder.add_source(
            config::Environment::default()
                .prefix("FOIMS")
                .separator("_")
                .try_parsing(true),
        );

        // 构建配置
        let config = builder.build()?;

        // 尝试反序列化
        let mut config: Self = config.try_deserialize()?;

        // 环境变量覆盖敏感信息
        if let Ok(db_password) = std::env::var("FOIMS_DATABASE_PASSWORD")
            && !db_password.is_empty()
        {
            config.database.password = db_password;
        }

        if let Ok(jwt_secret) = std::env::var("FOIMS_JWT_SECRET")
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

/// 解析带单位的时间字符串为秒数，如 `"90s"`、`"5m"`、`"2h"`、`"3d"`。
///
/// 手工解析而非正则：模式固定且简单，可避免静态 Regex 初始化的
/// panic 语义，并用 `checked_mul` 拦截超大数值溢出。
pub fn parse_duration(duration_str: &str) -> Result<u64, String> {
    let Some(unit) = duration_str.chars().last() else {
        return Err("Invalid duration format".to_string());
    };
    let unit_secs = match unit {
        's' => 1u64,   // 秒
        'm' => 60,     // 分钟
        'h' => 3600,   // 小时
        'd' => 86_400, // 天
        _ => return Err("Invalid duration format".to_string()),
    };

    let digits = &duration_str[..duration_str.len() - unit.len_utf8()];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Invalid duration format".to_string());
    }

    let value: u64 = digits
        .parse()
        .map_err(|_| "Invalid duration value".to_string())?;
    value
        .checked_mul(unit_secs)
        .ok_or_else(|| "Invalid duration value".to_string())
}
