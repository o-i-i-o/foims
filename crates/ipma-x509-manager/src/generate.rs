//! 自签名证书生成（rcgen）。

use std::path::PathBuf;

use ipma_common::msg;
use rcgen::{
    CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, KeyPair,
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

/// 生成自签名证书并写入生成目录，返回证书文件路径。
///
/// `extra_sans` 为调用方注入的额外 SAN（如服务器 public_url 的主机名），
/// 与请求中的 SAN 合并；两者均为空时回退使用 common_name。
pub async fn generate_self_signed(
    req: GenerateCertRequest,
    extra_sans: Vec<String>,
) -> Result<PathBuf, CertManagerError> {
    let common_name = req.common_name.trim().to_string();
    if common_name.is_empty() {
        return Err(CertManagerError::Validation(msg(
            "server.certificate.common_name_required",
        )));
    }
    if let Some(country) = &req.country
        && country.len() != 2
    {
        return Err(CertManagerError::Validation(msg(
            "server.certificate.country_invalid",
        )));
    }
    let validity_days = req
        .validity_days
        .map_or(DEFAULT_VALIDITY_DAYS, i64::from)
        .clamp(1, MAX_VALIDITY_DAYS);

    tokio::fs::create_dir_all(GENERATED_CERTS_DIR)
        .await
        .map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;

    let timestamp = chrono::Utc::now().timestamp();
    let base_name = format!("create_{timestamp}_cert");
    let cert_path = PathBuf::from(GENERATED_CERTS_DIR).join(format!("{base_name}.pem"));
    let key_path = PathBuf::from(GENERATED_CERTS_DIR).join(format!("{base_name}.key"));

    let generation = tokio::task::spawn_blocking(move || {
        build_self_signed_cert(&common_name, &req, validity_days, extra_sans)
    })
    .await
    .map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.generate_failed").with("error", e.to_string()),
        )
    })??;

    write_key_file(&key_path, &generation.key_pem).await?;
    tokio::fs::write(&cert_path, generation.cert_pem)
        .await
        .map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;

    Ok(cert_path)
}

struct GeneratedPem {
    cert_pem: String,
    key_pem: String,
}

fn build_self_signed_cert(
    common_name: &str,
    req: &GenerateCertRequest,
    validity_days: i64,
    extra_sans: Vec<String>,
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

    // SAN：请求条目 + 调用方注入（public_url 主机），去重；均为空时回退 CN
    let mut san_entries: Vec<String> = req.subject_alt_names.clone().unwrap_or_default();
    san_entries.extend(extra_sans);
    let mut sans: Vec<SanType> = Vec::new();
    for entry in &san_entries {
        if let Some(san) = parse_san(entry)
            && !sans.contains(&san)
        {
            sans.push(san);
        }
    }
    if sans.is_empty()
        && let Some(san) = parse_san(common_name)
    {
        sans.push(san);
    }
    params.subject_alt_names = sans;

    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];

    let cert = params.self_signed(&key_pair).map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.generate_failed").with("error", e.to_string()),
        )
    })?;

    Ok(GeneratedPem {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
    })
}

/// 写入私钥文件并设置 0600 权限（仅属主可读写）
pub(crate) async fn write_key_file(path: &PathBuf, content: &str) -> Result<(), CertManagerError> {
    tokio::fs::write(path, content).await.map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.write_failed").with("error", e.to_string()),
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms).map_err(|e| {
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
        }
    }

    #[test]
    fn 构建自签名证书_cn写入主题与签发者() {
        let req = minimal_request("box.example.com");
        let out = build_self_signed_cert("box.example.com", &req, 30, vec![])
            .unwrap_or_else(|e| panic!("构建证书失败: {e}"));
        assert!(out.cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(out.key_pem.starts_with("-----BEGIN PRIVATE KEY-----"));

        let Some(meta) = crate::listing::parse_cert_metadata(out.cert_pem.as_bytes()) else {
            panic!("生成的证书应可解析");
        };
        assert_eq!(meta.subject_cn.as_deref(), Some("box.example.com"));
        assert_eq!(
            meta.issuer_cn.as_deref(),
            Some("box.example.com"),
            "自签名签发者即自身"
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
    fn 构建自签名证书_san去重与cn回退() {
        // 无 SAN 时回退使用 CN（CN 是合法域名）
        let req = minimal_request("fallback.example.com");
        let out = build_self_signed_cert("fallback.example.com", &req, 3650, vec![])
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
        let out = build_self_signed_cert(
            "cn.example.com",
            &req,
            3650,
            vec![
                "alt.example.com".to_string(),
                "extra.example.com".to_string(),
            ],
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
        // 进程 + 线程唯一的临时子目录，测试后清理
        let dir = std::env::temp_dir().join(format!(
            "ipma_x509_write_test_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("创建临时目录失败: {e}"));
        let key_path = dir.join("test.key");

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

    #[test]
    fn 生成请求_serde缺省字段为none() {
        let req: GenerateCertRequest =
            serde_json::from_str(r#"{"common_name": "only.example.com"}"#)
                .unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(req.common_name, "only.example.com");
        assert!(req.organization.is_none());
        assert!(req.validity_days.is_none());
        assert!(req.subject_alt_names.is_none());
    }
}
