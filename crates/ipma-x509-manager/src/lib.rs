//! IPMA X.509 证书管理。
//!
//! 目录约定：
//! - 生成的证书（由站点根 CA 或导入 CA 签发）：`/etc/ssl/ipma-certs/`（create_{ts}_cert.pem/.key）
//! - 导入的证书：`/etc/ssl/ipma-import-certs/`（import_{ts}_cert.pem/.key）
//! - 站点根 CA：`/etc/ssl/ipma-ca/`（ca.pem + 可选 ca.key）
//! - 导入 CA 池：`/etc/ssl/ipma-import-cas/{id}/`（ca.pem + ca.key）
//!
//! 本 crate 只提供库函数（文件系统 + 解析），HTTP 提取与操作日志由
//! 主 crate 的 handler 层负责，与 ipma-visualization 的分工方式一致。

mod ca;
mod error;
mod generate;
mod import;
mod listing;
mod transfer;

pub use ca::{
    CA_DIR, CaInfo, CaStatus, GenerateCaRequest, IMPORT_CA_DIR, ROOT_CA_ID, ca_status, generate_ca,
    import_ca, list_cas, read_ca_cert_pem, set_ca_cert_only,
};
pub use error::CertManagerError;
pub use generate::{GenerateCertRequest, generate_certificate};
pub use import::import_certificate;
pub use listing::{CertFileInfo, CertKind, CertificateInventory, list_certificates};
pub use transfer::{delete_certificate, read_certificate};

/// 生成的自签名证书目录
pub const GENERATED_CERTS_DIR: &str = "/etc/ssl/ipma-certs";
/// 导入的证书目录
pub const IMPORTED_CERTS_DIR: &str = "/etc/ssl/ipma-import-certs";
