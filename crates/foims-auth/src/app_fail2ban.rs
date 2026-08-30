//! 应用层 fail2ban（登录失败自动封禁）。

use axum::extract::State;
use axum::response::Response;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use validator::Validate;

use crate::provider::AuthProvider;
use foims_common::AppError;
use foims_common::AppJson;
use foims_common::{log_warn, msg};

/// 应用层 Fail2ban 日志文件路径（供 OS fail2ban 监控）
const AUTH_LOG_PATH: &str = "/var/log/foims/auth.log";

/// 标识符清洗上限（字符数），与 login_logs.username 列宽对齐并留有余量
const IDENTIFIER_MAX_CHARS: usize = 64;

/// 标识符清洗：移除全部控制字符（含换行/回车/制表与 DEL）并截断到 64 字符。
///
/// username / ip 会拼入供 OS fail2ban 消费的平面日志与内存记录键：
/// 不清洗时攻击者可通过伪造的标识符注入换行伪造任意来源的失败记录
/// （诱导误封禁）或稀释自身失败计数。
fn sanitize_identifier(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .take(IDENTIFIER_MAX_CHARS)
        .collect()
}

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
///
/// 记录按「IP 维度」与「用户名维度」分别计数（键带 `ip:`/`user:` 前缀）：
/// 仅按 IP 计数时，攻击者伪造 IP 头（或分布式来源）即可绕过封禁，
/// 而针对同一用户名的爆破仍应被拦截（见 security-review A-1）。
pub struct AppFail2ban {
    /// 记录键（"ip:<addr>" / "user:<name>"）→ 失败记录
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

/// 启动后台清理任务：先加载持久化配置，再定期清除过期记录。
///
/// 主程序启动时以连接池调用（同时承担配置的启动加载职责）。
pub fn start_cleanup_task(pool: sqlx::PgPool) {
    tokio::spawn(async move {
        // 启动加载持久化配置；读库失败沿用内存默认值并告警
        load_config_from_db(&pool).await;
        // 每 5 分钟清理一次过期记录，并同步回收邮件发送频控的过期键
        //（防公开邮件端点的伪造键把内存撑到无上限，见 login.rs）
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            cleanup_expired_records();
            crate::login::cleanup_email_send_records();
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

/// 检查记录键是否被封禁
fn is_key_banned(store: &AppFail2ban, key: &str) -> bool {
    let config = store.config.lock().map(|c| c.clone()).unwrap_or_default();
    if !config.enabled {
        return false;
    }

    if let Ok(records) = store.records.lock()
        && let Some(record) = records.get(key)
        && let Some(until) = record.banned_until
    {
        return Instant::now() < until;
    }
    false
}

/// 检查 IP 是否被封禁
pub fn is_ip_banned(ip: &str) -> bool {
    is_key_banned(app_fail2ban(), &ip_key(ip))
}

/// 检查用户名是否被封禁（按用户名维度的爆破防护）
pub fn is_user_banned(username: &str) -> bool {
    is_key_banned(app_fail2ban(), &user_key(username))
}

/// 获取记录键的封禁剩余时间（秒），未封禁返回 0
fn key_ban_remaining(store: &AppFail2ban, key: &str) -> u64 {
    if let Ok(records) = store.records.lock()
        && let Some(record) = records.get(key)
        && let Some(until) = record.banned_until
    {
        let now = Instant::now();
        if now < until {
            return until.duration_since(now).as_secs();
        }
    }
    0
}

/// 获取 IP 封禁剩余时间（秒），未封禁返回 0
pub fn get_ban_remaining(ip: &str) -> u64 {
    key_ban_remaining(app_fail2ban(), &ip_key(ip))
}

/// 当前窗口内的失败次数（IP 与用户名维度取较大值），
/// 供登录验证码等按失败次数递进的机制判定触发条件
pub fn failure_count(ip: &str, username: &str) -> usize {
    let store = app_fail2ban();
    let config = store.config.lock().map(|c| c.clone()).unwrap_or_default();
    if !config.enabled {
        return 0;
    }
    let now = Instant::now();
    let findtime_dur = Duration::from_secs(config.findtime);

    let count_of = |key: &str| -> usize {
        store
            .records
            .lock()
            .map(|records| {
                records
                    .get(key)
                    .map(|record| {
                        record
                            .failures
                            .iter()
                            .filter(|&&t| now.duration_since(t) < findtime_dur)
                            .count()
                    })
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    };

    count_of(&ip_key(ip)).max(count_of(&user_key(username)))
}

/// 获取用户名封禁剩余时间（秒），未封禁返回 0
pub fn get_user_ban_remaining(username: &str) -> u64 {
    key_ban_remaining(app_fail2ban(), &user_key(username))
}

fn ip_key(ip: &str) -> String {
    // 统一在键构造入口清洗：检查键与记录键使用同一清洗结果，保证口径一致
    format!("ip:{}", sanitize_identifier(ip))
}

fn user_key(username: &str) -> String {
    format!("user:{}", sanitize_identifier(username))
}

/// 写入认证日志（供 OS fail2ban 监控）
///
/// 日志格式：`2026-07-17T12:00:00Z [FAIL] 192.168.1.100 - login failed for admin`
/// OS fail2ban 可配置 filter 正则：`^\[FAIL\] <HOST> - login failed`
fn write_auth_log(success: bool, ip: &str, username: &str, reason: Option<&str>) {
    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let status = if success { "[OK]" } else { "[FAIL]" };
    let reason_str = reason.map(|r| format!(" - {r}")).unwrap_or_default();
    // ip 与 username 均来自外部输入：清洗控制字符防止伪造日志行
    let ip = sanitize_identifier(ip);
    let username = sanitize_identifier(username);
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
                    log_warn!("log.fail2ban.auth_log_write_failed", error = e);
                }
            }
            Err(e) => {
                log_warn!(
                    "log.fail2ban.auth_log_open_failed",
                    path = AUTH_LOG_PATH,
                    error = e
                );
            }
        }
    });
}

/// 记录一次登录失败（IP 与用户名两个维度分别计数），返回是否因此次失败而被封禁
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

