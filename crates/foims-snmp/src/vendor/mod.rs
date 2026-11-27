//! 厂商/平台识别域：识别规则与私有采集 OID 集的静态注册表 + 识别纯函数。
//!
//! 设计（docs/SNMP厂商MIB识别与绑定方案.md §5）：
//! - 本域只放「规则与 OID 数据 + 识别纯函数」，采集逻辑（probe/discover_*）
//!   留在 [`crate::live::profile`]；
//! - 识别规则数据**全部出自生成脚本**（community/ 目录：内置品牌级 16 条 +
//!   社区 OS 级，勿手改）；采集 OID 库独立分层（profiles.rs，TOML 数据
//!   驱动：内置默认 profiles_default.toml + /etc/foims/vendor_profiles.toml
//!   运行时覆盖/扩展，人工核实无结构化上游），识别与采集经 vendor_kind()
//!   与 profiles 按 OS id 关联（§5.2）；
//! - 识别证据：sysObjectID 前缀最长匹配（LibreNMS 语义）+ sysDescr
//!   关键词/正则 + 主动探测（probe.rs）；平台（Linux/Windows/BSD…）与
//!   硬件厂商严格分层：平台 agent 企业号（8072/311/12325/30155 等）不作
//!   为厂商证据（§3）；
//! - 规则数据来源（§5.3，按需拉取不整目录拷贝）：IANA enterprise-numbers
//!   （企业号唯一权威）、SNMP::Info 企业号映射表、本地外部 MIB 资料池查证。
//!
//! 运行时零 MIB 依赖：采集是数字 OID 定向 GET，MIB 文件仅开发期查证用。

mod community;
mod discovery;
pub mod identify;
mod os_discovery;
pub mod platform;
pub mod probe;
pub mod profiles;

use crate::live::VendorKind;

// 识别纯函数在 vendor 根重导出：调用方（profile/sysinfo）统一走
// `vendor::identify(...)`，无需感知子模块结构
pub use identify::identify;
// LibreNMS os_discovery 采集规则（CPU/内存/温度）按识别出的 OS id 查表；
// 调用方与品牌级 OID 集按「OS 级规则 → 品牌库 → 公共链」顺序回退
pub use discovery::{CpuRule, DiscoveryProfile, MemRule, OidColumn, TempRule, discovery_profile};

/// 识别置信度：High = 双证据一致（企业号 + sysDescr 同厂商且未命中排除词），
/// 可驱动 detected 自动绑定；Low = 单证据/冲突/排除词命中，仅作建议（§6.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    High,
    Low,
}

/// 厂商识别规则。
pub struct VendorRule {
    pub id: &'static str,
    pub display_name: &'static str,
    /// 品牌别名（小写）：手填 brand 匹配与 sysDescr 识别共用一张表（§6.2）
    pub aliases: &'static [&'static str],
    /// sysObjectID 前缀（至少 7 弧=企业号级；OS 级规则带更深弧区分同企业号
    /// 产品线，LibreNMS 语义）。识别按最长前缀胜出；前缀第 7 弧为平台
    /// agent 号（8072 net-snmp 等）的由 identify 排除，不作厂商证据（§3）。
    /// 企业号级取值来源：IANA enterprise-numbers / SNMP::Info 映射表（已核实）
    pub sysobjectid_prefixes: &'static [&'static [u32]],
    /// sysDescr 词边界关键词（小写）：单词词即词元精确匹配，连字符/多词品牌
    /// （tp-link / versatile routing platform）按全词元命中。
    /// 来源：原 sysinfo.rs 词表收编 + 设备实测 sysDescr 样例
    pub descr_keywords: &'static [&'static str],
    /// sysDescr 排除关键词：命中即证据降级为 Low（OEM/固件上报他人企业号，
    /// 对应 LibreNMS sysDescr_except 实践）。初始规则暂无已知排除词
    pub descr_keywords_except: &'static [&'static str],
    /// sysDescr 正则关键词（Rust regex 语法——上游 PCRE 的无 look-around
    /// 子集，编译失败的丢弃并记录于 community.rs 生成头）。命中与关键词
    /// 同级记 sysDescr 证据（Low）。仅收正向 sysDescr_regex；except 排除
    /// 正则对无其他证据的 OS 无识别价值，不转换
    pub descr_regexes: &'static [&'static str],
    /// 主动探测检查（LibreNMS snmpget 语义，多条 AND）：前缀/关键词/正则
    /// 全未命中时经 probe 链路 GET 比对（识别期 IO，仅 profile 探测期一次，
    /// 见 vendor/probe.rs）。上游 op=!= 的负条件不转换（几乎所有值都满足，
    /// 无识别价值）
    pub probe_checks: &'static [ProbeCheck],
}

/// 主动探测单条检查：GET `oid` 后按 `op` 比对响应值与 `value`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeCheck {
    pub oid: &'static str,
    pub op: ProbeOp,
    pub value: &'static str,
}

/// 探测值比对方式（对应 LibreNMS snmpget op；无 op 字段默认 Exists）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOp {
    /// OID 有非异常响应即命中
    Exists,
    /// 值包含子串
    Contains,
    /// 值以前缀开头
    Starts,
    /// 值完全相等
    Equals,
    /// 值匹配正则（Rust regex 语法）
    Regex,
}

