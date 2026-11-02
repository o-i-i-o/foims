//! SMTP 邮件渠道：配置读取（密码 AES-GCM 加密存储）、发送与订阅收件人解析。
//!
//! 从 `system_configs` 表读取 SMTP 配置（密码以 AES-GCM 加密存储），
//! 通过 lettre 发送邮件。所有函数统一返回 `Result<_, AppError>`，
//! 与项目错误处理风格保持一致。
//!
//! 通知收件人存于 `system_configs`（config_type='notification'，
//! key='email_recipients'，JSON 对象数组）：每个收件人（系统用户）以
//! `NotifyScope` 独立订阅通知类型，发送时按类型过滤收件人，与 Webhook
//! 渠道（[`crate::webhook`]）彼此独立、互不依赖。业务事件的文案组装
//! （标题/正文）留在各生产方 crate，本模块只按 `(title, body)` 发送。

use std::time::Duration;

use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use foims_common::AppError;
use foims_common::crypto::{decrypt_password_async, encrypt_password_async};
use foims_common::{log_error, log_warn, msg};

use crate::kind::{NotifyKind, NotifyScope};
use crate::lang::{LocalizedText, NotifyLang};

const SMTP_TIMEOUT: Duration = Duration::from_secs(30);
const SPAWN_BLOCKING_TIMEOUT: Duration = Duration::from_secs(60);

