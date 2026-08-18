//! 证书读取与删除（带文件名校验，防路径穿越）。

use ipma_common::msg;

use crate::error::CertManagerError;
use crate::listing::CertKind;

/// 校验文件名：仅允许目录内的纯文件名（.pem / .key）
fn validate_filename(filename: &str, extension: &str) -> Result<(), CertManagerError> {
    let valid = !filename.is_empty()
        && !filename.contains('/')
        && !filename.contains('\\')
        && !filename.contains("..")
        && filename
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        && filename.ends_with(extension)
        && filename.len() > extension.len();
    if valid {
        Ok(())
    } else {
        Err(CertManagerError::Validation(msg(
            "server.certificate.filename_invalid",
        )))
    }
}

/// 校验文件基础名（不含扩展名，删除操作按 stem 删除 .pem 与 .key）
fn validate_stem(stem: &str) -> Result<(), CertManagerError> {
    let valid = !stem.is_empty()
        && !stem.contains('/')
        && !stem.contains('\\')
        && !stem.contains("..")
        && !stem.contains('.')
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valid {
        Ok(())
    } else {
        Err(CertManagerError::Validation(msg(
            "server.certificate.filename_invalid",
        )))
    }
}

/// 读取证书/私钥文件内容，返回 (文件名, 字节)
pub async fn read_certificate(
    kind: CertKind,
    filename: &str,
) -> Result<(String, Vec<u8>), CertManagerError> {
    let extension = if filename.ends_with(".key") {
        ".key"
    } else {
        ".pem"
    };
    validate_filename(filename, extension)?;

    let path = std::path::Path::new(kind.dir()).join(filename);
    let content = tokio::fs::read(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            CertManagerError::NotFound(msg("server.certificate.not_found"))
        } else {
            CertManagerError::Internal(
                msg("server.certificate.read_failed").with("error", e.to_string()),
            )
        }
    })?;

    Ok((filename.to_string(), content))
}

/// 删除证书对（{stem}.pem 与 {stem}.key），文件不存在视为未找到
pub async fn delete_certificate(kind: CertKind, file_stem: &str) -> Result<(), CertManagerError> {
    validate_stem(file_stem)?;

    let cert_path = std::path::Path::new(kind.dir()).join(format!("{file_stem}.pem"));
    let key_path = std::path::Path::new(kind.dir()).join(format!("{file_stem}.key"));

    if !cert_path.exists() && !key_path.exists() {
        return Err(CertManagerError::NotFound(msg(
            "server.certificate.not_found",
        )));
    }

    if cert_path.exists() {
        tokio::fs::remove_file(&cert_path).await.map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.delete_failed").with("error", e.to_string()),
            )
        })?;
    }
    if key_path.exists() {
        tokio::fs::remove_file(&key_path).await.map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.delete_failed").with("error", e.to_string()),
            )
        })?;
    }

    Ok(())
}
