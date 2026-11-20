//! SNMP 实时快照 Web API。
//!
//! 数据来自 [`crate::live_poll::LiveRegistry`]（只驻内存，不落库）：
//! [`list_live`] 给全量摘要列表（首次加载与 SSE 不可用时的兜底），
//! [`live_stream`] 推送变更（摘要 + 可选单设备端口明细）。
//!
//! 鉴权沿用 `/api/agents` nest 内的 `agent_admin_guard`（admin/secadmin）；
//! 浏览器 EventSource 同源请求自动携带 HttpOnly Cookie，无需令牌参数。

use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
use crate::live_interest::{InterestLease, InterestScope, LiveInterest};
use crate::live_poll::{
    AgentLiveDeviceSnapshot, AgentLiveRegistry, IpmiLiveDeviceSnapshot, IpmiLiveRegistry,
    LiveDeviceSnapshot, LiveRegistry,
};

/// SSE 推送节拍：注册表有更新时按该节拍检查。
const SSE_TICK_SECS: u64 = 1;
/// 仪表盘摘要的最小推送间隔：卡片需要全设备数据，但无需秒级。
const SSE_OVERVIEW_EMIT_SECS: u64 = 5;
/// SSE 心跳间隔：低于常见代理 60s 空闲超时，避免连接被误断。
const SSE_KEEPALIVE_SECS: u64 = 15;
/// 单次订阅允许登记的设备上限（主机列表当前页通常远小于该值）。
const MAX_WATCH_DEVICES: usize = 500;

/// 读取实时注册表的 AppState 能力（避免 agent-service 反向依赖主 crate）。
pub trait LiveProvider {
    fn live_registry(&self) -> Arc<LiveRegistry>;

    /// IPMI 实时快照注册表（与 SNMP 同一 SSE/列表载荷中按 source 区分）。
    fn ipmi_live_registry(&self) -> Arc<IpmiLiveRegistry>;

    /// Agent 上报实时快照注册表（ingest 入库成功后写入，按 machine_id 键）。
    fn agent_live_registry(&self) -> Arc<AgentLiveRegistry>;

    /// 实时观察订阅表：SSE 连接按 scope 登记，采集 worker 据此切换档位。
    fn live_interest(&self) -> Arc<LiveInterest>;

    /// 服务关闭信号：SSE 长连接需在退出时主动结束，否则会阻塞优雅关闭。
    fn live_shutdown(&self) -> tokio::sync::broadcast::Receiver<()>;
}

/// GET /api/agents/live/stream 查询参数。
#[derive(Debug, Deserialize)]
pub struct LiveStreamQuery {
    /// 可选：订阅单台设备的端口明细（详情弹窗使用）
    device_id: Option<Uuid>,
    /// 观察范围：`dashboard`（全设备汇总）或 `agents`（当前列表设备）。
    scope: Option<String>,
    /// `scope=agents` 时登记的当前页设备 id，逗号分隔。
    device_ids: Option<String>,
}

/// 单个传感器读数（IPMI 风扇/电压明细，`source='ipmi'` 行专用）。
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct LiveSensorRow {
    pub name: String,
    pub value: f64,
    pub unit: String,
}

