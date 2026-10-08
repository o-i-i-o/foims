//! HTTP/3 mTLS 通信可行性 demo（服务端 + 客户端一体，双子命令）。
//!
//! 目的：验证 docs/agent-design.md §2/§3 的关键假设——自签证书体系下，
//! quinn（QUIC，即 HTTP/3 的传输层）+ rustls 可以完成：
//! 1. 双向 mTLS：服务端强制校验客户端证书，无证书连接在握手阶段被拒；
//! 2. 加密传输：全程 TLS 1.3（可用 tcpdump 抓 UDP 9100 包佐证链路无明文）；
//! 3. 指标上报：客户端用真实采集器产出 Prometheus 文本，POST 到服务端并收
//!    到回执。
//!
//! 准备：`scripts/gen-h3-demo-certs.sh [目录]` 生成 demo CA 与两端证书。
//! 运行（需两个终端，服务端先启动）：
//! - 服务端：`cargo run -p foims-agent --example h3_demo -- server`
//! - 客户端：`cargo run -p foims-agent --example h3_demo -- client`
//! - 反例：  `cargo run -p foims-agent --example h3_demo -- client --no-cert`
//!   （预期：服务端拒绝无证书连接，客户端握手失败退出）

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use bytes::{Buf, Bytes};
use http::{Method, Request, Response, StatusCode};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// 统一错误类型（demo 简化：装箱任意 Error，附 Send+Sync 以便跨协程传递）
type DemoError = Box<dyn std::error::Error + Send + Sync>;

/// h3 请求流类型（正式实施将以薄 trait 封装，隔离 0.0.x API 变动）
type AgentStream = h3::server::RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>;

/// 指标上报路径（正式协议路径的占位）
const REPORT_PATH: &str = "/api/v1/agent/report";
/// 服务端请求体大小上限：8 MiB（防御性上限）
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
/// 客户端各阶段超时
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

const USAGE: &str = "用法: h3_demo server [--listen ADDR:PORT] [--certs-dir DIR]\n      h3_demo client [--addr ADDR:PORT] [--host 名称] [--certs-dir DIR] [--no-cert]";

fn main() -> ExitCode {
    init_tracing();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!("构建 tokio 运行时失败: {error}");
            return ExitCode::FAILURE;
        }
    };
    let result = match args.first().map(String::as_str) {
        Some("server") => runtime.block_on(server_main(args)),
        Some("client") => runtime.block_on(client_main(args)),
        _ => {
            tracing::error!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(code) => code,
        Err(error) => {
            tracing::error!(%error, "demo 执行失败");
            ExitCode::FAILURE
        }
    }
}

/// 初始化日志：默认 info 级，可用 RUST_LOG 覆盖；全部写入 stderr
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

/// demo 证书默认目录
fn default_certs_dir() -> String {
    "/tmp/foims-h3-demo-certs".to_string()
}

/// 读取 `--flag value` 形式的参数值
fn arg_value(args: &[String], flag: &str) -> Result<Option<String>, DemoError> {
    let Some(index) = args.iter().position(|item| item == flag) else {
        return Ok(None);
    };
    let value = args
        .get(index + 1)
        .ok_or_else(|| format!("{flag} 缺少参数"))?;
    Ok(Some(value.clone()))
}

/// 读取 PEM 证书文件（可能含多张）
fn load_certs(label: &str, path: &Path) -> Result<Vec<CertificateDer<'static>>, DemoError> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("打开{label}证书失败 {path:?}: {error}"))?;
    let mut reader = std::io::BufReader::new(file);
    Ok(rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析{label}证书失败 {path:?}: {error}"))?)
}

/// 读取 PEM 私钥
fn load_key(label: &str, path: &Path) -> Result<PrivateKeyDer<'static>, DemoError> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("打开{label}私钥失败 {path:?}: {error}"))?;
    let mut reader = std::io::BufReader::new(file);
    let key = rustls_pemfile::private_key(&mut reader)
        .map_err(|error| format!("解析{label}私钥失败 {path:?}: {error}"))?;
    match key {
        Some(key) => Ok(key),
        None => Err(format!("{label}私钥文件无有效内容: {path:?}").into()),
    }
}

// ---------------------------------------------------------------------------
// 服务端：监听 UDP 9100，强制客户端证书，接收指标上报
// ---------------------------------------------------------------------------

