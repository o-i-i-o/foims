use anyhow::Context;
use quinn::{Endpoint, ServerConfig};
use std::net::SocketAddr;
use tracing::info;
use crate::utils::buffer_pool::BUFFER_POOL;

// 启动 HTTP/3 服务器
pub async fn start_http3_server(
    server_host: String,
    port: u16,
    cert_path: &str,
    key_path: &str,
) -> anyhow::Result<()> {
    use pem::parse_many;
    use rustls_pki_types::CertificateDer;
    use rustls_pki_types::PrivateKeyDer;

    // 读取证书和私钥文件
    let cert_data = std::fs::read(cert_path)?;
    let key_data = std::fs::read(key_path)?;

    // 解析证书
    let certs = parse_many(&cert_data)?
        .into_iter()
        .filter(|pem| pem.tag() == "CERTIFICATE")
        .map(|pem| CertificateDer::from(pem.contents().to_vec()))
        .collect::<Vec<_>>();

    // 解析私钥
    let keys = parse_many(&key_data)?
        .into_iter()
        .filter(|pem| pem.tag() == "PRIVATE KEY")
        .map(|pem| PrivateKeyDer::Pkcs8(pem.contents().to_vec().into()))
        .collect::<Vec<_>>();

    if certs.is_empty() || keys.is_empty() {
        return Err(anyhow::anyhow!("No valid certificate or private key found"));
    }

    // 创建服务器配置
    let server_config = ServerConfig::with_single_cert(certs, keys[0].clone_key())
        .context("Failed to create server config")?;

    // 绑定到指定地址
    let addr = if server_host == "::" {
        // 处理IPv6通配符地址
        format!("[::]:{}", port).parse::<SocketAddr>()?
    } else {
        format!("{}:{}", server_host, port).parse::<SocketAddr>()?
    };

    // 创建 QUIC 端点
    let endpoint = Endpoint::server(server_config, addr)
        .context("Failed to create endpoint")?;

    info!("HTTP/3服务器监听在 {:?}", endpoint.local_addr()?);

    // 处理传入的连接
    while let Some(conn) = endpoint.accept().await {
        tokio::spawn(async move {
            if let Err(e) = handle_h3_connection_with_h3(conn).await {
                info!("连接错误: {:?}", e);
            }
        });
    }

    Ok(())
}

// 优化的 HTTP/3 连接处理
async fn handle_h3_connection_with_h3(conn: quinn::Incoming) -> anyhow::Result<()> {
    let connection = conn.await.context("Failed to accept QUIC connection")?;
    info!("新的HTTP/3连接: {:?}", connection.remote_address());

    // 处理连接上的流
    loop {
        tokio::select! {
            // 接受单向流（通常用于请求）
            uni_stream = connection.accept_uni() => {
                match uni_stream {
                    Ok(stream) => {
                        tokio::spawn(async move {
                            if let Err(e) = handle_stream(stream).await {
                                info!("流错误: {:?}", e);
                            }
                        });
                    }
                    Err(e) => {
                        info!("接受单向流错误: {:?}", e);
                        break;
                    }
                }
            }

            // 接受双向流
            bi_stream = connection.accept_bi() => {
                match bi_stream {
                    Ok((send_stream, recv_stream)) => {
                        tokio::spawn(async move {
                            if let Err(e) = handle_bi_stream_optimized(send_stream, recv_stream).await {
                                info!("双向流错误: {:?}", e);
                            }
                        });
                    }
                    Err(e) => {
                        info!("接受双向流错误: {:?}", e);
                        break;
                    }
                }
            }

            // 连接关闭
            _ = connection.closed() => {
                info!("连接关闭: {:?}", connection.remote_address());
                break;
            }
        }
    }

    Ok(())
}

