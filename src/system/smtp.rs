use anyhow::Result;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tracing::{error, warn};
use uuid::Uuid;

use crate::crypto::{decrypt_password, encrypt_password};
use crate::error::AppError;

pub async fn send_email_async(
    pool: &sqlx::PgPool,
    to_address: &str,
    subject: &str,
    body: &str,
) -> Result<(), AppError> {
    let smtp_config = get_smtp_config_from_db(pool)
        .await
        .ok_or_else(|| AppError::Internal("SMTP未配置".to_string()))?;

    let from_addr: Address = smtp_config
        .from
        .parse()
        .map_err(|e| AppError::Validation(format!("邮件配置错误: {e}")))?;
    let to_addr: Address = to_address
        .parse()
        .map_err(|e| AppError::Validation(format!("邮箱地址无效: {e}")))?;

    let email = Message::builder()
        .from(from_addr.into())
        .to(to_addr.into())
        .subject(subject)
        .body(body.to_string())
        .map_err(|e| AppError::Internal(format!("邮件构建失败: {e}")))?;

    let transport = build_smtp_transport(&smtp_config)?;

    tokio::task::spawn_blocking(move || transport.send(&email))
        .await
        .map_err(|e| AppError::Internal(format!("邮件发送任务失败: {e}")))?
        .map_err(|e| AppError::Internal(format!("发送邮件失败: {e:?}")))?;

    Ok(())
}

fn build_smtp_transport(config: &SmtpConfig) -> Result<SmtpTransport, AppError> {
    if config.secure || config.host == "smtp.qq.com" {
        let transport = SmtpTransport::relay(&config.host)
            .map_err(|e| AppError::Internal(format!("邮件服务连接失败: {e}")))?
            .port(config.port)
            .credentials(Credentials::new(config.username.clone(), config.password.clone()))
            .build();
        Ok(transport)
    } else {
        warn!("使用非加密SMTP连接发送邮件，凭据可能以明文传输 (host: {})", config.host);
        Ok(SmtpTransport::builder_dangerous(&config.host)
            .port(config.port)
            .credentials(Credentials::new(config.username.clone(), config.password.clone()))
            .build())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub secure: bool,
}

pub enum SmtpConfigError {
    NotConfigured,
    QueryFailed(String),
    Incomplete,
}

pub async fn get_smtp_config_from_db(pool: &PgPool) -> Option<SmtpConfig> {
    match get_smtp_config_from_db_inner(pool).await {
        Ok(config) => config,
        Err(SmtpConfigError::NotConfigured) => None,
        Err(SmtpConfigError::QueryFailed(e)) => {
            error!("查询SMTP配置失败: {}", e);
            None
        }
        Err(SmtpConfigError::Incomplete) => {
            warn!("SMTP配置不完整，缺少必要字段");
            None
        }
    }
}

async fn get_smtp_config_from_db_inner(pool: &PgPool) -> Result<Option<SmtpConfig>, SmtpConfigError> {
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
                "port" => port = value.parse().unwrap_or(0),
                "username" => username = value,
                "password" => password = decrypt_password(&value).unwrap_or_default(),
                "from" => from = value,
                "secure" => secure = value.parse().unwrap_or(false),
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

pub async fn save_smtp_config_to_db(pool: &PgPool, config: &SmtpConfig) -> Result<()> {
    let mut tx = pool.begin().await?;

    let smtp_configs = vec![
        ("host", config.host.clone()),
        ("port", config.port.to_string()),
        ("username", config.username.clone()),
        (
            "password",
            encrypt_password(&config.password).unwrap_or_else(|| {
                error!("SMTP密码加密失败");
                config.password.clone()
            }),
        ),
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

pub async fn test_smtp_connection(config: &SmtpConfig) -> Result<()> {
    let email = Message::builder()
        .from(config.from.parse::<Address>()?.into())
        .to(config.from.parse::<Address>()?.into())
        .subject("SMTP连接测试")
        .body("这是一封SMTP连接测试邮件，无需回复".to_string())?;

    let transport = if config.secure {
        SmtpTransport::relay(&config.host)?
            .port(config.port)
            .credentials(Credentials::new(
                config.username.clone(),
                config.password.clone(),
            ))
            .build()
    } else {
        warn!("使用非加密SMTP连接进行测试 (host: {})", config.host);
        SmtpTransport::builder_dangerous(&config.host)
            .port(config.port)
            .credentials(Credentials::new(
                config.username.clone(),
                config.password.clone(),
            ))
            .build()
    };

    tokio::task::spawn_blocking(move || transport.send(&email))
        .await
        .map_err(|e| anyhow::anyhow!("SMTP测试任务失败: {e}"))?
        .map_err(|e| anyhow::anyhow!("SMTP连接测试失败: {e:?}"))?;

    Ok(())
}

pub async fn send_email_to_users(
    pool: &PgPool,
    user_ids: &[Uuid],
    subject: &str,
    body: &str,
) -> Result<()> {
    let smtp_config = get_smtp_config_from_db(pool)
        .await
        .ok_or_else(|| anyhow::anyhow!("SMTP配置未设置"))?;

    let users = sqlx::query("SELECT email FROM users WHERE id = ANY($1)")
        .bind(user_ids)
        .fetch_all(pool)
        .await?;

    if users.is_empty() {
        return Err(anyhow::anyhow!("未找到指定用户"));
    }

    let mut recipients: Vec<String> = Vec::new();
    for row in users {
        let email: Option<String> = row.get("email");
        if let Some(email) = email {
            recipients.push(email);
        }
    }

    if recipients.is_empty() {
        return Err(anyhow::anyhow!("指定用户没有有效的邮箱地址"));
    }

    let mut email_builder = Message::builder()
        .from(smtp_config.from.parse::<Address>()?.into())
        .subject(subject);

    for recipient in &recipients {
        email_builder = email_builder.to(recipient.parse::<Address>()?.into());
    }

    let email = email_builder.body(body.to_string())?;

    let transport = build_smtp_transport(&smtp_config)?;

    tokio::task::spawn_blocking(move || transport.send(&email))
        .await
        .map_err(|e| anyhow::anyhow!("邮件发送任务失败: {e}"))?
        .map_err(|e| anyhow::anyhow!("发送邮件失败: {e:?}"))?;

    Ok(())
}

pub async fn send_mac_change_email(
    pool: &PgPool,
    workstation_name: &str,
    ip_address: &str,
    old_mac: &str,
    new_mac: &str,
) -> Result<()> {
    let user_ids = match sqlx::query_scalar::<_, String>(
        "SELECT value FROM system_configs WHERE config_type = 'notification' AND key = 'email_recipients'",
    )
    .fetch_optional(pool)
    .await
    {
        Ok(Some(recips)) => {
            recips.split(',')
                .filter_map(|id| Uuid::parse_str(id.trim()).ok())
                .collect::<Vec<Uuid>>()
        }
        Ok(None) => {
            warn!("未配置邮件收件人，跳过MAC变更邮件通知");
            return Ok(());
        }
        Err(e) => {
            error!("查询邮件收件人配置失败: {}", e);
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
