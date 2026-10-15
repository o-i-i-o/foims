//! HTTP/3 mTLS 上报 transport（平台无关）。
//!
//! 职责：
//! - 配置：`agent.toml`（server_addr 必填 host:port、token 必填 64 位十六进制、
//!   report_interval_secs 缺省 60）；证书固定放 cert_dir 下的 client.pem /
//!   client.key / ca.pem（由安装脚本放置）；
//! - 链路：HTTP/3 mTLS 请求/应答（每次重建连接，TLS 物料从磁盘实时读取）；
//! - 循环：启动立即上报一次 → interval ±10% 抖动 → 失败指数退避
//!   （interval×2^n 封顶 600s，成功重置），失败缓存最近 10 条、恢复后逐条补报；
//! - 控制面：服务端下发 report_interval（clamp [10,3600]，下发值优先）；
//! - 证书续期：剩余寿命低于阈值时经现有 mTLS 链路请求换发，产物落盘
//!   委托 [`Platform::persist_renewed_material`]（权限模型平台各异）。
//!
//! 平台集成：平台 crate 实现 [`Platform`]（版本/机器标识/采集一帧/续期落盘），
//! 注入 [`Reporter`] 即得完整上报循环。机器标识解析、指标采集与报告组装、
//! 文件权限等平台细节全部由平台 crate 承担，本 crate 不感知平台。

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::{Buf, Bytes};
use foims_protocol::report::{AgentReport, CertRenewRequest, CertRenewResponse, ReportResponse};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use serde::Deserialize;

