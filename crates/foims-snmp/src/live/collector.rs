//! 定向 GET 采集器：按探测期缓存的索引逐轮拉取状态/流量/资源组，
//! 仅在内存中做差值，输出 [`LiveSnapshot`]。
//!
//! 单轮请求按 [`MAX_OIDS_PER_REQUEST`] 分块顺序发送；任一分块失败只丢
//! 该块数据（`partial`），全部分块失败才把 `responded` 置 false 交给上层
//! 判定离线/退避。端口索引漂移（全部端口状态缺失）由上层触发重新探测。

use std::collections::{BTreeMap, HashMap};

use async_snmp::transport::UdpHandle;
use async_snmp::{Client, Oid, Value, oid};
use chrono::{DateTime, Utc};

use super::profile::apply_temp_unit;
use super::{
    DeviceProfile, EntityMem, LiveCounters, LivePortSnapshot, LiveSnapshot, MAX_OIDS_PER_REQUEST,
    MemPlan, PortCounters,
};
use crate::collect::metrics::iface_rate_bps;

/// 一次采集的读取项：OID 与语义标签一一对应。
struct ReadItem {
    oid: Oid,
    kind: ReadKind,
}

#[derive(Debug, Clone, Copy)]
enum ReadKind {
    Uptime,
    Admin(u32),
    Oper(u32),
    HcIn(u32),
    HcOut(u32),
    In32(u32),
    Out32(u32),
    Cpu,
    MemEntityUsage(usize),
    MemPoolUsed,
    MemPoolFree,
    MemHrSize,
    MemHrUsed,
    MemUcdTotal,
    MemUcdAvail,
    Temp(usize),
}

/// 单设备实时采集器：持有厂商 profile 与上一轮计数器状态。
pub struct LiveCollector {
    profile: DeviceProfile,
    prev: Option<LiveCounters>,
    resources: ResourceSnapshot,
}

/// 资源组（CPU/内存/温度）最近一次采到的值。
///
/// 资源组按 `resource_interval_secs`（默认 15s）采集，状态/流量按
/// `interval_secs`（默认 5s）采集；非资源轮次沿用缓存值，避免展示层在
/// 两种节奏之间闪断。
#[derive(Debug, Clone, Copy, Default)]
struct ResourceSnapshot {
    cpu_usage_pct: Option<f64>,
    mem_total: Option<u64>,
    mem_used: Option<u64>,
    mem_usage_pct: Option<f64>,
    temperature_c: Option<f64>,
}

impl LiveCollector {
    pub fn new(profile: DeviceProfile) -> Self {
        Self {
            profile,
            prev: None,
            resources: ResourceSnapshot::default(),
        }
    }

    pub fn profile(&self) -> &DeviceProfile {
        &self.profile
    }

