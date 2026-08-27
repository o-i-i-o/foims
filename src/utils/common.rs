//! 通用工具（ILIKE 转义、请求元信息、操作日志、IP 归一化等）。

use std::str::FromStr;
use uuid::Uuid;

use axum::extract::ConnectInfo;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use ipma_common::{log_debug, log_error, log_info, log_warn};
use std::net::SocketAddr;

use hex::encode;

use crate::error::AppError;
use ipma_common::msg;

#[must_use]
pub fn normalize_ipv4_address(ip: &str) -> String {
    if ip.starts_with("::ffff:") {
        ip.strip_prefix("::ffff:").unwrap_or(ip).to_string()
    } else {
        ip.to_string()
    }
}

/// 转义 ILIKE 搜索串中的特殊字符（\、%、_），并包裹为 `%...%` 模糊匹配模式。
///
/// 未转义时，用户输入的 `_` / `%` 会被当作通配符，导致名称含下划线（如 IP、设备型号）
/// 的查询返回错误结果。配合 sqlx 的 `bind` 使用（参数化，非字符串拼接）。
#[must_use]
pub fn escape_like(search: &str) -> String {
    let escaped = search
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

// ==================== CIDR 验证 ====================

/// 验证 CIDR 格式是否合法（与 PostgreSQL CIDR 类型语义一致）
/// ipnetwork crate 会自动归一化（如 2001::36/64 → 2001::/64），
/// 但 PostgreSQL CIDR 要求主机位全为0，因此需额外检查
#[must_use]
pub fn validate_cidr(cidr: &str) -> bool {
    match ipnetwork::IpNetwork::from_str(cidr) {
        Ok(ipnetwork::IpNetwork::V4(net)) => net.ip() == net.network(),
        Ok(ipnetwork::IpNetwork::V6(net)) => net.ip() == net.network(),
        Err(_) => false,
    }
}

#[must_use]
pub fn get_cidr_type(cidr: &str) -> Option<&'static str> {
    match ipnetwork::IpNetwork::from_str(cidr) {
        Ok(ipnetwork::IpNetwork::V4(_)) => Some("ipv4"),
        Ok(ipnetwork::IpNetwork::V6(_)) => Some("ipv6"),
        Err(_) => None,
    }
}

/// 检查子网 CIDR 是否属于父网 CIDR（使用 PostgreSQL 的 << 操作符逻辑）
/// subnet << supernet 表示 subnet 是 supernet 的子网
#[must_use]
pub fn cidr_contains_subnet(subnet: &str, supernet: &str) -> bool {
    let subnet_net = ipnetwork::IpNetwork::from_str(subnet);
    let supernet_net = ipnetwork::IpNetwork::from_str(supernet);

    match (subnet_net, supernet_net) {
        (Ok(subnet), Ok(supernet)) => {
            // 检查类型是否一致（IPv4 vs IPv6）
            match (subnet, supernet) {
                (ipnetwork::IpNetwork::V4(sub), ipnetwork::IpNetwork::V4(super_net)) => {
                    // 子网的 prefixlen 必须大于父网，且子网的网络地址在父网范围内
                    sub.prefix() > super_net.prefix() && super_net.contains(sub.network())
                }
                (ipnetwork::IpNetwork::V6(sub), ipnetwork::IpNetwork::V6(super_net)) => {
                    sub.prefix() > super_net.prefix() && super_net.contains(sub.network())
                }
                _ => false, // IPv4 和 IPv6 不能互相包含
            }
        }
        _ => false,
    }
}

/// 检查网段 CIDR 是否属于区域 CIDR 列表中的任一 CIDR
#[must_use]
pub fn cidr_belongs_to_region(cidr: &str, region_cidrs: &[String]) -> bool {
    if region_cidrs.is_empty() {
        return true; // 如果区域没有定义 CIDR，则不限制
    }

    region_cidrs
        .iter()
        .any(|region_cidr| cidr_contains_subnet(cidr, region_cidr))
}

/// 校验网关地址：格式合法、地址族一致（family 为 "ipv4"/"ipv6"），
/// 且必须落在同族 CIDR 网段范围内（与 PostgreSQL `inet <<= cidr` 语义一致）。
///
/// - 网关未提供（None 或空白）直接通过；
/// - 网关必须能解析为与 family 一致的 IP 地址（容忍 PostgreSQL INET
///   文本表示自带的 "/32"、"/128" 掩码后缀）；
/// - 必须提供同族 CIDR（创建时来自请求，更新时可为库中现值）。
pub fn validate_gateway_in_cidr(
    gateway: Option<&str>,
    cidr: Option<&str>,
    family: &str,
) -> Result<(), crate::error::AppError> {
    let validation_error = |key: String| crate::error::AppError::Validation(msg(key));

    let Some(gateway) = gateway.map(str::trim).filter(|g| !g.is_empty()) else {
        return Ok(());
    };
    let gateway = gateway.split('/').next().unwrap_or_default();

    let parsed: std::net::IpAddr = gateway
        .parse()
        .map_err(|_| validation_error(format!("server.network.{family}_gateway_invalid")))?;
    let family_ok = match parsed {
        std::net::IpAddr::V4(_) => family == "ipv4",
        std::net::IpAddr::V6(_) => family == "ipv6",
    };
    if !family_ok {
        return Err(validation_error(format!(
            "server.network.{family}_gateway_invalid"
        )));
    }

    let Some(cidr) = cidr.map(str::trim).filter(|c| !c.is_empty()) else {
        return Err(validation_error(format!(
            "server.network.{family}_gateway_requires_cidr"
        )));
    };

    let network = ipnetwork::IpNetwork::from_str(cidr)
        .map_err(|_| validation_error(format!("server.network.{family}_cidr_invalid")))?;
    if !network.contains(parsed) {
        return Err(validation_error(format!(
            "server.network.{family}_gateway_not_in_cidr"
        )));
    }

    Ok(())
}

