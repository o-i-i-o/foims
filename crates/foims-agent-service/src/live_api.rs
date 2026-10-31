//! SNMP 实时快照 Web API。
//!
//! 数据来自 [`crate::live_poll::LiveRegistry`]（只驻内存，不落库）：
//! [`list_live`] 给全量摘要列表（首次加载与 SSE 不可用时的兜底），
//! [`live_stream`] 推送变更（摘要 + 可选单设备端口明细）。
//!
//! 鉴权沿用 `/api/agents` nest 内的 `agent_admin_guard`（admin/secadmin）；
//! 浏览器 EventSource 同源请求自动携带 HttpOnly Cookie，无需令牌参数。

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{HeaderName, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::IntervalStream;
use uuid::Uuid;

use foims_auth::extractor::AdminOrSecAdminUser;
use foims_common::{AppError, msg};

use crate::agents_api::rfc3339;
use crate::live_poll::{LiveDeviceSnapshot, LiveRegistry};

/// SSE 推送节拍：与默认采样间隔（1s）对齐，注册表有写入时逐秒下发。
const SSE_TICK_SECS: u64 = 1;
/// SSE 心跳间隔：低于常见代理 60s 空闲超时，避免连接被误断。
const SSE_KEEPALIVE_SECS: u64 = 15;

/// 读取实时注册表的 AppState 能力（避免 agent-service 反向依赖主 crate）。
pub trait LiveProvider {
    fn live_registry(&self) -> Arc<LiveRegistry>;

    /// 服务关闭信号：SSE 长连接需在退出时主动结束，否则会阻塞优雅关闭。
    fn live_shutdown(&self) -> tokio::sync::broadcast::Receiver<()>;
}

/// GET /api/agents/live/stream 查询参数。
#[derive(Debug, Deserialize)]
pub struct LiveStreamQuery {
    /// 可选：订阅单台设备的端口明细（详情弹窗使用）
    device_id: Option<Uuid>,
}

/// 单设备实时摘要（SSE `summary` 事件元素与列表接口共用）。
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct LiveSummaryRow {
    pub device_id: Uuid,
    pub device_name: String,
    pub ip: String,
    pub collected_at: String,
    pub vendor: String,
    pub cpu_usage_pct: Option<f64>,
    pub mem_usage_pct: Option<f64>,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
    pub temperature_c: Option<f64>,
    pub uptime_secs: Option<i64>,
    /// 本轮流量采集端口名（未配置时为首端口）
    pub monitor_if: Option<String>,
    /// 采集端口入/出向速率（bit/s），不再全设备求和
    pub rx_bps: Option<f64>,
    pub tx_bps: Option<f64>,
    pub ports_up: u32,
    pub ports_total: u32,
    pub partial: bool,
}

/// 单端口实时状态（SSE `device` 事件明细）。
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct LivePort {
    pub if_index: u32,
    pub name: String,
    pub speed_bps: Option<u64>,
    pub admin_up: Option<bool>,
    pub oper_up: Option<bool>,
    pub rx_bps: Option<f64>,
    pub tx_bps: Option<f64>,
}

/// 单设备实时明细（摘要 + 端口）。
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct LiveDeviceDetail {
    pub summary: LiveSummaryRow,
    pub ports: Vec<LivePort>,
}

fn summary_row(device: &LiveDeviceSnapshot) -> LiveSummaryRow {
    let snapshot = &device.snapshot;
    LiveSummaryRow {
        device_id: device.device_id,
        device_name: device.device_name.clone(),
        ip: device.ip.clone(),
        collected_at: rfc3339(snapshot.collected_at),
        vendor: snapshot.vendor.as_str().to_string(),
        cpu_usage_pct: snapshot.cpu_usage_pct,
        mem_usage_pct: snapshot.mem_usage_pct,
        mem_used: snapshot.mem_used,
        mem_total: snapshot.mem_total,
        temperature_c: snapshot.temperature_c,
        uptime_secs: snapshot.uptime_secs,
        monitor_if: snapshot.monitor_if.clone(),
        rx_bps: snapshot.rx_bps,
        tx_bps: snapshot.tx_bps,
        ports_up: snapshot
            .ports
            .iter()
            .filter(|port| port.oper_up == Some(true))
            .count() as u32,
        ports_total: snapshot.ports.len() as u32,
        partial: snapshot.partial,
    }
}

/// 全量摘要：按设备名/IP 排序，保证 SSE 与列表输出稳定可比。
fn summary_rows(registry: &LiveRegistry) -> Vec<LiveSummaryRow> {
    let mut rows: Vec<LiveSummaryRow> = registry.all().iter().map(|d| summary_row(d)).collect();
    rows.sort_by(|a, b| {
        a.device_name
            .cmp(&b.device_name)
            .then_with(|| a.ip.cmp(&b.ip))
    });
    rows
}

fn device_detail(device: &LiveDeviceSnapshot) -> LiveDeviceDetail {
    LiveDeviceDetail {
        summary: summary_row(device),
        ports: device
            .snapshot
            .ports
            .iter()
            .map(|port| LivePort {
                if_index: port.if_index,
                name: port.name.clone(),
                speed_bps: port.speed_bps,
                admin_up: port.admin_up,
                oper_up: port.oper_up,
                rx_bps: port.rx_bps,
                tx_bps: port.tx_bps,
            })
            .collect(),
    }
}

/// GET /api/agents/live：全量实时快照（不含端口明细）。
pub async fn list_live<S: LiveProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
) -> Result<Response, AppError> {
    let rows = summary_rows(&state.live_registry());
    Ok(foims_common::ok_json(
        rows,
        msg("server.agent.list_retrieved"),
    ))
}

