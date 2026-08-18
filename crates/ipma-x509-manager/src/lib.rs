//! IPMA X.509 证书管理。
//!
//! 目录约定：
//! - 生成的自签名证书：`/etc/ssl/ipma-certs/`（create_{ts}_cert.pem/.key）
//! - 导入的证书：`/etc/ssl/ipma-import-certs/`（import_{ts}_cert.pem/.key）
//!
//! 本 crate 只提供库函数（文件系统 + 解析），HTTP 提取与操作日志由
//! 主 crate 的 handler 层负责，与 ipma-visualization 的分工方式一致。

mod error;
mod generate;
mod import;
mod listing;
mod transfer;

pub use error::CertManagerError;
pub use generate::{GenerateCertRequest, generate_self_signed};
pub use import::import_certificate;
pub use listing::{CertFileInfo, CertKind, CertificateInventory, list_certificates};
pub use transfer::{delete_certificate, read_certificate};

/// 生成的自签名证书目录
pub const GENERATED_CERTS_DIR: &str = "/etc/ssl/ipma-certs";
/// 导入的证书目录
pub const IMPORTED_CERTS_DIR: &str = "/etc/ssl/ipma-import-certs";
