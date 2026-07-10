use crate::config::Config;
use std::io;
use tracing::info;

pub async fn get_latest_certificate(dir: &str, cert_type: &str) -> Option<(String, String)> {
    let mut cert_files = Vec::new();

    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
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

        if tokio::fs::try_exists(&cert_path).await.unwrap_or(false)
            && tokio::fs::try_exists(&key_path).await.unwrap_or(false)
        {
            return Some((cert_path, key_path));
        }
    }

    None
}

pub async fn prepare_server_certificate(config: &Config) -> io::Result<(String, String)> {
    let cert_type = config.server.cert_type.as_deref().unwrap_or("self_signed");
    let app_name = env!("CARGO_PKG_NAME");
    let certs_dir = format!("/etc/{app_name}/certs");

    if !tokio::fs::try_exists(&certs_dir).await.unwrap_or(false) {
        info!("创建证书目录: {}", certs_dir);
        tokio::fs::create_dir_all(&certs_dir).await?;
    }

    let (cert_path, key_path) = match cert_type {
        "imported" => {
            if let Some((cert, key)) = get_latest_certificate(&certs_dir, "import").await {
                info!("使用导入的证书: {}", cert);
                (cert, key)
            } else {
                info!("未找到导入的证书，使用自签名证书");
                if let Some((cert, key)) = get_latest_certificate(&certs_dir, "create").await {
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
            if let Some((cert, key)) = get_latest_certificate(&certs_dir, "create").await {
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

    if !tokio::fs::try_exists(&cert_path).await.unwrap_or(false)
        || !tokio::fs::try_exists(&key_path).await.unwrap_or(false)
    {
        info!("生成自签名证书");
        use rcgen::generate_simple_self_signed;

        let certified_key = tokio::task::spawn_blocking(move || {
            generate_simple_self_signed(vec!["localhost".to_string()])
        })
        .await
        .map_err(io::Error::other)?
        .map_err(io::Error::other)?;

        let cert_pem = certified_key.cert.pem();
        let key_pem = certified_key.signing_key.serialize_pem();

        tokio::fs::write(&cert_path, cert_pem.as_bytes()).await?;
        tokio::fs::write(&key_path, key_pem.as_bytes()).await?;

        info!("自签名证书生成成功");
    }

    Ok((cert_path, key_path))
}

pub async fn load_rustls_config(
    cert_path: &str,
    key_path: &str,
) -> io::Result<rustls::ServerConfig> {
    let cert_data = tokio::fs::read(cert_path).await?;
    let key_data = tokio::fs::read(key_path).await?;

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
