//! 告警 episode 运行时：把「主机 + 指标」的一次持续超标建模为一个 episode，
//! 用持续时长（for_secs）、迟滞带、静默窗口、恢复冷却四个参数抑制抖动与刷屏。
//!
//! 与 [`crate::alerts`] 的分工：alerts 负责阈值配置的读写与校验（配置面），
//! 本模块负责评估、状态持久化与通知（数据面）。三条评估路径共用同一个
//! [`AlertRuntime`] 实例，因此同一故障不会因为多条路径各判各的而重复告警：
//! - SNMP 实时采集（秒级，见 [`crate::live_poll`]）；
//! - agent 上报入库（见 [`crate::ingest`]）；
//! - 常驻兜底扫描（[`AlertRuntime::sweep_once`]，读 monitored_hosts 快照列，
//!   负责 agent 停止上报后的复位；实时采集未开启时兼管 SNMP 主机）。
//!
//! 状态机（纯函数 [`Episode::step`]，无 IO，便于单测）：
//!
//! ```text
//! 无 ─超标─▶ Pending ─for_secs 到且冷却已过─▶ Active ─读数跌破复位水位─▶ Resolved
//!              │                               │
//!              ├─回落─▶ 消散（从不通知）        ├─静默窗口内再超标─▶ 就地累计通知文案
//!              │                               ├─静默窗口外再超标─▶ 重发一条新通知
//!              └─（恢复后再超标→ Pending）      └─迟滞带内─▶ 保持不动
//! ```
//!
//! 内存态是工作副本，`host_alert_episodes` 是事实源：状态跃迁与通知发送立即
//! 落盘，只有计数变化标记 dirty 由 [`AlertRuntime::start_sweeper`] 周期刷盘；
//! 进程重启由 [`AlertRuntime::hydrate`] 恢复既有 episode，避免同一故障重开告警。

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::{RwLock, broadcast::Receiver as ShutdownReceiver};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::alerts::{
    self, AlertThresholds, MetricValues, NOTIFICATION_TITLE_KEY, NOTIFICATION_TYPE,
};

/// 告警主体：`monitored_hosts.id` + 指标名（与 host_alert_episodes 主键同构）
type EpisodeKey = (Uuid, &'static str);

/// 策略缓存有效期（秒）：收件人与阈值在此期间内复用，配置保存会主动失效
const POLICY_TTL_SECS: i64 = 300;
/// dirty episode 刷盘周期（秒）
const FLUSH_INTERVAL_SECS: u64 = 60;
/// 兜底扫描周期（秒）
const SWEEP_INTERVAL_SECS: u64 = 300;
/// resolved 记录保留时长（秒）：超期即从库与内存回收
const RESOLVED_KEEP_SECS: i64 = 3_600;

/// 兜底扫描取数（仅 agent 上报主机）：SNMP 主机由秒级实时路径评估
const SWEEP_SQL_AGENT: &str = "SELECT id, hostname, ip, cpu_usage::float8 AS cpu_pct, \
     mem_usage_pct::float8 AS mem_pct, disk_usage_pct::float8 AS disk_pct, \
     max_temp::float8 AS temp_c \
     FROM monitored_hosts WHERE source = 'agent' AND status = 'active'";
/// 兜底扫描取数（agent + SNMP）：实时采集未开启时 SNMP 只有 5 分钟快照列可读
const SWEEP_SQL_BOTH: &str = "SELECT id, hostname, ip, cpu_usage::float8 AS cpu_pct, \
     mem_usage_pct::float8 AS mem_pct, disk_usage_pct::float8 AS disk_pct, \
     max_temp::float8 AS temp_c \
     FROM monitored_hosts WHERE source IN ('agent', 'snmp') AND status = 'active'";
/// 活动主机全集（episode 清理口径）：覆盖实时路径与兜底扫描两侧的主机
const ACTIVE_HOST_SQL: &str = "SELECT id FROM monitored_hosts WHERE status = 'active'";

/// episode 生命周期状态（host_alert_episodes.status 列）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpisodeStatus {
    /// 已观测到超标，尚未满足持续时长，未发通知
    Pending,
    /// 已开告警，正在窗口内累计或窗口外重发
    Active,
    /// 已恢复（读数跌破复位水位），等待冷却与回收
    Resolved,
}

impl EpisodeStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Resolved => "resolved",
        }
    }

    /// 解析库中状态值；未知值（历史脏数据）返回 None 由调用方跳过
    fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "active" => Some(Self::Active),
            "resolved" => Some(Self::Resolved),
            _ => None,
        }
    }
}

/// 单次读数相对阈值的判定档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    /// 严格大于阈值
    Over,
    /// 落在复位水位与阈值之间（抖动抑制带）
    Band,
    /// 跌破复位水位
    Under,
}

/// 复位水位：阈值按迟滞带比例下移后的取值，读数需低于它才算真正回落。
fn recover_level(threshold: u8, hysteresis_pct: u8) -> f64 {
    f64::from(threshold) * (100.0 - f64::from(hysteresis_pct)) / 100.0
}

fn level_of(value: f64, threshold: u8, hysteresis_pct: u8) -> Level {
    if value > f64::from(threshold) {
        Level::Over
    } else if value <= recover_level(threshold, hysteresis_pct) {
        Level::Under
    } else {
        Level::Band
    }
}

/// 抖动抑制窗口参数（取自 [`AlertThresholds`]，单独成类型便于状态机测试）。
#[derive(Debug, Clone, Copy)]
pub struct AlertWindows {
    pub for_secs: u64,
    pub quiet_secs: u64,
    pub cooldown_secs: u64,
    pub hysteresis_pct: u8,
}

