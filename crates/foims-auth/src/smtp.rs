//! SMTP 邮件发送模块。
//!
//! 从 `system_configs` 表读取 SMTP 配置（密码以 AES-GCM 加密存储），
//! 通过 lettre 发送系统通知邮件。所有函数统一返回 `Result<_, AppError>`，
//! 与项目错误处理风格保持一致。
//!
//! 通知收件人存于 `system_configs`（config_type='notification'，
//! key='email_recipients'，JSON 对象数组）：每个收件人（系统用户）以
//! `NotifyScope` 独立订阅通知类型，发送时按类型过滤收件人，与 Webhook
//! 渠道（`crate::webhook`）彼此独立、互不依赖。

use std::time::Duration;

use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use foims_common::AppError;
use foims_common::crypto::{decrypt_password_async, encrypt_password_async};
use foims_common::{log_error, log_warn, msg};

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
    fn 订阅范围缺省全启用() {
        let s = NotifyScope::default();
        assert!(s.ip_change && s.agent_alert && s.snmp_event);
    }

    #[test]
    fn 订阅范围按类型过滤() {
        let scope = NotifyScope {
            ip_change: false,
            agent_alert: true,
            snmp_event: false,
        };
        assert!(!scope.allows(NotifyKind::IpChange));
        assert!(scope.allows(NotifyKind::AgentAlert));
        assert!(!scope.allows(NotifyKind::SnmpEvent));
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
pub async fn test_smtp_connection(config: &SmtpConfig) -> Result<(), AppError> {
    let email = Message::builder()
        .from(parse_address(&config.from)?.into())
        .to(parse_address(&config.from)?.into())
        .subject("SMTP连接测试")
        .body("这是一封SMTP连接测试邮件，无需回复".to_string())
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

/// 通知业务类型（三类固定通知，触发点各归属其一）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyKind {
    /// IP 地址变更（含工位 IP 上设备更换）
    IpChange,
    /// 主机资源告警
    AgentAlert,
    /// SNMP 观测事件（未知终端接入等）
    SnmpEvent,
}

/// 通知订阅范围：一个通知接收方（邮件收件人 / Webhook 目标）接收哪些
/// 通知类型。邮件与 Webhook 渠道共用本结构，两渠道彼此独立配置。
/// 未落字段（如存量 JSON 缺项）按启用处理：通知多发的代价远小于静默漏报。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct NotifyScope {
    /// IP 地址变更通知（含工位 IP 上设备更换）开关。
    /// 存量键名 mac_change → ip_change 的重命名迁移由 upgrade SQL 负责（见 upgrade/0.32），
    /// 读取路径只认规范键名，不设 alias 兼容旧键。
    #[serde(rename = "ip_change", default = "default_true")]
    pub ip_change: bool,
    /// 主机资源告警通知开关
    #[serde(default = "default_true")]
    pub agent_alert: bool,
    /// SNMP 观测事件通知开关
    #[serde(default = "default_true")]
    pub snmp_event: bool,
}

impl Default for NotifyScope {
    fn default() -> Self {
        Self {
            ip_change: true,
            agent_alert: true,
            snmp_event: true,
        }
    }
}

impl NotifyScope {
    /// 该订阅范围是否接收指定类型的通知
    pub fn allows(self, kind: NotifyKind) -> bool {
        match kind {
            NotifyKind::IpChange => self.ip_change,
            NotifyKind::AgentAlert => self.agent_alert,
            NotifyKind::SnmpEvent => self.snmp_event,
        }
    }
}