    /// 采集一轮；`include_resources = false` 时跳过 CPU/内存/温度请求，
    /// 快照沿用上一次资源轮次的值（状态/流量仍每轮采集）。返回快照始终
    /// 包含端口数据（若本轮有任何响应）。
    pub async fn sample(
        &mut self,
        client: &Client<UdpHandle>,
        now: DateTime<Utc>,
        include_resources: bool,
    ) -> LiveSnapshot {
        let items = self.build_items(include_resources);
        let (values, responded, failed_batches) = fetch(client, &items).await;
        let now_unix = now.timestamp();
        let elapsed = self.prev.as_ref().map(|p| now_unix - p.ts).unwrap_or(0);

        let mut port_admin: BTreeMap<u32, bool> = BTreeMap::new();
        let mut port_oper: BTreeMap<u32, bool> = BTreeMap::new();
        let mut port_in: BTreeMap<u32, u64> = BTreeMap::new();
        let mut port_out: BTreeMap<u32, u64> = BTreeMap::new();
        let mut cpu_values: Vec<f64> = Vec::new();
        let mut mem_entity_usage: BTreeMap<usize, f64> = BTreeMap::new();
        let mut mem_pool_used = None;
        let mut mem_pool_free = None;
        let mut mem_hr_size = None;
        let mut mem_hr_used = None;
        let mut mem_ucd_total = None;
        let mut mem_ucd_avail = None;
        let mut temps: Vec<f64> = Vec::new();
        let mut uptime_secs = None;

        for item in &items {
            let Some(value) = values.get(&item.oid) else {
                continue;
            };
            match item.kind {
                ReadKind::Uptime => {
                    uptime_secs = value.as_u32().map(|ticks| i64::from(ticks) / 100);
                }
                ReadKind::Admin(idx) => {
                    if let Some(code) = value.as_u32() {
                        port_admin.insert(idx, code == 1);
                    }
                }
                ReadKind::Oper(idx) => {
                    if let Some(code) = value.as_u32() {
                        port_oper.insert(idx, code == 1);
                    }
                }
                ReadKind::HcIn(idx) | ReadKind::In32(idx) => {
                    if let Some(v) = value.as_u64() {
                        port_in.insert(idx, v);
                    }
                }
                ReadKind::HcOut(idx) | ReadKind::Out32(idx) => {
                    if let Some(v) = value.as_u64() {
                        port_out.insert(idx, v);
                    }
                }
                ReadKind::Cpu => {
                    if let Some(pct) = value.as_u32().filter(|p| *p <= 100) {
                        cpu_values.push(f64::from(pct));
                    }
                }
                ReadKind::MemEntityUsage(i) => {
                    if let Some(pct) = value.as_u32().filter(|p| *p <= 100) {
                        mem_entity_usage.insert(i, f64::from(pct));
                    }
                }
                ReadKind::MemPoolUsed => mem_pool_used = value.as_u64(),
                ReadKind::MemPoolFree => mem_pool_free = value.as_u64(),
                ReadKind::MemHrSize => mem_hr_size = value.as_u64(),
                ReadKind::MemHrUsed => mem_hr_used = value.as_u64(),
                ReadKind::MemUcdTotal => mem_ucd_total = value.as_u64(),
                ReadKind::MemUcdAvail => mem_ucd_avail = value.as_u64(),
                ReadKind::Temp(i) => {
                    if let Some(celsius) = raw_temp(value)
                        .and_then(|raw| {
                            self.profile
                                .temp
                                .get(i)
                                .and_then(|t| apply_temp_unit(raw, t.unit))
                        })
                        .filter(|c| 0.0 < *c && *c <= 200.0)
                    {
                        temps.push(celsius);
                    }
                }
            }
        }

        // 端口速率与计数器状态：首轮/间隔非正时只更新基线不出速率
        let prev_ports = self.prev.as_ref().map(|p| &p.ports);
        let mut next_ports: BTreeMap<u32, PortCounters> = BTreeMap::new();
        let mut ports = Vec::with_capacity(self.profile.ports.len());
        let mut rx_total = None::<f64>;
        let mut tx_total = None::<f64>;
        for meta in &self.profile.ports {
            let cur_in = port_in.get(&meta.if_index).copied();
            let cur_out = port_out.get(&meta.if_index).copied();
            let prev = prev_ports.and_then(|m| m.get(&meta.if_index));
            let (rx_bps, tx_bps) = match (prev, cur_in, cur_out) {
                (Some(p), Some(ci), Some(co)) if elapsed > 0 => (
                    iface_rate_bps(p.in_octets, ci, elapsed, meta.speed_bps),
                    iface_rate_bps(p.out_octets, co, elapsed, meta.speed_bps),
                ),
                _ => (None, None),
            };
            if let Some(v) = rx_bps {
                rx_total = Some(rx_total.unwrap_or(0.0) + v);
            }
            if let Some(v) = tx_bps {
                tx_total = Some(tx_total.unwrap_or(0.0) + v);
            }
            if let (Some(ci), Some(co)) = (cur_in, cur_out) {
                next_ports.insert(
                    meta.if_index,
                    PortCounters {
                        in_octets: ci,
                        out_octets: co,
                    },
                );
            }
            ports.push(LivePortSnapshot {
                if_index: meta.if_index,
                name: meta.name.clone(),
                speed_bps: meta.speed_bps,
                admin_up: port_admin.get(&meta.if_index).copied(),
                oper_up: port_oper.get(&meta.if_index).copied(),
                rx_bps,
                tx_bps,
            });
        }
        if !next_ports.is_empty() {
            self.prev = Some(LiveCounters {
                ts: now_unix,
                ports: next_ports,
            });
        }

        let cpu_usage_pct = cpu_values
            .into_iter()
            .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |m: f64| m.max(v))));
        let (mem_total, mem_used, mem_usage_pct) = match &self.profile.mem {
            MemPlan::None => (None, None, None),
            MemPlan::Pool { .. } => match (mem_pool_used, mem_pool_free) {
                (Some(used), Some(free)) => {
                    let total = used.saturating_add(free);
                    (Some(total), Some(used), usage_pct(total, used))
                }
                _ => (None, None, None),
            },
            MemPlan::EntityPercent { entities } => entity_mem_totals(entities, &mem_entity_usage),
            MemPlan::HrStorage { units, .. } => match (mem_hr_size, mem_hr_used) {
                (Some(size), Some(used)) => {
                    let total = size.saturating_mul(*units);
                    let used = used.saturating_mul(*units);
                    (Some(total), Some(used), usage_pct(total, used))
                }
                _ => (None, None, None),
            },
            MemPlan::Ucd { .. } => match (mem_ucd_total, mem_ucd_avail) {
                (Some(total_kb), Some(avail_kb)) => {
                    let total = total_kb.saturating_mul(1024);
                    let used = total_kb.saturating_sub(avail_kb).saturating_mul(1024);
                    (Some(total), Some(used), usage_pct(total, used))
                }
                _ => (None, None, None),
            },
        };

        let temperature_c = temps
            .into_iter()
            .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |m: f64| m.max(v))));

        let resources = if include_resources {
            let current = ResourceSnapshot {
                cpu_usage_pct,
                mem_total,
                mem_used,
                mem_usage_pct,
                temperature_c,
            };
            self.resources = current;
            current
        } else {
            self.resources
        };

        LiveSnapshot {
            collected_at: now,
            vendor: self.profile.vendor,
            cpu_usage_pct: resources.cpu_usage_pct,
            mem_total: resources.mem_total,
            mem_used: resources.mem_used,
            mem_usage_pct: resources.mem_usage_pct,
            temperature_c: resources.temperature_c,
            uptime_secs,
            ports,
            rx_bps: rx_total,
            tx_bps: tx_total,
            responded,
            partial: failed_batches > 0,
        }
    }

    /// 构造本轮读取项清单（顺序固定：uptime → 端口 → 资源组）。
    fn build_items(&self, include_resources: bool) -> Vec<ReadItem> {
        let mut items = Vec::with_capacity(self.profile.base_oid_count() + 8);
        items.push(ReadItem {
            oid: oid!(1, 3, 6, 1, 2, 1, 1, 3, 0),
            kind: ReadKind::Uptime,
        });
        for port in &self.profile.ports {
            let idx = port.if_index;
            items.push(ReadItem {
                oid: oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 7).child(idx),
                kind: ReadKind::Admin(idx),
            });
            items.push(ReadItem {
                oid: oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 8).child(idx),
                kind: ReadKind::Oper(idx),
            });
            if self.profile.use_hc {
                items.push(ReadItem {
                    oid: oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, 6).child(idx),
                    kind: ReadKind::HcIn(idx),
                });
                items.push(ReadItem {
                    oid: oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, 10).child(idx),
                    kind: ReadKind::HcOut(idx),
                });
            } else {
                items.push(ReadItem {
                    oid: oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 10).child(idx),
                    kind: ReadKind::In32(idx),
                });
                items.push(ReadItem {
                    oid: oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 16).child(idx),
                    kind: ReadKind::Out32(idx),
                });
            }
        }
        if include_resources {
            for source in &self.profile.cpu {
                items.push(ReadItem {
                    oid: source.oid(),
                    kind: ReadKind::Cpu,
                });
            }
            match &self.profile.mem {
                MemPlan::Pool { used, free } => {
                    items.push(ReadItem {
                        oid: used.oid(),
                        kind: ReadKind::MemPoolUsed,
                    });
                    items.push(ReadItem {
                        oid: free.oid(),
                        kind: ReadKind::MemPoolFree,
                    });
                }
                MemPlan::EntityPercent { entities } => {
                    for (i, entity) in entities.iter().enumerate() {
                        items.push(ReadItem {
                            oid: entity.usage.oid(),
                            kind: ReadKind::MemEntityUsage(i),
                        });
                    }
                }
                MemPlan::HrStorage { size, used, .. } => {
                    items.push(ReadItem {
                        oid: size.oid(),
                        kind: ReadKind::MemHrSize,
                    });
                    items.push(ReadItem {
                        oid: used.oid(),
                        kind: ReadKind::MemHrUsed,
                    });
                }
                MemPlan::Ucd { total_kb, avail_kb } => {
                    items.push(ReadItem {
                        oid: total_kb.oid(),
                        kind: ReadKind::MemUcdTotal,
                    });
                    items.push(ReadItem {
                        oid: avail_kb.oid(),
                        kind: ReadKind::MemUcdAvail,
                    });
                }
                MemPlan::None => {}
            }
            for (i, temp) in self.profile.temp.iter().enumerate() {
                items.push(ReadItem {
                    oid: temp.source.oid(),
                    kind: ReadKind::Temp(i),
                });
            }
        }
        items
    }
}

