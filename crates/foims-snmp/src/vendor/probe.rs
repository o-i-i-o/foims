//! 主动探测识别（LibreNMS snmpget 语义）：sysObjectID 前缀 / sysDescr
//! 关键词与正则全未命中时，对带 [`ProbeCheck`] 的规则做一轮 GET 值比对。
//! 识别期 IO 收敛于此：仅在 profile 探测链路调用（每 30 分钟一次），
//! 响应值经 [`ProbeOp`] 比对（多条检查 AND），首个全命中规则生效。
//! 上游 op=`!=` 的负条件不转换（几乎所有值都满足，无识别价值）。

use std::sync::LazyLock;

use async_snmp::{Oid, Value};
use regex::Regex;

use super::platform::PlatformCaps;
use super::{Confidence, ProbeOp, SnmpIdentity, registry};
use crate::live::{MAX_OIDS_PER_REQUEST, SnmpReader};

/// 全部探测检查的解析缓存（与 registry() 迭代顺序对齐，命中时按索引取回
/// 规则）：OID 解析 + op=Regex 的值正则编译，非法项直接丢弃该检查。
struct CompiledCheck {
    oid: Oid,
    op: ProbeOp,
    value: &'static str,
    regex: Option<Regex>,
}

static PROBES: LazyLock<Vec<(usize, Vec<CompiledCheck>)>> = LazyLock::new(|| {
    registry()
        .enumerate()
        .filter_map(|(idx, m)| {
            let checks: Vec<CompiledCheck> = m
                .rule
                .probe_checks
                .iter()
                .filter_map(|c| {
                    let oid = parse_oid(c.oid)?;
                    let regex = match c.op {
                        ProbeOp::Regex => Some(Regex::new(c.value).ok()?),
                        _ => None,
                    };
                    Some(CompiledCheck {
                        oid,
                        op: c.op,
                        value: c.value,
                        regex,
                    })
                })
                .collect();
            (!checks.is_empty()).then_some((idx, checks))
        })
        .collect()
});

