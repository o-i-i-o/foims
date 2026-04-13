use crate::config::Config;
use actix_multipart::Multipart;
use actix_web::{HttpResponse, web};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::io::Write;
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

// 生成自签名证书
pub async fn generate_cert(req: web::Json<CertGenerateRequest>) -> HttpResponse {
    info!("正在生成自签名证书");

    let cert_path = "certs/cert.pem";
    let key_path = "certs/key.pem";

    // 确保证书目录存在
    if !Path::new("certs").exists() {
        fs::create_dir_all("certs").unwrap();
    }

    // 生成自签名证书
    use rcgen::generate_simple_self_signed;

    // 生成自签名证书
    let certified_key = generate_simple_self_signed(vec![req.common_name.clone()]).unwrap();

    // 获取证书 PEM
    let cert_pem = certified_key.cert.pem();

    // 获取私钥 PEM
    let key_pem = certified_key.signing_key.serialize_pem();

    // 保存证书
    let mut cert_file = fs::File::create(cert_path).unwrap();
    cert_file.write_all(cert_pem.as_bytes()).unwrap();

    // 保存私钥
    let mut key_file = fs::File::create(key_path).unwrap();
    key_file.write_all(key_pem.as_bytes()).unwrap();

    info!("自签名证书生成成功");

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "证书生成成功",
        "data": null
    }))
}

// 导入证书
pub async fn import_cert(mut payload: Multipart) -> HttpResponse {
    info!("正在导入证书");

    let mut cert_data: Option<Vec<u8>> = None;
    let mut key_data: Option<Vec<u8>> = None;

    while let Ok(Some(mut field)) = payload.try_next().await {
        let name = field.name().expect("Field name not found").to_string();
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

    // 确保证书目录存在
    if !Path::new("certs").exists() {
        fs::create_dir_all("certs").unwrap();
    }

    // 保存导入的证书
    let cert_path = "certs/imported_cert.pem";
    let key_path = "certs/imported_key.pem";

    fs::write(cert_path, cert_data.unwrap()).unwrap();
    fs::write(key_path, key_data.unwrap()).unwrap();

    info!("证书导入成功");

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "证书导入成功",
        "data": null
    }))
}

// 获取证书信息
pub async fn get_cert_info() -> HttpResponse {
    info!("正在获取证书信息");

    let self_signed_cert_path = "certs/cert.pem";
    let self_signed_key_path = "certs/key.pem";
    let imported_cert_path = "certs/imported_cert.pem";
    let imported_key_path = "certs/imported_key.pem";

    let self_signed_exists =
        Path::new(self_signed_cert_path).exists() && Path::new(self_signed_key_path).exists();
    let imported_exists =
        Path::new(imported_cert_path).exists() && Path::new(imported_key_path).exists();

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

// 删除导入的证书
pub async fn delete_imported_cert() -> HttpResponse {
    info!("正在删除导入的证书");

    let imported_cert_path = "certs/imported_cert.pem";
    let imported_key_path = "certs/imported_key.pem";

    if Path::new(imported_cert_path).exists() {
        fs::remove_file(imported_cert_path).unwrap();
    }

    if Path::new(imported_key_path).exists() {
        fs::remove_file(imported_key_path).unwrap();
    }

    info!("导入的证书删除成功");

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "导入的证书删除成功",
        "data": null
    }))
}

// 辅助函数：获取最新的证书文件
pub fn get_latest_certificate(dir: &str, cert_type: &str) -> Option<(String, String)> {
    let mut cert_files = Vec::new();

    // 遍历目录下的所有文件
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename_os) = path.file_name()
                && let Some(filename) = filename_os.to_str()
            {
                // 检查文件名是否符合规则
                if (cert_type == "create" && filename.starts_with("create_"))
                    || (cert_type == "import" && filename.starts_with("import_"))
                {
                    cert_files.push(filename.to_string());
                }
            }
        }
    }

    // 按时间戳排序，获取最新的文件
    cert_files.sort_by(|a, b| b.cmp(a));

    if let Some(latest) = cert_files.first() {
        let base_name = latest.strip_suffix(".pem").unwrap_or(latest);
        let cert_path = format!("{}/{}.pem", dir, base_name);
        let key_path = format!("{}/{}.key", dir, base_name);

        // 检查证书和密钥文件是否都存在
        if Path::new(&cert_path).exists() && Path::new(&key_path).exists() {
            return Some((cert_path, key_path));
        }
    }

    None
}

// 准备服务器证书（查找现有或生成新的）
pub fn prepare_server_certificate(config: &Config) -> io::Result<(String, String)> {
    let cert_type = config.server.cert_type.as_deref().unwrap_or("self_signed");
    let app_name = env!("CARGO_PKG_NAME");
    let certs_dir = format!("/etc/{}/certs", app_name);

    // 确保证书目录存在
    if !Path::new(&certs_dir).exists() {
        info!("创建证书目录: {}", certs_dir);
        fs::create_dir_all(&certs_dir)?;
    }

    // 获取最新的证书文件
    let (cert_path, key_path) = match cert_type {
        "imported" => {
            // 检查导入的证书是否存在
            if let Some((cert, key)) = get_latest_certificate(&certs_dir, "import") {
                info!("使用导入的证书: {}", cert);
                (cert, key)
            } else {
                // 如果导入的证书不存在，使用自签名证书
                info!("未找到导入的证书，使用自签名证书");
                if let Some((cert, key)) = get_latest_certificate(&certs_dir, "create") {
                    (cert, key)
                } else {
                    // 如果自签名证书也不存在，生成带时间戳的文件名
                    let timestamp = chrono::Utc::now().timestamp();
                    let base_name = format!("create_{}_cert", timestamp);
                    (
                        format!("{}/{}.pem", certs_dir, base_name),
                        format!("{}/{}.key", certs_dir, base_name),
                    )
                }
            }
        }
        _ => {
            // 默认使用自签名证书
            if let Some((cert, key)) = get_latest_certificate(&certs_dir, "create") {
                info!("使用自签名证书: {}", cert);
                (cert, key)
            } else {
                // 如果自签名证书不存在，生成带时间戳的文件名
                let timestamp = chrono::Utc::now().timestamp();
                let base_name = format!("create_{}_cert", timestamp);
                (
                    format!("{}/{}.pem", certs_dir, base_name),
                    format!("{}/{}.key", certs_dir, base_name),
                )
            }
        }
    };

    // 如果证书文件不存在，生成自签名证书
    if !Path::new(&cert_path).exists() || !Path::new(&key_path).exists() {
        info!("生成自签名证书");
        // 生成自签名证书
        use rcgen::generate_simple_self_signed;

        // 生成自签名证书
        let certified_key =
            generate_simple_self_signed(vec!["localhost".to_string()]).map_err(io::Error::other)?;

        // 获取证书 PEM
        let cert_pem = certified_key.cert.pem();

        // 获取私钥 PEM
        let key_pem = certified_key.signing_key.serialize_pem();

        // 保存证书
        let mut cert_file = std::fs::File::create(&cert_path)?;
        cert_file.write_all(cert_pem.as_bytes())?;

        // 保存私钥
        let mut key_file = std::fs::File::create(&key_path)?;
        key_file.write_all(key_pem.as_bytes())?;

        info!("自签名证书生成成功");
    }

    Ok((cert_path, key_path))
}

// 加载 Rustls 配置（支持 HTTP/1.1 和 HTTP/2）
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

    // 配置 ALPN 协议，支持 HTTP/2 和 HTTP/1.1
    // ALPN 协议顺序：h2（HTTP/2）优先，然后是 http/1.1
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(config)
}
