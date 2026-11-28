//! 实时探测：识别（vendor::identify）+ 端口清单 + 行索引缓存初始化。
//!
//! 按 docs/SNMP模块重构方案.md §5.1，探测不再包含任何资源组 OID 知识：
//! 识别返回 [`crate::vendor::Identity`]（含解析后的采集计划与平台能力），
//! 资源组候选在采集轮由 [`crate::collect::resource::ResourceReader`] 解释，
//! 首轮 walk 的候选/行索引写入 [`DeviceProfile::cache`] 跨轮复用。
//!
//! 探测失败（系统组无响应）返回 [`ProbeError`]；资源组候选缺失只降级为
//! 空读数，不影响设备纳入实时采集。IO 经 [`crate::core::SnmpReader`]
//! 抽象：生产为 async_snmp UDP 客户端，测试为内存 OID 快照。

use std::collections::BTreeMap;

use async_snmp::{Oid, Value, oid};
use thiserror::Error;

use super::{DeviceProfile, MAX_LIVE_PORTS, PortMeta};
use crate::collect::resource::IndexCache;
use crate::core::{SnmpReader, probe_oid_values};
use crate::vendor;

/// 探测失败：系统组（sysObjectID/sysDescr）无响应，设备不可达。
#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("设备系统组无响应: {0}")]
    SystemGroup(String),
}

// OID 常量（公共 MIB；资源组 OID 全部由 profile 注册表声明）

fn sys_object_id() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 1, 2, 0)
}

fn sys_descr() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 1, 1, 0)
}

// IF-MIB（ifTable 与 ifXTable 列）
fn if_name() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, 1)
}
fn if_descr() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 2)
}
fn if_type() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 3)
}
fn if_high_speed() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, 15)
}
fn if_speed() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 5)
}
fn if_hc_in() -> Oid {
    oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, 6)
}

// 探测入口

/// 探测设备实时画像：系统组 GET + 识别 + 端口清单 walk。
///
/// 只读、无副作用；建议每 30 分钟或索引失效时重新调用一次。
pub async fn probe<R: SnmpReader>(reader: &R) -> Result<DeviceProfile, ProbeError> {
    // 系统组两个标量分别 GET：SNMPv1 下任一 OID 不存在会让整包 GET 报错，
    // 分开请求可保留另一字段继续识别；两者都无响应才判定不可达。
    let object_id = get_one(reader, sys_object_id())
        .await
        .and_then(|value| value.as_oid().cloned());
    let descr_value = get_one(reader, sys_descr()).await;
    let descr = descr_value
        .as_ref()
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if object_id.is_none() && descr.is_none() {
        return Err(ProbeError::SystemGroup("系统组 GET 无有效值".to_string()));
    }

    // 识别：先静态双通道（前缀/关键词/正则）；全未命中且注册表存在探测
    // 规则时，预取探测 OID 值后走探测通道（LibreNMS snmpget 语义，
    // 每探测周期仅此一次 GET 批）
    let mut identity = vendor::identify(object_id.as_ref(), descr, None);
    if identity.profile.id == "generic" {
        let probe_oids = vendor::registry().probe_oids();
        if !probe_oids.is_empty() {
            let mut oids = Vec::with_capacity(probe_oids.len());
            for s in probe_oids {
                match Oid::parse(s) {
                    Ok(parsed) => oids.push(parsed),
                    Err(e) => panic!("SNMP 探测 OID 解析失败（注册表不变量破坏）: {s}: {e}"),
                }
            }
            let values = probe_oid_values(reader, &oids).await;            let lookup = |oid: &str| values.get(oid).cloned();
            let probed = vendor::identify(object_id.as_ref(), descr, Some(&lookup));
            if probed.profile.id != "generic" {
                tracing::debug!("SNMP 探测通道识别 profile: {}", probed.profile.id);
                identity = probed;
            }
        }
    }

    let ports = discover_ports(reader).await;
    let use_hc = match ports.first() {
        Some(port) => match reader.get_many(&[if_hc_in().child(port.if_index)]).await {
            Ok(vbs) => vbs.first().is_some_and(|vb| !vb.value.is_exception()),
            Err(_) => false,
        },
        None => false,
    };

    tracing::debug!(
        "SNMP 实时探测完成: profile={}, brand={}, platform={}, os_version={}, ports={}",
        identity.profile.id,
        identity.brand,
        identity.platform.unwrap_or("unknown"),
        identity.os_version.as_deref().unwrap_or("-"),
        ports.len()
    );

    Ok(DeviceProfile {
        identity,
        cache: IndexCache::new(),
        ports,
        use_hc,
    })
}