/// 点分数字 OID 字符串 → Oid（识别期一次性解析）。
fn parse_oid(s: &str) -> Option<Oid> {
    let arcs: Vec<u32> = s
        .split('.')
        .filter(|p| !p.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    (!arcs.is_empty()).then(|| Oid::new(arcs))
}

/// 响应值文本化：OctetString 取原文，数值类型转十进制串（上游 snmpget
/// 比对均为字符串语义）。
fn value_text(v: &Value) -> Option<String> {
    if v.is_exception() {
        return None;
    }
    v.as_str()
        .map(str::to_string)
        .or_else(|| v.as_u64().map(|u| u.to_string()))
}

/// 单条检查比对：Exists 只看非异常响应，其余按值文本比对。
fn check_hit(check: &CompiledCheck, v: &Value) -> bool {
    if check.op == ProbeOp::Exists {
        return !v.is_exception();
    }
    value_text(v).is_some_and(|text| match check.op {
        ProbeOp::Contains => text.contains(check.value),
        ProbeOp::Starts => text.starts_with(check.value),
        ProbeOp::Equals => text == check.value,
        ProbeOp::Regex => check.regex.as_ref().is_some_and(|re| re.is_match(&text)),
        ProbeOp::Exists => false,
    })
}

/// 主动探测识别：GET 全部探测 OID（去重、分块 ≤[`MAX_OIDS_PER_REQUEST`]），
/// 逐规则比对全部检查（AND），首个全命中规则合成识别结果（置信度 Low：
/// 探测 op 多为宽松子串匹配，不足以驱动自动绑定）。
/// 请求失败静默放弃（设备可达性已由系统组 GET 证明，此处不再重试）。
pub async fn identify_by_probes<R: SnmpReader>(reader: &R) -> Option<SnmpIdentity> {
    let probes = &*PROBES;
    // 全部探测 OID 去重收集（Vec 线性查重，规模百级足够）
    let mut oids: Vec<Oid> = Vec::new();
    for (_, checks) in probes {
        for c in checks {
            if !oids.contains(&c.oid) {
                oids.push(c.oid.clone());
            }
        }
    }
    let mut values: Vec<(Oid, Value)> = Vec::new();
    for chunk in oids.chunks(MAX_OIDS_PER_REQUEST) {
        let resp = reader.get_many(chunk).await.ok()?;
        values.extend(resp.into_iter().map(|vb| (vb.oid, vb.value)));
    }
    for (idx, checks) in probes {
        let hit = checks.iter().all(|c| {
            values
                .iter()
                .find(|(oid, _)| oid == &c.oid)
                .is_some_and(|(_, v)| check_hit(c, v))
        });
        if hit {
            let m = registry().nth(*idx)?;
            return Some(SnmpIdentity {
                platform: None,
                os_version: None,
                vendor_guess: Some(m.rule.id),
                confidence: Confidence::Low,
                caps: PlatformCaps::ALL,
                evidence: vec![format!(
                    "主动探测命中 {}（{} 项检查）",
                    m.rule.display_name,
                    checks.len()
                )],
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;

    /// 构造 OctetString 响应值（探测比对的主流类型）。
    fn descr(v: &str) -> Value {
        Value::OctetString(Bytes::copy_from_slice(v.as_bytes()))
    }

    /// 构造已编译检查（含 op=Regex 的值正则编译）。
    fn check(oid_s: &str, op: ProbeOp, value: &'static str) -> CompiledCheck {
        CompiledCheck {
            oid: parse_oid(oid_s).unwrap(),
            op,
            value,
            regex: match op {
                ProbeOp::Regex => Some(Regex::new(value).unwrap()),
                _ => None,
            },
        }
    }

    #[test]
    fn parse_oid_仅收点分数字() {
        // 合法点分数字（community.rs 生成的探测 OID 均为此格式）
        let oid = parse_oid("1.3.6.1.4.1.53973.1.1.1.1.1.1.0").unwrap();
        assert_eq!(oid.arcs().last(), Some(&0));
        // MIB 名格式（上游靠 snmptranslate 解析）与空串拒绝
        assert_eq!(parse_oid("SNMPv2-MIB::sysName.0"), None);
        assert_eq!(parse_oid(""), None);
    }

    #[test]
    fn value_text_字符串与数值() {
        assert_eq!(
            value_text(&descr("APC agent v1")).as_deref(),
            Some("APC agent v1")
        );
        // 数值类型转十进制串（上游 snmpget 比对均为字符串语义）
        assert_eq!(value_text(&Value::Gauge32(42)).as_deref(), Some("42"));
        // 异常值无文本（Exists 只看非异常）
        assert_eq!(value_text(&Value::NoSuchObject), None);
    }

    #[test]
    fn check_hit_各op比对语义() {
        // 用例取自 community.rs 实际转换的社区规则
        // abbups：Contains "ABB"
        let contains = check("1.3.6.1.4.1.53973.1.1.1.1.1.1.0", ProbeOp::Contains, "ABB");
        assert!(check_hit(&contains, &descr("ABB UPS agent v1")));
        assert!(!check_hit(&contains, &descr("Schneider agent")));
        // apc-epdu：Equals "APC"（精确比对，前后缀不放宽）
        let equals = check("1.3.6.1.6.3.10.2.1.1.0", ProbeOp::Equals, "APC");
        assert!(check_hit(&equals, &descr("APC")));
        assert!(!check_hit(&equals, &descr("APCX")));
        // asuswrt-merlin：Starts "ASUSWRT-Merlin"
        let starts = check(
            "1.3.6.1.4.1.2021.7890.1.101.1",
            ProbeOp::Starts,
            "ASUSWRT-Merlin",
        );
        assert!(check_hit(&starts, &descr("ASUSWRT-Merlin RT-AX88U")));
        assert!(!check_hit(&starts, &descr("RT-AX88U ASUSWRT-Merlin")));
        // cmm：Exists（非异常即命中，value 不参与）
        let exists = check("1.3.6.1.4.1.161.19.3.4.4.2.0", ProbeOp::Exists, "CMM");
        assert!(check_hit(&exists, &descr("anything")));
        assert!(!check_hit(&exists, &Value::NoSuchObject));
        // Regex：值正则比对
        let regex = check("1.3.6.1.4.1.1.0", ProbeOp::Regex, r"v\d+\.\d+");
        assert!(check_hit(&regex, &descr("fw v2.1")));
        assert!(!check_hit(&regex, &descr("no digits")));
    }

    #[test]
    fn 探测表与生成数据对齐() {
        // community.rs 必须产出可用探测检查（生成口径仅收点分数字 OID），
        // 若为空说明生成口径或规则结构漂移
        assert!(!PROBES.is_empty());
        assert!(PROBES.iter().all(|(_, checks)| !checks.is_empty()));
    }
}
