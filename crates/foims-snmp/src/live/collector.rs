//! 定向 GET 采集器：按探测期缓存的索引逐轮拉取状态/流量，资源组经
//! [`ResourceReader`] 解释 profile 采集计划，输出 [`LiveSnapshot`]。
//!
//! 端口/uptime 组单请求按 [`MAX_OIDS_PER_REQUEST`] 分块后并发发送；任一分块
//! 失败只丢该块数据（`partial`），全部分块失败才把 `responded` 置 false 交给
//! 上层判定离线/退避。端口索引漂移（全部端口状态缺失）由上层触发重新探测。

use std::collections::{BTreeMap, HashMap};

use async_snmp::transport::UdpHandle;
use async_snmp::{Client, Oid, Value, oid};
use chrono::{DateTime, Utc};
use futures_util::future::join_all;

use super::{
    DeviceProfile, LiveCounters, LivePortSnapshot, LiveSampleSpec, LiveSnapshot,
    MAX_OIDS_PER_REQUEST, PortCounters, PortMeta,
};
use crate::collect::metrics::iface_rate_bps;
use crate::collect::resource::ResourceReader;

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
}

/// 单设备实时采集器：持有设备画像、采集端口与上一轮计数器状态。
pub struct LiveCollector {
    profile: DeviceProfile,
    /// 流量采集端口：配置端口未命中时回落探测结果首端口
    target: Option<PortMeta>,
    prev: Option<LiveCounters>,
}

impl LiveCollector {
    pub fn new(profile: DeviceProfile, monitor_if: Option<&str>) -> Self {
        let target = resolve_target(&profile.ports, monitor_if);
        Self {
            profile,
            target,
            prev: None,
        }
    }

    pub fn profile(&self) -> &DeviceProfile {
        &self.profile
    }

    /// 采集一轮：按 `spec` 选择端口状态/流量，资源组（CPU/内存/温度）
    /// 经 ResourceReader 解释（候选/索引缓存跨轮复用），保证快照完整。
    pub async fn sample(
        &mut self,
        client: &Client<UdpHandle>,
        now: DateTime<Utc>,
        spec: LiveSampleSpec,
    ) -> LiveSnapshot {
        let items = self.build_items(spec);
        let (values, responded, failed_batches) = fetch(client, &items).await;
        let now_unix = now.timestamp();
        let elapsed = self.prev.as_ref().map(|p| now_unix - p.ts).unwrap_or(0);

        let mut port_admin: BTreeMap<u32, bool> = BTreeMap::new();
        let mut port_oper: BTreeMap<u32, bool> = BTreeMap::new();
        let mut port_in: BTreeMap<u32, u64> = BTreeMap::new();
        let mut port_out: BTreeMap<u32, u64> = BTreeMap::new();
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
            }
        }

        // 首轮或间隔非正时只更新计数器基线、不出速率
        let prev_ports = self.prev.as_ref().map(|p| &p.ports);
        let target_index = self.target.as_ref().map(|port| port.if_index);
        let mut next_ports: BTreeMap<u32, PortCounters> = BTreeMap::new();
        let mut ports = Vec::with_capacity(self.profile.ports.len());
        let mut rx_total = None::<f64>;
        let mut tx_total = None::<f64>;
        for meta in &self.profile.ports {
            // 流量只在采集端口上计算；其余端口仅保留运行状态
            let is_target = Some(meta.if_index) == target_index;
            let (rx_bps, tx_bps) = if is_target && spec.traffic {
                match (
                    prev_ports.and_then(|m| m.get(&meta.if_index)),
                    port_in.get(&meta.if_index).copied(),
                    port_out.get(&meta.if_index).copied(),
                ) {
                    (Some(p), Some(ci), Some(co)) if elapsed > 0 => (
                        iface_rate_bps(p.in_octets, ci, elapsed, meta.speed_bps),
                        iface_rate_bps(p.out_octets, co, elapsed, meta.speed_bps),
                    ),
                    _ => (None, None),
                }
            } else {
                (None, None)
            };
            if is_target && spec.traffic {
                rx_total = rx_bps;
                tx_total = tx_bps;
                if let (Some(ci), Some(co)) = (
                    port_in.get(&meta.if_index).copied(),
                    port_out.get(&meta.if_index).copied(),
                ) {
                    next_ports.insert(
                        meta.if_index,
                        PortCounters {
                            in_octets: ci,
                            out_octets: co,
                        },
                    );
                }
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
        if spec.traffic {
            if !next_ports.is_empty() {
                self.prev = Some(LiveCounters {
                    ts: now_unix,
                    ports: next_ports,
                });
            }
        } else {
            // 未采流量的轮次清空计数器基线，避免下次切回流量模式时用
            // 过期时间差算出异常速率。
            self.prev = None;
        }

        // 资源组：解释器消费 profile 采集计划（候选缓存跨轮复用，首轮 walk、
        // 后续轮定向 GET）。实时路径不传上一轮 CPU 计数器，require_prev
        // 候选自动跳过。
        let identity = &self.profile.identity;
        let reader = ResourceReader::new(
            client,
            identity.plan,
            identity.caps,
            None,
            &self.profile.cache,
        );
        let cpu_usage_pct = reader
            .cpu()
            .await
            .into_iter()
            .map(|r| r.usage_pct)
            .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |m: f64| m.max(v))));
        let (mem_total, mem_used, mem_usage_pct) = match reader.mem().await {
            Some(mem) => (mem.total, mem.used, mem.usage_percent),
            None => (None, None, None),
        };
        let temperature_c = reader
            .temp()
            .await
            .into_iter()
            .map(|t| t.celsius)
            .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |m: f64| m.max(v))));

        LiveSnapshot {
            collected_at: now,
            brand: identity.brand.to_string(),
            cpu_usage_pct,
            mem_total,
            mem_used,
            mem_usage_pct,
            temperature_c,
            uptime_secs,
            monitor_if: self.target.as_ref().map(|port| port.name.clone()),
            ports,
            rx_bps: rx_total,
            tx_bps: tx_total,
            responded,
            partial: failed_batches > 0,
        }
    }

    /// 构造本轮读取项清单（顺序固定：uptime → 端口状态 → 采集端口流量）。
    /// 资源组 OID 不在此清单——候选选择由解释器在采集轮内完成。
    fn build_items(&self, spec: LiveSampleSpec) -> Vec<ReadItem> {
        let target_index = self.target.as_ref().map(|port| port.if_index);
        // 端口状态 2×N + 采集端口 2（64 位计数器）+ uptime
        let mut items = Vec::with_capacity(4 + self.profile.ports.len() * 2);
        items.push(ReadItem {
            oid: oid!(1, 3, 6, 1, 2, 1, 1, 3, 0),
            kind: ReadKind::Uptime,
        });
        for port in &self.profile.ports {
            let idx = port.if_index;
            if spec.ports {
                items.push(ReadItem {
                    oid: oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 7).child(idx),
                    kind: ReadKind::Admin(idx),
                });
                items.push(ReadItem {
                    oid: oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 8).child(idx),
                    kind: ReadKind::Oper(idx),
                });
            }
            if !spec.traffic || Some(idx) != target_index {
                continue;
            }
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
        items
    }
}