/// 上报路径（与服务端 `POST /agent/v1/report` 一致）
pub const REPORT_PATH: &str = "/agent/v1/report";
/// 证书续期路径（与服务端 `POST /agent/v1/renew` 一致）
pub const RENEW_PATH: &str = "/agent/v1/renew";
/// 缺省上报间隔（秒）
pub const DEFAULT_INTERVAL_SECS: u64 = 60;
/// 服务端可下发的间隔上下限
pub const MIN_INTERVAL_SECS: u64 = 10;
pub const MAX_INTERVAL_SECS: u64 = 3600;
/// 断网缓存条数上限（满则弃最旧）
const PENDING_CACHE_CAP: usize = 10;
/// 退避封顶秒数
const MAX_BACKOFF_SECS: u64 = 600;
/// 证书剩余寿命低于该天数触发自动续期（服务端 <90 天才签发，agent 端阈值取更低值）
const CERT_RENEW_THRESHOLD_DAYS: i64 = 30;
/// 握手与单请求阶段超时
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// 服务端响应体上限（1 MiB）：控制面响应远小于该值，超限视为链路异常
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// 上报链路错误
#[derive(Debug, thiserror::Error)]
pub enum ReporterError {
    #[error("配置错误: {0}")]
    Config(String),
    #[error("采集失败: {0}")]
    Collect(String),
    #[error("上报失败: {0}")]
    Report(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    /// 任意上报链路中间层错误（quinn/h3/http 等）的统一出口
    #[error("上报链路错误: {0}")]
    Transport(Box<dyn std::error::Error + Send + Sync>),
    #[error("响应解析失败: {0}")]
    Decode(#[from] serde_json::Error),
}

/// 把中间层错误装箱为 [`ReporterError::Transport`]（配合 map_err 使用）
fn transport<E>(error: E) -> ReporterError
where
    E: std::error::Error + Send + Sync + 'static,
{
    ReporterError::Transport(Box::new(error))
}

/// agent.toml 的最小字段集（未知字段忽略，便于服务端后续扩展）
#[derive(Debug, Deserialize)]
struct ConfigFile {
    server_addr: String,
    token: String,
    report_interval_secs: Option<u64>,
}

/// 上报配置（agent.toml + CLI/环境变量覆盖后的最终形态）
#[derive(Debug, Clone)]
pub struct ReporterConfig {
    /// 服务端地址 "host:port"（域名或 IP，IPv6 主机需方括号）
    pub server_addr: String,
    /// 64 位十六进制上报 token
    pub token: String,
    /// 上报间隔（秒），clamp 到 [10, 3600]
    pub interval_secs: u64,
    /// 证书目录（client.pem / client.key / ca.pem）
    pub cert_dir: PathBuf,
}

impl ReporterConfig {
    /// 从 agent.toml 文件加载
    pub fn load(config_path: &Path, cert_dir: PathBuf) -> Result<Self, ReporterError> {
        let text = std::fs::read_to_string(config_path).map_err(|error| {
            ReporterError::Config(format!("读取 {}: {error}", config_path.display()))
        })?;
        Self::from_toml_str(&text, cert_dir)
    }

    /// 从 TOML 文本解析
    pub fn from_toml_str(text: &str, cert_dir: PathBuf) -> Result<Self, ReporterError> {
        let parsed: ConfigFile = toml::from_str(text)
            .map_err(|error| ReporterError::Config(format!("agent.toml 解析失败: {error}")))?;
        Self::build(
            parsed.server_addr,
            parsed.token,
            parsed.report_interval_secs,
            cert_dir,
        )
    }

    /// 直接构造（--once / 冒烟客户端用），字段语义与 from_toml_str 一致
    pub fn build(
        server_addr: String,
        token: String,
        interval_secs: Option<u64>,
        cert_dir: PathBuf,
    ) -> Result<Self, ReporterError> {
        let addr = server_addr.trim().to_string();
        if addr.is_empty() {
            return Err(ReporterError::Config("server_addr 不能为空".to_string()));
        }
        parse_host_port(&addr)?;
        let token = token.trim().to_string();
        if !is_valid_token(&token) {
            return Err(ReporterError::Config(
                "token 必须为 64 位十六进制字符串".to_string(),
            ));
        }
        let interval_secs = interval_secs
            .unwrap_or(DEFAULT_INTERVAL_SECS)
            .clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS);
        Ok(Self {
            server_addr: addr,
            token,
            interval_secs,
            cert_dir,
        })
    }
}

/// 校验 "host:port" 形式并返回主机部分（IPv6 主机必须带方括号，去括号返回）
fn parse_host_port(server_addr: &str) -> Result<&str, ReporterError> {
    let Some((host_part, port_text)) = server_addr.rsplit_once(':') else {
        return Err(ReporterError::Config(format!(
            "server_addr 缺少端口: {server_addr}"
        )));
    };
    let port: u16 = port_text
        .parse()
        .map_err(|_| ReporterError::Config(format!("server_addr 端口非法: {server_addr}")))?;
    if port == 0 {
        return Err(ReporterError::Config(
            "server_addr 端口不能为 0".to_string(),
        ));
    }
    // 裸 IPv6 含多个 ':'，rsplit_once 切分点存在歧义（如 ::1:9100），
    // 无法可靠区分主机与端口，必须使用 [..]:port 方括号形式
    let host = if host_part.starts_with('[') {
        let Some(inner) = host_part
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        else {
            return Err(ReporterError::Config(format!(
                "server_addr IPv6 主机方括号不匹配: {server_addr}"
            )));
        };
        inner
    } else if host_part.contains(':') {
        return Err(ReporterError::Config(format!(
            "server_addr 含裸 IPv6 地址，必须使用方括号形式（如 [::1]:9100）: {server_addr}"
        )));
    } else {
        host_part
    };
    let host = host.trim();
    if host.is_empty() {
        return Err(ReporterError::Config(format!(
            "server_addr 主机部分为空: {server_addr}"
        )));
    }
    Ok(host)
}

/// token 校验：64 位十六进制字符
fn is_valid_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// interval ±10% 抖动（防齐发）；factor ∈ [0.9, 1.1]
fn jittered_interval(secs: u64, factor: f64) -> u64 {
    let scaled = secs as f64 * factor;
    // 下限保护：极小间隔 + 下限抖动不得到 0
    (scaled.round() as u64).max(1)
}

/// 连续失败第 n 次的退避秒数：interval × 2^n 封顶 600
fn backoff_secs(interval_secs: u64, fail_count: u32) -> u64 {
    let mut secs = interval_secs.max(1);
    for _ in 0..fail_count {
        secs = secs.saturating_mul(2);
        if secs >= MAX_BACKOFF_SECS {
            return MAX_BACKOFF_SECS;
        }
    }
    secs.min(MAX_BACKOFF_SECS)
}

/// 失败缓存入队：容量 10，满则弃最旧
fn cache_push(queue: &mut VecDeque<AgentReport>, report: AgentReport) {
    if queue.len() >= PENDING_CACHE_CAP {
        queue.pop_front();
    }
    queue.push_back(report);
}

/// 服务端下发的间隔 clamp 到 [10, 3600]
fn clamp_interval(secs: u64) -> u64 {
    secs.clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS)
}

