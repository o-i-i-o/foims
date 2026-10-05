//! 采集器框架：trait 定义、错误类型、注册表与抓取编排。
//!
//! 对齐 node_exporter 的语义：
//! - 每个采集器实现 [`Collector`]，名称与 node_exporter 保持一致，便于逐项对照移植；
//! - 每次抓取附加 `node_scrape_collector_duration_seconds` 与
//!   `node_scrape_collector_success`（与原版 execute() 行为一致：NoData 也计为失败）。

use std::time::Instant;

use crate::metric::MetricFamily;

/// 采集器统一接口（对应 node_exporter 的 `Collector.Update(ch)`）
pub trait Collector: Send + Sync {
    /// 采集器名称（与 node_exporter 一致）
    fn name(&self) -> &'static str;
    /// 执行一次采集；无数据可采时返回 [`CollectorError::NoData`]（不算失败启动）
    fn collect(&self) -> Result<Vec<MetricFamily>, CollectorError>;
}

/// 采集错误类型
#[derive(Debug, thiserror::Error)]
pub enum CollectorError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("解析失败 {file}: {reason}")]
    Parse { file: &'static str, reason: String },
    #[error("采集器无数据")]
    NoData,
}

/// 构造默认启用的采集器注册表（Linux）。
///
/// `only` 非空时按名称过滤（对应 node_exporter 的 `--collectors.enabled` 场景，
/// demo 阶段用 `--only` 参数传入）。
pub fn default_collectors(only: Option<&[String]>) -> Vec<Box<dyn Collector>> {
    let all: Vec<Box<dyn Collector>> = crate::collectors::build_defaults();
    match only {
        None => all,
        Some(names) => all
            .into_iter()
            .filter(|c| names.iter().any(|n| n == c.name()))
            .collect(),
    }
}

/// 执行一次完整抓取：逐个调用采集器并附加抓取耗时/成功标记指标族。
pub fn scrape(collectors: &[Box<dyn Collector>]) -> Vec<MetricFamily> {
    let mut out = Vec::new();
    for collector in collectors {
        let begin = Instant::now();
        let success = match collector.collect() {
            Ok(mut families) => {
                out.append(&mut families);
                1.0
            }
            // NoData 只记调试日志（与 node_exporter 行为一致）
            Err(CollectorError::NoData) => {
                tracing::debug!(collector = collector.name(), "采集器无数据");
                0.0
            }
            Err(error) => {
                tracing::warn!(collector = collector.name(), error = %error, "采集器失败");
                0.0
            }
        };
        let duration = begin.elapsed().as_secs_f64();
        let name = collector.name();

        let mut duration_family = MetricFamily::new(
            "node_scrape_collector_duration_seconds",
            "node_exporter: Duration of a collector scrape.",
            crate::metric::MetricType::Gauge,
        );
        duration_family.push_labeled(vec![("collector".to_string(), name.to_string())], duration);
        out.push(duration_family);

        let mut success_family = MetricFamily::new(
            "node_scrape_collector_success",
            "node_exporter: Whether a collector succeeded.",
            crate::metric::MetricType::Gauge,
        );
        success_family.push_labeled(vec![("collector".to_string(), name.to_string())], success);
        out.push(success_family);
    }
    out
}
