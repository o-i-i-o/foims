//! SNMP 设备实时采集（秒级定向 GET，内存快照，不落库）。
//!
//! 与 [`crate::snmp_poll`] 的 5 分钟全量轮询互补：
//! - 全量轮询负责趋势落库（monitored_hosts / host_metrics_history）；
//! - 本模块按设备探测出的厂商 profile 只 GET 必需 OID（接口状态、64 位流量
//!   计数器、CPU/内存/温度），结果写入 [`LiveRegistry`] 供实时展示与 Top 榜，
//!   不写指标表；秒级读数就地进 [`crate::alert_episode`] 的告警评估，使告警
//!   延迟与展示同量级。
//!
//! 调度模型：每台设备一个常驻 worker（会话复用，v3 只做一次引擎发现），
//! 启动时按设备序号在采样间隔内错峰；全局信号量限制同时在途的设备数；
//! 单轮无响应即丢弃会话并指数退避（上限 `max_backoff_secs`），恢复后回到
//! 正常间隔。设备清单每 [`DEVICE_RELOAD_SECS`] 重载一次，新增/删除/凭据
//! 变更通过重起对应 worker 生效。
//!
//! 快照语义：只在设备有响应时覆盖注册表；不可达期间保留最后一份成功快照，
//! 消费方以 `snapshot.collected_at` 判断新鲜度（见
//! [`LiveDeviceSnapshot::age_secs`]），不要仅凭条目存在判定在线。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_snmp::transport::UdpHandle;
use async_snmp::{Client, Retry};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use sqlx::PgPool;
use tokio::sync::Semaphore;
use tokio::sync::broadcast::Receiver as ShutdownReceiver;
use tokio::task::JoinHandle;
use uuid::Uuid;

use foims_common::config::SnmpLiveConfig;
use foims_snmp::core::{SnmpParamsLegacy, build_auth, format_snmp_error, snmp_target};
use foims_snmp::live::{LiveCollector, LiveSnapshot, MAX_OIDS_PER_REQUEST, probe};

use crate::alert_episode::{AlertRuntime, HostRef};
use crate::alerts::MetricValues;
use crate::snmp_poll::{
    SnmpDeviceRow, ensure_snmp_host_present, load_snmp_devices, load_snmp_host_id, snmp_machine_id,
};

/// 设备清单重载周期：新增/删除/凭据变更最迟在此时长内生效。
const DEVICE_RELOAD_SECS: u64 = 60;

/// 单设备实时快照（设备身份 + 采集结果）。
#[derive(Debug, Clone)]
pub struct LiveDeviceSnapshot {
    pub device_id: Uuid,
    pub device_name: String,
    pub ip: String,
    pub snapshot: LiveSnapshot,
}

impl LiveDeviceSnapshot {
    /// 快照年龄（秒，负值归零）：消费方据此过滤陈旧数据。
    pub fn age_secs(&self, now: DateTime<Utc>) -> i64 {
        (now - self.snapshot.collected_at).num_seconds().max(0)
    }
}

/// 实时快照注册表：设备 ID → 最近一次成功快照。
///
/// 只驻留内存，进程重启后由 worker 重新填充；写入方为各设备 worker，
/// 读取方为实时展示/Top 榜（后续阶段接入 SSE 与 HTTP）。
#[derive(Debug, Default)]
pub struct LiveRegistry {
    devices: DashMap<Uuid, Arc<LiveDeviceSnapshot>>,
    /// 写入代数：每次 upsert/remove 自增，SSE 端按变化推送而非空转。
    revision: AtomicU64,
}

