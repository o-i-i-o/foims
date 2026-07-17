use crate::error::AppError;
use crate::models::ApiResponse;
use actix_web::{HttpResponse, web};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use validator::Validate;

/// 应用层 Fail2ban 日志文件路径（供 OS fail2ban 监控）
const AUTH_LOG_PATH: &str = "/var/log/ipma/auth.log";

/// 应用层 Fail2ban 配置
#[derive(Debug, Clone)]
pub struct Fail2banConfig {
    /// 检测时间窗口（秒），默认 600
    pub findtime: u64,
    /// 最大失败次数，默认 5
    pub max_retry: u64,
    /// 封禁时长（秒），默认 1800
    pub bantime: u64,
    /// 是否启用，默认 true
    pub enabled: bool,
}

impl Default for Fail2banConfig {
    fn default() -> Self {
        Self {
            findtime: 600,
            max_retry: 5,
            bantime: 1800,
            enabled: true,
        }
    }
}

/// 单个 IP 的失败记录
#[derive(Debug, Clone)]
struct FailRecord {
    /// 失败时间戳列表
    failures: Vec<Instant>,
    /// 封禁到期时间（若存在）
    banned_until: Option<Instant>,
}

impl FailRecord {
    fn new() -> Self {
        Self {
            failures: Vec::new(),
            banned_until: None,
        }
    }
}

/// 应用层 Fail2ban 全局状态
pub struct AppFail2ban {
    /// IP -> 失败记录
    records: Mutex<HashMap<String, FailRecord>>,
    /// 配置
    config: Mutex<Fail2banConfig>,
}

impl AppFail2ban {
    fn new() -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
            config: Mutex::new(Fail2banConfig::default()),
        }
    }
}

/// 获取全局实例
static APP_FAIL2BAN: std::sync::OnceLock<AppFail2ban> = std::sync::OnceLock::new();

fn app_fail2ban() -> &'static AppFail2ban {
    APP_FAIL2BAN.get_or_init(AppFail2ban::new)
}

/// 启动后台清理任务，定期清除过期记录
pub fn start_cleanup_task() {
    tokio::spawn(async {
        // 每 5 分钟清理一次过期记录
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            cleanup_expired_records();
        }
    });
}

/// 清理过期的失败记录和封禁记录
fn cleanup_expired_records() {
    let store = app_fail2ban();
    let config = store.config.lock().map(|c| c.clone()).unwrap_or_default();
    let now = Instant::now();
    let findtime_dur = Duration::from_secs(config.findtime);

    if let Ok(mut records) = store.records.lock() {
        records.retain(|_, record| {
            // 清除过期的失败记录
            record
                .failures
                .retain(|&t| now.duration_since(t) < findtime_dur);
            // 清除已到期的封禁
            if let Some(until) = record.banned_until
                && now >= until
            {
                record.banned_until = None;
            }
            // 保留还有失败记录或仍在封禁中的 IP
            !record.failures.is_empty() || record.banned_until.is_some()
        });
    }
}

/// 检查 IP 是否被封禁
pub fn is_ip_banned(ip: &str) -> bool {
    let store = app_fail2ban();
    let config = store.config.lock().map(|c| c.clone()).unwrap_or_default();
    if !config.enabled {
        return false;
    }

    if let Ok(records) = store.records.lock()
        && let Some(record) = records.get(ip)
        && let Some(until) = record.banned_until
    {
        return Instant::now() < until;
    }
    false
}

/// 获取封禁剩余时间（秒），未封禁返回 0
pub fn get_ban_remaining(ip: &str) -> u64 {
    let store = app_fail2ban();
    if let Ok(records) = store.records.lock()
        && let Some(record) = records.get(ip)
        && let Some(until) = record.banned_until
    {
        let now = Instant::now();
        if now < until {
            return until.duration_since(now).as_secs();
        }
    }
    0
}

