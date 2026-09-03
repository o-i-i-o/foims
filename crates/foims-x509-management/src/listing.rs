//! 证书清点：扫描生成/导入目录，解析证书元数据（主题/有效期/剩余天数）。

use std::path::Path;

use chrono::{DateTime, Utc};
use foims_common::msg;
use serde::{Deserialize, Serialize};

use crate::error::CertManagerError;
use crate::{GENERATED_CERTS_DIR, IMPORTED_CERTS_DIR};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CertKind {
    Generated,
    Imported,
}

impl CertKind {
    /// 从路径参数解析（generated / imported）
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "generated" => Some(Self::Generated),
            "imported" => Some(Self::Imported),
            _ => None,
        }
    }

    pub fn dir(self) -> &'static str {
        match self {
            Self::Generated => GENERATED_CERTS_DIR,
            Self::Imported => IMPORTED_CERTS_DIR,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CertFileInfo {
    /// 文件基础名（如 create_1761234567_cert）
    pub file_stem: String,
    pub cert_filename: String,
    pub key_filename: Option<String>,
    pub subject_cn: Option<String>,
    pub issuer_cn: Option<String>,
    pub not_before: Option<DateTime<Utc>>,
    pub not_after: Option<DateTime<Utc>>,
    pub days_remaining: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CertificateInventory {
    pub generated: Vec<CertFileInfo>,
    pub imported: Vec<CertFileInfo>,
    /// 站点根 CA 状态（无 CA 时 available=false）
    pub ca: crate::ca::CaStatus,
    /// 可选签发 CA 列表（站点根 CA + 导入 CA 池，证书生成弹窗下拉数据源）
    pub cas: Vec<crate::ca::CaInfo>,
}

/// 解析出的证书元数据
#[derive(Debug, Default)]
pub(crate) struct CertMetadata {
    pub subject_cn: Option<String>,
    pub issuer_cn: Option<String>,
    pub not_before: Option<DateTime<Utc>>,
    pub not_after: Option<DateTime<Utc>>,
}

/// 解析证书 PEM 的元数据；失败返回 None（由调用方决定是否视为无效文件）
pub(crate) fn parse_cert_metadata(cert_pem: &[u8]) -> Option<CertMetadata> {
    let pems = pem::parse_many(cert_pem).ok()?;
    let cert_pem_block = pems.iter().find(|p| p.tag() == "CERTIFICATE")?;

    let (_, cert) = x509_parser::parse_x509_certificate(cert_pem_block.contents()).ok()?;

    let subject_cn = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|attr| attr.as_str().ok())
        .map(str::to_string);
    let issuer_cn = cert
        .issuer()
        .iter_common_name()
        .next()
        .and_then(|attr| attr.as_str().ok())
        .map(str::to_string);

    let not_before =
        DateTime::from_timestamp(cert.tbs_certificate.validity.not_before.timestamp(), 0);
    let not_after =
        DateTime::from_timestamp(cert.tbs_certificate.validity.not_after.timestamp(), 0);

    Some(CertMetadata {
        subject_cn,
        issuer_cn,
        not_before,
        not_after,
    })
}

/// 扫描单个目录，按 mtime 倒序返回证书信息
async fn scan_dir(dir: &str) -> Result<Vec<CertFileInfo>, CertManagerError> {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(CertManagerError::Internal(
                msg("server.certificate.list_failed").with("error", e.to_string()),
            ));
        }
    };

    // (stem, cert 文件名, mtime)
    let mut cert_files: Vec<(String, String, std::time::SystemTime)> = Vec::new();
    let mut key_stems: Vec<String> = Vec::new();

    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.list_failed").with("error", e.to_string()),
        )
    })? {
        let path = entry.path();
        let Some(filename) = path.file_name().and_then(|f| f.to_str()) else {
            continue;
        };
        let Ok(metadata) = entry.metadata().await else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }

        if let Some(stem) = filename.strip_suffix(".pem") {
            let mtime = metadata
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            cert_files.push((stem.to_string(), filename.to_string(), mtime));
        } else if filename.ends_with(".key")
            && let Some(stem) = filename.strip_suffix(".key")
        {
            key_stems.push(stem.to_string());
        }
    }

    cert_files.sort_by_key(|(_, _, mtime)| std::cmp::Reverse(*mtime));

    let mut infos = Vec::with_capacity(cert_files.len());
    for (stem, cert_filename, _) in cert_files {
        let cert_path = Path::new(dir).join(&cert_filename);
        let metadata = match tokio::fs::read(&cert_path).await {
            Ok(data) => parse_cert_metadata(&data).unwrap_or_default(),
            // 目录扫描与读取之间的竞态（并发删除）按无元数据降级；
            // 其他 IO 故障留痕，不在列表中静默伪装成「无 CN/有效期」
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Default::default(),
            Err(e) => {
                foims_common::log_warn!(
                    "log.certificate.cert_read_failed",
                    path = cert_path.display().to_string(),
                    error = e
                );
                Default::default()
            }
        };

        let key_filename = if key_stems.contains(&stem) {
            Some(format!("{stem}.key"))
        } else {
            None
        };
        let days_remaining = metadata.not_after.map(|na| {
            let now = Utc::now();
            (na - now).num_days()
        });

        infos.push(CertFileInfo {
            file_stem: stem,
            cert_filename,
            key_filename,
            subject_cn: metadata.subject_cn,
            issuer_cn: metadata.issuer_cn,
            not_before: metadata.not_before,
            not_after: metadata.not_after,
            days_remaining,
        });
    }

    Ok(infos)
}

