//! 精选静态 MIB-II 对照表（主机监控调试用，不入库、不触网）。
//!
//! 仅收录机房排障高频符号：system 组标量、ifTable/ifXTable 常见列、
//! hrStorage 主机资源组、常见 enterprise 前缀（cisco/huawei/h3c）与前 6 大
//! 组织根。非全量 MIB 编译——命中即标注符号名，未命中原样返回 OID。

use foims_models::MibEntry;

/// (OID, 符号名, 中文简述)。OID 不带前导点。
const MIB_TABLE: &[(&str, &str, &str)] = &[
    // 组织根
    ("1", "iso", "ISO 国际标准化组织"),
    ("1.3", "iso.org.dod", "美国国防部互联网"),
    ("1.3.6.1", "ietf", "IETF 标准管理信息库根（internet）"),
    ("1.3.6.1.4.1", "enterprises", "厂商私有 MIB 根"),
    // 厂商 enterprise 前缀
    ("1.3.6.1.4.1.9", "cisco", "Cisco 私有 MIB"),
    ("1.3.6.1.4.1.2011", "huawei", "华为私有 MIB"),
    ("1.3.6.1.4.1.25506", "h3c", "新华三 H3C 私有 MIB"),
    // MIB-II system 组（1.3.6.1.2.1.1）
    ("1.3.6.1.2.1.1.1", "sysDescr", "设备系统描述"),
    ("1.3.6.1.2.1.1.2", "sysObjectID", "设备厂商 enterprise OID"),
    ("1.3.6.1.2.1.1.3", "sysUpTime", "自上次重新初始化以来的时间滴"),
    ("1.3.6.1.2.1.1.4", "sysContact", "设备责任人文本"),
    ("1.3.6.1.2.1.1.5", "sysName", "设备名（主机名）"),
    ("1.3.6.1.2.1.1.6", "sysLocation", "设备物理位置"),
    ("1.3.6.1.2.1.1.7", "sysServices", "设备提供的服务协议套件位图"),
    // MIB-II interfaces（ifTable 1.3.6.1.2.1.2）
    ("1.3.6.1.2.1.2.1", "ifNumber", "网络接口数量"),
    ("1.3.6.1.2.1.2.2.1.1", "ifIndex", "接口索引"),
    ("1.3.6.1.2.1.2.2.1.2", "ifDescr", "接口描述"),
    ("1.3.6.1.2.1.2.2.1.3", "ifType", "接口类型"),
    ("1.3.6.1.2.1.2.2.1.4", "ifMtu", "接口最大传输单元"),
    ("1.3.6.1.2.1.2.2.1.5", "ifSpeed", "接口比特率（32 位）"),
    ("1.3.6.1.2.1.2.2.1.6", "ifPhysAddress", "接口物理（MAC）地址"),
    ("1.3.6.1.2.1.2.2.1.7", "ifAdminStatus", "接口期望状态"),
    ("1.3.6.1.2.1.2.2.1.8", "ifOperStatus", "接口当前运行状态"),
    ("1.3.6.1.2.1.2.2.1.10", "ifInOctets", "接口接收字节数"),
    ("1.3.6.1.2.1.2.2.1.16", "ifOutOctets", "接口发送字节数"),
    // ifXTable（1.3.6.1.2.1.31.1.1）64 位计数器与名
    ("1.3.6.1.2.1.31.1.1.1.1", "ifName", "接口名（可读）"),
    ("1.3.6.1.2.1.31.1.1.1.6", "ifHCInOctets", "接口接收字节数（64 位）"),
    ("1.3.6.1.2.1.31.1.1.1.10", "ifHCOutOctets", "接口发送字节数（64 位）"),
    // HOST-RESOURCES-MIB hrStorageTable（1.3.6.1.2.1.25.2.3）
    ("1.3.6.1.2.1.25.2.1", "hrStorageLastChanged", "hrStorageTable 最近变更时间"),
    ("1.3.6.1.2.1.25.2.3.1.2", "hrStorageDescr", "存储单元描述"),
    ("1.3.6.1.2.1.25.2.3.1.3", "hrStorageAllocationUnits", "存储分配单元大小"),
    ("1.3.6.1.2.1.25.2.3.1.5", "hrStorageSize", "存储总量（分配单元数）"),
    ("1.3.6.1.2.1.25.2.3.1.6", "hrStorageUsed", "存储已用（分配单元数）"),
    // HOST-RESOURCES-MIB 系统组（1.3.6.1.2.1.25.1）
    ("1.3.6.1.2.1.25.1.1", "hrSystemUptime", "主机系统运行时间"),
    ("1.3.6.1.2.1.25.3.2.1.3", "hrDeviceDescr", "设备描述（CPU/内存等）"),
    // UCD-SNMP 负载与 CPU（1.3.6.1.4.1.2021）
    ("1.3.6.1.4.1.2021.10.1.3.1", "laLoad1", "1 分钟系统平均负载"),
    ("1.3.6.1.4.1.2021.11.11.0", "ssCpuIdle", "UCD CPU 空闲百分比"),
    // ENTITY 传感器（1.3.6.1.2.1.99）
    ("1.3.6.1.2.1.99.1.1.1.2", "entSensorValue", "实体传感器读数"),
    // BRIDGE-MIB dot1dBase（1.3.6.1.2.1.17）
    ("1.3.6.1.2.1.17.1.4.1", "dot1dBasePortTable", "桥接端口表"),
];