/// 单设备实时摘要（SSE `summary` 事件元素与列表接口共用）。
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct LiveSummaryRow {
    pub device_id: Uuid,
    pub device_name: String,
    pub ip: String,
    pub collected_at: String,
    /// 采集来源：`snmp` | `ipmi` | `agent`（前端徽标与展示板块据此区分）
    pub source: String,
    pub vendor: String,
    pub cpu_usage_pct: Option<f64>,
    pub mem_usage_pct: Option<f64>,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
    /// 磁盘使用率（仅 agent 来源有实时读数；SNMP/IPMI 为 None，磁盘只有
    /// 5 分钟全量轮询有读数）
    pub disk_usage_pct: Option<f64>,
    pub temperature_c: Option<f64>,
    pub uptime_secs: Option<i64>,
    /// 本轮流量采集端口名（未配置时为首端口）
    pub monitor_if: Option<String>,
    /// 采集端口入/出向速率（bit/s），不再全设备求和
    pub rx_bps: Option<f64>,
    pub tx_bps: Option<f64>,
    /// 端口 up 数：仅本轮采样了全端口状态（Detail 档）时有值；
    /// 基线/列表/仪表盘档不采端口状态，为 None（不可误读为 0）
    pub ports_up: Option<u32>,
    /// 端口总数：来自厂商 profile（与是否采样状态无关）；IPMI/Agent 无端口语义，为 None
    pub ports_total: Option<u32>,
    pub partial: bool,
    /// IPMI 系统电源是否开启（Get Chassis Status；SNMP 行为 None）
    pub power_on: Option<bool>,
    /// IPMI 风扇读数列表（SNMP 行为空）
    pub fans: Vec<LiveSensorRow>,
    /// IPMI 电压读数列表（SNMP 行为空）
    pub volts: Vec<LiveSensorRow>,
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
        source: "snmp".to_string(),
        vendor: snapshot.vendor.as_str().to_string(),
        cpu_usage_pct: snapshot.cpu_usage_pct,
        mem_usage_pct: snapshot.mem_usage_pct,
        mem_used: snapshot.mem_used,
        mem_total: snapshot.mem_total,
        disk_usage_pct: None,
        temperature_c: snapshot.temperature_c,
        uptime_secs: snapshot.uptime_secs,
        monitor_if: snapshot.monitor_if.clone(),
        rx_bps: snapshot.rx_bps,
        tx_bps: snapshot.tx_bps,
        // 非 Detail 档快照里的端口状态全为 None：此时 up 数未知，置 None
        ports_up: snapshot
            .ports
            .iter()
            .any(|port| port.admin_up.is_some() || port.oper_up.is_some())
            .then(|| {
                snapshot
                    .ports
                    .iter()
                    .filter(|port| port.oper_up == Some(true))
                    .count() as u32
            }),
        ports_total: Some(snapshot.ports.len() as u32),
        partial: snapshot.partial,
        power_on: None,
        fans: Vec::new(),
        volts: Vec::new(),
    }
}

/// IPMI 实时快照转摘要行：无 CPU/内存/磁盘/端口语义，这些列置空；
/// 温度取 max_temp，风扇/电压明细随载荷下发供前端展示。
fn ipmi_summary_row(device: &IpmiLiveDeviceSnapshot) -> LiveSummaryRow {
    let snapshot = &device.snapshot;
    let map_sensor = |s: &foims_ipmi::collect::SensorValue| LiveSensorRow {
        name: s.name.clone(),
        value: s.value,
        unit: s.unit.clone(),
    };
    LiveSummaryRow {
        device_id: device.device_id,
        device_name: device.device_name.clone(),
        ip: device.ip.clone(),
        collected_at: rfc3339(snapshot.collected_at),
        source: "ipmi".to_string(),
        vendor: snapshot.vendor.clone(),
        cpu_usage_pct: None,
        mem_usage_pct: None,
        mem_used: None,
        mem_total: None,
        disk_usage_pct: None,
        temperature_c: snapshot.max_temp,
        uptime_secs: None,
        monitor_if: None,
        rx_bps: None,
        tx_bps: None,
        ports_up: None,
        ports_total: None,
        partial: false,
        power_on: Some(snapshot.power_on),
        fans: snapshot.fans.iter().map(map_sensor).collect(),
        volts: snapshot.volts.iter().map(map_sensor).collect(),
    }
}

/// Agent 上报快照转摘要行：device_id 取 `monitored_hosts.id`（主机列表按行 id
/// 匹配），vendor 展示操作系统名；流量为各网卡合计（与入库历史快照同口径）。
fn agent_summary_row(host: &AgentLiveDeviceSnapshot) -> LiveSummaryRow {
    let snapshot = &host.snapshot;
    LiveSummaryRow {
        device_id: host.host_id,
        device_name: host.device_name.clone(),
        ip: host.ip.clone(),
        collected_at: rfc3339(snapshot.collected_at),
        source: "agent".to_string(),
        vendor: host.os.clone(),
        cpu_usage_pct: snapshot.cpu_usage_pct,
        mem_usage_pct: snapshot.mem_usage_pct,
        mem_used: snapshot.mem_used,
        mem_total: snapshot.mem_total,
        disk_usage_pct: snapshot.disk_usage_pct,
        temperature_c: snapshot.temperature_c,
        uptime_secs: snapshot.uptime_secs,
        monitor_if: None,
        rx_bps: snapshot.rx_bps,
        tx_bps: snapshot.tx_bps,
        ports_up: None,
        ports_total: None,
        partial: false,
        power_on: None,
        fans: Vec::new(),
        volts: Vec::new(),
    }
}