impl AlertWindows {
    fn from_thresholds(t: &AlertThresholds) -> Self {
        Self {
            for_secs: t.for_secs,
            quiet_secs: t.quiet_window_secs,
            cooldown_secs: t.cooldown_secs,
            hysteresis_pct: t.hysteresis_pct,
        }
    }
}

/// [`Episode::step`] 要求调用方执行的副作用。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Effects {
    /// 状态跃迁（或阈值变化）：立即 upsert 行
    persist: bool,
    /// 未开过告警的 Pending 消散：删除行与内存条目
    delete: bool,
    /// 静默窗口内再次超标：就地刷新既有通知文案（计数随周期刷盘）
    refresh: bool,
    /// 开告警：新建通知
    notify_open: bool,
    /// 静默窗口外仍在超标：重发一条新通知并替换累计目标
    renotify: bool,
    /// 恢复：新建一条恢复通知
    notify_resolved: bool,
}

impl Effects {
    fn is_none(&self) -> bool {
        *self == Self::default()
    }
}

/// 单个「主机 + 指标」的告警 episode（内存工作副本）。
#[derive(Debug, Clone)]
pub struct Episode {
    pub host_id: Uuid,
    pub metric: &'static str,
    pub status: EpisodeStatus,
    /// 本轮超标起点（for_secs 计时基准）
    pub started_at: DateTime<Utc>,
    /// 最近一次开告警时刻
    pub opened_at: Option<DateTime<Utc>>,
    /// 最近一次恢复时刻（冷却判定基准，开新告警不清空）
    pub resolved_at: Option<DateTime<Utc>>,
    /// 最近一次超标观测时刻
    pub last_exceed_at: DateTime<Utc>,
    /// 最近一次发出通知时刻（静默窗口计时基准）
    pub last_notified_at: Option<DateTime<Utc>>,
    /// 最近一次读数
    pub last_value: f64,
    /// 最近一次评估生效的阈值（与当前配置不一致时立即改写并落盘）
    pub threshold: u8,
    /// 本 episode 内累计的超标观测次数
    pub exceed_count: i64,
    /// 当前累计通知的行 ID（就地刷新文案用；恢复后清空）
    pub notify_ids: Vec<Uuid>,
    /// 行是否已落库（未落库的 Pending 消散无需 DELETE，省掉无效写）
    pub persisted: bool,
    /// 计数/时间变化待刷盘
    pub dirty: bool,
}

impl Episode {
    /// 新建零观测 episode：尚未确认超标前的占位，[`Episode::is_placeholder`]
    /// 为真时调用方不保留它，也不产生任何写库与通知。
    fn pristine(host_id: Uuid, metric: &'static str, threshold: u8, now: DateTime<Utc>) -> Self {
        Self {
            host_id,
            metric,
            status: EpisodeStatus::Pending,
            started_at: now,
            opened_at: None,
            resolved_at: None,
            last_exceed_at: now,
            last_notified_at: None,
            last_value: 0.0,
            threshold,
            exceed_count: 0,
            notify_ids: Vec::new(),
            persisted: false,
            dirty: false,
        }
    }

    /// 从未观测到过超标的空壳（不落内存、不写库、不通知）
    fn is_placeholder(&self) -> bool {
        self.status == EpisodeStatus::Pending && self.exceed_count == 0 && !self.persisted
    }

    /// Pending → Active 的两道门：持续时长已满足，且距上次恢复已过冷却。
    /// 冷却未过时停在 Pending 计时，冷却一到（下一次超标观测）即开。
    fn promotable(&self, now: DateTime<Utc>, windows: &AlertWindows) -> bool {
        let sustained = (now - self.started_at).num_seconds() >= windows.for_secs as i64;
        let cooled = self
            .resolved_at
            .is_none_or(|at| (now - at).num_seconds() >= windows.cooldown_secs as i64);
        sustained && cooled
    }

    /// 开告警：只改状态，通知时刻与行 ID 由调用方在发送成功后回填
    /// （发送失败保持未通知态，下一轮观测自动重试，宁可重复不遗漏）。
    fn open(&mut self, now: DateTime<Utc>) {
        self.status = EpisodeStatus::Active;
        self.opened_at = Some(now);
    }

    /// 恢复：进入 Resolved 并记录时刻；累计通知行保留原文案不再更新。
    fn resolve(&mut self, now: DateTime<Utc>) {
        self.status = EpisodeStatus::Resolved;
        self.resolved_at = Some(now);
    }