pub async fn validate_network_in_room<'e, E>(
    executor: E,
    room_id: Uuid,
    network_id: Option<Uuid>,
) -> Result<(), crate::error::AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let Some(nid) = network_id else {
        return Ok(());
    };

    let network_in_room: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM room_networks WHERE room_id = $1 AND network_id = $2)",
    )
    .bind(room_id)
    .bind(nid)
    .fetch_one(executor)
    .await?;

    if !network_in_room {
        return Err(crate::error::AppError::Validation(msg(
            "server.network.not_in_room",
        )));
    }

    Ok(())
}

pub async fn get_room_id_by_workstation<'e, E>(
    executor: E,
    workstation_id: Uuid,
) -> Result<Option<Uuid>, crate::error::AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let room_id: Option<Uuid> =
        sqlx::query_scalar("SELECT room_id FROM workstations WHERE id = $1")
            .bind(workstation_id)
            .fetch_optional(executor)
            .await?
            .flatten();

    Ok(room_id)
}

pub async fn get_room_id_by_position<'e, E>(
    executor: E,
    position_id: Uuid,
) -> Result<Option<Uuid>, crate::error::AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let room_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT c.room_id FROM positions p LEFT JOIN cabinets c ON p.cabinet_id = c.id WHERE p.id = $1",
    )
    .bind(position_id)
    .fetch_optional(executor)
    .await?
    .flatten();

    Ok(room_id)
}

// ==================== Token 管理 ====================

#[must_use]
pub fn generate_token_hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token);
    let hash = hasher.finalize();
    let hash_bytes: &[u8] = hash.as_ref();
    encode(hash_bytes)
}

pub async fn is_token_revoked(pool: &sqlx::PgPool, token: &str) -> Result<bool, sqlx::Error> {
    let token_hash = generate_token_hash(token);

    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM revoked_tokens WHERE token_hash = $1 AND expiry > NOW()",
    )
    .bind(&token_hash)
    .fetch_one(pool)
    .await?;

    Ok(count > 0)
}

pub async fn revoke_token(
    pool: &sqlx::PgPool,
    token: &str,
    user_id: Option<Uuid>,
    expiry: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    let token_hash = generate_token_hash(token);

    // 检查用户是否存在，不存在则使用 NULL
    let valid_user_id = if let Some(uid) = user_id {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(uid)
            .fetch_one(pool)
            .await?;
        if exists { Some(uid) } else { None }
    } else {
        None
    };

    sqlx::query("INSERT INTO revoked_tokens (token_hash, user_id, expiry) VALUES ($1, $2, $3)")
        .bind(&token_hash)
        .bind(valid_user_id)
        .bind(expiry)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn cleanup_expired_revoked_tokens(pool: &sqlx::PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM revoked_tokens WHERE expiry < NOW()")
        .execute(pool)
        .await?;

    let deleted_count = result.rows_affected();
    if deleted_count > 0 {
        // 仅供 token_cleanup 定时任务调用，例行日志降为 debug 避免刷屏
        log_debug!("log.token.revoked_cleaned", count = deleted_count);
    }

    Ok(deleted_count)
}

pub async fn cleanup_old_token_usage(
    pool: &sqlx::PgPool,
    days_to_keep: i32,
) -> Result<u64, sqlx::Error> {
    let result =
        sqlx::query("DELETE FROM token_usage WHERE created_at < NOW() - INTERVAL '1 day' * $1")
            .bind(days_to_keep)
            .execute(pool)
            .await?;

    let deleted_count = result.rows_affected();
    if deleted_count > 0 {
        log_info!(
            "log.token.usage_cleaned",
            count = deleted_count,
            days = days_to_keep
        );
    }

    Ok(deleted_count)
}

// ==================== 请求元信息提取器 ====================

/// 请求元信息：从请求 parts 中提取 IP、语言、User-Agent、JWT claims、是否 HTTPS
/// 用于操作日志记录、语言检测、IP 提取等场景
#[derive(Clone, Debug, Default)]
pub struct RequestMeta {
    pub ip_address: String,
    pub user_lang: String,
    pub user_agent: String,
    pub is_secure: bool,
    pub claims: Option<crate::auth::utils::JwtClaims>,
}

impl RequestMeta {
    /// 从 JWT claims 解析用户 ID
    #[must_use]
    pub fn user_id(&self) -> Option<Uuid> {
        self.claims
            .as_ref()
            .and_then(|c| Uuid::parse_str(&c.sub).ok())
    }
}

impl<S: Send + Sync> FromRequestParts<S> for RequestMeta {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(RequestMeta {
            ip_address: get_real_ip_from_parts(parts),
            user_lang: detect_user_language_from_parts(parts),
            user_agent: get_user_agent_from_parts(parts),
            is_secure: is_secure_from_parts(parts),
            claims: parts
                .extensions
                .get::<crate::auth::utils::JwtClaims>()
                .cloned(),
        })
    }
}

