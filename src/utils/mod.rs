pub mod buffer_pool;

use ipnetwork::IpNetwork;
use macaddr::MacAddr;
use rand::Rng;
use regex;
use std::collections::HashMap;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::Duration;
use tracing::info;
use uuid::Uuid;

// 验证IP地址格式
pub fn validate_ip_address(ip: &str) -> bool {
    ip.parse::<IpNetwork>().is_ok() || ip.parse::<std::net::IpAddr>().is_ok()
}

// 验证MAC地址格式
pub fn validate_mac_address(mac: &str) -> bool {
    mac.parse::<MacAddr>().is_ok()
}

// 验证房间类型
pub fn validate_room_type(room_type: &Option<String>) -> bool {
    match room_type {
        Some(rt) => rt == "office" || rt == "data_center",
        None => true, // None值是有效的，因为它是可选字段
    }
}

// 生成随机密码
pub fn generate_random_password(length: usize) -> String {
    const CHARS: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()_+";
    let mut rng = rand::rng();
    (0..length)
        .map(|_| CHARS[rng.random_range(0..CHARS.len())] as char)
        .collect()
}

// 格式化IP地址
pub fn format_ip_address(ip: &str) -> Option<String> {
    if let Ok(ip_net) = ip.parse::<IpNetwork>() {
        Some(ip_net.to_string())
    } else if let Ok(ip_addr) = ip.parse::<std::net::IpAddr>() {
        Some(ip_addr.to_string())
    } else {
        None
    }
}

// 格式化MAC地址为****-****-****格式
pub fn format_mac_address(mac: &str) -> Option<String> {
    mac.parse::<MacAddr>().ok().map(|m| m.to_string())
}

use crate::config::Config;
use actix_web::{HttpMessage, HttpRequest};

// 操作日志参数结构体
pub struct OperationLogParams<'a> {
    pub user_id: &'a Uuid,
    pub action: &'a str,
    pub resource_type: &'a str,
    pub resource_id: &'a Uuid,
    pub details: &'a serde_json::Value,
    pub result: bool,
    pub ip_address: &'a str,
}

// 生成令牌哈希值，用于存储在撤销表中
pub fn generate_token_hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token);
    format!("{:x}", hasher.finalize())
}

// 检查令牌是否被撤销
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

// 撤销令牌
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

// 记录令牌使用情况
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

// 检查令牌使用频率限制
pub async fn check_token_usage_limit(
    pool: &sqlx::PgPool,
    token: &str,
) -> Result<bool, sqlx::Error> {
    let token_hash = generate_token_hash(token);

    // 检查最近1分钟内的请求次数
    let count_1min = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM token_usage WHERE token_hash = $1 AND created_at > NOW() - INTERVAL '1 minute'"
    )
    .bind(&token_hash)
    .fetch_one(pool)
    .await?;

    // 检查最近5分钟内的请求次数
    let count_5min = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM token_usage WHERE token_hash = $1 AND created_at > NOW() - INTERVAL '5 minutes'"
    )
    .bind(&token_hash)
    .fetch_one(pool)
    .await?;

    // 设置限制：1分钟内最多30次请求，5分钟内最多100次请求
    if count_1min > 30 || count_5min > 100 {
        return Ok(true); // 超过限制
    }

    Ok(false) // 未超过限制
}

// 记录操作日志的辅助函数
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

// 记录系统操作日志的辅助函数（自动从请求中提取用户信息）
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
    // 优先从请求扩展中获取JwtClaims（认证中间件已存储）
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
        //  fallback: 从请求头中提取JwtClaims
        // 注意：这里不再直接调用extract_claims_from_request，因为它已迁移到auth_utils.rs
        // 而是建议使用auth_utils中的方法
        // 直接使用默认值（Uuid::nil()）作为user_id
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

// 从请求中获取真实IP地址的辅助函数
pub fn get_real_ip_from_request(req: &actix_web::HttpRequest) -> String {
    // 先尝试从X-Forwarded-For头获取，这是最常用的代理IP头
    if let Some(xff) = req.headers().get("X-Forwarded-For")
        && let Ok(xff_str) = xff.to_str()
    {
        // X-Forwarded-For格式：client, proxy1, proxy2
        // 取第一个IP作为真实IP
        if let Some(real_ip) = xff_str.split(',').next().map(|s| s.trim().to_string()) {
            return real_ip;
        }
    }

    // 尝试从X-Real-IP头获取
    if let Some(x_real_ip) = req.headers().get("X-Real-IP")
        && let Ok(real_ip_str) = x_real_ip.to_str()
    {
        return real_ip_str.trim().to_string();
    }

    // 最后尝试从连接信息获取
    req.connection_info()
        .realip_remote_addr()
        .unwrap_or("unknown")
        .to_string()
}

