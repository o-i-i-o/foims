//! 证书导入（PEM 校验后写入导入目录）。

use std::path::PathBuf;

use ipma_common::msg;

use crate::IMPORTED_CERTS_DIR;
use crate::error::CertManagerError;
use crate::generate::write_key_file;

/// 校验 PEM 数据中包含指定标签的块
fn has_pem_block(data: &[u8], tag_pred: fn(&str) -> bool) -> bool {
    match pem::parse_many(data) {
        Ok(pems) => pems.iter().any(|p| tag_pred(p.tag())),
        Err(_) => false,
    }
}

fn is_certificate_tag(tag: &str) -> bool {
    tag == "CERTIFICATE"
}

fn is_private_key_tag(tag: &str) -> bool {
    tag.contains("PRIVATE KEY")
}

/// 导入证书与私钥（PEM），写入导入目录并返回证书文件路径。
///
/// 仅做结构校验（证书块/私钥块存在、证书可解析）；证书与私钥的
/// 匹配性由部署侧保证。
pub async fn import_certificate(
    cert_pem: Vec<u8>,
    key_pem: Vec<u8>,
) -> Result<PathBuf, CertManagerError> {
    if cert_pem.is_empty() || key_pem.is_empty() {
        return Err(CertManagerError::Validation(msg(
            "server.certificate.cert_file_missing",
        )));
    }
    if !has_pem_block(&cert_pem, is_certificate_tag) || !has_pem_block(&key_pem, is_private_key_tag)
    {
        return Err(CertManagerError::Validation(msg(
            "server.certificate.cert_file_invalid",
        )));
    }
    // 证书须可被 X.509 解析，避免存入垃圾数据
    if crate::listing::parse_cert_metadata(&cert_pem).is_none() {
        return Err(CertManagerError::Validation(msg(
            "server.certificate.cert_file_invalid",
        )));
    }

    tokio::fs::create_dir_all(IMPORTED_CERTS_DIR)
        .await
        .map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;

    let timestamp = chrono::Utc::now().timestamp();
    let base_name = format!("import_{timestamp}_cert");
    let cert_path = PathBuf::from(IMPORTED_CERTS_DIR).join(format!("{base_name}.pem"));
    let key_path = PathBuf::from(IMPORTED_CERTS_DIR).join(format!("{base_name}.key"));

    tokio::fs::write(&cert_path, cert_pem).await.map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.write_failed").with("error", e.to_string()),
        )
    })?;
    write_key_file(&key_path, &String::from_utf8_lossy(&key_pem)).await?;

    Ok(cert_path)
}
