use crate::auth::utils::JwtUtils;
use crate::config::Config;
use crate::db::DbPool;
use crate::utils::buffer_pool::get_buffer_pool;
use anyhow::Context;
use quinn::{Endpoint, ServerConfig};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

// 应用状态
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub pool: Option<DbPool>,
    pub semaphore: Arc<Semaphore>,
    pub rt_handle: Handle,
}

// 启动 HTTP/3 服务器
pub async fn start_http3_server(
    server_host: String,
    port: u16,
    cert_path: &str,
    key_path: &str,
    app_state: AppState,
) -> anyhow::Result<()> {
    use pem::parse_many;
    use rustls_pki_types::CertificateDer;
    use rustls_pki_types::PrivateKeyDer;

    // 读取证书和私钥文件
    let cert_data = std::fs::read(cert_path).context("Failed to read certificate file")?;
    let key_data = std::fs::read(key_path).context("Failed to read private key file")?;

    // 解析证书
    let certs = parse_many(&cert_data)
        .context("Failed to parse certificate")?
        .into_iter()
        .filter(|pem| pem.tag() == "CERTIFICATE")
        .map(|pem| CertificateDer::from(pem.contents().to_vec()))
        .collect::<Vec<_>>();

    // 解析私钥
    let keys = parse_many(&key_data)
        .context("Failed to parse private key")?
        .into_iter()
        .filter(|pem| pem.tag() == "PRIVATE KEY")
        .map(|pem| PrivateKeyDer::Pkcs8(pem.contents().to_vec().into()))
        .collect::<Vec<_>>();

    if certs.is_empty() {
        return Err(anyhow::anyhow!("No valid certificate found"));
    }

    if keys.is_empty() {
        return Err(anyhow::anyhow!("No valid private key found"));
    }

    // 创建服务器配置
    let server_config = ServerConfig::with_single_cert(certs, keys[0].clone_key())
        .context("Failed to create server config")?;

    // 绑定到指定地址
    let addr = match server_host.as_str() {
        "::" => {
            // IPv6双栈模式
            format!("[::]:{port}")
                .parse::<SocketAddr>()
                .context("Failed to parse IPv6 address")?
        }
        "0.0.0.0" => {
            // IPv4通配符地址
            format!("0.0.0.0:{port}")
                .parse::<SocketAddr>()
                .context("Failed to parse IPv4 address")?
        }
        _ => {
            // 具体IP地址
            format!("{server_host}:{port}")
                .parse::<SocketAddr>()
                .context("Failed to parse server address")?
        }
    };

    // 创建 QUIC 端点
    let endpoint =
        Endpoint::server(server_config, addr).context("Failed to create QUIC endpoint")?;

    let local_addr = endpoint
        .local_addr()
        .context("Failed to get local address")?;
    info!("HTTP/3服务器监听在 {:?} (QUIC与TLS共存)", local_addr);

    // 处理传入的连接
    while let Some(conn) = endpoint.accept().await {
        let app_state_clone = app_state.clone();
        app_state.rt_handle.spawn(async move {
            if let Err(e) = handle_h3_connection_with_h3(conn, app_state_clone).await {
                warn!("Connection error: {:?}", e);
            }
        });
    }

    Ok(())
}