async fn server_main(args: Vec<String>) -> Result<ExitCode, DemoError> {
    let listen: SocketAddr = arg_value(&args, "--listen")?
        .unwrap_or_else(|| "127.0.0.1:9100".to_string())
        .parse()
        .map_err(|error| format!("监听地址非法: {error}"))?;
    let certs_dir =
        PathBuf::from(arg_value(&args, "--certs-dir")?.unwrap_or_else(default_certs_dir));

    // TLS 配置：仅信任 demo CA 签发的客户端证书（WebPkiClientVerifier 强制要求）
    let mut roots = rustls::RootCertStore::empty();
    for cert in load_certs("CA", &certs_dir.join("ca.pem"))? {
        roots.add(cert)?;
    }
    let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots))
        .build()
        .map_err(|error| format!("构建客户端证书校验器失败: {error}"))?;
    let mut tls = rustls::ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(
            load_certs("服务端", &certs_dir.join("server.pem"))?,
            load_key("服务端", &certs_dir.join("server.key"))?,
        )
        .map_err(|error| format!("加载服务端证书/私钥失败: {error}"))?;
    // HTTP/3 要求 ALPN 协议名为 "h3"
    tls.alpn_protocols = vec![b"h3".to_vec()];
    let server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(tls)?,
    ));
    let endpoint = quinn::Endpoint::server(server_config, listen)?;
    tracing::info!(%listen, "demo 服务端已监听（UDP/QUIC，强制客户端证书），Ctrl-C 退出");

    while let Some(connecting) = endpoint.accept().await {
        tokio::spawn(async move {
            if let Err(error) = handle_connection(connecting).await {
                tracing::debug!(%error, "连接处理结束");
            }
        });
    }
    Ok(ExitCode::SUCCESS)
}

