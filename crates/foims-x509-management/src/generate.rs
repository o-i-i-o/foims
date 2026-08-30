//! 证书生成（rcgen）：强制由所选 CA（站点根 CA 或导入 CA）签发叶子证书。

use std::path::PathBuf;

use foims_common::msg;
use rcgen::{
    CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose, SanType,
};
use serde::{Deserialize, Serialize};

use crate::GENERATED_CERTS_DIR;
use crate::error::CertManagerError;

const DEFAULT_VALIDITY_DAYS: i64 = 3650;
const MAX_VALIDITY_DAYS: i64 = 36500;

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateCertRequest {
    pub common_name: String,
    pub organization: Option<String>,
    pub organizational_unit: Option<String>,
    pub country: Option<String>,
    pub state: Option<String>,
    pub locality: Option<String>,
    /// 有效期（天），缺省 3650
    pub validity_days: Option<i32>,
    /// 主题备用名称（IP 或域名）
    pub subject_alt_names: Option<Vec<String>>,
    /// 签发 CA 标识：缺省或 "root" 为站点根 CA，否则为导入 CA 池中的目录名。
    /// 证书强制由所选 CA 签发，不再提供自签名回退。
    #[serde(default)]
    pub ca_id: Option<String>,
}

/// 解析 SAN 条目：IP 地址优先，否则按 DNS 名称
fn parse_san(entry: &str) -> Option<SanType> {
    let trimmed = entry.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(ip) = trimmed.parse::<std::net::IpAddr>() {
        return Some(SanType::IpAddress(ip));
    }
    if let Ok(dns_name) = trimmed.try_into() {
        return Some(SanType::DnsName(dns_name));
    }
    None
}

/// 请求参数校验（等价 Validate 派生：本 crate 未依赖 validator，手写实现）：
/// DN 字段与 CA 请求共用同一套口径（CN 1..=64、org/ou/state/locality ≤64、
/// country trim 后 2 字符），SAN ≤100 条且单条 ≤253 字符（DNS 名称长度上限）。
fn validate_generate_request(req: &GenerateCertRequest) -> Result<(), CertManagerError> {
    crate::ca::validate_dn_fields(
        &req.common_name,
        req.organization.as_deref(),
        req.organizational_unit.as_deref(),
        req.country.as_deref(),
        req.state.as_deref(),
        req.locality.as_deref(),
    )?;

    let validation = |key: &'static str| CertManagerError::Validation(msg(key));
    if let Some(sans) = &req.subject_alt_names {
        if sans.len() > 100 {
            return Err(validation("server.certificate.san_invalid"));
        }
        if sans.iter().any(|s| s.trim().chars().count() > 253) {
            return Err(validation("server.certificate.san_invalid"));
        }
    }
    Ok(())
}

/// 生成证书的写入结果：文件路径 + 被丢弃的 SAN 条目数。
///
/// `dropped_san_count > 0` 表示请求中存在无法解析（非 IP、非合法 IA5 域名）
/// 而被丢弃的 SAN 条目，证书仍照常签发，但调用方应在成功响应中提示用户
/// 其请求的主机名可能不在 SAN 中（配合 message 键区分成功文案）。
#[derive(Debug)]
pub struct GeneratedCert {
    /// 证书文件路径
    pub cert_path: PathBuf,
    /// 因无法解析而被丢弃的 SAN 条目数
    pub dropped_san_count: usize,
}