fn default_true() -> bool {
    true
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

/// 按通知类型过滤收件人后发送一封邮件（无有效收件人时静默跳过）。
async fn send_notification_email(
    pool: &PgPool,
    kind: NotifyKind,
    subject: &str,
    body: &str,
) -> Result<(), AppError> {
    let Some(recipients) = load_mail_recipients(pool).await else {
        return Ok(());
    };
    let user_ids: Vec<Uuid> = recipients
        .into_iter()
        .filter(|r| r.scope.allows(kind))
        .map(|r| r.user_id)
        .collect();
    if user_ids.is_empty() {
        log_warn!("log.smtp.recipients_empty");
        return Ok(());
    }
    send_email_to_users(pool, &user_ids, subject, body).await
}

/// 主机告警指标的中文名与单位（邮件与 Webhook 文案共用）。
pub fn host_alert_metric_label(metric: &str) -> (&'static str, &'static str) {
    match metric {
        "cpu" => ("CPU使用率", "%"),
        "mem" => ("内存使用率", "%"),
        "disk" => ("磁盘使用率", "%"),
        "temp" => ("温度", "℃"),
        _ => ("指标", ""),
    }
}

/// IP 地址变更通知文案（标题, 正文），邮件与 Webhook 渠道共用。
///
/// MAC 为物理实体、IP 为虚拟实体：本通知表达“设备（{mac}）从登记 IP
/// 移动到了新的观测 IP”，`source_device` 为拉取到该观测的来源设备名。
pub fn ip_change_message(
    source_device: &str,
    workstation_name: &str,
    mac_address: &str,
    registered_ip: &str,
    observed_ip: &str,
) -> (String, String) {
    let title = format!("IP地址变更通知 - 工位: {workstation_name}");
    let body = format!(
        "尊敬的管理员：\n\n工位 {workstation_name} 的设备 IP 地址发生变更，详情如下：\n工位名称：{workstation_name}\n设备MAC：{mac_address}\n登记 IP：{registered_ip}\n观测 IP：{observed_ip}\n数据来源设备：{source_device}\n\n请确认此变更是否为授权操作。\n\n此致，\nFOIMS系统"
    );
    (title, body)
}

/// 发送 IP 地址变更告警邮件（同一物理设备 MAC 出现在非登记的 IP 上）。
///
/// 收件人存于 `system_configs`（config_type='notification'，
/// key='email_recipients'，JSON 对象数组），仅发送订阅了 ip_change 类型的
/// 收件人；无人订阅或未配置时静默跳过。
pub async fn send_ip_change_email(
    pool: &PgPool,
    source_device: &str,
    workstation_name: &str,
    mac_address: &str,
    registered_ip: &str,
    observed_ip: &str,
) -> Result<(), AppError> {
    let (title, body) = ip_change_message(
        source_device,
        workstation_name,
        mac_address,
        registered_ip,
        observed_ip,
    );
    send_notification_email(pool, NotifyKind::IpChange, &title, &body).await
}

/// 工位 IP 上设备更换通知文案（登记 IP 不变、学到新 MAC），邮件与 Webhook 渠道共用。
///
/// 与 IP 变更通知同属“MAC/IP 对应关系异动”类，受 ip_change 开关门控；
/// `source_device` 为拉取到该观测的来源设备名。
pub fn device_swap_message(
    source_device: &str,
    workstation_name: &str,
    ip_address: &str,
    old_mac: &str,
    new_mac: &str,
) -> (String, String) {
    let title = format!("工位设备更换通知 - 工位: {workstation_name}");
    let body = format!(
        "尊敬的管理员：\n\n工位 {workstation_name} 的 IP {ip_address} 上设备更换，详情如下：\n工位名称：{workstation_name}\nIP地址：{ip_address}\n旧MAC地址：{old_mac}\n新MAC地址：{new_mac}\n数据来源设备：{source_device}\n\n请确认此变更是否为授权操作。\n\n此致，\nFOIMS系统"
    );
    (title, body)
}

/// 发送工位 IP 上设备更换告警邮件（登记 IP 不变、学到新 MAC）。
pub async fn send_device_swap_email(
    pool: &PgPool,
    source_device: &str,
    workstation_name: &str,
    ip_address: &str,
    old_mac: &str,
    new_mac: &str,
) -> Result<(), AppError> {
    let (title, body) = device_swap_message(
        source_device,
        workstation_name,
        ip_address,
        old_mac,
        new_mac,
    );
    send_notification_email(pool, NotifyKind::IpChange, &title, &body).await
}

/// 主机资源告警通知文案，邮件与 Webhook 渠道共用。
///
/// `metric` 为 host_alert_episodes 的指标名（cpu/mem/disk/temp），
/// 值与阈值单位随指标变化（温度 ℃，其余 %）。
pub fn host_alert_message(
    hostname: &str,
    ip_address: &str,
    metric: &str,
    value: f64,
    threshold: u8,
) -> (String, String) {
    let (label, unit) = host_alert_metric_label(metric);
    let title = format!("主机资源告警 - {hostname}: {label}超标");
    let body = format!(
        "尊敬的管理员：\n\n主机 {hostname} 触发资源告警，详情如下：\n主机名称：{hostname}\nIP地址：{ip_address}\n告警指标：{label}\n当前值：{value:.1}{unit}\n告警阈值：{threshold}{unit}\n\n请及时确认处理。\n\n此致，\nFOIMS系统"
    );
    (title, body)
}

/// 发送主机资源告警邮件（CPU/内存/磁盘/温度阈值超标）。
pub async fn send_host_alert_email(
    pool: &PgPool,
    hostname: &str,
    ip_address: &str,
    metric: &str,
    value: f64,
    threshold: u8,
) -> Result<(), AppError> {
    let (title, body) = host_alert_message(hostname, ip_address, metric, value, threshold);
    send_notification_email(pool, NotifyKind::AgentAlert, &title, &body).await
}

/// 未知设备接入通知文案，邮件与 Webhook 渠道共用。
///
/// MAC 为物理实体，未知判定基于 MAC 未登记于任何设备网口；`ip`/`interface`/
/// `device_name` 仅为发现位置（在哪个设备端口、哪个 IP 上学到）。
pub fn unknown_device_message(
    device_name: &str,
    interface: &str,
    vlan_id: Option<i32>,
    ip_address: &str,
    mac_address: &str,
) -> (String, String) {
    let vlan_text = vlan_id.map_or_else(|| "-".to_string(), |v| v.to_string());
    let title = format!("未知设备接入通知 - {device_name}/{interface}");
    let body = format!(
        "尊敬的管理员：\n\n在设备 {device_name} 的端口 {interface} 上学到一个未登记的设备，详情如下：\n来源设备：{device_name}\n端口：{interface}\nVLAN：{vlan_text}\nMAC地址：{mac_address}\n发现位 IP：{ip_address}\n\n请确认该设备是否授权接入。\n\n此致，\nFOIMS系统"
    );
    (title, body)
}

/// 发送未知设备接入告警邮件（SNMP ARP/FDB 同步学到未登记的 MAC）。
///
/// 收件人复用 notification_recipients 机制。
pub async fn send_unknown_device_email(
    pool: &PgPool,
    device_name: &str,
    interface: &str,
    vlan_id: Option<i32>,
    ip_address: &str,
    mac_address: &str,
) -> Result<(), AppError> {
    let (title, body) =
        unknown_device_message(device_name, interface, vlan_id, ip_address, mac_address);
    send_notification_email(pool, NotifyKind::SnmpEvent, &title, &body).await
}
