//! HTTP/3 mTLS 指标上报（docs/agent-design.md §3.3/§4 阶段 3）。
//!
//! 职责：
//! - 机器标识：`/etc/machine-id` → `/var/lib/dbus/machine-id` → 本地持久化文件
//!   （uuid v4，目录 0755 / 文件 0644，无写盘权限时回退内存随机值并告警）；
//! - 配置：`agent.toml`（server_addr 必填 host:port、token 必填 64 位十六进制、
//!   report_interval_secs 缺省 60）；证书固定放 cert_dir 下的 client.pem /
//!   client.key / ca.pem（由安装脚本放置）；
//! - 采集：复用现有采集器输出（scrape），diskstats/netdev 累计计数器差分为
//!   IOPS 与 bps，状态保存在 [`Reporter`] 内；
//! - 循环：启动立即上报一次 → interval ±10% 抖动 → 失败指数退避
//!   （interval×2^n 封顶 600s，成功重置），失败缓存最近 10 条、恢复后逐条补报；
//! - 控制面：服务端下发 report_interval（clamp [10,3600]，下发值优先）。

use std::collections::{BTreeMap, VecDeque};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::{Buf, Bytes};
use foims_common::report::{
    AgentReport, CertRenewRequest, CertRenewResponse, ReportCpu, ReportDisk, ReportMemory,
    ReportNet, ReportResponse, ReportSensor, ReportSystem,
};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use serde::Deserialize;

use crate::VERSION;
use crate::metric::{MetricFamily, Sample};

/// 上报路径（与服务端 `POST /agent/v1/report` 一致）
pub const REPORT_PATH: &str = "/agent/v1/report";
/// 证书续期路径（与服务端 `POST /agent/v1/renew` 一致）
pub const RENEW_PATH: &str = "/agent/v1/renew";
/// 缺省配置文件路径
pub const DEFAULT_CONFIG_PATH: &str = "/etc/foims-agent/agent.toml";
/// 缺省证书目录（client.pem / client.key / ca.pem 由安装脚本放置）
pub const DEFAULT_CERT_DIR: &str = "/etc/foims-agent";
/// 证书目录环境变量覆盖（测试/联调用）
pub const ENV_CERT_DIR: &str = "FOIMS_AGENT_CERT_DIR";
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
/// CPU 使用率两次采样窗口
const CPU_SAMPLE_WINDOW: Duration = Duration::from_millis(200);
/// 握手与单请求阶段超时
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

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

/// 校验 "host:port" 形式并返回主机部分（IPv6 主机需方括号，去括号返回）
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
    // 裸 IPv6 无端口会在这里被误切，要求 IPv6 主机必须带方括号
    let host = host_part
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host_part)
        .trim();
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

/// machine-id 三个候选路径（抽出来便于测试注入临时目录）
#[derive(Debug, Clone)]
pub struct MachineIdPaths {
    /// /etc/machine-id（优先）
    pub etc_machine_id: PathBuf,
    /// /var/lib/dbus/machine-id（回退）
    pub dbus_machine_id: PathBuf,
    /// agent 本地持久化文件（前两者都缺失时生成 uuid v4 写入）
    pub local_file: PathBuf,
}

impl MachineIdPaths {
    /// 生产路径
    pub fn production() -> Self {
        Self {
            etc_machine_id: PathBuf::from("/etc/machine-id"),
            dbus_machine_id: PathBuf::from("/var/lib/dbus/machine-id"),
            local_file: PathBuf::from("/var/lib/foims-agent/machine-id"),
        }
    }
}

/// 解析机器标识：trim + 小写；三处都拿不到时生成 uuid v4 并尝试持久化，
/// 写盘失败（如无权限）则回退内存随机值并告警（每次进程启动会变化）。
pub fn resolve_machine_id(paths: &MachineIdPaths) -> String {
    let candidates = [
        &paths.etc_machine_id,
        &paths.dbus_machine_id,
        &paths.local_file,
    ];
    for candidate in candidates {
        let Ok(text) = std::fs::read_to_string(candidate) else {
            continue;
        };
        let cleaned = text.trim().to_lowercase();
        if !cleaned.is_empty() {
            return cleaned;
        }
    }
    // 生成 uuid v4（simple 格式 32 位小写十六进制，与 machine-id 惯例一致）
    let generated = uuid::Uuid::new_v4().simple().to_string();
    match persist_machine_id(&paths.local_file, &generated) {
        Ok(()) => tracing::info!(
            path = %paths.local_file.display(),
            "已生成并持久化 machine-id"
        ),
        Err(error) => tracing::warn!(
            %error,
            path = %paths.local_file.display(),
            "machine-id 持久化失败，本进程使用内存随机值（重启会变化）"
        ),
    }
    generated
}

