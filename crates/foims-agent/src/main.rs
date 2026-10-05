//! FOIMS Agent 可执行入口（demo 阶段：本地采集 + 文本/JSON 输出）。
//!
//! 用法：
//! - `foims-agent`                          采集一次，输出 Prometheus 文本格式
//! - `foims-agent --list`                   列出已移植的采集器
//! - `foims-agent --only loadavg,meminfo`   仅启用指定采集器
//! - `foims-agent --format json`            采集一次，输出 JSON
//! - `foims-agent --listen 0.0.0.0:9100`    常驻模式：HTTP 提供 /metrics
//!
//! 日志统一走 tracing 输出到 stderr，stdout 只承载指标数据（便于管道/对照验证）。

use std::io::{self, Read, Write as IoWrite};
use std::net::TcpStream;
use std::process::ExitCode;
use std::sync::Arc;

use foims_agent::{MetricFamily, VERSION, default_collectors, encode_text_all, scrape};

/// 输出格式
#[derive(Debug, Clone, Copy)]
enum OutputFormat {
    /// Prometheus 文本格式（与 node_exporter 一致）
    Prometheus,
    /// JSON（demo 调试用）
    Json,
}

/// 命令行参数
struct Cli {
    list: bool,
    version: bool,
    only: Option<Vec<String>>,
    format: OutputFormat,
    listen: Option<String>,
}

fn main() -> ExitCode {
    init_tracing();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_cli(&args) {
        Err(message) => {
            stderr_line(&format!("参数错误: {message}"));
            stderr_line(USAGE);
            ExitCode::from(2)
        }
        Ok(cli) => run(cli),
    }
}

const USAGE: &str = "用法: foims-agent [--version] [--list] [--only 名称,名称] [--format prometheus|json] [--listen ADDR:PORT]";

/// 初始化日志：默认 info 级，可用 RUST_LOG 覆盖；写入 stderr 避免污染指标输出
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .init();
}

/// 解析命令行参数
fn parse_cli(args: &[String]) -> Result<Cli, String> {
    let mut cli = Cli {
        list: false,
        version: false,
        only: None,
        format: OutputFormat::Prometheus,
        listen: None,
    };
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "--list" => cli.list = true,
            "--version" | "-V" => cli.version = true,
            "--only" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--only 缺少参数".to_string())?;
                let names: Vec<String> = value
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
                    .collect();
                if names.is_empty() {
                    return Err("--only 参数为空".to_string());
                }
                cli.only = Some(names);
            }
            "--format" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--format 缺少参数".to_string())?;
                cli.format = match value.as_str() {
                    "prometheus" | "text" => OutputFormat::Prometheus,
                    "json" => OutputFormat::Json,
                    other => return Err(format!("未知输出格式: {other}")),
                };
            }
            "--listen" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--listen 缺少参数".to_string())?;
                cli.listen = Some(value.clone());
            }
            "--help" | "-h" => {
                stderr_line(USAGE);
                std::process::exit(0);
            }
            other => return Err(format!("未知参数: {other}")),
        }
        index += 1;
    }
    Ok(cli)
}

fn run(cli: Cli) -> ExitCode {
    if cli.list {
        return list_collectors();
    }
    if cli.version {
        // 与 --help 一致走 stderr（本 CLI 约定 stdout 只承载指标数据）
        stderr_line(&format!("foims-agent {VERSION}"));
        return ExitCode::SUCCESS;
    }
    // 校验 --only 名称合法性（不存在的名称直接报错，避免静默空输出）
    if let Some(names) = &cli.only {
        let available: Vec<&str> = default_collectors(None).iter().map(|c| c.name()).collect();
        for name in names {
            if !available.contains(&name.as_str()) {
                stderr_line(&format!(
                    "未知采集器: {name}（可用: {}）",
                    available.join(", ")
                ));
                return ExitCode::from(2);
            }
        }
    }
    let collectors = Arc::new(default_collectors(cli.only.as_deref()));
    match &cli.listen {
        Some(addr) => match serve(addr, collectors, cli.format) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                stderr_line(&message);
                ExitCode::FAILURE
            }
        },
        None => one_shot(&collectors, cli.format),
    }
}