impl LiveRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&self, snapshot: LiveDeviceSnapshot) {
        self.devices.insert(snapshot.device_id, Arc::new(snapshot));
        self.revision.fetch_add(1, Ordering::Relaxed);
    }

    /// 当前写入代数（只增不减，消费方比对前后值判断是否有更新）。
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    pub fn get(&self, device_id: Uuid) -> Option<Arc<LiveDeviceSnapshot>> {
        self.devices
            .get(&device_id)
            .map(|entry| entry.value().clone())
    }

    /// 当前全部快照（按设备 ID 无序）。
    pub fn all(&self) -> Vec<Arc<LiveDeviceSnapshot>> {
        self.devices
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    pub fn remove(&self, device_id: Uuid) {
        if self.devices.remove(&device_id).is_some() {
            self.revision.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }
}

/// 启动 SNMP 实时采集（`enabled = false` 时不启动）。
///
/// `registry` 由调用方持有用于后续查询；本函数启动的常驻 supervisor 与
/// 各设备 worker 也各持一份引用，调用方丢弃句柄不影响采集。
/// `alerts` 为告警 episode 运行时：设备每轮成功采样后就地评估 cpu/内存/温度
/// （磁盘只有 5 分钟全量轮询有读数，交由兜底扫描维护）。
pub fn start_live_poller(
    pool: PgPool,
    config: SnmpLiveConfig,
    registry: Arc<LiveRegistry>,
    alerts: Arc<AlertRuntime>,
    shutdown: ShutdownReceiver<()>,
) {
    if !config.enabled {
        return;
    }
    tokio::spawn(run_supervisor(pool, config, registry, alerts, shutdown));
}

/// 事件循环状态：跨轮保留的会话、profile 与采样节拍。
struct WorkerState {
    client: Option<Client<UdpHandle>>,
    collector: Option<LiveCollector>,
    last_probe: Option<Instant>,
    endpoint: String,
    timeout: Duration,
}

/// 单个设备 worker 的不可变上下文。
struct WorkerContext {
    device_id: Uuid,
    /// 告警 episode 主体（`monitored_hosts.id`），与设备 id 不同源
    host_id: Uuid,
    device_name: String,
    ip: String,
    /// 实时流量采集端口名（未配置时采集器回落首端口）
    monitor_if: Option<String>,
    params: SnmpParamsLegacy,
    config: SnmpLiveConfig,
    registry: Arc<LiveRegistry>,
    alerts: Arc<AlertRuntime>,
    semaphore: Arc<Semaphore>,
    initial_delay: Duration,
}

/// 已启动 worker 的句柄：凭据/地址变化时用于比对与重启。
struct WorkerHandle {
    params: SnmpParamsLegacy,
    monitor_if: Option<String>,
    task: JoinHandle<()>,
}

/// supervisor：周期重载设备清单并按差异增删/重启 worker。
async fn run_supervisor(
    pool: PgPool,
    config: SnmpLiveConfig,
    registry: Arc<LiveRegistry>,
    alerts: Arc<AlertRuntime>,
    mut shutdown: ShutdownReceiver<()>,
) {
    let semaphore = Arc::new(Semaphore::new(config.max_concurrency.max(1)));
    let mut workers: HashMap<Uuid, WorkerHandle> = HashMap::new();
    let mut started = false;

    loop {
        match load_snmp_devices(&pool).await {
            Ok(rows) => {
                reconcile(
                    &pool,
                    &rows,
                    &config,
                    &registry,
                    &alerts,
                    &semaphore,
                    &mut workers,
                )
                .await;
                if !started {
                    started = true;
                    foims_common::log_info!(
                        "system.snmp_live_started",
                        devices = workers.len(),
                        interval = config.interval_secs
                    );
                }
            }
            Err(e) => {
                foims_common::log_error!("system.snmp_live_devices_load_failed", error = e);
            }
        }

        tokio::select! {
            _ = shutdown.recv() => break,
            _ = tokio::time::sleep(Duration::from_secs(DEVICE_RELOAD_SECS)) => {}
        }
    }

    for (_, worker) in workers.drain() {
        worker.task.abort();
    }
    foims_common::log_info!("system.snmp_live_stopped");
}

/// 对比数据库设备清单与运行中的 worker：新增/变更重启，删除清理。
async fn reconcile(
    pool: &PgPool,
    rows: &[SnmpDeviceRow],
    config: &SnmpLiveConfig,
    registry: &Arc<LiveRegistry>,
    alerts: &Arc<AlertRuntime>,
    semaphore: &Arc<Semaphore>,
    workers: &mut HashMap<Uuid, WorkerHandle>,
) {
    let total = rows.len();
    let mut seen = HashSet::with_capacity(total);

    for (index, row) in rows.iter().enumerate() {
        let device_id = row.device.id;
        seen.insert(device_id);

        // 告警 episode 以监控主机行为主体：基线行缺失时补建并即刻回填 id，
        // 使新配置 SNMP 的设备首轮就进入采集与告警，不必等 5 分钟轮询
        let host_id = match row.host_id {
            Some(id) => id,
            None => {
                ensure_snmp_host_present(
                    pool,
                    &snmp_machine_id(device_id),
                    device_id,
                    &row.device.name,
                    &row.ip,
                )
                .await;
                match load_snmp_host_id(pool, device_id).await {
                    Some(id) => id,
                    None => continue,
                }
            }
        };

        // 参数构造含凭据解密：失败时保留旧 worker（凭据可能只是暂时不可解密），
        // 从未启动过的设备等下一轮重载再试
        let params = match row.device.to_snmp_params_async(&row.ip).await {
            Ok(params) => params,
            Err(e) => {
                tracing::warn!(
                    "SNMP 实时采集参数构造失败: 设备 {}（{device_id}）: {e}",
                    row.device.name
                );
                continue;
            }
        };

        if workers
            .get(&device_id)
            .is_some_and(|worker| worker.params == params && worker.monitor_if == row.monitor_if)
        {
            continue;
        }
        if let Some(worker) = workers.remove(&device_id) {
            worker.task.abort();
        }

        let context = WorkerContext {
            device_id,
            host_id,
            device_name: row.device.name.clone(),
            ip: row.ip.clone(),
            monitor_if: row.monitor_if.clone(),
            params: params.clone(),
            config: config.clone(),
            registry: registry.clone(),
            alerts: alerts.clone(),
            semaphore: semaphore.clone(),
            initial_delay: stagger_delay(config.interval_secs, index, total),
        };
        workers.insert(
            device_id,
            WorkerHandle {
                params,
                monitor_if: row.monitor_if.clone(),
                task: tokio::spawn(run_device_worker(context)),
            },
        );
    }

    workers.retain(|device_id, worker| {
        if seen.contains(device_id) {
            true
        } else {
            worker.task.abort();
            registry.remove(*device_id);
            false
        }
    });
}

/// 错峰延迟：把 `total` 台设备的首轮采集均匀铺在 `interval_secs` 内。
fn stagger_delay(interval_secs: u64, index: usize, total: usize) -> Duration {
    if total <= 1 || index == 0 {
        return Duration::ZERO;
    }
    let slot_millis = interval_secs.max(1).saturating_mul(1000) / total as u64;
    Duration::from_millis(slot_millis.saturating_mul(index as u64))
}

/// 单设备常驻采集循环：每轮先取全局许可，结束后按结果正常间隔或退避休眠。
///
/// 关闭由 supervisor 统一 abort（sleep/probe/sample 任意点被丢弃都安全）。
async fn run_device_worker(context: WorkerContext) {
    tokio::time::sleep(context.initial_delay).await;

    let interval = Duration::from_secs(context.config.interval_secs.max(1));
    let timeout = Duration::from_secs(context.config.timeout_secs.max(1));
    let probe_interval = Duration::from_secs(
        context
            .config
            .inventory_refresh_secs
            .max(context.config.interval_secs.max(1)),
    );
    let max_backoff = Duration::from_secs(context.config.max_backoff_secs.max(interval.as_secs()));

    let mut state = WorkerState {
        client: None,
        collector: None,
        last_probe: None,
        endpoint: snmp_target(&context.ip, context.params.port),
        timeout,
    };
    let mut backoff = interval;

    loop {
        let cycle_start = Instant::now();
        // 索引刷新：网络重建/堆叠变化后 ifIndex 可能重排，重新探测一次
        if state
            .last_probe
            .is_some_and(|at| at.elapsed() >= probe_interval)
        {
            state.collector = None;
        }

        let permit = match context.semaphore.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => return,
        };
        let ok = run_cycle(&context, &mut state).await;
        drop(permit);

        let wait = if ok {
            backoff = interval;
            interval
        } else {
            let wait = backoff;
            backoff = backoff.saturating_mul(2).min(max_backoff);
            wait
        };
        tokio::time::sleep(wait.saturating_sub(cycle_start.elapsed())).await;
    }
}