// 优化的 HTTP/3 双向流处理
async fn handle_bi_stream_optimized(
    mut send_stream: quinn::SendStream,
    mut recv_stream: quinn::RecvStream,
) -> anyhow::Result<()> {
    // 从缓冲区池获取缓冲区
    let mut buf = BUFFER_POOL.get();
    let mut chunk = [0; 1024];

    // 流式处理请求体
    loop {
        match recv_stream.read(&mut chunk).await {
            Ok(Some(0)) => break, // 流结束
            Ok(Some(n)) => buf.extend_from_slice(&chunk[..n]),
            Ok(None) => break, // 流结束
            Err(e) => {
                // 归还缓冲区到池
                BUFFER_POOL.put(buf);
                info!("读取请求流错误: {:?}", e);
                return Err(e.into());
            }
        }
    }

    // 解析并处理HTTP/3请求
    let response = process_http3_request_optimized(&buf).await?;

    // 归还缓冲区到池
    BUFFER_POOL.put(buf);

    // 发送响应
    send_stream
        .write_all(&response)
        .await
        .context("Failed to send response")?;

    // 结束发送流
    send_stream
        .finish()
        .context("Failed to finish send stream")?;

    Ok(())
}

// 优化的 HTTP/3 请求处理
async fn process_http3_request_optimized(request_data: &[u8]) -> anyhow::Result<Vec<u8>> {
    // 尝试解析HTTP请求
    let request_str = String::from_utf8_lossy(request_data);
    info!("接收到HTTP/3请求: {}", request_str);

    // 简单的HTTP请求解析
    let mut method = "GET";
    let mut path = "/";

    // 解析请求行
    if let Some(first_line) = request_str.lines().next() {
        let parts: Vec<&str> = first_line.split_whitespace().collect();
        if parts.len() >= 2 {
            method = parts[0];
            path = parts[1];
        }
    }

    // 处理不同的路径
    let (status, content_type, body) = match path {
        "/" => {
            // 根路径重定向到登录页面
            let _body = "";
            let location = "/static/index.html";
            let response = format!(
                "HTTP/3 302 Found\r\ncontent-type: text/plain\r\ncontent-length: 0\r\nlocation: {}\r\n\r\n",
                location
            );
            return Ok(response.into_bytes());
        }
        "/health" => {
            // 健康检查
            let body = r#"{"status": "ok"}"#;
            ("200 OK", "application/json", body)
        }
        "/api/auth/login" if method == "POST" => {
            // 登录请求
            let body = r#"{"success": true, "message": "Login endpoint accessed via HTTP/3"}"#;
            ("200 OK", "application/json", body)
        }
        _ if path.starts_with("/static/") => {
            // 静态文件请求
            let body = "Static file access via HTTP/3";
            ("200 OK", "text/plain", body)
        }
        _ if path.starts_with("/api/") => {
            // API请求
            let body = r#"{"success": true, "message": "API endpoint accessed via HTTP/3"}"#;
            ("200 OK", "application/json", body)
        }
        _ => {
            // 默认响应
            let body = "Hello from HTTP/3 server!";
            ("200 OK", "text/plain", body)
        }
    };

    // 生成HTTP/3响应
    let response = format!(
        "HTTP/3 {}\r\ncontent-type: {}\r\ncontent-length: {}\r\n\r\n{}",
        status,
        content_type,
        body.len(),
        body
    );

    Ok(response.into_bytes())
}

// 处理 HTTP/3 流
async fn handle_stream(mut stream: quinn::RecvStream) -> anyhow::Result<()> {
    // 读取流数据
    let mut buf = Vec::new();
    let mut chunk = [0; 1024];

    loop {
        match stream.read(&mut chunk).await {
            Ok(Some(0)) => break, // 流结束
            Ok(Some(n)) => buf.extend_from_slice(&chunk[..n]),
            Ok(None) => break, // 流结束
            Err(e) => return Err(e.into()),
        }
    }

    // 处理单向流数据（通常用于服务器推送等）
    info!("在HTTP/3单向流上接收到 {} 字节", buf.len());

    Ok(())
}
