//! 网络与 HTTP 请求纯工具（CIDR 校验、ILIKE 转义、真实 IP 提取等）。
//!
//! 本模块不依赖任何业务表与业务类型，可被所有 crate 复用；
//! 依赖业务表/`JwtClaims` 的工具保留在主程序 `utils::common`。

use std::net::SocketAddr;
use std::str::FromStr;

use axum::extract::ConnectInfo;
use axum::http::request::Parts;
use hex::encode;

use crate::{AppError, msg};

/// IPv4 映射地址（`::ffff:a.b.c.d`）剥离前缀返回纯 IPv4 文本，其余原样返回；
/// 用于 IP 记录与展示的统一口径。
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

// CIDR 验证

/// 验证 CIDR 格式是否合法（与 PostgreSQL CIDR 类型语义一致）
/// ipnetwork 解析时原样保留地址与掩码、不校验也不改写主机位，
/// 而 PostgreSQL CIDR 要求主机位全为 0，因此需额外比较 ip() 与 network()
#[must_use]
pub fn validate_cidr(cidr: &str) -> bool {
    match ipnetwork::IpNetwork::from_str(cidr) {
        Ok(ipnetwork::IpNetwork::V4(net)) => net.ip() == net.network(),
        Ok(ipnetwork::IpNetwork::V6(net)) => net.ip() == net.network(),
        Err(_) => false,
    }
}

/// CIDR 的地址族标识（"ipv4"/"ipv6"，与网关校验 family 参数同口径）；非法输入返回 None。
#[must_use]
pub fn get_cidr_type(cidr: &str) -> Option<&'static str> {
    match ipnetwork::IpNetwork::from_str(cidr) {
        Ok(ipnetwork::IpNetwork::V4(_)) => Some("ipv4"),
        Ok(ipnetwork::IpNetwork::V6(_)) => Some("ipv6"),
        Err(_) => None,
    }
}