    let mut banned = false;
    if let Ok(mut records) = store.records.lock() {
        for key in [ip_key(ip), user_key(username)] {
            let record = records.entry(key.clone()).or_insert_with(FailRecord::new);
            // 清除过期的失败记录
            record
                .failures
                .retain(|&t| now.duration_since(t) < findtime_dur);
            // 记录本次失败
            record.failures.push(now);

            // 检查是否达到封禁阈值
            if record.failures.len() as u64 >= config.max_retry {
                record.banned_until = Some(now + Duration::from_secs(config.bantime));
                log_warn!(
                    "log.fail2ban.ip_banned",
                    key = record_key_label(&key),
                    count = record.failures.len(),
                    seconds = config.bantime
                );
                banned = true;
            }
        }
    }
    banned
}

/// 记录键的用户可读标签（日志透出维度类型）
fn record_key_label(key: &str) -> String {
    key.to_string()
}

/// 记录登录成功，清除该 IP 与该用户名的失败记录
pub fn record_login_success(ip: &str, username: &str) {
    // 写入日志文件（供 OS fail2ban 监控）
    write_auth_log(true, ip, username, None);

    let store = app_fail2ban();
    if let Ok(mut records) = store.records.lock() {
        records.remove(&ip_key(ip));
        records.remove(&user_key(username));
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
    /// 用户名维度封禁与跟踪（爆破同一账户的攻击不受换 IP 影响）
    pub banned_usernames: Vec<BannedNameInfo>,
    pub tracked_usernames: Vec<TrackedNameInfo>,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct BannedNameInfo {
    pub username: String,
    pub remaining_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrackedNameInfo {
    pub username: String,
    pub failure_count: u64,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateFail2banConfigRequest {
    pub enabled: Option<bool>,
    #[validate(range(
        min = 60,
        max = 86400,
        message = "server.fail2ban.validation.findtime_range"
    ))]
    pub findtime: Option<u64>,
    #[validate(range(
        min = 1,
        max = 100,
        message = "server.fail2ban.validation.max_retry_range"
    ))]
    pub max_retry: Option<u64>,
    #[validate(range(
        min = 60,
        max = 604800,
        message = "server.fail2ban.validation.bantime_range"
    ))]
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