    /// 纯状态推进一步：按本次读数更新内存态并返回需要执行的副作用。
    ///
    /// `value` 为 None（本次无读数）时保持原状态——实时路径与兜底扫描覆盖的
    /// 指标不同，任一路径缺读数都不构成「恢复」证据，否则两条路径会互相打掉
    /// 对方的告警。
    pub fn step(
        &mut self,
        value: Option<f64>,
        threshold: u8,
        windows: &AlertWindows,
        now: DateTime<Utc>,
    ) -> Effects {
        let mut effects = Effects::default();
        if self.threshold != threshold {
            self.threshold = threshold;
            effects.persist = true;
        }
        let Some(value) = value else {
            return effects;
        };
        let level = level_of(value, threshold, windows.hysteresis_pct);

        match self.status {
            EpisodeStatus::Pending => match level {
                Level::Over => {
                    self.exceed_count += 1;
                    self.last_value = value;
                    self.last_exceed_at = now;
                    self.dirty = true;
                    if self.promotable(now, windows) {
                        self.open(now);
                        effects.notify_open = true;
                    }
                }
                // 尚未升级为告警：读数回落即消散，不打扰任何人
                Level::Band | Level::Under => effects.delete = true,
            },
            EpisodeStatus::Active => match level {
                Level::Over => {
                    self.exceed_count += 1;
                    self.last_value = value;
                    self.last_exceed_at = now;
                    self.dirty = true;
                    // 通知行缺失（上次发送失败）时按窗口已过处理，直接重发
                    match self.last_notified_at {
                        Some(at)
                            if (now - at).num_seconds() < windows.quiet_secs as i64
                                && !self.notify_ids.is_empty() =>
                        {
                            effects.refresh = true
                        }
                        _ => effects.renotify = true,
                    }
                }
                // 迟滞带内：既不累计也不复位，避免贴着阈值抖动反复开合
                Level::Band => {}
                Level::Under => {
                    self.resolve(now);
                    effects.notify_resolved = true;
                }
            },
            EpisodeStatus::Resolved => {
                if level != Level::Over {
                    return effects;
                }
                // 恢复后再次超标：开新 episode（计数清零），保留 resolved_at
                // 供冷却门使用
                self.status = EpisodeStatus::Pending;
                self.started_at = now;
                self.opened_at = None;
                self.last_notified_at = None;
                self.notify_ids.clear();
                self.exceed_count = 1;
                self.last_value = value;
                self.last_exceed_at = now;
                effects.persist = true;
                if self.promotable(now, windows) {
                    self.open(now);
                    effects.notify_open = true;
                }
            }
        }
        effects
    }
}

/// 被评估主机的标识（通知文案与日志用）。
#[derive(Debug, Clone)]
pub struct HostRef {
    /// monitored_hosts.id
    pub id: Uuid,
    pub hostname: String,
    pub ip: String,
}

/// 单指标评估的输入（聚合成结构体，避免长参数列表）。
struct MetricEvaluation<'a> {
    host: &'a HostRef,
    metric: &'static str,
    threshold: u8,
    value: Option<f64>,
    /// 开告警正文的 i18n key（静默窗口内的就地累计复用同一条文案）
    body_key: &'static str,
    /// 恢复正文的 i18n key（与 body_key 同指标成对）
    resolved_key: &'static str,
    now: DateTime<Utc>,
}

/// 缓存的评估策略：全局阈值 + 通知收件人（一轮评估内复用，避免逐台查库）。
#[derive(Debug, Clone)]
struct CachedPolicy {
    thresholds: AlertThresholds,
    recipients: Vec<Uuid>,
    loaded_at: DateTime<Utc>,
}

/// 邮件参数（仅开告警时发送，窗口内累计与重发不再发信，避免邮箱刷屏）。
struct EmailAlert<'a> {
    metric: &'a str,
    value: f64,
    threshold: u8,
}

/// 兜底扫描读取的主机快照行（列口径与实时路径的指标一致，None = 无值）。
#[derive(Debug, Clone, sqlx::FromRow)]
struct SweepHost {
    id: Uuid,
    hostname: Option<String>,
    ip: Option<String>,
    cpu_pct: Option<f64>,
    mem_pct: Option<f64>,
    disk_pct: Option<f64>,
    temp_c: Option<f64>,
}

impl SweepHost {
    fn reference(&self) -> HostRef {
        HostRef {
            id: self.id,
            hostname: self.hostname.clone().unwrap_or_else(|| "-".to_string()),
            ip: self.ip.clone().unwrap_or_else(|| "-".to_string()),
        }
    }

    fn values(&self) -> MetricValues {
        MetricValues {
            cpu_pct: self.cpu_pct,
            mem_pct: self.mem_pct,
            disk_pct: self.disk_pct,
            temp_c: self.temp_c,
        }
    }
}

/// host_alert_episodes 行映射（时间列可空处即「该阶段未发生」）。
#[derive(Debug, Clone, sqlx::FromRow)]
struct EpisodeRow {
    host_id: Uuid,
    metric: String,
    status: String,
    started_at: DateTime<Utc>,
    opened_at: Option<DateTime<Utc>>,
    resolved_at: Option<DateTime<Utc>>,
    last_exceed_at: DateTime<Utc>,
    last_notified_at: Option<DateTime<Utc>>,
    last_value: Option<f64>,
    threshold: i16,
    exceed_count: i64,
    notify_ids: String,
}

impl EpisodeRow {
    /// 行 → episode；指标名不在白名单、状态未知或阈值越界（历史脏数据）
    /// 时返回 None 跳过。
    fn into_episode(self) -> Option<Episode> {
        let metric = alerts::metric_of(&self.metric)?;
        let status = EpisodeStatus::parse(&self.status)?;
        let threshold = u8::try_from(self.threshold).ok()?;
        Some(Episode {
            host_id: self.host_id,
            metric,
            status,
            started_at: self.started_at,
            opened_at: self.opened_at,
            resolved_at: self.resolved_at,
            last_exceed_at: self.last_exceed_at,
            last_notified_at: self.last_notified_at,
            last_value: self.last_value.unwrap_or_default(),
            threshold,
            exceed_count: self.exceed_count,
            notify_ids: split_ids(&self.notify_ids),
            persisted: true,
            dirty: false,
        })
    }
}

/// 解析 notify_ids 列（逗号分隔 UUID 串，非法项忽略）。
fn split_ids(raw: &str) -> Vec<Uuid> {
    raw.split(',')
        .filter(|item| !item.is_empty())
        .filter_map(|item| Uuid::parse_str(item).ok())
        .collect()
}