/// 摘要（SNMP + IPMI + Agent）：按设备名/IP 排序，保证 SSE 与列表输出稳定可比。
///
/// `keep = None` 输出全量（仪表盘与列表接口）；`Some(set)` 只保留指定
/// `device_id`（列表页按当前页、详情按单设备），避免 1s 连接反复序列化全量。
/// SNMP 用 devices.id、IPMI 用 devices.id、Agent 用 monitored_hosts.id，
/// 前端登记集合时按来源取对应 id。
fn summary_rows(
    live: &LiveRegistry,
    ipmi: &IpmiLiveRegistry,
    agent: &AgentLiveRegistry,
    keep: Option<&HashSet<Uuid>>,
) -> Vec<LiveSummaryRow> {
    let wanted = |id: Uuid| keep.is_none_or(|set| set.contains(&id));
    let mut rows: Vec<LiveSummaryRow> = live
        .all()
        .iter()
        .filter(|d| wanted(d.device_id))
        .map(|d| summary_row(d))
        .chain(
            ipmi.all()
                .iter()
                .filter(|d| wanted(d.device_id))
                .map(|d| ipmi_summary_row(d)),
        )
        .chain(
            agent
                .all()
                .iter()
                .filter(|d| wanted(d.host_id))
                .map(|d| agent_summary_row(d)),
        )
        .collect();
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
    let live = state.live_registry();
    let ipmi = state.ipmi_live_registry();
    let agent = state.agent_live_registry();
    let rows = summary_rows(&live, &ipmi, &agent, None);
    Ok(foims_common::ok_json(
        rows,
        msg("server.agent.list_retrieved"),
    ))
}

/// SSE 流的去重状态：三个注册表代数 + 已推送的设备快照时间 + 观察租约。
struct StreamState {
    revision: u64,
    ipmi_revision: u64,
    agent_revision: u64,
    collected_at: Option<DateTime<Utc>>,
    /// 连接持有的观察租约；每个节拍续期，断连 Drop 后采集自动回落基线。
    lease: InterestLease,
    /// 上次下发摘要的时间，用于仪表盘 5s 节流。
    last_emit: Option<Instant>,
    emit_every: Duration,
    shutdown: tokio::sync::broadcast::Receiver<()>,
}

/// 解析 `device_ids=a,b,c`，忽略非法项并限制数量。
fn parse_device_ids(raw: Option<&str>) -> Vec<Uuid> {
    raw.map(|raw| {
        raw.split(',')
            .filter_map(|part| Uuid::parse_str(part.trim()).ok())
            .take(MAX_WATCH_DEVICES)
            .collect()
    })
    .unwrap_or_default()
}