/// 组装请求 URL（IPv6 字面量地址需方括号）
fn request_uri(addr: SocketAddr, path: &str) -> String {
    let host = match addr {
        SocketAddr::V4(v4) => v4.ip().to_string(),
        SocketAddr::V6(v6) => format!("[{}]", v6.ip()),
    };
    format!("https://{host}:{}{path}", addr.port())
}

/// 平台钩子：平台 crate 注入的具体行为（上报循环的其余部分全部复用 core）。
pub trait Platform: Send + 'static {
    /// agent 版本（--version 展示与上报 user-agent）
    fn version(&self) -> String;
    /// 机器唯一标识（实现方处理回退与告警，不失败）。
    /// Linux：/etc/machine-id → /var/lib/dbus/machine-id → 本地持久化 uuid；
    /// Windows：注册表 MachineGuid。
    fn machine_id(&self) -> String;
    /// 同步采集一帧完整报告（阻塞调用，core 经 `spawn_blocking` 调度，
    /// 实现方需自行维护差分状态；失败时保持内部状态不变，与原语义一致）
    fn collect_frame(&mut self) -> Result<AgentReport, ReporterError>;
    /// 续期产物落盘：证书与私钥先写同目录临时文件、写全后依次原子替换
    /// （权限模型平台各异：unix 0644/0600，Windows 继承 ACL）
    fn persist_renewed_material(
        &self,
        cert_dir: &Path,
        cert_pem: &str,
        key_pem: &str,
    ) -> Result<(), ReporterError>;
}

/// HTTP/3 上报器：持有平台实现、失败缓存与服务端下发的间隔。
///
/// 平台实现以 `Arc<Mutex<P>>` 承载：同步采集（约 200ms+）在
/// `spawn_blocking` 中锁定执行，避免阻塞 tokio worker 线程
/// （h3 心跳/响应处理）。
pub struct Reporter<P: Platform> {
    config: ReporterConfig,
    platform: Arc<Mutex<P>>,
    machine_id: String,
    agent_version: String,
    /// 当前生效间隔（服务端下发值优先，clamp 后覆盖配置值）
    interval_secs: u64,
    /// 连续失败计数（成功清零）
    fail_count: u32,
    /// 断网缓存（带原始 collected_at，恢复后补报）
    pending: VecDeque<AgentReport>,
}

impl<P: Platform> Reporter<P> {
    /// 构造：从平台实现解析版本与 machine_id（解析不失败，内部处理回退）
    pub fn new(config: ReporterConfig, platform: P) -> Self {
        let agent_version = platform.version();
        let machine_id = platform.machine_id();
        let interval_secs = config.interval_secs;
        Self {
            config,
            platform: Arc::new(Mutex::new(platform)),
            machine_id,
            agent_version,
            interval_secs,
            fail_count: 0,
            pending: VecDeque::new(),
        }
    }

    /// 当前机器标识
    pub fn machine_id(&self) -> &str {
        &self.machine_id
    }

    /// 采集一帧 AgentReport（内部含阻塞采集，经 [`tokio::task::spawn_blocking`]
    /// 移入阻塞线程池执行）。
    pub async fn collect_report(&mut self) -> Result<AgentReport, ReporterError> {
        let platform = Arc::clone(&self.platform);
        let report = tokio::task::spawn_blocking(move || {
            // 中毒视为不可恢复异常之外的场景：取回内部数据继续工作
            let mut guard = platform
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.collect_frame()
        })
        .await
        .map_err(|error| ReporterError::Collect(format!("采集线程异常退出: {error}")))??;
        Ok(report)
    }

    /// 单轮「采集 + 上报」（--once 与冒烟客户端用）
    pub async fn report_once(&mut self) -> Result<ReportResponse, ReporterError> {
        let report = self.collect_report().await?;
        let response = self.send_report(&report).await;
        if response.is_ok() {
            // 上报成功说明链路可用，顺带检查证书是否临近过期
            self.maybe_renew_cert().await;
        }
        response
    }

