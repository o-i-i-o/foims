use crate::config::Config;
use actix_multipart::Multipart;
use actix_web::{HttpResponse, web};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::Path;
use tracing::info;

#[derive(Debug, Deserialize, Serialize)]
pub struct CertGenerateRequest {
    pub common_name: String,
    pub country: String,
    pub state: String,
    pub locality: String,
    pub organization: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CertResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<CertInfo>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CertInfo {
    pub cert_type: String,
    pub cert_path: String,
    pub key_path: String,
    pub exists: bool,
}

pub async fn generate_cert(req: web::Json<CertGenerateRequest>) -> HttpResponse {
    info!("正在生成自签名证书");

    let cert_path = "certs/cert.pem";
    let key_path = "certs/key.pem";

    if !Path::new("certs").exists()
        && let Err(e) = tokio::fs::create_dir_all("certs").await {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("创建证书目录失败: {}", e),
                "data": null
            }));
        }

    use rcgen::generate_simple_self_signed;

    let certified_key = match generate_simple_self_signed(vec![req.common_name.clone()]) {
        Ok(key) => key,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("证书生成失败: {}", e),
                "data": null
            }));
        }
    };

    let cert_pem = certified_key.cert.pem();
    let key_pem = certified_key.signing_key.serialize_pem();

    if let Err(e) = tokio::fs::write(cert_path, cert_pem.as_bytes()).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("证书文件写入失败: {}", e),
            "data": null
        }));
    }

    if let Err(e) = tokio::fs::write(key_path, key_pem.as_bytes()).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("密钥文件写入失败: {}", e),
            "data": null
        }));
    }

    info!("自签名证书生成成功");

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "证书生成成功",
        "data": null
    }))
}

pub async fn import_cert(mut payload: Multipart) -> HttpResponse {
    info!("正在导入证书");

    let mut cert_data: Option<Vec<u8>> = None;
    let mut key_data: Option<Vec<u8>> = None;

    while let Ok(Some(mut field)) = payload.try_next().await {
        let name = match field.name() {
            Some(n) => n.to_string(),
            None => {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "success": false,
                    "message": "表单字段名称无效",
                    "data": null
                }));
            }
        };
        let mut data = Vec::new();

        while let Ok(Some(chunk)) = field.try_next().await {
            data.extend_from_slice(&chunk);
        }

        if name == "cert" {
            cert_data = Some(data);
        } else if name == "key" {
            key_data = Some(data);
        }
    }

    if cert_data.is_none() || key_data.is_none() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "缺少证书或私钥",
            "data": null
        }));
    }

    if !Path::new("certs").exists()
        && let Err(e) = tokio::fs::create_dir_all("certs").await {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("创建证书目录失败: {}", e),
                "data": null
            }));
        }

    let cert_path = "certs/imported_cert.pem";
    let key_path = "certs/imported_key.pem";

    let cert_data = cert_data.expect("证书数据丢失");
    let key_data = key_data.expect("密钥数据丢失");

    if let Err(e) = tokio::fs::write(cert_path, &cert_data).await {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("证书文件写入失败: {}", e),
            "data": null
        }));
    }

    if let Err(e) = tokio::fs::write(key_path, &key_data).await {
        let _ = tokio::fs::remove_file(cert_path).await;
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "message": format!("密钥文件写入失败: {}", e),
            "data": null
        }));
    }

    info!("证书导入成功");

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "证书导入成功",
        "data": null
    }))
}

pub async fn get_cert_info() -> HttpResponse {
    info!("正在获取证书信息");

    let self_signed_cert_path = "certs/cert.pem";
    let self_signed_key_path = "certs/key.pem";
    let imported_cert_path = "certs/imported_cert.pem";
    let imported_key_path = "certs/imported_key.pem";

    let self_signed_exists = tokio::fs::metadata(self_signed_cert_path).await.is_ok()
        && tokio::fs::metadata(self_signed_key_path).await.is_ok();
    let imported_exists = tokio::fs::metadata(imported_cert_path).await.is_ok()
        && tokio::fs::metadata(imported_key_path).await.is_ok();

    let cert_info = CertInfo {
        cert_type: if imported_exists {
            "imported".to_string()
        } else {
            "self_signed".to_string()
        },
        cert_path: if imported_exists {
            imported_cert_path.to_string()
        } else {
            self_signed_cert_path.to_string()
        },
        key_path: if imported_exists {
            imported_key_path.to_string()
        } else {
            self_signed_key_path.to_string()
        },
        exists: self_signed_exists || imported_exists,
    };

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "证书信息获取成功",
        "data": cert_info
    }))
}

