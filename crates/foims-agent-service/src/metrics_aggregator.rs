//! 指标降采样聚合器：秒级原始读数 → 5 分钟桶（granularity=5）。
//!
//! 三层存储的中间层（原始秒级 → 5min 桶 → 1h 桶）：agent 上报、SNMP/IPMI
//! 实时与轮询路径在构造快照后同步调用 [`MetricsAggregator::ingest`]（纯内存，
//! 无 IO），后台 flush 循环每 30s 把到期桶落库 host_metrics_history
//! （granularity=5），单行 JSONB 形如 `{"cpu":{"avg","min","max","last","cnt"}}`，
//! 与 upgrade 脚本的历史回填格式一致；1h 桶（granularity=60）由定时任务
//! agent_metrics_rollup 从 5min 桶滚降生成，不经过本模块。
//!
//! 内存与落库口径：
//! - 水位去重：同主机样本按 collected_at 单调递增接受，乱序/重放样本丢弃，
//!   与原始层唯一索引的幂等去重同源，防重放双计；
//! - 指纹切换：顶层键集（含 null 值键）变化视为指标口径变化，切换前的
//!   半截桶移入 stale 原样落库——不同口径数据不混桶、不丢数据；
//! - 迟到吸收：桶时间窗结束后再滞留 [`BUCKET_LINGER_SECS`] 才落库，
//!   窗口边缘的迟到样本仍能进桶；
//! - 冲突丢弃：落库 ON CONFLICT DO NOTHING——进程重启后重新累积的半截桶
//!   不得覆盖重启前已落库的完整桶（完整桶胜出）。

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use tokio::sync::broadcast::Receiver as ShutdownReceiver;
use uuid::Uuid;

use foims_common::log_error;

/// 桶跨度（秒）：granularity=5 即 5 分钟一桶。
const BUCKET_SPAN_SECS: i64 = 300;
/// 桶滞留（秒）：时间窗结束后再等此时长才落库，吸收迟到样本。
const BUCKET_LINGER_SECS: i64 = 600;
/// flush 循环节拍（秒）。
const FLUSH_TICK_SECS: u64 = 30;

/// 单指标键在桶内的累加器（avg 落库时由 sum/cnt 求得）。
#[derive(Debug, Clone)]
struct MetricAcc {
    sum: f64,
    min: f64,
    max: f64,
    /// 桶内最新样本值（样本按采集时刻单调到达，覆盖即最新）
    last: f64,
    cnt: u64,
}

impl MetricAcc {
    fn new(v: f64) -> Self {
        Self {
            sum: v,
            min: v,
            max: v,
            last: v,
            cnt: 1,
        }
    }

    fn push(&mut self, v: f64) {
        self.sum += v;
        self.min = self.min.min(v);
        self.max = self.max.max(v);
        self.last = v;
        self.cnt += 1;
    }
}

/// 单主机的分桶状态。
#[derive(Debug, Default)]
struct HostBuckets {
    /// bucket_ts（epoch 秒，按 BUCKET_SPAN_SECS 对齐）→ 指标键 → 累加器
    buckets: HashMap<i64, BTreeMap<String, MetricAcc>>,
    /// 指标键集切换前的半截桶：口径不同的数据不混桶，滞留到期原样落库
    stale: Vec<(i64, BTreeMap<String, MetricAcc>)>,
    /// 指标键集指纹（排序后的顶层键列表）
    fingerprint: Option<Vec<String>>,
    /// 已接受样本的最大采集时刻（epoch 秒），单调水位
    watermark: Option<i64>,
}

/// 降采样聚合器：host_id → 分桶状态。采集路径同步喂入，flush 循环异步落库。
#[derive(Debug, Default)]
pub struct MetricsAggregator {
    hosts: DashMap<Uuid, HostBuckets>,
}