/// 通知正文：指标专属文案 key + 主机标识与本时刻观测值。
///
/// 开告警、重发与静默窗口内的就地累计共用本函数（同一文案，只刷新数值与
/// 累计次数），恢复用同指标的 `resolved_key`。
fn alert_content(host: &HostRef, episode: &Episode, body_key: &str) -> String {
    json!({
        "key": body_key,
        "params": {
            "hostname": host.hostname,
            "ip": host.ip,
            "value": format!("{:.1}", episode.last_value),
            "threshold": episode.threshold.to_string(),
            "count": episode.exceed_count.to_string(),
        }
    })
    .to_string()
}

/// 读取告警运行时的 AppState 能力（避免 agent-service 反向依赖主 crate）。
///
/// 返回 None 表示本进程未启用告警评估（初始化模式无连接池），调用方据此跳过
/// 评估与配置补扫。
pub trait AlertProvider {
    fn alert_runtime(&self) -> Option<Arc<AlertRuntime>>;
}

/// 告警评估运行时：阈值缓存 + 内存 episode 表 + episode 持久化与通知。
///
/// 进程内单实例（由 main.rs 构造，经 [`AlertProvider`] 与各采集上下文共享）。
/// 初始化模式无数据库连接池时不构造，评估路径缺席。
pub struct AlertRuntime {
    pool: PgPool,
    /// 已评估的 episode（含 Pending/Resolved，规模受告警数约束）
    episodes: DashMap<EpisodeKey, Episode>,
    policy: RwLock<Option<CachedPolicy>>,
    /// 配置保存后置位：下一轮评估强制重载阈值与收件人
    policy_stale: AtomicBool,
    /// SNMP 主机是否由实时路径负责评估：为真时兜底扫描跳过 source='snmp' 行，
    /// 避免 5 分钟前的快照读数把实时路径刚恢复的告警重新拖回 Pending
    live_covers_snmp: bool,
}

impl AlertRuntime {
    pub fn new(pool: PgPool, live_covers_snmp: bool) -> Self {
        Self {
            pool,
            episodes: DashMap::new(),
            policy: RwLock::new(None),
            policy_stale: AtomicBool::new(true),
            live_covers_snmp,
        }
    }

    /// 阈值配置变更：作废缓存，下一轮评估重新读取。
    pub fn invalidate_policy(&self) {
        self.policy_stale.store(true, Ordering::Relaxed);
    }

