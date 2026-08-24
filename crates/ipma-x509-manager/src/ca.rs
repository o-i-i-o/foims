//! 站点根 CA 与导入 CA 池管理：生成、导入、状态查询与导出。
//!
//! CA 存储分两处：
//! - 站点根 CA：`/etc/ssl/ipma-ca/`（ca.pem + 可选 ca.key），由"生成CA"写入，
//!   是程序 HTTPS 与终端信任的默认签发 CA；
//! - 导入 CA 池：`/etc/ssl/ipma-import-cas/{id}/`（ca.pem + ca.key），
//!   由"导入CA"写入，仅作为证书签发的备选 CA，不改变站点根 CA。
//!
//! 证书生成强制由 CA 签发（根 CA 或导入 CA 池中任选，无 CA 时拒绝生成），
//! 不再提供自签名回退。
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
/// 导入 CA 池目录（每个 CA 独立子目录：ca.pem + ca.key）
pub const IMPORT_CA_DIR: &str = "/etc/ssl/ipma-import-cas";
/// 站点根 CA 在 CA 列表中的固定标识
pub const ROOT_CA_ID: &str = "root";

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

/// CA 列表项（证书生成弹窗的 CA 下拉框数据源）
#[derive(Debug, Serialize, Deserialize)]
pub struct CaInfo {
    /// CA 标识：根 CA 固定为 "root"，导入 CA 为其目录名
    pub id: String,
    /// CA 主题 CN
    pub name: Option<String>,
    /// 来源："root"（自生成根 CA）/ "imported"（导入 CA）
    pub source: String,
    /// 私钥是否在本机（无私钥的 CA 不能签发）
    pub has_key: bool,
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

fn import_ca_dir(id: &str) -> PathBuf {
    Path::new(IMPORT_CA_DIR).join(id)
}

/// CA 标识只允许出现字母数字与连字符/下划线，防止路径穿越
fn valid_ca_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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

/// 导入已有 CA（证书 + 私钥），写入导入 CA 池的独立子目录，返回该目录。
///
/// 校验：证书可解析、BasicConstraints CA:TRUE、私钥可解析且与证书公钥匹配。
/// 导入 CA 不覆盖站点根 CA；同秒内多次导入以序号后缀区分目录。
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

    tokio::fs::create_dir_all(IMPORT_CA_DIR)
        .await
        .map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;

    // 目录名：import_{时间戳}，已存在时追加序号，保证重复导入互不覆盖
    let timestamp = chrono::Utc::now().timestamp();
    let mut id = format!("import_{timestamp}");
    let mut seq = 1;
    while tokio::fs::try_exists(import_ca_dir(&id))
        .await
        .unwrap_or(false)
    {
        id = format!("import_{timestamp}_{seq}");
        seq += 1;
    }
    let target_dir = import_ca_dir(&id);

    tokio::fs::create_dir_all(&target_dir).await.map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.write_failed").with("error", e.to_string()),
        )
    })?;
    // 证书与私钥成对写入，避免新旧错配
    write_key_file(&target_dir.join(CA_KEY_FILE), &key_pem).await?;
    tokio::fs::write(target_dir.join(CA_CERT_FILE), &cert_pem)
        .await
        .map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;

    Ok(target_dir)
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

/// 读取 CA 证书 PEM（导出用，仅站点根 CA）；不存在返回 NotFound
pub async fn read_ca_cert_pem() -> Result<Vec<u8>, CertManagerError> {
    read_ca_cert()
        .await
        .ok_or_else(|| CertManagerError::NotFound(msg("server.certificate.ca_not_found")))
}

/// CA 列表：站点根 CA（自生成）在前，导入 CA 池按目录名在后。
/// 根 CA 不存在时列表可能只含导入 CA（或为空）。
pub async fn list_cas() -> Vec<CaInfo> {
    let mut list = Vec::new();

    let root = ca_status().await;
    if root.available {
        list.push(CaInfo {
            id: ROOT_CA_ID.to_string(),
            name: root.subject_cn.clone(),
            source: "root".to_string(),
            has_key: root.has_key,
            not_before: root.not_before,
            not_after: root.not_after,
            days_remaining: root.days_remaining,
        });
    }

    let Ok(mut entries) = tokio::fs::read_dir(IMPORT_CA_DIR).await else {
        return list;
    };
    let mut imported = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let id = entry.file_name().to_string_lossy().to_string();
        if !valid_ca_id(&id) || id == ROOT_CA_ID {
            continue;
        }
        let cert_path = import_ca_dir(&id).join(CA_CERT_FILE);
        let Some(cert_pem) = tokio::fs::read(&cert_path).await.ok() else {
            continue;
        };
        let Some(meta) = crate::listing::parse_cert_metadata(&cert_pem) else {
            continue;
        };
        let has_key = tokio::fs::try_exists(import_ca_dir(&id).join(CA_KEY_FILE))
            .await
            .unwrap_or(false);
        imported.push(CaInfo {
            id,
            name: meta.subject_cn,
            source: "imported".to_string(),
            has_key,
            not_before: meta.not_before,
            not_after: meta.not_after,
            days_remaining: meta
                .not_after
                .map(|na| (na - chrono::Utc::now()).num_days()),
        });
    }
    imported.sort_by(|a, b| a.id.cmp(&b.id));
    list.extend(imported);
    list
}

/// 按 CA 标识读取证书与私钥 PEM（签发叶子证书用）。
/// id 为空或 "root" 取站点根 CA；否则取导入 CA 池对应目录。
/// 任一文件缺失返回 None（由调用方决定报错语义）。
pub(crate) async fn load_ca_material_by_id(id: Option<&str>) -> Option<(String, String)> {
    let id = id.unwrap_or(ROOT_CA_ID).trim();
    let (cert_path, key_path) = if id.is_empty() || id == ROOT_CA_ID {
        (ca_cert_path(), ca_key_path())
    } else if valid_ca_id(id) {
        (
            import_ca_dir(id).join(CA_CERT_FILE),
            import_ca_dir(id).join(CA_KEY_FILE),
        )
    } else {
        return None;
    };
    let cert = tokio::fs::read_to_string(cert_path).await.ok()?;
    let key = tokio::fs::read_to_string(key_path).await.ok()?;
    Some((cert, key))
}

/// 按 CA 标识检查证书文件是否存在（生成证书时区分"CA 不存在"与"无私钥"）
pub(crate) async fn ca_cert_exists(id: Option<&str>) -> bool {
    let id = id.unwrap_or(ROOT_CA_ID).trim();
    let path = if id.is_empty() || id == ROOT_CA_ID {
        ca_cert_path()
    } else if valid_ca_id(id) {
        import_ca_dir(id).join(CA_CERT_FILE)
    } else {
        return false;
    };
    tokio::fs::try_exists(path).await.unwrap_or(false)
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
