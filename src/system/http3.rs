use crate::config::Config;
use crate::db::DbPool;
use anyhow::Context;
use bytes::{Buf, Bytes};
use h3::server::RequestResolver;
use h3_quinn::quinn;
use h3_quinn::quinn::crypto::rustls::QuicServerConfig;
use http::StatusCode;
use pem::parse_many;
use rustls_pki_types::CertificateDer;
use rustls_pki_types::PrivateKeyDer;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

static H3_ALPN: &[u8] = b"h3";

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub pool: Option<DbPool>,
    pub semaphore: Arc<Semaphore>,
    pub http_port: u16,
}

pub async fn start_http3_server(
    server_host: String,
    port: u16,
    cert_path: &str,
    key_path: &str,
    app_state: AppState,
) -> anyhow::Result<()> {
    let cert_data = std::fs::read(cert_path).context("读取证书文件失败")?;
    let key_data = std::fs::read(key_path).context("读取私钥文件失败")?;

    let certs = parse_many(&cert_data)
        .context("解析证书失败")?
        .into_iter()
        .filter(|p| p.tag() == "CERTIFICATE")
        .map(|p| CertificateDer::from(p.contents().to_vec()))
        .collect::<Vec<_>>();

    let keys = parse_many(&key_data)
        .context("解析私钥失败")?
        .into_iter()
        .filter(|p| p.tag() == "PRIVATE KEY")
        .map(|p| PrivateKeyDer::Pkcs8(p.contents().to_vec().into()))
        .collect::<Vec<_>>();

    if certs.is_empty() {
        return Err(anyhow::anyhow!("未找到有效证书"));
    }
    if keys.is_empty() {
        return Err(anyhow::anyhow!("未找到有效私钥"));
    }

    let mut tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, keys[0].clone_key())
        .map_err(|e| anyhow::anyhow!("创建TLS配置失败: {}", e))?;
    tls_config.alpn_protocols = vec![H3_ALPN.to_vec()];

    let quic_server_config = QuicServerConfig::try_from(tls_config)
        .map_err(|e| anyhow::anyhow!("创建QUIC服务端配置失败: {}", e))?;
    let server_config = quinn::ServerConfig::with_crypto(Arc::new(quic_server_config));

    let addr = parse_listen_addr(&server_host, port)?;

    let endpoint = quinn::Endpoint::server(server_config, addr)
        .with_context(|| format!("创建QUIC端点失败 (地址: {addr}), 端口可能被占用"))?;

    let local_addr = endpoint.local_addr().context("获取本地地址失败")?;
    info!("HTTP/3服务器监听在 {:?} (QUIC/UDP)", local_addr);

    let http_port = app_state.http_port;

    while let Some(incoming_conn) = endpoint.accept().await {
        let app_state = app_state.clone();
        tokio::spawn(async move {
            match incoming_conn.await {
                Ok(conn) => {
                    let remote = conn.remote_address();
                    debug!("新的QUIC连接: {:?}", remote);
                    if let Err(e) = handle_h3_connection(conn, app_state, http_port).await {
                        error!("HTTP/3连接处理错误: {:?}", e);
                    }
                }
                Err(e) => {
                    error!("接受QUIC连接失败: {:?}", e);
                }
            }
        });
    }

    Ok(())
}

async fn handle_h3_connection(
    conn: quinn::Connection,
    app_state: AppState,
    http_port: u16,
) -> anyhow::Result<()> {
    let quinn_conn = h3_quinn::Connection::new(conn);
    let mut h3_conn = h3::server::Connection::new(quinn_conn).await?;

    loop {
        match h3_conn.accept().await {
            Ok(Some(resolver)) => {
                let app_state = app_state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_h3_request(resolver, app_state, http_port).await {
                        warn!("HTTP/3请求处理错误: {:?}", e);
                    }
                });
            }
            Ok(None) => {
                debug!("HTTP/3连接关闭 (GOAWAY)");
                break;
            }
            Err(e) => {
                if e.is_h3_no_error() {
                    debug!("HTTP/3连接正常关闭");
                } else {
                    warn!("HTTP/3连接错误: {}", e);
                }
                break;
            }
        }
    }

    Ok(())
}