/// 持久化 machine-id：父目录 0755、文件 0644
fn persist_machine_id(path: &Path, value: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        let mut perms = std::fs::metadata(parent)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(parent, perms)?;
    }
    std::fs::write(path, value)?;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o644);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

/// /proc/stat 聚合行采样（原始 USER_HZ 单位，比值与单位无关）
#[derive(Debug, Clone, Copy, PartialEq)]
struct CpuSample {
    total: f64,
    idle: f64,
}

/// 解析 /proc/stat 聚合 "cpu" 行：total 为全部列之和，idle 取 idle + iowait
fn parse_cpu_aggregate(text: &str) -> Option<CpuSample> {
    let line = text.lines().find(|line| line.starts_with("cpu "))?;
    let mut values = Vec::new();
    for field in line.split_whitespace().skip(1) {
        let value: f64 = field.parse().ok()?;
        values.push(value);
    }
    if values.len() < 4 {
        return None;
    }
    let total: f64 = values.iter().sum();
    let idle = values[3] + values.get(4).copied().unwrap_or(0.0);
    Some(CpuSample { total, idle })
}

/// 读取当前 CPU 采样
fn read_cpu_sample() -> Result<CpuSample, ReporterError> {
    let text = std::fs::read_to_string("/proc/stat")
        .map_err(|error| ReporterError::Collect(format!("读取 /proc/stat: {error}")))?;
    parse_cpu_aggregate(&text)
        .ok_or_else(|| ReporterError::Collect("/proc/stat 无聚合 cpu 行".to_string()))
}

/// 两次采样差分得使用率（0-100）；时间倒流或无推进时按 0 处理
fn cpu_usage_pct(prev: &CpuSample, curr: &CpuSample) -> f64 {
    let dt = curr.total - prev.total;
    if dt <= 0.0 {
        return 0.0;
    }
    let idle = curr.idle - prev.idle;
    ((1.0 - idle / dt) * 100.0).clamp(0.0, 100.0)
}

/// diskstats 累计计数器（I/O 差分基准）
#[derive(Debug, Clone, Copy, Default)]
struct DiskCounters {
    reads: f64,
    writes: f64,
    io_time_secs: f64,
}

/// netdev 累计计数器（bps 差分基准）
#[derive(Debug, Clone, Copy, Default)]
struct NetCounters {
    rx_bytes: f64,
    tx_bytes: f64,
}

/// 上轮差分基准：diskstats/netdev 计数器 + 采样时刻
#[derive(Debug, Clone, Default)]
struct DiffBasis {
    at: Option<Instant>,
    disks: BTreeMap<String, DiskCounters>,
    nets: BTreeMap<String, NetCounters>,
}

/// diskstats 差分 → (read_iops, write_iops, util_pct)；
/// 计数回绕/重置（curr < prev）或间隔非正时按 0 处理
fn disk_rates(prev: &DiskCounters, curr: &DiskCounters, elapsed_secs: f64) -> (f64, f64, f64) {
    if elapsed_secs <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let read_delta = curr.reads - prev.reads;
    let write_delta = curr.writes - prev.writes;
    let io_delta = curr.io_time_secs - prev.io_time_secs;
    if read_delta < 0.0 || write_delta < 0.0 || io_delta < 0.0 {
        return (0.0, 0.0, 0.0);
    }
    (
        read_delta / elapsed_secs,
        write_delta / elapsed_secs,
        (io_delta / elapsed_secs * 100.0).clamp(0.0, 100.0),
    )
}

/// netdev 差分 → (rx_bps, tx_bps)；回绕/重置按 0 处理
fn net_rates(prev: &NetCounters, curr: &NetCounters, elapsed_secs: f64) -> (f64, f64) {
    if elapsed_secs <= 0.0 {
        return (0.0, 0.0);
    }
    let rx = curr.rx_bytes - prev.rx_bytes;
    let tx = curr.tx_bytes - prev.tx_bytes;
    if rx < 0.0 || tx < 0.0 {
        return (0.0, 0.0);
    }
    (rx / elapsed_secs, tx / elapsed_secs)
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

/// 指标族查找（按名）
fn find_family<'a>(families: &'a [MetricFamily], name: &str) -> Option<&'a MetricFamily> {
    families.iter().find(|family| family.name == name)
}