// 发送MAC地址变更通知
pub async fn send_mac_change_notification(
    pool: &sqlx::PgPool,
    workstation_id: &Uuid,
    ip_address: &str,
    old_mac: &str,
    new_mac: &str,
) -> Result<(), sqlx::Error> {
    // 查询工位名称
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

    // 尝试发送邮件通知（通过调用smtp模块，避免循环依赖）
    // 使用 crate::system::smtp 直接调用，因为smtp不再依赖utils
    let _ = crate::system::smtp::send_mac_change_email(pool, &workstation_name, ip_address, old_mac, new_mac).await;

    Ok(())
}



// 获取指定IP地址的真实MAC地址
pub fn get_real_mac_address(ip: &str) -> Option<String> {
    // 首先尝试从ARP缓存读取
    if let Some(mac) = read_mac_from_arp_cache(ip) {
        return Some(mac);
    }

    // 如果缓存中没有，尝试ping后再读取
    // 这里使用同步方式，因为这个函数被同步调用
    if ip.parse::<IpAddr>().is_ok() {
        // 使用系统ping命令触发ARP
        let _ = std::process::Command::new("ping")
            .args(["-c", "1", "-W", "1", ip])
            .output();

        // 再次尝试读取ARP缓存
        if let Some(mac) = read_mac_from_arp_cache(ip) {
            return Some(mac);
        }
    }

    None
}

// 从/proc/net/arp文件读取MAC地址
fn read_mac_from_arp_cache(ip: &str) -> Option<String> {
    let content = std::fs::read_to_string("/proc/net/arp").ok()?;

    for line in content.lines().skip(1) {
        // 跳过标题行
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && parts[0] == ip {
            let mac = parts[3].to_uppercase();
            // 排除无效的MAC地址 (00:00:00:00:00:00 或 incomplete)
            if mac != "00:00:00:00:00:00" && validate_mac_address(&mac) {
                return Some(mac);
            }
        }
    }
    None
}

// 批量获取MAC地址（先ping所有IP，再批量读取ARP表）
pub async fn batch_get_mac_addresses(ips: &[String]) -> HashMap<String, Option<String>> {
    let mut results = HashMap::new();

    // 首先从现有ARP缓存读取
    let arp_cache = read_arp_cache();
    for ip in ips {
        if let Some(mac) = arp_cache.get(ip) {
            results.insert(ip.clone(), Some(mac.clone()));
        }
    }

    // 找出缓存中没有的IP
    let missing_ips: Vec<&String> = ips.iter().filter(|ip| !results.contains_key(*ip)).collect();

    if missing_ips.is_empty() {
        return results;
    }

    // 并发ping所有缺失的IP以触发ARP学习
    let _ = batch_ping(&missing_ips).await;

    // 等待一小段时间让ARP表更新
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 重新读取ARP缓存
    let arp_cache = read_arp_cache();

    // 更新结果
    for ip in missing_ips {
        let mac = arp_cache.get(ip).cloned();
        results.insert(ip.clone(), mac);
    }

    results
}

// 读取整个ARP缓存表
fn read_arp_cache() -> HashMap<String, String> {
    let mut cache = HashMap::new();

    if let Ok(content) = std::fs::read_to_string("/proc/net/arp") {
        for line in content.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let ip = parts[0].to_string();
                let mac = parts[3].to_uppercase();
                // 排除无效的MAC地址
                if mac != "00:00:00:00:00:00" && validate_mac_address(&mac) {
                    cache.insert(ip, mac);
                }
            }
        }
    }

    cache
}

// 批量ping IP地址
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

// 使用pnet发送ICMP ping
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

    // 对于IPv6，直接使用系统ping
    let ipv4_addr = match ip_addr {
        IpAddr::V4(addr) => addr,
        IpAddr::V6(_) => return system_ping(ip),
    };

    // 尝试创建传输通道（需要root权限）
    let protocol = Layer4(Ipv4(IpNextHeaderProtocols::Icmp));
    let (mut tx, _rx) = match transport_channel(1024, protocol) {
        Ok((tx, rx)) => (tx, rx),
        Err(_) => {
            // 如果没有权限，回退到系统ping
            return system_ping(ip);
        }
    };

    // 构建ICMP Echo请求包
    let mut buffer = [0u8; 64];
    let mut icmp_packet = match MutableEchoRequestPacket::new(&mut buffer) {
        Some(packet) => packet,
        None => return system_ping(ip),
    };

    icmp_packet.set_icmp_type(IcmpTypes::EchoRequest);
    icmp_packet.set_identifier(rand::random());
    icmp_packet.set_sequence_number(1);

    // 计算校验和 - 使用 IcmpPacket 进行校验和计算
    let checksum = {
        let icmp_data = icmp_packet.packet();
        if let Some(icmp_pkt) = IcmpPacket::new(icmp_data) {
            pnet::packet::icmp::checksum(&icmp_pkt)
        } else {
            0
        }
    };
    icmp_packet.set_checksum(checksum);

    // 发送ICMP请求
    if tx.send_to(icmp_packet, IpAddr::V4(ipv4_addr)).is_err() {
        return system_ping(ip);
    }

    // 等待响应（简化处理，只等待短时间）
    // 实际上我们主要是想触发ARP，不需要等待ICMP响应
    tokio::time::sleep(Duration::from_millis(50)).await;

    true
}

