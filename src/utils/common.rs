use ipnetwork::IpNetwork;
use macaddr::MacAddr;
use regex;
use std::collections::HashMap;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::Duration;
use tracing::info;
use uuid::Uuid;

use crate::config::Config;
use actix_web::{HttpMessage, HttpRequest};

// ==================== 常量定义 ====================

pub const DEFAULT_PAGE: i64 = 1;
pub const DEFAULT_PAGE_SIZE: i64 = 100;
pub const MAX_PAGE_SIZE: i64 = 1000;

pub const IPV4_CIDR_REGEX: &str = r"^(?:(?:(?:[0-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-5])\.){3}(?:[0-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-5])/(?:[0-9]|[12]?[0-9]|3[0-2]))$";
pub const IPV6_CIDR_REGEX: &str = r"^(?:[0-9a-fA-F:]+/(?:[0-9]|[1-9][0-9]|1[01][0-9]|12[0-8]))$";

lazy_static::lazy_static! {
    static ref IPV4_CIDR_PATTERN: regex::Regex = regex::Regex::new(IPV4_CIDR_REGEX).unwrap();
    static ref IPV6_CIDR_PATTERN: regex::Regex = regex::Regex::new(IPV6_CIDR_REGEX).unwrap();
}

// ==================== IP/MAC 地址验证与格式化 ====================

pub fn validate_ip_address(ip: &str) -> bool {
    ip.parse::<IpNetwork>().is_ok() || ip.parse::<std::net::IpAddr>().is_ok()
}

pub fn validate_mac_address(mac: &str) -> bool {
    mac.parse::<MacAddr>().is_ok()
}

pub fn format_ip_address(ip: &str) -> Option<String> {
    if let Ok(ip_net) = ip.parse::<IpNetwork>() {
        Some(ip_net.to_string())
    } else if let Ok(ip_addr) = ip.parse::<std::net::IpAddr>() {
        Some(ip_addr.to_string())
    } else {
        None
    }
}

pub fn format_mac_address(mac: &str) -> Option<String> {
    mac.parse::<MacAddr>().ok().map(|m| m.to_string())
}

pub fn normalize_ipv4_address(ip: &str) -> String {
    if ip.starts_with("::ffff:") {
        ip.strip_prefix("::ffff:").unwrap_or(ip).to_string()
    } else {
        ip.to_string()
    }
}

// ==================== CIDR 验证 ====================

pub fn validate_cidr(cidr: &str) -> bool {
    if !IPV4_CIDR_PATTERN.is_match(cidr) && !IPV6_CIDR_PATTERN.is_match(cidr) {
        return false;
    }

    ipnetwork::IpNetwork::from_str(cidr).is_ok()
}

pub fn get_cidr_type(cidr: &str) -> Option<&'static str> {
    if IPV4_CIDR_PATTERN.is_match(cidr) {
        Some("ipv4")
    } else if IPV6_CIDR_PATTERN.is_match(cidr) {
        Some("ipv6")
    } else {
        None
    }
}

pub fn validate_ip_in_cidr(ip_address: &str, network: &crate::models::Network) -> Result<bool, actix_web::HttpResponse> {
    let ip_addr = match std::net::IpAddr::from_str(ip_address) {
        Ok(ip) => ip,
        Err(_) => {
            return Err(actix_web::HttpResponse::BadRequest()
                .json(crate::models::ApiResponse::<()>::error("无效的IP地址格式")));
        }
    };

    let is_ipv4 = matches!(ip_addr, std::net::IpAddr::V4(_));
    let mut is_valid = false;

    let cidr_fields = vec![
        if is_ipv4 {
            network.ipv4_cidr.clone().unwrap_or_default()
        } else {
            "".to_string()
        },
        if !is_ipv4 {
            network.ipv6_cidr.clone().unwrap_or_default()
        } else {
            "".to_string()
        },
    ];

    for cidr_str in cidr_fields {
        if cidr_str.is_empty() {
            continue;
        }

        match ipnetwork::IpNetwork::from_str(&cidr_str) {
            Ok(network_cidr) => {
                if network_cidr.contains(ip_addr) {
                    is_valid = true;
                    break;
                }
            }
            Err(_) => {
                continue;
            }
        }
    }

    Ok(is_valid)
}