/// 分块 GET：返回（OID → 值, 是否收到有效值, 失败分块数）。
///
/// `responded` 只统计非异常 varbind：设备对整个请求回 noSuchObject 时
/// 视为无可用数据，由上层判定不可达，避免用空快照覆盖上一轮成功数据。
async fn fetch(
    client: &Client<UdpHandle>,
    items: &[ReadItem],
) -> (HashMap<Oid, Value>, bool, usize) {
    let mut values = HashMap::with_capacity(items.len());
    let mut responded = false;
    let mut failed = 0usize;
    for chunk in items.chunks(MAX_OIDS_PER_REQUEST) {
        let oids: Vec<Oid> = chunk.iter().map(|item| item.oid.clone()).collect();
        match client.get_many(&oids).await {
            Ok(response) => {
                for vb in response.varbinds {
                    if !vb.value.is_exception() {
                        values.insert(vb.oid, vb.value);
                        responded = true;
                    }
                }
            }
            Err(e) => {
                failed += 1;
                tracing::debug!(
                    "SNMP 实时分块 GET 失败（{} 个 OID）: {}",
                    chunk.len(),
                    crate::core::format_snmp_error(e)
                );
            }
        }
    }
    (values, responded, failed)
}

fn raw_temp(value: &Value) -> Option<f64> {
    value
        .as_i32()
        .map(f64::from)
        .or_else(|| value.as_u64().map(|v| v as f64))
}