/// 检查子网 CIDR 是否属于父网 CIDR（对齐 PostgreSQL `subnet << supernet`：
/// 严格包含——子网前缀更长且网络地址落在父网内，相同前缀不构成包含）。
#[must_use]
pub fn cidr_contains_subnet(subnet: &str, supernet: &str) -> bool {
    let subnet_net = ipnetwork::IpNetwork::from_str(subnet);
    let supernet_net = ipnetwork::IpNetwork::from_str(supernet);

    match (subnet_net, supernet_net) {
        (Ok(subnet), Ok(supernet)) => {
            match (subnet, supernet) {
                (ipnetwork::IpNetwork::V4(sub), ipnetwork::IpNetwork::V4(super_net)) => {
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

/// 检查网段 CIDR 是否属于区域 CIDR 列表中的任一 CIDR；区域未配置 CIDR 时视为不限制。
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
) -> Result<(), AppError> {
    let validation_error = |key: String| AppError::Validation(msg(key));

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

// Token 哈希

/// 令牌的 SHA-256 摘要（64 位小写十六进制）：注册/上报令牌落库只存哈希不存明文。
#[must_use]
pub fn generate_token_hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token);
    let hash = hasher.finalize();
    let hash_bytes: &[u8] = hash.as_ref();
    encode(hash_bytes)
}

/// 生成 32 字节随机注册令牌的 64 位小写十六进制字符串（agent 注册令牌）。
#[must_use]
pub fn generate_enrollment_token() -> String {
    use rand::Rng;
    let mut bytes = vec![0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 校验注册令牌格式：64 位十六进制（与上报 Bearer 令牌同口径）。
#[must_use]
pub fn valid_enrollment_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

// HTTP 请求处理

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

/// 从 Accept-Language 取首个语言标签的主子标签（zh-CN→zh）；仅支持 zh/en，其余（含头缺失）回退 zh。
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

/// login_logs.user_agent 列宽（VARCHAR(255)）
const USER_AGENT_MAX_CHARS: usize = 255;

/// 从 axum 请求 parts 中获取 User-Agent
///
/// 按字符数截断到 255 以内（对齐 login_logs 的 VARCHAR(255)，
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

/// 可信代理判定：仅信任回环地址（127.0.0.1 / ::1）。
///
/// 反向代理（nginx）与后端同机部署经回环/UDS 通信，属于可信来源；
/// RFC1918 私网与 IPv6 ULA 一律不再视为可信——TCP 直接暴露时内网任意
/// 客户端若被信任即可伪造 `X-Real-IP` 轮换身份，绕过 IP 限流与 fail2ban。
fn is_trusted_proxy(ip: &std::net::IpAddr) -> bool {
    ip.is_loopback()
}

// IP/CIDR 纯函数工具
//
// 供 foims-resource（IP 资源域）与 foims-data-management（导入导出校验）
// 共用，避免各 crate 手写掩码比较与地址族推导。

/// 解析 inet 值（可选 /nn 后缀，按 PG 语义忽略掩码只取地址）。
#[must_use]
pub fn parse_inet(value: &str) -> Option<std::net::IpAddr> {
    let addr = value.split('/').next()?;
    addr.parse::<std::net::IpAddr>().ok()
}

/// 解析 cidr 值，返回 (网络地址, 前缀长度)。
#[must_use]
pub fn parse_cidr(value: &str) -> Option<(std::net::IpAddr, u8)> {
    let (addr, prefix) = value.split_once('/')?;
    let addr: std::net::IpAddr = addr.parse().ok()?;
    let prefix: u8 = prefix.parse().ok()?;
    let max = match addr {
        std::net::IpAddr::V4(_) => 32,
        std::net::IpAddr::V6(_) => 128,
    };
    if prefix > max {
        return None;
    }
    Some((addr, prefix))
}

/// 地址是否属于子网（含网络地址与广播地址）。
#[must_use]
pub fn ip_in_cidr(ip: std::net::IpAddr, cidr_addr: std::net::IpAddr, prefix: u8) -> bool {
    match (ip, cidr_addr) {
        (std::net::IpAddr::V4(ip), std::net::IpAddr::V4(net)) => {
            if prefix > 32 {
                return false;
            }
            let mask = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            };
            (u32::from(ip) & mask) == (u32::from(net) & mask)
        }
        (std::net::IpAddr::V6(ip), std::net::IpAddr::V6(net)) => {
            if prefix > 128 {
                return false;
            }
            let mask = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - prefix)
            };
            (u128::from(ip) & mask) == (u128::from(net) & mask)
        }
        _ => false,
    }
}

/// 由地址自动推导 IP 版本号（interface_ips.ip_version 列 4/6；非法地址返回校验错误）
pub fn detect_ip_version(ip: &str) -> Result<i16, AppError> {
    match ip.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V6(_)) => Ok(6),
        Ok(std::net::IpAddr::V4(_)) => Ok(4),
        Err(e) => Err(AppError::Validation(
            msg("server.ip.invalid_ip").with("ip", ip).with("error", e),
        )),
    }
}

// MAC 地址归一

/// 归一 MAC：剔除 `:`/`-`/`.`/空格并转大写，返回 `AABBCCDDEEFF` 形态。
///
/// 台账侧（人工录入 / CSV 导入）与 SNMP 观测侧书写形态不一，所有落库与
/// 跨表比对必须经本函数，否则同一物理 MAC 会因大小写/分隔符差异被当作
/// 两个不同值，唯一性约束与冲突检测随之失效。
#[must_use]
pub fn normalize_mac(mac: &str) -> String {
    mac.chars()
        .filter(|c| !matches!(c, ':' | '-' | '.' | ' '))
        .flat_map(char::to_uppercase)
        .collect()
}

/// MAC 是否合法：归一后恰为 12 位十六进制（分隔符无关）。
///
/// 接受 `aa:bb:cc:dd:ee:ff`、`AA-BB-CC-DD-EE-FF`、`aabb.ccdd.eeff`、
/// `aabbccddeeff` 四种常见书写。
#[must_use]
pub fn valid_mac(mac: &str) -> bool {
    let normalized = normalize_mac(mac);
    normalized.len() == 12 && normalized.bytes().all(|b| b.is_ascii_hexdigit())
}

