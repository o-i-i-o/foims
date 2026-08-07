use std::str::FromStr;
use tracing::{error, info, warn};
use uuid::Uuid;

use axum::extract::ConnectInfo;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::net::SocketAddr;

use hex::encode;

#[must_use]
pub fn normalize_ipv4_address(ip: &str) -> String {
    if ip.starts_with("::ffff:") {
        ip.strip_prefix("::ffff:").unwrap_or(ip).to_string()
    } else {
        ip.to_string()
    }
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
        return Err(crate::error::AppError::Validation(
            "所选网段不属于该房间的可用网段".to_string(),
        ));
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
        info!("Cleaned up {} expired revoked tokens", deleted_count);
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
        info!(
            "Cleaned up {} old token usage records (older than {} days)",
            deleted_count, days_to_keep
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

// ==================== HTTP 请求处理 ====================

/// 从 axum 请求 parts 中获取真实客户端 IP
///
/// 优先级：
/// 1. 若 peer 是可信代理（或 UDS 无 peer 信息，默认视为可信），使用 X-Forwarded-For
/// 2. 其次使用 X-Real-IP
/// 3. 否则使用 peer IP（TCP）或 "unknown"（UDS 无转发头）
#[must_use]
pub fn get_real_ip_from_parts(parts: &Parts) -> String {
    // 从 extensions 获取 ConnectInfo（TCP 监听时可用）
    let peer_info: Option<std::net::IpAddr> = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip());

    // UDS 场景无 peer IP，默认视为可信代理（即位于 nginx 之后）
    let peer_trusted = peer_info.map(|ip| is_trusted_proxy(&ip)).unwrap_or(true);

    if peer_trusted {
        if let Some(xff) = parts.headers.get("X-Forwarded-For")
            && let Ok(xff_str) = xff.to_str()
            && let Some(real_ip) = xff_str.split(',').next().map(|s| s.trim().to_string())
            && !real_ip.is_empty()
        {
            return normalize_ipv4_address(&real_ip);
        }

        if let Some(x_real_ip) = parts.headers.get("X-Real-IP")
            && let Ok(real_ip_str) = x_real_ip.to_str()
            && !real_ip_str.trim().is_empty()
        {
            return normalize_ipv4_address(real_ip_str.trim());
        }
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

/// 从 axum 请求 parts 中获取 User-Agent
#[must_use]
pub fn get_user_agent_from_parts(parts: &Parts) -> String {
    parts
        .headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown")
        .to_string()
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
                warn!("未找到工位ID: {}", workstation_id);
                workstation_id.to_string()
            }
            Err(e) => {
                warn!("查询工位名称失败: {}", e);
                workstation_id.to_string()
            }
        };

    let content =
        format!("工位 {workstation_name} {ip_address} 的MAC地址已从 {old_mac} 变更为 {new_mac}");
    crate::log::notification::create_notification(
        pool,
        "MAC地址变更",
        &content,
        "mac_change",
        None,
    )
    .await?;
    info!(
        "MAC地址变更站内通知创建成功: 工位={}, IP={}",
        workstation_name, ip_address
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
        Ok(()) => info!("MAC地址变更邮件通知发送成功: 工位={}", workstation_name),
        Err(e) => {
            let msg = format!("{e}");
            if msg.contains("SMTP配置未设置") {
                warn!(
                    "MAC地址变更邮件通知跳过: 工位={}, 原因: {}",
                    workstation_name, msg
                );
            } else {
                error!(
                    "MAC地址变更邮件通知发送失败: 工位={}, 错误: {}",
                    workstation_name, msg
                );
            }
        }
    }

    Ok(())
}

pub fn log_bilingual(message_key: &str) {
    let zh_message = rust_i18n::t!(message_key, locale = "zh");
    let en_message = rust_i18n::t!(message_key, locale = "en");

    info!("[中文] {}", zh_message);
    info!("[English] {}", en_message);
}

// ==================== 网络查询工具 ====================

pub const NETWORK_QUERY: &str = r"
    SELECT n.id, n.name, n.network_region_id, nt.name as network_region, 
           n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, 
           n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, 
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
                    crate::error::AppError::Internal(format!("IPv4 DNS反序列化失败: {e}"))
                })
            })
            .transpose()?,
        ipv6_dns: row
            .get::<Option<serde_json::Value>, _>(9)
            .map(|v| {
                serde_json::from_value(v).map_err(|e| {
                    crate::error::AppError::Internal(format!("IPv6 DNS反序列化失败: {e}"))
                })
            })
            .transpose()?,
        description: row.get(10),
        created_at: row.get(11),
        updated_at: row.get(12),
    })
}