// 优化的 HTTP/3 连接处理
async fn handle_h3_connection_with_h3(
    conn: quinn::Incoming,
    app_state: AppState,
) -> anyhow::Result<()> {
    let connection = conn.await.context("Failed to accept QUIC connection")?;
    let remote_addr = connection.remote_address();
    debug!("新的HTTP/3连接: {:?}", remote_addr);

    // 处理连接上的流
    loop {
        tokio::select! {
            // 接受单向流（通常用于请求）
            uni_stream = connection.accept_uni() => {
                match uni_stream {
                    Ok(stream) => {
                        let app_state_clone = app_state.clone();
                        app_state.rt_handle.spawn(async move {
                            // 获取信号量许可
                            let permit = app_state_clone.semaphore.acquire().await;
                            if let Ok(_permit) = permit {
                                if let Err(e) = handle_stream(stream).await {
                                    debug!("流错误: {:?}", e);
                                }
                                // 许可会在作用域结束时自动释放
                            } else {
                                warn!("Failed to acquire semaphore permit for uni stream");
                            }
                        });
                    }
                    Err(e) => {
                        debug!("接受单向流错误: {:?}", e);
                        break;
                    }
                }
            }

            // 接受双向流
            bi_stream = connection.accept_bi() => {
                match bi_stream {
                    Ok((send_stream, recv_stream)) => {
                        let app_state_clone = app_state.clone();
                        let semaphore_clone = app_state_clone.semaphore.clone();
                        app_state.rt_handle.spawn(async move {
                            // 获取信号量许可
                            let permit = semaphore_clone.acquire().await;
                            if let Ok(_permit) = permit {
                                if let Err(e) = handle_bi_stream_optimized(send_stream, recv_stream, app_state_clone).await {
                                    debug!("双向流错误: {:?}", e);
                                }
                                // 许可会在作用域结束时自动释放
                            } else {
                                warn!("Failed to acquire semaphore permit for bi stream");
                            }
                        });
                    }
                    Err(e) => {
                        debug!("接受双向流错误: {:?}", e);
                        break;
                    }
                }
            }

            // 连接关闭
            _ = connection.closed() => {
                debug!("连接关闭: {:?}", remote_addr);
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
    app_state: AppState,
) -> anyhow::Result<()> {
    // 从缓冲区池获取缓冲区
    let buffer_pool = get_buffer_pool();
    let mut buf = buffer_pool.get();
    let mut chunk = [0; 1024];

    // 流式处理请求体
    loop {
        match recv_stream.read(&mut chunk).await {
            Ok(Some(0)) | Ok(None) => break,
            Ok(Some(n)) => buf.extend_from_slice(&chunk[..n]),
            Err(e) => {
                // 归还缓冲区到池
                buffer_pool.put(buf);
                debug!("读取请求流错误: {:?}", e);
                return Err(e.into());
            }
        }
    }

    // 解析并处理HTTP/3请求
    let response = process_http3_request_optimized(&buf, app_state)?;

    // 归还缓冲区到池
    buffer_pool.put(buf);

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
fn process_http3_request_optimized(
    request_data: &[u8],
    app_state: AppState,
) -> anyhow::Result<Vec<u8>> {
    // 尝试解析HTTP请求
    let request_str = String::from_utf8_lossy(request_data);
    debug!("接收到HTTP/3请求: {}", request_str);

    // 简单的HTTP请求解析
    let mut _method = "GET";
    let mut path = "/";
    let mut headers = std::collections::HashMap::new();

    // 解析请求行和头信息
    let mut lines = request_str.lines();
    if let Some(first_line) = lines.next() {
        let parts: Vec<&str> = first_line.split_whitespace().collect();
        if parts.len() >= 2 {
            _method = parts[0];
            path = parts[1];
        }
    }

    // 解析头信息
    for line in lines {
        if line.is_empty() {
            break; // 头信息结束
        }
        if let Some((key, value)) = line.split_once(": ") {
            headers.insert(key.to_lowercase(), value.to_string());
        }
    }

    // 处理认证
    let mut user_info = None;
    if let Some(auth_header) = headers.get("authorization")
        && auth_header.starts_with("Bearer ")
    {
        let token = auth_header.trim_start_matches("Bearer ");
        let jwt_utils = JwtUtils::new(&app_state.config);
        if let Ok(claims) = jwt_utils.validate_token(token) {
            user_info = Some((claims.sub, claims.username, claims.role));
        }
    }

    // 处理不同的路径
    let response = match path {
        "/" => {
            // 根路径重定向到登录页面
            let location = "/static/index.html";
            format!(
                "HTTP/3 302 Found\r\ncontent-type: text/plain\r\ncontent-length: 0\r\nlocation: {location}\r\n\r\n"
            )
        }
        "/health" => {
            // 健康检查
            let body = json!({"status": "ok"}).to_string();
            format!(
                "HTTP/3 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
        }
        "/api/auth/me" => {
            // 获取当前用户信息
            if let Some((user_id, username, role)) = user_info {
                let body = json!({"success": true, "message": "Success", "data": {"id": user_id, "username": username, "role": role}}).to_string();
                format!(
                    "HTTP/3 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
            } else {
                let body =
                    json!({"success": false, "message": "Unauthorized", "data": null}).to_string();
                format!(
                    "HTTP/3 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
            }
        }
        _ if path.starts_with("/static/") => {
            // 静态文件请求
            let body = "Static file access via HTTP/3";
            format!(
                "HTTP/3 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
        }
        _ if path.starts_with("/api/") => {
            // API请求
            if user_info.is_some() {
                let body = json!({"success": true, "message": "API endpoint accessed via HTTP/3"})
                    .to_string();
                format!(
                    "HTTP/3 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
            } else {
                let body =
                    json!({"success": false, "message": "Unauthorized", "data": null}).to_string();
                format!(
                    "HTTP/3 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
            }
        }
        _ => {
            // 默认响应
            let body = "Hello from HTTP/3 server!";
            format!(
                "HTTP/3 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
        }
    };

    Ok(response.into_bytes())
}

// 处理 HTTP/3 流
async fn handle_stream(mut stream: quinn::RecvStream) -> anyhow::Result<()> {
    // 读取流数据
    let mut buf = Vec::new();
    let mut chunk = [0; 1024];

    loop {
        match stream.read(&mut chunk).await {
            Ok(Some(0)) | Ok(None) => break,
            Ok(Some(n)) => buf.extend_from_slice(&chunk[..n]),
            Err(e) => return Err(e.into()),
        }
    }

    // 处理单向流数据（通常用于服务器推送等）
    info!("在HTTP/3单向流上接收到 {} 字节", buf.len());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_app_state_creation() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // 创建一个简单的配置
            let config = Config {
                database: crate::config::DatabaseConfig {
                    host: "localhost".to_string(),
                    port: 5432,
                    database: "ipma".to_string(),
                    username: "postgres".to_string(),
                    password: "password".to_string(),
                    max_connections: 10,
                    query_timeout_secs: 30,
                    slow_query_threshold_ms: 1000,
                },
                server: crate::config::ServerConfig {
                    host: "0.0.0.0".to_string(),
                    host_ipv6: Some("::".to_string()),
                    http_enabled: Some(true),
                    http_port: Some(80),
                    https_enabled: Some(true),
                    https_port: Some(443),
                    auto_https: Some(false),
                    http_version: Some("HTTP/3".to_string()),
                    cert_type: Some("self_signed".to_string()),
                    public_url: "http://localhost".to_string(),
                    session_timeout: Some(30),
                    page_timeout: Some(30),
                },
                jwt: crate::config::JwtConfig {
                    secret: "test_secret".to_string(),
                    access_token_expiry: "15m".to_string(),
                    refresh_token_expiry: "7d".to_string(),
                },
                init: crate::config::InitConfig { enabled: false },
                i18n: Some(crate::config::I18nConfig {
                    default_language: "zh".to_string(),
                    supported_languages: vec!["zh".to_string(), "en".to_string()],
                }),
                rate_limit: crate::config::RateLimitConfig::default(),
            };

            // 创建 AppState
            let semaphore = Arc::new(Semaphore::new(100));
            let rt_handle = Handle::current();

            let app_state = AppState {
                config,
                pool: None,
                semaphore,
                rt_handle,
            };

            // 验证 AppState 创建成功
            assert_eq!(app_state.semaphore.available_permits(), 100);
        });
    }

    #[test]
    fn test_process_http3_request() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // 创建一个简单的配置
            let config = Config {
                database: crate::config::DatabaseConfig {
                    host: "localhost".to_string(),
                    port: 5432,
                    database: "ipma".to_string(),
                    username: "postgres".to_string(),
                    password: "password".to_string(),
                    max_connections: 10,
                    query_timeout_secs: 30,
                    slow_query_threshold_ms: 1000,
                },
                server: crate::config::ServerConfig {
                    host: "0.0.0.0".to_string(),
                    host_ipv6: Some("::".to_string()),
                    http_enabled: Some(true),
                    http_port: Some(80),
                    https_enabled: Some(true),
                    https_port: Some(443),
                    auto_https: Some(false),
                    http_version: Some("HTTP/3".to_string()),
                    cert_type: Some("self_signed".to_string()),
                    public_url: "http://localhost".to_string(),
                    session_timeout: Some(30),
                    page_timeout: Some(30),
                },
                jwt: crate::config::JwtConfig {
                    secret: "test_secret".to_string(),
                    access_token_expiry: "15m".to_string(),
                    refresh_token_expiry: "7d".to_string(),
                },
                init: crate::config::InitConfig { enabled: false },
                i18n: Some(crate::config::I18nConfig {
                    default_language: "zh".to_string(),
                    supported_languages: vec!["zh".to_string(), "en".to_string()],
                }),
                rate_limit: crate::config::RateLimitConfig::default(),
            };

            // 创建 AppState
            let semaphore = Arc::new(Semaphore::new(100));
            let rt_handle = Handle::current();

            let app_state = AppState {
                config,
                pool: None,
                semaphore,
                rt_handle,
            };

            // 测试健康检查请求
            let health_request = "GET /health HTTP/3\r\nHost: localhost\r\n\r\n";
            let response =
                process_http3_request_optimized(health_request.as_bytes(), app_state.clone())
                    .unwrap();
            let response_str = String::from_utf8_lossy(&response);
            assert!(response_str.contains("200 OK"));
            assert!(response_str.contains("status"));
            assert!(response_str.contains("ok"));

            // 测试根路径重定向
            let root_request = "GET / HTTP/3\r\nHost: localhost\r\n\r\n";
            let response =
                process_http3_request_optimized(root_request.as_bytes(), app_state.clone())
                    .unwrap();
            let response_str = String::from_utf8_lossy(&response);
            assert!(response_str.contains("302 Found"));
            assert!(response_str.contains("location: /static/index.html"));
        });
    }
}