async fn handle_h3_request(
    resolver: RequestResolver<h3_quinn::Connection, Bytes>,
    app_state: AppState,
    http_port: u16,
) -> anyhow::Result<()> {
    let (request, mut stream) = resolver
        .resolve_request()
        .await
        .map_err(|e| anyhow::anyhow!("解析HTTP/3请求失败: {}", e))?;

    let method = request.method().clone();
    let uri = request.uri().clone();
    let path = uri.path().to_string();
    let host_header = request
        .headers()
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");

    debug!("HTTP/3请求: {} {} (host: {})", method, path, host_header);

    let mut request_body = Bytes::new();
    loop {
        match stream.recv_data().await {
            Ok(Some(mut chunk)) => {
                let buf = chunk.copy_to_bytes(chunk.remaining());
                if request_body.is_empty() {
                    request_body = buf;
                } else {
                    let mut combined = Vec::with_capacity(request_body.len() + buf.len());
                    combined.extend_from_slice(&request_body);
                    combined.extend_from_slice(&buf);
                    request_body = Bytes::from(combined);
                }
            }
            Ok(None) => break,
            Err(e) => {
                debug!("接收数据流错误: {}", e);
                break;
            }
        }
    }

    let response = proxy_to_local_server(
        &method,
        &uri,
        host_header,
        &request_body,
        http_port,
        &app_state,
    )
    .await;

    match response {
        Ok((status, headers, body)) => {
            let mut builder = http::Response::builder().status(status);
            for (name, value) in &headers {
                if let Ok(v) = http::HeaderValue::from_bytes(value.as_bytes()) {
                    builder = builder.header(name.as_str(), v);
                }
            }

            let resp = builder
                .body(())
                .map_err(|e| anyhow::anyhow!("构建响应失败: {}", e))?;

            stream
                .send_response(resp)
                .await
                .map_err(|e| anyhow::anyhow!("发送响应失败: {}", e))?;

            if !body.is_empty() {
                stream
                    .send_data(body)
                    .await
                    .map_err(|e| anyhow::anyhow!("发送数据失败: {}", e))?;
            }
            stream
                .finish()
                .await
                .map_err(|e| anyhow::anyhow!("结束流失败: {}", e))?;
        }
        Err(e) => {
            warn!("代理请求到本地服务器失败: {:?}", e);
            let resp = http::Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(())
                .map_err(|e| anyhow::anyhow!("构建错误响应失败: {}", e))?;

            stream
                .send_response(resp)
                .await
                .map_err(|e| anyhow::anyhow!("发送错误响应失败: {}", e))?;
            let body = Bytes::from("Bad Gateway: failed to proxy request");
            stream
                .send_data(body)
                .await
                .map_err(|e| anyhow::anyhow!("发送错误数据失败: {}", e))?;
            stream
                .finish()
                .await
                .map_err(|e| anyhow::anyhow!("结束错误流失败: {}", e))?;
        }
    }

    Ok(())
}

async fn proxy_to_local_server(
    method: &http::Method,
    uri: &http::Uri,
    host: &str,
    body: &[u8],
    http_port: u16,
    app_state: &AppState,
) -> anyhow::Result<(StatusCode, Vec<(String, String)>, Bytes)> {
    let path_and_query = uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(uri.path());

    let target = format!("http://127.0.0.1:{}{}", http_port, path_and_query);

    let mut req = http::Request::builder()
        .method(method)
        .uri(&target)
        .header("host", host)
        .header("x-forwarded-for", "127.0.0.1")
        .header("x-forwarded-proto", "https")
        .header("x-forwarded-host", host)
        .header("via", "h3");

    for (idx, value) in app_state.config.server.cors_allowed_origins.iter().enumerate() {
        if idx == 0 {
            req = req.header("x-original-origin", value.as_str());
        }
    }

    let request = if !body.is_empty() {
        req.body(body.to_vec())?
    } else {
        req.body(Vec::new())?
    };

    let response = simple_http_request(request).await?;

    Ok((response.status, response.headers, response.body))
}

