//! Webhook 通知推送模块（通用 / 钉钉 / 企业微信，多目标）。
//!
//! 目标列表存于 `system_configs`（config_type='webhook'，key='targets'，
//! JSON 对象数组）：每条目标独立指定渠道（generic/dingtalk/wecom）、接收
//! 地址、备注名与订阅范围（`NotifyScope`），可同时配置多条、互不影响，
//! 与邮件渠道（`crate::smtp`）彼此独立。钉钉加签密钥在 JSON 内以 AES-GCM
//! 密文存储（与 SMTP 密码同口径）。
//!
//! 发送按各目标自身的订阅范围过滤，逐目标尽力推送：单目标失败仅记日志，
//! 不影响其他目标与业务结果（与邮件渠道降级口径一致）。
//!
//! 协议口径（对齐各平台群机器人 Webhook）：
//! - 钉钉：POST markdown 消息到机器人地址，配置加签密钥时追加
//!   `timestamp` 与 HMAC-SHA256 签名（`"{timestamp}\n{secret}"` 后 Base64）
//! - 企业微信：POST text 消息到群机器人完整 Webhook 地址
//! - 通用：POST JSON `{"title", "message"}` 到自定义 URL

use std::sync::OnceLock;
use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, KeyInit, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::PgPool;

use foims_common::AppError;
use foims_common::crypto::{decrypt_password_async, encrypt_password_async};
use foims_common::{log_error, log_info, msg};

use crate::smtp::{NotifyKind, NotifyScope};

const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(10);

/// Webhook 目标列表在 system_configs 中的存储位置
const WEBHOOK_CONFIG_TYPE: &str = "webhook";
const WEBHOOK_TARGETS_KEY: &str = "targets";

/// 单条 Webhook 推送目标。
///
/// `secret` 按阶段承载不同形态：`load_webhook_targets_raw` /
/// `save_webhook_targets` 中为 AES-GCM 密文，`load_webhook_targets`
/// （发送/测试路径）中为明文。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookTarget {
    /// 目标 ID（UUID，前端生成；测试端点据此定位已保存目标）
    pub id: String,
    /// 备注名（便于识别多条目标；空时日志退回渠道标识）
    pub name: String,
    pub channel: WebhookChannel,
    /// 接收端完整 URL（通用/钉钉为机器人地址，企业微信为群机器人完整地址）
    pub url: String,
    /// 钉钉加签密钥（仅 dingtalk 渠道有效；非钉钉渠道为空）
    #[serde(default)]
    pub secret: String,
    /// 订阅范围（flatten 平铺存储，与邮件收件人同构）
    #[serde(flatten)]
    pub scope: NotifyScope,
}

/// Webhook 目标渠道。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
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
    /// 日志与错误信息中的渠道标识
    pub fn label(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::DingTalk => "dingtalk",
            Self::Wecom => "wecom",
        }
    }
}

/// 保存目标载荷：`secret` 为 `None`（或空串）表示按 id 保留已存密钥，
/// 为 `Some(明文)` 表示加密覆盖（口径同 SMTP 密码留空保持）。
pub struct WebhookTargetUpsert {
    pub id: String,
    pub name: String,
    pub channel: WebhookChannel,
    pub url: String,
    pub secret: Option<String>,
    pub scope: NotifyScope,
}

/// 读取目标列表（`secret` 保持密文形态，供保存合并与配置回显使用）。
///
/// 供配置端点使用：解析失败向上报错（配置展示不能降级为空列表，
/// 否则管理员保存时会把全部目标覆盖丢失）。
pub async fn load_webhook_targets_raw(pool: &PgPool) -> Result<Vec<WebhookTarget>, String> {
    let value = sqlx::query_scalar::<_, String>(
        "SELECT value FROM system_configs WHERE config_type = $1 AND key = $2",
    )
    .bind(WEBHOOK_CONFIG_TYPE)
    .bind(WEBHOOK_TARGETS_KEY)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("query failed: {e}"))?;
    match value {
        Some(value) => serde_json::from_str(&value).map_err(|e| format!("parse failed: {e}")),
        None => Ok(Vec::new()),
    }
}

