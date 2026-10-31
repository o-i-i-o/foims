//! 主机监控调试：Ping / DNS 解析 / TCP 端口探测（admin/secadmin，手动输入目标）。
//!
//! SNMP GET/WALK 与 MIB 对照在 `foims_snmp::debug`（依赖 SNMP 客户端）。本模块
//! 聚焦通用网络诊断：ping 经系统 `ping` 二进制（部署主机需具备 iputils，缺失即
//! 明确报错、不静默回退），DNS/端口经 tokio。目标地址沿用
//! `foims_common::net::resolve_probe_ip` 的 SSRF 约束（ping/端口触网场景）；
//! DNS 为纯解析、按信息性查询不加 SSRF 限制。角色由 `/api/agents` nest 的
//! `agent_admin_guard` 强制，处理器不再取令牌。

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::response::Response;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use foims_common::net::resolve_probe_ip;
use foims_common::{AppError, AppJson, msg, ok_json};

/// ping 发包数上限（防放大）。
const PING_MAX_COUNT: u32 = 10;
/// ping 单包等待（`-W`）秒上限。
const PING_MAX_WAIT_SECS: u64 = 8;
/// TCP 端口连接超时秒上限。
const PORT_MAX_TIMEOUT_SECS: u64 = 10;

/// POST /api/agents/debug/ping：对目标执行有限次 ICMP ping，回显可达性与 RTT。
pub async fn debug_ping(AppJson(req): AppJson<PingRequest>) -> Result<Response, AppError> {
    let host = req.host.trim();
    if host.is_empty() {
        return Err(AppError::Validation(msg("server.debug.target_required")));
    }
    // 解析并施加 SSRF 约束，ping 解析后的 IP（避免主机名绕过约束）
    let ip = resolve_probe_ip(host).await?;

    let count = req.count.unwrap_or(4).clamp(1, PING_MAX_COUNT);
    let wait = req.timeout_secs.unwrap_or(2).clamp(1, PING_MAX_WAIT_SECS);

    let output = tokio::process::Command::new("ping")
        .args([
            "-c",
            &count.to_string(),
            "-W",
            &wait.to_string(),
            &ip.to_string(),
        ])
        .output()
        .await
        .map_err(|_| AppError::Database(msg("server.debug.ping_unavailable")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let text = if stdout.is_empty() { stderr.clone() } else { stdout };

    let parsed = parse_ping_output(&text);
    Ok(ok_json(
        PingResult {
            host: host.to_string(),
            address: ip.to_string(),
            success: parsed.received > 0,
            transmitted: parsed.transmitted,
            received: parsed.received,
            loss_pct: parsed.loss_pct,
            min_ms: parsed.min_ms,
            avg_ms: parsed.avg_ms,
            max_ms: parsed.max_ms,
            raw: text,
        },
        "server.debug.ping.success",
    ))
}

/// POST /api/agents/debug/dns：解析主机名，回显全部地址与耗时（纯解析，不加 SSRF 限制）。
pub async fn debug_dns(AppJson(req): AppJson<DnsRequest>) -> Result<Response, AppError> {
    let host = req.host.trim();
    if host.is_empty() {
        return Err(AppError::Validation(msg("server.debug.target_required")));
    }
    let started = Instant::now();
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, 0))
        .await
        .map_err(|_| {
            AppError::Validation(msg("server.debug.host_unresolved").with("host", host))
        })?
        .collect();
    let elapsed_ms = started.elapsed().as_millis() as u64;

    Ok(ok_json(
        DnsResult {
            host: host.to_string(),
            addresses: addrs.iter().map(|a| a.ip().to_string()).collect(),
            elapsed_ms,
        },
        "server.debug.dns.success",
    ))
}

