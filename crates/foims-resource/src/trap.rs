//! SNMP Trap/Inform 接收服务：常驻 UDP 监听，收到的消息逐管理员写入站内通知，
//! 在「日志-通知」页面展示。
//!
//! - v1/v2c：可选 community 白名单（`communities` 为空表示接受任意）；
//! - v3：按 `users` 配置 USM 用户，未配置任何用户时拒绝全部 v3 通知；
//!   引擎 ID 每次启动随机生成（发送方经发现流程自动感知，无需持久化 boots）；
//! - inform：接受后由库自动应答 Response-PDU，业务侧无需额外处理；
//! - 冷却窗口：同一来源地址在 `cooldown_secs` 内的后续通知只记服务日志，
//!   不重复写站内通知，防止 trap 风暴刷爆通知列表。

use std::convert::Infallible;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use async_snmp::VarBind;
use async_snmp::notification::{Notification, NotificationReceiver, ReceivedNotification};
use async_snmp::v3::{AuthoritativeEngine, UsmUser};
use dashmap::DashMap;
use foims_common::config::{SnmpTrapConfig, SnmpTrapUsmUser};
use foims_common::{log_error, log_info, log_warn};
use tokio::sync::broadcast::Receiver as ShutdownReceiver;
use uuid::Uuid;

use crate::helpers::create_notification;

/// 站内通知类型标识（notifications.notification_type 列）
const NOTIFICATION_TYPE_SNMP_TRAP: &str = "snmp_trap";
/// 站内通知标题 i18n key（前端按用户语言翻译展示）
const NOTIFICATION_TITLE_KEY: &str = "server.notification.snmp_trap.title";
/// 无变量绑定时的正文 i18n key
const NOTIFICATION_BODY_KEY: &str = "server.notification.snmp_trap.body";
/// 带变量绑定时的正文 i18n key
const NOTIFICATION_BODY_VARBINDS_KEY: &str = "server.notification.snmp_trap.body_varbinds";
/// 变量绑定文本最大长度（超出截断，防止超长 trap 撑爆通知内容）
const MAX_VARBINDS_TEXT_CHARS: usize = 500;
/// 冷却表容量上限：达到后清理过期条目，防止伪造来源地址撑大内存
const COOLDOWN_MAP_CAPACITY: usize = 1024;

/// 启动 Trap/Inform 接收任务；`enabled = false` 时不启动。
pub fn start_trap_receiver(
    pool: sqlx::PgPool,
    config: SnmpTrapConfig,
    shutdown: ShutdownReceiver<()>,
) {
    if !config.enabled {
        return;
    }
    tokio::spawn(run_trap_receiver(pool, config, shutdown));
}

/// 接收主循环：绑定失败或运行错误只记日志不终止进程（接收为可选功能）。
async fn run_trap_receiver(
    pool: sqlx::PgPool,
    config: SnmpTrapConfig,
    mut shutdown: ShutdownReceiver<()>,
) {
    let receiver = match build_receiver(&config).await {
        Ok(receiver) => receiver,
        Err(message) => {
            log_error!(
                "system.snmp_trap_bind_failed",
                addr = config.bind_addr,
                error = message
            );
            return;
        }
    };
    log_info!("system.snmp_trap_started", addr = receiver.local_addr());

    let cooldowns: DashMap<IpAddr, Instant> = DashMap::new();
    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                log_info!("system.snmp_trap_stopped");
                return;
            }
            received = receiver.recv() => match received {
                Ok(item) => handle_notification(&pool, &config, item, &cooldowns).await,
                Err(e) => {
                    // 接收错误不终止监听；短暂退避避免持续错误时热循环
                    log_error!("system.snmp_trap_recv_error", error = e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            },
        }
    }
}

/// 按配置构建通知接收器。
async fn build_receiver(config: &SnmpTrapConfig) -> Result<NotificationReceiver, String> {
    let mut builder = NotificationReceiver::builder().bind(config.bind_addr.as_str());

    if !config.communities.is_empty() {
        builder = builder.communities(config.communities.iter().map(String::as_str));
    }

    // 库要求：配置任一 USM 用户必须同时提供权威引擎与显式接受策略
    if !config.users.is_empty() {
        let engine_id = async_snmp::v3::generate_engine_id().map_err(|e| e.to_string())?;
        // 持久化回调保持无操作：引擎 ID 每次启动重新随机生成，重启后发送方
        // 重新发现即可，不存在跨重启的 boots 时序问题
        let engine = AuthoritativeEngine::install(engine_id, |_| Ok::<(), Infallible>(()))
            .map_err(|e| e.to_string())?;
        builder = builder.authoritative_engine(engine);

        for user in &config.users {
            match build_usm_user(user) {
                Ok(built) => {
                    // 闭包必须持有全部所有权数据（'static）：库把闭包存入接收器，
                    // 不得借用本次循环的配置引用
                    let username = user.username.clone();
                    builder = builder
                        .usm_user(username, move |_| Ok(built))
                        .map_err(|e| e.to_string())?;
                }
                Err(message) => {
                    log_error!(
                        "system.snmp_trap_user_invalid",
                        username = user.username,
                        error = message
                    );
                }
            }
        }
        builder = builder.accept_all_notifications();
    }

    builder.build().await.map_err(|e| e.to_string())
}

