//! 日志外发（syslog）：将操作日志与登录日志实时转发到外部采集主机。
//!
//! 配置存于 `system_configs`（config_type='log_forwarding'）：
//! enabled / protocol(udp|tcp) / host / port。转发是尽力而为的旁路
//! 动作：未启用或发送失败仅记录日志，不影响业务与本地落库。
//! 报文格式遵循 RFC 3164（BSD syslog），facility 固定 local0。

use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use ipma_common::{log_debug, log_warn, msg};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use crate::app_state::AppState;
use crate::routes::static_files::AppJson;
use ipma_common::AppError;

/// local0(16) × 8 + informational(6)
const SYSLOG_PRI_INFO: u32 = 16 * 8 + 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogForwardingConfig {
    pub enabled: bool,
    /// udp 或 tcp（不区分大小写，非法值按 udp 处理）
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

/// 读取外发配置（未配置时返回默认值：关闭）
pub async fn load(pool: &PgPool) -> LogForwardingConfig {
    let mut config = LogForwardingConfig::default();
    let Ok(rows) =
        sqlx::query("SELECT key, value FROM system_configs WHERE config_type = 'log_forwarding'")
            .fetch_all(pool)
            .await
    else {
        return config;
    };

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
    config
}

/// 校验配置可用的必要字段
fn validate(config: &LogForwardingConfig) -> Result<(), AppError> {
    if config.enabled && (config.host.trim().is_empty() || config.port == 0) {
        return Err(AppError::Validation(msg(
            "server.logs.forwarding_incomplete",
        )));
    }
    if !config.protocol.is_empty() && !["udp", "tcp"].contains(&config.protocol.as_str()) {
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

/// 异步外发一条日志（fire-and-forget，不阻塞业务路径）
pub fn spawn_forward(pool: PgPool, message: String) {
    tokio::spawn(async move {
        let config = load(&pool).await;
        if !config.enabled {
            return;
        }
        if let Err(e) = send(&config, &message).await {
            log_warn!("log.forwarding.send_failed", error = e);
        }
    });
}

/// 组装 RFC 3164 报文并发送
async fn send(config: &LogForwardingConfig, message: &str) -> Result<(), String> {
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
    let packet = format!("<{SYSLOG_PRI_INFO}>{timestamp} ipma op: {trimmed}");

    let addr = format!("{}:{}", config.host.trim(), config.port);
    if config.protocol == "tcp" {
        let mut stream = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            tokio::net::TcpStream::connect(&addr),
        )
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
        use tokio::io::AsyncWriteExt;
        stream
            .write_all(packet.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    } else {
        let socket = tokio::net::UdpSocket::bind(("::", 0))
            .await
            .map_err(|e| e.to_string())?;
        socket
            .send_to(packet.as_bytes(), &addr)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ==================== HTTP 端点（安全管理员管辖） ====================

pub async fn get_forwarding(
    State(state): State<Arc<AppState>>,
    _secadmin: crate::auth::extractor::SecAdminUser,
) -> Result<Response, AppError> {
    let config = load(&state.pool()?.get_conn()).await;
    Ok(ipma_common::ok_json(
        config,
        "server.logs.forwarding_retrieved",
    ))
}

pub async fn update_forwarding(
    State(state): State<Arc<AppState>>,
    _secadmin: crate::auth::extractor::SecAdminUser,
    AppJson(req): AppJson<LogForwardingConfig>,
) -> Result<Response, AppError> {
    save(&state.pool()?.get_conn(), &req).await?;
    Ok(ipma_common::ok_json((), "server.logs.forwarding_updated"))
}

/// 发送一条测试报文验证连通性
pub async fn test_forwarding(
    State(state): State<Arc<AppState>>,
    _secadmin: crate::auth::extractor::SecAdminUser,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();
    let config = load(&conn).await;
    if !config.enabled {
        return Err(AppError::Validation(msg("server.logs.forwarding_disabled")));
    }
    send(&config, "IPMA log forwarding test message")
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.logs.forwarding_test_failed").with("error", e))
        })?;
    log_debug!("log.forwarding.test_sent");
    Ok(ipma_common::ok_json((), "server.logs.forwarding_test_sent"))
}