/// MAC 的规范存储形态 `AA:BB:CC:DD:EE:FF`（大写、冒号分隔、17 字符）。
///
/// 落库统一写规范形态（列宽 VARCHAR(20)），使前端/报表/导出不必再判断
/// 分隔符风格；比对仍以 [`normalize_mac`] 为口径，二者不可互相替代。
/// 长度非法时原样返回归一结果，不做二次猜测。
#[must_use]
pub fn canonical_mac(mac: &str) -> String {
    let normalized = normalize_mac(mac);
    if normalized.len() != 12 {
        return normalized;
    }
    normalized
        .as_bytes()
        .chunks(2)
        .map(|pair| String::from_utf8_lossy(pair).into_owned())
        .collect::<Vec<_>>()
        .join(":")
}

/// Host 值是否指向本机：仅接受 `localhost`、`127.0.0.1`、`::1`
/// （域名不区分大小写；容忍带端口 `127.0.0.1:443`、方括号 IPv6
/// `[::1]:443` 与裸 IPv6 `::1` 三种形态）。
#[must_use]
fn is_localhost_host_value(raw: &str) -> bool {
    let host = if let Some(rest) = raw.strip_prefix('[') {
        // [::1]:443 → ::1
        match rest.split_once(']') {
            Some((h, _)) => h,
            None => return false,
        }
    } else if raw.matches(':').count() == 1 {
        // 单冒号：host:port（IPv4/域名）
        match raw.split_once(':') {
            Some((h, _)) => h,
            None => raw,
        }
    } else {
        // 裸 IPv6（多冒号无端口）或普通主机名
        raw
    };
    matches!(
        host.to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1"
    )
}

