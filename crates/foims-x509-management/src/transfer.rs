//! 证书删除（带文件名校验，防路径穿越）。
//!
//! 说明：历史遗留的 `read_certificate`（可读私钥内容）因全仓库无调用方
//! 且属潜在误用面（私钥外发通道）已移除；私钥内容不提供任何读取接口。

use foims_common::msg;

use crate::error::CertManagerError;
use crate::listing::CertKind;

/// 校验文件基础名（不含扩展名，删除/应用操作按 stem 定位 .pem 与 .key）
pub(crate) fn validate_stem(stem: &str) -> Result<(), CertManagerError> {
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

/// 删除证书对（{stem}.pem 与 {stem}.key），文件不存在视为未找到
pub async fn delete_certificate(kind: CertKind, file_stem: &str) -> Result<(), CertManagerError> {
    validate_stem(file_stem)?;

    let cert_path = std::path::Path::new(kind.dir()).join(format!("{file_stem}.pem"));
    let key_path = std::path::Path::new(kind.dir()).join(format!("{file_stem}.key"));

    let cert_exists = tokio::fs::try_exists(&cert_path).await.unwrap_or(false);
    let key_exists = tokio::fs::try_exists(&key_path).await.unwrap_or(false);
    if !cert_exists && !key_exists {
        return Err(CertManagerError::NotFound(msg(
            "server.certificate.not_found",
        )));
    }

    let delete_err = |e: std::io::Error| {
        CertManagerError::Internal(
            msg("server.certificate.delete_failed").with("error", e.to_string()),
        )
    };
    // 先删私钥（敏感残留优先清除）；删除幂等可重试，中途失败留下的
    // 一侧由下次删除按未完成对继续清理
    if key_exists {
        tokio::fs::remove_file(&key_path)
            .await
            .map_err(delete_err)?;
    }
    if cert_exists {
        tokio::fs::remove_file(&cert_path)
            .await
            .map_err(delete_err)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
