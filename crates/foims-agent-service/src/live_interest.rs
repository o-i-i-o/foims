//! 实时采集订阅登记：把「谁正在看哪些设备」转成每台设备的采样档位。
//!
//! 前端 SSE 连接持有 [`InterestLease`]，每个心跳刷新有效期；租约释放（连接断开）
//! 后自动消失。采集 worker 每轮按 [`LiveInterest::demand_for`] 选择采样范围与间隔，
//! 无人观察的设备回落到低频基线。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use uuid::Uuid;

/// 设备采样档位，数值越大优先级越高。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LiveDemand {
    /// 无人观察：只维持低频资源组采样。
    Baseline = 0,
    /// 仪表盘卡片：需要全设备汇总与流量，但 5 秒级即可。
    Overview = 1,
    /// 主机列表当前页：只更新列表展示的资源组。
    Watch = 2,
    /// 选中设备详情：资源组 + 流量 + 全端口状态。
    Detail = 3,
}

/// 一条订阅覆盖的范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterestScope {
    /// 仪表盘：对全部设备生效。
    Dashboard,
    /// 主机列表：只对列出的设备生效。
    Devices,
    /// 单设备详情。
    Detail(Uuid),
}

struct Entry {
    scope: InterestScope,
    devices: HashSet<Uuid>,
    expires: Instant,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<u64, Entry>,
}

/// 进程内实时订阅表。
pub struct LiveInterest {
    inner: Mutex<Inner>,
    next_id: AtomicU64,
    ttl: Duration,
}

impl LiveInterest {
    /// `ttl` 是租约有效期；SSE 每个心跳调用 [`InterestLease::touch`] 续期。
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            next_id: AtomicU64::new(0),
            ttl: ttl.max(Duration::from_secs(1)),
        }
    }

    /// 登记一条订阅；返回的租约在 drop 时自动注销。
    pub fn lease(
        self: &Arc<Self>,
        scope: InterestScope,
        devices: impl IntoIterator<Item = Uuid>,
    ) -> InterestLease {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let now = Instant::now();
        let mut inner = self.lock();
        inner.entries.retain(|_, entry| entry.expires > now);
        inner.entries.insert(
            id,
            Entry {
                scope,
                devices: devices.into_iter().collect(),
                expires: now + self.ttl,
            },
        );
        InterestLease {
            registry: Arc::clone(self),
            id,
        }
    }

    /// 某台设备当前的最高采样档位。
    pub fn demand_for(&self, device: Uuid) -> LiveDemand {
        let now = Instant::now();
        let inner = self.lock();
        let mut demand = LiveDemand::Baseline;
        for entry in inner.entries.values() {
            if entry.expires <= now {
                continue;
            }
            match &entry.scope {
                InterestScope::Dashboard => demand = demand.max(LiveDemand::Overview),
                InterestScope::Devices => {
                    if entry.devices.contains(&device) {
                        demand = demand.max(LiveDemand::Watch);
                    }
                }
                InterestScope::Detail(id) => {
                    if *id == device {
                        demand = LiveDemand::Detail;
                    }
                }
            }
        }
        demand
    }

    fn touch(&self, id: u64) {
        let mut inner = self.lock();
        if let Some(entry) = inner.entries.get_mut(&id) {
            entry.expires = Instant::now() + self.ttl;
        }
    }

    fn release(&self, id: u64) {
        self.lock().entries.remove(&id);
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }
}

/// SSE 连接持有的订阅租约。
pub struct InterestLease {
    registry: Arc<LiveInterest>,
    id: u64,
}

impl InterestLease {
    /// 心跳续期；连接断开后由 Drop 注销。
    pub fn touch(&self) {
        self.registry.touch(self.id);
    }
}

impl Drop for InterestLease {
    fn drop(&mut self) {
        self.registry.release(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn demand_priority_and_scope() {
        let interest = Arc::new(LiveInterest::new(Duration::from_secs(30)));
        let target = device(1);
        assert_eq!(interest.demand_for(target), LiveDemand::Baseline);

        let dashboard = interest.lease(InterestScope::Dashboard, std::iter::empty::<Uuid>());
        assert_eq!(interest.demand_for(device(2)), LiveDemand::Overview);

        let watch = interest.lease(InterestScope::Devices, [target]);
        assert_eq!(interest.demand_for(target), LiveDemand::Watch);
        assert_eq!(interest.demand_for(device(2)), LiveDemand::Overview);

        let detail = interest.lease(InterestScope::Detail(target), std::iter::empty::<Uuid>());
        assert_eq!(interest.demand_for(target), LiveDemand::Detail);

        drop(detail);
        assert_eq!(interest.demand_for(target), LiveDemand::Watch);
        drop(watch);
        assert_eq!(interest.demand_for(target), LiveDemand::Overview);
        drop(dashboard);
        assert_eq!(interest.demand_for(target), LiveDemand::Baseline);
    }

    #[test]
    fn unreferenced_devices_stay_baseline() {
        let interest = Arc::new(LiveInterest::new(Duration::from_secs(30)));
        let _watch = interest.lease(InterestScope::Devices, [device(1)]);
        assert_eq!(interest.demand_for(device(1)), LiveDemand::Watch);
        assert_eq!(interest.demand_for(device(2)), LiveDemand::Baseline);
    }
}
