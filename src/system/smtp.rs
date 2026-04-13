use anyhow::Result;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tracing::{error, warn};
use uuid::Uuid;

use crate::crypto::{decrypt_password, encrypt_password};

// SMTP配置结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub secure: bool,
}

// 从数据库获取SMTP配置
pub async fn get_smtp_config_from_db(pool: &PgPool) -> Option<SmtpConfig> {
    let smtp_config_result = sqlx::query(
        "SELECT key, value FROM system_configs WHERE config_type = 'smtp' ORDER BY key",
    )
    .fetch_all(pool)
    .await;

    match smtp_config_result {
        Ok(rows) => {
            if rows.is_empty() {
                None
            } else {
                let mut host = String::new();
                let mut port = 0;
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
                            "password" => password = decrypt_password(&value),
                            "from" => from = value,
                            "secure" => secure = value.parse().unwrap_or(false),
                            _ => {}
                        }
                    }
                }

                if !host.is_empty() && port > 0 && !username.is_empty() && !from.is_empty() {
                    Some(SmtpConfig {
                        host,
                        port,
                        username,
                        password,
                        from,
                        secure,
                    })
                } else {
                    None
                }
            }
        }
        Err(_) => None,
    }
}

// 保存SMTP配置到数据库
pub async fn save_smtp_config_to_db(pool: &PgPool, config: &SmtpConfig) -> Result<()> {
    let mut tx = pool.begin().await?;

    let smtp_configs = vec![
        ("host", config.host.clone()),
        ("port", config.port.to_string()),
        ("username", config.username.clone()),
        ("password", encrypt_password(&config.password)),
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

// 测试SMTP连接
pub async fn test_smtp_connection(config: &SmtpConfig) -> Result<()> {
    // 构建测试邮件
    let email = Message::builder()
        .from(config.from.parse::<Address>()?.into())
        .to(config.from.parse::<Address>()?.into()) // 发送给自己作为测试
        .subject("SMTP连接测试")
        .body("这是一封SMTP连接测试邮件，无需回复".to_string())?;

    // 创建SMTP传输
    let transport = if config.secure || config.host == "smtp.qq.com" {
        // 对于需要安全连接的情况，使用SSL配置
        SmtpTransport::relay(&config.host)?
            .port(config.port)
            .credentials(Credentials::new(
                config.username.clone(),
                config.password.clone(),
            ))
            .build()
    } else {
        // 非安全连接
        SmtpTransport::builder_dangerous(&config.host)
            .port(config.port)
            .credentials(Credentials::new(
                config.username.clone(),
                config.password.clone(),
            ))
            .build()
    };

    // 发送测试邮件
    transport.send(&email)?;
    Ok(())
}

// 发送邮件给指定用户
pub async fn send_email_to_users(
    pool: &PgPool,
    user_ids: &[Uuid],
    subject: &str,
    body: &str,
) -> Result<()> {
    // 获取SMTP配置
    let smtp_config = get_smtp_config_from_db(pool)
        .await
        .ok_or_else(|| anyhow::anyhow!("SMTP配置未设置"))?;

    // 根据user_ids查询用户邮箱
    let users = sqlx::query("SELECT email FROM users WHERE id = ANY($1)")
        .bind(user_ids)
        .fetch_all(pool)
        .await?;

    if users.is_empty() {
        return Err(anyhow::anyhow!("未找到指定用户"));
    }

    // 构建收件人列表
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

    // 构建邮件
    let mut email_builder = Message::builder()
        .from(smtp_config.from.parse::<Address>()?.into())
        .subject(subject);

    // 添加收件人
    for recipient in &recipients {
        email_builder = email_builder.to(recipient.parse::<Address>()?.into());
    }

    // 设置邮件内容
    let email = email_builder.body(body.to_string())?;

    // 创建SMTP传输
    let transport = if smtp_config.secure || smtp_config.host == "smtp.qq.com" {
        SmtpTransport::relay(&smtp_config.host)?
            .port(smtp_config.port)
            .credentials(Credentials::new(
                smtp_config.username.clone(),
                smtp_config.password.clone(),
            ))
            .build()
    } else {
        SmtpTransport::builder_dangerous(&smtp_config.host)
            .port(smtp_config.port)
            .credentials(Credentials::new(
                smtp_config.username.clone(),
                smtp_config.password.clone(),
            ))
            .build()
    };

    // 发送邮件
    transport.send(&email)?;
    Ok(())
}

// 发送MAC地址变更邮件通知
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
        "尊敬的管理员：\n\n工位 {} 的MAC地址已发生变更，详情如下：\n工位名称：{}\nIP地址：{}\n旧MAC地址：{}\n新MAC地址：{}\n\n请确认此变更是否为授权操作。\n\n此致，\nIPMA系统",
        workstation_name, workstation_name, ip_address, old_mac, new_mac
    );

    send_email_to_users(
        pool,
        &user_ids,
        &format!("MAC地址变更通知 - 工位: {}", workstation_name),
        &email_body,
    )
    .await
}
