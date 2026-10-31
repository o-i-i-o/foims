//! Webhook 通知推送模块（通用 / 钉钉 / 企业微信）。
//!
//! 配置存于 `system_configs`（config_type='webhook'，key-value），钉钉加签
//! 密钥以 AES-GCM 加密存储（与 SMTP 密码同口径）。发送受「通知配置」矩阵
//! 的 webhook_* 开关门控，向全部已配置渠道尽力推送：单渠道失败仅记日志，
//! 不影响其他渠道与业务结果（与邮件渠道降级口径一致）。
//!
//! 协议口径（对齐各平台群机器人 Webhook）：
//! - 钉钉：POST markdown 消息到机器人地址，配置加签密钥时追加
//!   `timestamp` 与 HMAC-SHA256 签名（`"{timestamp}\n{secret}"` 后 Base64）
//! - 企业微信：POST text 消息到 `qyapi.weixin.qq.com/cgi-bin/webhook/send?key=`
//! - 通用：POST JSON `{"title", "message"}` 到自定义 URL

use std::sync::OnceLock;
use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, KeyInit, Mac};
use reqwest::Client;
use serde::Deserialize;
use sha2::Sha256;
use sqlx::{PgPool, Row};

use foims_common::AppError;
use foims_common::crypto::{decrypt_password_async, encrypt_password_async};
use foims_common::{log_error, log_info, msg};

const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(10);

/// Webhook 配置在 system_configs 中的 config_type 与各渠道 key
const WEBHOOK_CONFIG_TYPE: &str = "webhook";
const GENERIC_URL_KEY: &str = "generic_url";
const DINGTALK_URL_KEY: &str = "dingtalk_url";
const DINGTALK_SECRET_KEY: &str = "dingtalk_secret";
const WECOM_KEY_CONFIG_KEY: &str = "wecom_key";

/// 三渠道 Webhook 配置（URL/key 为空串表示该渠道未配置）。
#[derive(Debug, Clone, Default)]
pub struct WebhookConfig {
    /// 通用 Webhook 接收端完整 URL
    pub generic_url: String,
    /// 钉钉机器人 Webhook 地址（含 access_token）
    pub dingtalk_url: String,
    /// 钉钉加签密钥（落库前 AES-GCM 加密，读取时解密）
    pub dingtalk_secret: String,
    /// 企业微信群机器人 key（Webhook 地址的 key 参数）
    pub wecom_key: String,
}

/// Webhook 通知的业务类型（对应通知矩阵的 webhook_* 开关列）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookNotifyKind {
    IpChange,
    AgentAlert,
    SnmpEvent,
}

impl WebhookNotifyKind {
    /// 该业务类型的 Webhook 方式开关（与 smtp::NotificationMethods 的
    /// webhook_* 字段一一对应）
    fn enabled(self, methods: crate::smtp::NotificationMethods) -> bool {
        match self {
            Self::IpChange => methods.webhook_ip_change,
            Self::AgentAlert => methods.webhook_agent_alert,
            Self::SnmpEvent => methods.webhook_snmp_event,
        }
    }
}

/// 可单独测试的渠道标识（配置端点 `channel` 参数）。
#[derive(Debug, Clone, Copy, Deserialize, ts_rs::TS)]
#[ts(export)]
pub enum WebhookChannel {
    #[serde(rename = "generic")]
    Generic,
    #[serde(rename = "dingtalk")]
    DingTalk,
    #[serde(rename = "wecom")]
    Wecom,
}

impl WebhookChannel {
    fn config_key(self) -> &'static str {
        match self {
            Self::Generic => GENERIC_URL_KEY,
            Self::DingTalk => DINGTALK_URL_KEY,
            Self::Wecom => WECOM_KEY_CONFIG_KEY,
        }
    }
}

/// 读取三渠道 Webhook 配置；查询失败或密钥解密失败时按全未配置降级并留痕
/// （通知缺失的代价远小于阻塞业务，与 SMTP 读取降级口径一致）。
pub async fn get_webhook_config_from_db(pool: &PgPool) -> WebhookConfig {
    match get_webhook_config_inner(pool).await {
        Ok(config) => config,
        Err(e) => {
            log_error!("log.webhook.config_query_failed", error = e);
            WebhookConfig::default()
        }
    }
}