pub async fn delete_imported_cert() -> HttpResponse {
    info!("正在删除导入的证书");

    let imported_cert_path = "certs/imported_cert.pem";
    let imported_key_path = "certs/imported_key.pem";

    if tokio::fs::metadata(imported_cert_path).await.is_ok()
        && let Err(e) = tokio::fs::remove_file(imported_cert_path).await {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("删除证书文件失败: {}", e),
                "data": null
            }));
        }

    if tokio::fs::metadata(imported_key_path).await.is_ok()
        && let Err(e) = tokio::fs::remove_file(imported_key_path).await {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("删除密钥文件失败: {}", e),
                "data": null
            }));
        }

    info!("导入的证书删除成功");

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "导入的证书删除成功",
        "data": null
    }))
}

#[must_use]
pub fn get_latest_certificate(dir: &str, cert_type: &str) -> Option<(String, String)> {
    let mut cert_files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename_os) = path.file_name()
                && let Some(filename) = filename_os.to_str()
                && ((cert_type == "create" && filename.starts_with("create_"))
                    || (cert_type == "import" && filename.starts_with("import_")))
            {
                cert_files.push(filename.to_string());
            }
        }
    }

    cert_files.sort_by(|a, b| b.cmp(a));

    if let Some(latest) = cert_files.first() {
        let base_name = latest.strip_suffix(".pem").unwrap_or(latest);
        let cert_path = format!("{dir}/{base_name}.pem");
        let key_path = format!("{dir}/{base_name}.key");

        if Path::new(&cert_path).exists() && Path::new(&key_path).exists() {
            return Some((cert_path, key_path));
        }
    }

    None
}

pub fn prepare_server_certificate(config: &Config) -> io::Result<(String, String)> {
    let cert_type = config.server.cert_type.as_deref().unwrap_or("self_signed");
    let app_name = env!("CARGO_PKG_NAME");
    let certs_dir = format!("/etc/{app_name}/certs");

    if !Path::new(&certs_dir).exists() {
        info!("创建证书目录: {}", certs_dir);
        std::fs::create_dir_all(&certs_dir)?;
    }

    let (cert_path, key_path) = match cert_type {
        "imported" => {
            if let Some((cert, key)) = get_latest_certificate(&certs_dir, "import") {
                info!("使用导入的证书: {}", cert);
                (cert, key)
            } else {
                info!("未找到导入的证书，使用自签名证书");
                if let Some((cert, key)) = get_latest_certificate(&certs_dir, "create") {
                    (cert, key)
                } else {
                    let timestamp = chrono::Utc::now().timestamp();
                    let base_name = format!("create_{timestamp}_cert");
                    (
                        format!("{certs_dir}/{base_name}.pem"),
                        format!("{certs_dir}/{base_name}.key"),
                    )
                }
            }
        }
        _ => {
            if let Some((cert, key)) = get_latest_certificate(&certs_dir, "create") {
                info!("使用自签名证书: {}", cert);
                (cert, key)
            } else {
                let timestamp = chrono::Utc::now().timestamp();
                let base_name = format!("create_{timestamp}_cert");
                (
                    format!("{certs_dir}/{base_name}.pem"),
                    format!("{certs_dir}/{base_name}.key"),
                )
            }
        }
    };

    if !Path::new(&cert_path).exists() || !Path::new(&key_path).exists() {
        info!("生成自签名证书");
        use rcgen::generate_simple_self_signed;

        let certified_key =
            generate_simple_self_signed(vec!["localhost".to_string()]).map_err(io::Error::other)?;

        let cert_pem = certified_key.cert.pem();
        let key_pem = certified_key.signing_key.serialize_pem();

        let mut cert_file = std::fs::File::create(&cert_path)?;
        cert_file.write_all(cert_pem.as_bytes())?;

        let mut key_file = std::fs::File::create(&key_path)?;
        key_file.write_all(key_pem.as_bytes())?;

        info!("自签名证书生成成功");
    }

    Ok((cert_path, key_path))
}

pub fn load_rustls_config(cert_path: &str, key_path: &str) -> io::Result<rustls::ServerConfig> {
    let cert_data = std::fs::read(cert_path)?;
    let key_data = std::fs::read(key_path)?;

    let certs = pem::parse_many(&cert_data)
        .map_err(io::Error::other)?
        .into_iter()
        .filter(|pem| pem.tag() == "CERTIFICATE")
        .map(|pem| rustls_pki_types::CertificateDer::from(pem.contents().to_vec()))
        .collect::<Vec<_>>();

    let keys = pem::parse_many(&key_data)
        .map_err(io::Error::other)?
        .into_iter()
        .filter(|pem| pem.tag() == "PRIVATE KEY")
        .map(|pem| rustls_pki_types::PrivatePkcs8KeyDer::from(pem.contents().to_vec()))
        .collect::<Vec<_>>();

    if keys.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "No private key found",
        ));
    }
    let key = keys[0].clone_key();

    use rustls::pki_types::PrivateKeyDer;

    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, PrivateKeyDer::Pkcs8(key))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(config)
}