/// 读取目标列表（加签密钥已解密为明文）；查询失败或密钥解密失败时按
/// 无目标降级并留痕（通知缺失的代价远小于阻塞业务，与 SMTP 读取降级口径一致）。
pub async fn load_webhook_targets(pool: &PgPool) -> Vec<WebhookTarget> {
    let raw = match load_webhook_targets_raw(pool).await {
        Ok(raw) => raw,
        Err(e) => {
            log_error!("log.webhook.config_query_failed", error = e);
            return Vec::new();
        }
    };
    let mut targets = Vec::with_capacity(raw.len());
    for mut target in raw {
        if !target.secret.is_empty() {
            match decrypt_password_async(std::mem::take(&mut target.secret)).await {
                Ok(plain) => target.secret = plain,
                Err(e) => {
                    log_error!("log.webhook.config_query_failed", error = e);
                    return Vec::new();
                }
            }
        }
        targets.push(target);
    }
    targets
}

/// 保存目标列表（全量替换）：合并「保留已存密钥」语义并加密后，整体
/// 写入 targets 单行。
pub async fn save_webhook_targets(
    pool: &PgPool,
    upserts: &[WebhookTargetUpsert],
) -> Result<(), AppError> {
    let existing = load_webhook_targets_raw(pool).await.map_err(|e| {
        AppError::Internal(msg("server.webhook.config_query_failed").with("error", e))
    })?;

    let mut cipher_targets = Vec::with_capacity(upserts.len());
    for upsert in upserts {
        let secret = match upsert.secret.as_deref().map(str::trim) {
            Some(plain) if !plain.is_empty() => encrypt_password_async(plain.to_string())
                .await
                .map_err(|e| {
                    AppError::Internal(msg("server.webhook.secret_encrypt_failed").with("error", e))
                })?,
            // 留空 = 保留该 id 已存密钥；目标不存在则置空（新目标无密钥）
            _ => existing
                .iter()
                .find(|t| t.id == upsert.id)
                .map(|t| t.secret.clone())
                .unwrap_or_default(),
        };
        cipher_targets.push(WebhookTarget {
            id: upsert.id.clone(),
            name: upsert.name.trim().to_string(),
            channel: upsert.channel,
            url: upsert.url.trim().to_string(),
            secret,
            scope: upsert.scope,
        });
    }

    let value = serde_json::to_string(&cipher_targets)
        .map_err(|e| AppError::Internal(msg("server.system.serialize_failed").with("error", e)))?;
    sqlx::query(
        "INSERT INTO system_configs (config_type, key, value) VALUES ($1, $2, $3)
         ON CONFLICT (config_type, key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
    )
    .bind(WEBHOOK_CONFIG_TYPE)
    .bind(WEBHOOK_TARGETS_KEY)
    .bind(&value)
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(msg("server.db.operation_failed").with("error", e)))?;
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

/// 发送企业微信群机器人消息（text，标题与正文以换行拼接）；
/// `url` 为群机器人完整 Webhook 地址。
async fn send_wecom(url: &str, title: &str, body: &str) -> Result<(), AppError> {
    let payload = serde_json::json!({
        "msgtype": "text",
        "text": { "content": format!("{title}\n{body}") },
    });
    post_json(url, &payload).await
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

/// 日志中的目标标识：备注名优先，空时退回渠道标识。
fn target_label(target: &WebhookTarget) -> String {
    let name = target.name.trim();
    if name.is_empty() {
        target.channel.label().to_string()
    } else {
        name.to_string()
    }
}

/// 记录单目标发送结果（成功/失败均记日志，不向调用方传播错误）。
fn record_outcome(label: &str, outcome: Result<(), AppError>) {
    match outcome {
        Ok(()) => log_info!("log.webhook.sent", channel = label),
        Err(e) => log_error!("log.webhook.send_failed", channel = label, error = e),
    }
}

/// 向全部已保存且订阅了该类型的目标推送业务通知。
///
/// 各目标按自身订阅范围过滤；单目标失败仅记日志不影响其他目标。
pub async fn send_notification_webhook(pool: &PgPool, kind: NotifyKind, title: &str, body: &str) {
    let targets = load_webhook_targets(pool).await;
    for target in targets.iter().filter(|t| t.scope.allows(kind)) {
        let label = target_label(target);
        let outcome = match target.channel {
            WebhookChannel::Generic => send_generic(&target.url, title, body).await,
            WebhookChannel::DingTalk => {
                send_dingtalk(&target.url, &target.secret, title, body).await
            }
            WebhookChannel::Wecom => send_wecom(&target.url, title, body).await,
        };
        record_outcome(&label, outcome);
    }
}

/// 向指定目标发送测试消息（复用已保存配置；目标不存在返回校验错误）。
pub async fn test_webhook_target(pool: &PgPool, target_id: &str) -> Result<(), AppError> {
    let targets = load_webhook_targets(pool).await;
    let target = targets
        .into_iter()
        .find(|t| t.id == target_id)
        .ok_or_else(|| AppError::Validation(msg("server.webhook.target_not_found")))?;

    let title = "FOIMS Webhook 测试";
    let body = "这是一条 Webhook 连接测试消息，请忽略，无需回复。";
    match target.channel {
        WebhookChannel::Generic => send_generic(&target.url, title, body).await,
        WebhookChannel::DingTalk => send_dingtalk(&target.url, &target.secret, title, body).await,
        WebhookChannel::Wecom => send_wecom(&target.url, title, body).await,
    }
}

// ---- 业务通知封装（与 smtp 侧发送函数一一对应，文案复用其 *_message 函数，
// 内部完成目标订阅范围过滤，调用点无需再读配置） ----

/// 发送 IP 地址变更 Webhook 通知（仅推送给订阅 ip_change 类型的目标）。
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
    send_notification_webhook(pool, NotifyKind::IpChange, &title, &body).await;
}

/// 发送工位 IP 上设备更换 Webhook 通知（仅推送给订阅 ip_change 类型的目标）。
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
    send_notification_webhook(pool, NotifyKind::IpChange, &title, &body).await;
}

