//! 证书应用到 nginx 配置：将选中证书的证书/私钥路径整体替换进
//! nginx 的 foims.conf（ssl_certificate / ssl_certificate_key 指令）。
//!
//! 目标文件按候选目录探测：Debian 系 `sites-available` 与 RHEL 系
//! `conf.d`，仅处理实际存在的文件；两个候选都不存在时返回未找到
//! 错误。不负责重载/重启 nginx（由前端提示用户手动重启）。

use std::path::{Path, PathBuf};

use foims_common::msg;

use crate::error::CertManagerError;
use crate::listing::CertKind;

/// nginx 配置目录下的目标文件名
const NGINX_CONF_NAME: &str = "foims.conf";

/// 候选配置目录：Debian 系 sites-available / RHEL 系 conf.d
const NGINX_CONF_DIRS: [&str; 2] = ["/etc/nginx/sites-available", "/etc/nginx/conf.d"];

/// 应用结果：实际改写的配置文件路径
pub struct NginxApplyResult {
    pub updated: Vec<PathBuf>,
}

/// 临时文件 + rename 原子写入配置：截断式直写在进程崩溃/磁盘满时会
/// 留下半截 foims.conf，nginx 下次 reload 失败（与 ca.rs 原子写同口径）
async fn write_conf_atomic(conf: &Path, content: &str) -> Result<(), CertManagerError> {
    use tokio::io::AsyncWriteExt;

    let tmp_path = PathBuf::from(format!(
        "{}.{}.{}.tmp",
        conf.display(),
        std::process::id(),
        chrono::Utc::now().timestamp_micros()
    ));
    let write_err = |e: std::io::Error| {
        CertManagerError::Internal(
            msg("server.certificate.nginx_conf_write_failed").with("error", e),
        )
    };

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .await
        .map_err(write_err)?;

    let write_result: std::io::Result<()> = async {
        file.write_all(content.as_bytes()).await?;
        file.sync_all().await
    }
    .await;

    if let Err(e) = write_result {
        if let Err(remove_err) = tokio::fs::remove_file(&tmp_path).await {
            foims_common::log_warn!("log.certificate.conf_tmp_remove_failed", error = remove_err);
        }
        return Err(write_err(e));
    }

    if let Err(e) = tokio::fs::rename(&tmp_path, conf).await {
        if let Err(remove_err) = tokio::fs::remove_file(&tmp_path).await {
            foims_common::log_warn!("log.certificate.conf_tmp_remove_failed", error = remove_err);
        }
        return Err(write_err(e));
    }
    Ok(())
}

/// 将证书对（{stem}.pem + {stem}.key）的路径整体替换进候选 foims.conf。
///
/// 证书与私钥文件必须同时存在（nginx 两者都需要）；候选文件仅存在其一
/// 时只处理该文件；替换计数为 0（文件均不含证书指令）时返回验证错误，
/// 避免"应用成功"却什么都没改。
pub async fn apply_certificate_to_nginx(
    kind: CertKind,
    file_stem: &str,
) -> Result<NginxApplyResult, CertManagerError> {
    crate::transfer::validate_stem(file_stem)?;

    let cert_path = Path::new(kind.dir()).join(format!("{file_stem}.pem"));
    let key_path = Path::new(kind.dir()).join(format!("{file_stem}.key"));
    if !path_exists(&cert_path).await {
        return Err(CertManagerError::NotFound(msg(
            "server.certificate.not_found",
        )));
    }
    if !path_exists(&key_path).await {
        return Err(CertManagerError::NotFound(msg(
            "server.certificate.key_not_found",
        )));
    }

    let mut existing = Vec::new();
    for dir in NGINX_CONF_DIRS {
        let conf = Path::new(dir).join(NGINX_CONF_NAME);
        if path_exists(&conf).await {
            existing.push(conf);
        }
    }
    if existing.is_empty() {
        return Err(CertManagerError::NotFound(msg(
            "server.certificate.nginx_conf_not_found",
        )));
    }

    let mut updated = Vec::new();
    let mut replaced = 0usize;
    for conf in existing {
        let content = tokio::fs::read_to_string(&conf).await.map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.nginx_conf_read_failed").with("error", e),
            )
        })?;
        let (new_content, count) = replace_cert_directives(&content, &cert_path, &key_path);
        if count > 0 {
            write_conf_atomic(&conf, &new_content).await?;
            updated.push(conf);
        }
        replaced += count;
    }

    if replaced == 0 {
        return Err(CertManagerError::Validation(msg(
            "server.certificate.apply_no_directive",
        )));
    }

    Ok(NginxApplyResult { updated })
}

async fn path_exists(path: &Path) -> bool {
    tokio::fs::try_exists(path).await.unwrap_or(false)
}

/// 逐行替换 ssl_certificate / ssl_certificate_key 指令的路径参数，
/// 返回（新内容, 命中指令数）。保留行首缩进，路径整体替换为目标
/// 路径；注释行（# 开头）与注释内嵌的指令不受影响，指令行原有的
/// 路径与行尾注释不保留。ssl_certificate_key 须先于 ssl_certificate
/// 判断（后者是前者的前缀）。
fn replace_cert_directives(content: &str, cert_path: &Path, key_path: &Path) -> (String, usize) {
    let mut count = 0usize;
    let mut lines = Vec::with_capacity(content.lines().count());
    for line in content.lines() {
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        let directive = if trimmed.starts_with("ssl_certificate_key") {
            format!("ssl_certificate_key {}", key_path.display())
        } else if trimmed.starts_with("ssl_certificate") {
            format!("ssl_certificate {}", cert_path.display())
        } else {
            lines.push(line.to_string());
            continue;
        };
        lines.push(format!("{indent}{directive};"));
        count += 1;
    }
    let mut out = lines.join("\n");
    if content.ends_with('\n') {
        out.push('\n');
    }
    (out, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CERT: &str = "/etc/ssl/foims-certs/a.pem";
    const KEY: &str = "/etc/ssl/foims-certs/a.key";

    #[test]
    fn 替换_证书与私钥路径整体更新() {
        let content = "server {\n    ssl_certificate /old/site.pem;\n    ssl_certificate_key /old/site.key;\n}\n";
        let (out, count) = replace_cert_directives(content, Path::new(CERT), Path::new(KEY));
        assert_eq!(count, 2);
        assert!(out.contains(&format!("ssl_certificate {CERT};")));
        assert!(out.contains(&format!("ssl_certificate_key {KEY};")));
        assert!(!out.contains("/old/"));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn 替换_key前缀不误伤_注释行保留() {
        let content = "# ssl_certificate /old/comment.pem;\nssl_certificate /old/x.pem;\nssl_trusted_certificate /ca.pem;\n";
        let (out, count) = replace_cert_directives(content, Path::new(CERT), Path::new(KEY));
        assert_eq!(count, 1);
        assert!(out.contains("# ssl_certificate /old/comment.pem;"));
        assert!(out.contains("ssl_trusted_certificate /ca.pem;"));
    }

    #[test]
    fn 替换_无指令时计数为零() {
        let content = "server {\n    listen 80;\n}\n";
        let (out, count) = replace_cert_directives(content, Path::new(CERT), Path::new(KEY));
        assert_eq!(count, 0);
        assert_eq!(out, content);
    }
}