    /// 当前内存中的 episode 快照（按主机/指标无序；供 API 与测试观察）。
    pub fn episodes_snapshot(&self) -> Vec<Episode> {
        self.episodes
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// 启动时恢复既有 episode（进程重启不重复告警）。失败仅记日志：
    /// 内存态为空时最坏结果是一次重复通知，不影响采集与展示主流程。
    pub async fn hydrate(&self) {
        let rows = sqlx::query_as::<_, EpisodeRow>(
            "SELECT host_id, metric, status, started_at, opened_at, resolved_at, \
                    last_exceed_at, last_notified_at, last_value, threshold, \
                    exceed_count, notify_ids \
             FROM host_alert_episodes",
        )
        .fetch_all(&self.pool)
        .await;
        let rows = match rows {
            Ok(rows) => rows,
            Err(e) => {
                foims_common::log_error!("log.agent.episode_hydrate_failed", error = e);
                return;
            }
        };
        let mut loaded = 0usize;
        for row in rows {
            let Some(episode) = row.into_episode() else {
                continue;
            };
            self.episodes
                .insert((episode.host_id, episode.metric), episode);
            loaded += 1;
        }
        if loaded > 0 {
            foims_common::log_info!("log.agent.episode_hydrated", episodes = loaded);
        }
    }

    /// 取评估策略：缓存有效期内直接复用；阈值或收件人查询失败返回 None
    /// （收件人查不到就不评估，避免发出无人可见的通知后把告警记成已通知）。
    async fn policy(&self) -> Option<CachedPolicy> {
        let stale = self.policy_stale.load(Ordering::Relaxed);
        {
            let cached = self.policy.read().await;
            if let Some(policy) = cached.as_ref()
                && !stale
                && (Utc::now() - policy.loaded_at).num_seconds() < POLICY_TTL_SECS
            {
                return Some(policy.clone());
            }
        }
        let thresholds = alerts::load_thresholds(&self.pool).await;
        let recipients = match alerts::admin_recipient_ids(&self.pool).await {
            Ok(ids) => ids,
            Err(e) => {
                foims_common::log_error!("log.agent.alert_recipients_query_failed", error = e);
                return None;
            }
        };
        let policy = CachedPolicy {
            thresholds,
            recipients,
            loaded_at: Utc::now(),
        };
        *self.policy.write().await = Some(policy.clone());
        self.policy_stale.store(false, Ordering::Relaxed);
        Some(policy)
    }

    /// 单台主机一轮评估（实时与上报路径入口）。
    pub async fn evaluate_host(&self, host: &HostRef, values: MetricValues) {
        let Some(policy) = self.policy().await else {
            return;
        };
        if !policy.thresholds.enabled {
            return;
        }
        self.evaluate_with_policy(host, values, &policy).await;
    }

    async fn evaluate_with_policy(
        &self,
        host: &HostRef,
        values: MetricValues,
        policy: &CachedPolicy,
    ) {
        let windows = AlertWindows::from_thresholds(&policy.thresholds);
        let now = Utc::now();
        for spec in alerts::metric_specs(&policy.thresholds, values) {
            let Some(threshold) = spec.threshold else {
                continue;
            };
            let evaluation = MetricEvaluation {
                host,
                metric: spec.metric,
                threshold,
                value: spec.value,
                body_key: spec.body_key,
                resolved_key: spec.resolved_key,
                now,
            };
            self.evaluate_metric(&evaluation, &windows, policy).await;
        }
    }

    /// 单指标评估：先在内存里推进状态（无副作用时不留痕），再执行副作用。
    async fn evaluate_metric(
        &self,
        ev: &MetricEvaluation<'_>,
        windows: &AlertWindows,
        policy: &CachedPolicy,
    ) {
        let key = (ev.host.id, ev.metric);
        // 常见路径快进：没有进行中的 episode 且本次未超标，不建条目也不写库
        let exceeding = ev.value.is_some_and(|v| v > f64::from(ev.threshold));
        if !exceeding && !self.episodes.contains_key(&key) {
            return;
        }

        let (mut episode, effects) = match self.episodes.get_mut(&key) {
            Some(mut existing) => {
                let mut episode = existing.clone();
                let effects = episode.step(ev.value, ev.threshold, windows, ev.now);
                if episode.is_placeholder() {
                    return;
                }
                *existing = episode.clone();
                (episode, effects)
            }
            None => {
                let mut episode = Episode::pristine(ev.host.id, ev.metric, ev.threshold, ev.now);
                let effects = episode.step(ev.value, ev.threshold, windows, ev.now);
                if episode.is_placeholder() {
                    return;
                }
                self.episodes.insert(key, episode.clone());
                (episode, effects)
            }
        };

        if effects.delete {
            self.episodes.remove(&key);
            if episode.persisted
                && let Err(e) = self.delete_row(ev.host.id, ev.metric).await
            {
                foims_common::log_error!("log.agent.episode_delete_failed", error = e);
            }
            return;
        }
        if effects.is_none() {
            return;
        }
        self.apply_effects(&mut episode, effects, ev, &policy.recipients)
            .await;
        self.episodes.insert(key, episode);
    }

    /// 执行 [`Effects`]：发通知 + 落盘。任一步失败只记日志并保持「未通知」态，
    /// 由下一轮观测重试（宁可重复，不遗漏告警）。
    async fn apply_effects(
        &self,
        episode: &mut Episode,
        effects: Effects,
        ev: &MetricEvaluation<'_>,
        recipients: &[Uuid],
    ) {
        if effects.notify_open || effects.renotify {
            let content = alert_content(ev.host, episode, ev.body_key);
            let email = effects.notify_open.then_some(EmailAlert {
                metric: episode.metric,
                value: episode.last_value,
                threshold: episode.threshold,
            });
            match self
                .notify(ev.host, episode, &content, email, recipients)
                .await
            {
                Ok(ids) => {
                    episode.notify_ids = ids;
                    episode.last_notified_at = Some(ev.now);
                }
                Err(e) => foims_common::log_error!("log.agent.alert_notify_failed", error = e),
            }
            episode.persisted = true;
            if let Err(e) = self.upsert(episode).await {
                foims_common::log_error!("log.agent.episode_persist_failed", error = e);
            }
            return;
        }

        if effects.notify_resolved {
            let content = alert_content(ev.host, episode, ev.resolved_key);
            match self
                .notify(ev.host, episode, &content, None, recipients)
                .await
            {
                Ok(_) => episode.notify_ids.clear(),
                Err(e) => foims_common::log_error!("log.agent.alert_notify_failed", error = e),
            }
            if let Err(e) = self.upsert(episode).await {
                foims_common::log_error!("log.agent.episode_persist_failed", error = e);
            }
            return;
        }

        if effects.refresh {
            // 静默窗口内不重发：同一批通知行就地刷新，文案不变、只更新数值与累计次数
            let content = alert_content(ev.host, episode, ev.body_key);
            if let Err(e) = self
                .refresh_notifications(&episode.notify_ids, &content)
                .await
            {
                foims_common::log_error!("log.agent.alert_accumulate_failed", error = e);
            }
            return;
        }

        if effects.persist {
            episode.persisted = true;
            if let Err(e) = self.upsert(episode).await {
                foims_common::log_error!("log.agent.episode_persist_failed", error = e);
            }
        }
    }

    /// 向全部管理员各写一条站内通知，返回新行 ID（静默窗口内就地刷新按 ID 更新）。
    async fn notify(
        &self,
        host: &HostRef,
        episode: &Episode,
        content: &str,
        email: Option<EmailAlert<'_>>,
        recipients: &[Uuid],
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let mut ids = Vec::with_capacity(recipients.len());
        for recipient in recipients {
            ids.push(
                foims_snmp::events::create_notification(
                    &self.pool,
                    NOTIFICATION_TITLE_KEY,
                    content,
                    NOTIFICATION_TYPE,
                    Some(recipient),
                )
                .await?,
            );
        }
        foims_common::log_info!(
            "log.agent.episode_notified",
            hostname = host.hostname.as_str(),
            metric = episode.metric,
            recipients = recipients.len()
        );

        // 邮件仅在开告警时发送，且受「通知配置」主机告警邮件开关门控；
        // 发送失败只记日志——邮件丢失可接受，站内通知失败才保持未通知态重试
        if let Some(email) = email
            && foims_auth::smtp::load_notification_methods(&self.pool)
                .await
                .agent_alert
            && let Err(e) = foims_auth::smtp::send_host_alert_email(
                &self.pool,
                &host.hostname,
                &host.ip,
                email.metric,
                email.value,
                email.threshold,
            )
            .await
        {
            foims_common::log_error!(
                "log.agent.alert_email_send_failed",
                hostname = host.hostname.as_str(),
                metric = email.metric,
                error = e
            );
        }
        Ok(ids)
    }

    /// 静默窗口内累计：就地改写同一批通知的文案（不新增行、不改时间，
    /// 通知中心的条目数与已读状态都不被持续告警刷新）。
    async fn refresh_notifications(&self, ids: &[Uuid], content: &str) -> Result<(), sqlx::Error> {
        if ids.is_empty() {
            return Ok(());
        }
        let joined = ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        sqlx::query(
            "UPDATE notifications SET content = $2 \
             WHERE id::text = ANY(string_to_array($1, ','))",
        )
        .bind(&joined)
        .bind(content)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn upsert(&self, episode: &Episode) -> Result<(), sqlx::Error> {
        let notify_ids = episode
            .notify_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        sqlx::query(
            r"INSERT INTO host_alert_episodes (host_id, metric, status, started_at, opened_at,
                                               resolved_at, last_exceed_at, last_notified_at,
                                               last_value, threshold, exceed_count, notify_ids,
                                               updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW())
               ON CONFLICT (host_id, metric) DO UPDATE SET
                   status = EXCLUDED.status,
                   started_at = EXCLUDED.started_at,
                   opened_at = EXCLUDED.opened_at,
                   resolved_at = EXCLUDED.resolved_at,
                   last_exceed_at = EXCLUDED.last_exceed_at,
                   last_notified_at = EXCLUDED.last_notified_at,
                   last_value = EXCLUDED.last_value,
                   threshold = EXCLUDED.threshold,
                   exceed_count = EXCLUDED.exceed_count,
                   notify_ids = EXCLUDED.notify_ids,
                   updated_at = NOW()",
        )
        .bind(episode.host_id)
        .bind(episode.metric)
        .bind(episode.status.as_str())
        .bind(episode.started_at)
        .bind(episode.opened_at)
        .bind(episode.resolved_at)
        .bind(episode.last_exceed_at)
        .bind(episode.last_notified_at)
        .bind(episode.last_value)
        .bind(i16::from(episode.threshold))
        .bind(episode.exceed_count)
        .bind(notify_ids)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_row(&self, host_id: Uuid, metric: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM host_alert_episodes WHERE host_id = $1 AND metric = $2")
            .bind(host_id)
            .bind(metric)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 刷盘周期内变化的计数（Pending 等待时长、Active 累计次数）。
    pub async fn flush_dirty(&self) {
        let targets: Vec<Episode> = self
            .episodes
            .iter()
            .filter(|entry| entry.value().dirty || !entry.value().persisted)
            .map(|entry| entry.value().clone())
            .collect();
        for mut episode in targets {
            if let Err(e) = self.upsert(&episode).await {
                foims_common::log_error!("log.agent.episode_persist_failed", error = e);
                continue;
            }
            episode.dirty = false;
            episode.persisted = true;
            self.episodes
                .insert((episode.host_id, episode.metric), episode);
        }
    }

    /// 兜底扫描：用 monitored_hosts 快照列评估一轮（复位停止上报的主机告警），
    /// 清理主机已不在 active 集合的 episode，回收过期 resolved，最后刷盘。
    pub async fn sweep_once(&self) {
        let Some(policy) = self.policy().await else {
            return;
        };
        let hosts = match self.load_hosts().await {
            Ok(hosts) => hosts,
            Err(e) => {
                foims_common::log_error!("log.agent.episode_sweep_load_failed", error = e);
                return;
            }
        };
        if policy.thresholds.enabled {
            for host in &hosts {
                self.evaluate_with_policy(&host.reference(), host.values(), &policy)
                    .await;
            }
        }
        self.cleanup_missing().await;
        self.purge_resolved().await;
        self.flush_dirty().await;
    }

    async fn load_hosts(&self) -> Result<Vec<SweepHost>, sqlx::Error> {
        // 实时采集在跑时 SNMP 主机交给秒级路径，扫描只兜底 agent 上报主机，
        // 否则旧快照会把刚恢复的告警重新拖回 Pending
        let sql = if self.live_covers_snmp {
            SWEEP_SQL_AGENT
        } else {
            SWEEP_SQL_BOTH
        };
        sqlx::query_as::<_, SweepHost>(sql)
            .fetch_all(&self.pool)
            .await
    }

    /// 主机行被删除/禁用/离线后其 episode 不再有意义：静默清理
    /// （失联不等于恢复，不发恢复通知）。口径取全量 active 主机，不能复用
    /// 兜底扫描的结果集——实时路径覆盖的 SNMP 主机不在扫描子集内。
    /// 查询失败时跳过清理，避免把仍在告警的 episode 误删后重复通知。
    async fn cleanup_missing(&self) {
        let active = match sqlx::query_scalar::<_, Uuid>(ACTIVE_HOST_SQL)
            .fetch_all(&self.pool)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                foims_common::log_error!("log.agent.episode_active_hosts_load_failed", error = e);
                return;
            }
        };
        let active: HashSet<Uuid> = active.into_iter().collect();
        let stale: Vec<EpisodeKey> = self
            .episodes
            .iter()
            .filter(|entry| !active.contains(&entry.key().0))
            .map(|entry| *entry.key())
            .collect();
        for (host_id, metric) in stale {
            self.episodes.remove(&(host_id, metric));
            if let Err(e) = self.delete_row(host_id, metric).await {
                foims_common::log_error!("log.agent.episode_delete_failed", error = e);
            }
        }
    }

    /// 回收过期 resolved：内存与库同步（超期条目按已清理对待，
    /// 此后再次超标视为全新告警，不再受旧冷却约束）。
    async fn purge_resolved(&self) {
        let cutoff = Utc::now() - chrono::Duration::seconds(RESOLVED_KEEP_SECS);
        self.episodes.retain(|_, episode| {
            episode.status != EpisodeStatus::Resolved
                || episode.resolved_at.is_none_or(|at| at >= cutoff)
        });
        let result = sqlx::query(
            "DELETE FROM host_alert_episodes WHERE status = 'resolved' \
             AND resolved_at < NOW() - make_interval(secs => $1)",
        )
        .bind(RESOLVED_KEEP_SECS as f64)
        .execute(&self.pool)
        .await;
        if let Err(e) = result {
            foims_common::log_error!("log.agent.episode_purge_failed", error = e);
        }
    }

    /// 常驻调度：每 [`FLUSH_INTERVAL_SECS`] 刷一次计数，每
    /// [`SWEEP_INTERVAL_SECS`] 做一轮兜底扫描；关闭信号到达即退出。
    pub fn start_sweeper(self: Arc<Self>, mut shutdown: ShutdownReceiver<()>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let sweeps_per_tick = (SWEEP_INTERVAL_SECS / FLUSH_INTERVAL_SECS).max(1);
            let mut ticks = 0u64;
            loop {
                tokio::select! {
                    _ = shutdown.recv() => break,
                    _ = tokio::time::sleep(Duration::from_secs(FLUSH_INTERVAL_SECS)) => {}
                }
                ticks += 1;
                if ticks.is_multiple_of(sweeps_per_tick) {
                    self.sweep_once().await;
                } else {
                    self.flush_dirty().await;
                }
            }
            foims_common::log_info!("log.agent.episode_sweeper_stopped");
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000 + secs, 0)
            .single()
            .expect("测试时刻构造失败")
    }