async fn get_webhook_config_inner(pool: &PgPool) -> Result<WebhookConfig, String> {
    let rows = sqlx::query(
        "SELECT key, value FROM system_configs WHERE config_type = $1 AND value IS NOT NULL",
    )
    .bind(WEBHOOK_CONFIG_TYPE)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("query failed: {e}"))?;

    let mut config = WebhookConfig::default();
    for row in rows {
        let key: String = row.get("key");
        let value: String = row.get("value");
        match key.as_str() {
            GENERIC_URL_KEY => config.generic_url = value,
            DINGTALK_URL_KEY => config.dingtalk_url = value,
            DINGTALK_SECRET_KEY => {
                config.dingtalk_secret = decrypt_password_async(value)
                    .await
                    .map_err(|e| format!("secret decrypt failed: {e}"))?;
            }
            WECOM_KEY_CONFIG_KEY => config.wecom_key = value,
            _ => {}
        }
    }
    Ok(config)
}

/// 保存三渠道 Webhook 配置（加签密钥加密后逐 key 写入，与 SMTP 同口径）。
pub async fn save_webhook_config_to_db(
    pool: &PgPool,
    config: &WebhookConfig,
) -> Result<(), AppError> {
    let encrypted_secret = if config.dingtalk_secret.is_empty() {
        String::new()
    } else {
        encrypt_password_async(config.dingtalk_secret.clone())
            .await
            .map_err(|e| {
                AppError::Internal(msg("server.webhook.secret_encrypt_failed").with("error", e))
            })?
    };

    let entries = [
        (GENERIC_URL_KEY, config.generic_url.trim().to_string()),
        (DINGTALK_URL_KEY, config.dingtalk_url.trim().to_string()),
        (DINGTALK_SECRET_KEY, encrypted_secret),
        (WECOM_KEY_CONFIG_KEY, config.wecom_key.trim().to_string()),
    ];

    let mut tx = pool.begin().await?;
    for (key, value) in entries {
        sqlx::query(
            "INSERT INTO system_configs (config_type, key, value)
             VALUES ($1, $2, $3)
             ON CONFLICT (config_type, key)
             DO UPDATE SET value = $3, updated_at = NOW()",
        )
        .bind(WEBHOOK_CONFIG_TYPE)
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// 进程内复用的 HTTP 客户端（统一超时；构建失败回退默认客户端）。
fn http_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(WEBHOOK_TIMEOUT)
            .build()
            .unwrap_or_else(|_| Client::new())
    })
}

/// 统一 POST JSON 并判定结果：非 2xx 视为失败；2xx 且响应体可解析出
/// `errcode != 0` 视为钉钉/企业微信业务拒绝，其余接收方响应体无标准格式，
/// 以 HTTP 状态为准。
async fn post_json(url: &str, payload: &serde_json::Value) -> Result<(), AppError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(AppError::Validation(msg("server.webhook.url_required")));
    }
    let resp = http_client()
        .post(url)
        .json(payload)
        .send()
        .await
        .map_err(|e| AppError::Internal(msg("server.webhook.request_failed").with("error", e)))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Internal(msg("server.webhook.request_failed").with("error", e)))?;

    if !status.is_success() {
        return Err(AppError::Internal(
            msg("server.webhook.http_error").with("status", status.as_u16()),
        ));
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
        && value
            .get("errcode")
            .and_then(serde_json::Value::as_i64)
            .is_some_and(|code| code != 0)
    {
        let errmsg = value
            .get("errmsg")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        return Err(AppError::Internal(
            msg("server.webhook.remote_rejected").with("errmsg", errmsg),
        ));
    }
    Ok(())
}

/// 发送通用 Webhook：POST JSON `{"title", "message"}`。
async fn send_generic(url: &str, title: &str, body: &str) -> Result<(), AppError> {
    let payload = serde_json::json!({ "title": title, "message": body });
    post_json(url, &payload).await
}

