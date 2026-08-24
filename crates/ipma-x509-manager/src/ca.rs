//! 站点根 CA 管理：生成、导入、状态查询与导出。
//!
//! CA 固定存储于 `/etc/ssl/ipma-ca/`（ca.pem + 可选 ca.key）：
//! - 自生成或导入的 CA 同时具备证书与私钥，可为本站签发服务器证书；
//! - 随服务器证书一并导入的 CA 仅有证书（ca.key 不存在），只能导出给
//!   终端信任，不能用于签发。
//!
//! CA 证书本身是公开数据，私钥绝不允许通过任何接口外发。

use std::path::{Path, PathBuf};

use ipma_common::msg;
use rcgen::{BasicConstraints, DistinguishedName, DnType, IsCa, KeyPair, KeyUsagePurpose};
use serde::{Deserialize, Serialize};

use crate::error::CertManagerError;
use crate::generate::write_key_file;

/// 站点 CA 存储目录
pub const CA_DIR: &str = "/etc/ssl/ipma-ca";
const CA_CERT_FILE: &str = "ca.pem";
const CA_KEY_FILE: &str = "ca.key";

const DEFAULT_CA_VALIDITY_DAYS: i64 = 7300;
const MAX_CA_VALIDITY_DAYS: i64 = 36500;

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateCaRequest {
    pub common_name: String,
    pub organization: Option<String>,
    pub organizational_unit: Option<String>,
    pub country: Option<String>,
    pub state: Option<String>,
    pub locality: Option<String>,
    /// 有效期（天），缺省 7300（约 20 年）
    pub validity_days: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CaStatus {
    pub available: bool,
    /// 私钥是否在本机（决定能否签发证书）
    pub has_key: bool,
    pub subject_cn: Option<String>,
    pub issuer_cn: Option<String>,
    pub not_before: Option<chrono::DateTime<chrono::Utc>>,
    pub not_after: Option<chrono::DateTime<chrono::Utc>>,
    pub days_remaining: Option<i64>,
}

fn ca_cert_path() -> PathBuf {
    Path::new(CA_DIR).join(CA_CERT_FILE)
}

fn ca_key_path() -> PathBuf {
    Path::new(CA_DIR).join(CA_KEY_FILE)
}

/// 读取 CA 证书 PEM；不存在时返回 None
async fn read_ca_cert() -> Option<Vec<u8>> {
    tokio::fs::read(ca_cert_path()).await.ok()
}

/// 校验证书是 CA（BasicConstraints CA:TRUE）；
/// 提供私钥时一并校验与证书公钥匹配（防止导入错配的证书/私钥对）
fn validate_ca(cert_pem: &[u8], key_pem: Option<&str>) -> Result<(), CertManagerError> {
    let invalid = || CertManagerError::Validation(msg("server.certificate.ca_invalid"));
    let unparsable = || CertManagerError::Validation(msg("server.certificate.cert_file_invalid"));

    let pems = pem::parse_many(cert_pem).map_err(|_| unparsable())?;
    let Some(block) = pems.iter().find(|p| p.tag() == "CERTIFICATE") else {
        return Err(unparsable());
    };
    let (_, cert) =
        x509_parser::parse_x509_certificate(block.contents()).map_err(|_| unparsable())?;

    let is_ca = cert
        .basic_constraints()
        .map_err(|_| invalid())?
        .is_some_and(|ext| ext.value.ca);
    if !is_ca {
        return Err(invalid());
    }

    if let Some(key_pem) = key_pem {
        let key_pair = KeyPair::from_pem(key_pem).map_err(|_| invalid())?;
        let cert_key: &[u8] = cert
            .tbs_certificate
            .subject_pki
            .subject_public_key
            .data
            .as_ref();
        if cert_key != key_pair.public_key_raw() {
            return Err(invalid());
        }
    }
    Ok(())
}

/// 组装 DN（与叶子证书共用同一套字段约定）
fn build_dn(req: &GenerateCaRequest) -> DistinguishedName {
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, req.common_name.trim());
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
    dn
}