// ==================== 操作日志 ====================

pub struct OperationLogParams<'a> {
    pub ip_address: &'a str,
    pub user_id: Option<Uuid>,
    pub action: &'a str,
    pub resource_type: &'a str,
    pub resource_id: Option<&'a Uuid>,
    pub details: &'a serde_json::Value,
    pub result: bool,
}

pub async fn log_system_operation(
    pool: &sqlx::PgPool,
    params: OperationLogParams<'_>,
) -> Result<(), sqlx::Error> {
    // 检查用户是否存在，不存在则使用 NULL
    let valid_user_id = if let Some(uid) = params.user_id {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(uid)
            .fetch_one(pool)
            .await?;
        if exists { Some(uid) } else { None }
    } else {
        None
    };

    sqlx::query(r"INSERT INTO operation_logs (id, user_id, action, resource_type, resource_id, details, result, ip_address, created_at) 
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)")
        .bind(Uuid::new_v4())
        .bind(valid_user_id)
        .bind(params.action)
        .bind(params.resource_type)
        .bind(params.resource_id)
        .bind(params.details)
        .bind(params.result)
        .bind(params.ip_address)
        .bind(chrono::Utc::now())
        .execute(pool)
        .await?;
    Ok(())
}

/// 尽力而为地记录操作日志：失败时仅打印警告，不影响主流程。
///
/// 用于替代各 handler 中重复的 `if let Err(e) = log_system_operation(...).await { warn!(...) }` 样板。
/// 所有调用点均使用 `result: true`（失败路径由各 handler 自行返回错误）。
pub async fn log_op_best_effort(
    pool: &sqlx::PgPool,
    meta: &RequestMeta,
    action: &str,
    resource_type: &str,
    resource_id: Option<&Uuid>,
    details: &serde_json::Value,
) {
    if let Err(e) = log_system_operation(
        pool,
        OperationLogParams {
            ip_address: &meta.ip_address,
            user_id: meta.user_id(),
            action,
            resource_type,
            resource_id,
            details,
            result: true,
        },
    )
    .await
    {
        log_warn!("log.operation.record_failed", error = e);
    }

    // 审计外发（syslog）：旁路尽力而为，未启用时内部直接跳过
    let forward_message = format!(
        "op action={action} resource={resource_type} ip={} user={} details={details}",
        meta.ip_address,
        meta.user_id()
            .map(|u| u.to_string())
            .unwrap_or_else(|| "-".to_string()),
    );
    crate::log::forwarding::spawn_forward(pool.clone(), forward_message);
}

// ==================== HTTP 请求处理 ====================

/// 从 axum 请求 parts 中获取真实客户端 IP
///
/// 优先级：
/// 1. 若 peer 是可信代理（或 UDS 无 peer 信息，默认视为可信），使用 X-Real-IP
///    （nginx 用 `proxy_set_header X-Real-IP $remote_addr;` 覆盖式设置，
///    客户端无法伪造）
/// 2. 否则使用 peer IP（TCP）或 "unknown"（UDS 无转发头）
///
/// 安全要点：不信任 X-Forwarded-For——nginx 用 `$proxy_add_x_forwarded_for`
/// 时其首段是客户端可伪造的值，若在此采信，攻击者可按请求轮换伪造 IP
/// 绕过限流与 fail2ban（见 security-review I-1/A-1）。
#[must_use]
pub fn get_real_ip_from_parts(parts: &Parts) -> String {
    // 从 extensions 获取 ConnectInfo（TCP 监听时可用）
    let peer_info: Option<std::net::IpAddr> = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip());

    // UDS 场景无 peer IP，默认视为可信代理（即位于 nginx 之后）
    let peer_trusted = peer_info.map(|ip| is_trusted_proxy(&ip)).unwrap_or(true);

    if peer_trusted
        && let Some(x_real_ip) = parts.headers.get("X-Real-IP")
        && let Ok(real_ip_str) = x_real_ip.to_str()
        && !real_ip_str.trim().is_empty()
    {
        return normalize_ipv4_address(real_ip_str.trim());
    }

    let ip = peer_info
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    normalize_ipv4_address(&ip)
}

/// 从 axum 请求 parts 中检测用户语言
#[must_use]
pub fn detect_user_language_from_parts(parts: &Parts) -> String {
    if let Some(accept_language) = parts.headers.get("Accept-Language")
        && let Ok(accept_language_str) = accept_language.to_str()
        && let Some(lang) = accept_language_str.split(',').next()
    {
        let lang_code = lang.split('-').next().unwrap_or("").trim();
        if lang_code == "zh" || lang_code == "en" {
            return lang_code.to_string();
        }
    }

    "zh".to_string()
}