// 使用系统ping命令作为后备方案
fn system_ping(ip: &str) -> bool {
    std::process::Command::new("ping")
        .args(["-c", "1", "-W", "1", ip])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// 发送系统告警通知
pub async fn send_system_alert(
    pool: &sqlx::PgPool,
    title: &str,
    content: &str,
    alert_type: &str,
    user_id: Option<&Uuid>,
) -> Result<(), sqlx::Error> {
    // 使用sqlx直接插入通知，避免循环依赖
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

// 数据库错误处理工具函数
pub fn handle_db_error<E: std::fmt::Display>(err: E, message: &str) -> actix_web::HttpResponse {
    let err_str = err.to_string();
    
    // 检查是否是CIDR格式错误
    if err_str.contains("invalid cidr value") {
        return actix_web::HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "不符合CIDR格式",
            "data": null
        }));
    }
    
    // 检查是否是INET格式错误（IP地址格式错误）
    if err_str.contains("invalid inet value") {
        return actix_web::HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "不符合IP地址格式",
            "data": null
        }));
    }
    
    // 其他数据库错误
    actix_web::HttpResponse::InternalServerError().json(serde_json::json!({
        "success": false,
        "message": format!("{}: {}", message, err),
        "data": null
    }))
}

// CIDR验证工具
pub fn validate_cidr(cidr: &str) -> bool {
    // 单独的IPv4 CIDR正则
    let ipv4_regex = regex::Regex::new(r"^(?:(?:(?:[0-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-5])\.){3}(?:[0-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-5])/(?:[0-9]|[12]?[0-9]|3[0-2]))$").unwrap();
    // 单独的IPv6 CIDR正则
    let ipv6_regex =
        regex::Regex::new(r"^(?:[0-9a-fA-F:]+/(?:[0-9]|[1-9][0-9]|1[01][0-9]|12[0-8]))$").unwrap();

    // 首先检查格式是否正确
    if !ipv4_regex.is_match(cidr) && !ipv6_regex.is_match(cidr) {
        return false;
    }

    // 然后尝试使用ipnetwork库解析CIDR，确保它是有效的网络地址
    match ipnetwork::IpNetwork::from_str(cidr) {
        Ok(_) => true,
        Err(_) => false
    }
}

// 获取CIDR类型
pub fn get_cidr_type(cidr: &str) -> Option<&'static str> {
    let ipv4_regex = regex::Regex::new(r"^(?:(?:(?:[0-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-5])\.){3}(?:[0-9]|[1-9][0-9]|1[0-9]{2}|2[0-4][0-9]|25[0-5])/(?:[0-9]|[12]?[0-9]|3[0-2]))$").unwrap();
    let ipv6_regex =
        regex::Regex::new(r"^(?:[0-9a-fA-F:]+/(?:[0-9]|[1-9][0-9]|1[01][0-9]|12[0-8]))$").unwrap();

    if ipv4_regex.is_match(cidr) {
        Some("ipv4")
    } else if ipv6_regex.is_match(cidr) {
        Some("ipv6")
    } else {
        None
    }
}

// 同时输出中英文的日志记录函数
pub fn log_bilingual(message_key: &str) {
    // 设置语言为中文，获取中文消息
    rust_i18n::set_locale("zh");
    let zh_message = rust_i18n::t!(message_key);

    // 设置语言为英文，获取英文消息
    rust_i18n::set_locale("en");
    let en_message = rust_i18n::t!(message_key);

    // 同时输出中英文消息
    info!("[中文] {}", zh_message);
    info!("[English] {}", en_message);
}

// 从请求中检测用户语言偏好
pub fn detect_user_language(req: &actix_web::HttpRequest) -> String {
    // 首先从请求头中获取Accept-Language
    if let Some(accept_language) = req.headers().get("Accept-Language")
        && let Ok(accept_language_str) = accept_language.to_str()
        && let Some(lang) = accept_language_str.split(',').next()
    {
        // 提取语言代码，忽略地区代码
        let lang_code = lang.split('-').next().unwrap_or("").trim();
        if lang_code == "zh" || lang_code == "en" {
            return lang_code.to_string();
        }
    }

    // 默认返回中文
    "zh".to_string()
}
