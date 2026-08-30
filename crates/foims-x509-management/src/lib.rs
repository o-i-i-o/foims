//! FOIMS X.509 证书管理。
//!
//! 目录约定：
//! - 生成的证书（由站点根 CA 或导入 CA 签发）：`/etc/ssl/foims-certs/`（create_{ts}_cert.pem/.key）
//! - 导入的证书：`/etc/ssl/foims-import-certs/`（import_{ts}_cert.pem/.key）
//! - 站点根 CA：`/etc/ssl/foims-ca/`（ca.pem + 可选 ca.key）
//! - 导入 CA 池：`/etc/ssl/foims-import-cas/{id}/`（ca.pem + ca.key）
//!
//! 本 crate 只提供库函数（文件系统 + 解析），HTTP 提取与操作日志由
//! 主 crate 的 handler 层负责，与 foims-visualization 的分工方式一致。

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
pub use transfer::delete_certificate;

/// 生成的自签名证书目录
pub const GENERATED_CERTS_DIR: &str = "/etc/ssl/foims-certs";
/// 导入的证书目录
pub const IMPORTED_CERTS_DIR: &str = "/etc/ssl/foims-import-certs";

/// 生成唯一证书文件基础名：`{prefix}{毫秒时间戳}_cert`。
///
/// 秒级时间戳在同一秒内两次生成/导入会得到同名文件，交错写入可留下
/// 证书 A + 私钥 B 的错配对；因此采用毫秒时间戳，仍冲突时追加递增
/// 序号（.pem 与 .key 任一存在即视为冲突）。
/// 生产路径的生成/导入已统一走 generate::write_cert_pair_exclusive 的
/// create_new 独占创建 + AlreadyExists 换名重试，本函数仅服务于测试
///（固定时间戳验证冲突递增序号），故仅测试构建下编译。
#[cfg(test)]
pub(crate) async fn unique_cert_stem_with_timestamp(
    dir: &str,
    prefix: &str,
    timestamp: i64,
) -> String {
    let mut stem = format!("{prefix}{timestamp}_cert");
    let mut seq = 1u64;
    loop {
        let cert_exists =
            tokio::fs::try_exists(std::path::Path::new(dir).join(format!("{stem}.pem")))
                .await
                .unwrap_or(false);
        let key_exists =
            tokio::fs::try_exists(std::path::Path::new(dir).join(format!("{stem}.key")))
                .await
                .unwrap_or(false);
        if !cert_exists && !key_exists {
            return stem;
        }
        stem = format!("{prefix}{timestamp}_{seq}_cert");
        seq += 1;
    }
}

/// 由时间戳与冲突序号构造证书文件基础名（与 [`unique_cert_stem_with_timestamp`]
/// 的命名一致）：seq 为 0 时不带序号后缀，否则追加 `_seq`。
pub(crate) fn cert_stem_for(prefix: &str, timestamp: i64, seq: u64) -> String {
    if seq == 0 {
        format!("{prefix}{timestamp}_cert")
    } else {
        format!("{prefix}{timestamp}_{seq}_cert")
    }
}