// ==================== Token 管理 ====================

pub fn generate_token_hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token);
    format!("{:x}", hasher.finalize())
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
    user_id: &Uuid,
    expiry: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    let token_hash = generate_token_hash(token);

    sqlx::query("INSERT INTO revoked_tokens (token_hash, user_id, expiry) VALUES ($1, $2, $3)")
        .bind(&token_hash)
        .bind(user_id)
        .bind(expiry)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn record_token_usage(
    pool: &sqlx::PgPool,
    token: &str,
    user_id: &Uuid,
    ip_address: &str,
    user_agent: &str,
    request_path: &str,
) -> Result<(), sqlx::Error> {
    let token_hash = generate_token_hash(token);

    sqlx::query(
        "INSERT INTO token_usage (token_hash, user_id, ip_address, user_agent, request_path) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(&token_hash)
    .bind(user_id)
    .bind(ip_address)
    .bind(user_agent)
    .bind(request_path)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn check_token_usage_limit(
    pool: &sqlx::PgPool,
    token: &str,
) -> Result<bool, sqlx::Error> {
    let token_hash = generate_token_hash(token);

    let count_1min = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM token_usage WHERE token_hash = $1 AND created_at > NOW() - INTERVAL '1 minute'"
    )
    .bind(&token_hash)
    .fetch_one(pool)
    .await?;

    let count_5min = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM token_usage WHERE token_hash = $1 AND created_at > NOW() - INTERVAL '5 minutes'"
    )
    .bind(&token_hash)
    .fetch_one(pool)
    .await?;

    if count_1min > 30 || count_5min > 100 {
        return Ok(true);
    }

    Ok(false)
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

pub async fn cleanup_old_token_usage(pool: &sqlx::PgPool, days_to_keep: i32) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM token_usage WHERE created_at < NOW() - INTERVAL '1 day' * $1"
    )
    .bind(days_to_keep)
    .execute(pool)
    .await?;
    
    let deleted_count = result.rows_affected();
    if deleted_count > 0 {
        info!("Cleaned up {} old token usage records (older than {} days)", deleted_count, days_to_keep);
    }
    
    Ok(deleted_count)
}

// ==================== 操作日志 ====================

pub struct OperationLogParams<'a> {
    pub user_id: &'a Uuid,
    pub action: &'a str,
    pub resource_type: &'a str,
    pub resource_id: &'a Uuid,
    pub details: &'a serde_json::Value,
    pub result: bool,
    pub ip_address: &'a str,
}

