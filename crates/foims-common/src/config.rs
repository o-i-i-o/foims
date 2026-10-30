//! 应用配置加载（多路径搜索、环境变量覆盖、时长解析）。

use config::Config as ConfigBuilder;
use serde::{Deserialize, Serialize};
use std::path::Path;

// 配置文件搜索路径（按优先级）：系统配置目录（服务/安装包部署）→ 当前目录（仅开发
// 环境直接运行二进制时使用；systemd 单元不设工作目录，不会命中此项）
//
// 不保留旧版 /opt/foims/config.toml 备用路径：/opt/foims 已废弃（旧静态资源
// 目录，前端资源改到 /usr/share/foims/web），配置只落在 /etc/foims 或开发
// 工作目录，避免"同一份配置存在两个可写副本"导致运维改错文件、升级/回滚
// 互相覆盖。
pub const CONFIG_PATHS: [&str; 2] = [
    "/etc/foims/config", // 系统配置目录（生产唯一落点）- 最高优先级
    "config",            // 当前目录（仅开发直接运行二进制，不用于服务部署）
];

/// 命中的配置路径主干（不含 .toml 后缀，按 [`CONFIG_PATHS`] 优先级）；
/// 全部候选不存在时回退系统配置目录默认值。
#[must_use]
pub fn get_config_path() -> &'static str {
    for path in &CONFIG_PATHS {
        if Path::new(&format!("{path}.toml")).exists() {
            return path;
        }
    }
    "/etc/foims/config"
}

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
/// - nginx 与 axum 通过 UDS + h2c 通信，多路复用 + keepalive 性能优于 HTTP/1.1
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ListenConfig {
    /// UDS socket 文件路径，默认 /run/foims/api.sock
    #[serde(default = "default_uds_path")]
    pub uds_path: String,
    /// UDS socket 属组（反代进程所属组，如 nginx 的 www-data）。
    /// socket 权限固定 0660：仅属主与属组可访问，防止本机任意进程
    /// 直连伪造 X-Real-IP 头访问初始化接口（见 security-review I-1）。
    #[serde(default = "default_uds_group")]
    pub uds_group: String,
}

fn default_uds_path() -> String {
    "/run/foims/api.sock".to_string()
}

fn default_uds_group() -> String {
    "www-data".to_string()
}