/// 执行一轮：按需建连/探测，再采样并写入注册表；返回本轮是否成功。
async fn run_cycle(context: &WorkerContext, state: &mut WorkerState) -> bool {
    if state.client.is_none() {
        match connect_live(&state.endpoint, &context.params, state.timeout).await {
            Ok(client) => {
                tracing::debug!(
                    "SNMP 实时会话已建立: 设备 {}（{}，{}）",
                    context.device_name,
                    context.device_id,
                    context.ip
                );
                state.client = Some(client);
            }
            Err(e) => {
                tracing::debug!(
                    "SNMP 实时会话建立失败: 设备 {}（{}，{}）: {e}",
                    context.device_name,
                    context.device_id,
                    context.ip
                );
                return false;
            }
        }
    }
    let Some(client) = state.client.as_ref() else {
        return false;
    };

    if state.collector.is_none() {
        match probe(client).await {
            Ok(profile) => {
                tracing::debug!(
                    "SNMP 实时 profile 探测完成: 设备 {}（{}）: 厂商 {}, 端口 {}, 内存 {:?}, 温度源 {}, HC={}",
                    context.device_name,
                    context.device_id,
                    profile.vendor.as_str(),
                    profile.ports.len(),
                    profile.mem,
                    profile.temp.len(),
                    profile.use_hc
                );
                state.collector = Some(LiveCollector::new(profile, context.monitor_if.as_deref()));
                state.last_probe = Some(Instant::now());
            }
            Err(e) => {
                tracing::debug!(
                    "SNMP 实时 profile 探测失败: 设备 {}（{}，{}）: {e}",
                    context.device_name,
                    context.device_id,
                    context.ip
                );
                return false;
            }
        }
    }

    let Some(collector) = state.collector.as_mut() else {
        return false;
    };
    let snapshot = collector.sample(client, Utc::now()).await;

    if !snapshot.responded {
        tracing::debug!(
            "SNMP 实时采集无响应: 设备 {}（{}，{}）",
            context.device_name,
            context.device_id,
            context.ip
        );
        // 丢弃会话：下一轮重连，重新走 v3 引擎发现
        state.client = None;
        return false;
    }
    // 端口状态全缺说明探测期的 ifIndex 已失效，下一轮重建 profile
    if !snapshot.ports.is_empty()
        && snapshot
            .ports
            .iter()
            .all(|port| port.admin_up.is_none() && port.oper_up.is_none())
    {
        tracing::debug!(
            "SNMP 实时端口状态全部缺失，下一轮重新探测: 设备 {}（{}）",
            context.device_name,
            context.device_id
        );
        state.collector = None;
    }

    let cpu_pct = snapshot.cpu_usage_pct;
    let mem_pct = snapshot.mem_usage_pct;
    let temp_c = snapshot.temperature_c;
    context.registry.upsert(LiveDeviceSnapshot {
        device_id: context.device_id,
        device_name: context.device_name.clone(),
        ip: context.ip.clone(),
        snapshot,
    });

    // 告警评估：秒级读数进 episode 运行时（磁盘指标实时路径无读数，置 None
    // 保持既有状态，由兜底扫描按 5 分钟快照维护）；失败在运行时内部记日志
    context
        .alerts
        .evaluate_host(
            &HostRef {
                id: context.host_id,
                hostname: context.device_name.clone(),
                ip: context.ip.clone(),
            },
            MetricValues {
                cpu_pct,
                mem_pct,
                disk_pct: None,
                temp_c,
            },
        )
        .await;
    true
}