/// 清点生成目录与导入目录的全部证书，并附带站点 CA 状态与可选 CA 列表
pub async fn list_certificates() -> Result<CertificateInventory, CertManagerError> {
    Ok(CertificateInventory {
        generated: scan_dir(GENERATED_CERTS_DIR).await?,
        imported: scan_dir(IMPORTED_CERTS_DIR).await?,
        ca: crate::ca::ca_status().await,
        cas: crate::ca::list_cas().await,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind解析_合法值与非法值() {
        assert_eq!(CertKind::parse("generated"), Some(CertKind::Generated));
        assert_eq!(CertKind::parse("imported"), Some(CertKind::Imported));
        assert_eq!(CertKind::parse("Generated"), None, "大小写敏感");
        assert_eq!(CertKind::parse("other"), None);
        assert_eq!(CertKind::parse(""), None);
    }

    #[test]
    fn kind目录映射_与crate常量一致() {
        assert_eq!(CertKind::Generated.dir(), crate::GENERATED_CERTS_DIR);
        assert_eq!(CertKind::Generated.dir(), "/etc/ssl/foims-certs");
        assert_eq!(CertKind::Imported.dir(), crate::IMPORTED_CERTS_DIR);
        assert_eq!(CertKind::Imported.dir(), "/etc/ssl/foims-import-certs");
    }

    #[test]
    fn kind序列化_小写往返() {
        for (kind, text) in [
            (CertKind::Generated, "\"generated\""),
            (CertKind::Imported, "\"imported\""),
        ] {
            let json = serde_json::to_string(&kind).unwrap_or_else(|e| panic!("序列化失败: {e}"));
            assert_eq!(json, text);
            let back: CertKind =
                serde_json::from_str(&json).unwrap_or_else(|e| panic!("反序列化失败: {e}"));
            assert_eq!(back, kind);
        }
        // 非法值反序列化失败
        assert!(serde_json::from_str::<CertKind>("\"unknown\"").is_err());
    }

    /// 用 rcgen 在内存中生成一张自签名证书（不触碰文件系统）
    fn make_test_cert_pem(cn: &str) -> Vec<u8> {
        let key_pair = rcgen::KeyPair::generate().unwrap_or_else(|e| panic!("生成密钥失败: {e}"));
        let mut params = rcgen::CertificateParams::default();
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, cn);
        params.distinguished_name = dn;
        let cert = params
            .self_signed(&key_pair)
            .unwrap_or_else(|e| panic!("生成证书失败: {e}"));
        cert.pem().into_bytes()
    }

    #[test]
    fn 证书元数据解析_提取cn与有效期() {
        let pem = make_test_cert_pem("test.example.com");
        let Some(meta) = parse_cert_metadata(&pem) else {
            panic!("有效证书应解析出元数据");
        };
        assert_eq!(meta.subject_cn.as_deref(), Some("test.example.com"));
        // 自签名证书的签发者即自身
        assert_eq!(meta.issuer_cn.as_deref(), Some("test.example.com"));
        let Some(not_before) = meta.not_before else {
            panic!("应解析出 not_before");
        };
        let Some(not_after) = meta.not_after else {
            panic!("应解析出 not_after");
        };
        assert!(not_after > not_before, "有效期应正序");
        assert!(
            not_after - not_before > chrono::Duration::days(365),
            "默认有效期应超过一年"
        );
    }

    #[test]
    fn 证书元数据解析_无效输入返回none() {
        // 非 PEM 数据
        assert!(parse_cert_metadata(b"this is not a pem").is_none());
        // PEM 块但不是证书
        let not_cert = b"-----BEGIN NOTE-----\nSGVsbG8=\n-----END NOTE-----\n";
        assert!(parse_cert_metadata(not_cert).is_none());
        // 证书块但内容非 DER
        let bad_cert = b"-----BEGIN CERTIFICATE-----\nSGVsbG8=\n-----END CERTIFICATE-----\n";
        assert!(parse_cert_metadata(bad_cert).is_none());
        // 空输入
        assert!(parse_cert_metadata(b"").is_none());
    }
}
