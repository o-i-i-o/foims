//! 证书导入（PEM 校验后写入导入目录）。

use std::path::PathBuf;

use ipma_common::msg;

use crate::IMPORTED_CERTS_DIR;
use crate::error::CertManagerError;
use crate::generate::write_cert_pair_exclusive;

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
/// 校验：证书可被 X.509 解析、私钥可解析（KeyPair）且公钥与证书公钥
/// 匹配——内容为 "Hello" 的假私钥或错配对在此即被拒绝。
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
    // 证书/私钥解析与公钥匹配移入阻塞线程（KeyPair::from_pem 涉及密钥计算）
    let validated = {
        let cert_for_check = cert_pem.clone();
        let key_for_check = String::from_utf8_lossy(&key_pem).to_string();
        tokio::task::spawn_blocking(move || {
            crate::ca::validate_cert_key_pair(&cert_for_check, &key_for_check)
        })
        .await
        .map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.generate_failed").with("error", e.to_string()),
            )
        })?
    };
    validated?;

    tokio::fs::create_dir_all(IMPORTED_CERTS_DIR)
        .await
        .map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;

    // 证书/私钥成对独占写入（与生成路径共用 write_cert_pair_exclusive）：
    // 私钥先 create_new(0600)、证书再 create_new，同名冲突（AlreadyExists）
    // 换下一序号重试并清理半成品，杜绝预检-写入间隙被并发导入抢占后
    // 留下"证书 A + 私钥 B"的错配对
    let key_pem_str = String::from_utf8_lossy(&key_pem).to_string();
    let cert_path = write_cert_pair_exclusive(
        IMPORTED_CERTS_DIR,
        "import_",
        &String::from_utf8_lossy(&cert_pem),
        &key_pem_str,
    )
    .await?;

    Ok(cert_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造仅含指定标签 PEM 块的字节流（内容不要求为真实 DER）
    fn pem_bytes(tag: &str) -> Vec<u8> {
        format!("-----BEGIN {tag}-----\nSGVsbG8=\n-----END {tag}-----\n").into_bytes()
    }

    #[test]
    fn 证书标签判定_仅精确匹配certificate() {
        assert!(is_certificate_tag("CERTIFICATE"));
        assert!(!is_certificate_tag("certificate"), "大小写敏感");
        assert!(!is_certificate_tag("X509 CERTIFICATE"));
    }

    #[test]
    fn 私钥标签判定_匹配各类private_key() {
        assert!(is_private_key_tag("PRIVATE KEY"));
        assert!(is_private_key_tag("RSA PRIVATE KEY"));
        assert!(is_private_key_tag("EC PRIVATE KEY"));
        assert!(is_private_key_tag("ENCRYPTED PRIVATE KEY"));
        assert!(!is_private_key_tag("PUBLIC KEY"));
    }

    #[test]
    fn pem块检测_按标签谓词判定() {
        let cert = pem_bytes("CERTIFICATE");
        assert!(has_pem_block(&cert, is_certificate_tag));
        assert!(!has_pem_block(&cert, is_private_key_tag));

        let key = pem_bytes("RSA PRIVATE KEY");
        assert!(has_pem_block(&key, is_private_key_tag));
        assert!(!has_pem_block(&key, is_certificate_tag));
    }

    #[test]
    fn pem块检测_非pem数据返回false() {
        assert!(!has_pem_block(b"garbage data", is_certificate_tag));
        assert!(!has_pem_block(b"", is_certificate_tag));
    }

    /// 构造单线程 tokio 运行时（本 crate 的 tokio 未启用 macros 特性）
    fn with_runtime<F: std::future::Future>(fut: F) -> F::Output {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap_or_else(|e| panic!("构造测试运行时失败: {e}"));
        rt.block_on(fut)
    }

    /// 空输入在校验阶段即被拒绝，不触碰文件系统
    #[test]
    fn 导入_空输入返回校验错误() {
        let err = with_runtime(import_certificate(vec![], vec![]))
            .err()
            .unwrap_or_else(|| panic!("空输入应被拒绝"));
        match err {
            CertManagerError::Validation(m) => {
                assert_eq!(m.key(), "server.certificate.cert_file_missing")
            }
            other => panic!("应为校验错误，实际 {other}"),
        }
    }

    /// 私钥不含 PRIVATE KEY 块时在校验阶段被拒绝
    #[test]
    fn 导入_私钥格式非法返回校验错误() {
        let cert = pem_bytes("CERTIFICATE");
        let bad_key = pem_bytes("NOTE");
        let err = with_runtime(import_certificate(cert, bad_key))
            .err()
            .unwrap_or_else(|| panic!("非法私钥应被拒绝"));
        match err {
            CertManagerError::Validation(m) => {
                assert_eq!(m.key(), "server.certificate.cert_file_invalid")
            }
            other => panic!("应为校验错误，实际 {other}"),
        }
    }

    /// 证书块存在但无法被 X.509 解析（内容非 DER）时被拒绝
    #[test]
    fn 导入_证书不可解析返回校验错误() {
        let cert = pem_bytes("CERTIFICATE"); // 内容为 "Hello"，非 DER
        let key = pem_bytes("PRIVATE KEY");
        let err = with_runtime(import_certificate(cert, key))
            .err()
            .unwrap_or_else(|| panic!("不可解析证书应被拒绝"));
        match err {
            CertManagerError::Validation(m) => {
                assert_eq!(m.key(), "server.certificate.cert_file_invalid")
            }
            other => panic!("应为校验错误，实际 {other}"),
        }
    }

    /// 用 rcgen 在内存中生成一张可解析的自签名证书（不触碰文件系统）
    fn real_cert_pem() -> String {
        let key_pair = rcgen::KeyPair::generate().unwrap_or_else(|e| panic!("生成密钥失败: {e}"));
        let mut params = rcgen::CertificateParams::default();
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "import.example.com");
        params.distinguished_name = dn;
        let cert = params
            .self_signed(&key_pair)
            .unwrap_or_else(|e| panic!("生成证书失败: {e}"));
        cert.pem()
    }

    /// 证书可解析但私钥为假内容（"Hello"）时被拒绝，不再放行
    #[test]
    fn 导入_假私钥被拒绝() {
        let cert = real_cert_pem().into_bytes();
        let fake_key = pem_bytes("PRIVATE KEY"); // 内容非真实密钥
        let err = with_runtime(import_certificate(cert, fake_key))
            .err()
            .unwrap_or_else(|| panic!("假私钥应被拒绝"));
        match err {
            CertManagerError::Validation(m) => {
                assert_eq!(m.key(), "server.certificate.cert_file_invalid")
            }
            other => panic!("应为校验错误，实际 {other}"),
        }
    }

    /// 证书与私钥均可解析但公钥不匹配（另一把钥匙）时被拒绝
    #[test]
    fn 导入_私钥与证书公钥不匹配被拒绝() {
        let cert = real_cert_pem().into_bytes();
        let other_key = rcgen::KeyPair::generate().unwrap_or_else(|e| panic!("生成密钥失败: {e}"));
        let mismatched_key = other_key.serialize_pem().into_bytes();
        let err = with_runtime(import_certificate(cert, mismatched_key))
            .err()
            .unwrap_or_else(|| panic!("错配私钥应被拒绝"));
        match err {
            CertManagerError::Validation(m) => {
                assert_eq!(m.key(), "server.certificate.cert_file_invalid")
            }
            other => panic!("应为校验错误，实际 {other}"),
        }
    }
}