    fn windows(
        for_secs: u64,
        quiet_secs: u64,
        cooldown_secs: u64,
        hysteresis_pct: u8,
    ) -> AlertWindows {
        AlertWindows {
            for_secs,
            quiet_secs,
            cooldown_secs,
            hysteresis_pct,
        }
    }

    fn episode(now: DateTime<Utc>) -> Episode {
        Episode::pristine(Uuid::new_v4(), alerts::METRIC_CPU, 80, now)
    }

    #[test]
    fn 无告警记录时回落读数不产生状态() {
        let now = at(0);
        let mut ep = episode(now);
        let effects = ep.step(Some(10.0), 80, &windows(15, 300, 600, 5), now);
        assert!(effects.delete, "未开过告警的 Pending 直接消散");
        assert!(ep.is_placeholder(), "从未超标的空壳不留痕");

        let mut ep = episode(now);
        assert!(ep.step(None, 80, &windows(15, 300, 600, 5), now).is_none());
        assert!(ep.is_placeholder());
    }

    #[test]
    fn 持续时长未满保持待决_到点开告警() {
        let w = windows(15, 300, 600, 5);
        let mut ep = episode(at(0));
        let first = ep.step(Some(90.0), 80, &w, at(0));
        assert_eq!(ep.status, EpisodeStatus::Pending);
        assert!(!first.notify_open, "首帧未满 for_secs");
        assert_eq!(ep.exceed_count, 1);

        let second = ep.step(Some(91.0), 80, &w, at(10));
        assert!(!second.notify_open);
        let third = ep.step(Some(92.0), 80, &w, at(15));
        assert!(third.notify_open, "满 15 秒开告警");
        assert_eq!(ep.status, EpisodeStatus::Active);
        assert_eq!(ep.opened_at, Some(at(15)));
        assert!(ep.last_notified_at.is_none(), "发送时刻由运行时回填");
    }