/// 发送邮件（带超时保护，阻塞发送移入 spawn_blocking 线程池）。
async fn send_with_timeout(transport: SmtpTransport, email: Message) -> Result<(), AppError> {
    tokio::time::timeout(
        SPAWN_BLOCKING_TIMEOUT,
        tokio::task::spawn_blocking(move || transport.send(&email)),
    )
    .await
    .map_err(|_| AppError::Internal(msg("server.smtp.send_timeout")))?
    .map_err(|e| AppError::Internal(msg("server.smtp.send_task_failed").with("error", e)))?
    .map_err(|e| AppError::Internal(msg("server.smtp.send_failed").with("error", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 合法地址解析成功() {
        assert!(parse_address("ops@example.com").is_ok());
        assert!(parse_address("a.b+c@sub.example.co.uk").is_ok());
    }

    #[test]
    fn 非法地址返回校验错误() {
        assert!(parse_address("not-an-email").is_err());
    }

    #[test]
    fn secure_与_qq_主机走_relay_其余明文构造成功() {
        let base = SmtpConfig {
            host: "smtp.example.com".to_string(),
            port: 587,
            username: "u".to_string(),
            password: "p".to_string(),
            from: "ops@example.com".to_string(),
            secure: true,
        };
        // secure 走 relay（STARTTLS），仅构造传输器不建立连接
        assert!(build_smtp_transport(&base).is_ok());
        // 明文分支：builder_dangerous 同样只构造不连接
        let insecure = SmtpConfig {
            secure: false,
            host: "mail.example.com".to_string(),
            ..base.clone()
        };
        assert!(build_smtp_transport(&insecure).is_ok());
        // QQ 邮箱即便 secure=false 也强制走 relay
        let qq = SmtpConfig {
            secure: false,
            host: "smtp.qq.com".to_string(),
            ..base
        };
        assert!(build_smtp_transport(&qq).is_ok());
    }

    #[test]
    fn 收件人_json_平铺解析与序列化() {
        // scope 以 flatten 平铺存储（无嵌套 scope 对象），缺省字段按启用处理
        let r: MailRecipient = serde_json::from_str(
            r#"{"user_id":"0b9e6b5e-0f1e-4c2a-9d3b-1f2a3b4c5d6e","agent_alert":false}"#,
        )
        .unwrap_or_else(|e| panic!("平铺结构应可读: {e}"));
        assert!(r.scope.ip_change && r.scope.snmp_event);
        assert!(!r.scope.agent_alert);
        // 存量 JSON 无 language 字段：按中文回落处理
        assert_eq!(r.language, NotifyLang::Zh);
    }
}

/// 解析邮箱地址，失败时返回携带原始地址的验证错误。
fn parse_address(raw: &str) -> Result<Address, AppError> {
    raw.parse().map_err(|e| {
        AppError::Validation(
            msg("server.smtp.invalid_address")
                .with("address", raw)
                .with("error", e),
        )
    })
}

/// 使用数据库中的 SMTP 配置发送邮件。
pub async fn send_email_async(
    pool: &sqlx::PgPool,
    to_address: &str,
    subject: &str,
    body: &str,
) -> Result<(), AppError> {
    let smtp_config = get_smtp_config_from_db(pool)
        .await
        .ok_or_else(|| AppError::Validation(msg("server.smtp.not_configured")))?;

    let email = Message::builder()
        .from(parse_address(&smtp_config.from)?.into())
        .to(parse_address(to_address)?.into())
        .subject(subject)
        .body(body.to_string())
        .map_err(|e| AppError::Internal(msg("server.smtp.build_failed").with("error", e)))?;

    let transport = build_smtp_transport(&smtp_config)?;

    send_with_timeout(transport, email).await
}

/// 根据配置构建 SMTP 传输器。
///
/// `secure` 或 QQ 邮箱等强制 TLS 的主机走 relay（STARTTLS）；
/// 其余走明文连接并记录告警。
fn build_smtp_transport(config: &SmtpConfig) -> Result<SmtpTransport, AppError> {
    let credentials = Credentials::new(config.username.clone(), config.password.clone());

    if config.secure || config.host == "smtp.qq.com" {
        let transport = SmtpTransport::relay(&config.host)
            .map_err(|e| AppError::Internal(msg("server.smtp.connect_failed").with("error", e)))?
            .port(config.port)
            .credentials(credentials)
            .timeout(Some(SMTP_TIMEOUT))
            .build();
        Ok(transport)
    } else {
        log_warn!("log.smtp.insecure_connection", host = config.host);
        Ok(SmtpTransport::builder_dangerous(&config.host)
            .port(config.port)
            .credentials(credentials)
            .timeout(Some(SMTP_TIMEOUT))
            .build())
    }
}

/// SMTP 连接配置（`password` 在内存中为明文，落库前加密）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub secure: bool,
}

/// SMTP 配置读取失败原因。
pub enum SmtpConfigError {
    NotConfigured,
    QueryFailed(String),
    Incomplete,
}

/// 读取 SMTP 配置；未配置或读取失败时返回 `None` 并记录日志。
pub async fn get_smtp_config_from_db(pool: &PgPool) -> Option<SmtpConfig> {
    match get_smtp_config_from_db_inner(pool).await {
        Ok(config) => config,
        Err(SmtpConfigError::NotConfigured) => None,
        Err(SmtpConfigError::QueryFailed(e)) => {
            log_error!("log.smtp.query_failed", error = e);
            None
        }
        Err(SmtpConfigError::Incomplete) => {
            log_warn!("log.smtp.incomplete");
            None
        }
    }
}

async fn get_smtp_config_from_db_inner(
    pool: &PgPool,
) -> Result<Option<SmtpConfig>, SmtpConfigError> {
    let rows = sqlx::query(
        "SELECT key, value FROM system_configs WHERE config_type = 'smtp' ORDER BY key",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| SmtpConfigError::QueryFailed(e.to_string()))?;

    if rows.is_empty() {
        return Ok(None);
    }

    let mut host = String::new();
    let mut port = 0u16;
    let mut username = String::new();
    let mut password = String::new();
    let mut from = String::new();
    let mut secure = false;

    for row in rows {
        let key: String = row.get("key");
        let value: Option<String> = row.get("value");
        if let Some(value) = value {
            match key.as_str() {
                "host" => host = value,
                "port" => {
                    port = value.parse().map_err(|e| {
                        SmtpConfigError::QueryFailed(format!("port parse failed: {e}"))
                    })?
                }
                "username" => username = value,
                "password" => {
                    password = decrypt_password_async(value).await.map_err(|e| {
                        SmtpConfigError::QueryFailed(format!("password decrypt failed: {e}"))
                    })?
                }
                "from" => from = value,
                "secure" => {
                    secure = value.parse().map_err(|e| {
                        SmtpConfigError::QueryFailed(format!("secure parse failed: {e}"))
                    })?
                }
                _ => {}
            }
        }
    }

    if !host.is_empty() && port > 0 && !username.is_empty() && !from.is_empty() {
        Ok(Some(SmtpConfig {
            host,
            port,
            username,
            password,
            from,
            secure,
        }))
    } else {
        Err(SmtpConfigError::Incomplete)
    }
}

/// 保存 SMTP 配置到数据库（密码加密后以 key-value 形式逐项写入）。
pub async fn save_smtp_config_to_db(pool: &PgPool, config: &SmtpConfig) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;

    let encrypted_password = encrypt_password_async(config.password.clone())
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.smtp.password_encrypt_failed").with("error", e))
        })?;

    let smtp_configs = [
        ("host", config.host.clone()),
        ("port", config.port.to_string()),
        ("username", config.username.clone()),
        ("password", encrypted_password),
        ("from", config.from.clone()),
        ("secure", config.secure.to_string()),
    ];

    for (key, value) in smtp_configs {
        sqlx::query(
            "INSERT INTO system_configs (config_type, key, value)
                     VALUES ('smtp', $1, $2)
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

/// 向配置的发件人自发自收一封测试邮件，验证 SMTP 连通性。
///
/// 测试邮件收件人即配置的发件人自身，无通知卡片语言设置来源，
/// 故主题与正文以中英双语并列呈现。
pub async fn test_smtp_connection(config: &SmtpConfig) -> Result<(), AppError> {
    let email = Message::builder()
        .from(parse_address(&config.from)?.into())
        .to(parse_address(&config.from)?.into())
        .subject("SMTP连接测试 / SMTP Connection Test")
        .body(
            "这是一封SMTP连接测试邮件，无需回复。\nThis is an SMTP connection test email, no reply is needed."
                .to_string(),
        )
        .map_err(|e| AppError::Internal(msg("server.smtp.build_failed").with("error", e)))?;

    let transport = build_smtp_transport(config)?;

    send_with_timeout(transport, email).await
}

/// 轻量检查 SMTP 是否已完整配置（不解密密码，不写日志）。
///
/// 供登录方式发现等公开端点使用：未配置时邮件登录入口应静默隐藏，
/// 而不是在运行日志里反复产生告警。
pub async fn smtp_configured(pool: &PgPool) -> bool {
    let Ok(count) = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(DISTINCT key) FROM system_configs
         WHERE config_type = 'smtp' AND key IN ('host', 'port', 'username', 'from')
           AND value IS NOT NULL AND value <> ''",
    )
    .fetch_one(pool)
    .await
    else {
        return false;
    };
    count >= 4
}

/// 向指定用户群发邮件（收件人缺失视为业务错误而非跳过）。
pub async fn send_email_to_users(
    pool: &PgPool,
    user_ids: &[Uuid],
    subject: &str,
    body: &str,
) -> Result<(), AppError> {
    let Some(smtp_config) = get_smtp_config_from_db(pool).await else {
        log_warn!("log.smtp.not_configured");
        return Err(AppError::Validation(msg("server.smtp.not_configured")));
    };

    let users = sqlx::query("SELECT email FROM users WHERE id = ANY($1)")
        .bind(user_ids)
        .fetch_all(pool)
        .await?;

    if users.is_empty() {
        return Err(AppError::Validation(msg(
            "server.smtp.recipients_not_found",
        )));
    }

    let mut recipients: Vec<String> = Vec::new();
    for row in users {
        let email: Option<String> = row.get("email");
        if let Some(email) = email {
            recipients.push(email);
        }
    }

    if recipients.is_empty() {
        return Err(AppError::Validation(msg("server.smtp.recipients_no_email")));
    }

    // 收件人全部放 BCC：群发邮箱列表互相不可见（To/Cc 会向全部收件人泄露地址）。
    // BCC 地址仍进入 SMTP 信封（lettre 发送时剥离 BCC 头），邮件可正常投递
    let mut email_builder = Message::builder()
        .from(parse_address(&smtp_config.from)?.into())
        .subject(subject);

    for recipient in &recipients {
        email_builder = email_builder.bcc(parse_address(recipient)?.into());
    }

    let email = email_builder
        .body(body.to_string())
        .map_err(|e| AppError::Internal(msg("server.smtp.build_failed").with("error", e)))?;

    let transport = build_smtp_transport(&smtp_config)?;

    send_with_timeout(transport, email).await
}

/// 通知邮件收件人（系统用户 + 订阅范围）。
///
/// 存储为 `system_configs`（config_type='notification'，key='email_recipients'）
/// JSON 对象数组的元素，scope 以 flatten 平铺存储（无嵌套对象）。
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct MailRecipient {
    /// 收件人用户 ID（发送时经 users 表解析邮箱）
    pub user_id: Uuid,
    /// 通知语言（存量 JSON 无该字段按中文处理）
    #[serde(default)]
    pub language: NotifyLang,
    #[serde(flatten)]
    pub scope: NotifyScope,
}

/// 读取通知邮件收件人及其订阅范围。
///
/// 返回 None 表示「收件人未配置/读取失败，静默跳过发送」：配置缺失是正常
/// 业务状态；配置损坏或查询失败降级为跳过但必须留痕，不得伪装成已发送。
pub async fn load_mail_recipients(pool: &PgPool) -> Option<Vec<MailRecipient>> {
    let value = match sqlx::query_scalar::<_, String>(
        "SELECT value FROM system_configs WHERE config_type = 'notification' AND key = 'email_recipients'",
    )
    .fetch_optional(pool)
    .await
    {
        Ok(Some(value)) => value,
        Ok(None) => {
            log_warn!("log.smtp.no_recipients_configured");
            return None;
        }
        Err(e) => {
            log_error!("log.smtp.recipients_query_failed", error = e);
            return None;
        }
    };
    match serde_json::from_str::<Vec<MailRecipient>>(&value) {
        Ok(recipients) if !recipients.is_empty() => Some(recipients),
        Ok(_) => {
            log_warn!("log.smtp.recipients_empty");
            None
        }
        Err(e) => {
            // 配置损坏时降级为跳过发送，但必须留痕而非静默吞掉
            log_error!("log.smtp.recipients_parse_failed", error = e);
            None
        }
    }
}

/// 按通知类型过滤收件人后按语言分组发送邮件（无有效收件人时静默跳过）。
///
/// 收件人按各自设置的 `language` 分为 zh/en 两组（最多两封 BCC 邮件），
/// 每组文案取该组语言；单组发送失败仅 log_error 留痕并继续另一组，
/// 任一组失败即向上报错由调用方按业务语境留痕（沿用尽力发送语义）。
pub(crate) async fn send_notification_email(
    pool: &PgPool,
    kind: NotifyKind,
    title: &LocalizedText,
    body: &LocalizedText,
) -> Result<(), AppError> {
    let Some(recipients) = load_mail_recipients(pool).await else {
        return Ok(());
    };
    let mut subscribed = 0usize;
    let mut failed = false;
    for lang in [NotifyLang::Zh, NotifyLang::En] {
        let user_ids: Vec<Uuid> = recipients
            .iter()
            .filter(|r| r.language == lang && r.scope.allows(kind))
            .map(|r| r.user_id)
            .collect();
        if user_ids.is_empty() {
            continue;
        }
        subscribed += user_ids.len();
        if let Err(e) = send_email_to_users(pool, &user_ids, title.pick(lang), body.pick(lang))
            .await
        {
            log_error!("log.smtp.group_send_failed", lang = lang.label(), error = e);
            failed = true;
        }
    }
    if subscribed == 0 {
        log_warn!("log.smtp.recipients_empty");
        return Ok(());
    }
    if failed {
        return Err(AppError::Internal(msg("server.smtp.group_send_failed")));
    }
    Ok(())
}
