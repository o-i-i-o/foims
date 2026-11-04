//! 厂商/平台识别域：识别规则与私有采集 OID 集的静态注册表 + 识别纯函数。
//!
//! 设计（docs/SNMP厂商MIB识别与绑定方案.md §5）：
//! - 本域只放「规则与 OID 数据 + 识别纯函数」，采集逻辑（probe/discover_*）
//!   留在 [`crate::live::profile`]；
//! - 识别双证据：sysObjectID 企业号（弱证据，net-snmp 可装在任何 OS）+
//!   sysDescr 词边界关键词（强证据），双证据一致才 High（驱动后续自动绑定）；
//! - 平台（Linux/Windows/BSD…）与硬件厂商严格分层：平台 agent 企业号
//!   （8072/311/12325/30155 等）不作为厂商证据（§3）；
//! - 规则数据来源（§5.3，按需拉取不整目录拷贝）：IANA enterprise-numbers
//!   （企业号唯一权威）、SNMP::Info 企业号映射表、本地外部 MIB 资料池查证；
//!   扩展 = 新增厂商模块文件 + registry 注册 + 单测，不动核心逻辑。
//!
//! 运行时零 MIB 依赖：采集是数字 OID 定向 GET，MIB 文件仅开发期查证用。

mod cisco;
mod community;
mod h3c;
mod huawei;
pub mod identify;
pub mod platform;
pub mod probe;

use async_snmp::Oid;

use crate::live::VendorKind;

// 识别纯函数在 vendor 根重导出：调用方（profile/sysinfo）统一走
// `vendor::identify(...)`，无需感知子模块结构
pub use identify::identify;

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

/// 厂商内存采集 OID 方案参数（探测期据此确定 MemPlan 形态与索引）。
pub enum VendorMemOids {
    /// CISCO-MEMORY-POOL-MIB：name 定位 pool 行，used/free 两列（字节），
    /// total = used + free。
    Pool {
        name: fn() -> Oid,
        used: fn() -> Oid,
        free: fn() -> Oid,
    },
    /// 华为/华三实体表：usage 百分比列 + size 列缓存总量
    ///（优先 MB 口径，字节兜底，单位换算因子随字段）。
    EntityPercent {
        usage: fn() -> Oid,
        size_mega: Option<(fn() -> Oid, u64)>,
        size_bytes: Option<fn() -> Oid>,
    },
}

/// 厂商私有采集 OID 集（仅 CPU/内存/温度三族；端口走公共 IF-MIB）。
pub struct VendorOidSet {
    /// CPU 使用率列（整数百分比，按序尝试，首个非空列生效）
    pub cpu_columns: &'static [fn() -> Oid],
    pub mem: VendorMemOids,
    /// 温度列（直接摄氏度）
    pub temp_columns: &'static [fn() -> Oid],
}

/// 厂商库条目：识别规则引用 + 可选私有采集 OID 集。
/// `oid_set = None` 表示识别-only（识别得出但不提供绑定，§5.3.3）。
/// 规则以 `&'static` 引用存放：规则定义与 OID 集同置于各厂商模块文件，
/// 注册表不复制规则数据。
pub struct VendorMib {
    pub rule: &'static VendorRule,
    pub oid_set: Option<&'static VendorOidSet>,
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
    BUILTIN.iter().chain(community::COMMUNITY)
}

// —— 内置品牌级规则（三家采集库 + 13 识别-only，收编 sysinfo.rs 词表）——