/// 建立实时采集会话：关闭内建重试，超时取实时配置（默认 2s），
/// 单请求 OID 上限与采集器分块口径一致（IPv6 载荷安全）。
async fn connect_live(
    endpoint: &str,
    params: &SnmpParamsLegacy,
    timeout: Duration,
) -> Result<Client<UdpHandle>, String> {
    let auth = build_auth(params)?;
    Client::builder(endpoint, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .retry(Retry::none())
        .max_oids_per_request(MAX_OIDS_PER_REQUEST)
        .connect()
        .await
        .map_err(|e| format!("创建SNMP实时会话失败: {}", format_snmp_error(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use foims_snmp::live::VendorKind;

    fn snapshot(device_id: Uuid) -> LiveDeviceSnapshot {
        LiveDeviceSnapshot {
            device_id,
            device_name: "sw-test".to_string(),
            ip: "192.0.2.1".to_string(),
            snapshot: LiveSnapshot {
                collected_at: Utc::now(),
                vendor: VendorKind::Generic,
                cpu_usage_pct: None,
                mem_total: None,
                mem_used: None,
                mem_usage_pct: None,
                temperature_c: None,
                uptime_secs: Some(100),
                monitor_if: None,
                ports: Vec::new(),
                rx_bps: None,
                tx_bps: None,
                responded: true,
                partial: false,
            },
        }
    }

    #[test]
    fn 错峰延迟_均布且不超间隔() {
        assert_eq!(stagger_delay(5, 0, 500), Duration::ZERO);
        assert_eq!(stagger_delay(5, 250, 500), Duration::from_millis(2500));
        assert_eq!(stagger_delay(5, 499, 500), Duration::from_millis(4990));
        assert_eq!(stagger_delay(5, 0, 1), Duration::ZERO);
    }

    #[test]
    fn 注册表_写入读取与删除() {
        let registry = LiveRegistry::new();
        let id = Uuid::new_v4();
        assert!(registry.is_empty());
        registry.upsert(snapshot(id));
        assert_eq!(registry.len(), 1);
        let entry = registry.get(id).expect("刚写入的快照应可读");
        assert_eq!(entry.device_name, "sw-test");
        assert_eq!(entry.age_secs(Utc::now()), 0);
        assert_eq!(registry.all().len(), 1);
        registry.remove(id);
        assert!(registry.is_empty());
        assert!(registry.get(id).is_none());
    }
}