/// 生成自签名根 CA 并覆盖写入 CA 目录，返回证书路径。
///
/// x509v3 扩展：BasicConstraints CA:TRUE（无路径长度限制）、
/// KeyUsage 含 keyCertSign/cRLSign，不含 EKU（CA 不做终端认证）。
pub async fn generate_ca(req: GenerateCaRequest) -> Result<PathBuf, CertManagerError> {
    let common_name = req.common_name.trim().to_string();
    if common_name.is_empty() {
        return Err(CertManagerError::Validation(msg(
            "server.certificate.common_name_required",
        )));
    }
    if let Some(country) = &req.country
        && country.trim().len() != 2
    {
        return Err(CertManagerError::Validation(msg(
            "server.certificate.country_invalid",
        )));
    }
    let validity_days = req
        .validity_days
        .map_or(DEFAULT_CA_VALIDITY_DAYS, i64::from)
        .clamp(1, MAX_CA_VALIDITY_DAYS);

    tokio::fs::create_dir_all(CA_DIR).await.map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.write_failed").with("error", e.to_string()),
        )
    })?;

    let generation = tokio::task::spawn_blocking(move || {
        let key_pair = KeyPair::generate().map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.generate_failed").with("error", e.to_string()),
            )
        })?;

        let now = time::OffsetDateTime::now_utc();
        let mut params = rcgen::CertificateParams::default();
        params.not_before = now;
        params.not_after = now + time::Duration::days(validity_days);
        params.distinguished_name = build_dn(&req);
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        // 根 CA 的密钥用途：签发证书与吊销列表（RFC 5280 要求 keyCertSign）
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];

        let cert = params.self_signed(&key_pair).map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.generate_failed").with("error", e.to_string()),
            )
        })?;

        Ok::<_, CertManagerError>((cert.pem(), key_pair.serialize_pem()))
    })
    .await
    .map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.generate_failed").with("error", e.to_string()),
        )
    })??;

    write_key_file(&ca_key_path(), &generation.1).await?;
    tokio::fs::write(ca_cert_path(), &generation.0)
        .await
        .map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;

    Ok(ca_cert_path())
}

/// 导入已有 CA（证书 + 私钥），覆盖写入 CA 目录。
///
/// 校验：证书可解析、BasicConstraints CA:TRUE、私钥可解析且与证书公钥匹配。
pub async fn import_ca(cert_pem: Vec<u8>, key_pem: Vec<u8>) -> Result<PathBuf, CertManagerError> {
    if cert_pem.is_empty() || key_pem.is_empty() {
        return Err(CertManagerError::Validation(msg(
            "server.certificate.ca_key_required",
        )));
    }
    let key_pem = String::from_utf8_lossy(&key_pem).to_string();
    // 解析与匹配校验移入阻塞线程（KeyPair::from_pem 涉及密钥计算）
    let validated = {
        let cert_for_check = cert_pem.clone();
        let key_for_check = key_pem.clone();
        tokio::task::spawn_blocking(move || validate_ca(&cert_for_check, Some(&key_for_check)))
            .await
            .map_err(|e| {
                CertManagerError::Internal(
                    msg("server.certificate.generate_failed").with("error", e.to_string()),
                )
            })?
    };
    validated?;

    tokio::fs::create_dir_all(CA_DIR).await.map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.write_failed").with("error", e.to_string()),
        )
    })?;
    // 证书与私钥成对覆盖，避免新旧错配
    write_key_file(&ca_key_path(), &key_pem).await?;
    tokio::fs::write(ca_cert_path(), &cert_pem)
        .await
        .map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;

    Ok(ca_cert_path())
}

/// 仅导入 CA 证书（无私钥）：供"随服务器证书一并导入 CA"场景，
/// 写入 ca.pem 并删除既有 ca.key（防止旧私钥配新证书造成错签）。
pub async fn set_ca_cert_only(cert_pem: Vec<u8>) -> Result<(), CertManagerError> {
    if cert_pem.is_empty() {
        return Err(CertManagerError::Validation(msg(
            "server.certificate.cert_file_missing",
        )));
    }
    let validated = {
        let cert_for_check = cert_pem.clone();
        tokio::task::spawn_blocking(move || validate_ca(&cert_for_check, None))
            .await
            .map_err(|e| {
                CertManagerError::Internal(
                    msg("server.certificate.generate_failed").with("error", e.to_string()),
                )
            })?
    };
    validated?;

    tokio::fs::create_dir_all(CA_DIR).await.map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.write_failed").with("error", e.to_string()),
        )
    })?;
    tokio::fs::write(ca_cert_path(), &cert_pem)
        .await
        .map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;
    // 清除可能存在的旧私钥，确保 CA 状态一致（cert-only 不可签发）
    let _ = tokio::fs::remove_file(ca_key_path()).await;
    Ok(())
}

/// 查询 CA 状态（不存在时 available=false）
pub async fn ca_status() -> CaStatus {
    let mut status = CaStatus {
        available: false,
        has_key: false,
        subject_cn: None,
        issuer_cn: None,
        not_before: None,
        not_after: None,
        days_remaining: None,
    };
    let Some(cert_pem) = read_ca_cert().await else {
        return status;
    };
    let Some(meta) = crate::listing::parse_cert_metadata(&cert_pem) else {
        return status;
    };
    status.available = true;
    status.has_key = tokio::fs::try_exists(ca_key_path()).await.unwrap_or(false);
    status.days_remaining = meta
        .not_after
        .map(|na| (na - chrono::Utc::now()).num_days());
    status.subject_cn = meta.subject_cn;
    status.issuer_cn = meta.issuer_cn;
    status.not_before = meta.not_before;
    status.not_after = meta.not_after;
    status
}