impl MetricsAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    /// 喂入一次原始读数（同步、纯内存）：采集路径构造快照后就地调用。
    pub fn ingest(&self, host_id: Uuid, collected_at: DateTime<Utc>, metrics: &Value) {
        let Value::Object(obj) = metrics else {
            return;
        };
        let ts = collected_at.timestamp();
        let mut hb = self.hosts.entry(host_id).or_default();
        // 水位去重：乱序/重放（collected_at ≤ 已接受最大时刻）直接丢弃
        if hb.watermark.is_some_and(|wm| ts <= wm) {
            return;
        }
        hb.watermark = Some(ts);

        // 指纹 = 顶层键集（含 null 值键：temp=null 抖动不视为口径变化）
        let mut keys: Vec<String> = obj.keys().cloned().collect();
        keys.sort();
        if hb.fingerprint.as_ref() != Some(&keys) {
            // 口径切换：当前未满桶整体移入 stale，新口径从空累积
            let drained: Vec<(i64, BTreeMap<String, MetricAcc>)> = hb.buckets.drain().collect();
            hb.stale.extend(drained);
            hb.fingerprint = Some(keys);
        }

        let bucket_ts = ts.div_euclid(BUCKET_SPAN_SECS) * BUCKET_SPAN_SECS;
        let bucket = hb.buckets.entry(bucket_ts).or_default();
        for (key, value) in obj {
            // 仅收有限数值键：null/字符串/NaN/无穷不进桶
            if let Some(v) = value.as_f64().filter(|n| n.is_finite()) {
                bucket
                    .entry(key.clone())
                    .and_modify(|acc| acc.push(v))
                    .or_insert_with(|| MetricAcc::new(v));
            }
        }
    }

    /// 落库到期桶与 stale 半截桶（flush 循环节拍与进程退出前调用）。
    pub async fn flush(&self, pool: &PgPool, now: DateTime<Utc>) {
        let cutoff = now.timestamp() - BUCKET_LINGER_SECS;
        let mut writes: Vec<(Uuid, i64, BTreeMap<String, MetricAcc>)> = Vec::new();
        for mut hb in self.hosts.iter_mut() {
            let expired = take_expired(&mut hb, cutoff);
            let host_id = *hb.key();
            for (bucket_ts, accs) in expired {
                writes.push((host_id, bucket_ts, accs));
            }
        }
        for (host_id, bucket_ts, accs) in writes {
            let Some(collected_at) = DateTime::from_timestamp(bucket_ts, 0) else {
                continue;
            };
            let metrics = bucket_metrics_json(&accs);
            let result = sqlx::query(
                r"INSERT INTO host_metrics_history (agent_id, granularity, collected_at, metrics)
                  VALUES ($1, 5, $2, $3) ON CONFLICT (agent_id, granularity, collected_at) DO NOTHING",
            )
            .bind(host_id)
            .bind(collected_at)
            .bind(&metrics)
            .execute(pool)
            .await;
            if let Err(e) = result {
                log_error!("log.agent.metrics_flush_failed", error = e);
            }
        }
    }
}

/// 取出到期桶（bucket_ts ≤ cutoff）与全部 stale 半截桶（纯函数，便于单测）。
fn take_expired(hb: &mut HostBuckets, cutoff: i64) -> Vec<(i64, BTreeMap<String, MetricAcc>)> {
    let expired: Vec<i64> = hb
        .buckets
        .keys()
        .copied()
        .filter(|ts| *ts <= cutoff)
        .collect();
    let mut out: Vec<(i64, BTreeMap<String, MetricAcc>)> = expired
        .into_iter()
        .filter_map(|ts| hb.buckets.remove(&ts).map(|accs| (ts, accs)))
        .collect();
    out.append(&mut hb.stale);
    out
}

/// 桶累加器 → 落库 JSONB：`{"key":{"avg","min","max","last","cnt"}}`，
/// avg 求均值后保留 4 位小数（与 upgrade 回填脚本口径一致）。
fn bucket_metrics_json(accs: &BTreeMap<String, MetricAcc>) -> Value {
    let mut out = Map::with_capacity(accs.len());
    for (key, acc) in accs {
        out.insert(
            key.clone(),
            json!({
                "avg": round4(acc.sum / acc.cnt as f64),
                "min": acc.min,
                "max": acc.max,
                "last": acc.last,
                "cnt": acc.cnt,
            }),
        );
    }
    Value::Object(out)
}