// 认证/加密协议字符串解析复用 device::snmp 的唯一定义
//（库内 FromStr 别名表 + AES-192/256 Blumenthal 旧别名，覆盖范围
// 为本模块旧手写解析表的超集），不再各自维护匹配表
use crate::device::snmp::{parse_auth_protocol, parse_priv_protocol};

/// 按配置构建 USM 用户；配置非法时返回错误说明，调用方跳过该用户继续。
fn build_usm_user(config: &SnmpTrapUsmUser) -> Result<UsmUser, String> {
    let auth = if config.auth_protocol.is_empty() {
        None
    } else {
        Some(parse_auth_protocol(&config.auth_protocol)?)
    };
    let privacy = if config.priv_protocol.is_empty() {
        None
    } else {
        Some(parse_priv_protocol(&config.priv_protocol)?)
    };

    let user = UsmUser::new(config.username.clone());
    let user = match (auth, privacy) {
        (None, None) => user,
        (Some(a), None) => user
            .auth(a, config.auth_password.as_bytes())
            .map_err(|e| e.to_string())?,
        (Some(a), Some(p)) => user
            .auth_priv(
                a,
                config.auth_password.as_bytes(),
                p,
                config.priv_password.as_bytes(),
            )
            .map_err(|e| e.to_string())?,
        // USM 安全级别没有 noAuth + priv 的组合，加密必须与认证配套
        (None, Some(_)) => return Err("配置了加密协议但未配置认证协议".to_string()),
    };
    Ok(user)
}

/// 处理一条已接受的 Trap/Inform：冷却限流后逐管理员写入站内通知。
async fn handle_notification(
    pool: &sqlx::PgPool,
    config: &SnmpTrapConfig,
    received: ReceivedNotification,
    cooldowns: &DashMap<IpAddr, Instant>,
) {
    let notification = received.notification;
    let source_ip = received.source.ip();
    let version = notification.version().to_string();
    let kind = notification_kind(&notification);
    let trap_oid = notification
        .trap_oid()
        .map(|oid| oid.to_string())
        .unwrap_or_else(|_| "-".to_string());

    log_info!(
        "system.snmp_trap_received",
        source = source_ip,
        version = version,
        kind = kind,
        oid = trap_oid
    );

    if config.cooldown_secs > 0 && is_in_cooldown(cooldowns, source_ip, config.cooldown_secs) {
        log_warn!("system.snmp_trap_suppressed", source = source_ip);
        return;
    }

    let varbinds_text = format_varbinds(notification.varbinds());
    let body_key = if varbinds_text.is_empty() {
        NOTIFICATION_BODY_KEY
    } else {
        NOTIFICATION_BODY_VARBINDS_KEY
    };
    let content = serde_json::json!({
        "key": body_key,
        "params": {
            "source": source_ip.to_string(),
            "version": version,
            "kind": kind,
            "trap_oid": trap_oid,
            "varbinds": varbinds_text,
        }
    })
    .to_string();

    // 通知目标：所有启用状态的管理员（admin/secadmin）按人各发一条，
    // 已读状态随用户独立（与 MAC 变更通知语义一致）
    let admin_ids: Vec<Uuid> = match sqlx::query_scalar(
        "SELECT id FROM users WHERE status = TRUE AND role IN ('admin', 'secadmin')",
    )
    .fetch_all(pool)
    .await
    {
        Ok(ids) => ids,
        Err(e) => {
            log_error!("system.snmp_trap_recipients_query_failed", error = e);
            return;
        }
    };

    let mut written = 0usize;
    for admin_id in &admin_ids {
        // 单条写入失败不中断剩余收件人
        if create_notification(
            pool,
            NOTIFICATION_TITLE_KEY,
            &content,
            NOTIFICATION_TYPE_SNMP_TRAP,
            Some(admin_id),
        )
        .await
        .is_ok()
        {
            written += 1;
        } else {
            log_error!(
                "system.snmp_trap_notification_write_failed",
                source = source_ip
            );
        }
    }
    if written > 0 {
        log_info!(
            "system.snmp_trap_notification_created",
            source = source_ip,
            recipients = written
        );
    }
}