/// POST /api/agents/debug/port：对目标端口发起 TCP 连接，回显可达性与握手耗时。
pub async fn debug_port(AppJson(req): AppJson<PortRequest>) -> Result<Response, AppError> {
    let host = req.host.trim();
    if host.is_empty() {
        return Err(AppError::Validation(msg("server.debug.target_required")));
    }
    let ip = resolve_probe_ip(host).await?;
    let port = req.port;
    let timeout = Duration::from_secs(req.timeout_secs.unwrap_or(3).clamp(1, PORT_MAX_TIMEOUT_SECS));
    let addr = SocketAddr::new(ip, port);

    let started = Instant::now();
    let attempt = tokio::time::timeout(timeout, connect_tcp(addr)).await;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let (reachable, error) = match attempt {
        Err(_) => (false, msg("server.debug.port_timeout").to_string()),
        Ok(Err(_)) => (false, msg("server.debug.port_refused").to_string()),
        Ok(Ok(())) => (true, String::new()),
    };

    Ok(ok_json(
        PortResult {
            host: host.to_string(),
            address: ip.to_string(),
            port,
            reachable,
            elapsed_ms,
            error,
        },
        "server.debug.port.success",
    ))
}

/// 建立 TCP 连接并立即关闭，仅探测端口是否可达。
async fn connect_tcp(addr: SocketAddr) -> std::io::Result<()> {
    let mut stream = TcpStream::connect(addr).await?;
    // 主动 shutdown 避免遗留半开连接；错误在此探测语义下可忽略
    let _ = stream.shutdown().await;
    Ok(())
}

/// ping 输出解析结果（内部结构，仅取诊断关心的计数字段）。
struct PingParsed {
    transmitted: u32,
    received: u32,
    loss_pct: f64,
    min_ms: Option<f64>,
    avg_ms: Option<f64>,
    max_ms: Option<f64>,
}

/// 从 `ping` 文本输出提取统计行：丢包/收发计数与 RTT min/avg/max。
///
/// 逐行扫描，命中关键字才提取；字段缺失保持默认（0 / None），不影响可达性判定
/// （可达性由 received>0 决定）。不依赖固定语言环境文案，尽力解析。
fn parse_ping_output(text: &str) -> PingParsed {
    let mut parsed = PingParsed {
        transmitted: 0,
        received: 0,
        loss_pct: 100.0,
        min_ms: None,
        avg_ms: None,
        max_ms: None,
    };
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("packet loss") {
            if let Some((tx, rx, loss)) = parse_stats_line(line) {
                parsed.transmitted = tx;
                parsed.received = rx;
                parsed.loss_pct = loss;
            }
        } else if lower.contains("min/avg/max") {
            if let Some((min, avg, max)) = parse_rtt_line(line) {
                parsed.min_ms = Some(min);
                parsed.avg_ms = Some(avg);
                parsed.max_ms = Some(max);
            }
        }
    }
    parsed
}

/// 解析统计行：`N packets transmitted, M received, P% packet loss`。
fn parse_stats_line(line: &str) -> Option<(u32, u32, f64)> {
    let tokens: Vec<String> = line
        .split_whitespace()
        .map(|tk| tk.trim_matches(|c| c == ',' || c == '%').to_string())
        .collect();

    let transmitted = find_number_before_keyword(&tokens, "transmitted")?;
    let received = find_number_before_keyword(&tokens, "received").unwrap_or(0);
    let loss = find_percent_before_keyword(&tokens, "loss").unwrap_or(0.0);
    Some((transmitted, received, loss))
}

/// RTT 行：`... = min/avg/max/mdev ms`，取 `=` 后首个空白分隔的斜杠串前三段。
fn parse_rtt_line(line: &str) -> Option<(f64, f64, f64)> {
    let after = line.split_once('=').map(|(_, rest)| rest)?.trim();
    let group = after.split_whitespace().next()?;
    let parts: Vec<&str> = group.split('/').collect();
    if parts.len() < 3 {
        return None;
    }
    let min = parts[0].parse::<f64>().ok()?;
    let avg = parts[1].parse::<f64>().ok()?;
    let max = parts[2].parse::<f64>().ok()?;
    Some((min, avg, max))
}