/// 发送钉钉机器人消息（markdown）；配置了加签密钥时按官方口径追加
/// `timestamp` 与 URL 编码后的 HMAC-SHA256 签名。
async fn send_dingtalk(url: &str, secret: &str, title: &str, body: &str) -> Result<(), AppError> {
    let mut target = url.to_string();
    if !secret.trim().is_empty() {
        let timestamp = chrono::Utc::now().timestamp_millis();
        let sign = dingtalk_sign(timestamp, secret);
        let separator = if url.contains('?') { "&" } else { "?" };
        target.push_str(&format!("{separator}timestamp={timestamp}&sign={sign}"));
    }
    let payload = serde_json::json!({
        "msgtype": "markdown",
        "markdown": {
            "title": title,
            "text": format!("### {title}\n{body}"),
        },
    });
    post_json(&target, &payload).await
}

/// 发送企业微信群机器人消息（text，标题与正文以换行拼接）。
async fn send_wecom(key: &str, title: &str, body: &str) -> Result<(), AppError> {
    let url = format!(
        "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key={}",
        key.trim()
    );
    let payload = serde_json::json!({
        "msgtype": "text",
        "text": { "content": format!("{title}\n{body}") },
    });
    post_json(&url, &payload).await
}

/// 钉钉加签：HMAC-SHA256（密钥为加签密钥，内容为 `"{timestamp}\n{secret}"`）
/// 后 Base64，再做 URL 编码（Base64 字符集中的 `+ / =` 需转义）。
fn dingtalk_sign(timestamp_millis: i64, secret: &str) -> String {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        // HMAC 接受任意长度密钥，new_from_slice 实际不会失败；类型要求
        // 处理错误分支，返回空签名（请求会因签名校验失败被钉钉拒绝而非静默丢失）
        return String::new();
    };
    mac.update(format!("{timestamp_millis}\n{secret}").as_bytes());
    let encoded = STANDARD.encode(mac.finalize().into_bytes());
    encoded
        .replace('+', "%2B")
        .replace('/', "%2F")
        .replace('=', "%3D")
}

/// 发送单渠道并留痕（成功/失败均记日志，不向调用方传播错误）。
async fn dispatch(channel: &str, send: impl std::future::Future<Output = Result<(), AppError>>) {
    match send.await {
        Ok(()) => log_info!("log.webhook.sent", channel = channel),
        Err(e) => log_error!("log.webhook.send_failed", channel = channel, error = e),
    }
}

/// 向全部已配置渠道推送业务通知（受矩阵 webhook_* 开关门控）。
///
/// 开关关闭或渠道未配置时静默跳过；单渠道失败仅记日志不影响其他渠道。
pub async fn send_notification_webhook(
    pool: &PgPool,
    kind: WebhookNotifyKind,
    title: &str,
    body: &str,
) {
    if !kind.enabled(crate::smtp::load_notification_methods(pool).await) {
        return;
    }
    let config = get_webhook_config_from_db(pool).await;

    if !config.generic_url.trim().is_empty() {
        let url = config.generic_url.clone();
        let title = title.to_string();
        let body = body.to_string();
        dispatch("generic", send_generic(&url, &title, &body)).await;
    }
    if !config.dingtalk_url.trim().is_empty() {
        let url = config.dingtalk_url.clone();
        let secret = config.dingtalk_secret.clone();
        let title = title.to_string();
        let body = body.to_string();
        dispatch("dingtalk", send_dingtalk(&url, &secret, &title, &body)).await;
    }
    if !config.wecom_key.trim().is_empty() {
        let key = config.wecom_key.clone();
        let title = title.to_string();
        let body = body.to_string();
        dispatch("wecom", send_wecom(&key, &title, &body)).await;
    }
}

/// 向指定渠道发送测试消息（复用已保存配置；渠道未配置返回校验错误）。
pub async fn test_webhook_channel(pool: &PgPool, channel: WebhookChannel) -> Result<(), AppError> {
    let config = get_webhook_config_from_db(pool).await;
    let title = "FOIMS Webhook 测试";
    let body = "这是一条 Webhook 连接测试消息，请忽略，无需回复。";

    let configured = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM system_configs
         WHERE config_type = $1 AND key = $2 AND value IS NOT NULL AND value <> ''",
    )
    .bind(WEBHOOK_CONFIG_TYPE)
    .bind(channel.config_key())
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    if configured == 0 {
        return Err(AppError::Validation(msg(
            "server.webhook.channel_not_configured",
        )));
    }

    match channel {
        WebhookChannel::Generic => send_generic(&config.generic_url, title, body).await,
        WebhookChannel::DingTalk => {
            send_dingtalk(&config.dingtalk_url, &config.dingtalk_secret, title, body).await
        }
        WebhookChannel::Wecom => send_wecom(&config.wecom_key, title, body).await,
    }
}