/// 归一查询：去首尾空白与点、转小写（用于名称匹配），返回处理后的文本。
fn normalize_query(query: &str) -> String {
    query.trim().trim_matches('.').to_ascii_lowercase()
}

/// 是否形如 OID（仅数字与点，且至少含一个数字）。
fn looks_like_oid(query: &str) -> bool {
    let stripped = query.trim().trim_start_matches('.');
    !stripped.is_empty()
        && stripped.chars().all(|c| c.is_ascii_digit() || c == '.')
        && stripped.chars().any(|c| c.is_ascii_digit())
}

/// 双向对照查询：OID→名称（点分相等或作为前缀命中子表）或名称→OID（子串匹配）。
///
/// 空查询返回空列表；无匹配同样返回空列表（前端展示"未命中"）。
#[must_use]
pub fn lookup(query: &str) -> Vec<MibEntry> {
    let q = normalize_query(query);
    if q.is_empty() {
        return Vec::new();
    }
    MIB_TABLE
        .iter()
        .filter_map(|(oid, name, desc)| {
            let matched = if looks_like_oid(&q) {
                // OID 查询：等值命中，或表项以「查询.」为前缀（查父节点列出子项）
                q == *oid || oid.starts_with(&format!("{q}."))
            } else {
                // 名称查询：符号名或描述子串匹配
                name.to_ascii_lowercase().contains(&q) || desc.to_lowercase().contains(&q)
            };
            matched.then(|| MibEntry {
                oid: (*oid).to_string(),
                name: (*name).to_string(),
                description: (*desc).to_string(),
            })
        })
        .collect()
}

/// 反查：给定 OID 文本（可带前导点），命中表项则返回符号名，否则空串。
///
/// 供 SNMP GET/WALK 结果标注符号名（点分精确匹配）。
#[must_use]
pub fn name_of(oid: &str) -> String {
    let stripped = oid.trim().trim_start_matches('.');
    MIB_TABLE
        .iter()
        .find(|(table_oid, _, _)| *table_oid == stripped)
        .map(|(_, name, _)| (*name).to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_by_oid_exact_and_prefix() {
        let exact = lookup("1.3.6.1.2.1.1.5");
        assert_eq!(exact.len(), 1);
        assert_eq!(exact.first().map(|e| e.name.as_str()), Some("sysName"));

        // 前缀查父节点列出子项
        let system = lookup("1.3.6.1.2.1.1");
        assert!(system.iter().any(|e| e.name == "sysDescr"));
        assert!(system.iter().any(|e| e.name == "sysName"));
    }

    #[test]
    fn lookup_by_name_case_insensitive_substring() {
        let hits = lookup("SYSNAME");
        assert!(hits.iter().any(|e| e.name == "sysName"));
        let cn = lookup("负载");
        assert!(cn.iter().any(|e| e.name == "laLoad1"));
    }

    #[test]
    fn lookup_empty_or_unknown_returns_empty() {
        assert!(lookup("").is_empty());
        assert!(lookup("   ").is_empty());
        assert!(lookup("不存在的符号xyz").is_empty());
    }

    #[test]
    fn name_of_strips_leading_dot() {
        assert_eq!(name_of(".1.3.6.1.2.1.1.1"), "sysDescr");
        assert_eq!(name_of("1.3.6.1.2.1.1.1"), "sysDescr");
        assert_eq!(name_of("1.2.3.4.999"), "");
    }
}