/// 在关键字之前定位最近的整数值（如 "4" transmitted）。
fn find_number_before_keyword(tokens: &[String], keyword: &str) -> Option<u32> {
    let pos = tokens.iter().position(|tk| tk.eq_ignore_ascii_case(keyword))?;
    tokens[..pos].iter().rev().find_map(|tk| tk.parse::<u32>().ok())
}

/// 在关键字之前定位最近的百分比值（丢包率，如 "0" 对应 0%）。
fn find_percent_before_keyword(tokens: &[String], keyword: &str) -> Option<f64> {
    // 统计行按 `,` 分段，百分号紧跟数字（"0%"）；去分隔后数字 token 即为百分比值
    let pos = tokens.iter().position(|tk| tk.eq_ignore_ascii_case(keyword))?;
    tokens[..pos]
        .iter()
        .rev()
        .find_map(|tk| tk.trim_end_matches('%').parse::<f64>().ok())
}

/// ping 请求体。
#[derive(Debug, serde::Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct PingRequest {
    /// 目标主机名或 IP
    pub host: String,
    /// 发包数（缺省 4，上限 10）
    pub count: Option<u32>,
    /// 单包等待秒（缺省 2，上限 8）
    pub timeout_secs: Option<u64>,
}

/// ping 结果。
#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct PingResult {
    pub host: String,
    pub address: String,
    pub success: bool,
    pub transmitted: u32,
    pub received: u32,
    pub loss_pct: f64,
    pub min_ms: Option<f64>,
    pub avg_ms: Option<f64>,
    pub max_ms: Option<f64>,
    /// ping 原始输出（截断展示用）
    pub raw: String,
}

/// DNS 请求体。
#[derive(Debug, serde::Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct DnsRequest {
    pub host: String,
}

/// DNS 结果。
#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DnsResult {
    pub host: String,
    pub addresses: Vec<String>,
    pub elapsed_ms: u64,
}

/// TCP 端口探测请求体。
#[derive(Debug, serde::Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct PortRequest {
    pub host: String,
    pub port: u16,
    /// 连接超时秒（缺省 3，上限 10）
    pub timeout_secs: Option<u64>,
}

/// TCP 端口探测结果。
#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct PortResult {
    pub host: String,
    pub address: String,
    pub port: u16,
    pub reachable: bool,
    pub elapsed_ms: u64,
    /// 不可达时的错误文案（可达为空串）
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stats_and_rtt_lines() {
        let text = "PING 10.0.0.1 (10.0.0.1) 56(84) bytes of data.\n\
                    64 bytes from 10.0.0.1: icmp_seq=1 ttl=64 time=0.045 ms\n\
                    \n\
                    --- 10.0.0.1 ping statistics ---\n\
                    4 packets transmitted, 4 received, 0% packet loss, time 3005ms\n\
                    rtt min/avg/max/mdev = 0.045/0.052/0.061/0.007 ms";
        let parsed = parse_ping_output(text);
        assert_eq!(parsed.transmitted, 4);
        assert_eq!(parsed.received, 4);
        assert_eq!(parsed.loss_pct, 0.0);
        assert_eq!(parsed.min_ms, Some(0.045));
        assert_eq!(parsed.avg_ms, Some(0.052));
        assert_eq!(parsed.max_ms, Some(0.061));
    }

    #[test]
    fn parse_total_loss_keeps_zero_received() {
        let text = "4 packets transmitted, 0 received, 100% packet loss, time 3017ms";
        let parsed = parse_ping_output(text);
        assert_eq!(parsed.transmitted, 4);
        assert_eq!(parsed.received, 0);
        assert_eq!(parsed.loss_pct, 100.0);
        assert!(parsed.max_ms.is_none());
    }
}