// ---- 业务通知封装（与 smtp 侧发送函数一一对应，文案复用其 *_message 函数，
// 内部完成 webhook_* 开关门控，调用点无需再读矩阵） ----

/// 发送 IP 地址变更 Webhook 通知（受 webhook_ip_change 开关门控）。
pub async fn send_ip_change_webhook(
    pool: &PgPool,
    source_device: &str,
    workstation_name: &str,
    mac_address: &str,
    registered_ip: &str,
    observed_ip: &str,
) {
    let (title, body) = crate::smtp::ip_change_message(
        source_device,
        workstation_name,
        mac_address,
        registered_ip,
        observed_ip,
    );
    send_notification_webhook(pool, WebhookNotifyKind::IpChange, &title, &body).await;
}

/// 发送工位 IP 上设备更换 Webhook 通知（受 webhook_ip_change 开关门控）。
pub async fn send_device_swap_webhook(
    pool: &PgPool,
    source_device: &str,
    workstation_name: &str,
    ip_address: &str,
    old_mac: &str,
    new_mac: &str,
) {
    let (title, body) = crate::smtp::device_swap_message(
        source_device,
        workstation_name,
        ip_address,
        old_mac,
        new_mac,
    );
    send_notification_webhook(pool, WebhookNotifyKind::IpChange, &title, &body).await;
}

/// 发送主机资源告警 Webhook 通知（受 webhook_agent_alert 开关门控）。
pub async fn send_host_alert_webhook(
    pool: &PgPool,
    hostname: &str,
    ip_address: &str,
    metric: &str,
    value: f64,
    threshold: u8,
) {
    let (title, body) =
        crate::smtp::host_alert_message(hostname, ip_address, metric, value, threshold);
    send_notification_webhook(pool, WebhookNotifyKind::AgentAlert, &title, &body).await;
}

/// 发送未知设备接入 Webhook 通知（受 webhook_snmp_event 开关门控）。
pub async fn send_unknown_device_webhook(
    pool: &PgPool,
    device_name: &str,
    interface: &str,
    vlan_id: Option<i32>,
    ip_address: &str,
    mac_address: &str,
) {
    let (title, body) = crate::smtp::unknown_device_message(
        device_name,
        interface,
        vlan_id,
        ip_address,
        mac_address,
    );
    send_notification_webhook(pool, WebhookNotifyKind::SnmpEvent, &title, &body).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 钉钉签名与标准实现一致() {
        // 期望值由 Python hmac/hashlib 标准库按同一输入计算（官方加签口径）
        assert_eq!(
            dingtalk_sign(1_700_000_000_000, "secret123"),
            "dmKht5wnRNI8D3ORFxZjr58I3IDg5FOcP5dFPicdUYg%3D"
        );
    }

    #[test]
    fn 钉钉签名输出已做_url_编码() {
        // 任一输入下输出不得含裸露的 Base64 特殊字符
        let sign = dingtalk_sign(1_700_000_000_001, "another/secret+key=");
        assert!(!sign.contains('+'));
        assert!(!sign.contains('/'));
        assert!(!sign.contains('=') || sign.ends_with("%3D"));
    }

    #[test]
    fn 通知业务类型映射矩阵开关() {
        let methods = crate::smtp::NotificationMethods {
            ip_change: true,
            agent_alert: true,
            snmp_event: true,
            webhook_ip_change: false,
            webhook_agent_alert: true,
            webhook_snmp_event: false,
        };
        assert!(!WebhookNotifyKind::IpChange.enabled(methods));
        assert!(WebhookNotifyKind::AgentAlert.enabled(methods));
        assert!(!WebhookNotifyKind::SnmpEvent.enabled(methods));
    }

    #[test]
    fn 渠道标识映射存储键() {
        assert_eq!(WebhookChannel::Generic.config_key(), "generic_url");
        assert_eq!(WebhookChannel::DingTalk.config_key(), "dingtalk_url");
        assert_eq!(WebhookChannel::Wecom.config_key(), "wecom_key");
    }
}