// 端口清单

/// 物理以太网 + 聚合口类型（ethernetCsmacd/fastEthernet/gigabitEthernet/ieee8023adLag）。
const LIVE_IF_TYPES: [u32; 4] = [6, 62, 117, 161];

/// 端口静态清单：ifName/ifDescr、ifType、ifHighSpeed/ifSpeed。
async fn discover_ports<R: SnmpReader>(reader: &R) -> Vec<PortMeta> {
    let names = walk_column(reader, if_name()).await;
    let descrs = walk_column(reader, if_descr()).await;
    let types = walk_column(reader, if_type()).await;
    let high_speeds = walk_column(reader, if_high_speed()).await;
    let speeds = walk_column(reader, if_speed()).await;

    let mut indexes: Vec<u32> = names
        .keys()
        .chain(descrs.keys())
        .copied()
        .filter(|idx| {
            types
                .get(idx)
                .and_then(Value::as_u32)
                .is_some_and(|t| LIVE_IF_TYPES.contains(&t))
        })
        .collect();
    indexes.sort_unstable();
    indexes.dedup();

    let mut ports: Vec<PortMeta> = indexes
        .into_iter()
        .filter_map(|if_index| {
            let name = names
                .get(&if_index)
                .or_else(|| descrs.get(&if_index))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())?
                .to_string();
            let speed_bps = match high_speeds.get(&if_index).and_then(Value::as_u64) {
                Some(mbps) if mbps > 0 => Some(mbps.saturating_mul(1_000_000)),
                _ => speeds.get(&if_index).and_then(Value::as_u64),
            };
            Some(PortMeta {
                if_index,
                name,
                speed_bps,
            })
        })
        .collect();
    if ports.len() > MAX_LIVE_PORTS {
        tracing::debug!(
            "SNMP 实时端口数 {} 超过上限 {}，截断（按 ifIndex 排序）",
            ports.len(),
            MAX_LIVE_PORTS
        );
        ports.truncate(MAX_LIVE_PORTS);
    }
    ports
}

// 工具

/// walk 一列并返回 `行索引 → 值`（异常值跳过）。
pub(crate) async fn walk_column<R: SnmpReader>(reader: &R, base: Oid) -> BTreeMap<u32, Value> {
    let mut map = BTreeMap::new();
    for vb in reader.walk(&base).await {
        if vb.value.is_exception() {
            continue;
        }
        if let Some(&index) = vb.oid.arcs().last() {
            map.insert(index, vb.value);
        }
    }
    map
}