/// 单样本取值（缺失按 0，采集失败不应让上报中断）
fn sample_value(families: &[MetricFamily], name: &str) -> f64 {
    find_family(families, name)
        .and_then(|family| family.samples.first())
        .map(|sample| sample.value)
        .unwrap_or(0.0)
}

/// 取样本指定标签值
fn label_value<'a>(sample: &'a Sample, key: &str) -> Option<&'a str> {
    sample
        .labels
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, value)| value.as_str())
}

/// 按多标签精确匹配样本
fn find_labeled<'a>(
    families: &'a [MetricFamily],
    name: &str,
    want: &[(&str, &str)],
) -> Option<&'a Sample> {
    find_family(families, name)?.samples.iter().find(|sample| {
        want.iter()
            .all(|(key, value)| label_value(sample, key) == Some(*value))
    })
}

/// 设备标签 → 磁盘设备名（"/dev/sda1" → "sda1"；无 /dev 前缀取末段）
fn disk_device_name(device_label: &str) -> &str {
    device_label.rsplit('/').next().unwrap_or(device_label)
}

/// /proc/uptime 首字段（秒）
fn read_uptime_secs() -> u64 {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|text| {
            text.split_whitespace()
                .next()
                .and_then(|f| f.parse::<f64>().ok())
        })
        .map(|secs| secs.max(0.0) as u64)
        .unwrap_or(0)
}

/// /proc 下数字目录计数（进程总数）
fn count_processes() -> u32 {
    let count = std::fs::read_dir("/proc")
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry.file_name().to_str().is_some_and(|name| {
                        !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit())
                    })
                })
                .count()
        })
        .unwrap_or(0);
    count as u32
}

/// 主机名：uname nodename → /proc/sys/kernel/hostname → "unknown"
fn read_hostname() -> String {
    let raw = rustix::system::uname();
    let nodename = raw.nodename().to_string_lossy().trim().to_string();
    if !nodename.is_empty() {
        return nodename;
    }
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|text| text.trim().to_string())
        .ok()
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// RFC3339 UTC 当前时刻（如 "2026-10-10T04:00:00Z"）
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// 同步采集一帧报告（含 200ms CPU 差分窗口与 /proc 阻塞读取），
/// 设计为不依赖 &self 的纯函数，便于整体移入 [`tokio::task::spawn_blocking`]；
/// 返回报告与新差分基准，调用方负责落回 Reporter 状态。
fn collect_sync(
    machine_id: String,
    prev: Option<DiffBasis>,
) -> Result<(AgentReport, DiffBasis), ReporterError> {
    let cpu_start = read_cpu_sample()?;
    let sample_at = Instant::now();
    let collectors = crate::default_collectors(None);
    let families = crate::scrape(&collectors);
    std::thread::sleep(CPU_SAMPLE_WINDOW);
    let cpu_end = read_cpu_sample()?;

    let usage_pct = cpu_usage_pct(&cpu_start, &cpu_end);
    Ok(build_report(
        &machine_id,
        prev,
        &families,
        sample_at,
        usage_pct,
    ))
}

/// 由 scrape 输出组装 AgentReport，并产出新差分基准（diskstats/netdev）
fn build_report(
    machine_id: &str,
    prev: Option<DiffBasis>,
    families: &[MetricFamily],
    sample_at: Instant,
    usage_pct: f64,
) -> (AgentReport, DiffBasis) {
    let raw = rustix::system::uname();
    let now = now_rfc3339();

    // 差分：与上轮计数器对比（首轮无基准，速率记 0）
    let prev = prev.unwrap_or_default();
    let elapsed = prev
        .at
        .map(|at| sample_at.duration_since(at).as_secs_f64())
        .unwrap_or(0.0);

    // ---- CPU / 负载 / 内存 ----
    let cores = count_cores(families);
    let memory = ReportMemory {
        total: sample_value(families, "node_memory_MemTotal_bytes").max(0.0) as u64,
        used: used_memory_bytes(families),
        swap_total: sample_value(families, "node_memory_SwapTotal_bytes").max(0.0) as u64,
        swap_used: (sample_value(families, "node_memory_SwapTotal_bytes")
            - sample_value(families, "node_memory_SwapFree_bytes"))
        .max(0.0) as u64,
    };

    // ---- 磁盘：filesystem 容量 + diskstats 差分 ----
    let diskstats = collect_disk_counters(families);
    // 首轮 elapsed 为 0，差分自动归 0，无需特判空基准
    let disks = build_disks(families, &diskstats, Some(&prev.disks), elapsed);

    // ---- 网络：netdev 差分 ----
    let netdev = collect_net_counters(families);
    let nets = build_nets(families, &netdev, Some(&prev.nets), elapsed);

    // ---- 更新差分基准 ----
    let basis = DiffBasis {
        at: Some(sample_at),
        disks: diskstats,
        nets: netdev,
    };

    let report = AgentReport {
        machine_id: machine_id.to_string(),
        hostname: read_hostname(),
        agent_version: VERSION.to_string(),
        collected_at: now,
        system: ReportSystem {
            os: os_description(families, &raw),
            kernel: raw.release().to_string_lossy().into_owned(),
            arch: raw.machine().to_string_lossy().into_owned(),
            uptime_secs: read_uptime_secs(),
        },
        cpu: ReportCpu {
            usage_pct,
            cores,
            load1: sample_value(families, "node_load1"),
            load5: sample_value(families, "node_load5"),
            load15: sample_value(families, "node_load15"),
        },
        memory,
        disks,
        nets,
        sensors: collect_sensors(families),
        processes: count_processes(),
    };
    (report, basis)
}