/// 生成证书并写入生成目录，返回写入结果（含被丢弃的 SAN 条目数）。
///
/// 强制由所选 CA 签发（`req.ca_id` 缺省视为站点根 CA）：
/// CA 不存在、无私钥或有效期不足（已过期或剩余不足 1 天）时直接报错。
///
/// `extra_sans` 为调用方注入的额外 SAN（如服务器 public_url 的主机名），
/// 与请求中的 SAN 合并；两者均为空时回退使用 common_name。
pub async fn generate_certificate(
    req: GenerateCertRequest,
    extra_sans: Vec<String>,
) -> Result<GeneratedCert, CertManagerError> {
    // 统一请求校验（CN/组织/国家/SAN 条数与长度）
    validate_generate_request(&req)?;
    let common_name = req.common_name.trim().to_string();

    let validity_days = req
        .validity_days
        .map_or(DEFAULT_VALIDITY_DAYS, i64::from)
        .clamp(1, MAX_VALIDITY_DAYS);

    // CA 物料在阻塞线程外读取，避免阻塞异步运行时；缺失即拒绝生成
    let ca_material = match crate::ca::load_ca_material_by_id(req.ca_id.as_deref()).await {
        Some(material) => material,
        None => {
            // 区分"CA 不存在"与"CA 无私钥"给出准确错误
            if crate::ca::ca_cert_exists(req.ca_id.as_deref()).await {
                return Err(CertManagerError::Validation(msg(
                    "server.certificate.ca_key_missing",
                )));
            }
            return Err(CertManagerError::NotFound(msg(
                "server.certificate.ca_not_found",
            )));
        }
    };

    // 签发前校验所选 CA 有效期：CA 已过期或剩余不足 1 天时拒绝签发——
    // 叶子证书有效期不得超过签发 CA 的 not_after，否则链提前断裂，
    // 不再以 max(1) 硬签出必然无效的证书
    let validity_days = {
        let ca_remaining = crate::listing::parse_cert_metadata(ca_material.0.as_bytes())
            .and_then(|meta| meta.not_after)
            .map(|not_after| (not_after - chrono::Utc::now()).num_days());
        match ca_remaining {
            Some(remaining) if remaining < 1 => {
                return Err(CertManagerError::Validation(msg(
                    "server.certificate.ca_expired",
                )));
            }
            Some(remaining) => validity_days.min(remaining),
            None => validity_days,
        }
    };

    tokio::fs::create_dir_all(GENERATED_CERTS_DIR)
        .await
        .map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;

    let generation = tokio::task::spawn_blocking(move || {
        build_leaf_cert(&common_name, &req, validity_days, extra_sans, ca_material)
    })
    .await
    .map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.generate_failed").with("error", e.to_string()),
        )
    })??;

    // 毫秒时间戳候选名 + create_new 独占创建：并发抢占以 AlreadyExists
    // 递增序号重试（有界）并清理半成品，杜绝"证书 A + 私钥 B"的错配对
    let cert_path = write_cert_pair_exclusive(
        GENERATED_CERTS_DIR,
        "create_",
        &generation.cert_pem,
        &generation.key_pem,
    )
    .await?;

    Ok(GeneratedCert {
        cert_path,
        dropped_san_count: generation.dropped_san_count,
    })
}

/// 文件名冲突重试上限（毫秒时间戳下冲突概率极低，仅作有界保护）
const MAX_FILENAME_RETRIES: u64 = 100;

/// 以毫秒时间戳候选名独占创建证书/私钥文件对（生成与导入共用）。
///
/// 私钥先以 create_new(0600) 独占创建；证书再以 create_new 写入。
/// 任一步因同名冲突（AlreadyExists）失败时清理已创建的文件并换下一序号；
/// 证书写失败（非冲突）时清理私钥后报错，避免残留孤儿私钥。
pub(crate) async fn write_cert_pair_exclusive(
    dir: &str,
    prefix: &str,
    cert_pem: &str,
    key_pem: &str,
) -> Result<PathBuf, CertManagerError> {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let mut seq: u64 = 0;
    loop {
        let stem = crate::cert_stem_for(prefix, timestamp, seq);
        let cert_path = PathBuf::from(dir).join(format!("{stem}.pem"));
        let key_path = PathBuf::from(dir).join(format!("{stem}.key"));

        match create_key_file_new(&key_path, key_pem).await {
            Ok(()) => match create_cert_file_new(&cert_path, cert_pem).await {
                Ok(()) => return Ok(cert_path),
                // 证书同名冲突：清理刚创建的私钥后换名重试
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if let Err(remove_err) = tokio::fs::remove_file(&key_path).await {
                        return Err(CertManagerError::Internal(
                            msg("server.certificate.write_failed")
                                .with("error", remove_err.to_string()),
                        ));
                    }
                }
                // 证书写失败（非冲突）：清理私钥，避免残留孤儿私钥
                Err(e) => {
                    if let Err(remove_err) = tokio::fs::remove_file(&key_path).await {
                        return Err(CertManagerError::Internal(
                            msg("server.certificate.write_failed")
                                .with("error", remove_err.to_string()),
                        ));
                    }
                    return Err(CertManagerError::Internal(
                        msg("server.certificate.write_failed").with("error", e.to_string()),
                    ));
                }
            },
            // 私钥同名冲突：直接换名重试
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => {
                return Err(CertManagerError::Internal(
                    msg("server.certificate.write_failed").with("error", e.to_string()),
                ));
            }
        }

        seq += 1;
        if seq > MAX_FILENAME_RETRIES {
            return Err(CertManagerError::Internal(
                msg("server.certificate.write_failed")
                    .with("error", format!("文件名冲突重试耗尽: {prefix}_{timestamp}")),
            ));
        }
    }
}