    #[test]
    fn 迟滞带内保持告警不复位() {
        let w = windows(0, 300, 600, 10);
        let mut ep = episode(at(0));
        assert!(ep.step(Some(90.0), 80, &w, at(0)).notify_open);
        // 复位水位 = 80 * 0.9 = 72：75 落在带内，既不累计也不复位
        let band = ep.step(Some(75.0), 80, &w, at(5));
        assert!(band.is_none(), "带内无副作用");
        assert_eq!(ep.status, EpisodeStatus::Active);
        // 跌破 72 才算恢复
        let resolved = ep.step(Some(71.0), 80, &w, at(10));
        assert!(resolved.notify_resolved);
        assert_eq!(ep.status, EpisodeStatus::Resolved);
    }

    #[test]
    fn 静默窗口内累计_窗口外重发() {
        let w = windows(0, 300, 600, 0);
        let mut ep = episode(at(0));
        ep.step(Some(90.0), 80, &w, at(0));
        ep.last_notified_at = Some(at(0));
        ep.notify_ids.push(Uuid::new_v4());

        let refresh = ep.step(Some(95.0), 80, &w, at(200));
        assert!(refresh.refresh, "窗口内只累计");
        assert_eq!(ep.exceed_count, 2);

        let renotify = ep.step(Some(96.0), 80, &w, at(301));
        assert!(renotify.renotify, "窗口外重发一条");
    }

    #[test]
    fn 通知行缺失时按重发处理() {
        let w = windows(0, 300, 600, 0);
        let mut ep = episode(at(0));
        ep.step(Some(90.0), 80, &w, at(0));
        ep.last_notified_at = Some(at(0));
        // 上次发送失败：有通知时刻但没有行 ID，窗口内也应重试
        let effects = ep.step(Some(93.0), 80, &w, at(5));
        assert!(effects.renotify);
    }