impl Default for ListenConfig {
    fn default() -> Self {
        Self {
            uds_path: default_uds_path(),
            uds_group: default_uds_group(),
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
/// - `communities`：v1/v2c community 白名单，空列表时拒绝处理全部 v1/v2c
///   通知（防伪造来源注入站内通知）；
/// - `users`：v3 USM 用户表，为空时拒绝全部 v3 通知；
/// - `cooldown_secs`：同一来源地址的通知冷却窗口，窗口内只记服务日志不重复写站内通知。
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct SnmpTrapConfig {
    #[serde(default)]
    pub enabled: bool,
    /// 监听地址主机部分（IPv4/IPv6/主机名；端口见 [`Self::port`]）。
    #[serde(default = "default_trap_bind_addr")]
    pub bind_addr: String,
    /// 监听端口（标准 trap 端口 162 需 root 权限，可改用非特权端口）。
    #[serde(default = "default_trap_port")]
    pub port: u16,
    #[serde(default)]
    pub communities: Vec<String>,
    #[serde(default)]
    pub users: Vec<SnmpTrapUsmUser>,
    #[serde(default = "default_trap_cooldown_secs")]
    pub cooldown_secs: u64,
}

fn default_trap_bind_addr() -> String {
    "0.0.0.0".to_string()
}

const fn default_trap_port() -> u16 {
    162
}

const fn default_trap_cooldown_secs() -> u64 {
    30
}

impl SnmpTrapConfig {
    /// 监听端点（`host:port` 拼接，供 UDP 绑定与日志输出）。
    pub fn bind_endpoint(&self) -> String {
        format!("{}:{}", self.bind_addr, self.port)
    }
}

impl Default for SnmpTrapConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_addr: default_trap_bind_addr(),
            port: default_trap_port(),
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
    /// 实时采集配置（秒级定向 GET，供 SSE 推送与 Top 榜；不写库）。
    ///
    /// 暂未在系统设置页暴露：PUT /api/system/config 的 snmp 段不带该字段时
    /// 反序列化为 `None`，由调用方沿用磁盘值；[`Config::load`] 在读取配置
    /// 文件后把 `None` 归一为 [`SnmpLiveConfig::default`]（`enabled = true`，
    /// 零配置即可获得实时展示）。
    #[serde(default)]
    pub live: Option<SnmpLiveConfig>,
}

/// SNMP 实时采集配置。
///
/// 与 5 分钟全量轮询解耦：仅按厂商 profile 定向 GET 状态/流量/资源组，
/// 结果只进内存快照（趋势与告警仍由全量轮询落库）。开关关闭时完全不
/// 启动采集任务。
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct SnmpLiveConfig {
    /// 总开关（缺省开启；500 台以内建议保持开启）
    #[serde(default = "default_live_enabled")]
    pub enabled: bool,
    /// 状态/流量组采样间隔（秒，下限 2）
    #[serde(default = "default_live_interval")]
    pub interval_secs: u64,
    /// 资源组（CPU/内存/温度）采样间隔（秒，通常高于状态组以降低负载）
    #[serde(default = "default_live_resource_interval")]
    pub resource_interval_secs: u64,
    /// 单次请求超时（秒，实时路径需短于全量轮询的 5s）
    #[serde(default = "default_live_timeout")]
    pub timeout_secs: u64,
    /// 单轮采样最大并发设备数
    #[serde(default = "default_live_concurrency")]
    pub max_concurrency: usize,
    /// 设备清单与厂商 profile 的刷新间隔（秒）
    #[serde(default = "default_live_inventory_refresh")]
    pub inventory_refresh_secs: u64,
    /// 连续失败后的最大退避（秒）
    #[serde(default = "default_live_max_backoff")]
    pub max_backoff_secs: u64,
}

const fn default_live_enabled() -> bool {
    true
}

const fn default_live_interval() -> u64 {
    5
}

const fn default_live_resource_interval() -> u64 {
    15
}

const fn default_live_timeout() -> u64 {
    2
}

const fn default_live_concurrency() -> usize {
    64
}

const fn default_live_inventory_refresh() -> u64 {
    1800
}

const fn default_live_max_backoff() -> u64 {
    60
}

impl Default for SnmpLiveConfig {
    fn default() -> Self {
        Self {
            enabled: default_live_enabled(),
            interval_secs: default_live_interval(),
            resource_interval_secs: default_live_resource_interval(),
            timeout_secs: default_live_timeout(),
            max_concurrency: default_live_concurrency(),
            inventory_refresh_secs: default_live_inventory_refresh(),
            max_backoff_secs: default_live_max_backoff(),
        }
    }
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
            live: None,
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
    #[serde(default)]
    pub agent: AgentConfig,
}

/// Agent 采集（foims-agent 分发/接收）配置段（docs/agent-design.md）。
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct AgentConfig {
    /// 注册令牌（64 位十六进制明文，与 jwt.secret 同级敏感）：agent 上报
    /// Bearer 令牌与配置包内 token 共用该值，首报自动注册按 machine_id
    /// 判重。启动为空时自动生成并写回配置文件；配置页可查看/重新生成。
    #[serde(default)]
    pub enrollment_token: String,
    /// QUIC 上报监听开关（nginx 不代理 UDP，需防火墙放行监听端口）。
    #[serde(default)]
    pub enabled: bool,
    /// QUIC 监听地址主机部分；[::] 双栈（IPv4-mapped 兼容 IPv4 agent）。
    #[serde(default = "default_agent_bind_addr")]
    pub bind_addr: String,
    /// QUIC 监听端口（UDP，默认 9100）。
    #[serde(default = "default_agent_port")]
    pub port: u16,
    /// 上报间隔下限（秒）：响应控制面下发的 report_interval 不低于该值。
    #[serde(default = "default_agent_report_interval")]
    pub report_interval_secs: u64,
    /// 采集间隔（秒）：agent 本机高频采集节奏，与上报间隔解耦后帧先入
    /// 本地队列，按上报间隔整批发送（生效时 clamp [1,60]）。
    #[serde(default = "default_agent_collect_interval")]
    pub collect_interval_secs: u64,
    /// 单条上报 payload 上限（字节），超出返回 413。
    #[serde(default = "default_agent_max_report_bytes")]
    pub max_report_bytes: usize,
    /// host_metrics_history 保留天数（清理任务按此删除旧行）。
    #[serde(default = "default_agent_history_retention_days")]
    pub history_retention_days: u32,
    /// snmp_events 保留天数（同一清理任务按此删除旧事件）。
    #[serde(default = "default_agent_history_retention_days")]
    pub snmp_event_retention_days: u32,
    /// 离线判定倍数：last_seen 超过上报间隔 × 该倍数置为 offline。
    #[serde(default = "default_agent_offline_factor")]
    pub offline_factor: u32,
    /// 上报地址模式开关：开启后下载包内上报地址取系统配置 server.public_url
    /// 的主机名 + 监听端口（监听地址端口可能经代理/NAT 映射，并非真实上报地址）。
    #[serde(default)]
    pub use_public_url: bool,
    /// 手动上报地址覆盖（开关关闭时生效）：host:port 形式，留空回退监听地址推导。
    #[serde(default)]
    pub report_url_override: String,
}

fn default_agent_bind_addr() -> String {
    "[::]".to_string()
}

const fn default_agent_port() -> u16 {
    9100
}

const fn default_agent_report_interval() -> u64 {
    60
}

const fn default_agent_collect_interval() -> u64 {
    5
}

const fn default_agent_max_report_bytes() -> usize {
    262_144
}

const fn default_agent_history_retention_days() -> u32 {
    30
}

const fn default_agent_offline_factor() -> u32 {
    3
}

impl AgentConfig {
    /// 监听端点（`host:port` 拼接，供 QUIC 绑定与日志输出）。
    pub fn bind_endpoint(&self) -> String {
        format!("{}:{}", self.bind_addr, self.port)
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            enrollment_token: String::new(),
            enabled: false,
            bind_addr: default_agent_bind_addr(),
            port: default_agent_port(),
            report_interval_secs: default_agent_report_interval(),
            collect_interval_secs: default_agent_collect_interval(),
            max_report_bytes: default_agent_max_report_bytes(),
            history_retention_days: default_agent_history_retention_days(),
            snmp_event_retention_days: default_agent_history_retention_days(),
            offline_factor: default_agent_offline_factor(),
            use_public_url: false,
            report_url_override: String::new(),
        }
    }
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
    /// 加载配置：按 [`CONFIG_PATHS`] 命中文件 → `FOIMS_*` 环境变量覆盖 →
    /// 缺段归一（[snmp.live] 补默认）→ 关键项校验（database.host 非空、
    /// jwt.secret ≥32 字节），任一校验失败返回 Err 阻止启动。
    pub fn load() -> Result<Self, config::ConfigError> {
        let mut builder = ConfigBuilder::builder();

        let config_path = get_config_path();
        let config_file = format!("{config_path}.toml");

        if Path::new(&config_file).exists() {
            builder = builder.add_source(config::File::with_name(config_path));
        }

        // 环境变量优先于配置文件；单下划线即嵌套分隔符（separator="_" 映射为 "."），支持：
        // - FOIMS_DATABASE_PASSWORD（覆盖 database.password）
        // - FOIMS_JWT_SECRET（覆盖 jwt.secret）
        builder = builder.add_source(
            config::Environment::default()
                .prefix("FOIMS")
                .separator("_")
                .try_parsing(true),
        );

        let config = builder.build()?;
        let mut config: Self = config.try_deserialize()?;

        // [snmp.live] 的缺省归一口径见 SnmpLiveConfig 字段注释（反序列化保持 None）
        if config.snmp.live.is_none() {
            config.snmp.live = Some(SnmpLiveConfig::default());
        }

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