/// 写入认证日志（供 OS fail2ban 监控）
///
/// 日志格式：`2026-07-17T12:00:00Z [FAIL] 192.168.1.100 - login failed for admin`
/// OS fail2ban 可配置 filter 正则：`^\[FAIL\] <HOST> - login failed`
fn write_auth_log(success: bool, ip: &str, username: &str, reason: Option<&str>) {
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let status = if success { "[OK]" } else { "[FAIL]" };
    let reason_str = reason.map(|r| format!(" - {r}")).unwrap_or_default();
    let log_line = format!("{timestamp} {status} {ip} - user: {username}{reason_str}\n");

    // 异步写入日志文件，失败不影响主流程
    let line = log_line.clone();
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        // 确保日志目录存在
        if let Some(parent) = std::path::Path::new(AUTH_LOG_PATH).parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        // 以追加模式打开文件，失败则记录 warning
        match tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(AUTH_LOG_PATH)
            .await
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(line.as_bytes()).await {
                    tracing::warn!("写入认证日志失败: {}", e);
                }
            }
            Err(e) => {
                tracing::warn!("打开认证日志文件 {} 失败: {}", AUTH_LOG_PATH, e);
            }
        }
    });
}

/// 记录一次登录失败，返回是否因此次失败而被封禁
pub fn record_login_failure(ip: &str, username: &str, reason: &str) -> bool {
    // 写入日志文件（供 OS fail2ban 监控）
    write_auth_log(false, ip, username, Some(reason));

    let store = app_fail2ban();
    let config = store.config.lock().map(|c| c.clone()).unwrap_or_default();
    if !config.enabled {
        return false;
    }

    let now = Instant::now();
    let findtime_dur = Duration::from_secs(config.findtime);

    if let Ok(mut records) = store.records.lock() {
        let record = records
            .entry(ip.to_string())
            .or_insert_with(FailRecord::new);
        // 清除过期的失败记录
        record
            .failures
            .retain(|&t| now.duration_since(t) < findtime_dur);
        // 记录本次失败
        record.failures.push(now);

        // 检查是否达到封禁阈值
        if record.failures.len() as u64 >= config.max_retry {
            record.banned_until = Some(now + Duration::from_secs(config.bantime));
            tracing::warn!(
                "IP {} 登录失败 {} 次，已封禁 {} 秒",
                ip,
                record.failures.len(),
                config.bantime
            );
            return true;
        }
    }
    false
}

/// 记录登录成功，清除该 IP 的失败记录
pub fn record_login_success(ip: &str, username: &str) {
    // 写入日志文件（供 OS fail2ban 监控）
    write_auth_log(true, ip, username, None);

    let store = app_fail2ban();
    if let Ok(mut records) = store.records.lock() {
        records.remove(ip);
    }
}