/// 单连接处理：展示已验证的客户端证书，循环接受 h3 请求
async fn handle_connection(incoming: quinn::Incoming) -> Result<(), DemoError> {
    let connection = incoming.await?;
    let peer = connection.remote_address();
    // rustls 已在握手期完成客户端证书验证，这里仅取出展示
    let client_certs = connection
        .peer_identity()
        .and_then(|identity| identity.downcast::<Vec<CertificateDer<'static>>>().ok())
        .map(|certs| certs.len())
        .unwrap_or(0);
    tracing::info!(%peer, certs = client_certs, "QUIC 连接建立：客户端证书已验证");

    let mut h3_conn = h3::server::Connection::new(h3_quinn::Connection::new(connection)).await?;
    // h3 0.0.8：accept 产出 RequestResolver，再解析出请求与流
    while let Some(resolver) = h3_conn.accept().await? {
        let (request, stream) = resolver.resolve_request().await?;
        if let Err(error) = handle_request(request, stream).await {
            tracing::warn!(%error, "请求处理失败");
        }
    }
    tracing::debug!(%peer, "连接关闭");
    Ok(())
}

/// 单请求处理：校验路径/方法，读取全部请求体并回执
async fn handle_request(request: Request<()>, mut stream: AgentStream) -> Result<(), DemoError> {
    let path = request.uri().path().to_string();

    // 读取请求体（链路上为 TLS 1.3 密文，此处为服务端解密后的明文）
    let mut body: Vec<u8> = Vec::new();
    loop {
        match stream.recv_data().await? {
            Some(chunk) => {
                body.extend_from_slice(chunk.chunk());
                if body.len() > MAX_BODY_BYTES {
                    send_response(
                        &mut stream,
                        StatusCode::PAYLOAD_TOO_LARGE,
                        b"request body too large",
                    )
                    .await?;
                    return Ok(());
                }
            }
            None => break,
        }
    }

    if path != REPORT_PATH {
        send_response(&mut stream, StatusCode::NOT_FOUND, b"unknown path").await?;
        return Ok(());
    }
    if request.method() != Method::POST {
        send_response(&mut stream, StatusCode::METHOD_NOT_ALLOWED, b"POST only").await?;
        return Ok(());
    }

    let preview = String::from_utf8_lossy(&body[..body.len().min(96)]);
    tracing::info!(
        %path,
        bytes = body.len(),
        first_line = preview.lines().next().unwrap_or(""),
        "已接收指标上报（服务端解密后明文）"
    );
    send_response(&mut stream, StatusCode::OK, b"foims-demo-ok").await?;
    Ok(())
}

/// 发送纯文本响应并结束流
async fn send_response(
    stream: &mut AgentStream,
    status: StatusCode,
    text: &'static [u8],
) -> Result<(), DemoError> {
    let response = Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(())?;
    stream.send_response(response).await?;
    stream.send_data(Bytes::from_static(text)).await?;
    stream.finish().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 客户端：采集本机指标，经 HTTP/3 mTLS 上报
// ---------------------------------------------------------------------------

async fn client_main(args: Vec<String>) -> Result<ExitCode, DemoError> {
    let addr: SocketAddr = arg_value(&args, "--addr")?
        .unwrap_or_else(|| "127.0.0.1:9100".to_string())
        .parse()
        .map_err(|error| format!("服务地址非法: {error}"))?;
    let server_name = arg_value(&args, "--host")?.unwrap_or_else(|| "localhost".to_string());
    let certs_dir =
        PathBuf::from(arg_value(&args, "--certs-dir")?.unwrap_or_else(default_certs_dir));
    let no_cert = args.iter().any(|item| item == "--no-cert");

    // 1. 采集真实指标（复用正式采集框架，与 foims-agent CLI 同一数据源）
    let collectors = foims_agent::default_collectors(None);
    let families = foims_agent::scrape(&collectors);
    let body = foims_agent::encode_text_all(&families);
    tracing::info!(
        families = families.len(),
        bytes = body.len(),
        "已采集本机指标"
    );

    // 2. TLS 配置：信任 demo CA；默认携带客户端证书，--no-cert 反例不带
    let mut roots = rustls::RootCertStore::empty();
    for cert in load_certs("CA", &certs_dir.join("ca.pem"))? {
        roots.add(cert)?;
    }
    let mut tls = if no_cert {
        tracing::warn!("--no-cert 模式：不带客户端证书（预期被服务端拒绝）");
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    } else {
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_client_auth_cert(
                load_certs("客户端", &certs_dir.join("agent.pem"))?,
                load_key("客户端", &certs_dir.join("agent.key"))?,
            )
            .map_err(|error| format!("加载客户端证书/私钥失败: {error}"))?
    };
    tls.alpn_protocols = vec![b"h3".to_vec()];
    let client_config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls)?,
    ));

    // 本地端点与目标地址族保持一致（IPv6 目标绑定 [::]:0）
    let local: SocketAddr = if addr.is_ipv4() {
        "0.0.0.0:0".parse()?
    } else {
        "[::]:0".parse()?
    };
    let mut endpoint = quinn::Endpoint::client(local)?;
    endpoint.set_default_client_config(client_config);

    // 3. QUIC 握手（TLS 1.3；SNI/证书校验名默认 localhost，须命中服务端 SAN）
    let connecting = endpoint.connect(addr, &server_name)?;
    let connection = tokio::time::timeout(CLIENT_TIMEOUT, connecting)
        .await
        .map_err(|_| format!("连接 {addr} 超时（服务端未监听？）"))??;
    tracing::info!(
        remote = %connection.remote_address(),
        "QUIC 连接建立（TLS 1.3 加密 + 客户端证书认证）"
    );

    // 4. HTTP/3 请求上报
    let (mut driver, mut conn) = h3::client::new(h3_quinn::Connection::new(connection)).await?;
    tokio::spawn(async move {
        // h3 0.0.8：wait_idle 结束时直接返回终态错误（正常关闭也如此）
        let error = driver.wait_idle().await;
        tracing::debug!(%error, "h3 驱动结束");
    });
    let request = Request::builder()
        .method(Method::POST)
        .uri(report_uri(&server_name, addr.port()))
        .header("content-type", "text/plain; version=0.0.4; charset=utf-8")
        .header(
            "user-agent",
            concat!("foims-agent-demo/", env!("CARGO_PKG_VERSION")),
        )
        .body(())?;
    let mut stream = tokio::time::timeout(CLIENT_TIMEOUT, conn.send_request(request)).await??;
    stream.send_data(Bytes::from(body)).await?;
    stream.finish().await?;
    let response = tokio::time::timeout(CLIENT_TIMEOUT, stream.recv_response()).await??;
    let mut resp_body: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.recv_data().await? {
        resp_body.extend_from_slice(chunk.chunk());
    }
    tracing::info!(
        status = %response.status(),
        body = %String::from_utf8_lossy(&resp_body),
        "收到服务端回执，上报完成"
    );
    endpoint.close(0u32.into(), b"done");
    Ok(ExitCode::SUCCESS)
}

/// 组装上报 URL（IPv6 字面量主机名需方括号，demo 兜底处理）
fn report_uri(server_name: &str, port: u16) -> String {
    let host = if server_name.contains(':') {
        format!("[{server_name}]")
    } else {
        server_name.to_string()
    };
    format!("https://{host}:{port}{}", REPORT_PATH)
}