static BUILTIN: &[VendorMib] = &[
    VendorMib {
        rule: &cisco::RULE,
        oid_set: Some(&cisco::OID_SET),
    },
    VendorMib {
        rule: &huawei::RULE,
        oid_set: Some(&huawei::OID_SET),
    },
    VendorMib {
        rule: &h3c::RULE,
        oid_set: Some(&h3c::OID_SET),
    },
    // —— 识别-only 品牌（识别得出但不提供绑定，§5.3.3）——
    // 企业号来源：SNMP::Info 企业号映射表（IANA 注册）
    only(&VendorRule {
        id: "juniper",
        display_name: "Juniper",
        aliases: &["juniper"],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2636]],
        descr_keywords: &["juniper", "junos"],
        descr_keywords_except: &[],
        descr_regexes: &[],
        probe_checks: &[],
    }),
    only(&VendorRule {
        id: "dell",
        display_name: "Dell",
        aliases: &["dell"],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 674]],
        descr_keywords: &["dell"],
        descr_keywords_except: &[],
        descr_regexes: &[],
        probe_checks: &[],
    }),
    // 企业号 11 同时覆盖 Comware（→h3c 冲突，描述词胜出）与
    // ProVision/Aruba（本条），冲突组合一律 Low + 人工确认（§6.1）
    only(&VendorRule {
        id: "hp",
        display_name: "HP/HPE",
        aliases: &[
            "hp",
            "hpe",
            "procurve",
            "hewlett packard",
            "hewlett-packard",
        ],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 11]],
        descr_keywords: &["hp", "hpe", "procurve", "hewlett packard"],
        descr_keywords_except: &[],
        descr_regexes: &[],
        probe_checks: &[],
    }),
    only(&VendorRule {
        id: "aruba",
        display_name: "Aruba",
        aliases: &["aruba"],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 14823]],
        descr_keywords: &["aruba", "arubaos"],
        descr_keywords_except: &[],
        descr_regexes: &[],
        probe_checks: &[],
    }),
    only(&VendorRule {
        id: "netgear",
        display_name: "Netgear",
        aliases: &["netgear"],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4526]],
        descr_keywords: &["netgear", "prosafe"],
        descr_keywords_except: &[],
        descr_regexes: &[],
        probe_checks: &[],
    }),
    // TP-Link/Linksys/ZyXEL 企业号未核实，留空不虚构（§5.3.1）
    only(&VendorRule {
        id: "tp-link",
        display_name: "TP-Link",
        aliases: &["tp-link", "tplink", "tp link"],
        sysobjectid_prefixes: &[],
        descr_keywords: &["tp-link", "tplink"],
        descr_keywords_except: &[],
        descr_regexes: &[],
        probe_checks: &[],
    }),
    only(&VendorRule {
        id: "linksys",
        display_name: "Linksys",
        aliases: &["linksys"],
        sysobjectid_prefixes: &[],
        descr_keywords: &["linksys"],
        descr_keywords_except: &[],
        descr_regexes: &[],
        probe_checks: &[],
    }),
    only(&VendorRule {
        id: "ubiquiti",
        display_name: "Ubiquiti",
        aliases: &["ubiquiti", "ubnt", "unifi"],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 4413],
            &[1, 3, 6, 1, 4, 1, 10002],
            &[1, 3, 6, 1, 4, 1, 41112],
        ],
        descr_keywords: &["ubiquiti", "ubnt", "unifi"],
        descr_keywords_except: &[],
        descr_regexes: &[],
        probe_checks: &[],
    }),
    only(&VendorRule {
        id: "mikrotik",
        display_name: "MikroTik",
        aliases: &["mikrotik", "routeros"],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 14988]],
        descr_keywords: &["mikrotik", "routeros"],
        descr_keywords_except: &[],
        descr_regexes: &[],
        probe_checks: &[],
    }),
    only(&VendorRule {
        id: "extreme",
        display_name: "Extreme Networks",
        aliases: &["extreme", "extremenetworks", "extreme networks"],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1916]],
        descr_keywords: &["extreme", "extremexos"],
        descr_keywords_except: &[],
        descr_regexes: &[],
        probe_checks: &[],
    }),
    only(&VendorRule {
        id: "alcatel",
        display_name: "Alcatel",
        aliases: &["alcatel", "alcatel-lucent"],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6486]],
        descr_keywords: &["alcatel"],
        descr_keywords_except: &[],
        descr_regexes: &[],
        probe_checks: &[],
    }),
    only(&VendorRule {
        id: "zyxel",
        display_name: "ZyXEL",
        aliases: &["zyxel"],
        sysobjectid_prefixes: &[],
        descr_keywords: &["zyxel"],
        descr_keywords_except: &[],
        descr_regexes: &[],
        probe_checks: &[],
    }),
    only(&VendorRule {
        id: "d-link",
        display_name: "D-Link",
        aliases: &["d-link", "dlink"],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 171]],
        descr_keywords: &["d-link", "dlink"],
        descr_keywords_except: &[],
        descr_regexes: &[],
        probe_checks: &[],
    }),
];

/// 识别-only 品牌条目（无采集库）。
const fn only(rule: &'static VendorRule) -> VendorMib {
    VendorMib {
        rule,
        oid_set: None,
    }
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
/// 社区 OS 级 id 按 LibreNMS group 归属采集库：cisco 组全产品线绑定 Cisco
/// MIB 家族（探测失败自动回退公共链，激进绑定安全）；华为仅 VRP 网络设备
/// 绑定（oceanstor/smartax 等产品线走 Generic，公共链等价）。
pub fn vendor_kind(id: Option<&str>) -> VendorKind {
    match id {
        // Cisco 产品线（LibreNMS group=cisco）
        Some(
            "cisco" | "ios" | "iosxe" | "iosxr" | "nxos" | "ftd" | "asa" | "pixos" | "catos"
            | "cat1900" | "fxos" | "sanos" | "waas" | "cimc" | "acano" | "acs" | "acsw" | "apic"
            | "ciscoepc" | "ciscome1200" | "ciscosce" | "ciscospa" | "ciscosrp" | "ons" | "viptela",
        ) => VendorKind::Cisco,
        // 华为 VRP 网络设备（yunshan=云山/CloudEngine，VRP8 家族实体表同构）
        Some("huawei" | "vrp" | "yunshan") => VendorKind::Huawei,
        // 新华三 Comware
        Some("h3c" | "comware") => VendorKind::H3c,
        _ => VendorKind::Generic,
    }
}

/// 规则 id → 私有采集 OID 集（识别-only 品牌与未知 id → None）。
pub fn oid_set(id: Option<&str>) -> Option<&'static VendorOidSet> {
    id.and_then(|id| registry().find(|m| m.rule.id == id))
        .and_then(|m| m.oid_set)
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
        assert!(oid_set(Some("cisco")).is_some());
        assert!(oid_set(Some("juniper")).is_none());
        assert!(oid_set(None).is_none());
        assert_eq!(display_name(Some("hp")), "HP/HPE");
        assert_eq!(display_name(Some("unknown-brand")), "Unknown");
        assert_eq!(display_name(None), "Unknown");
    }
}