/// 分块 GET：返回（OID → 值, 是否收到有效值, 失败分块数）。
///
/// 分块之间并发下发（各块 OID 不相交，响应靠 request-id 匹配）：串行
/// 会让多块设备的 RTT 叠加到秒级，直接拖坐采样节拍。`responded` 只统计
/// 非异常 varbind：设备对整个请求回 noSuchObject 时视为无可用数据，由上
/// 层判定不可达，避免用空快照覆盖上一轮成功数据。
async fn fetch(
    client: &Client<UdpHandle>,
    items: &[ReadItem],
) -> (HashMap<Oid, Value>, bool, usize) {
    let batches = join_all(items.chunks(MAX_OIDS_PER_REQUEST).map(|chunk| {
        let oids: Vec<Oid> = chunk.iter().map(|item| item.oid.clone()).collect();
        let len = oids.len();
        async move { (len, client.get_many(&oids).await) }
    }))
    .await;

    let mut values = HashMap::with_capacity(items.len());
    let mut responded = false;
    let mut failed = 0usize;
    for (len, result) in batches {
        match result {
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
                    "SNMP 实时分块 GET 失败（{len} 个 OID）: {}",
                    crate::core::format_snmp_error(e).log_string()
                );
            }
        }
    }
    (values, responded, failed)
}

/// 解析采集端口：精确名 → 忽略大小写 → 去分隔符归一；均未命中回落首端口。
fn resolve_target(ports: &[PortMeta], monitor_if: Option<&str>) -> Option<PortMeta> {
    let Some(name) = monitor_if.map(str::trim).filter(|name| !name.is_empty()) else {
        return ports.first().cloned();
    };
    if let Some(hit) = ports.iter().find(|port| port.name == name) {
        return Some(hit.clone());
    }
    if let Some(hit) = ports
        .iter()
        .find(|port| port.name.eq_ignore_ascii_case(name))
    {
        return Some(hit.clone());
    }
    let key = normalize_port_name(name);
    if let Some(hit) = ports
        .iter()
        .find(|port| normalize_port_name(&port.name) == key)
    {
        return Some(hit.clone());
    }
    tracing::debug!("SNMP 实时采集端口 {name} 未在探测结果中命中，回落首端口");
    ports.first().cloned()
}

/// 端口名归一：仅保留字母数字并转小写（"Gi0/1" 与 "gi0-1" 等价）。
fn normalize_port_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 采集端口_配置命中与回落() {
        let ports = vec![
            PortMeta {
                if_index: 1,
                name: "Gi0/1".to_string(),
                speed_bps: None,
            },
            PortMeta {
                if_index: 2,
                name: "Gi0/2".to_string(),
                speed_bps: None,
            },
        ];
        assert_eq!(
            resolve_target(&ports, Some("gi0/2")).map(|port| port.if_index),
            Some(2)
        );
        // 去分隔符归一后命中首端口（Gi0-1 与 Gi0/1 等价）
        assert_eq!(
            resolve_target(&ports, Some("Gi0-1")).map(|port| port.if_index),
            Some(1)
        );
        // 未命中回落首端口；空清单无目标
        assert_eq!(
            resolve_target(&ports, Some("不存在")).map(|port| port.if_index),
            Some(1)
        );
        assert!(resolve_target(&[], Some("Gi0/1")).is_none());
    }
}