/// HTTP/3 上报器：持有差分状态、失败缓存与服务端下发的间隔
pub struct Reporter {
    config: ReporterConfig,
    machine_id: String,
    /// 当前生效间隔（服务端下发值优先，clamp 后覆盖配置值）
    interval_secs: u64,
    /// 连续失败计数（成功清零）
    fail_count: u32,
    /// 断网缓存（带原始 collected_at，恢复后补报）
    pending: VecDeque<AgentReport>,
    /// 上轮 diskstats/netdev 计数器基准
    prev: Option<DiffBasis>,
}

impl Reporter {
    /// 构造：解析 machine_id（内部处理回退与告警，不失败）
    pub fn new(config: ReporterConfig) -> Self {
        let machine_id = resolve_machine_id(&MachineIdPaths::production());
        let interval_secs = config.interval_secs;
        Self {
            config,
            machine_id,
            interval_secs,
            fail_count: 0,
            pending: VecDeque::new(),
            prev: None,
        }
    }

    /// 当前机器标识
    pub fn machine_id(&self) -> &str {
        &self.machine_id
    }

    /// 采集一帧 AgentReport（内部含 200ms CPU 差分窗口与 scrape 阻塞），
    /// 并更新 diskstats/netdev 差分基准。
    /// 同步采集部分经 [`tokio::task::spawn_blocking`] 移入阻塞线程池执行，
    /// 避免约 200ms+ 的阻塞拖慢 tokio worker 线程（h3 心跳/响应处理）。
    pub async fn collect_report(&mut self) -> Result<AgentReport, ReporterError> {
        // 差分基准与机器标识克隆后移交阻塞线程；采集失败时保持原状态不变
        // （与既有语义一致：read_cpu_sample 失败时不更新差分基准）
        let prev = self.prev.clone();
        let machine_id = self.machine_id.clone();
        let (report, basis) = tokio::task::spawn_blocking(move || collect_sync(machine_id, prev))
            .await
            .map_err(|error| ReporterError::Collect(format!("采集线程异常退出: {error}")))??;
        self.prev = Some(basis);
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
        let remaining_days = match foims_common::x509::cert_remaining(&client_pem) {
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

    /// 执行一次续期请求并用服务端签发的新物料原子替换磁盘证书/私钥
    /// （证书 0644 / 私钥 0600）。请求经旧证书建立的 mTLS 链路发送，
    /// 服务端校验请求体证书与连接证书一致后才签发。
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
        write_renewed_material(&self.config.cert_dir, &renewed.cert_pem, &renewed.key_pem)?;
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
            .header(
                "user-agent",
                concat!("foims-agent/", env!("CARGO_PKG_VERSION")),
            )
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

/// 内存已用：total - available（旧内核无 MemAvailable 回退 MemFree）
fn used_memory_bytes(families: &[MetricFamily]) -> u64 {
    let total = sample_value(families, "node_memory_MemTotal_bytes");
    let available = find_family(families, "node_memory_MemAvailable_bytes")
        .and_then(|family| family.samples.first())
        .map(|sample| sample.value)
        .unwrap_or_else(|| sample_value(families, "node_memory_MemFree_bytes"));
    (total - available).max(0.0) as u64
}

/// 逻辑核心数：node_cpu_seconds_total 中 mode=user 的样本数（每核一条）
fn count_cores(families: &[MetricFamily]) -> u32 {
    find_family(families, "node_cpu_seconds_total")
        .map(|family| {
            family
                .samples
                .iter()
                .filter(|sample| label_value(sample, "mode") == Some("user"))
                .count() as u32
        })
        .unwrap_or(0)
}

/// os 描述：os-release PRETTY_NAME，缺失回退 uname sysname
fn os_description(families: &[MetricFamily], raw: &rustix::system::Uname) -> String {
    find_family(families, "node_os_info")
        .and_then(|family| family.samples.first())
        .and_then(|sample| label_value(sample, "pretty_name"))
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| raw.sysname().to_string_lossy().into_owned())
}

/// 从 diskstats 三族指标收集各设备计数器
fn collect_disk_counters(families: &[MetricFamily]) -> BTreeMap<String, DiskCounters> {
    let mut counters: BTreeMap<String, DiskCounters> = BTreeMap::new();
    let mut insert =
        |families: &[MetricFamily], name: &str, pick: fn(&mut DiskCounters) -> &mut f64| {
            if let Some(family) = find_family(families, name) {
                for sample in &family.samples {
                    if let Some(device) = label_value(sample, "device") {
                        let entry = counters.entry(device.to_string()).or_default();
                        *pick(entry) = sample.value;
                    }
                }
            }
        };
    insert(families, "node_disk_reads_completed_total", |c| {
        &mut c.reads
    });
    insert(families, "node_disk_writes_completed_total", |c| {
        &mut c.writes
    });
    insert(families, "node_disk_io_time_seconds_total", |c| {
        &mut c.io_time_secs
    });
    counters
}

/// filesystem 容量 + diskstats 差分 → ReportDisk 列表
/// （设备名映射到挂载设备；无法匹配的挂载点 IOPS/util 记 0）
fn build_disks(
    families: &[MetricFamily],
    diskstats: &BTreeMap<String, DiskCounters>,
    prev: Option<&BTreeMap<String, DiskCounters>>,
    elapsed_secs: f64,
) -> Vec<ReportDisk> {
    let Some(size_family) = find_family(families, "node_filesystem_size_bytes") else {
        return Vec::new();
    };
    let mut disks = Vec::new();
    for size_sample in &size_family.samples {
        let Some(device_label) = label_value(size_sample, "device") else {
            continue;
        };
        let Some(mount) = label_value(size_sample, "mountpoint") else {
            continue;
        };
        let total = size_sample.value.max(0.0) as u64;
        let free = find_labeled(
            families,
            "node_filesystem_free_bytes",
            &[("device", device_label), ("mountpoint", mount)],
        )
        .map(|sample| sample.value)
        .unwrap_or(0.0);
        let used = (size_sample.value - free).max(0.0) as u64;

        let device = disk_device_name(device_label).to_string();
        let (read_iops, write_iops, util_pct) = match (
            prev.and_then(|basis| basis.get(&device)),
            diskstats.get(&device),
        ) {
            (Some(prev_counters), Some(curr_counters)) => {
                disk_rates(prev_counters, curr_counters, elapsed_secs)
            }
            _ => (0.0, 0.0, 0.0),
        };
        disks.push(ReportDisk {
            device,
            mount: mount.to_string(),
            total,
            used,
            read_iops,
            write_iops,
            util_pct,
        });
    }
    disks
}

/// 从 netdev 指标收集各接口字节计数器
fn collect_net_counters(families: &[MetricFamily]) -> BTreeMap<String, NetCounters> {
    let mut counters: BTreeMap<String, NetCounters> = BTreeMap::new();
    let mut insert =
        |families: &[MetricFamily], name: &str, pick: fn(&mut NetCounters) -> &mut f64| {
            if let Some(family) = find_family(families, name) {
                for sample in &family.samples {
                    if let Some(device) = label_value(sample, "device") {
                        let entry = counters.entry(device.to_string()).or_default();
                        *pick(entry) = sample.value;
                    }
                }
            }
        };
    insert(families, "node_network_receive_bytes_total", |c| {
        &mut c.rx_bytes
    });
    insert(families, "node_network_transmit_bytes_total", |c| {
        &mut c.tx_bytes
    });
    counters
}

/// netdev 字节差分 → ReportNet 列表（首轮无基准 bps 记 0，errors 取累计值）
fn build_nets(
    families: &[MetricFamily],
    netdev: &BTreeMap<String, NetCounters>,
    prev: Option<&BTreeMap<String, NetCounters>>,
    elapsed_secs: f64,
) -> Vec<ReportNet> {
    let mut nets = Vec::new();
    for (iface, curr_counters) in netdev {
        let (rx_bps, tx_bps) = match prev.and_then(|basis| basis.get(iface)) {
            Some(prev_counters) => net_rates(prev_counters, curr_counters, elapsed_secs),
            None => (0.0, 0.0),
        };
        let errors =
            (receive_errors(families, iface) + transmit_errors(families, iface)).max(0.0) as u64;
        nets.push(ReportNet {
            iface: iface.clone(),
            rx_bps,
            tx_bps,
            errors,
        });
    }
    nets
}

/// 单接口累计接收错误数
fn receive_errors(families: &[MetricFamily], iface: &str) -> f64 {
    find_labeled(
        families,
        "node_network_receive_errs_total",
        &[("device", iface)],
    )
    .map(|sample| sample.value)
    .unwrap_or(0.0)
}

/// 单接口累计发送错误数
fn transmit_errors(families: &[MetricFamily], iface: &str) -> f64 {
    find_labeled(
        families,
        "node_network_transmit_errs_total",
        &[("device", iface)],
    )
    .map(|sample| sample.value)
    .unwrap_or(0.0)
}

/// 传感器列表：hwmon 温度（采集器已折算 ℃）与风扇（RPM）、thermal 热区（℃）；
/// 标签沿用采集器逻辑：hwmon 用 chip + sensor_label（无则 sensor 键），
/// 热区用 type + zone
fn collect_sensors(families: &[MetricFamily]) -> Vec<ReportSensor> {
    let mut sensors = Vec::new();
    let mut push_hwmon = |name: &str, kind: &'static str| {
        let Some(family) = find_family(families, name) else {
            return;
        };
        for sample in &family.samples {
            let Some(chip) = label_value(sample, "chip") else {
                continue;
            };
            let Some(sensor) = label_value(sample, "sensor") else {
                continue;
            };
            let label_text = find_labeled(
                families,
                "node_hwmon_sensor_label",
                &[("chip", chip), ("sensor", sensor)],
            )
            .and_then(|labeled| label_value(labeled, "label"))
            .unwrap_or(sensor);
            sensors.push(ReportSensor {
                label: format!("{chip} {label_text}"),
                kind: kind.to_string(),
                value: sample.value,
            });
        }
    };
    push_hwmon("node_hwmon_temp_celsius", "temp");
    push_hwmon("node_hwmon_fan_input", "fan");

    // thermal 热区温度（node_thermal_zone_temp 值已是摄氏度）
    if let Some(family) = find_family(families, "node_thermal_zone_temp") {
        for sample in &family.samples {
            let ztype = label_value(sample, "type").unwrap_or("thermal");
            let zone = label_value(sample, "zone").unwrap_or("");
            sensors.push(ReportSensor {
                label: format!("{ztype} {zone}"),
                kind: "temp".to_string(),
                value: sample.value,
            });
        }
    }
    sensors
}

/// 组装请求 URL（IPv6 字面量地址需方括号）
fn request_uri(addr: SocketAddr, path: &str) -> String {
    let host = match addr {
        SocketAddr::V4(v4) => v4.ip().to_string(),
        SocketAddr::V6(v6) => format!("[{}]", v6.ip()),
    };
    format!("https://{host}:{}{path}", addr.port())
}

/// 清理续期临时文件（失败仅记录调试日志，不影响主流程）
fn cleanup_temp(tmp: &Path) {
    if let Err(error) = std::fs::remove_file(tmp) {
        tracing::debug!(%error, path = %tmp.display(), "清理续期临时文件失败");
    }
}

/// 在目标文件同目录写临时文件并设定权限（供原子替换使用）
fn write_temp_file(path: &Path, data: &[u8], mode: u32) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let tmp = PathBuf::from(format!("{}.new", path.display()));
    std::fs::write(&tmp, data)?;
    if let Err(error) = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode)) {
        cleanup_temp(&tmp);
        return Err(error);
    }
    Ok(tmp)
}