pub async fn log_operation(
    pool: &sqlx::PgPool,
    params: OperationLogParams<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"INSERT INTO operation_logs (id, user_id, action, resource_type, resource_id, details, result, ip_address, created_at) 
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#)
        .bind(Uuid::new_v4())
        .bind(params.user_id)
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

#[allow(clippy::too_many_arguments)]
pub async fn log_system_operation(
    pool: &sqlx::PgPool,
    req: &HttpRequest,
    _config: &Config,
    action: &str,
    resource_type: &str,
    resource_id: &Uuid,
    details: &serde_json::Value,
    result: bool,
) -> Result<(), sqlx::Error> {
    let claims = {
        let extensions = req.extensions();
        let claims_opt = extensions.get::<crate::auth::utils::JwtClaims>();
        claims_opt.cloned()
    };

    if let Some(claims) = claims {
        let user_id = Uuid::parse_str(&claims.sub).unwrap_or(Uuid::nil());
        let ip_address = get_real_ip_from_request(req);

        log_operation(
            pool,
            OperationLogParams {
                user_id: &user_id,
                action,
                resource_type,
                resource_id,
                details,
                result,
                ip_address: &ip_address,
            },
        )
        .await
    } else {
        let ip_address = get_real_ip_from_request(req);
        log_operation(
            pool,
            OperationLogParams {
                user_id: &Uuid::nil(),
                action,
                resource_type,
                resource_id,
                details,
                result,
                ip_address: &ip_address,
            },
        )
        .await
    }
}

// ==================== HTTP 请求处理 ====================

pub fn get_real_ip_from_request(req: &HttpRequest) -> String {
    if let Some(xff) = req.headers().get("X-Forwarded-For")
        && let Ok(xff_str) = xff.to_str()
        && let Some(real_ip) = xff_str.split(',').next().map(|s| s.trim().to_string())
    {
        return normalize_ipv4_address(&real_ip);
    }

    if let Some(x_real_ip) = req.headers().get("X-Real-IP")
        && let Ok(real_ip_str) = x_real_ip.to_str()
    {
        return normalize_ipv4_address(real_ip_str.trim());
    }

    let ip = req.connection_info()
        .realip_remote_addr()
        .unwrap_or("unknown")
        .to_string();
    
    normalize_ipv4_address(&ip)
}

pub fn detect_user_language(req: &HttpRequest) -> String {
    if let Some(accept_language) = req.headers().get("Accept-Language")
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

// ==================== 错误处理 ====================

pub fn handle_db_error<E: std::fmt::Display>(err: E, message: &str) -> actix_web::HttpResponse {
    use tracing::error;
    
    let err_str = err.to_string();
    error!("数据库错误: {}", err_str);
    
    if err_str.contains("invalid cidr value") {
        return actix_web::HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "不符合CIDR格式",
            "data": null
        }));
    }
    
    if err_str.contains("invalid inet value") {
        return actix_web::HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "不符合IP地址格式",
            "data": null
        }));
    }
    
    if err_str.contains("duplicate key") || err_str.contains("unique constraint") {
        return actix_web::HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "数据已存在，请检查是否有重复记录",
            "data": null
        }));
    }
    
    if err_str.contains("foreign key") || err_str.contains("violates foreign key constraint") {
        return actix_web::HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "关联数据不存在或无法删除",
            "data": null
        }));
    }
    
    if err_str.contains("connection") || err_str.contains("timeout") {
        return actix_web::HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": "数据库连接异常，请稍后重试",
            "data": null
        }));
    }
    
    actix_web::HttpResponse::InternalServerError().json(serde_json::json!({
        "success": false,
        "message": message,
        "data": null
    }))
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
            Ok(None) => workstation_id.to_string(),
            Err(_) => workstation_id.to_string(),
        };

    let content = format!(
        "工位 {} {} 的MAC地址已从 {} 变更为 {}",
        workstation_name, ip_address, old_mac, new_mac
    );
    crate::log::notification::create_notification(
        pool,
        "MAC地址变更",
        &content,
        "mac_change",
        None,
    )
    .await?;

    let _ = crate::system::smtp::send_mac_change_email(pool, &workstation_name, ip_address, old_mac, new_mac).await;

    Ok(())
}

pub async fn send_system_alert(
    pool: &sqlx::PgPool,
    title: &str,
    content: &str,
    alert_type: &str,
    user_id: Option<&Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"INSERT INTO notifications (id, user_id, title, content, notification_type, read, created_at) 
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#)
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(title)
        .bind(content)
        .bind(alert_type)
        .bind(false)
        .bind(chrono::Utc::now())
        .execute(pool)
        .await?;
    Ok(())
}

// ==================== MAC 地址获取 ====================

pub fn get_real_mac_address(ip: &str) -> Option<String> {
    if let Some(mac) = read_mac_from_arp_cache(ip) {
        return Some(mac);
    }

    if ip.parse::<IpAddr>().is_ok() {
        let _ = std::process::Command::new("ping")
            .args(["-c", "1", "-W", "1", ip])
            .output();

        if let Some(mac) = read_mac_from_arp_cache(ip) {
            return Some(mac);
        }
    }

    None
}

fn read_mac_from_arp_cache(ip: &str) -> Option<String> {
    let content = std::fs::read_to_string("/proc/net/arp").ok()?;

    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && parts[0] == ip {
            let mac = parts[3].to_uppercase();
            if mac != "00:00:00:00:00:00" && validate_mac_address(&mac) {
                return Some(mac);
            }
        }
    }
    None
}

