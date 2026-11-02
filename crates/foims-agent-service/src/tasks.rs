//! Agent 监控闭环调度任务执行器（设计 docs/agent-design.md §5）：由主程序
//! TaskRegistry 注册、系统级定时任务驱动的 agent 离线判定、历史指标分层清理、
//! 1h 桶滚降、SNMP 与 IPMI 设备轮询执行器（采集细节见 [`crate::snmp_poll`]
//! 与 [`crate::ipmi_poll`]）。

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use foims_scheduler::{SchedulerError, SchedulerResult, TaskContext, TaskExecutor};
use serde_json::{Value, json};
use uuid::Uuid;

use foims_common::{AppMessage, log_debug, msg};

use crate::ipmi_poll;
use crate::metrics_aggregator::{MetricsAggregator, round4};
use crate::snmp_poll;

/// SNMP 轮询间隔缺省值（秒，调度注册侧保持一致）
const DEFAULT_SNMP_POLL_INTERVAL_SECS: u64 = 300;
/// IPMI 轮询间隔缺省值（秒，与 SNMP 同 5 分钟节奏）
const DEFAULT_IPMI_POLL_INTERVAL_SECS: u64 = 300;
/// 历史保留天数缺省值与上限（天数类清理任务共用口径）
const DEFAULT_RETENTION_DAYS: i64 = 30;
const MAX_RETENTION_DAYS: i64 = 3650;
/// 原始层（granularity=0）保留天数缺省值：秒级数据量最大，短保留
const DEFAULT_RAW_RETENTION_DAYS: i64 = 3;
/// 5min 桶层（granularity=5）保留天数缺省值：趋势曲线主层
const DEFAULT_ARCHIVE_RETENTION_DAYS: i64 = 30;
/// 1h 桶层（granularity=60）保留天数缺省值：长期趋势层
const DEFAULT_ROLLUP_RETENTION_DAYS: i64 = 365;
/// 单轮滚降处理的小时桶数上限：每小时例行运行，积压断点顺延至下一轮
const ROLLUP_HOURS_PER_RUN: i64 = 500;
/// 原始层→5min 桶补聚合回看窗口（小时）：覆盖进程重启/落库瞬断造成的近期
/// 桶层缺口（原始层默认保留 3 天，每小时自愈一次，26h 留足整天余量）；
/// i32 对应 make_interval(hours => int) 签名
const RECONCILE_LOOKBACK_HOURS: i32 = 26;

