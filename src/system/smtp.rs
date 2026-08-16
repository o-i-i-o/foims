//! SMTP 邮件发送模块。
//!
//! 从 `system_configs` 表读取 SMTP 配置（密码以 AES-GCM 加密存储），
//! 通过 lettre 发送系统通知邮件。所有函数统一返回 `Result<_, AppError>`，
//! 与项目错误处理风格保持一致。

use std::time::Duration;

use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tracing::{error, warn};
use uuid::Uuid;

use crate::crypto::{decrypt_password_async, encrypt_password_async};
use crate::error::AppError;

const SMTP_TIMEOUT: Duration = Duration::from_secs(30);
const SPAWN_BLOCKING_TIMEOUT: Duration = Duration::from_secs(60);

/// 发送邮件（带超时保护，阻塞发送移入 spawn_blocking 线程池）。
async fn send_with_timeout(transport: SmtpTransport, email: Message) -> Result<(), AppError> {
    tokio::time::timeout(
        SPAWN_BLOCKING_TIMEOUT,
        tokio::task::spawn_blocking(move || transport.send(&email)),
    )
    .await
    .map_err(|_| AppError::Internal("邮件发送超时".to_string()))?
    .map_err(|e| AppError::Internal(format!("邮件发送任务失败: {e}")))?
    .map_err(|e| AppError::Internal(format!("发送邮件失败: {e:?}")))?;
    Ok(())
}

/// 解析邮箱地址，失败时返回携带原始地址的验证错误。
fn parse_address(raw: &str) -> Result<Address, AppError> {
    raw.parse()
        .map_err(|e| AppError::Validation(format!("邮箱地址无效: {raw}: {e}")))
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
        .ok_or_else(|| AppError::NotFound("SMTP未配置".to_string()))?;

    let email = Message::builder()
        .from(parse_address(&smtp_config.from)?.into())
        .to(parse_address(to_address)?.into())
        .subject(subject)
        .body(body.to_string())
        .map_err(|e| AppError::Internal(format!("邮件构建失败: {e}")))?;

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
            .map_err(|e| AppError::Internal(format!("邮件服务连接失败: {e}")))?
            .port(config.port)
            .credentials(credentials)
            .timeout(Some(SMTP_TIMEOUT))
            .build();
        Ok(transport)
    } else {
        warn!(
            "使用非加密SMTP连接发送邮件，凭据可能以明文传输 (host: {})",
            config.host
        );
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
            error!("查询SMTP配置失败: {e}");
            None
        }
        Err(SmtpConfigError::Incomplete) => {
            warn!("SMTP配置不完整，缺少必要字段");
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
                    port = value
                        .parse()
                        .map_err(|e| SmtpConfigError::QueryFailed(format!("port解析失败: {e}")))?
                }
                "username" => username = value,
                "password" => {
                    password = decrypt_password_async(value)
                        .await
                        .map_err(|e| SmtpConfigError::QueryFailed(format!("密码解密失败: {e}")))?
                }
                "from" => from = value,
                "secure" => {
                    secure = value
                        .parse()
                        .map_err(|e| SmtpConfigError::QueryFailed(format!("secure解析失败: {e}")))?
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
        .map_err(|e| AppError::Internal(format!("SMTP密码加密失败: {e}")))?;

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
pub async fn test_smtp_connection(config: &SmtpConfig) -> Result<(), AppError> {
    let email = Message::builder()
        .from(parse_address(&config.from)?.into())
        .to(parse_address(&config.from)?.into())
        .subject("SMTP连接测试")
        .body("这是一封SMTP连接测试邮件，无需回复".to_string())
        .map_err(|e| AppError::Internal(format!("邮件构建失败: {e}")))?;

    let transport = build_smtp_transport(config)?;

    send_with_timeout(transport, email).await
}

/// 向指定用户群发邮件（收件人缺失视为业务错误而非跳过）。
pub async fn send_email_to_users(
    pool: &PgPool,
    user_ids: &[Uuid],
    subject: &str,
    body: &str,
) -> Result<(), AppError> {
    let Some(smtp_config) = get_smtp_config_from_db(pool).await else {
        warn!("SMTP配置未设置");
        return Err(AppError::NotFound("SMTP配置未设置".to_string()));
    };

    let users = sqlx::query("SELECT email FROM users WHERE id = ANY($1)")
        .bind(user_ids)
        .fetch_all(pool)
        .await?;

    if users.is_empty() {
        return Err(AppError::NotFound("未找到指定用户".to_string()));
    }

    let mut recipients: Vec<String> = Vec::new();
    for row in users {
        let email: Option<String> = row.get("email");
        if let Some(email) = email {
            recipients.push(email);
        }
    }

    if recipients.is_empty() {
        return Err(AppError::Validation(
            "指定用户没有有效的邮箱地址".to_string(),
        ));
    }

    let mut email_builder = Message::builder()
        .from(parse_address(&smtp_config.from)?.into())
        .subject(subject);

    for recipient in &recipients {
        email_builder = email_builder.to(parse_address(recipient)?.into());
    }

    let email = email_builder
        .body(body.to_string())
        .map_err(|e| AppError::Internal(format!("邮件构建失败: {e}")))?;

    let transport = build_smtp_transport(&smtp_config)?;

    send_with_timeout(transport, email).await
}

/// 发送 MAC 地址变更告警邮件。
///
/// 收件人列表存于 `system_configs`（config_type='notification'，
/// key='email_recipients'，JSON 数组），未配置时静默跳过。
pub async fn send_mac_change_email(
    pool: &PgPool,
    workstation_name: &str,
    ip_address: &str,
    old_mac: &str,
    new_mac: &str,
) -> Result<(), AppError> {
    let user_ids = match sqlx::query_scalar::<_, String>(
        "SELECT value FROM system_configs WHERE config_type = 'notification' AND key = 'email_recipients'",
    )
    .fetch_optional(pool)
    .await
    {
        Ok(Some(recips)) => {
            // 收件人以 JSON 数组形式存储（与 system/config.rs update_notification_settings 写入格式一致）
            serde_json::from_str::<Vec<Uuid>>(&recips).unwrap_or_default()
        }
        Ok(None) => {
            warn!("未配置邮件收件人，跳过MAC变更邮件通知");
            return Ok(());
        }
        Err(e) => {
            error!("查询邮件收件人配置失败: {e}");
            return Ok(());
        }
    };

    if user_ids.is_empty() {
        warn!("邮件收件人列表为空，跳过MAC变更邮件通知");
        return Ok(());
    }

    let email_body = format!(
        "尊敬的管理员：\n\n工位 {workstation_name} 的MAC地址已发生变更，详情如下：\n工位名称：{workstation_name}\nIP地址：{ip_address}\n旧MAC地址：{old_mac}\n新MAC地址：{new_mac}\n\n请确认此变更是否为授权操作。\n\n此致，\nIPMA系统"
    );

    send_email_to_users(
        pool,
        &user_ids,
        &format!("MAC地址变更通知 - 工位: {workstation_name}"),
        &email_body,
    )
    .await
}