/// 续期产物落盘：证书与私钥先全部写为同目录 `.new` 临时文件（证书 0644 /
/// 私钥 0600），写全后依次 rename 原子替换。临时文件阶段任一步失败即清理
/// 并报错，旧物料保持不变；rename 阶段失败同样清理未完成项（同目录 rename
/// 在写入成功后几乎不可能失败，跨文件瞬时窗口可忽略）。
fn write_renewed_material(
    cert_dir: &Path,
    cert_pem: &str,
    key_pem: &str,
) -> Result<(), ReporterError> {
    let cert_path = cert_dir.join("client.pem");
    let key_path = cert_dir.join("client.key");
    let cert_tmp = write_temp_file(&cert_path, cert_pem.as_bytes(), 0o644)?;
    let key_tmp = match write_temp_file(&key_path, key_pem.as_bytes(), 0o600) {
        Ok(tmp) => tmp,
        Err(error) => {
            cleanup_temp(&cert_tmp);
            return Err(ReporterError::Io(error));
        }
    };
    if let Err(error) = std::fs::rename(&cert_tmp, &cert_path) {
        cleanup_temp(&cert_tmp);
        cleanup_temp(&key_tmp);
        return Err(ReporterError::Io(error));
    }
    if let Err(error) = std::fs::rename(&key_tmp, &key_path) {
        cleanup_temp(&key_tmp);
        return Err(ReporterError::Io(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use foims_common::report::{ReportCpu, ReportSystem};
    use std::io::Write;

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
            disks: Vec::new(),
            nets: Vec::new(),
            sensors: Vec::new(),
            processes: 0,
        }
    }

    /// 唯一临时目录
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "foims-agent-test-{}-{}-{tag}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("建临时目录失败: {e}"));
        dir
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
    fn test_resolve_machine_id_from_etc() {
        let dir = temp_dir("etc");
        let etc = dir.join("machine-id");
        let mut file = std::fs::File::create(&etc).unwrap_or_else(|e| panic!("建文件失败: {e}"));
        file.write_all(b"ABCDEF0123456789ABCDEF0123456789\n")
            .unwrap_or_else(|e| panic!("写入失败: {e}"));
        let paths = MachineIdPaths {
            etc_machine_id: etc,
            dbus_machine_id: dir.join("missing-dbus"),
            local_file: dir.join("missing-local"),
        };
        assert_eq!(
            resolve_machine_id(&paths),
            "abcdef0123456789abcdef0123456789"
        );
    }

    #[test]
    fn test_resolve_machine_id_dbus_fallback() {
        let dir = temp_dir("dbus");
        let dbus = dir.join("machine-id");
        std::fs::write(&dbus, "  0123456789ABCDEF0123456789ABCDEF \n")
            .unwrap_or_else(|e| panic!("写入失败: {e}"));
        let paths = MachineIdPaths {
            etc_machine_id: dir.join("missing-etc"),
            dbus_machine_id: dbus,
            local_file: dir.join("missing-local"),
        };
        assert_eq!(
            resolve_machine_id(&paths),
            "0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn test_resolve_machine_id_generates_local_file() {
        let dir = temp_dir("gen");
        let paths = MachineIdPaths {
            etc_machine_id: dir.join("missing-etc"),
            dbus_machine_id: dir.join("missing-dbus"),
            local_file: dir.join("state/machine-id"),
        };
        let first = resolve_machine_id(&paths);
        assert_eq!(first.len(), 32);
        assert!(
            first
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );
        // 已持久化且可复读
        let persisted =
            std::fs::read_to_string(&paths.local_file).unwrap_or_else(|e| panic!("读取失败: {e}"));
        assert_eq!(persisted.trim(), first);
        // 再次解析命中本地文件
        assert_eq!(resolve_machine_id(&paths), first);
    }

    #[test]
    fn test_resolve_machine_id_unwritable_falls_back_to_memory() {
        let dir = temp_dir("unwritable");
        // 以普通文件占位父目录路径，create_dir_all 必然失败
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, b"x").unwrap_or_else(|e| panic!("写入失败: {e}"));
        let paths = MachineIdPaths {
            etc_machine_id: dir.join("missing-etc"),
            dbus_machine_id: dir.join("missing-dbus"),
            local_file: blocker.join("machine-id"),
        };
        let id = resolve_machine_id(&paths);
        assert_eq!(id.len(), 32);
        assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
        // 未产生持久化文件
        assert!(std::fs::read_to_string(&paths.local_file).is_err());
    }

    #[test]
    fn test_cpu_usage_diff() {
        let prev = CpuSample {
            total: 100.0,
            idle: 80.0,
        };
        let curr = CpuSample {
            total: 200.0,
            idle: 160.0,
        };
        assert!((cpu_usage_pct(&prev, &curr) - 20.0).abs() < 1e-9);
        // 全忙
        let busy = CpuSample {
            total: 200.0,
            idle: 80.0,
        };
        assert!((cpu_usage_pct(&prev, &busy) - 100.0).abs() < 1e-9);
        // 时间倒流
        assert_eq!(cpu_usage_pct(&curr, &prev), 0.0);
    }

    #[test]
    fn test_parse_cpu_aggregate() {
        let text = "cpu  10 20 30 40 10 0 0 0 0 0\ncpu0 1 1 1 1 0 0 0 0 0 0\nintr 1\n";
        let sample = parse_cpu_aggregate(text).unwrap_or_else(|| panic!("应解析出聚合行"));
        assert!((sample.total - 110.0).abs() < 1e-9);
        assert!((sample.idle - 50.0).abs() < 1e-9);
        assert!(parse_cpu_aggregate("cpu").is_none());
        assert!(parse_cpu_aggregate("cpu a b c d").is_none());
        assert!(parse_cpu_aggregate("intr 1").is_none());
    }

    #[test]
    fn test_disk_rates() {
        let prev = DiskCounters {
            reads: 100.0,
            writes: 50.0,
            io_time_secs: 10.0,
        };
        let curr = DiskCounters {
            reads: 160.0,
            writes: 60.0,
            io_time_secs: 16.0,
        };
        let (read_iops, write_iops, util_pct) = disk_rates(&prev, &curr, 10.0);
        assert!((read_iops - 6.0).abs() < 1e-9);
        assert!((write_iops - 1.0).abs() < 1e-9);
        assert!((util_pct - 60.0).abs() < 1e-9);
        // 计数重置
        let reset = DiskCounters {
            reads: 1.0,
            writes: 1.0,
            io_time_secs: 0.0,
        };
        let (r, w, u) = disk_rates(&curr, &reset, 10.0);
        assert_eq!((r, w, u), (0.0, 0.0, 0.0));
        // 零间隔
        let (r, w, u) = disk_rates(&prev, &curr, 0.0);
        assert_eq!((r, w, u), (0.0, 0.0, 0.0));
    }

    #[test]
    fn test_net_rates() {
        let prev = NetCounters {
            rx_bytes: 1_000.0,
            tx_bytes: 500.0,
        };
        let curr = NetCounters {
            rx_bytes: 11_000.0,
            tx_bytes: 1_500.0,
        };
        let (rx_bps, tx_bps) = net_rates(&prev, &curr, 10.0);
        assert!((rx_bps - 1_000.0).abs() < 1e-9);
        assert!((tx_bps - 100.0).abs() < 1e-9);
        // 回绕按 0
        let reset = NetCounters {
            rx_bytes: 0.0,
            tx_bytes: 0.0,
        };
        assert_eq!(net_rates(&curr, &reset, 10.0), (0.0, 0.0));
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
    fn test_disk_device_name() {
        assert_eq!(disk_device_name("/dev/sda1"), "sda1");
        assert_eq!(disk_device_name("/dev/mapper/vg-root"), "vg-root");
        assert_eq!(disk_device_name("tmpfs"), "tmpfs");
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
    fn test_write_renewed_material() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("renew-material");
        write_renewed_material(&dir, "CERT-1", "KEY-1")
            .unwrap_or_else(|e| panic!("首次落盘失败: {e}"));
        let cert = std::fs::read_to_string(dir.join("client.pem"))
            .unwrap_or_else(|e| panic!("读取证书失败: {e}"));
        assert_eq!(cert, "CERT-1");
        let key = std::fs::read_to_string(dir.join("client.key"))
            .unwrap_or_else(|e| panic!("读取私钥失败: {e}"));
        assert_eq!(key, "KEY-1");
        let cert_mode = std::fs::metadata(dir.join("client.pem"))
            .unwrap_or_else(|e| panic!("读证书元数据失败: {e}"))
            .permissions()
            .mode();
        assert_eq!(cert_mode & 0o777, 0o644, "证书应为 0644");
        let key_mode = std::fs::metadata(dir.join("client.key"))
            .unwrap_or_else(|e| panic!("读私钥元数据失败: {e}"))
            .permissions()
            .mode();
        assert_eq!(key_mode & 0o777, 0o600, "私钥应为 0600");
        // 目录内无 .new 临时文件残留
        let leftovers = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("读目录失败: {e}"))
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".new"))
            .count();
        assert_eq!(leftovers, 0, "不应有临时文件残留");

        // 再次落盘应整体替换
        write_renewed_material(&dir, "CERT-2", "KEY-2")
            .unwrap_or_else(|e| panic!("替换落盘失败: {e}"));
        let cert = std::fs::read_to_string(dir.join("client.pem"))
            .unwrap_or_else(|e| panic!("复读证书失败: {e}"));
        assert_eq!(cert, "CERT-2");

        // 目录不存在应整体失败且不产生半成品
        let missing = dir.join("no-such-subdir");
        assert!(write_renewed_material(&missing, "CERT-3", "KEY-3").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