/// 厂商库条目：识别规则引用（识别-only 品牌无采集库，§5.3.3）。
/// 规则以 `&'static` 引用存放：识别规则数据全部在 community/ 生成目录
///（内置品牌级 builtin.rs + 社区 OS 级 os_*.rs）。私有采集 OID 库已
/// profile 化（[`profiles`]：内置默认 + /etc 配置文件），与识别规则解耦，
/// 经 [`profiles::registry`] 按 OS id 关联。
pub struct VendorMib {
    pub rule: &'static VendorRule,
}

/// 识别结果（read-only，每次探测可产出）。
pub struct SnmpIdentity {
    /// 平台规则 id（linux/windows/…，Unknown 为 None）
    pub platform: Option<&'static str>,
    /// 内核/系统版本（展示用，示例格式需实测校准）
    pub os_version: Option<String>,
    /// 厂商库 id（识别-only 品牌也可能命中）
    pub vendor_guess: Option<&'static str>,
    pub confidence: Confidence,
    /// 采集能力声明：probe 按此跳过已知缺失的 MIB 族（§3.2）
    pub caps: platform::PlatformCaps,
    /// 证据说明（UI 展示 + 调试）
    pub evidence: Vec<String>,
}

/// 全部识别规则：内置品牌级（含三家采集库）+ 社区 OS 级
///（[community] 模块，LibreNMS os_detection 全量转换）。
/// 顺序即 sysDescr 识别优先级：内置品牌级在前（词表更精、判定优先），
/// 社区 OS 级在后（前缀匹配不受顺序影响，最长前缀胜出）。
pub fn registry() -> impl Iterator<Item = &'static VendorMib> {
    community::COMMUNITY.iter().flat_map(|g| g.iter())
}

/// 手填品牌 → 厂商库匹配（display_name/aliases/id 大小写不敏感，
/// §6.2 manual 绑定入口；未命中 → None，属合法自定义品牌）。
pub fn find_by_brand(brand: &str) -> Option<&'static VendorMib> {
    let key = brand.trim().to_lowercase();
    if key.is_empty() {
        return None;
    }
    registry().find(|m| {
        m.rule.id == key
            || m.rule.display_name.to_lowercase() == key
            || m.rule.aliases.iter().any(|a| *a == key)
    })
}

/// 规则 id → 采集计划区分键（无采集库或未知 id → Generic 公共回退链）。
/// 映射数据 KIND_BY_OS 由生成脚本按上游 group 产出（cisco 组全产品线绑定
/// Cisco MIB 家族，探测失败自动回退公共链，激进绑定安全；华为仅 VRP 网络
/// 设备——oceanstor/smartax 等产品线走 Generic；新华三 Comware），见
/// community/mod.rs。
pub fn vendor_kind(id: Option<&str>) -> VendorKind {
    id.and_then(|id| community::KIND_BY_OS.iter().find(|(os, _)| *os == id))
        .map_or(VendorKind::Generic, |(_, kind)| *kind)
}

/// 规则 id → 展示名（snmp-info brand 口径，未知 → "Unknown"，
/// 兼容原 sysinfo.rs 历史取值如 "HP/HPE"）。
pub fn display_name(id: Option<&str>) -> &'static str {
    id.and_then(|id| registry().find(|m| m.rule.id == id))
        .map_or("Unknown", |m| m.rule.display_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 品牌匹配_大小写不敏感含别名() {
        assert_eq!(find_by_brand("Cisco").map(|m| m.rule.id), Some("cisco"));
        assert_eq!(find_by_brand("CISCO").map(|m| m.rule.id), Some("cisco"));
        assert_eq!(find_by_brand("hp/hpe").map(|m| m.rule.id), Some("hp"));
        assert_eq!(find_by_brand("HPE").map(|m| m.rule.id), Some("hp"));
        assert_eq!(find_by_brand("ProCurve").map(|m| m.rule.id), Some("hp"));
        assert_eq!(find_by_brand("TP-Link").map(|m| m.rule.id), Some("tp-link"));
        assert_eq!(find_by_brand("  H3C ").map(|m| m.rule.id), Some("h3c"));
        assert_eq!(find_by_brand("3Com").map(|m| m.rule.id), Some("h3c"));
        // 未命中 → 合法自定义品牌，不报错（§7.2）
        assert!(find_by_brand("MyBrand").is_none());
        assert!(find_by_brand("").is_none());
    }

    #[test]
    fn 厂商映射_kind与oid集() {
        assert_eq!(vendor_kind(Some("cisco")), VendorKind::Cisco);
        assert_eq!(vendor_kind(Some("huawei")), VendorKind::Huawei);
        assert_eq!(vendor_kind(Some("h3c")), VendorKind::H3c);
        // 识别-only 品牌与未知 id：展示归展示，采集走 Generic
        assert_eq!(vendor_kind(Some("juniper")), VendorKind::Generic);
        assert_eq!(vendor_kind(None), VendorKind::Generic);
        // 采集库 profile 化：内置三家经 profiles 注册表按 OS id 绑定
        assert!(profiles::registry().builtin(Some("cisco")).is_some());
        assert!(profiles::registry().builtin(Some("huawei")).is_some());
        assert!(profiles::registry().builtin(Some("h3c")).is_some());
        assert!(profiles::registry().builtin(Some("juniper")).is_none());
        assert!(profiles::registry().builtin(None).is_none());
        assert_eq!(display_name(Some("hp")), "HP/HPE");
        assert_eq!(display_name(Some("unknown-brand")), "Unknown");
        assert_eq!(display_name(None), "Unknown");
    }
}