struct GeneratedPem {
    cert_pem: String,
    key_pem: String,
    /// 因无法解析而被丢弃（未进入证书）的 SAN 条目数
    dropped_san_count: usize,
}

fn build_leaf_cert(
    common_name: &str,
    req: &GenerateCertRequest,
    validity_days: i64,
    extra_sans: Vec<String>,
    ca_material: (String, String),
) -> Result<GeneratedPem, CertManagerError> {
    let key_pair = KeyPair::generate().map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.generate_failed").with("error", e.to_string()),
        )
    })?;

    let now = time::OffsetDateTime::now_utc();
    let mut params = CertificateParams::default();
    params.not_before = now;
    params.not_after = now + time::Duration::days(validity_days);

    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, common_name);
    if let Some(org) = req
        .organization
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        dn.push(DnType::OrganizationName, org);
    }
    if let Some(ou) = req
        .organizational_unit
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        dn.push(DnType::OrganizationalUnitName, ou);
    }
    if let Some(country) = req
        .country
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        dn.push(DnType::CountryName, country);
    }
    if let Some(state) = req
        .state
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        dn.push(DnType::StateOrProvinceName, state);
    }
    if let Some(locality) = req
        .locality
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        dn.push(DnType::LocalityName, locality);
    }
    params.distinguished_name = dn;

    // SAN：请求条目 + 调用方注入（public_url 主机），去重；均为空时回退 CN。
    // 现代浏览器（含 HTTP/3 over QUIC 的 RFC 9525 主机名匹配）只认 SAN、忽略 CN，
    // 因此 SAN 是服务器证书的必备 x509v3 扩展。
    let mut san_entries: Vec<String> = req.subject_alt_names.clone().unwrap_or_default();
    san_entries.extend(extra_sans);
    let mut sans: Vec<SanType> = Vec::new();
    let mut invalid_san_count = 0usize;
    for entry in &san_entries {
        match parse_san(entry) {
            Some(san) => {
                if !sans.contains(&san) {
                    sans.push(san);
                }
            }
            // 无法解析的条目（非 IP、非合法 IA5 域名）计数告警，不再静默丢弃
            None => invalid_san_count += 1,
        }
    }
    if invalid_san_count > 0 {
        foims_common::log_warn!(
            "log.certificate.san_parse_failed",
            count = invalid_san_count
        );
    }
    if sans.is_empty()
        && let Some(san) = parse_san(common_name)
    {
        sans.push(san);
    }
    // 全部 SAN 条目无效且 CN 不可作为 SAN 时拒绝生成：
    // 无 SAN 的服务器证书在浏览器侧不可用，不能报成功
    if sans.is_empty() {
        return Err(CertManagerError::Validation(msg(
            "server.certificate.san_invalid",
        )));
    }
    params.subject_alt_names = sans;

    // x509v3 扩展：显式 CA:FALSE（叶子证书最佳实践）、
    // EKU 仅 ServerAuth（同时覆盖 TLS TCP 与 QUIC/HTTP3 的服务器认证）、
    // KeyUsage DigitalSignature（ECDSA P-256 密钥协商仅依赖签名）
    params.is_ca = IsCa::ExplicitNoCa;
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];

    // 强制 CA 签发（物料已在异步层校验存在）
    let (ca_cert_pem, ca_key_pem) = ca_material;
    let ca_key = KeyPair::from_pem(&ca_key_pem).map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.generate_failed").with("error", e.to_string()),
        )
    })?;
    let issuer = rcgen::Issuer::from_ca_cert_pem(&ca_cert_pem, ca_key).map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.generate_failed").with("error", e.to_string()),
        )
    })?;
    let cert = params.signed_by(&key_pair, &issuer).map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.generate_failed").with("error", e.to_string()),
        )
    })?;

    Ok(GeneratedPem {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
        dropped_san_count: invalid_san_count,
    })
}