/// 从任务配置读取正整数参数（缺失、类型不符或 ≤0 时回落默认值）。
fn config_u64(config: &Value, key: &str, default: u64) -> u64 {
    config
        .get(key)
        .and_then(Value::as_u64)
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

/// 从任务配置读取保留天数（缺失/类型不符/越界回落默认值，范围 1-3650）。
fn retention_days(config: &Value, key: &str, default: i64) -> i64 {
    config
        .get(key)
        .and_then(Value::as_i64)
        .filter(|d| (1..=MAX_RETENTION_DAYS).contains(d))
        .unwrap_or(default)
}

/// Agent 离线判定任务执行器：last_seen 超过 判定间隔 × 倍数 的
/// active agent 置为 offline（disabled 不参与判定）。
/// 仅对 source='agent' 生效：SNMP 采集行轮询间隔更长（默认 5 分钟），
/// 由 snmp_poll 按自身间隔 × 2 独立判定离线。
///
/// 判定间隔与采集节奏解耦（agent 独立维护上报节奏，服务端不再可配），
/// 固定基准 [`AGENT_OFFLINE_BASE_INTERVAL_SECS`]；离线敏感度倍数仍为
/// AgentConfig.offline_factor；任务配置显式给出 interval_secs/factor 时
/// 以任务配置优先。
pub struct AgentOfflineTaskExecutor {
    /// 默认判定间隔（秒），固定基准常量
    default_interval_secs: u64,
    /// 默认离线倍数，注册时取自 AgentConfig.offline_factor
    default_factor: u64,
}

/// 离线判定间隔固定基准（秒）：agent 上报节奏 clamp 后下限 10s 批量，
/// 60s 基准容忍连续丢批不误判，保持既有判定节奏不变。
pub const AGENT_OFFLINE_BASE_INTERVAL_SECS: u64 = 60;

impl AgentOfflineTaskExecutor {
    pub fn new(default_interval_secs: u64, default_factor: u64) -> Self {
        Self {
            default_interval_secs,
            default_factor,
        }
    }
}

#[async_trait]
impl TaskExecutor for AgentOfflineTaskExecutor {
    fn task_type(&self) -> &str {
        "agent_offline"
    }

    /// 每分钟例行运行且多数轮次无翻转，成功日志降为 debug 避免刷屏
    fn debug_routine_logs(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<AppMessage> {
        let interval_secs = config_u64(&ctx.config, "interval_secs", self.default_interval_secs);
        let factor = config_u64(&ctx.config, "factor", self.default_factor);
        let seconds = interval_secs.saturating_mul(factor);

        let result = sqlx::query(
            r"UPDATE monitored_hosts SET status = 'offline'
               WHERE status = 'active'
                 AND source = 'agent'
                 AND last_seen IS NOT NULL
                 AND last_seen < NOW() - make_interval(secs => $1)",
        )
        .bind(seconds as f64)
        .execute(&ctx.pool)
        .await
        .map_err(|e| {
            SchedulerError::Execution(msg("server.task.agent_offline_failed").with("error", e))
        })?;

        log_debug!(
            "log.task.agent_offline_completed",
            count = result.rows_affected()
        );
        Ok(msg("server.task.agent_offline_completed").with("count", result.rows_affected()))
    }
}

/// Agent 历史指标分层清理任务执行器：按 granularity 分层删除超过保留期的
/// host_metrics_history 行（原始秒级/5min 桶/1h 桶保留期独立可配），并同轮
/// 清理 snmp_events。
pub struct AgentHistoryCleanupTaskExecutor;

#[async_trait]
impl TaskExecutor for AgentHistoryCleanupTaskExecutor {
    fn task_type(&self) -> &str {
        "agent_history_cleanup"
    }

    /// 每日例行运行，成功日志降为 debug 避免刷屏
    fn debug_routine_logs(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<AppMessage> {
        // 三层保留期：原始层（秒级，体积最大）默认 3 天、5min 桶层默认 30 天、
        // 1h 桶层（长期趋势）默认 365 天，均 clamp 1-3650
        let layers: [(&str, i64, i32); 3] = [
            ("retention_days", DEFAULT_RAW_RETENTION_DAYS, 0),
            ("archive_retention_days", DEFAULT_ARCHIVE_RETENTION_DAYS, 5),
            ("rollup_retention_days", DEFAULT_ROLLUP_RETENTION_DAYS, 60),
        ];
        let mut total = 0u64;
        for (key, default_days, granularity) in layers {
            let days = retention_days(&ctx.config, key, default_days);
            let result = sqlx::query(
                r"DELETE FROM host_metrics_history
                   WHERE granularity = $1
                     AND collected_at < NOW() - make_interval(days => $2)",
            )
            .bind(granularity)
            .bind(i32::try_from(days).unwrap_or(i32::MAX))
            .execute(&ctx.pool)
            .await
            .map_err(|e| {
                SchedulerError::Execution(
                    msg("server.task.agent_history_cleanup_failed").with("error", e),
                )
            })?;
            total += result.rows_affected();
        }

        log_debug!("log.task.agent_history_cleanup_completed", count = total);

        // 同任务内追加清理 snmp_events：保留天数取任务 config（注册侧由 agent
        // 配置段注入，缺省 30，范围 1-3650），与指标分层保留期独立
        let snmp_days = retention_days(
            &ctx.config,
            "snmp_event_retention_days",
            DEFAULT_RETENTION_DAYS,
        );
        let snmp_result = sqlx::query(
            r"DELETE FROM snmp_events
               WHERE occurred_at < NOW() - make_interval(days => $1)",
        )
        .bind(i32::try_from(snmp_days).unwrap_or(i32::MAX))
        .execute(&ctx.pool)
        .await
        .map_err(|e| {
            SchedulerError::Execution(
                msg("server.task.agent_history_cleanup_failed").with("error", e),
            )
        })?;
        log_debug!(
            "log.snmp.events_cleanup_completed",
            count = snmp_result.rows_affected()
        );

        Ok(msg("server.task.agent_history_cleanup_completed").with("count", total))
    }
}

/// SNMP 设备轮询任务执行器：采集已配置 SNMP 的设备（MIB-II 系统组），
/// 结果以 source='snmp' 的 monitored_hosts 行呈现于主机监控页，并按
/// 轮询间隔 × 2 翻转连续失联设备为 offline。
pub struct AgentSnmpPollTaskExecutor {
    /// 降采样聚合器喂料口（None = SNMP 实时采集开启，秒级样本已覆盖，
    /// 由实时路径喂入；注册侧按 snmp.live.enabled 决定）
    metrics: Option<Arc<MetricsAggregator>>,
}

impl AgentSnmpPollTaskExecutor {
    pub fn new(metrics: Option<Arc<MetricsAggregator>>) -> Self {
        Self { metrics }
    }
}

#[async_trait]
impl TaskExecutor for AgentSnmpPollTaskExecutor {
    fn task_type(&self) -> &str {
        "agent_snmp_poll"
    }

    /// 5 分钟例行运行，成功日志降为 debug 避免刷屏
    fn debug_routine_logs(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<AppMessage> {
        let interval_secs = config_u64(
            &ctx.config,
            "interval_secs",
            DEFAULT_SNMP_POLL_INTERVAL_SECS,
        );
        let (ok, fail) = snmp_poll::poll_all(&ctx.pool, interval_secs, self.metrics.clone()).await;
        log_debug!("log.task.agent_snmp_poll_completed", ok = ok, fail = fail);
        Ok(msg("server.task.agent_snmp_poll_completed").with("ok", ok).with("fail", fail))
    }
}

/// IPMI 设备带外轮询任务执行器：采集已配置 IPMI 凭据的设备（Get Device ID +
/// Chassis Status + SDR 传感器），结果以 source='ipmi' 的 monitored_hosts 行
/// 呈现于主机监控页，并按轮询间隔 × 2 翻转连续失联设备为 offline
/// （采集细节见 [`crate::ipmi_poll`]）。
pub struct AgentIpmiPollTaskExecutor {
    /// 降采样聚合器喂料口（None = IPMI 实时采集开启，秒级样本已覆盖，
    /// 注册侧按 ipmi.live.enabled 决定）
    metrics: Option<Arc<MetricsAggregator>>,
}

impl AgentIpmiPollTaskExecutor {
    pub fn new(metrics: Option<Arc<MetricsAggregator>>) -> Self {
        Self { metrics }
    }
}

#[async_trait]
impl TaskExecutor for AgentIpmiPollTaskExecutor {
    fn task_type(&self) -> &str {
        "agent_ipmi_poll"
    }

    /// 5 分钟例行运行，成功日志降为 debug 避免刷屏
    fn debug_routine_logs(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<AppMessage> {
        let interval_secs = config_u64(
            &ctx.config,
            "interval_secs",
            DEFAULT_IPMI_POLL_INTERVAL_SECS,
        );
        let (ok, fail) = ipmi_poll::poll_all(&ctx.pool, interval_secs, self.metrics.clone()).await;
        log_debug!("log.task.agent_ipmi_poll_completed", ok = ok, fail = fail);
        Ok(msg("server.task.agent_ipmi_poll_completed").with("ok", ok).with("fail", fail))
    }
}

/// 1h 桶滚降任务执行器：先做原始层→5min 桶补聚合（自愈重启/落库瞬断造成的
/// 桶层缺口），再把已完结小时的 granularity=5 行按 (agent_id, 小时) 内存合并为
/// granularity=60 行（avg 按样本数 cnt 加权、min/max 取极值、last 取末桶、cnt
/// 累加），每小时第 10 分运行补断点漏滚；1h 行内嵌 `_src_max_id`（滚降时源行
/// 最大 id），源层数据后到时据此重滚。
pub struct AgentMetricsRollupTaskExecutor;

#[async_trait]
impl TaskExecutor for AgentMetricsRollupTaskExecutor {
    fn task_type(&self) -> &str {
        "agent_metrics_rollup"
    }

    /// 每小时例行运行，成功日志降为 debug 避免刷屏
    fn debug_routine_logs(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<AppMessage> {
        // 第一步：原始层→5min 桶补聚合。仅聚合「5min 层整桶缺失」的桶（NOT
        // EXISTS 防与已有半截桶混口径），且桶窗结束已超过 flush 滞留期
        //（≤ NOW()-15min）防与聚合器在途桶竞争——与聚合器落库同为
        // ON CONFLICT DO NOTHING，先到者胜、口径一致，无重复计数。
        // 与 upgrade 回填脚本同一聚合口径（avg 4 位小数、last 取最新样本）。
        let healed = sqlx::query(
            r"INSERT INTO host_metrics_history (agent_id, granularity, collected_at, metrics)
              SELECT agent_id, 5, bucket,
                     jsonb_object_agg(k, jsonb_build_object(
                         'avg', round((sum_v / cnt)::numeric, 4),
                         'min', min_v, 'max', max_v, 'last', last_v, 'cnt', cnt))
              FROM (
                  SELECT agent_id, bucket, k,
                         sum(nv) AS sum_v,
                         min(nv) AS min_v,
                         max(nv) AS max_v,
                         (array_agg(nv ORDER BY collected_at DESC))[1] AS last_v,
                         count(*) AS cnt
                  FROM (
                      SELECT s.agent_id, s.bucket, s.collected_at, e.k,
                             (e.v #>> '{}')::double precision AS nv
                      FROM (
                          SELECT h.agent_id, h.collected_at, h.metrics,
                                 to_timestamp(floor(extract(epoch FROM h.collected_at) / 300) * 300)
                                     AS bucket
                          FROM host_metrics_history h
                          WHERE h.granularity = 0
                            AND h.collected_at >= NOW() - make_interval(hours => $1)
                            AND to_timestamp(floor(extract(epoch FROM h.collected_at) / 300) * 300)
                                <= NOW() - INTERVAL '15 minutes'
                            AND jsonb_typeof(h.metrics) = 'object'
                            AND NOT EXISTS (
                                SELECT 1 FROM host_metrics_history h5
                                WHERE h5.agent_id = h.agent_id
                                  AND h5.granularity = 5
                                  AND h5.collected_at =
                                      to_timestamp(floor(extract(epoch FROM h.collected_at) / 300) * 300)
                            )
                      ) s
                      CROSS JOIN LATERAL jsonb_each(s.metrics) AS e(k, v)
                      WHERE jsonb_typeof(e.v) = 'number'
                  ) samples
                  GROUP BY agent_id, bucket, k
              ) key_stats
              GROUP BY agent_id, bucket
              ON CONFLICT (agent_id, granularity, collected_at) DO NOTHING",
        )
        .bind(RECONCILE_LOOKBACK_HOURS)
        .execute(&ctx.pool)
        .await
        .map_err(|e| {
            SchedulerError::Execution(msg("server.task.agent_metrics_rollup_failed").with("error", e))
        })?
        .rows_affected();

        // 第二步：选待滚降小时——无 1h 桶行，或已有行落后于本小时 5min 数据
        //（_src_max_id 记录滚降时源行最大 id，小于当前最大 id 即补聚合/迟到
        // 桶已到，需重滚）。小时窗已结束且留足 5min 桶落库滞留（10min）余量。
        let pending = sqlx::query_as::<_, (Uuid, DateTime<Utc>)>(
            r"SELECT DISTINCT agent_id,
                      to_timestamp(floor(extract(epoch FROM collected_at) / 3600) * 3600) AS hour_bucket
               FROM host_metrics_history h1
               WHERE granularity = 5
                 AND to_timestamp(floor(extract(epoch FROM collected_at) / 3600) * 3600)
                     + INTERVAL '1 hour' <= NOW() - INTERVAL '15 minutes'
                 AND NOT EXISTS (
                     SELECT 1 FROM host_metrics_history h2
                     WHERE h2.agent_id = h1.agent_id
                       AND h2.granularity = 60
                       AND h2.collected_at =
                           to_timestamp(floor(extract(epoch FROM h1.collected_at) / 3600) * 3600)
                       AND (h2.metrics ->> '_src_max_id')::bigint =
                           (SELECT max(h3.id) FROM host_metrics_history h3
                            WHERE h3.agent_id = h1.agent_id
                              AND h3.granularity = 5
                              AND h3.collected_at >=
                                  to_timestamp(floor(extract(epoch FROM h1.collected_at) / 3600) * 3600)
                              AND h3.collected_at <
                                  to_timestamp(floor(extract(epoch FROM h1.collected_at) / 3600) * 3600)
                                      + INTERVAL '1 hour')
                 )
               ORDER BY hour_bucket
               LIMIT $1",
        )
        .bind(ROLLUP_HOURS_PER_RUN)
        .fetch_all(&ctx.pool)
        .await
        .map_err(|e| {
            SchedulerError::Execution(msg("server.task.agent_metrics_rollup_failed").with("error", e))
        })?;

        let pending_count = pending.len();
        for &(agent_id, hour) in &pending {
            let hour_end = hour + chrono::Duration::hours(1);
            let rows: Vec<(i64, Value)> = sqlx::query_as(
                r"SELECT id, metrics FROM host_metrics_history
                   WHERE agent_id = $1 AND granularity = 5
                     AND collected_at >= $2 AND collected_at < $3
                   ORDER BY collected_at",
            )
            .bind(agent_id)
            .bind(hour)
            .bind(hour_end)
            .fetch_all(&ctx.pool)
            .await
            .map_err(|e| {
                SchedulerError::Execution(
                    msg("server.task.agent_metrics_rollup_failed").with("error", e),
                )
            })?;
            let metrics = rollup_metrics(&rows);
            sqlx::query(
                r"INSERT INTO host_metrics_history (agent_id, granularity, collected_at, metrics)
                  VALUES ($1, 60, $2, $3)
                  ON CONFLICT (agent_id, granularity, collected_at)
                  DO UPDATE SET metrics = EXCLUDED.metrics",
            )
            .bind(agent_id)
            .bind(hour)
            .bind(&metrics)
            .execute(&ctx.pool)
            .await
            .map_err(|e| {
                SchedulerError::Execution(
                    msg("server.task.agent_metrics_rollup_failed").with("error", e),
                )
            })?;
        }

        log_debug!(
            "log.task.agent_metrics_rollup_completed",
            count = pending_count,
            healed = healed
        );
        Ok(
            msg("server.task.agent_metrics_rollup_completed")
                .with("count", pending_count)
                .with("healed", healed),
        )
    }
}

/// 5min 桶指标展开（聚合层行内 JSONB 单键）。
struct BucketSample {
    avg: f64,
    min: f64,
    max: f64,
    last: f64,
    cnt: u64,
}

/// 5min 桶 JSONB 单键 → 指标展开（任一字段缺失/类型不符返回 None）。
fn parse_bucket_sample(spec: &Value) -> Option<BucketSample> {
    Some(BucketSample {
        avg: spec.get("avg").and_then(Value::as_f64)?,
        min: spec.get("min").and_then(Value::as_f64)?,
        max: spec.get("max").and_then(Value::as_f64)?,
        last: spec.get("last").and_then(Value::as_f64)?,
        cnt: spec.get("cnt").and_then(Value::as_u64)?,
    })
}

/// 1h 滚降累加器：avg 按 cnt 加权（Σ(avg×cnt)/Σcnt），min/max 取极值，
/// last 逐桶覆盖（行按 collected_at 升序，后到即最新），cnt 累加。
#[derive(Default)]
struct RollupAcc {
    sum_weighted: f64,
    min: Option<f64>,
    max: Option<f64>,
    last: f64,
    cnt: u64,
}

impl RollupAcc {
    fn push(&mut self, s: &BucketSample) {
        self.sum_weighted += s.avg * s.cnt as f64;
        self.min = Some(self.min.map_or(s.min, |m| m.min(s.min)));
        self.max = Some(self.max.map_or(s.max, |m| m.max(s.max)));
        self.last = s.last;
        self.cnt += s.cnt;
    }
}

/// 5min 桶行 (id, metrics) 合并为 1h 桶 metrics：按键加权均值/极值/末值/计数
/// 重组（纯函数，便于单测），并内嵌 `_src_max_id`（源行最大 id，滚降任务据此
/// 判断 1h 桶是否落后于补聚合/迟到的 5min 数据）；结构不合法的键跳过。
fn rollup_metrics(rows: &[(i64, Value)]) -> Value {
    let mut accs: BTreeMap<String, RollupAcc> = BTreeMap::new();
    let mut src_max_id = 0i64;
    for (row_id, row) in rows {
        src_max_id = src_max_id.max(*row_id);
        let Some(obj) = row.as_object() else {
            continue;
        };
        for (key, spec) in obj {
            let Some(sample) = parse_bucket_sample(spec) else {
                continue;
            };
            accs.entry(key.clone()).or_default().push(&sample);
        }
    }
    let mut out = serde_json::Map::with_capacity(accs.len() + 1);
    for (key, acc) in &accs {
        if acc.cnt == 0 {
            continue;
        }
        let (Some(min), Some(max)) = (acc.min, acc.max) else {
            continue;
        };
        out.insert(
            key.clone(),
            json!({
                "avg": round4(acc.sum_weighted / acc.cnt as f64),
                "min": min,
                "max": max,
                "last": acc.last,
                "cnt": acc.cnt,
            }),
        );
    }
    out.insert("_src_max_id".to_string(), json!(src_max_id));
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 读取正整数否则回落默认() {
        let cfg = serde_json::json!({"interval_secs": 60, "zero": 0, "neg": -1, "text": "x"});
        assert_eq!(config_u64(&cfg, "interval_secs", 300), 60);
        // 缺失键 → 默认
        assert_eq!(config_u64(&cfg, "missing", 300), 300);
        // 0 被过滤 → 默认
        assert_eq!(config_u64(&cfg, "zero", 300), 300);
        // 负数/非数字（as_u64 返回 None）→ 默认
        assert_eq!(config_u64(&cfg, "neg", 300), 300);
        assert_eq!(config_u64(&cfg, "text", 300), 300);
    }

    #[test]
    fn 保留天数越界回落默认() {
        let cfg = serde_json::json!({"retention_days": 7, "zero": 0, "big": 9999, "text": "x"});
        assert_eq!(retention_days(&cfg, "retention_days", 3), 7);
        // 缺失/0/超上限/非数字 → 默认
        assert_eq!(retention_days(&cfg, "missing", 3), 3);
        assert_eq!(retention_days(&cfg, "zero", 3), 3);
        assert_eq!(retention_days(&cfg, "big", 3), 3);
        assert_eq!(retention_days(&cfg, "text", 3), 3);
    }

    #[test]
    fn 一小时滚降按键加权合并() {
        // 两个 5min 桶：cpu 均值按 cnt 加权（(10×4 + 20×6)/10 = 16），
        // min/max 取极值，last 取末桶，cnt 累加
        let rows = vec![
            (
                11i64,
                json!({"cpu": {"avg": 10.0, "min": 8.0, "max": 12.0, "last": 11.0, "cnt": 4},
                       "temp": {"avg": 30.0, "min": 30.0, "max": 30.0, "last": 30.0, "cnt": 4}}),
            ),
            (
                12i64,
                json!({"cpu": {"avg": 20.0, "min": 15.0, "max": 25.0, "last": 22.0, "cnt": 6}}),
            ),
        ];
        let out = rollup_metrics(&rows);
        let cpu = out["cpu"].as_object().unwrap();
        assert_eq!(cpu["avg"], json!(16.0));
        assert_eq!(cpu["min"], json!(8.0));
        assert_eq!(cpu["max"], json!(25.0));
        assert_eq!(cpu["last"], json!(22.0));
        assert_eq!(cpu["cnt"], json!(10));
        // 仅首桶存在的键原样合并
        assert_eq!(out["temp"]["cnt"], json!(4));
        // 内嵌源行最大 id，供滚降新鲜度判断
        assert_eq!(out["_src_max_id"], json!(12));
    }

    #[test]
    fn 滚降跳过结构不合法的键() {
        let rows = vec![(
            7i64,
            json!({
                "cpu": {"avg": 10.0, "min": 8.0, "max": 12.0, "last": 11.0, "cnt": 4},
                "bad": {"avg": "x"},
                "worse": "scalar",
            }),
        )];
        let out = rollup_metrics(&rows);
        let obj = out.as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert!(obj.contains_key("cpu"));
        assert_eq!(out["_src_max_id"], json!(7));
    }
}