pub async fn batch_get_mac_addresses(ips: &[String]) -> HashMap<String, Option<String>> {
    let mut results = HashMap::new();

    let arp_cache = read_arp_cache();
    for ip in ips {
        if let Some(mac) = arp_cache.get(ip) {
            results.insert(ip.clone(), Some(mac.clone()));
        }
    }

    let missing_ips: Vec<&String> = ips.iter().filter(|ip| !results.contains_key(*ip)).collect();

    if missing_ips.is_empty() {
        return results;
    }

    let _ = batch_ping(&missing_ips).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    let arp_cache = read_arp_cache();

    for ip in missing_ips {
        let mac = arp_cache.get(ip).cloned();
        results.insert(ip.clone(), mac);
    }

    results
}

fn read_arp_cache() -> HashMap<String, String> {
    let mut cache = HashMap::new();

    if let Ok(content) = std::fs::read_to_string("/proc/net/arp") {
        for line in content.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let ip = parts[0].to_string();
                let mac = parts[3].to_uppercase();
                if mac != "00:00:00:00:00:00" && validate_mac_address(&mac) {
                    cache.insert(ip, mac);
                }
            }
        }
    }

    cache
}

async fn batch_ping(ips: &[&String]) -> Vec<bool> {
    use futures_util::future::join_all;

    let mut handles = Vec::new();

    for ip in ips {
        let ip_clone = ip.to_string();
        let handle = tokio::spawn(async move { ping_ip(&ip_clone).await });
        handles.push(handle);
    }

    let results = join_all(handles).await;
    results.into_iter().map(|r| r.unwrap_or(false)).collect()
}

async fn ping_ip(ip: &str) -> bool {
    use pnet::packet::Packet;
    use pnet::packet::icmp::{IcmpPacket, IcmpTypes, echo_request::MutableEchoRequestPacket};
    use pnet::packet::ip::IpNextHeaderProtocols;
    use pnet::transport::{
        TransportChannelType::Layer4, TransportProtocol::Ipv4, transport_channel,
    };

    let ip_addr: IpAddr = match ip.parse() {
        Ok(addr) => addr,
        Err(_) => return system_ping(ip),
    };

    let ipv4_addr = match ip_addr {
        IpAddr::V4(addr) => addr,
        IpAddr::V6(_) => return system_ping(ip),
    };

    let protocol = Layer4(Ipv4(IpNextHeaderProtocols::Icmp));
    let (mut tx, _rx) = match transport_channel(1024, protocol) {
        Ok((tx, rx)) => (tx, rx),
        Err(_) => {
            return system_ping(ip);
        }
    };

    let mut buffer = [0u8; 64];
    let mut icmp_packet = match MutableEchoRequestPacket::new(&mut buffer) {
        Some(packet) => packet,
        None => return system_ping(ip),
    };

    icmp_packet.set_icmp_type(IcmpTypes::EchoRequest);
    icmp_packet.set_identifier(rand::random());
    icmp_packet.set_sequence_number(1);

    let checksum = {
        let icmp_data = icmp_packet.packet();
        if let Some(icmp_pkt) = IcmpPacket::new(icmp_data) {
            pnet::packet::icmp::checksum(&icmp_pkt)
        } else {
            0
        }
    };
    icmp_packet.set_checksum(checksum);

    if tx.send_to(icmp_packet, IpAddr::V4(ipv4_addr)).is_err() {
        return system_ping(ip);
    }

    tokio::time::sleep(Duration::from_millis(50)).await;

    true
}

fn system_ping(ip: &str) -> bool {
    std::process::Command::new("ping")
        .args(["-c", "1", "-W", "1", ip])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ==================== 日志工具 ====================

pub fn log_bilingual(message_key: &str) {
    rust_i18n::set_locale("zh");
    let zh_message = rust_i18n::t!(message_key);

    rust_i18n::set_locale("en");
    let en_message = rust_i18n::t!(message_key);

    info!("[中文] {}", zh_message);
    info!("[English] {}", en_message);
}