/// 以 `create_new` 独占创建私钥文件并写入内容（unix 下 0600 权限）。
///
/// 目标已存在时返回 `ErrorKind::AlreadyExists`，由调用方换名重试；
/// 不复用 [`write_key_file`] 的"先删后建"覆盖语义。
async fn create_key_file_new(path: &PathBuf, content: &str) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;

    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).await?;
    file.write_all(content.as_bytes()).await?;
    file.flush().await?;
    Ok(())
}

/// 以 `create_new` 独占创建证书文件并写入内容（默认 0644 权限，
/// 证书为公开物料）。冲突语义同 [`create_key_file_new`]。
async fn create_cert_file_new(path: &PathBuf, content: &str) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;
    file.write_all(content.as_bytes()).await?;
    file.flush().await?;
    Ok(())
}

/// 写入私钥文件并保证 0600 权限（仅属主可读写）。
///
/// unix 平台以 `OpenOptions::create_new + mode(0o600)` 创建后写入，消除
/// "先写后 chmod"窗口期内其他用户可读（0644）的私钥泄露面；已存在的旧
/// 文件先删除（仅 CA 重生成需要覆盖语义），保证新建时一定套用 0600。
/// 非 unix 平台无 POSIX 权限位语义，保持直接写入。
pub(crate) async fn write_key_file(path: &PathBuf, content: &str) -> Result<(), CertManagerError> {
    // 先移除既有文件（NotFound 容忍），确保后续 create_new 以 0600 新建
    if let Err(e) = tokio::fs::remove_file(path).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(CertManagerError::Internal(
            msg("server.certificate.write_failed").with("error", e.to_string()),
        ));
    }

    #[cfg(unix)]
    {
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .await
            .map_err(|e| {
                CertManagerError::Internal(
                    msg("server.certificate.write_failed").with("error", e.to_string()),
                )
            })?;
        file.write_all(content.as_bytes()).await.map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;
        // tokio File 的写入先进内部缓冲，必须显式 flush 确保落盘
        file.flush().await.map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;
    }

    #[cfg(not(unix))]
    {
        tokio::fs::write(path, content).await.map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn san解析_ipv4与ipv6优先按地址处理() {
        let Some(SanType::IpAddress(ip)) = parse_san("192.168.1.1") else {
            panic!("IPv4 应解析为 IpAddress");
        };
        assert_eq!(
            ip,
            "192.168.1.1"
                .parse::<std::net::IpAddr>()
                .unwrap_or_else(|e| panic!("解析失败: {e}"))
        );

        let Some(SanType::IpAddress(ip)) = parse_san("::1") else {
            panic!("IPv6 应解析为 IpAddress");
        };
        assert_eq!(
            ip,
            "::1"
                .parse::<std::net::IpAddr>()
                .unwrap_or_else(|e| panic!("解析失败: {e}"))
        );
    }

    #[test]
    fn san解析_域名按dns处理() {
        let Some(SanType::DnsName(dns)) = parse_san("gw.example.com") else {
            panic!("域名应解析为 DnsName");
        };
        assert_eq!(dns.as_str(), "gw.example.com");
    }

    #[test]
    fn san解析_空白与首尾空格() {
        // 首尾空格会被裁剪后按域名解析
        let Some(SanType::DnsName(dns)) = parse_san("  host.example.com  ") else {
            panic!("裁剪后的域名应解析为 DnsName");
        };
        assert_eq!(dns.as_str(), "host.example.com");

        assert!(parse_san("").is_none(), "空串返回 None");
        assert!(parse_san("   ").is_none(), "纯空白返回 None");
    }

    #[test]
    fn san解析_非法输入返回none() {
        // IA5 域名仅接受 ASCII，非 ASCII 字符无法转换
        assert!(parse_san("例え.jp").is_none(), "非 ASCII 无法作为 IA5 域名");
        assert!(
            parse_san("中文.example").is_none(),
            "非 ASCII 无法作为 IA5 域名"
        );
    }

    /// 构造仅含 CN 的最小请求
    fn minimal_request(cn: &str) -> GenerateCertRequest {
        GenerateCertRequest {
            common_name: cn.to_string(),
            organization: None,
            organizational_unit: None,
            country: None,
            state: None,
            locality: None,
            validity_days: None,
            subject_alt_names: None,
            ca_id: None,
        }
    }

    /// 构造测试用 CA 物料（证书 PEM, 私钥 PEM）
    fn test_ca_material() -> (String, String) {
        let ca_key = KeyPair::generate().unwrap_or_else(|e| panic!("生成 CA 密钥失败: {e}"));
        let mut ca_params = CertificateParams::default();
        let mut ca_dn = DistinguishedName::new();
        ca_dn.push(DnType::CommonName, "FOIMS Test Root CA");
        ca_params.distinguished_name = ca_dn;
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca_cert = ca_params
            .self_signed(&ca_key)
            .unwrap_or_else(|e| panic!("生成 CA 失败: {e}"));
        (ca_cert.pem(), ca_key.serialize_pem())
    }

    #[test]
    fn 构建证书_cn写入主题且由ca签发() {
        let req = minimal_request("box.example.com");
        let out = build_leaf_cert("box.example.com", &req, 30, vec![], test_ca_material())
            .unwrap_or_else(|e| panic!("构建证书失败: {e}"));
        assert!(out.cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(out.key_pem.starts_with("-----BEGIN PRIVATE KEY-----"));

        let Some(meta) = crate::listing::parse_cert_metadata(out.cert_pem.as_bytes()) else {
            panic!("生成的证书应可解析");
        };
        assert_eq!(meta.subject_cn.as_deref(), Some("box.example.com"));
        assert_eq!(
            meta.issuer_cn.as_deref(),
            Some("FOIMS Test Root CA"),
            "叶子证书应由所选 CA 签发"
        );
        let Some(nb) = meta.not_before else {
            panic!("缺 not_before")
        };
        let Some(na) = meta.not_after else {
            panic!("缺 not_after")
        };
        let days = (na - nb).num_days();
        assert!(
            (29..=31).contains(&days),
            "有效期应为 30 天左右，实际 {days}"
        );
    }

    #[test]
    fn 构建证书_san去重与cn回退() {
        // 无 SAN 时回退使用 CN（CN 是合法域名）
        let req = minimal_request("fallback.example.com");
        let out = build_leaf_cert(
            "fallback.example.com",
            &req,
            3650,
            vec![],
            test_ca_material(),
        )
        .unwrap_or_else(|e| panic!("构建证书失败: {e}"));
        let (sans, _) = extract_sans(&out.cert_pem);
        assert_eq!(
            sans.iter().filter(|s| s == &"fallback.example.com").count(),
            1,
            "CN 应作为回退 SAN 恰好出现一次: {sans:?}"
        );

        // 请求 SAN 与注入 SAN 合并去重
        let mut req = minimal_request("cn.example.com");
        req.subject_alt_names = Some(vec![
            "192.168.1.10".to_string(),
            "192.168.1.10".to_string(),
            "alt.example.com".to_string(),
        ]);
        let out = build_leaf_cert(
            "cn.example.com",
            &req,
            3650,
            vec![
                "alt.example.com".to_string(),
                "extra.example.com".to_string(),
            ],
            test_ca_material(),
        )
        .unwrap_or_else(|e| panic!("构建证书失败: {e}"));
        let (dns_sans, ip_sans) = extract_sans(&out.cert_pem);
        assert_eq!(
            dns_sans
                .iter()
                .filter(|s| s.as_str() == "alt.example.com")
                .count(),
            1,
            "重复 SAN 应去重"
        );
        assert!(
            dns_sans.contains(&"extra.example.com".to_string()),
            "注入的 SAN 应保留: {dns_sans:?}"
        );
        assert!(
            ip_sans.iter().any(|b| b.as_slice() == [192, 168, 1, 10]),
            "IP SAN 应保留: {ip_sans:?}"
        );
    }

    /// 从 PEM 证书中提取 SAN，返回 (DNS 名称列表, IP 地址字节列表)
    fn extract_sans(cert_pem: &str) -> (Vec<String>, Vec<Vec<u8>>) {
        let pems = pem::parse_many(cert_pem.as_bytes()).unwrap_or_default();
        let Some(block) = pems.iter().find(|p| p.tag() == "CERTIFICATE") else {
            panic!("应包含证书块");
        };
        let Ok((_, cert)) = x509_parser::parse_x509_certificate(block.contents()) else {
            panic!("证书应可解析");
        };
        let Ok(Some(san_ext)) = cert.subject_alternative_name() else {
            return (Vec::new(), Vec::new());
        };
        let mut dns = Vec::new();
        let mut ips = Vec::new();
        for gn in &san_ext.value.general_names {
            match gn {
                x509_parser::extensions::GeneralName::DNSName(d) => dns.push(d.to_string()),
                x509_parser::extensions::GeneralName::IPAddress(b) => ips.push(b.to_vec()),
                _ => {}
            }
        }
        (dns, ips)
    }

    /// 私钥写入临时目录：内容一致且权限为 0600，测试后清理
    #[test]
    fn 写入私钥文件_内容与权限() {
        let (dir, key_path) = temp_key_dir("write");
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("创建临时目录失败: {e}"));

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap_or_else(|e| panic!("构造测试运行时失败: {e}"));
        rt.block_on(write_key_file(&key_path, "SECRET-KEY-CONTENT"))
            .unwrap_or_else(|e| panic!("写入私钥失败: {e}"));

        let content =
            std::fs::read_to_string(&key_path).unwrap_or_else(|e| panic!("读取私钥失败: {e}"));
        assert_eq!(content, "SECRET-KEY-CONTENT");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&key_path)
                .unwrap_or_else(|e| panic!("读取元数据失败: {e}"))
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "私钥应仅属主可读写");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 构造进程 + 线程唯一的临时目录与私钥路径
    fn temp_key_dir(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "foims_x509_{tag}_test_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let key_path = dir.join("test.key");
        (dir, key_path)
    }

    /// 构造单线程 tokio 运行时（本 crate 的 tokio 未启用 macros 特性）
    fn with_runtime<F: std::future::Future>(fut: F) -> F::Output {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap_or_else(|e| panic!("构造测试运行时失败: {e}"));
        rt.block_on(fut)
    }

    /// 覆盖写入：已存在文件先删除后以 0600 新建，内容更新且权限不变
    #[test]
    fn 写入私钥文件_覆盖已存在文件() {
        let (dir, key_path) = temp_key_dir("overwrite");
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("创建临时目录失败: {e}"));

        with_runtime(async {
            write_key_file(&key_path, "OLD")
                .await
                .unwrap_or_else(|e| panic!("首次写入失败: {e}"));
            write_key_file(&key_path, "NEW")
                .await
                .unwrap_or_else(|e| panic!("覆盖写入失败: {e}"));
        });

        let content =
            std::fs::read_to_string(&key_path).unwrap_or_else(|e| panic!("读取私钥失败: {e}"));
        assert_eq!(content, "NEW");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&key_path)
                .unwrap_or_else(|e| panic!("读取元数据失败: {e}"))
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "覆盖后仍应仅属主可读写");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 请求校验：CN 长度、组织长度、SAN 条数与单条长度、国家代码（trim 后）
    #[test]
    fn 生成请求校验_长度与国家代码() {
        let mut req = minimal_request("gw.example.com");
        assert!(validate_generate_request(&req).is_ok());

        // CN 空 / 超 64 字符
        req.common_name = "   ".to_string();
        assert!(validate_generate_request(&req).is_err(), "空 CN 应被拒绝");
        req.common_name = "C".repeat(65);
        assert!(validate_generate_request(&req).is_err(), "超长 CN 应被拒绝");
        req.common_name = "C".repeat(64);
        assert!(validate_generate_request(&req).is_ok(), "64 字符 CN 应合法");

        // 组织超 64 字符
        req.organization = Some("O".repeat(65));
        assert!(validate_generate_request(&req).is_err());
        req.organization = Some("O".repeat(64));
        assert!(validate_generate_request(&req).is_ok());

        // 国家代码 trim 后长度必须为 2（与 CA 侧统一口径）
        req.country = Some(" C ".to_string());
        assert!(
            validate_generate_request(&req).is_err(),
            "trim 后 1 位应被拒绝"
        );
        req.country = Some(" CN".to_string());
        assert!(
            validate_generate_request(&req).is_ok(),
            "trim 后 2 位应合法"
        );

        // SAN 条数与单条长度
        req.country = None;
        req.subject_alt_names = Some(vec!["a.example.com".to_string(); 101]);
        assert!(
            validate_generate_request(&req).is_err(),
            "超过 100 条 SAN 应被拒绝"
        );
        req.subject_alt_names = Some(vec!["a.example.com".to_string(); 100]);
        assert!(validate_generate_request(&req).is_ok(), "100 条 SAN 应合法");
        req.subject_alt_names = Some(vec!["s".repeat(254)]);
        assert!(
            validate_generate_request(&req).is_err(),
            "单条超 253 字符应被拒绝"
        );
        req.subject_alt_names = Some(vec!["s".repeat(253)]);
        assert!(
            validate_generate_request(&req).is_ok(),
            "单条 253 字符应合法"
        );
    }

    /// SAN 全部无效且 CN 不可作为 SAN 时拒绝生成（不再产出无 SAN 的废证书）
    #[test]
    fn 构建证书_san全部无效且cn不可用时拒绝() {
        // CN 与全部 SAN 均为非 ASCII（IA5 域名不可编码，parse_san 返回 None）
        let cn = "例えサーバー";
        let mut req = minimal_request(cn);
        req.subject_alt_names = Some(vec!["例え.jp".to_string(), "中文.example".to_string()]);
        let err = build_leaf_cert(cn, &req, 365, vec![], test_ca_material())
            .err()
            .unwrap_or_else(|| panic!("无可用 SAN 应被拒绝"));
        match err {
            CertManagerError::Validation(m) => {
                assert_eq!(m.key(), "server.certificate.san_invalid")
            }
            other => panic!("应为校验错误，实际 {other}"),
        }
    }

    /// 同名冲突时 unique_cert_stem_with_timestamp 追加递增序号（固定时间戳保证确定性）
    #[test]
    fn 唯一基础名_冲突时追加序号() {
        let dir = std::env::temp_dir().join(format!(
            "foims_x509_stem_test_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("创建临时目录失败: {e}"));
        let dir_str = dir.to_string_lossy().to_string();
        let fixed_ts = 1761234567890i64;

        let first = with_runtime(crate::unique_cert_stem_with_timestamp(
            &dir_str, "create_", fixed_ts,
        ));
        assert_eq!(first, format!("create_{fixed_ts}_cert"));
        // 占用 first 的 .pem，同一时间戳内再次生成应追加递增序号
        std::fs::write(dir.join(format!("{first}.pem")), b"x")
            .unwrap_or_else(|e| panic!("占位写入失败: {e}"));
        let second = with_runtime(crate::unique_cert_stem_with_timestamp(
            &dir_str, "create_", fixed_ts,
        ));
        assert_eq!(second, format!("create_{fixed_ts}_1_cert"));

        // 仅 .key 被占用同样视为冲突
        let _ = std::fs::remove_file(dir.join(format!("{first}.pem")));
        std::fs::write(dir.join(format!("{first}.key")), b"x")
            .unwrap_or_else(|e| panic!("占位写入失败: {e}"));
        let third = with_runtime(crate::unique_cert_stem_with_timestamp(
            &dir_str, "create_", fixed_ts,
        ));
        assert_eq!(third, format!("create_{fixed_ts}_1_cert"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 生成请求_serde缺省字段为none() {
        let req: GenerateCertRequest =
            serde_json::from_str(r#"{"common_name": "only.example.com"}"#)
                .unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(req.common_name, "only.example.com");
        assert!(req.organization.is_none());
        assert!(req.validity_days.is_none());
        assert!(req.subject_alt_names.is_none());
        assert!(req.ca_id.is_none(), "ca_id 缺省为 None");
    }

    /// CA 签发路径：叶子证书的签发者应为 CA 的 CN
    #[test]
    fn 构建证书_ca签发叶子() {
        let (ca_cert_pem, ca_key_pem) = test_ca_material();

        let req = minimal_request("leaf.example.com");
        let out = build_leaf_cert(
            "leaf.example.com",
            &req,
            365,
            vec![],
            (ca_cert_pem, ca_key_pem),
        )
        .unwrap_or_else(|e| panic!("构建证书失败: {e}"));

        let Some(meta) = crate::listing::parse_cert_metadata(out.cert_pem.as_bytes()) else {
            panic!("生成的证书应可解析");
        };
        assert_eq!(meta.subject_cn.as_deref(), Some("leaf.example.com"));
        assert_eq!(
            meta.issuer_cn.as_deref(),
            Some("FOIMS Test Root CA"),
            "叶子证书应由 CA 签发"
        );
    }
}