    /// 上报主循环：立即上报一次 → 间隔（含抖动/退避）→ 循环。
    /// 采集/上报失败均计入退避；成功后先补报缓存再继续。
    pub async fn run(&mut self) {
        tracing::info!(
            server = %self.config.server_addr,
            interval = self.interval_secs,
            machine_id = %self.machine_id,
            version = %self.agent_version,
            "Agent 上报循环启动"
        );
        loop {
            match self.collect_report().await {
                Err(error) => {
                    self.fail_count += 1;
                    tracing::warn!(%error, fails = self.fail_count, "采集失败，进入退避");
                }
                Ok(report) => match self.send_report(&report).await {
                    Ok(response) => {
                        self.fail_count = 0;
                        self.apply_interval(response.report_interval);
                        self.log_response(&response);
                        self.replay_pending().await;
                        self.maybe_renew_cert().await;
                    }
                    Err(error) => {
                        self.fail_count += 1;
                        tracing::warn!(%error, fails = self.fail_count, "上报失败，已入缓存");
                        cache_push(&mut self.pending, report);
                    }
                },
            }
            self.sleep_next().await;
        }
    }

    /// 本轮休眠：有连续失败走指数退避，否则按间隔 ±10% 抖动
    async fn sleep_next(&mut self) {
        let secs = if self.fail_count > 0 {
            backoff_secs(self.interval_secs, self.fail_count)
        } else {
            let factor: f64 = rand::random_range(0.9..=1.1);
            jittered_interval(self.interval_secs, factor)
        };
        tracing::debug!(secs, "休眠至下一轮");
        tokio::time::sleep(Duration::from_secs(secs)).await;
    }

    /// 应用服务端下发的上报间隔（下发值优先，clamp [10,3600]）
    fn apply_interval(&mut self, server_interval: u64) {
        let clamped = clamp_interval(server_interval);
        if clamped != self.interval_secs {
            tracing::info!(
                old = self.interval_secs,
                new = clamped,
                "服务端调整上报间隔，下一轮生效"
            );
            self.interval_secs = clamped;
        }
    }

    /// 控制面日志：采集器开关（版本通告不再比较：agent 版本独立自管理）
    fn log_response(&self, response: &ReportResponse) {
        if !response.collectors.is_empty() {
            tracing::debug!(collectors = ?response.collectors, "服务端采集器开关");
        }
    }

    /// 补报断网缓存：逐条间隔 1s、保留原始 collected_at；
    /// 任一条失败则放回队头并累计失败（下一轮进入退避）
    async fn replay_pending(&mut self) {
        while let Some(report) = self.pending.pop_front() {
            tokio::time::sleep(Duration::from_secs(1)).await;
            match self.send_report(&report).await {
                Ok(_) => {
                    tracing::info!(
                        collected_at = %report.collected_at,
                        "缓存补报成功"
                    );
                }
                Err(error) => {
                    self.fail_count += 1;
                    tracing::warn!(%error, "缓存补报失败，放回队列头并进入退避");
                    self.pending.push_front(report);
                    return;
                }
            }
        }
    }

    /// 证书剩余寿命检查：低于阈值时经 HTTP/3 mTLS 请求服务端换发。
    /// 检查/续期失败仅记录告警，不影响上报主流程（下轮成功后继续尝试）。
    async fn maybe_renew_cert(&self) {
        let cert_path = self.config.cert_dir.join("client.pem");
        let Ok(client_pem) = std::fs::read_to_string(&cert_path) else {
            tracing::warn!(path = %cert_path.display(), "读取客户端证书失败，跳过续期检查");
            return;
        };
        let remaining_days = match foims_protocol::x509::cert_remaining(&client_pem) {
            Ok((days, _)) => days,
            Err(error) => {
                tracing::warn!(path = %cert_path.display(), %error, "解析客户端证书失败，跳过续期检查");
                return;
            }
        };
        if remaining_days >= CERT_RENEW_THRESHOLD_DAYS {
            return;
        }
        tracing::info!(remaining_days, "客户端证书临近过期，尝试自动续期");
        match self.try_renew_cert(&client_pem).await {
            Ok(renewed) => tracing::info!(
                not_after = %renewed.not_after,
                "客户端证书续期成功，下一轮上报启用新证书"
            ),
            Err(error) => {
                tracing::warn!(%error, "客户端证书续期失败，下轮继续尝试");
            }
        }
    }