/// 读取 CA 证书 PEM（导出用）；不存在返回 NotFound
pub async fn read_ca_cert_pem() -> Result<Vec<u8>, CertManagerError> {
    read_ca_cert()
        .await
        .ok_or_else(|| CertManagerError::NotFound(msg("server.certificate.ca_not_found")))
}

/// 读取 CA 证书 DER（Windows 导入格式）；不存在返回 NotFound
pub async fn read_ca_cert_der() -> Result<Vec<u8>, CertManagerError> {
    let pem_data = read_ca_cert_pem().await?;
    let pems = pem::parse_many(&pem_data)
        .map_err(|_| CertManagerError::Validation(msg("server.certificate.cert_file_invalid")))?;
    let block = pems
        .iter()
        .find(|p| p.tag() == "CERTIFICATE")
        .ok_or_else(|| CertManagerError::Validation(msg("server.certificate.cert_file_invalid")))?;
    Ok(block.contents().to_vec())
}

/// 读取 CA 证书与私钥 PEM（签发叶子证书用）；任一缺失返回 None
pub(crate) async fn load_ca_material() -> Option<(String, String)> {
    let cert = tokio::fs::read_to_string(ca_cert_path()).await.ok()?;
    let key = tokio::fs::read_to_string(ca_key_path()).await.ok()?;
    Some((cert, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造最小 CA 生成请求
    fn ca_request(cn: &str) -> GenerateCaRequest {
        GenerateCaRequest {
            common_name: cn.to_string(),
            organization: None,
            organizational_unit: None,
            country: None,
            state: None,
            locality: None,
            validity_days: Some(30),
        }
    }

    #[test]
    fn ca请求_serde缺省字段为none() {
        let req: GenerateCaRequest = serde_json::from_str(r#"{"common_name": "IPMA Root CA"}"#)
            .unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(req.common_name, "IPMA Root CA");
        assert!(req.validity_days.is_none());
    }

    /// CA 属性校验：叶子证书（无 BasicConstraints CA:TRUE）应被拒绝
    #[test]
    fn ca校验_非ca证书被拒绝() {
        let key_pair = rcgen::KeyPair::generate().unwrap_or_else(|e| panic!("生成密钥失败: {e}"));
        let mut params = rcgen::CertificateParams::default();
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "leaf.example.com");
        params.distinguished_name = dn;
        let leaf = params
            .self_signed(&key_pair)
            .unwrap_or_else(|e| panic!("生成证书失败: {e}"));

        assert!(
            validate_ca(leaf.pem().as_bytes(), None).is_err(),
            "叶子证书不应通过 CA 校验"
        );
    }

    /// CA 属性校验：显式 CA:TRUE 证书应通过
    #[test]
    fn ca校验_ca证书通过() {
        let key_pair = rcgen::KeyPair::generate().unwrap_or_else(|e| panic!("生成密钥失败: {e}"));
        let mut params = rcgen::CertificateParams::default();
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "Test Root CA");
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca = params
            .self_signed(&key_pair)
            .unwrap_or_else(|e| panic!("生成证书失败: {e}"));

        assert!(validate_ca(ca.pem().as_bytes(), None).is_ok());
    }

    /// 私钥与证书公钥匹配校验：另一把钥匙应被拒绝
    #[test]
    fn 私钥匹配_错配私钥被拒绝() {
        let ca_key = rcgen::KeyPair::generate().unwrap_or_else(|e| panic!("生成密钥失败: {e}"));
        let mut params = rcgen::CertificateParams::default();
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "Match Root CA");
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca = params
            .self_signed(&ca_key)
            .unwrap_or_else(|e| panic!("生成证书失败: {e}"));
        let ca_pem = ca.pem();

        assert!(validate_ca(ca_pem.as_bytes(), Some(&ca_key.serialize_pem())).is_ok());

        let other_key = rcgen::KeyPair::generate().unwrap_or_else(|e| panic!("生成密钥失败: {e}"));
        assert!(validate_ca(ca_pem.as_bytes(), Some(&other_key.serialize_pem())).is_err());
    }

    /// DN 组装：空可选字段被跳过
    #[test]
    fn dn组装_可选字段过滤() {
        let req = ca_request("DN Root CA");
        let _ = build_dn(&req); // 不应 panic
    }
}
