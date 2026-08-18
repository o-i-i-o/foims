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