/// 发送主机资源告警 Webhook 通知（仅推送给订阅 agent_alert 类型的目标）。
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
    send_notification_webhook(pool, NotifyKind::AgentAlert, &title, &body).await;
}

/// 发送未知设备接入 Webhook 通知（仅推送给订阅 snmp_event 类型的目标）。
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
    send_notification_webhook(pool, NotifyKind::SnmpEvent, &title, &body).await;
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
    fn 目标_json_平铺序列化往返() {
        // scope 平铺存储（无嵌套对象），secret 缺省视为空
        let json = r#"{"id":"t1","name":"运维群","channel":"dingtalk","url":"https://oapi.dingtalk.com/robot/send?access_token=x","secret":"cipher","ip_change":true,"agent_alert":false}"#;
        let target: WebhookTarget =
            serde_json::from_str(json).unwrap_or_else(|e| panic!("目标 JSON 应可读: {e}"));
        assert_eq!(target.channel, WebhookChannel::DingTalk);
        assert!(!target.scope.agent_alert);
        assert!(target.scope.ip_change);
        assert!(target.scope.snmp_event, "缺省字段按启用处理");

        let encoded = serde_json::to_string(&target).unwrap_or_else(|e| panic!("序列化失败: {e}"));
        let back: WebhookTarget =
            serde_json::from_str(&encoded).unwrap_or_else(|e| panic!("回读失败: {e}"));
        assert_eq!(back.id, "t1");
        assert_eq!(back.secret, "cipher");
    }

    #[test]
    fn 渠道标识输出() {
        assert_eq!(WebhookChannel::Generic.label(), "generic");
        assert_eq!(WebhookChannel::DingTalk.label(), "dingtalk");
        assert_eq!(WebhookChannel::Wecom.label(), "wecom");
    }
}
