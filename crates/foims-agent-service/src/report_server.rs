//! QUIC/HTTP3 指标接收服务端（设计 docs/agent-design.md §3.3）。
//!
//! 监听 UDP（默认 [::]:9100 双栈），以站点 CA 为唯一客户端信任锚强制 mTLS
//! （无客户端证书的连接在握手阶段被拒）。每条连接循环接受 h3 请求，
//! 仅放行 `POST /agent/v1/report`，读满请求体（超限回 413）后交由
//! [`crate::ingest::ingest`] 处理并回 JSON。
//!
//! h3 0.0.8 accept 形态与 crates/foims-agent/examples/h3_demo.rs 保持一致：
//! Incoming → Connection → accept() → Option<RequestResolver> → resolve_request()。

use std::sync::Arc;

use axum::Json;
use bytes::{Buf, Bytes};
use http::{Method, Request, Response, StatusCode, header};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde_json::Value;

use foims_common::config::AgentConfig;
use foims_common::net::normalize_ipv4_address;
use foims_common::{log_debug, log_error, log_info, log_warn};

use crate::ingest;
use crate::renew;

/// 上报端点路径（协议约定 §3.3）
pub const REPORT_PATH: &str = "/agent/v1/report";
/// 客户端证书续期端点路径（协议约定 §3.4，mTLS 鉴权）
pub const RENEW_PATH: &str = "/agent/v1/renew";

/// h3 请求流类型（h3 0.0.x API 变动以本别名隔离）
type AgentStream = h3::server::RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>;

/// 接收服务上下文：连接池与配置快照。
#[derive(Clone)]
pub struct ReportContext {
    /// 应用数据库连接池
    pub pool: sqlx::PgPool,
    /// 上报间隔下限（秒，响应控制面下发值不低于该值）
    pub report_interval_secs: u64,
    /// 单条上报 payload 上限（字节，超出 413）
    pub max_report_bytes: usize,
    /// 服务端版本（响应 latest_version 通告）
    pub server_version: String,
}

/// 启动入口（main.rs 调用）：`agent.enabled` 为真时异步完成证书准备并拉起
/// 监听；证书缺失/绑定失败仅记错误日志，不影响主服务进程。
pub fn spawn_report_server_task(
    pool: sqlx::PgPool,
    agent: AgentConfig,
    server_version: &'static str,
) {
    if !agent.enabled {
        log_debug!("log.agent.report_disabled");
        return;
    }
    log_info!("log.agent.report_enabling", bind = agent.bind_addr);
    tokio::spawn(async move {
        let ctx = ReportContext {
            pool,
            report_interval_secs: agent.report_interval_secs,
            max_report_bytes: agent.max_report_bytes,
            server_version: server_version.to_string(),
        };
        // 证书物料缺失时拒绝启动监听（mTLS 无法建立），不 panic
        let material = match crate::cert::ensure_agent_certs().await {
            Ok(m) => m,
            Err(e) => {
                log_error!("log.agent.cert_prepare_failed", error = e);
                return;
            }
        };
        if let Err(e) = run(
            ctx,
            agent.bind_addr.clone(),
            material.server_cert_path,
            material.server_key_path,
        )
        .await
        {
            log_error!("log.agent.report_server_failed", error = e);
        }
    });
}

/// 读取 PEM 证书文件（可含多张），解析失败/无有效内容时报错。
fn load_certs(path: &str, label: &str) -> Result<Vec<CertificateDer<'static>>, String> {
    let data = std::fs::read(path).map_err(|e| format!("打开{label}失败 {path}: {e}"))?;
    let certs: Vec<CertificateDer<'static>> = pem::parse_many(&data)
        .map_err(|e| format!("解析{label} PEM 失败 {path}: {e}"))?
        .into_iter()
        .filter(|p| p.tag() == "CERTIFICATE")
        .map(|p| CertificateDer::from(p.contents().to_vec()))
        .collect();
    if certs.is_empty() {
        return Err(format!("{label}无有效证书: {path}"));
    }
    Ok(certs)
}

/// 读取 PEM 私钥（PKCS#8，rcgen 产出口径），缺证书段时报错。
fn load_key(path: &str, label: &str) -> Result<PrivateKeyDer<'static>, String> {
    let data = std::fs::read(path).map_err(|e| format!("打开{label}失败 {path}: {e}"))?;
    pem::parse_many(&data)
        .map_err(|e| format!("解析{label} PEM 失败 {path}: {e}"))?
        .into_iter()
        .find(|p| p.tag() == "PRIVATE KEY")
        .map(|p| PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(p.contents().to_vec())))
        .ok_or_else(|| format!("{label}无有效私钥: {path}"))
}

/// 构建强制 mTLS 的 QUIC Endpoint：仅信任站点 CA 签发的客户端证书。
fn build_endpoint(
    bind_addr: &str,
    server_cert_path: &str,
    server_key_path: &str,
) -> Result<quinn::Endpoint, String> {
    let mut roots = rustls::RootCertStore::empty();
    for cert in load_certs(crate::SITE_CA_PATH, "站点 CA 证书")? {
        roots
            .add(cert)
            .map_err(|e| format!("站点 CA 证书入库失败: {e}"))?;
    }
    let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots))
        .build()
        .map_err(|e| format!("构建客户端证书校验器失败: {e}"))?;
    let mut tls = rustls::ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(
            load_certs(server_cert_path, "Agent 服务端证书")?,
            load_key(server_key_path, "Agent 服务端私钥")?,
        )
        .map_err(|e| format!("加载 Agent 服务端证书/私钥失败: {e}"))?;
    // HTTP/3 要求 ALPN 协议名为 "h3"
    tls.alpn_protocols = vec![b"h3".to_vec()];
    let server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(tls)
            .map_err(|e| format!("QUIC TLS 配置转换失败: {e}"))?,
    ));
    let addr: std::net::SocketAddr = bind_addr
        .parse()
        .map_err(|e| format!("监听地址非法 {bind_addr}: {e}"))?;
    quinn::Endpoint::server(server_config, addr).map_err(|e| format!("QUIC 绑定 {addr} 失败: {e}"))
}