    /// 执行一次续期请求并经平台实现原子替换磁盘证书/私钥。
    /// 请求经旧证书建立的 mTLS 链路发送，服务端校验请求体证书与连接证书
    /// 一致后才签发。
    async fn try_renew_cert(&self, client_pem: &str) -> Result<CertRenewResponse, ReporterError> {
        let body = serde_json::to_vec(&CertRenewRequest {
            client_cert_pem: client_pem.to_string(),
        })
        .map_err(|error| ReporterError::Report(format!("序列化续期请求: {error}")))?;
        let (status, resp_body) = self.h3_exchange(RENEW_PATH, body).await?;
        if status != http::StatusCode::OK {
            return Err(ReporterError::Report(format!(
                "服务端拒绝续期（{status}）: {}",
                String::from_utf8_lossy(&resp_body)
            )));
        }
        let renewed: CertRenewResponse = serde_json::from_slice(&resp_body)?;
        validate_renewed_material(&renewed.cert_pem, &renewed.key_pem)?;
        let platform = Arc::clone(&self.platform);
        let cert_dir = self.config.cert_dir.clone();
        let cert_pem = renewed.cert_pem.clone();
        let key_pem = renewed.key_pem.clone();
        // 落盘为同步阻塞操作，移入阻塞线程池（量小，失败路径简单）
        tokio::task::spawn_blocking(move || {
            let guard = platform
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.persist_renewed_material(&cert_dir, &cert_pem, &key_pem)
        })
        .await
        .map_err(|error| ReporterError::Report(format!("续期落盘线程异常退出: {error}")))??;
        Ok(renewed)
    }

