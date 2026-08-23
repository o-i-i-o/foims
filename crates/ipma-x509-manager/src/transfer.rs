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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 文件名校验_合法文件名通过() {
        assert!(validate_filename("cert.pem", ".pem").is_ok());
        assert!(validate_filename("create_1761234567_cert.pem", ".pem").is_ok());
        assert!(validate_filename("a_B-9.key", ".key").is_ok());
        assert!(
            validate_filename("import-1.cert.pem", ".pem").is_ok(),
            "多段后缀允许"
        );
    }

    #[test]
    fn 文件名校验_空名与裸扩展名拒绝() {
        assert!(validate_filename("", ".pem").is_err(), "空文件名");
        assert!(validate_filename(".pem", ".pem").is_err(), "仅有扩展名");
        assert!(validate_filename(".key", ".key").is_err(), "仅有扩展名");
    }

    #[test]
    fn 文件名校验_路径穿越与分隔符拒绝() {
        assert!(
            validate_filename("../etc/passwd.pem", ".pem").is_err(),
            "相对路径"
        );
        assert!(validate_filename("a/../b.pem", ".pem").is_err(), "中间 ..");
        assert!(
            validate_filename("/abs/path.pem", ".pem").is_err(),
            "绝对路径"
        );
        assert!(
            validate_filename("dir\\cert.pem", ".pem").is_err(),
            "反斜杠"
        );
        assert!(validate_filename("dir/cert.pem", ".pem").is_err(), "正斜杠");
    }

    #[test]
    fn 文件名校验_非法字符与扩展名不匹配拒绝() {
        assert!(
            validate_filename("cër.pem", ".pem").is_err(),
            "非 ASCII 字符"
        );
        assert!(validate_filename("cert name.pem", ".pem").is_err(), "空格");
        assert!(validate_filename("cert.txt", ".pem").is_err(), "扩展名不符");
        assert!(
            validate_filename("cert.pem", ".key").is_err(),
            "扩展名不匹配"
        );
    }

    #[test]
    fn stem校验_合法基础名通过() {
        assert!(validate_stem("create_1761234567_cert").is_ok());
        assert!(validate_stem("aB9-_x").is_ok());
    }

    #[test]
    fn stem校验_空名与路径穿越拒绝() {
        assert!(validate_stem("").is_err());
        assert!(validate_stem("..").is_err());
        assert!(validate_stem("a/b").is_err());
        assert!(validate_stem("a\\b").is_err());
    }

    #[test]
    fn stem校验_含点号与非法字符拒绝() {
        assert!(validate_stem("a.b").is_err(), "stem 不得含点号");
        assert!(validate_stem("café").is_err(), "非 ASCII 字符");
        assert!(validate_stem("a b").is_err(), "空格");
    }

    /// 构造单线程 tokio 运行时（本 crate 的 tokio 未启用 macros 特性，
    /// 无法使用 #[tokio::test]）
    fn with_runtime<F: std::future::Future>(fut: F) -> F::Output {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap_or_else(|e| panic!("构造测试运行时失败: {e}"));
        rt.block_on(fut)
    }

    /// 读取证书前的文件名校验优先于文件系统访问，
    /// 非法文件名不会触碰磁盘
    #[test]
    fn 读取证书_非法文件名返回校验错误() {
        let err = with_runtime(read_certificate(CertKind::Generated, "../etc/passwd"))
            .err()
            .unwrap_or_else(|| panic!("非法文件名应被拒绝"));
        assert!(matches!(err, CertManagerError::Validation(_)));
    }

    /// 删除前的 stem 校验优先于文件系统访问
    #[test]
    fn 删除证书_非法stem返回校验错误() {
        let err = with_runtime(delete_certificate(CertKind::Imported, "bad.stem"))
            .err()
            .unwrap_or_else(|| panic!("含点号的 stem 应被拒绝"));
        assert!(matches!(err, CertManagerError::Validation(_)));
    }

    /// 不存在的 stem（时间戳保证唯一）在两个目录均不存在时报未找到
    #[test]
    fn 删除证书_不存在的stem返回未找到() {
        let stem = format!("no_such_{}", uuid_like_stem());
        let err = with_runtime(delete_certificate(CertKind::Generated, &stem))
            .err()
            .unwrap_or_else(|| panic!("不存在的 stem 应报未找到"));
        assert!(matches!(err, CertManagerError::NotFound(_)));
    }

    /// 生成不依赖 uuid crate 的唯一 stem（时间戳 + 计数）
    fn uuid_like_stem() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        format!(
            "{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        )
    }
}