/// 接收主循环：阻塞当前任务直至进程退出（关闭随进程整体终止）。
pub async fn run(
    ctx: ReportContext,
    bind_addr: String,
    server_cert_path: String,
    server_key_path: String,
) -> Result<(), String> {
    let endpoint = build_endpoint(&bind_addr, &server_cert_path, &server_key_path)?;
    let local = endpoint
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_default();
    log_info!("log.agent.report_listening", bind = local);
    while let Some(incoming) = endpoint.accept().await {
        let ctx = ctx.clone();
        tokio::spawn(handle_connection(ctx, incoming));
    }
    Ok(())
}

/// 单连接处理：等待握手完成（此处已完成客户端证书校验），循环接受 h3 请求。
async fn handle_connection(ctx: ReportContext, incoming: quinn::Incoming) {
    let Ok(connection) = incoming.await else {
        return;
    };
    // IPv4-mapped 对端地址归一为 IPv4 点分形式后入库
    let peer_ip = normalize_ipv4_address(&connection.remote_address().ip().to_string());
    log_debug!("log.agent.quic_connected", peer = peer_ip);
    // 对端客户端证书 leaf（mTLS 握手已验链；续期端点绑定请求体证书用）
    let peer_leaf = peer_leaf_cert(&connection);

    let Ok(mut h3_conn) = h3::server::Connection::new(h3_quinn::Connection::new(connection)).await
    else {
        log_debug!("log.agent.h3_init_failed", peer = peer_ip);
        return;
    };
    // h3 0.0.8：accept 产出 RequestResolver，再解析出请求与流
    loop {
        match h3_conn.accept().await {
            Ok(Some(resolver)) => match resolver.resolve_request().await {
                Ok((request, stream)) => {
                    if let Err(e) =
                        handle_request(&ctx, request, stream, &peer_ip, peer_leaf.as_deref()).await
                    {
                        log_warn!("log.agent.report_request_failed", peer = peer_ip, error = e);
                    }
                }
                Err(e) => {
                    log_debug!(
                        "log.agent.h3_request_resolve_failed",
                        peer = peer_ip,
                        error = e
                    )
                }
            },
            Ok(None) => break,
            Err(e) => {
                log_debug!("log.agent.h3_accept_failed", peer = peer_ip, error = e);
                break;
            }
        }
    }
}

/// 从 QUIC 连接提取对端客户端证书 leaf 的 DER 字节（rustls crypto 下
/// peer_identity 为 [`CertificateDer`] 链，链路校验已在握手阶段完成）。
fn peer_leaf_cert(connection: &quinn::Connection) -> Option<Vec<u8>> {
    let identity = connection.peer_identity()?;
    let certs = identity.downcast::<Vec<CertificateDer<'static>>>().ok()?;
    certs.first().map(|cert| cert.as_ref().to_vec())
}

/// 单请求处理：仅放行 POST + 上报/续期路径；读满请求体（超限 413）后
/// 按路径分发至 ingest（上报）或 renew（证书续期）。
async fn handle_request(
    ctx: &ReportContext,
    request: Request<()>,
    mut stream: AgentStream,
    peer_ip: &str,
    peer_leaf_der: Option<&[u8]>,
) -> Result<(), String> {
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    if path != REPORT_PATH && path != RENEW_PATH {
        return send_json(
            &mut stream,
            StatusCode::NOT_FOUND,
            serde_json::json!({"error": "未知路径"}),
        )
        .await;
    }
    if method != Method::POST {
        return send_json(
            &mut stream,
            StatusCode::METHOD_NOT_ALLOWED,
            serde_json::json!({"error": "仅接受 POST"}),
        )
        .await;
    }

    // 读请求体（链路为 TLS 1.3 密文，此处为解密后明文），超限即拒绝
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = stream
        .recv_data()
        .await
        .map_err(|e| format!("读取请求体失败: {e}"))?
    {
        body.extend_from_slice(chunk.chunk());
        if body.len() > ctx.max_report_bytes {
            return send_json(
                &mut stream,
                StatusCode::PAYLOAD_TOO_LARGE,
                serde_json::json!({"error": "请求体积超过上限"}),
            )
            .await;
        }
    }

    let (status, payload) = if path == RENEW_PATH {
        let (status, Json(payload)) = renew::handle_renew(ctx, &body, peer_leaf_der).await;
        (status, payload)
    } else {
        let (status, payload) = ingest::ingest(ctx, request.headers(), &body, peer_ip).await;
        (status, payload.0)
    };
    send_json(&mut stream, status, payload).await
}

/// 回送 JSON 响应并结束流。
async fn send_json(
    stream: &mut AgentStream,
    status: StatusCode,
    payload: Value,
) -> Result<(), String> {
    let body = serde_json::to_vec(&payload).map_err(|e| format!("响应序列化失败: {e}"))?;
    let response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(())
        .map_err(|e| format!("响应构建失败: {e}"))?;
    stream
        .send_response(response)
        .await
        .map_err(|e| format!("响应发送失败: {e}"))?;
    stream
        .send_data(Bytes::from(body))
        .await
        .map_err(|e| format!("响应体发送失败: {e}"))?;
    stream
        .finish()
        .await
        .map_err(|e| format!("流收尾失败: {e}"))
}