    #[test]
    fn 恢复后冷却未满不重开() {
        let w = windows(15, 300, 600, 5);
        let mut ep = episode(at(0));
        ep.step(Some(90.0), 80, &w, at(0));
        ep.resolve(at(10));
        // 第 20 秒再次超标：进入 Pending 但冷却（600s）未到，不开告警
        let effects = ep.step(Some(90.0), 80, &w, at(20));
        assert_eq!(ep.status, EpisodeStatus::Pending);
        assert!(effects.persist);
        assert!(!effects.notify_open);
        // 冷却一到，下一次超标观测即开
        let opened = ep.step(Some(90.0), 80, &w, at(620));
        assert!(opened.notify_open);
        assert_eq!(ep.status, EpisodeStatus::Active);
        // 新 episode 从恢复后的那次观测重新计数（20s 待决 + 620s 升级），
        // 不继承上一轮的累计
        assert_eq!(ep.exceed_count, 2);
    }

    #[test]
    fn 无读数保持状态不判为恢复() {
        let w = windows(0, 300, 600, 5);
        let mut ep = episode(at(0));
        ep.step(Some(90.0), 80, &w, at(0));
        let effects = ep.step(None, 80, &w, at(100));
        assert!(effects.is_none());
        assert_eq!(ep.status, EpisodeStatus::Active, "缺读数不推翻实时判定");
    }

    #[test]
    fn 阈值变化立即改写并落盘() {
        let w = windows(0, 300, 600, 0);
        let mut ep = episode(at(0));
        ep.step(Some(90.0), 80, &w, at(0));
        // 阈值上调到读数之上：同一轮观测即按新口径判定，不必等下一份读数
        let effects = ep.step(Some(90.0), 95, &w, at(5));
        assert_eq!(ep.threshold, 95);
        assert!(effects.persist, "阈值同步落库");
        assert!(effects.notify_resolved, "按新阈值判定恢复");
    }

    #[test]
    fn 复位水位按迟滞比例下移() {
        assert_eq!(recover_level(80, 10), 72.0);
        assert_eq!(recover_level(80, 0), 80.0);
        assert_eq!(level_of(75.0, 80, 10), Level::Band);
        assert_eq!(level_of(72.0, 80, 10), Level::Under);
        assert_eq!(level_of(80.0, 80, 0), Level::Under);
        assert_eq!(level_of(80.1, 80, 0), Level::Over);
    }

    #[test]
    fn 窗口参数_取自阈值配置() {
        let thresholds = AlertThresholds {
            enabled: true,
            cpu_pct: Some(90),
            ..Default::default()
        };
        let w = AlertWindows::from_thresholds(&thresholds);
        assert_eq!(
            (w.for_secs, w.quiet_secs, w.cooldown_secs, w.hysteresis_pct),
            (15, 300, 600, 5)
        );
    }

    #[test]
    fn 行映射_未知指标与状态跳过() {
        let base = EpisodeRow {
            host_id: Uuid::new_v4(),
            metric: "cpu".to_string(),
            status: "active".to_string(),
            started_at: at(0),
            opened_at: Some(at(1)),
            resolved_at: None,
            last_exceed_at: at(2),
            last_notified_at: Some(at(3)),
            last_value: Some(91.5),
            threshold: 80,
            exceed_count: 7,
            notify_ids: String::new(),
        };
        let episode = base.clone().into_episode().expect("合法行应还原");
        assert_eq!(episode.status, EpisodeStatus::Active);
        assert_eq!(episode.metric, "cpu");
        assert!(episode.persisted);

        let mut bad_metric = base.clone();
        bad_metric.metric = "gpu".to_string();
        assert!(bad_metric.into_episode().is_none());

        let mut bad_status = base.clone();
        bad_status.status = "alerting".to_string();
        assert!(bad_status.into_episode().is_none());
    }

    #[test]
    fn 通知行标识_解析逗号串() {
        let id = Uuid::new_v4();
        let raw = format!("{id},,bad-uuid");
        let ids = split_ids(&raw);
        assert_eq!(ids, vec![id]);
        assert!(split_ids("").is_empty());
    }

    #[test]
    fn 通知文案_带累计次数参数() {
        let host = HostRef {
            id: Uuid::new_v4(),
            hostname: "sw-1".to_string(),
            ip: "2001:db8::1".to_string(),
        };
        let mut ep = episode(at(0));
        let w = windows(0, 300, 600, 0);
        ep.step(Some(91.234), 80, &w, at(0));
        let content: serde_json::Value =
            serde_json::from_str(&alert_content(&host, &ep, "body_cpu")).expect("应为合法 JSON");
        assert_eq!(content["key"], "body_cpu");
        assert_eq!(content["params"]["hostname"], "sw-1");
        assert_eq!(content["params"]["ip"], "2001:db8::1");
        assert_eq!(content["params"]["value"], "91.2");
        assert_eq!(content["params"]["threshold"], "80");
        assert_eq!(content["params"]["count"], "1");

        // 恢复走同指标成对的另一条文案 key，参数口径一致
        let resolved: serde_json::Value =
            serde_json::from_str(&alert_content(&host, &ep, "resolved_cpu"))
                .expect("应为合法 JSON");
        assert_eq!(resolved["key"], "resolved_cpu");
        assert_eq!(resolved["params"]["count"], "1");
    }

    #[test]
    fn 指标名白名单还原() {
        assert_eq!(alerts::metric_of("cpu"), Some(alerts::METRIC_CPU));
        assert_eq!(alerts::metric_of("temp"), Some(alerts::METRIC_TEMP));
        assert_eq!(alerts::metric_of("gpu"), None);
    }
}