// ==================== API 数据结构 ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct AppFail2banStatus {
    pub enabled: bool,
    pub findtime: u64,
    pub max_retry: u64,
    pub bantime: u64,
    pub log_path: String,
    pub banned_ips: Vec<BannedIpInfo>,
    pub tracked_ips: Vec<TrackedIpInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BannedIpInfo {
    pub ip: String,
    pub remaining_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrackedIpInfo {
    pub ip: String,
    pub failure_count: u64,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateFail2banConfigRequest {
    pub enabled: Option<bool>,
    #[validate(range(min = 60, max = 86400, message = "检测时间必须在60到86400秒之间"))]
    pub findtime: Option<u64>,
    #[validate(range(min = 1, max = 100, message = "最大重试次数必须在1到100之间"))]
    pub max_retry: Option<u64>,
    #[validate(range(min = 60, max = 604800, message = "封禁时长必须在60到604800秒之间"))]
    pub bantime: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BanIpRequest {
    pub ip: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UnbanIpRequest {
    pub ip: String,
}

// ==================== API Handler ====================

/// 获取应用层 fail2ban 状态（仅管理员）
pub async fn get_app_fail2ban_status(
    _admin: crate::auth::extractor::AdminUser,
) -> Result<HttpResponse, AppError> {
    let store = app_fail2ban();
    let config = store.config.lock().map(|c| c.clone()).unwrap_or_default();

    let mut banned_ips: Vec<BannedIpInfo> = vec![];
    let mut tracked_ips: Vec<TrackedIpInfo> = vec![];
    let now = Instant::now();

    if let Ok(records) = store.records.lock() {
        for (ip, record) in records.iter() {
            if let Some(until) = record.banned_until
                && now < until
            {
                banned_ips.push(BannedIpInfo {
                    ip: ip.clone(),
                    remaining_seconds: until.duration_since(now).as_secs(),
                });
            }
            if !record.failures.is_empty() && record.banned_until.is_none() {
                tracked_ips.push(TrackedIpInfo {
                    ip: ip.clone(),
                    failure_count: record.failures.len() as u64,
                });
            }
        }
    }

    banned_ips.sort_by_key(|a| a.remaining_seconds);
    tracked_ips.sort_by_key(|b| std::cmp::Reverse(b.failure_count));

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        AppFail2banStatus {
            enabled: config.enabled,
            findtime: config.findtime,
            max_retry: config.max_retry,
            bantime: config.bantime,
            log_path: AUTH_LOG_PATH.to_string(),
            banned_ips,
            tracked_ips,
        },
        "success",
    )))
}

/// 更新应用层 fail2ban 配置（仅管理员）
pub async fn update_app_fail2ban_config(
    _admin: crate::auth::extractor::AdminUser,
    req: web::Json<UpdateFail2banConfigRequest>,
) -> Result<HttpResponse, AppError> {
    req.validate()?;
    let store = app_fail2ban();

    if let Ok(mut config) = store.config.lock() {
        if let Some(enabled) = req.enabled {
            config.enabled = enabled;
        }
        if let Some(findtime) = req.findtime {
            config.findtime = findtime;
        }
        if let Some(max_retry) = req.max_retry {
            config.max_retry = max_retry;
        }
        if let Some(bantime) = req.bantime {
            config.bantime = bantime;
        }
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({"message": "配置已更新"}),
        "success",
    )))
}

/// 手动解封 IP（仅管理员）
pub async fn app_unban_ip(
    _admin: crate::auth::extractor::AdminUser,
    req: web::Json<UnbanIpRequest>,
) -> Result<HttpResponse, AppError> {
    let ip = req.ip.trim().to_string();
    if ip.parse::<std::net::IpAddr>().is_err() {
        return Err(AppError::Validation(format!("无效的IP地址: {ip}")));
    }

    let store = app_fail2ban();
    if let Ok(mut records) = store.records.lock()
        && let Some(record) = records.get_mut(&ip)
    {
        record.banned_until = None;
        record.failures.clear();
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({"message": format!("IP {ip} 已解封"), "ip": ip}),
        "success",
    )))
}

/// 手动封禁 IP（仅管理员）
pub async fn app_ban_ip(
    _admin: crate::auth::extractor::AdminUser,
    req: web::Json<BanIpRequest>,
) -> Result<HttpResponse, AppError> {
    let ip = req.ip.trim().to_string();
    if ip.parse::<std::net::IpAddr>().is_err() {
        return Err(AppError::Validation(format!("无效的IP地址: {ip}")));
    }

    let store = app_fail2ban();
    let config = store.config.lock().map(|c| c.clone()).unwrap_or_default();

    if let Ok(mut records) = store.records.lock() {
        let record = records.entry(ip.clone()).or_insert_with(FailRecord::new);
        record.banned_until = Some(Instant::now() + Duration::from_secs(config.bantime));
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({"message": format!("IP {ip} 已封禁 {} 秒", config.bantime), "ip": ip}),
        "success",
    )))
}