/// SSE 流的去重状态：注册表代数 + 已推送的设备快照时间 + 关闭信号。
struct StreamState {
    revision: u64,
    collected_at: Option<DateTime<Utc>>,
    shutdown: tokio::sync::broadcast::Receiver<()>,
}

/// GET /api/agents/live/stream：SSE 变更推送。
///
/// - `summary`：注册表有写入时下发全部设备摘要；
/// - `device`：仅当请求带 `device_id` 且该设备快照更新时下发端口明细。
///
/// 设备不可达期间注册表保留最后一份成功快照（不推送空值），消费方按
/// `collected_at` 判断新鲜度。
pub async fn live_stream<S: LiveProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
    Query(query): Query<LiveStreamQuery>,
) -> Response {
    let registry = state.live_registry();
    let device_id = query.device_id;
    let tick = IntervalStream::new(tokio::time::interval(Duration::from_secs(SSE_TICK_SECS)));
    let stream_state = StreamState {
        revision: u64::MAX,
        collected_at: None,
        shutdown: state.live_shutdown(),
    };

    let stream = tick
        .scan(stream_state, move |stream_state, _| {
            // 进程关闭：结束流以放行优雅关闭（最长延迟一个节拍）
            match stream_state.shutdown.try_recv() {
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {}
                _ => return std::future::ready(None),
            }
            let mut events: Vec<Result<Event, Infallible>> = Vec::new();
            let revision = registry.revision();
            if revision != stream_state.revision {
                stream_state.revision = revision;
                if let Ok(data) = serde_json::to_string(&summary_rows(&registry)) {
                    events.push(Ok(Event::default().event("summary").data(data)));
                }
            }
            if let Some(device_id) = device_id
                && let Some(device) = registry.get(device_id)
                && stream_state.collected_at != Some(device.snapshot.collected_at)
            {
                stream_state.collected_at = Some(device.snapshot.collected_at);
                if let Ok(data) = serde_json::to_string(&device_detail(&device)) {
                    events.push(Ok(Event::default().event("device").data(data)));
                }
            }
            std::future::ready(Some(events))
        })
        .flat_map(tokio_stream::iter);

    let sse = Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(SSE_KEEPALIVE_SECS))
            .text("keep-alive"),
    );
    // X-Accel-Buffering: nginx 关闭该响应的缓冲（SSE 必须逐条下发）
    (
        [
            (header::CACHE_CONTROL, "no-cache"),
            (HeaderName::from_static("x-accel-buffering"), "no"),
        ],
        sse,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{device_detail, summary_row, summary_rows};
    use crate::live_poll::{LiveDeviceSnapshot, LiveRegistry};
    use chrono::Utc;
    use foims_snmp::live::{LivePortSnapshot, LiveSnapshot, VendorKind};
    use uuid::Uuid;

    fn sample_snapshot(name: &str) -> LiveDeviceSnapshot {
        LiveDeviceSnapshot {
            device_id: Uuid::new_v4(),
            device_name: name.to_string(),
            ip: "192.0.2.10".to_string(),
            snapshot: LiveSnapshot {
                collected_at: Utc::now(),
                vendor: VendorKind::Cisco,
                cpu_usage_pct: Some(12.5),
                mem_total: Some(1024),
                mem_used: Some(256),
                mem_usage_pct: Some(25.0),
                temperature_c: Some(41.0),
                uptime_secs: Some(3600),
                monitor_if: Some("Gi0/1".to_string()),
                ports: vec![
                    LivePortSnapshot {
                        if_index: 1,
                        name: "Gi0/1".to_string(),
                        speed_bps: Some(1_000_000_000),
                        admin_up: Some(true),
                        oper_up: Some(true),
                        rx_bps: Some(1000.0),
                        tx_bps: Some(2000.0),
                    },
                    LivePortSnapshot {
                        if_index: 2,
                        name: "Gi0/2".to_string(),
                        speed_bps: None,
                        admin_up: Some(true),
                        oper_up: Some(false),
                        rx_bps: None,
                        tx_bps: None,
                    },
                ],
                rx_bps: Some(1000.0),
                tx_bps: Some(2000.0),
                responded: true,
                partial: false,
            },
        }
    }

    #[test]
    fn 摘要_统计端口与厂商() {
        let row = summary_row(&sample_snapshot("sw-1"));
        assert_eq!(row.device_name, "sw-1");
        assert_eq!(row.vendor, "cisco");
        assert_eq!(row.ports_total, 2);
        assert_eq!(row.ports_up, 1);
        assert_eq!(row.cpu_usage_pct, Some(12.5));
        // RFC 3339 秒精度 + Z 后缀
        assert!(row.collected_at.ends_with('Z'));
    }

    #[test]
    fn 摘要列表_按设备名排序() {
        let registry = LiveRegistry::new();
        registry.upsert(sample_snapshot("sw-b"));
        registry.upsert(sample_snapshot("sw-a"));
        let names: Vec<String> = summary_rows(&registry)
            .into_iter()
            .map(|row| row.device_name)
            .collect();
        assert_eq!(names, vec!["sw-a", "sw-b"]);
    }

    #[test]
    fn 设备明细_携带端口列表() {
        let detail = device_detail(&sample_snapshot("sw-1"));
        assert_eq!(detail.summary.ports_total, 2);
        assert_eq!(detail.ports.len(), 2);
        assert_eq!(detail.ports[0].if_index, 1);
        assert_eq!(detail.ports[0].oper_up, Some(true));
        assert_eq!(detail.ports[1].oper_up, Some(false));
    }
}