// ==================== 配置持久化 ====================

/// 从 system_configs 加载持久化配置（config_type='fail2ban'，KV 形式与
/// LDAP/SSO/SMTP 一致）；未配置时保持内存默认值，读库失败仅告警。
pub async fn load_config_from_db(pool: &sqlx::PgPool) {
    let rows =
        match sqlx::query("SELECT key, value FROM system_configs WHERE config_type = 'fail2ban'")
            .fetch_all(pool)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                log_warn!("log.fail2ban.config_load_failed", error = e);
                return;
            }
        };

    let store = app_fail2ban();
    if let Ok(mut config) = store.config.lock() {
        for row in rows {
            let key: String = row.get("key");
            let value: Option<String> = row.get("value");
            let Some(value) = value else { continue };
            // 与更新接口的校验范围保持一致，防止越界值经库直入内存
            match key.as_str() {
                "enabled" => config.enabled = value == "true",
                "findtime" => {
                    if let Ok(v) = value.parse::<u64>() {
                        config.findtime = v.clamp(60, 86400);
                    }
                }
                "max_retry" => {
                    if let Ok(v) = value.parse::<u64>() {
                        config.max_retry = v.clamp(1, 100);
                    }
                }
                "bantime" => {
                    if let Ok(v) = value.parse::<u64>() {
                        config.bantime = v.clamp(60, 604800);
                    }
                }
                _ => {}
            }
        }
    }
}