struct SimpleHttpResponse {
    status: StatusCode,
    headers: Vec<(String, String)>,
    body: Bytes,
}

async fn simple_http_request(
    request: http::Request<Vec<u8>>,
) -> anyhow::Result<SimpleHttpResponse> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let uri = request.uri();
    let host = uri.host().unwrap_or("127.0.0.1");
    let port = uri.port_u16().unwrap_or(80);
    let method = request.method();
    let path = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    let body = request.body();

    let stream = TcpStream::connect((host, port))
        .await
        .with_context(|| format!("连接本地HTTP服务器失败 ({}:{})", host, port))?;
    let (mut read_half, mut write_half) = stream.into_split();

    let mut request_bytes = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
        method, path, host
    );

    let mut headers_to_forward = Vec::new();
    for (name, value) in request.headers() {
        let name_lower = name.as_str().to_lowercase();
        if name_lower == "host"
            || name_lower == "connection"
            || name_lower == "transfer-encoding"
        {
            continue;
        }
        if let Ok(v) = value.to_str() {
            headers_to_forward.push((name.as_str().to_string(), v.to_string()));
        }
    }

    if !body.is_empty() {
        headers_to_forward.push(("content-length".to_string(), body.len().to_string()));
    }

    for (name, value) in &headers_to_forward {
        request_bytes.push_str(&format!("{}: {}\r\n", name, value));
    }
    request_bytes.push_str("\r\n");

    write_half.write_all(request_bytes.as_bytes()).await?;
    if !body.is_empty() {
        write_half.write_all(body).await?;
    }
    write_half.shutdown().await?;

    let mut response_bytes = Vec::new();
    read_half.read_to_end(&mut response_bytes).await?;

    let response_str = String::from_utf8_lossy(&response_bytes);
    parse_http_response(&response_str, &response_bytes)
}

fn parse_http_response(
    response_str: &str,
    raw_bytes: &[u8],
) -> anyhow::Result<SimpleHttpResponse> {
    let header_end = response_str
        .find("\r\n\r\n")
        .ok_or_else(|| anyhow::anyhow!("HTTP响应格式无效: 未找到头部结束标记"))?;

    let header_section = &response_str[..header_end];
    let body_start = header_end + 4;

    let mut lines = header_section.lines();
    let status_line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("HTTP响应格式无效: 缺少状态行"))?;

    let status: StatusCode = parse_status_line(status_line)?;

    let mut headers = Vec::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(": ") {
            let name_lower = name.to_lowercase();
            if name_lower == "transfer-encoding"
                || name_lower == "connection"
                || name_lower == "keep-alive"
            {
                continue;
            }
            headers.push((name.to_string(), value.to_string()));
        }
    }

    let body = if body_start < raw_bytes.len() {
        Bytes::copy_from_slice(&raw_bytes[body_start..])
    } else {
        Bytes::new()
    };

    Ok(SimpleHttpResponse {
        status,
        headers,
        body,
    })
}

fn parse_status_line(line: &str) -> anyhow::Result<StatusCode> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(anyhow::anyhow!("无效的HTTP状态行: {}", line));
    }
    let code: u16 = parts[1].parse().context("解析HTTP状态码失败")?;
    StatusCode::from_u16(code).context("无效的HTTP状态码")
}

fn parse_listen_addr(host: &str, port: u16) -> anyhow::Result<SocketAddr> {
    match host {
        "::" => SocketAddr::from_str(&format!("[::]:{port}")).context("解析IPv6双栈地址失败"),
        "0.0.0.0" => {
            SocketAddr::from_str(&format!("0.0.0.0:{port}")).context("解析IPv4地址失败")
        }
        _ => {
            if host.contains(':') {
                SocketAddr::from_str(&format!("[{host}]:{port}")).context("解析IPv6地址失败")
            } else {
                SocketAddr::from_str(&format!("{host}:{port}")).context("解析服务器地址失败")
            }
        }
    }
}