/// 保留 4 位小数（就近舍入）。
pub(crate) fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// flush 常驻循环：每 30s 落库一次到期桶；关闭信号到达后先冲刷一次再退出，
/// 尽量减少重启窗口内的桶丢失。
pub async fn run_metrics_flush_loop(
    aggregator: Arc<MetricsAggregator>,
    pool: PgPool,
    mut shutdown: ShutdownReceiver<()>,
) {
    let mut tick = tokio::time::interval(Duration::from_secs(FLUSH_TICK_SECS));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = shutdown.recv() => break,
            _ = tick.tick() => aggregator.flush(&pool, Utc::now()).await,
        }
    }
    aggregator.flush(&pool, Utc::now()).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().unwrap()
    }

    #[test]
    fn 同桶样本累加均值极值与计数() {
        let agg = MetricsAggregator::new();
        let host = Uuid::nil();
        agg.ingest(host, at(100), &json!({"cpu": 10.0, "temp": 30.0}));
        agg.ingest(host, at(160), &json!({"cpu": 20.0, "temp": Value::Null}));
        // temp=null 只占指纹不产生数值样本；cpu 累计两样本
        let hb = agg.hosts.get(&host).unwrap();
        let cpu = &hb.buckets[&0];
        assert_eq!(cpu["cpu"].cnt, 2);
        assert!((cpu["cpu"].sum - 30.0).abs() < 1e-9);
        assert_eq!(cpu["cpu"].min, 10.0);
        assert_eq!(cpu["cpu"].max, 20.0);
        assert_eq!(cpu["cpu"].last, 20.0);
        assert_eq!(cpu["temp"].cnt, 1);
    }

    #[test]
    fn 跨桶按300秒对齐归桶() {
        let agg = MetricsAggregator::new();
        let host = Uuid::nil();
        // 299s 归 0 号桶，300s 归 300 号桶
        agg.ingest(host, at(299), &json!({"cpu": 1.0}));
        agg.ingest(host, at(300), &json!({"cpu": 2.0}));
        let hb = agg.hosts.get(&host).unwrap();
        assert!(hb.buckets.contains_key(&0));
        assert!(hb.buckets.contains_key(&300));
    }

    #[test]
    fn 水位丢弃乱序与重放样本() {
        let agg = MetricsAggregator::new();
        let host = Uuid::nil();
        agg.ingest(host, at(400), &json!({"cpu": 1.0}));
        agg.ingest(host, at(399), &json!({"cpu": 9.0})); // 乱序丢弃
        agg.ingest(host, at(400), &json!({"cpu": 9.0})); // 重放丢弃
        let hb = agg.hosts.get(&host).unwrap();
        // 400s 归 300 号桶（300-599s）
        assert_eq!(hb.buckets[&300]["cpu"].cnt, 1);
        assert_eq!(hb.buckets[&300]["cpu"].last, 1.0);
    }

    #[test]
    fn 指纹切换半截桶进stale不混口径() {
        let agg = MetricsAggregator::new();
        let host = Uuid::nil();
        agg.ingest(host, at(100), &json!({"cpu": 1.0, "mem": 2.0}));
        agg.ingest(host, at(160), &json!({"cpu": 3.0})); // 键集变化 → 旧桶进 stale
        let hb = agg.hosts.get(&host).unwrap();
        assert_eq!(hb.stale.len(), 1);
        // 新口径桶不含旧口径键
        assert_eq!(hb.buckets[&0]["cpu"].cnt, 1);
        assert!(!hb.buckets[&0].contains_key("mem"));
    }

    #[test]
    fn 非有限与非数值键跳过() {
        let agg = MetricsAggregator::new();
        let host = Uuid::nil();
        agg.ingest(
            host,
            at(100),
            &json!({"cpu": f64::NAN, "mem": "40", "disk": null, "temp": 30.0}),
        );
        let hb = agg.hosts.get(&host).unwrap();
        let bucket = &hb.buckets[&0];
        assert!(!bucket.contains_key("cpu"));
        assert!(!bucket.contains_key("mem"));
        assert!(!bucket.contains_key("disk"));
        assert_eq!(bucket["temp"].cnt, 1);
        // 指纹含全部顶层键（含 null 键），共 4 键
        assert_eq!(hb.fingerprint.as_ref().unwrap().len(), 4);
    }

    #[test]
    fn 到期桶与stale按阈值取出() {
        let mut hb = HostBuckets::default();
        hb.buckets.insert(0, BTreeMap::new());
        hb.buckets.insert(600, BTreeMap::new());
        hb.stale.push((900, BTreeMap::new()));
        // cutoff=599：0 号桶到期 + stale 全取，600 号桶保留
        let taken = take_expired(&mut hb, 599);
        assert_eq!(taken.len(), 2);
        assert!(taken.iter().any(|(ts, _)| *ts == 0));
        assert!(taken.iter().any(|(ts, _)| *ts == 900));
        assert!(hb.buckets.contains_key(&600));
        assert!(hb.stale.is_empty());
    }

    #[test]
    fn 落库JSON结构与均值舍入() {
        let mut accs = BTreeMap::new();
        accs.insert(
            "cpu".to_string(),
            MetricAcc {
                sum: 10.0,
                min: 4.0,
                max: 6.0,
                last: 6.0,
                cnt: 2,
            },
        );
        let metrics = bucket_metrics_json(&accs);
        let cpu = metrics["cpu"].as_object().unwrap();
        assert_eq!(cpu.len(), 5);
        assert_eq!(cpu["avg"], json!(5.0));
        assert_eq!(cpu["min"], json!(4.0));
        assert_eq!(cpu["max"], json!(6.0));
        assert_eq!(cpu["last"], json!(6.0));
        assert_eq!(cpu["cnt"], json!(2));
        // 1/3 均值保留 4 位小数
        assert_eq!(round4(1.0 / 3.0), 0.3333);
    }
}