/// 判定请求是否来自本机（供仅限本机使用的端点做来源守卫）。
///
/// 判定要点：
///   1. TCP 直连（extensions 携带 ConnectInfo）时以真实对端地址为准——
///      X-Real-IP 是普通请求头，内网客户端可任意伪造，直接据此判定
///      回环等于开放鉴权绕过；
///   2. 仅当对端确为回环地址（本机直连，或经监听在同一台机器上的 nginx
///      转发）或连接来自 UDS（无对端信息，由本机 nginx 代理）时，才采信
///      X-Real-IP 头判定真实来源——nginx 以
///      `proxy_set_header X-Real-IP $remote_addr;` 覆盖式写入，取自
///      TCP 对端，客户端无法伪造。切勿改用 X-Forwarded-For：其经
///      `$proxy_add_x_forwarded_for` 会保留客户端伪造的首段值；
///   3. Host 头兜底校验（防 DNS rebinding）：rebinding 攻击把攻击域名
///      解析到 127.0.0.1，来源 IP 判定无法区分，故 Host（或 h2c 链路的
///      `:authority`，经请求 URI 暴露）必须为 localhost/127.0.0.1/::1；
///      本项目 nginx 以 `proxy_set_header Host $host` 原样透传客户端
///      Host，本机初始化访问应使用 localhost/127.0.0.1；
///   4. fail-close：无法判定来源（含 Host 信息缺失）时默认拒绝。
#[must_use]
pub fn is_localhost_request_from_parts(parts: &Parts) -> bool {
    let peer_loopback = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip().is_loopback());

    let header_ip = parts
        .headers
        .get("X-Real-IP")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<std::net::IpAddr>().ok());

    let source_ok = match peer_loopback {
        // 回环对端：本机直连或经本机 nginx 转发，按 X-Real-IP 判定真实来源
        //（无头时视为本机直连放行）
        Some(true) => header_ip.is_none_or(|ip| ip.is_loopback()),
        Some(false) => false,
        // UDS：无对端信息，仅能依赖本机反代写入的 X-Real-IP 判定
        None => header_ip.is_some_and(|ip| ip.is_loopback()),
    };
    if !source_ok {
        return false;
    }

    let host = parts
        .headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| parts.uri.host());
    host.is_some_and(is_localhost_host_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    // IP 归一化

    #[test]
    fn test_normalize_ipv4_address_strips_v4_mapped_prefix() {
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

    // ILIKE 转义

    #[test]
    fn test_escape_like_plain_text() {
        assert_eq!(escape_like("交换机"), "%交换机%");
        assert_eq!(escape_like(""), "%%");
    }

    #[test]
    fn test_escape_like_special_characters() {
        assert_eq!(escape_like(r"a\b"), r"%a\\b%");
        assert_eq!(escape_like("50%"), r"%50\%%");
        assert_eq!(escape_like("ip_v6"), r"%ip\_v6%");
        // 混合场景：\ 先转义为 \\，随后 % 与 _ 再转义
        assert_eq!(escape_like(r"100_\a"), r"%100\_\\a%");
    }

    #[test]
    fn test_escape_like_backslash_before_percent() {
        assert_eq!(escape_like(r"\%"), r"%\\\%%");
    }

    // CIDR 工具

    #[test]
    fn test_validate_cidr_valid() {
        assert!(validate_cidr("192.168.1.0/24"));
        assert!(validate_cidr("10.0.0.0/8"));
        assert!(validate_cidr("0.0.0.0/0"));
        assert!(validate_cidr("2001:db8::/32"));
        assert!(validate_cidr("fe80::/10"));
    }

    #[test]
    fn test_validate_cidr_host_bits_rejected() {
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
        assert!(cidr_contains_subnet("192.168.1.0/26", "192.168.1.0/24"));
        assert!(cidr_contains_subnet("10.1.2.0/28", "10.0.0.0/8"));
        assert!(cidr_contains_subnet("2001:db8:1::/48", "2001:db8::/32"));
    }

    #[test]
    fn test_cidr_contains_subnet_invalid() {
        assert!(!cidr_contains_subnet("192.168.1.0/24", "192.168.1.0/24"));
        assert!(!cidr_contains_subnet("192.168.0.0/16", "192.168.1.0/24"));
        assert!(!cidr_contains_subnet("10.2.0.0/16", "192.168.0.0/16"));
        assert!(!cidr_contains_subnet("10.0.0.0/8", "::/0"));
        assert!(!cidr_contains_subnet("bad", "10.0.0.0/8"));
        assert!(!cidr_contains_subnet("10.0.0.0/8", "bad"));
    }

    #[test]
    fn test_cidr_belongs_to_region() {
        let region: Vec<String> = vec!["10.0.0.0/8".to_string(), "172.16.0.0/12".to_string()];
        assert!(cidr_belongs_to_region("10.1.0.0/16", &region));
        assert!(cidr_belongs_to_region("172.16.5.0/24", &region));
        assert!(!cidr_belongs_to_region("192.168.1.0/24", &region));
        assert!(cidr_belongs_to_region("192.168.1.0/24", &[]));
    }

    // 网关校验

    /// 从 AppError::Validation 中取出消息 key，便于断言
    fn validation_key(result: Result<(), AppError>) -> String {
        match result {
            Err(AppError::Validation(m)) => m.key().to_string(),
            Err(other) => panic!("应为 Validation 错误，实际 {other}"),
            Ok(()) => panic!("应为 Err，实际 Ok"),
        }
    }

    #[test]
    fn test_validate_gateway_in_cidr_none_gateway_passes() {
        assert!(validate_gateway_in_cidr(None, None, "ipv4").is_ok());
        assert!(validate_gateway_in_cidr(Some("  "), Some("10.0.0.0/8"), "ipv4").is_ok());
    }

    #[test]
    fn test_validate_gateway_in_cidr_valid() {
        assert!(validate_gateway_in_cidr(Some("10.1.0.254"), Some("10.1.0.0/24"), "ipv4").is_ok());
        assert!(
            validate_gateway_in_cidr(Some("10.1.0.254/32"), Some("10.1.0.0/24"), "ipv4").is_ok()
        );
        assert!(
            validate_gateway_in_cidr(Some("2001:db8::1"), Some("2001:db8::/32"), "ipv6").is_ok()
        );
    }

    #[test]
    fn test_validate_gateway_in_cidr_format_and_family_errors() {
        assert_eq!(
            validation_key(validate_gateway_in_cidr(Some("bad-ip"), None, "ipv4")),
            "server.network.ipv4_gateway_invalid"
        );
        assert_eq!(
            validation_key(validate_gateway_in_cidr(
                Some("10.0.0.1"),
                Some("2001:db8::/32"),
                "ipv6"
            )),
            "server.network.ipv6_gateway_invalid"
        );
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

    // Token 哈希

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

    // 请求元信息（Parts 级纯函数）

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
        let parts = parts_with_headers(&[("X-Real-IP", "1.2.3.4")]);
        assert_eq!(get_real_ip_from_parts(&parts), "1.2.3.4");
    }

    #[test]
    fn test_get_real_ip_ignores_x_forwarded_for() {
        // X-Forwarded-For 不再作为回退来源（首段可伪造，见 I-1/A-1）
        let parts = parts_with_headers(&[("X-Forwarded-For", "5.6.7.8, 10.0.0.1")]);
        assert_eq!(get_real_ip_from_parts(&parts), "unknown");
        let both = parts_with_headers(&[("X-Real-IP", "1.2.3.4"), ("X-Forwarded-For", "5.6.7.8")]);
        assert_eq!(get_real_ip_from_parts(&both), "1.2.3.4");
    }

    #[test]
    fn test_get_real_ip_unknown_without_any_source() {
        let parts = parts_with_headers(&[]);
        assert_eq!(get_real_ip_from_parts(&parts), "unknown");
    }

    #[test]
    fn test_get_real_ip_normalizes_v4_mapped_header() {
        let parts = parts_with_headers(&[("X-Real-IP", "::ffff:9.9.9.9")]);
        assert_eq!(get_real_ip_from_parts(&parts), "9.9.9.9");
    }

    #[test]
    fn test_get_real_ip_untrusted_peer_ignores_headers() {
        let parts = parts_with_peer(
            IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
            &[("X-Real-IP", "1.2.3.4"), ("X-Forwarded-For", "5.6.7.8")],
        );
        assert_eq!(get_real_ip_from_parts(&parts), "8.8.8.8");
    }

    #[test]
    fn test_get_real_ip_trusted_peer_uses_x_real_ip() {
        let loopback = parts_with_peer(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            &[("X-Real-IP", "1.2.3.4")],
        );
        assert_eq!(get_real_ip_from_parts(&loopback), "1.2.3.4");

        // 可信 peer 但仅有可伪造的 X-Forwarded-For 时不再采信，回退 peer IP
        let loopback2 = parts_with_peer(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            &[("X-Forwarded-For", "5.6.7.8")],
        );
        assert_eq!(get_real_ip_from_parts(&loopback2), "127.0.0.1");
    }

    #[test]
    fn test_detect_user_language() {
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
        let long_ua = "U".repeat(300);
        let parts = parts_with_headers(&[("User-Agent", long_ua.as_str())]);
        let ua = get_user_agent_from_parts(&parts);
        assert_eq!(ua.chars().count(), 255);
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

    // 可信代理判定

    #[test]
    fn test_is_trusted_proxy() {
        assert!(is_trusted_proxy(&IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_trusted_proxy(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(!is_trusted_proxy(&IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3))));
        assert!(!is_trusted_proxy(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(!is_trusted_proxy(&IpAddr::V4(Ipv4Addr::new(
            192, 168, 1, 1
        ))));
        assert!(!is_trusted_proxy(&IpAddr::V6(Ipv6Addr::new(
            0xfd00, 0, 0, 0, 0, 0, 0, 1
        ))));
        assert!(!is_trusted_proxy(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_trusted_proxy(&IpAddr::V6(Ipv6Addr::new(
            0x2001, 0xdb8, 0, 0, 0, 0, 0, 1
        ))));
    }

    // MAC 归一

    #[test]
    fn test_normalize_mac_strips_separators_and_uppercases() {
        for raw in [
            "aa:bb:cc:dd:ee:ff",
            "AA-BB-CC-DD-EE-FF",
            "aabb.ccdd.eeff",
            "  aabbccddeeFF  ",
            "AABBCCDDEEFF",
        ] {
            assert_eq!(normalize_mac(raw), "AABBCCDDEEFF", "{raw} 应归一为同一值");
        }
        assert_eq!(normalize_mac(""), "");
    }

    #[test]
    fn test_canonical_mac_formats_colon_separated() {
        for raw in ["aa:bb:cc:dd:ee:ff", "AA-BB-CC-DD-EE-FF", "aabb.ccdd.eeff"] {
            assert_eq!(
                canonical_mac(raw),
                "AA:BB:CC:DD:EE:FF",
                "{raw} 应规范为冒号大写形态"
            );
        }
        assert_eq!(canonical_mac("abcd"), "ABCD");
    }

    #[test]
    fn test_valid_mac_accepts_common_forms() {
        for ok in [
            "aa:bb:cc:dd:ee:ff",
            "AA-BB-CC-DD-EE-FF",
            "aabb.ccdd.eeff",
            "aabbccddeeff",
        ] {
            assert!(valid_mac(ok), "{ok} 应判定为合法 MAC");
        }
    }

    #[test]
    fn test_valid_mac_rejects_invalid() {
        for bad in [
            "",
            "aa:bb:cc:dd:ee",
            "aa:bb:cc:dd:ee:ff:00",
            "gg:bb:cc:dd:ee:ff",
            "not-a-mac",
            // 单字符分组简写不属四种规范形态，归一后不足 12 位十六进制
            "a:b:c:d:e:f",
        ] {
            assert!(!valid_mac(bad), "{bad} 应判定为非法 MAC");
        }
    }

    // Host 校验（防 DNS rebinding）

    #[test]
    fn test_is_localhost_host_value() {
        for good in [
            "localhost",
            "LOCALHOST",
            "127.0.0.1",
            "127.0.0.1:443",
            "::1",
            "[::1]",
            "[::1]:8443",
        ] {
            assert!(is_localhost_host_value(good), "{good} 应指向本机");
        }
        for bad in [
            "evil.com",
            "EVIL.com",
            "192.168.1.10",
            "10.0.0.2:80",
            "[::1",
            "",
        ] {
            assert!(!is_localhost_host_value(bad), "{bad} 不应指向本机");
        }
    }

    #[test]
    fn test_is_localhost_request_host_guard() {
        // UDS + X-Real-IP 回环：Host 为 localhost 时放行
        let ok = parts_with_headers(&[("X-Real-IP", "127.0.0.1"), ("Host", "localhost:8443")]);
        assert!(is_localhost_request_from_parts(&ok));

        // DNS rebinding：来源回环但 Host 为攻击域名 → 拒绝
        let rebound = parts_with_headers(&[("X-Real-IP", "127.0.0.1"), ("Host", "evil.com")]);
        assert!(!is_localhost_request_from_parts(&rebound));

        // 无 Host 头也无 URI authority → fail-close 拒绝
        let no_host = parts_with_headers(&[("X-Real-IP", "127.0.0.1")]);
        assert!(!is_localhost_request_from_parts(&no_host));

        // 回环直连 + Host 带端口放行
        let direct = parts_with_peer(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &[("Host", "127.0.0.1:8080")],
        );
        assert!(is_localhost_request_from_parts(&direct));

        // 回环直连但 Host 非本机（经本机代理访问外部虚拟主机）→ 拒绝
        let foreign = parts_with_peer(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &[("Host", "intranet.example.org")],
        );
        assert!(!is_localhost_request_from_parts(&foreign));
    }
}