/// 定向 GET 单值（异常值视为缺失）。
pub(crate) async fn get_one<R: SnmpReader>(reader: &R, oid: Oid) -> Option<Value> {
    reader
        .get_many(&[oid])
        .await
        .ok()?
        .into_iter()
        .next()
        .filter(|vb| !vb.value.is_exception())
        .map(|vb| vb.value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_snmp::{Oid, Value, VarBind};

    /// 内存 OID 快照 mock：get 精确匹配，walk 返回子树（含子弧）。
    struct MockSnmpReader {
        data: Vec<(Oid, Value)>,
    }

    impl MockSnmpReader {
        fn new(data: Vec<(Oid, Value)>) -> Self {
            Self { data }
        }
    }

    impl SnmpReader for MockSnmpReader {
        async fn get_many(&self, oids: &[Oid]) -> Result<Vec<VarBind>, String> {
            oids.iter()
                .map(|oid| {
                    self.data
                        .iter()
                        .find(|(o, _)| o == oid)
                        .map(|(o, v)| VarBind::new(o.clone(), v.clone()))
                        .ok_or_else(|| "noSuchName".to_string())
                })
                .collect()
        }

        async fn walk(&self, base: &Oid) -> Vec<VarBind> {
            self.data
                .iter()
                .filter(|(o, _)| o.starts_with(base))
                .map(|(o, v)| VarBind::new(o.clone(), v.clone()))
                .collect()
        }
    }

    fn oid(s: &str) -> Oid {
        Oid::parse(s).unwrap()
    }

    fn snmp_str(s: &str) -> Value {
        Value::from(s)
    }

    #[tokio::test]
    async fn 探测_华为设备_识别与端口() {
        let data = vec![
            (
                oid("1.3.6.1.2.1.1.2.0"),
                Value::ObjectIdentifier(oid("1.3.6.1.4.1.2011.2.23.1")),
            ),
            (
                oid("1.3.6.1.2.1.1.1.0"),
                snmp_str("Huawei S5720 Versatile Routing Platform Software"),
            ),
            // 端口：ifName/ifType/ifHighSpeed
            (oid("1.3.6.1.2.1.31.1.1.1.1.1"), snmp_str("GE0/0/1")),
            (oid("1.3.6.1.2.1.2.2.1.3.1"), Value::Integer(6)),
            (oid("1.3.6.1.2.1.31.1.1.1.15.1"), Value::Gauge32(1000)),
        ];
        let profile = probe(&MockSnmpReader::new(data)).await.unwrap();
        assert_eq!(profile.identity.profile.id, "yunshan");
        assert_eq!(profile.identity.brand, "Huawei");
        assert_eq!(profile.ports.len(), 1);
        assert_eq!(profile.ports[0].name, "GE0/0/1");
        // 识别结果携带解析后的采集计划（huawei 族链 + generic 兜底）
        let cpu = profile.identity.plan.family(crate::vendor::Family::Cpu);
        assert!(!cpu.candidates.is_empty());
    }

    #[tokio::test]
    async fn 探测_未知设备走generic兜底() {
        let data = vec![
            (
                oid("1.3.6.1.2.1.1.2.0"),
                Value::ObjectIdentifier(oid("1.3.6.1.4.1.99999.1")),
            ),
            (oid("1.3.6.1.2.1.1.1.0"), snmp_str("Some Generic Device")),
        ];
        let profile = probe(&MockSnmpReader::new(data)).await.unwrap();
        assert_eq!(profile.identity.profile.id, "generic");
        assert_eq!(profile.identity.brand, "generic");
    }

    #[tokio::test]
    async fn 探测_系统组无响应报错() {
        let result = probe(&MockSnmpReader::new(Vec::new())).await;
        assert!(matches!(result, Err(ProbeError::SystemGroup(_))));
    }

    #[tokio::test]
    async fn 探测_probe通道_预取值裁决() {
        // 静态证据全未命中 → 探测 OID 预取 → 命中 abbups
        let mut data = vec![
            (
                oid("1.3.6.1.2.1.1.2.0"),
                Value::ObjectIdentifier(oid("1.3.6.1.4.1.99998.1")),
            ),
            (oid("1.3.6.1.2.1.1.1.0"), snmp_str("Unknown Agent")),
        ];
        data.push((
            oid("1.3.6.1.4.1.53973.1.1.1.1.1.1.0"),
            snmp_str("ABB UPS agent v1"),
        ));
        let profile = probe(&MockSnmpReader::new(data)).await.unwrap();
        assert_eq!(profile.identity.profile.id, "abbups");
    }

    #[tokio::test]
    async fn 探测_ports截断与去重() {
        // ifName 缺失时回落 ifDescr；ifType 过滤虚拟口
        let data = vec![
            (
                oid("1.3.6.1.2.1.1.2.0"),
                Value::ObjectIdentifier(oid("1.3.6.1.4.1.99999.1")),
            ),
            (oid("1.3.6.1.2.1.1.1.0"), snmp_str("Some Generic Device")),
            (oid("1.3.6.1.2.1.2.2.1.2.1"), snmp_str("GE0/0/1")),
            (oid("1.3.6.1.2.1.2.2.1.2.2"), snmp_str("Vlan1")),
            (oid("1.3.6.1.2.1.2.2.1.3.1"), Value::Integer(6)),
            (oid("1.3.6.1.2.1.2.2.1.3.2"), Value::Integer(131)),
        ];
        let profile = probe(&MockSnmpReader::new(data)).await.unwrap();
        assert_eq!(profile.ports.len(), 1);
        assert_eq!(profile.ports[0].name, "GE0/0/1");
    }
}