    /// 经 HTTP/3 mTLS 发送一次 POST JSON 请求并读取完整响应（上报与续期共用）。
    /// 每次调用重建连接，TLS 物料从磁盘实时读取：续期替换证书后下一轮自动生效。
    async fn h3_exchange(
        &self,
        path: &str,
        body: Vec<u8>,
    ) -> Result<(http::StatusCode, Vec<u8>), ReporterError> {
        // 地址解析（域名/IPv4/IPv6 均可；IPv6 友好）
        let addr: SocketAddr = tokio::net::lookup_host(self.config.server_addr.as_str())
            .await
            .map_err(|error| {
                ReporterError::Report(format!("解析服务地址 {}: {error}", self.config.server_addr))
            })?
            .next()
            .ok_or_else(|| ReporterError::Report("服务地址无可达地址".to_string()))?;

        // TLS：信任 ca.pem，携带客户端证书，ALPN "h3"（IP 直连走 SAN IP 校验）
        let tls = self.build_tls_config()?;
        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls).map_err(transport)?,
        ));
        let local: SocketAddr = if addr.is_ipv4() {
            "0.0.0.0:0".parse().map_err(transport)?
        } else {
            "[::]:0".parse().map_err(transport)?
        };
        let mut endpoint = quinn::Endpoint::client(local).map_err(transport)?;
        endpoint.set_default_client_config(client_config);

        // 连接：server 名传原始主机部分（pki-types 自动按 IP 字面量/DNS 处理）
        let server_name = parse_host_port(&self.config.server_addr)
            .map_err(|error| ReporterError::Config(format!("server_addr 失效: {error}")))?;
        let connecting = endpoint.connect(addr, server_name).map_err(transport)?;
        let connection = tokio::time::timeout(HANDSHAKE_TIMEOUT, connecting)
            .await
            .map_err(|_| ReporterError::Report(format!("连接 {addr} 超时")))?
            .map_err(transport)?;
        tracing::debug!(remote = %connection.remote_address(), "QUIC 连接已建立");

        let (mut driver, mut conn) = h3::client::new(h3_quinn::Connection::new(connection))
            .await
            .map_err(transport)?;
        tokio::spawn(async move {
            let error = driver.wait_idle().await;
            tracing::debug!(%error, "h3 驱动结束");
        });

        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri(request_uri(addr, path))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", self.config.token))
            .header("user-agent", format!("foims-agent/{}", self.agent_version))
            .body(())
            .map_err(transport)?;
        let mut stream = tokio::time::timeout(REQUEST_TIMEOUT, conn.send_request(request))
            .await
            .map_err(|_| ReporterError::Report("发送请求超时".to_string()))?
            .map_err(transport)?;
        stream
            .send_data(Bytes::from(body))
            .await
            .map_err(transport)?;
        stream.finish().await.map_err(transport)?;

        let response = tokio::time::timeout(REQUEST_TIMEOUT, stream.recv_response())
            .await
            .map_err(|_| ReporterError::Report("等待响应超时".to_string()))?
            .map_err(transport)?;
        let mut resp_body: Vec<u8> = Vec::new();
        loop {
            let chunk = tokio::time::timeout(REQUEST_TIMEOUT, stream.recv_data())
                .await
                .map_err(|_| ReporterError::Report("读取响应超时".to_string()))?
                .map_err(transport)?;
            let Some(chunk) = chunk else { break };
            // 服务端响应仅为控制面 JSON（回执/续期/自更新指令），
            // 异常超大体视为链路异常，防止无界读取耗尽内存
            if resp_body.len() + chunk.remaining() > MAX_RESPONSE_BYTES {
                return Err(ReporterError::Report(format!(
                    "响应体超过 {MAX_RESPONSE_BYTES} 字节上限"
                )));
            }
            resp_body.extend_from_slice(chunk.chunk());
        }
        endpoint.close(0u32.into(), b"done");
        tracing::debug!(bytes = resp_body.len(), "收到服务端响应");

        Ok((response.status(), resp_body))
    }

    /// 经 HTTP/3 mTLS 上报一次并解析控制面响应
    async fn send_report(&self, report: &AgentReport) -> Result<ReportResponse, ReporterError> {
        let body = serde_json::to_vec(report)
            .map_err(|error| ReporterError::Collect(format!("序列化上报体: {error}")))?;
        let (status, resp_body) = self.h3_exchange(REPORT_PATH, body).await?;
        if status != http::StatusCode::OK {
            return Err(ReporterError::Report(format!(
                "服务端返回 {}（401 token 无效 / 403 已吊销 / 413 超限 / 429 过频）",
                status
            )));
        }
        serde_json::from_slice(&resp_body).map_err(ReporterError::from)
    }

    /// 构建客户端 TLS 配置：ca.pem 信任锚 + 客户端证书/私钥 + ALPN h3
    fn build_tls_config(&self) -> Result<rustls::ClientConfig, ReporterError> {
        let ca_path = self.config.cert_dir.join("ca.pem");
        let cert_path = self.config.cert_dir.join("client.pem");
        let key_path = self.config.cert_dir.join("client.key");

        let mut roots = rustls::RootCertStore::empty();
        for cert in CertificateDer::pem_file_iter(&ca_path).map_err(|error| {
            ReporterError::Config(format!("打开 {} 失败: {error}", ca_path.display()))
        })? {
            let cert = cert.map_err(|error| {
                ReporterError::Config(format!("解析 {}: {error}", ca_path.display()))
            })?;
            roots.add(cert).map_err(transport)?;
        }
        let verifier = rustls::client::WebPkiServerVerifier::builder(Arc::new(roots))
            .build()
            .map_err(transport)?;
        let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(&cert_path)
            .map_err(|error| {
                ReporterError::Config(format!("打开 {} 失败: {error}", cert_path.display()))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                ReporterError::Config(format!("解析 {}: {error}", cert_path.display()))
            })?;
        if certs.is_empty() {
            return Err(ReporterError::Config(format!(
                "客户端证书为空: {}",
                cert_path.display()
            )));
        }
        let key = PrivateKeyDer::from_pem_file(&key_path).map_err(|error| {
            ReporterError::Config(format!("解析 {}: {error}", key_path.display()))
        })?;

        let mut tls = rustls::ClientConfig::builder()
            .with_webpki_verifier(verifier)
            .with_client_auth_cert(certs, key)
            .map_err(|error| ReporterError::Config(format!("装载客户端证书/私钥: {error}")))?;
        // HTTP/3 要求 ALPN 协议名 "h3"（与 h3_demo 一致）
        tls.alpn_protocols = vec![b"h3".to_vec()];
        Ok(tls)
    }
}