fn usage_pct(total: u64, used: u64) -> Option<f64> {
    (total > 0).then(|| (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
}

/// 实体表内存聚合：在「有使用率」的实体中选缓存总量最大者（主控板），
/// 用总量 × 百分比换算已用字节；全部实体都无总量时只报最大使用率。
fn entity_mem_totals(
    entities: &[EntityMem],
    usage: &BTreeMap<usize, f64>,
) -> (Option<u64>, Option<u64>, Option<f64>) {
    let chosen = entities
        .iter()
        .enumerate()
        .filter_map(|(i, entity)| usage.get(&i).map(|pct| (entity.total_bytes, *pct)))
        .max_by_key(|(total, _)| total.unwrap_or(0));
    match chosen {
        Some((Some(total), pct)) => {
            let used = (total as f64 * pct / 100.0).round() as u64;
            (Some(total), Some(used), Some(pct))
        }
        Some((None, pct)) => (None, None, Some(pct)),
        None => (None, None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::{PortMeta, RawSource, TempSource, TempUnit, VendorKind};

    fn source(arc: u32, index: u32) -> RawSource {
        RawSource::indexed(
            Oid::parse(&format!("1.3.6.1.4.1.9.9.999.{arc}")).unwrap(),
            index,
        )
    }

    fn profile(mem: MemPlan, temp: Vec<TempSource>) -> DeviceProfile {
        DeviceProfile {
            vendor: VendorKind::Cisco,
            cpu: vec![source(1, 1)],
            mem,
            temp,
            ports: vec![PortMeta {
                if_index: 1,
                name: "Gi0/1".to_string(),
                speed_bps: Some(1_000_000_000),
            }],
            use_hc: true,
        }
    }

    #[test]
    fn 内存方案_实体百分比选择主控并换算字节() {
        let entities = vec![
            EntityMem {
                usage: source(2, 1),
                total_bytes: Some(1_000_000_000),
            },
            EntityMem {
                usage: source(2, 2),
                total_bytes: Some(2_000_000),
            },
        ];
        let mut usage = BTreeMap::new();
        usage.insert(0, 20.0);
        usage.insert(1, 90.0);
        assert_eq!(
            entity_mem_totals(&entities, &usage),
            (Some(1_000_000_000), Some(200_000_000), Some(20.0))
        );
    }

    #[test]
    fn 内存方案_实体无总量时只报使用率() {
        let entities = vec![EntityMem {
            usage: source(2, 1),
            total_bytes: None,
        }];
        let mut usage = BTreeMap::new();
        usage.insert(0, 42.0);
        assert_eq!(
            entity_mem_totals(&entities, &usage),
            (None, None, Some(42.0))
        );
        assert_eq!(
            entity_mem_totals(&entities, &BTreeMap::new()),
            (None, None, None)
        );
    }

    #[test]
    fn 内存使用率_钳制与零总量() {
        assert_eq!(usage_pct(100, 50), Some(50.0));
        assert_eq!(usage_pct(0, 0), None);
        assert_eq!(usage_pct(100, 200), Some(100.0));
    }

    #[test]
    fn 温度换算_毫与实体单位() {
        assert_eq!(apply_temp_unit(45000.0, TempUnit::MilliCelsius), Some(45.0));
        let src = TempSource {
            source: source(3, 1),
            unit: TempUnit::EntitySensor {
                scale: 8,
                precision: 0,
            },
        };
        let collector = LiveCollector::new(profile(MemPlan::None, vec![src]));
        assert_eq!(collector.profile().temp.len(), 1);
    }
}