/// login_logs.user_agent / token_usage.user_agent 列宽（VARCHAR(255)）
const USER_AGENT_MAX_CHARS: usize = 255;

/// 从 axum 请求 parts 中获取 User-Agent
///
/// 按字符数截断到 255 以内（对齐 login_logs/token_usage 的 VARCHAR(255)，
/// 防止超长 UA 直写数据库报错丢日志；按 chars 截断避免切断多字节字符）
#[must_use]
pub fn get_user_agent_from_parts(parts: &Parts) -> String {
    let ua = parts
        .headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown");
    if ua.chars().count() > USER_AGENT_MAX_CHARS {
        ua.chars().take(USER_AGENT_MAX_CHARS).collect()
    } else {
        ua.to_string()
    }
}

/// 判断请求是否为 HTTPS（基于 X-Forwarded-Proto 头，适用于反代后的 UDS 部署）
#[must_use]
pub fn is_secure_from_parts(parts: &Parts) -> bool {
    parts
        .headers
        .get("X-Forwarded-Proto")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("https"))
        .unwrap_or(false)
}

fn is_trusted_proxy(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private(),
        std::net::IpAddr::V6(v6) => v6.is_loopback() || is_ipv6_ula(v6),
    }
}

fn is_ipv6_ula(v6: &std::net::Ipv6Addr) -> bool {
    let segments = v6.segments();
    (segments[0] & 0xfe00) == 0xfc00
}

// ==================== 通知与告警 ====================

/// 组装站内通知内容：以 JSON 形式存储「消息 key + 动态参数」，
/// 前端展示时解析并按用户语言翻译；历史遗留的纯文本内容原样展示。
fn encode_notification_content(key: &str, params: &[(&str, &str)]) -> String {
    let params: std::collections::HashMap<String, String> = params
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    serde_json::json!({ "key": key, "params": params }).to_string()
}

pub async fn send_mac_change_notification(
    pool: &sqlx::PgPool,
    workstation_id: &Uuid,
    ip_address: &str,
    old_mac: &str,
    new_mac: &str,
) -> Result<(), sqlx::Error> {
    let workstation_name =
        match sqlx::query_scalar::<_, String>("SELECT name FROM workstations WHERE id = $1")
            .bind(workstation_id)
            .fetch_optional(pool)
            .await
        {
            Ok(Some(name)) => name,
            Ok(None) => {
                log_warn!("log.workstation.not_found", id = workstation_id);
                workstation_id.to_string()
            }
            Err(e) => {
                log_warn!("log.workstation.query_name_failed", error = e);
                workstation_id.to_string()
            }
        };

    let content = encode_notification_content(
        "server.notification.mac_change.body",
        &[
            ("workstation", &workstation_name),
            ("ip", ip_address),
            ("old_mac", old_mac),
            ("new_mac", new_mac),
        ],
    );
    // 通知目标：所有启用状态的管理员（admin/secadmin）按人各发一条，
    // 已读状态随用户独立；此前统一写 user_id = NULL 的行不匹配任何人的
    // 查询条件（user_id = $1），通知列表对所有用户恒为空
    let admin_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM users WHERE status = TRUE AND role IN ('admin', 'secadmin')",
    )
    .fetch_all(pool)
    .await?;

    for admin_id in &admin_ids {
        crate::log::notification::create_notification(
            pool,
            "server.notification.mac_change.title",
            &content,
            "mac_change",
            Some(admin_id),
        )
        .await?;
    }
    log_info!(
        "log.mac_change.notification_created",
        workstation = workstation_name,
        ip = ip_address,
        recipients = admin_ids.len()
    );

    match crate::system::smtp::send_mac_change_email(
        pool,
        &workstation_name,
        ip_address,
        old_mac,
        new_mac,
    )
    .await
    {
        Ok(()) => {
            log_info!("log.mac_change.email_sent", workstation = workstation_name)
        }
        // SMTP 未配置属预期情形，降级为告警日志
        Err(AppError::NotFound(m)) => {
            log_warn!(
                "log.mac_change.email_skipped",
                workstation = workstation_name,
                reason = m.key()
            );
        }
        Err(e) => {
            log_error!(
                "log.mac_change.email_send_failed",
                workstation = workstation_name,
                error = e
            );
        }
    }

    Ok(())
}

// ==================== 网络查询工具 ====================

pub const NETWORK_QUERY: &str = r"
    SELECT n.id, n.name, n.network_region_id, nt.name as network_region, 
           n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, 
           host(n.ipv4_gateway), host(n.ipv6_gateway), 
           (SELECT json_agg(host(d)) FROM unnest(n.ipv4_dns) AS d) as ipv4_dns, 
           (SELECT json_agg(host(d)) FROM unnest(n.ipv6_dns) AS d) as ipv6_dns, 
           n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
    FROM network_cidrs n 
    JOIN network_regions nt ON n.network_region_id = nt.id 
    WHERE n.id = $1
";

