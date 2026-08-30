//! 日志外发（syslog）：将操作日志与登录日志实时转发到外部采集主机。
//!
//! 配置存于 `system_configs`（config_type='log_forwarding'）：
//! enabled / protocol(udp|tcp) / host / port。转发是尽力而为的旁路
//! 动作：未启用或发送失败仅记录日志，不影响业务与本地落库。
//! 报文格式遵循 RFC 3164（BSD syslog），facility 固定 local0。

use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use foims_common::{log_debug, log_warn, msg};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use crate::app_state::AppState;
use foims_common::AppError;
use foims_common::AppJson;

/// local0(16) × 8 + informational(6)
const SYSLOG_PRI_INFO: u32 = 16 * 8 + 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogForwardingConfig {
    pub enabled: bool,
    /// udp 或 tcp（不区分大小写，保存前统一小写；空/非法值一律校验拒绝）
    pub protocol: String,
    pub host: String,
    pub port: u16,
}

impl Default for LogForwardingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            protocol: "udp".to_string(),
            host: String::new(),
            port: 514,
        }
    }
}

/// 读取外发配置（未配置的键回落默认值：关闭）。
///
/// DB 错误向上传播：吞成默认配置会把 DB 故障伪装成「外发已关闭」，
/// 真实配置被静默掩盖。
pub async fn load(pool: &PgPool) -> Result<LogForwardingConfig, AppError> {
    let rows =
        sqlx::query("SELECT key, value FROM system_configs WHERE config_type = 'log_forwarding'")
            .fetch_all(pool)
            .await?;

    let mut config = LogForwardingConfig::default();
    for row in rows {
        let key: String = row.get("key");
        let value: Option<String> = row.get("value");
        let Some(value) = value else { continue };
        match key.as_str() {
            "enabled" => config.enabled = value == "true",
            "protocol" => config.protocol = value.to_lowercase(),
            "host" => config.host = value.trim().to_string(),
            "port" => {
                if let Ok(port) = value.parse::<u16>() {
                    config.port = port;
                }
            }
            _ => {}
        }
    }
    Ok(config)
}

/// 校验配置可用的必要字段。
///
/// protocol 只允许 udp/tcp：空串与非法值一律拒绝，避免「空串绕过校验
/// 被持久化、发送时按 UDP 处理」的口径漏洞。
fn validate(config: &LogForwardingConfig) -> Result<(), AppError> {
    if config.enabled && (config.host.trim().is_empty() || config.port == 0) {
        return Err(AppError::Validation(msg(
            "server.logs.forwarding_incomplete",
        )));
    }
    if !["udp", "tcp"].contains(&config.protocol.as_str()) {
        return Err(AppError::Validation(msg(
            "server.logs.forwarding_protocol_invalid",
        )));
    }
    Ok(())
}