/// 列出全部已移植采集器（每行一个，便于脚本处理）
fn list_collectors() -> ExitCode {
    let mut out = io::stdout().lock();
    for collector in default_collectors(None) {
        if let Err(error) = writeln!(out, "{}", collector.name()) {
            stderr_line(&format!("输出失败: {error}"));
            return ExitCode::FAILURE;
        }
    }
    if out.flush().is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// 采集一次并输出到 stdout
fn one_shot(collectors: &[Box<dyn foims_agent::Collector>], format: OutputFormat) -> ExitCode {
    let families = scrape(collectors);
    let text = match render(&families, format) {
        Ok(text) => text,
        Err(message) => {
            stderr_line(&message);
            return ExitCode::FAILURE;
        }
    };
    let mut out = io::stdout().lock();
    if let Err(error) = out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        stderr_line(&format!("输出失败: {error}"));
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// 按指定格式渲染指标
fn render(families: &[MetricFamily], format: OutputFormat) -> Result<String, String> {
    match format {
        OutputFormat::Prometheus => Ok(encode_text_all(families)),
        OutputFormat::Json => {
            let value = serde_json::json!(
                families
                    .iter()
                    .map(|family| serde_json::json!({
                        "name": family.name,
                        "help": family.help,
                        "type": family.mtype.as_str(),
                        "samples": family
                            .samples
                            .iter()
                            .map(|sample| serde_json::json!({
                                "labels": sample
                                    .labels
                                    .iter()
                                    .cloned()
                                    .collect::<std::collections::BTreeMap<String, String>>(),
                                "value": sample.value,
                            }))
                            .collect::<Vec<serde_json::Value>>(),
                    }))
                    .collect::<Vec<serde_json::Value>>()
            );
            serde_json::to_string_pretty(&value).map_err(|error| format!("JSON 编码失败: {error}"))
        }
    }
}

/// 常驻模式：极简 HTTP/1.1 服务，GET /metrics 返回指标
fn serve(
    addr: &str,
    collectors: Arc<Vec<Box<dyn foims_agent::Collector>>>,
    format: OutputFormat,
) -> Result<(), String> {
    let listener =
        std::net::TcpListener::bind(addr).map_err(|error| format!("绑定 {addr} 失败: {error}"))?;
    tracing::info!(addr, version = VERSION, "foims-agent HTTP 服务已启动");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let collectors = Arc::clone(&collectors);
                std::thread::spawn(move || handle_conn(stream, collectors, format));
            }
            Err(error) => tracing::warn!(%error, "接受连接失败"),
        }
    }
    Ok(())
}

/// 处理单个 HTTP 连接（demo 实现：仅解析请求行，不持久连接）
fn handle_conn(
    mut stream: TcpStream,
    collectors: Arc<Vec<Box<dyn foims_agent::Collector>>>,
    format: OutputFormat,
) {
    let mut buffer = [0u8; 4096];
    let read = match stream.read(&mut buffer) {
        Ok(read) => read,
        Err(error) => {
            tracing::debug!(%error, "读取请求失败");
            return;
        }
    };
    let request = String::from_utf8_lossy(&buffer[..read]);
    // 请求行形如 "GET /metrics?x=1 HTTP/1.1"，取中间的路径并去掉查询串
    let raw_path = request.split_whitespace().nth(1).unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/");

    let (status, content_type, body) = match path {
        "/metrics" => {
            let families = scrape(&collectors);
            match render(&families, format) {
                Ok(text) => ("200 OK", metrics_content_type(format), text),
                Err(message) => (
                    "500 Internal Server Error",
                    "text/plain; charset=utf-8",
                    format!("渲染失败: {message}\n"),
                ),
            }
        }
        "/" => ("200 OK", "text/plain; charset=utf-8", landing_page()),
        _ => (
            "404 Not Found",
            "text/plain; charset=utf-8",
            "404 page not found\n".to_string(),
        ),
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    if let Err(error) = stream.write_all(response.as_bytes()) {
        tracing::debug!(%error, "写出响应失败");
    }
}

/// /metrics 的 Content-Type（Prometheus 文本格式带版本标识）
fn metrics_content_type(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Prometheus => "text/plain; version=0.0.4; charset=utf-8",
        OutputFormat::Json => "application/json; charset=utf-8",
    }
}

/// 根路径的简版说明页
fn landing_page() -> String {
    let mut page = String::new();
    page.push_str("<html><head><title>FOIMS Agent</title></head><body>\n");
    page.push_str("<h1>FOIMS Agent</h1>\n<p>node_exporter 的 Rust 重写（demo 阶段）。</p>\n");
    page.push_str("<p><a href=\"/metrics\">Metrics</a></p>\n");
    page.push_str("</body></html>\n");
    page
}

/// 写一行到 stderr（clippy print_stderr 护栏下的统一出口）
fn stderr_line(message: &str) {
    let mut err = io::stderr().lock();
    let _ = writeln!(err, "{message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cli_defaults() {
        let cli = parse_cli(&[]).unwrap_or_else(|e| panic!("解析失败: {e}"));
        assert!(!cli.list);
        assert!(!cli.version);
        assert!(cli.only.is_none());
        assert!(cli.listen.is_none());
        assert!(matches!(cli.format, OutputFormat::Prometheus));
    }

    #[test]
    fn test_parse_cli_version_flag() {
        let args = vec!["--version".to_string()];
        let cli = parse_cli(&args).unwrap_or_else(|e| panic!("解析失败: {e}"));
        assert!(cli.version);
    }

    #[test]
    fn test_parse_cli_full() {
        let args: Vec<String> = vec![
            "--only",
            "loadavg, meminfo",
            "--format",
            "json",
            "--listen",
            "0.0.0.0:9100",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let cli = parse_cli(&args).unwrap_or_else(|e| panic!("解析失败: {e}"));
        assert_eq!(
            cli.only,
            Some(vec!["loadavg".to_string(), "meminfo".to_string()])
        );
        assert_eq!(cli.listen.as_deref(), Some("0.0.0.0:9100"));
        assert!(matches!(cli.format, OutputFormat::Json));
    }

    #[test]
    fn test_parse_cli_rejects_unknown() {
        let args = vec!["--bogus".to_string()];
        assert!(parse_cli(&args).is_err());
    }
}