pub fn parse_network_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<crate::models::Network, crate::error::AppError> {
    use sqlx::Row;

    Ok(crate::models::Network {
        id: row.get(0),
        name: row.get(1),
        network_region_id: row.get(2),
        network_region: row.get(3),
        ipv4_cidr: row.get(4),
        ipv6_cidr: row.get(5),
        ipv4_gateway: row.get(6),
        ipv6_gateway: row.get(7),
        ipv4_dns: row
            .get::<Option<serde_json::Value>, _>(8)
            .map(|v| {
                serde_json::from_value(v).map_err(|e| {
                    crate::error::AppError::Internal(
                        msg("server.common.deserialize_failed").with("error", e),
                    )
                })
            })
            .transpose()?,
        ipv6_dns: row
            .get::<Option<serde_json::Value>, _>(9)
            .map(|v| {
                serde_json::from_value(v).map_err(|e| {
                    crate::error::AppError::Internal(
                        msg("server.common.deserialize_failed").with("error", e),
                    )
                })
            })
            .transpose()?,
        description: row.get(10),
        created_at: row.get(11),
        updated_at: row.get(12),
    })
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    // ---------- IP 归一化 ----------

    #[test]
    fn test_normalize_ipv4_address_strips_v4_mapped_prefix() {
        // IPv4 映射的 IPv6 地址去除 ::ffff: 前缀
        assert_eq!(normalize_ipv4_address("::ffff:192.168.1.1"), "192.168.1.1");
        assert_eq!(normalize_ipv4_address("::ffff:8.8.8.8"), "8.8.8.8");
    }

    #[test]
    fn test_normalize_ipv4_address_keeps_others() {
        // 普通 IPv4 / 原生 IPv6 原样返回
        assert_eq!(normalize_ipv4_address("10.0.0.1"), "10.0.0.1");
        assert_eq!(normalize_ipv4_address("2001:db8::1"), "2001:db8::1");
        assert_eq!(normalize_ipv4_address("::1"), "::1");
        assert_eq!(normalize_ipv4_address(""), "");
    }

    // ---------- ILIKE 转义 ----------

    #[test]
    fn test_escape_like_plain_text() {
        // 普通文本仅包裹 % 通配符
        assert_eq!(escape_like("交换机"), "%交换机%");
        assert_eq!(escape_like(""), "%%");
    }

    #[test]
    fn test_escape_like_special_characters() {
        // 反斜杠 / % / _ 依次转义，防止被当作通配符
        assert_eq!(escape_like(r"a\b"), r"%a\\b%");
        assert_eq!(escape_like("50%"), r"%50\%%");
        assert_eq!(escape_like("ip_v6"), r"%ip\_v6%");
        // 混合场景：\ 先转义为 \\，随后 % 与 _ 再转义
        assert_eq!(escape_like(r"100_\a"), r"%100\_\\a%");
    }

    #[test]
    fn test_escape_like_backslash_before_percent() {
        // 转义顺序：\ 先被转义为 \\，% 再被转义为 \%
        assert_eq!(escape_like(r"\%"), r"%\\\%%");
    }

    // ---------- CIDR 工具 ----------

    #[test]
    fn test_validate_cidr_valid() {
        // 主机位全为 0 的网络地址合法
        assert!(validate_cidr("192.168.1.0/24"));
        assert!(validate_cidr("10.0.0.0/8"));
        assert!(validate_cidr("0.0.0.0/0"));
        assert!(validate_cidr("2001:db8::/32"));
        assert!(validate_cidr("fe80::/10"));
    }

    #[test]
    fn test_validate_cidr_host_bits_rejected() {
        // 主机位非 0（PostgreSQL CIDR 语义不允许）拒绝
        assert!(!validate_cidr("192.168.1.1/24"));
        assert!(!validate_cidr("2001:db8::1/32"));
    }

    #[test]
    fn test_validate_cidr_invalid_input() {
        // 非法格式 / 掩码越界拒绝
        for bad in ["not-a-cidr", "192.168.1.0/33", "10.0.0.0/-1", ""] {
            assert!(!validate_cidr(bad), "{bad} 应被判定为非法 CIDR");
        }
    }

    #[test]
    fn test_validate_cidr_bare_ip_treated_as_host_prefix() {
        // 特征测试：ipnetwork 将无掩码的裸 IP 解析为 /32，
        // 因此 validate_cidr("192.168.1.0") 视为合法的 32 位前缀 CIDR
        assert!(validate_cidr("192.168.1.0"));
        assert!(validate_cidr("2001:db8::1"));
    }

    #[test]
    fn test_get_cidr_type() {
        assert_eq!(get_cidr_type("10.0.0.0/8"), Some("ipv4"));
        assert_eq!(get_cidr_type("2001:db8::/32"), Some("ipv6"));
        assert_eq!(get_cidr_type("bad-cidr"), None);
    }

    #[test]
    fn test_cidr_contains_subnet_valid() {
        // 子网前缀更长且网络地址落在父网内 → 包含
        assert!(cidr_contains_subnet("192.168.1.0/26", "192.168.1.0/24"));
        assert!(cidr_contains_subnet("10.1.2.0/28", "10.0.0.0/8"));
        assert!(cidr_contains_subnet("2001:db8:1::/48", "2001:db8::/32"));
    }

    #[test]
    fn test_cidr_contains_subnet_invalid() {
        // 相同前缀不构成包含关系
        assert!(!cidr_contains_subnet("192.168.1.0/24", "192.168.1.0/24"));
        // 前缀更短（超网）不包含
        assert!(!cidr_contains_subnet("192.168.0.0/16", "192.168.1.0/24"));
        // 网络地址不在父网内
        assert!(!cidr_contains_subnet("10.2.0.0/16", "192.168.0.0/16"));
        // IPv4 与 IPv6 互不包含
        assert!(!cidr_contains_subnet("10.0.0.0/8", "::/0"));
        // 非法输入
        assert!(!cidr_contains_subnet("bad", "10.0.0.0/8"));
        assert!(!cidr_contains_subnet("10.0.0.0/8", "bad"));
    }

    #[test]
    fn test_cidr_belongs_to_region() {
        let region: Vec<String> = vec!["10.0.0.0/8".to_string(), "172.16.0.0/12".to_string()];
        assert!(cidr_belongs_to_region("10.1.0.0/16", &region));
        assert!(cidr_belongs_to_region("172.16.5.0/24", &region));
        assert!(!cidr_belongs_to_region("192.168.1.0/24", &region));
        // 区域无 CIDR 时不做限制
        assert!(cidr_belongs_to_region("192.168.1.0/24", &[]));
    }

    // ---------- 网关校验 ----------

    /// 从 AppError::Validation 中取出消息 key，便于断言
    fn validation_key(result: Result<(), crate::error::AppError>) -> String {
        match result {
            Err(crate::error::AppError::Validation(m)) => m.key().to_string(),
            Err(other) => panic!("应为 Validation 错误，实际 {other}"),
            Ok(()) => panic!("应为 Err，实际 Ok"),
        }
    }

    #[test]
    fn test_validate_gateway_in_cidr_none_gateway_passes() {
        // 网关缺失 / 空白直接通过
        assert!(validate_gateway_in_cidr(None, None, "ipv4").is_ok());
        assert!(validate_gateway_in_cidr(Some("  "), Some("10.0.0.0/8"), "ipv4").is_ok());
    }

    #[test]
    fn test_validate_gateway_in_cidr_valid() {
        assert!(validate_gateway_in_cidr(Some("10.1.0.254"), Some("10.1.0.0/24"), "ipv4").is_ok());
        // PostgreSQL INET 文本自带的 /32 后缀被容忍
        assert!(
            validate_gateway_in_cidr(Some("10.1.0.254/32"), Some("10.1.0.0/24"), "ipv4").is_ok()
        );
        // IPv6 网关落在 IPv6 网段内
        assert!(
            validate_gateway_in_cidr(Some("2001:db8::1"), Some("2001:db8::/32"), "ipv6").is_ok()
        );
    }

    #[test]
    fn test_validate_gateway_in_cidr_format_and_family_errors() {
        // 网关格式非法
        assert_eq!(
            validation_key(validate_gateway_in_cidr(Some("bad-ip"), None, "ipv4")),
            "server.network.ipv4_gateway_invalid"
        );
        // 地址族不匹配：IPv4 网关配 ipv6 家族
        assert_eq!(
            validation_key(validate_gateway_in_cidr(
                Some("10.0.0.1"),
                Some("2001:db8::/32"),
                "ipv6"
            )),
            "server.network.ipv6_gateway_invalid"
        );
        // IPv6 网关配 ipv4 家族
        assert_eq!(
            validation_key(validate_gateway_in_cidr(
                Some("2001:db8::1"),
                Some("10.0.0.0/8"),
                "ipv4"
            )),
            "server.network.ipv4_gateway_invalid"
        );
    }

    #[test]
    fn test_validate_gateway_in_cidr_requires_cidr() {
        // 提供了网关但未提供 CIDR → 报需要 CIDR
        assert_eq!(
            validation_key(validate_gateway_in_cidr(Some("10.0.0.1"), None, "ipv4")),
            "server.network.ipv4_gateway_requires_cidr"
        );
        // CIDR 为空白同样视为未提供
        assert_eq!(
            validation_key(validate_gateway_in_cidr(
                Some("2001:db8::1"),
                Some(" "),
                "ipv6"
            )),
            "server.network.ipv6_gateway_requires_cidr"
        );
    }

    #[test]
    fn test_validate_gateway_in_cidr_not_in_cidr() {
        // 网关不在网段内 / CIDR 非法
        assert_eq!(
            validation_key(validate_gateway_in_cidr(
                Some("192.168.1.1"),
                Some("10.0.0.0/8"),
                "ipv4"
            )),
            "server.network.ipv4_gateway_not_in_cidr"
        );
        assert_eq!(
            validation_key(validate_gateway_in_cidr(
                Some("10.0.0.1"),
                Some("bad-cidr"),
                "ipv4"
            )),
            "server.network.ipv4_cidr_invalid"
        );
    }

    // ---------- Token 哈希 ----------

    #[test]
    fn test_generate_token_hash_known_vector() {
        // SHA-256("abc") 的标准十六进制结果
        assert_eq!(
            generate_token_hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_generate_token_hash_properties() {
        // 输出为 64 位十六进制；确定性；不同输入不同哈希
        let h1 = generate_token_hash("token-A");
        assert_eq!(h1.len(), 64);
        assert_eq!(h1, generate_token_hash("token-A"));
        assert_ne!(h1, generate_token_hash("token-B"));
        // 空串也有稳定输出
        assert_eq!(
            generate_token_hash(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    // ---------- 请求元信息（Parts 级纯函数） ----------

    /// 构造带请求头的 Parts（无 extensions）
    fn parts_with_headers(headers: &[(&str, &str)]) -> Parts {
        let mut builder = axum::http::Request::builder();
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let Ok(req) = builder.body(()) else {
            panic!("构造测试请求失败");
        };
        let (parts, _payload) = req.into_parts();
        parts
    }

    /// 构造带 ConnectInfo 扩展的 Parts
    fn parts_with_peer(peer: IpAddr, headers: &[(&str, &str)]) -> Parts {
        let mut parts = parts_with_headers(headers);
        parts
            .extensions
            .insert(ConnectInfo(SocketAddr::new(peer, 8080)));
        parts
    }

    #[test]
    fn test_get_real_ip_prefers_x_real_ip_without_peer() {
        // 无 peer 信息（UDS 场景默认可信代理）：优先 X-Real-IP
        let parts = parts_with_headers(&[("X-Real-IP", "1.2.3.4")]);
        assert_eq!(get_real_ip_from_parts(&parts), "1.2.3.4");
    }

    #[test]
    fn test_get_real_ip_ignores_x_forwarded_for() {
        // X-Forwarded-For 不再作为回退来源（首段可伪造，见 I-1/A-1）
        let parts = parts_with_headers(&[("X-Forwarded-For", "5.6.7.8, 10.0.0.1")]);
        assert_eq!(get_real_ip_from_parts(&parts), "unknown");

        // 存在 X-Real-IP 时优先采用，忽略 X-Forwarded-For
        let both = parts_with_headers(&[("X-Real-IP", "1.2.3.4"), ("X-Forwarded-For", "5.6.7.8")]);
        assert_eq!(get_real_ip_from_parts(&both), "1.2.3.4");
    }

    #[test]
    fn test_get_real_ip_unknown_without_any_source() {
        // 无任何来源时返回 "unknown"
        let parts = parts_with_headers(&[]);
        assert_eq!(get_real_ip_from_parts(&parts), "unknown");
    }

    #[test]
    fn test_get_real_ip_normalizes_v4_mapped_header() {
        // 头中的 IPv4 映射地址同样被归一化
        let parts = parts_with_headers(&[("X-Real-IP", "::ffff:9.9.9.9")]);
        assert_eq!(get_real_ip_from_parts(&parts), "9.9.9.9");
    }

    #[test]
    fn test_get_real_ip_untrusted_peer_ignores_headers() {
        // 公网直连（不可信 peer）时忽略转发头，直接使用 peer IP
        let parts = parts_with_peer(
            IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
            &[("X-Real-IP", "1.2.3.4"), ("X-Forwarded-For", "5.6.7.8")],
        );
        assert_eq!(get_real_ip_from_parts(&parts), "8.8.8.8");
    }

    #[test]
    fn test_get_real_ip_trusted_peer_uses_x_real_ip() {
        // 内网 / 回环 peer 视为可信代理，优先 X-Real-IP
        let private = parts_with_peer(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
            &[("X-Real-IP", "1.2.3.4")],
        );
        assert_eq!(get_real_ip_from_parts(&private), "1.2.3.4");

        // 可信 peer 但仅有可伪造的 X-Forwarded-For 时不再采信，回退 peer IP
        let loopback = parts_with_peer(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            &[("X-Forwarded-For", "5.6.7.8")],
        );
        assert_eq!(get_real_ip_from_parts(&loopback), "127.0.0.1");
    }

    #[test]
    fn test_detect_user_language() {
        // 取首语言的主子标签；仅支持 zh / en，其余回退 zh
        let zh = parts_with_headers(&[("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")]);
        assert_eq!(detect_user_language_from_parts(&zh), "zh");

        let en = parts_with_headers(&[("Accept-Language", "en-US,en;q=0.9")]);
        assert_eq!(detect_user_language_from_parts(&en), "en");

        // 不支持的语言回退默认 zh
        let fr = parts_with_headers(&[("Accept-Language", "fr-FR,fr;q=0.9")]);
        assert_eq!(detect_user_language_from_parts(&fr), "zh");

        // 头缺失回退 zh
        assert_eq!(
            detect_user_language_from_parts(&parts_with_headers(&[])),
            "zh"
        );
    }

    #[test]
    fn test_get_user_agent() {
        let with_ua = parts_with_headers(&[("User-Agent", "Mozilla/5.0 (X11; Linux)")]);
        assert_eq!(
            get_user_agent_from_parts(&with_ua),
            "Mozilla/5.0 (X11; Linux)"
        );

        // 头缺失返回 unknown
        assert_eq!(
            get_user_agent_from_parts(&parts_with_headers(&[])),
            "unknown"
        );
    }

    #[test]
    fn test_get_user_agent_truncated_to_column_limit() {
        // 超长 UA 按字符截断到 255（对齐 VARCHAR(255) 列宽）
        let long_ua = "U".repeat(300);
        let parts = parts_with_headers(&[("User-Agent", long_ua.as_str())]);
        let ua = get_user_agent_from_parts(&parts);
        assert_eq!(ua.chars().count(), 255);

        // 未超长时原样保留
        let short = parts_with_headers(&[("User-Agent", "curl/8.0")]);
        assert_eq!(get_user_agent_from_parts(&short), "curl/8.0");
    }

    #[test]
    fn test_is_secure_from_parts() {
        // 仅 https（大小写不敏感）判定为安全连接
        let https = parts_with_headers(&[("X-Forwarded-Proto", "https")]);
        assert!(is_secure_from_parts(&https));

        let upper = parts_with_headers(&[("X-Forwarded-Proto", "HTTPS")]);
        assert!(is_secure_from_parts(&upper));

        let http = parts_with_headers(&[("X-Forwarded-Proto", "http")]);
        assert!(!is_secure_from_parts(&http));

        assert!(!is_secure_from_parts(&parts_with_headers(&[])));
    }

    // ---------- 可信代理判定（私有辅助函数） ----------

    #[test]
    fn test_is_trusted_proxy() {
        // 回环与内网 IPv4 可信
        assert!(is_trusted_proxy(&IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_trusted_proxy(&IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3))));
        assert!(is_trusted_proxy(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_trusted_proxy(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        // 公网地址不可信
        assert!(!is_trusted_proxy(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        // IPv6 回环与 ULA 可信，公网 IPv6 不可信
        assert!(is_trusted_proxy(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(is_trusted_proxy(&IpAddr::V6(Ipv6Addr::new(
            0xfd00, 0, 0, 0, 0, 0, 0, 1
        ))));
        assert!(!is_trusted_proxy(&IpAddr::V6(Ipv6Addr::new(
            0x2001, 0xdb8, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn test_is_ipv6_ula() {
        // fc00::/7（fd.. 与 fc.. 开头）为 ULA
        assert!(is_ipv6_ula(&Ipv6Addr::new(0xfd12, 0, 0, 0, 0, 0, 0, 1)));
        assert!(is_ipv6_ula(&Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 1)));
        // fe80（链路本地）、2001（公网）不属于 ULA
        assert!(!is_ipv6_ula(&Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)));
        assert!(!is_ipv6_ula(&Ipv6Addr::new(
            0x2001, 0xdb8, 0, 0, 0, 0, 0, 1
        )));
    }

    // ---------- RequestMeta::user_id ----------

    /// 构造 JwtClaims 的测试辅助函数
    fn claims_with_sub(sub: &str) -> crate::auth::utils::JwtClaims {
        crate::auth::utils::JwtClaims {
            sub: sub.to_string(),
            username: "tester".to_string(),
            role: "admin".to_string(),
            exp: 4_102_444_800,
            iat: 1_700_000_000,
            iss: "ipma".to_string(),
            jti: "test-jti".to_string(),
            aud: "ipma-web".to_string(),
            token_type: "access".to_string(),
            device_fingerprint: None,
            ip_address: None,
        }
    }

    #[test]
    fn test_request_meta_user_id_from_claims() {
        // 合法 UUID 的 sub 可解析出用户 ID
        let uid = Uuid::new_v4();
        let meta = RequestMeta {
            claims: Some(claims_with_sub(&uid.to_string())),
            ..RequestMeta::default()
        };
        assert_eq!(meta.user_id(), Some(uid));
    }

    #[test]
    fn test_request_meta_user_id_invalid_or_missing() {
        // sub 非法或缺失 claims 时返回 None
        let meta_bad = RequestMeta {
            claims: Some(claims_with_sub("not-a-uuid")),
            ..RequestMeta::default()
        };
        assert_eq!(meta_bad.user_id(), None);

        let meta_none = RequestMeta::default();
        assert_eq!(meta_none.user_id(), None);
        assert_eq!(meta_none.ip_address, "");
        assert_eq!(meta_none.user_lang, "");
    }

    // ---------- 通知内容编码（私有辅助函数） ----------

    #[test]
    fn test_encode_notification_content() {
        let content = encode_notification_content(
            "server.notification.mac_change.body",
            &[("ip", "10.0.0.1"), ("old_mac", "aa:aa")],
        );
        let value: serde_json::Value =
            serde_json::from_str(&content).unwrap_or_else(|e| panic!("应为合法 JSON: {e}"));
        assert_eq!(value["key"], "server.notification.mac_change.body");
        assert_eq!(value["params"]["ip"], "10.0.0.1");
        assert_eq!(value["params"]["old_mac"], "aa:aa");
    }

    #[test]
    fn test_encode_notification_content_empty_params() {
        // 无参数时 params 为空对象
        let content = encode_notification_content("some.key", &[]);
        let value: serde_json::Value =
            serde_json::from_str(&content).unwrap_or_else(|e| panic!("应为合法 JSON: {e}"));
        assert_eq!(value["key"], "some.key");
        assert_eq!(
            value["params"].as_object().map(serde_json::Map::len),
            Some(0)
        );
    }
}