/// 保存外发配置
pub async fn save(pool: &PgPool, config: &LogForwardingConfig) -> Result<(), AppError> {
    validate(config)?;

    let items = [
        ("enabled", config.enabled.to_string()),
        ("protocol", config.protocol.to_lowercase()),
        ("host", config.host.trim().to_string()),
        ("port", config.port.to_string()),
    ];

    let mut tx = pool.begin().await?;
    for (key, value) in items {
        sqlx::query(
            "INSERT INTO system_configs (config_type, key, value)
             VALUES ('log_forwarding', $1, $2)
             ON CONFLICT (config_type, key) DO UPDATE SET value = $2, updated_at = NOW()",
        )
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// 异步外发一条日志（fire-and-forget，不阻塞业务路径）。
///
/// 配置读取失败（DB 故障）时放弃本次外发并记录日志：
/// 宁可漏发也不以错误的默认配置发送。
pub fn spawn_forward(pool: PgPool, message: String) {
    tokio::spawn(async move {
        let config = match load(&pool).await {
            Ok(config) => config,
            Err(e) => {
                log_warn!("log.forwarding.load_failed", error = e);
                return;
            }
        };
        if !config.enabled {
            return;
        }
        if let Err(e) = send(&config, &message).await {
            log_warn!("log.forwarding.send_failed", error = e);
        }
    });
}

/// 组装 RFC 3164 报文并发送。
///
/// 连接与写入共用同一 3 秒预算：服务端接受连接但不读取时，
/// `write_all` 同样会超时返回，避免请求挂起与连接泄漏。
async fn send(config: &LogForwardingConfig, message: &str) -> Result<(), String> {
    const SEND_TIMEOUT_SECS: u64 = 3;

    let timestamp = chrono::Utc::now().format("%b %e %H:%M:%S");
    // 报文按 1KB 截断，匹配常见 syslog 采集端的报文上限
    let trimmed: String = if message.len() > 1024 {
        let mut end = 1024;
        while end > 0 && !message.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &message[..end])
    } else {
        message.to_string()
    };
    let packet = format!("<{SYSLOG_PRI_INFO}>{timestamp} foims op: {trimmed}");

    // 以 (host, port) 元组形式解析目标地址：裸 IPv6 字面量（如 2001:db8::1）
    // 拼成 "host:port" 字符串会被误解析为非法地址，元组形式可正确处理
    let host = config.host.trim();
    if config.protocol == "tcp" {
        let connect = tokio::net::TcpStream::connect((host, config.port));
        let mut stream =
            tokio::time::timeout(std::time::Duration::from_secs(SEND_TIMEOUT_SECS), connect)
                .await
                .map_err(|_| "连接超时".to_string())?
                .map_err(|e| e.to_string())?;
        use tokio::io::AsyncWriteExt;
        let write = stream.write_all(packet.as_bytes());
        tokio::time::timeout(std::time::Duration::from_secs(SEND_TIMEOUT_SECS), write)
            .await
            .map_err(|_| "写入超时".to_string())?
            .map_err(|e| e.to_string())?;
    } else {
        // 先解析目标得到具体 SocketAddr，再按其地址族绑定对应通配地址：
        // 固定绑定 IPv6 通配会在 bindv6only=1 主机上丢失所有 IPv4 目标
        let mut addrs = tokio::net::lookup_host((host, config.port))
            .await
            .map_err(|e| e.to_string())?;
        let target = addrs
            .next()
            .ok_or_else(|| "目标地址解析结果为空".to_string())?;
        let bind_addr: std::net::SocketAddr = match target {
            std::net::SocketAddr::V4(_) => "0.0.0.0:0"
                .parse::<std::net::SocketAddr>()
                .map_err(|e| e.to_string())?,
            std::net::SocketAddr::V6(_) => "[::]:0"
                .parse::<std::net::SocketAddr>()
                .map_err(|e| e.to_string())?,
        };
        let socket = tokio::net::UdpSocket::bind(bind_addr)
            .await
            .map_err(|e| e.to_string())?;
        let send = socket.send_to(packet.as_bytes(), target);
        tokio::time::timeout(std::time::Duration::from_secs(SEND_TIMEOUT_SECS), send)
            .await
            .map_err(|_| "发送超时".to_string())?
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ==================== HTTP 端点（安全管理员管辖） ====================

pub async fn get_forwarding(
    State(state): State<Arc<AppState>>,
    _secadmin: foims_auth::extractor::SecAdminUser,
) -> Result<Response, AppError> {
    // DB 错误透传，不再静默回退「关闭」默认配置
    let config = load(&state.pool()?.get_conn()).await?;
    Ok(foims_common::ok_json(
        config,
        "server.logs.forwarding_retrieved",
    ))
}

pub async fn update_forwarding(
    State(state): State<Arc<AppState>>,
    _secadmin: foims_auth::extractor::SecAdminUser,
    AppJson(req): AppJson<LogForwardingConfig>,
) -> Result<Response, AppError> {
    save(&state.pool()?.get_conn(), &req).await?;
    Ok(foims_common::ok_json((), "server.logs.forwarding_updated"))
}

/// 发送一条测试报文验证连通性
pub async fn test_forwarding(
    State(state): State<Arc<AppState>>,
    _secadmin: foims_auth::extractor::SecAdminUser,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();
    let config = load(&conn).await?;
    if !config.enabled {
        return Err(AppError::Validation(msg("server.logs.forwarding_disabled")));
    }
    send(&config, "FOIMS log forwarding test message")
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.logs.forwarding_test_failed").with("error", e))
        })?;
    log_debug!("log.forwarding.test_sent");
    Ok(foims_common::ok_json(
        (),
        "server.logs.forwarding_test_sent",
    ))
}