/// 将配置持久化到 system_configs（config_type='fail2ban'）。
pub async fn save_config_to_db(
    pool: &sqlx::PgPool,
    config: &Fail2banConfig,
) -> Result<(), AppError> {
    let entries = [
        ("enabled", config.enabled.to_string()),
        ("findtime", config.findtime.to_string()),
        ("max_retry", config.max_retry.to_string()),
        ("bantime", config.bantime.to_string()),
    ];

    let mut tx = pool.begin().await?;
    for (key, value) in entries {
        sqlx::query(
            "INSERT INTO system_configs (config_type, key, value)
             VALUES ('fail2ban', $1, $2)
             ON CONFLICT (config_type, key)
             DO UPDATE SET value = $2, updated_at = NOW()",
        )
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

// ==================== API Handler ====================

/// 获取应用层 fail2ban 状态（仅管理员）
pub async fn get_app_fail2ban_status<P: AuthProvider>(
    _admin: crate::extractor::AdminUser,
) -> Result<Response, AppError> {
    let store = app_fail2ban();
    let config = store.config.lock().map(|c| c.clone()).unwrap_or_default();

    let mut banned_ips: Vec<BannedIpInfo> = vec![];
    let mut tracked_ips: Vec<TrackedIpInfo> = vec![];
    let mut banned_usernames: Vec<BannedNameInfo> = vec![];
    let mut tracked_usernames: Vec<TrackedNameInfo> = vec![];
    let now = Instant::now();

    if let Ok(records) = store.records.lock() {
        for (key, record) in records.iter() {
            let banned_until = record.banned_until.filter(|until| now < *until);
            let Some(key_str) = key.strip_prefix("ip:") else {
                // 用户名维度记录
                let Some(name) = key.strip_prefix("user:") else {
                    continue;
                };
                if let Some(until) = banned_until {
                    banned_usernames.push(BannedNameInfo {
                        username: name.to_string(),
                        remaining_seconds: until.duration_since(now).as_secs(),
                    });
                } else if !record.failures.is_empty() {
                    tracked_usernames.push(TrackedNameInfo {
                        username: name.to_string(),
                        failure_count: record.failures.len() as u64,
                    });
                }
                continue;
            };
            if let Some(until) = banned_until {
                banned_ips.push(BannedIpInfo {
                    ip: key_str.to_string(),
                    remaining_seconds: until.duration_since(now).as_secs(),
                });
            }
            if !record.failures.is_empty() && banned_until.is_none() {
                tracked_ips.push(TrackedIpInfo {
                    ip: key_str.to_string(),
                    failure_count: record.failures.len() as u64,
                });
            }
        }
    }

    banned_ips.sort_by_key(|a| a.remaining_seconds);
    tracked_ips.sort_by_key(|b| std::cmp::Reverse(b.failure_count));
    banned_usernames.sort_by_key(|a| a.remaining_seconds);
    tracked_usernames.sort_by_key(|b| std::cmp::Reverse(b.failure_count));

    Ok(foims_common::ok_json(
        AppFail2banStatus {
            enabled: config.enabled,
            findtime: config.findtime,
            max_retry: config.max_retry,
            bantime: config.bantime,
            log_path: AUTH_LOG_PATH.to_string(),
            banned_ips,
            tracked_ips,
            banned_usernames,
            tracked_usernames,
        },
        "server.common.success",
    ))
}

/// 更新应用层 fail2ban 配置（仅管理员；变更持久化到 system_configs）
pub async fn update_app_fail2ban_config<P: AuthProvider>(
    state: State<Arc<P>>,
    _admin: crate::extractor::AdminUser,
    AppJson(req): AppJson<UpdateFail2banConfigRequest>,
) -> Result<Response, AppError> {
    req.validate()?;
    let store = app_fail2ban();

    // 先在当前配置副本上叠加变更并持久化，落库成功后才生效到内存，
    // 避免出现「内存已生效、重启即回退」的静默不一致
    let mut new_config = store.config.lock().map(|c| c.clone()).unwrap_or_default();
    if let Some(enabled) = req.enabled {
        new_config.enabled = enabled;
    }
    if let Some(findtime) = req.findtime {
        new_config.findtime = findtime;
    }
    if let Some(max_retry) = req.max_retry {
        new_config.max_retry = max_retry;
    }
    if let Some(bantime) = req.bantime {
        new_config.bantime = bantime;
    }

    if let Err(e) = save_config_to_db(&state.pool()?.get_conn(), &new_config).await {
        log_warn!(
            "log.fail2ban.config_persist_failed",
            error = e.message().log_string()
        );
        return Err(e);
    }

    if let Ok(mut config) = store.config.lock() {
        *config = new_config;
    }

    Ok(foims_common::ok_json(
        serde_json::json!({"message": "server.fail2ban.config_updated"}),
        "server.fail2ban.config_updated",
    ))
}

/// 手动解封 IP（仅管理员）
pub async fn app_unban_ip<P: AuthProvider>(
    _admin: crate::extractor::AdminUser,
    AppJson(req): AppJson<UnbanIpRequest>,
) -> Result<Response, AppError> {
    let ip = req.ip.trim().to_string();
    if ip.parse::<std::net::IpAddr>().is_err() {
        return Err(AppError::Validation(
            msg("server.fail2ban.invalid_ip").with("ip", &ip),
        ));
    }

    let store = app_fail2ban();
    let key = ip_key(&ip);
    if let Ok(mut records) = store.records.lock()
        && let Some(record) = records.get_mut(&key)
    {
        record.banned_until = None;
        record.failures.clear();
    }

    let response = msg("server.fail2ban.ip_unbanned").with("ip", &ip);
    Ok(foims_common::ok_json(
        serde_json::json!({"message": "server.fail2ban.ip_unbanned", "ip": ip}),
        response,
    ))
}

/// 手动封禁 IP（仅管理员）
pub async fn app_ban_ip<P: AuthProvider>(
    _admin: crate::extractor::AdminUser,
    AppJson(req): AppJson<BanIpRequest>,
) -> Result<Response, AppError> {
    let ip = req.ip.trim().to_string();
    if ip.parse::<std::net::IpAddr>().is_err() {
        return Err(AppError::Validation(
            msg("server.fail2ban.invalid_ip").with("ip", &ip),
        ));
    }

    let store = app_fail2ban();
    let config = store.config.lock().map(|c| c.clone()).unwrap_or_default();

    // 停用状态下 is_key_banned 恒为 false：写入 banned_until 不会产生任何
    // 封禁效果，必须显式报错而非假装成功误导管理员
    if !config.enabled {
        return Err(AppError::Validation(msg(
            "server.fail2ban.disabled_ban_rejected",
        )));
    }

    if let Ok(mut records) = store.records.lock() {
        let record = records.entry(ip_key(&ip)).or_insert_with(FailRecord::new);
        record.banned_until = Some(Instant::now() + Duration::from_secs(config.bantime));
    }

    let response = msg("server.fail2ban.ip_banned")
        .with("ip", &ip)
        .with("seconds", config.bantime);
    Ok(foims_common::ok_json(
        serde_json::json!({"message": "server.fail2ban.ip_banned", "ip": ip}),
        response,
    ))
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 标识符清洗：控制字符与 DEL 被移除，超长被截断到 64 字符
    #[test]
    fn 标识符清洗_移除控制字符并截断() {
        // 换行/回车/制表等控制字符全部移除，阻断日志行注入
        assert_eq!(
            sanitize_identifier("alice\n1.2.3.4 - login failed"),
            "alice1.2.3.4 - login failed"
        );
        assert_eq!(sanitize_identifier("a\r\n\tb"), "ab");
        // DEL(0x7F) 与其他 C0 控制字符一并移除
        assert_eq!(sanitize_identifier("a\u{7f}b\u{1}c"), "abc");
        // 普通字符（含中文）原样保留
        assert_eq!(sanitize_identifier("张三-01"), "张三-01");
        // 超长截断到 64 字符
        let long = "x".repeat(100);
        assert_eq!(
            sanitize_identifier(&long).chars().count(),
            IDENTIFIER_MAX_CHARS
        );
        // 空串保持为空
        assert_eq!(sanitize_identifier(""), "");
    }

    /// 验证 IP 与用户名双维度计数与封禁互不干扰
    #[test]
    fn test_fail2ban_records_ip_and_username_dimensions() {
        let store = AppFail2ban::new();
        let config = Fail2banConfig {
            findtime: 600,
            max_retry: 3,
            bantime: 60,
            enabled: true,
        };
        if let Ok(mut c) = store.config.lock() {
            *c = config;
        }

        // 同一用户名在两个不同 IP 上失败：user 维度累计 2 次，两个 IP 各 1 次
        {
            let now = Instant::now();
            let mut records = store.records.lock().unwrap_or_else(|e| e.into_inner());
            for ip in ["1.1.1.1", "2.2.2.2"] {
                let rec = records.entry(ip_key(ip)).or_insert_with(FailRecord::new);
                rec.failures.push(now);
            }
            let user = records
                .entry(user_key("admin"))
                .or_insert_with(FailRecord::new);
            user.failures.push(now);
            user.failures.push(now);
        }

        assert!(!is_key_banned(&store, &ip_key("1.1.1.1")));
        assert!(!is_key_banned(&store, &user_key("admin")));
        assert_eq!(
            store
                .records
                .lock()
                .map(|r| r.get(&user_key("admin")).map(|x| x.failures.len()))
                .unwrap_or(None),
            Some(2)
        );

        // 第 3 次失败（user 维度）触发封禁：两个维度同时被封
        {
            let now = Instant::now();
            let mut records = store.records.lock().unwrap_or_else(|e| e.into_inner());
            let user = records
                .entry(user_key("admin"))
                .or_insert_with(FailRecord::new);
            user.failures.push(now);
            user.banned_until = Some(now + Duration::from_secs(60));
        }
        assert!(is_key_banned(&store, &user_key("admin")));
        assert!(key_ban_remaining(&store, &user_key("admin")) > 0);
        // IP 维度未达阈值，不受影响
        assert!(!is_key_banned(&store, &ip_key("1.1.1.1")));
    }
}