/// 通知种类文案：trap（未确认）/ inform（已确认）。
fn notification_kind(notification: &Notification) -> &'static str {
    if notification.is_confirmed() {
        "inform"
    } else {
        "trap"
    }
}

/// 判断来源是否处于冷却窗口内；不在窗口时记录本次时间并做容量清理。
fn is_in_cooldown(
    cooldowns: &DashMap<IpAddr, Instant>,
    source: IpAddr,
    cooldown_secs: u64,
) -> bool {
    let now = Instant::now();
    let window = Duration::from_secs(cooldown_secs);
    let in_cooldown = cooldowns
        .get(&source)
        .is_some_and(|entry| now.duration_since(*entry) < window);
    if in_cooldown {
        return true;
    }
    // 容量保护：条目达到上限时先清理过期来源再记录当前来源
    if cooldowns.len() >= COOLDOWN_MAP_CAPACITY {
        cooldowns.retain(|_, last| now.duration_since(*last) < window);
    }
    cooldowns.insert(source, now);
    false
}

/// 将变量绑定格式化为 `oid=值` 列表（`; ` 分隔），超长截断。
fn format_varbinds(varbinds: &[VarBind]) -> String {
    let text = varbinds
        .iter()
        .map(|vb| format!("{}={}", vb.oid, vb.value))
        .collect::<Vec<_>>()
        .join("; ");
    truncate_text(&text, MAX_VARBINDS_TEXT_CHARS)
}

/// 按字符边界截断文本，截断处以「…」结尾。
fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}…")
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use async_snmp::v3::{AuthProtocol, PrivProtocol};
    use async_snmp::{Value, oid};

    #[test]
    fn test_parse_auth_protocol() {
        // 共享解析器返回 Result；.ok() 后与旧 Option 口径对齐断言
        assert_eq!(
            parse_auth_protocol("SHA256").ok(),
            Some(AuthProtocol::Sha256)
        );
        assert_eq!(parse_auth_protocol("sha").ok(), Some(AuthProtocol::Sha1));
        assert_eq!(parse_auth_protocol("md5").ok(), Some(AuthProtocol::Md5));
        assert!(parse_auth_protocol("rc4").is_err());
        assert!(parse_auth_protocol("").is_err());
    }

    #[test]
    fn test_parse_priv_protocol() {
        assert_eq!(parse_priv_protocol("AES").ok(), Some(PrivProtocol::Aes128));
        assert_eq!(
            parse_priv_protocol("aes256").ok(),
            Some(PrivProtocol::Aes256Blumenthal)
        );
        assert_eq!(parse_priv_protocol("3des").ok(), Some(PrivProtocol::Des3));
        assert!(parse_priv_protocol("chacha").is_err());
    }

    #[test]
    fn test_build_usm_user_no_auth_no_priv() {
        let config = SnmpTrapUsmUser {
            username: "trapuser".to_string(),
            auth_protocol: String::new(),
            auth_password: String::new(),
            priv_protocol: String::new(),
            priv_password: String::new(),
        };
        let user = build_usm_user(&config).unwrap_or_else(|e| panic!("应为合法配置: {e}"));
        assert_eq!(user.username().as_ref(), b"trapuser");
    }

    #[test]
    fn test_build_usm_user_priv_requires_auth() {
        let config = SnmpTrapUsmUser {
            username: "trapuser".to_string(),
            auth_protocol: String::new(),
            auth_password: String::new(),
            priv_protocol: "aes128".to_string(),
            priv_password: "privpass".to_string(),
        };
        assert!(build_usm_user(&config).is_err());
    }

    #[test]
    fn test_build_usm_user_unknown_protocol() {
        let config = SnmpTrapUsmUser {
            username: "trapuser".to_string(),
            auth_protocol: "rot13".to_string(),
            auth_password: String::new(),
            priv_protocol: String::new(),
            priv_password: String::new(),
        };
        assert!(build_usm_user(&config).is_err());
    }

    #[test]
    fn test_format_varbinds() {
        let varbinds = vec![
            VarBind::new(oid!(1, 3, 6, 1, 2, 1, 1, 3, 0), Value::Integer(42)),
            VarBind::new(oid!(1, 3, 6, 1, 4, 1, 8072, 999), Value::Null),
        ];
        assert_eq!(
            format_varbinds(&varbinds),
            "1.3.6.1.2.1.1.3.0=42; 1.3.6.1.4.1.8072.999=NULL"
        );
        assert_eq!(format_varbinds(&[]), "");
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("short", 10), "short");
        let long = "a".repeat(MAX_VARBINDS_TEXT_CHARS + 10);
        let truncated = truncate_text(&long, MAX_VARBINDS_TEXT_CHARS);
        assert_eq!(truncated.chars().count(), MAX_VARBINDS_TEXT_CHARS + 1);
        assert!(truncated.ends_with('…'));
    }
}