/// 续期产物有效性校验（落盘前拦截坏物料）：证书可解析、私钥含 PEM 头。
/// 坏物料一旦替换旧证书，agent 将无法再建立 mTLS 链路（续期依赖旧证书），
/// 只能人工修复，故必须在校验通过后才允许平台实现写盘。
fn validate_renewed_material(cert_pem: &str, key_pem: &str) -> Result<(), ReporterError> {
    foims_protocol::x509::cert_remaining(cert_pem).map_err(|error| {
        ReporterError::Report(format!("续期返回的证书 PEM 非法，拒绝落盘: {error}"))
    })?;
    let key_valid = [
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----",
        "-----BEGIN RSA PRIVATE KEY-----",
    ]
    .iter()
    .any(|header| key_pem.contains(header));
    if !key_valid {
        return Err(ReporterError::Report(
            "续期返回的私钥不含 PEM 私钥块，拒绝落盘".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use foims_protocol::report::{
        ReportCpu, ReportDisk, ReportMemory, ReportNet, ReportSensor, ReportSystem,
    };

    /// 最小可用上报体（仅用于缓存容量测试）
    fn sample_report(machine_id: &str) -> AgentReport {
        AgentReport {
            machine_id: machine_id.to_string(),
            hostname: "host".to_string(),
            agent_version: "0.0.0".to_string(),
            collected_at: "2026-10-10T00:00:00Z".to_string(),
            system: ReportSystem {
                os: "Linux".to_string(),
                kernel: "6.8".to_string(),
                arch: "x86_64".to_string(),
                uptime_secs: 1,
            },
            cpu: ReportCpu {
                usage_pct: 0.0,
                cores: 1,
                load1: 0.0,
                load5: 0.0,
                load15: 0.0,
            },
            memory: ReportMemory {
                total: 0,
                used: 0,
                swap_total: 0,
                swap_used: 0,
            },
            disks: Vec::<ReportDisk>::new(),
            nets: Vec::<ReportNet>::new(),
            sensors: Vec::<ReportSensor>::new(),
            processes: 0,
        }
    }

    #[test]
    fn test_parse_cli_style_token_validation() {
        let ok = "a".repeat(64);
        assert!(is_valid_token(&ok));
        let upper = "A".repeat(64);
        assert!(is_valid_token(&upper));
        assert!(!is_valid_token("abc"));
        assert!(!is_valid_token(&"g".repeat(64)));
        assert!(!is_valid_token(&"a".repeat(63)));
    }

    #[test]
    fn test_parse_host_port() {
        assert_eq!(
            parse_host_port("example.com:9100").unwrap_or_default(),
            "example.com"
        );
        assert_eq!(
            parse_host_port("1.2.3.4:9100").unwrap_or_default(),
            "1.2.3.4"
        );
        assert_eq!(
            parse_host_port("[2001:db8::1]:9100").unwrap_or_default(),
            "2001:db8::1"
        );
        assert!(parse_host_port("example.com").is_err());
        assert!(parse_host_port("example.com:0").is_err());
        assert!(parse_host_port("example.com:99999").is_err());
        assert!(parse_host_port(":9100").is_err());
        // 裸 IPv6 与方括号不匹配一律拒绝
        assert!(parse_host_port("::1:9100").is_err());
        assert!(parse_host_port("2001:db8::1:9100").is_err());
        assert!(parse_host_port("[2001:db8::1:9100").is_err());
    }

    #[test]
    fn test_config_from_toml() {
        let toml_text = format!(
            "server_addr = \"10.0.0.1:9100\"\ntoken = \"{}\"\nreport_interval_secs = 30\nextra = \"忽略\"\n",
            "ab".repeat(32)
        );
        let config = ReporterConfig::from_toml_str(&toml_text, PathBuf::from("/tmp/certs"))
            .unwrap_or_else(|e| panic!("解析失败: {e}"));
        assert_eq!(config.server_addr, "10.0.0.1:9100");
        assert_eq!(config.interval_secs, 30);
        assert_eq!(config.cert_dir, PathBuf::from("/tmp/certs"));

        // 缺省间隔
        let default_text = format!("server_addr = \"h:1\"\ntoken = \"{}\"\n", "ab".repeat(32));
        let default_config = ReporterConfig::from_toml_str(&default_text, PathBuf::from("/c"))
            .unwrap_or_else(|e| panic!("解析失败: {e}"));
        assert_eq!(default_config.interval_secs, DEFAULT_INTERVAL_SECS);

        // 非法 token / 缺字段 / 非法间隔 clamp
        let bad_token = format!("server_addr = \"h:1\"\ntoken = \"{}\"\n", "ab".repeat(31));
        assert!(ReporterConfig::from_toml_str(&bad_token, PathBuf::from("/c")).is_err());
        assert!(ReporterConfig::from_toml_str("token = \"x\"\n", PathBuf::from("/c")).is_err());
        let huge_interval = format!(
            "server_addr = \"h:1\"\ntoken = \"{}\"\nreport_interval_secs = 99999\n",
            "ab".repeat(32)
        );
        let clamped = ReporterConfig::from_toml_str(&huge_interval, PathBuf::from("/c"))
            .unwrap_or_else(|e| panic!("解析失败: {e}"));
        assert_eq!(clamped.interval_secs, MAX_INTERVAL_SECS);
    }

    #[test]
    fn test_jittered_interval_bounds() {
        assert_eq!(jittered_interval(60, 1.0), 60);
        assert_eq!(jittered_interval(60, 0.9), 54);
        assert_eq!(jittered_interval(60, 1.1), 66);
        // 下限保护：不得到 0
        assert_eq!(jittered_interval(1, 0.9), 1);
    }

    #[test]
    fn test_backoff_sequence() {
        assert_eq!(backoff_secs(60, 0), 60);
        assert_eq!(backoff_secs(60, 1), 120);
        assert_eq!(backoff_secs(60, 2), 240);
        assert_eq!(backoff_secs(60, 3), 480);
        assert_eq!(backoff_secs(60, 4), 600);
        assert_eq!(backoff_secs(60, 10), 600);
        // 小间隔同样封顶
        assert_eq!(backoff_secs(1, 20), 600);
    }

    #[test]
    fn test_cache_capacity_truncation() {
        let mut queue = VecDeque::new();
        for index in 0..12 {
            cache_push(&mut queue, sample_report(&format!("id-{index}")));
        }
        assert_eq!(queue.len(), PENDING_CACHE_CAP);
        // 最旧的 0/1 已被丢弃，队头为 2
        assert_eq!(
            queue
                .front()
                .unwrap_or_else(|| panic!("队头应为空检查"))
                .machine_id,
            "id-2"
        );
        assert_eq!(
            queue
                .back()
                .unwrap_or_else(|| panic!("队尾应为空检查"))
                .machine_id,
            "id-11"
        );
    }

    #[test]
    fn test_clamp_interval() {
        assert_eq!(clamp_interval(5), MIN_INTERVAL_SECS);
        assert_eq!(clamp_interval(60), 60);
        assert_eq!(clamp_interval(5_000), MAX_INTERVAL_SECS);
    }

    #[test]
    fn test_request_uri() {
        let v4: SocketAddr = "1.2.3.4:9100"
            .parse()
            .unwrap_or_else(|e| panic!("解析失败: {e}"));
        assert_eq!(
            request_uri(v4, REPORT_PATH),
            "https://1.2.3.4:9100/agent/v1/report"
        );
        let v6: SocketAddr = "[2001:db8::1]:9100"
            .parse()
            .unwrap_or_else(|e| panic!("解析失败: {e}"));
        assert_eq!(
            request_uri(v6, RENEW_PATH),
            "https://[2001:db8::1]:9100/agent/v1/renew"
        );
    }

    #[test]
    fn test_validate_renewed_material() {
        // 证书非法 / 私钥缺 PEM 头均拒绝
        assert!(validate_renewed_material("not-a-pem", "-----BEGIN PRIVATE KEY-----").is_err());
        assert!(validate_renewed_material("not-a-pem", "key").is_err());
        // 合法私钥头 + 非法证书：证书校验先拦截
        assert!(
            validate_renewed_material(
                "-----BEGIN CERTIFICATE-----\nbroken\n",
                "-----BEGIN PRIVATE KEY-----"
            )
            .is_err()
        );
    }
}