/// GET /api/agents/live/stream：SSE 变更推送。
///
/// - `summary`：按观察范围节流下发摘要（仪表盘 5s 全量；列表/详情 1s，
///   且只含登记设备，避免 1s 连接反复序列化数百条无关设备）；
/// - `device`：仅当请求带 `device_id` 且该设备快照更新时下发端口明细。
///
/// 同时按 `scope`/`device_ids` 登记观察租约，驱动后端切换采样档位。
/// 设备不可达期间注册表保留最后一份成功快照（不推送空值），消费方按
/// `collected_at` 判断新鲜度。
pub async fn live_stream<S: LiveProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
    Query(query): Query<LiveStreamQuery>,
) -> Response {
    let registry = state.live_registry();
    let ipmi_registry = state.ipmi_live_registry();
    let agent_registry = state.agent_live_registry();
    let device_id = query.device_id;
    let (scope, emit_every) = match device_id {
        Some(id) => (
            InterestScope::Detail(id),
            Duration::from_secs(SSE_TICK_SECS),
        ),
        None if query.scope.as_deref() == Some("agents") => {
            (InterestScope::Devices, Duration::from_secs(SSE_TICK_SECS))
        }
        None => (
            InterestScope::Dashboard,
            Duration::from_secs(SSE_OVERVIEW_EMIT_SECS),
        ),
    };
    let devices = parse_device_ids(query.device_ids.as_deref());
    // 摘要裁剪范围：仪表盘全量；列表按下发设备集合；详情只该设备。
    // 列表集合同时含 SNMP/IPMI 的 devices.id 与 Agent 的 monitored_hosts.id，
    // 采集侧只会命中 SNMP worker，注册进来不影响档位。
    let summary_keep: Option<HashSet<Uuid>> = match &scope {
        InterestScope::Dashboard => None,
        InterestScope::Devices => Some(devices.iter().copied().collect()),
        InterestScope::Detail(id) => Some(HashSet::from([*id])),
    };
    let lease = state.live_interest().lease(scope, devices);
    let tick = IntervalStream::new(tokio::time::interval(Duration::from_secs(SSE_TICK_SECS)));
    let stream_state = StreamState {
        revision: u64::MAX,
        ipmi_revision: u64::MAX,
        agent_revision: u64::MAX,
        collected_at: None,
        lease,
        last_emit: None,
        emit_every,
        shutdown: state.live_shutdown(),
    };

    let stream = tick
        .scan(stream_state, move |stream_state, _| {
            // 进程关闭：结束流以放行优雅关闭（最长延迟一个节拍）
            match stream_state.shutdown.try_recv() {
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {}
                _ => return std::future::ready(None),
            }
            // 活连接每个节拍续期，保证观察档位不因 TTL 过期而降级；
            // 真正断连时由流对象 Drop 立即注销，TTL 只是兜底
            stream_state.lease.touch();
            let mut events: Vec<Result<Event, Infallible>> = Vec::new();
            let revision = registry.revision();
            let ipmi_revision = ipmi_registry.revision();
            let agent_revision = agent_registry.revision();
            let due = stream_state
                .last_emit
                .is_none_or(|at| at.elapsed() >= stream_state.emit_every);
            if due
                && (revision != stream_state.revision
                    || ipmi_revision != stream_state.ipmi_revision
                    || agent_revision != stream_state.agent_revision)
            {
                stream_state.revision = revision;
                stream_state.ipmi_revision = ipmi_revision;
                stream_state.agent_revision = agent_revision;
                stream_state.last_emit = Some(Instant::now());
                if let Ok(data) = serde_json::to_string(&summary_rows(
                    &registry,
                    &ipmi_registry,
                    &agent_registry,
                    summary_keep.as_ref(),
                )) {
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
    use super::{
        MAX_WATCH_DEVICES, agent_summary_row, device_detail, ipmi_summary_row, parse_device_ids,
        summary_row, summary_rows,
    };
    use crate::live_poll::{
        AgentLiveDeviceSnapshot, AgentLiveRegistry, AgentLiveSnapshot, IpmiLiveDeviceSnapshot,
        IpmiLiveRegistry, LiveDeviceSnapshot, LiveRegistry,
    };
    use chrono::Utc;
    use foims_ipmi::IpmiLiveSnapshot;
    use foims_snmp::live::{LivePortSnapshot, LiveSnapshot, VendorKind};
    use std::collections::HashSet;
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

    fn sample_ipmi_snapshot(name: &str) -> IpmiLiveDeviceSnapshot {
        IpmiLiveDeviceSnapshot {
            device_id: Uuid::new_v4(),
            device_name: name.to_string(),
            ip: "198.51.100.9".to_string(),
            snapshot: IpmiLiveSnapshot {
                collected_at: Utc::now(),
                responded: true,
                max_temp: Some(55.0),
                fans: Vec::new(),
                volts: Vec::new(),
                power_on: true,
                vendor: "IPMI BMC".to_string(),
            },
        }
    }

    fn sample_agent_snapshot(name: &str) -> AgentLiveDeviceSnapshot {
        AgentLiveDeviceSnapshot {
            host_id: Uuid::new_v4(),
            machine_id: "m-1234567890abcdef".to_string(),
            device_name: name.to_string(),
            ip: "203.0.113.7".to_string(),
            os: "Ubuntu".to_string(),
            snapshot: AgentLiveSnapshot {
                collected_at: Utc::now(),
                cpu_usage_pct: Some(30.0),
                mem_used: Some(512),
                mem_total: Some(2048),
                mem_usage_pct: Some(25.0),
                disk_usage_pct: Some(62.5),
                temperature_c: Some(47.5),
                uptime_secs: Some(7200),
                rx_bps: Some(300.0),
                tx_bps: Some(400.0),
            },
        }
    }

    #[test]
    fn 摘要_统计端口与厂商() {
        let row = summary_row(&sample_snapshot("sw-1"));
        assert_eq!(row.source, "snmp");
        assert_eq!(row.device_name, "sw-1");
        assert_eq!(row.vendor, "cisco");
        assert_eq!(row.ports_total, Some(2));
        assert_eq!(row.ports_up, Some(1));
        assert_eq!(row.cpu_usage_pct, Some(12.5));
        // RFC 3339 秒精度 + Z 后缀
        assert!(row.collected_at.ends_with('Z'));
    }

    #[test]
    fn 摘要_未采端口状态时up数未知() {
        // 基线/列表/仪表盘档不下发 admin/oper，快照端口状态全为 None
        let mut device = sample_snapshot("sw-2");
        for port in &mut device.snapshot.ports {
            port.admin_up = None;
            port.oper_up = None;
        }
        let row = summary_row(&device);
        assert_eq!(row.ports_up, None);
        // 总数来自 profile，与是否采样状态无关
        assert_eq!(row.ports_total, Some(2));
    }

    #[test]
    fn 摘要_按设备集合裁剪() {
        let live = LiveRegistry::new();
        let ipmi = IpmiLiveRegistry::new();
        let agent = AgentLiveRegistry::new();
        let snmp_a = sample_snapshot("sw-a");
        let snmp_b = sample_snapshot("sw-b");
        let id_a = snmp_a.device_id;
        live.upsert(snmp_a);
        live.upsert(snmp_b);
        ipmi.upsert(sample_ipmi_snapshot("bmc-1"));
        agent.upsert(sample_agent_snapshot("host-1"));

        let all = summary_rows(&live, &ipmi, &agent, None);
        assert_eq!(all.len(), 4);

        let keep = HashSet::from([id_a]);
        let filtered = summary_rows(&live, &ipmi, &agent, Some(&keep));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].device_id, id_a);
    }

    #[test]
    fn 设备观察列表_忽略非法项并限量() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let raw = format!("{a}, bad ,{b},");
        assert_eq!(parse_device_ids(Some(&raw)), vec![a, b]);

        let many = (0..MAX_WATCH_DEVICES + 5)
            .map(|n| Uuid::from_u128(n as u128).to_string())
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(parse_device_ids(Some(&many)).len(), MAX_WATCH_DEVICES);
        assert!(parse_device_ids(None).is_empty());
    }

    #[test]
    fn ipmi摘要_无资源列仅温度电源() {
        let row = ipmi_summary_row(&sample_ipmi_snapshot("bmc-1"));
        assert_eq!(row.source, "ipmi");
        assert_eq!(row.temperature_c, Some(55.0));
        assert_eq!(row.power_on, Some(true));
        // IPMI 无 CPU/内存/端口语义
        assert_eq!(row.cpu_usage_pct, None);
        assert_eq!(row.ports_total, None);
    }

    #[test]
    fn agent摘要_资源与磁盘读数带主机id() {
        let host = sample_agent_snapshot("web-01");
        let row = agent_summary_row(&host);
        assert_eq!(row.source, "agent");
        assert_eq!(row.vendor, "Ubuntu");
        // device_id 为 monitored_hosts.id，供主机列表按行 id 匹配
        assert_eq!(row.device_id, host.host_id);
        assert_eq!(row.cpu_usage_pct, Some(30.0));
        assert_eq!(row.disk_usage_pct, Some(62.5));
        assert_eq!(row.temperature_c, Some(47.5));
        assert_eq!(row.rx_bps, Some(300.0));
        // Agent 无端口/电源语义
        assert_eq!(row.ports_total, None);
        assert_eq!(row.power_on, None);
    }

    #[test]
    fn 摘要列表_合并三来源并按设备名排序() {
        let registry = LiveRegistry::new();
        registry.upsert(sample_snapshot("sw-b"));
        registry.upsert(sample_snapshot("sw-a"));
        let ipmi = IpmiLiveRegistry::new();
        ipmi.upsert(sample_ipmi_snapshot("bmc-m"));
        let agent = AgentLiveRegistry::new();
        agent.upsert(sample_agent_snapshot("web-01"));
        let names: Vec<String> = summary_rows(&registry, &ipmi, &agent, None)
            .into_iter()
            .map(|row| row.device_name)
            .collect();
        assert_eq!(names, vec!["bmc-m", "sw-a", "sw-b", "web-01"]);
    }

    #[test]
    fn 设备明细_携带端口列表() {
        let detail = device_detail(&sample_snapshot("sw-1"));
        assert_eq!(detail.summary.ports_total, Some(2));
        assert_eq!(detail.ports.len(), 2);
        assert_eq!(detail.ports[0].if_index, 1);
        assert_eq!(detail.ports[0].oper_up, Some(true));
        assert_eq!(detail.ports[1].oper_up, Some(false));
    }
}
