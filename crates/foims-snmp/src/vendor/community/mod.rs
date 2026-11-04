//! LibreNMS os_detection 全量转换 + 内置品牌级规则（识别数据单一来源）。
//! 由 da-scripts/gen_vendor_community.py 于 2026-11-04 生成，**勿手改**：
//! 更新上游后重跑脚本再生成；规则结构变化须同步 vendor/mod.rs。
//!
//! 上游 822 个定义 = 平台类 29 个（platform.rs 域）+ 本表 808 条（内置品牌 16 + 社区 OS 792）。对账：822 全覆盖，无跳过。
//! - 与内置品牌同 id 证据合并 1 个（如 netgear.yaml 并入内置 netgear，display/aliases 保持内置）
//! - 空证据占位 54 个：上游证据为 MIB 名格式探测（厂商 MIB 不可得，零 MIB 依赖无法解析）/平台号前缀/except 正则/低质关键词，无可转换证据 → 生成空规则（识别永不命中、无风险），保留 OS 名录完整性，上游补充证据后重跑自动补全
//! - 丢弃平台 agent 号前缀 75 条（8072 net-snmp 等，§3.1 不作厂商证据）
//! - 丢弃高风险关键词（单词元 ≤2 字符）0 条、孤儿关键词（AND 伙伴失效，块内前缀为平台号/含主动探测）65 条
//! - 正则：sysDescr_regex/except 原始 177 条，仅收纯 sysDescr 块正向正则61 条（PCRE 定界符剥除，/i 转 (?i)，Rust regex 编译失败运行时跳过）；空定界丢弃 0 条
//! - 主动探测：snmpget 块 84 个，转换 31 项检查、丢弃（op=!=/未知/缺 OID）25 条、MIB 名格式 OID（MIB::obj，零 MIB 依赖无法解析）28 条
//! - 多词元关键词保留（本项目全词元命中，比上游子串严格，漏报方向安全）

use super::{VendorMib, VendorRule};

/// 识别-only 品牌条目（无采集库）。
pub(super) const fn only(rule: &'static VendorRule) -> VendorMib {
    VendorMib {
        rule,
        oid_set: None,
    }
}

/// 全部规则分桶：内置品牌级在前（词表更精、sysDescr 判定优先），
/// 社区 OS 级按 id 首字母四桶（前缀匹配不受顺序影响，最长前缀胜出）。
pub(super) static COMMUNITY: &[&[VendorMib]] = &[
    builtin::BUILTIN,
    os_a_g::OS_AG,
    os_h_n::OS_HN,
    os_o_t::OS_OT,
    os_u_z::OS_UZ,
];

/// OS id → 采集库归属（vendor_kind() 查表，miss → Generic 公共回退链）。
/// cisco 组全产品线绑定 Cisco MIB 家族（探测失败自动回退公共链，激进
/// 绑定安全）；华为仅 VRP 网络设备（oceanstor/smartax 等产品线走
/// Generic）；新华三 Comware。
pub(super) static KIND_BY_OS: &[(&str, crate::live::VendorKind)] = &[
    ("acs", crate::live::VendorKind::Cisco),
    ("acsw", crate::live::VendorKind::Cisco),
    ("apic", crate::live::VendorKind::Cisco),
    ("asa", crate::live::VendorKind::Cisco),
    ("cat1900", crate::live::VendorKind::Cisco),
    ("catos", crate::live::VendorKind::Cisco),
    ("cimc", crate::live::VendorKind::Cisco),
    ("cisco", crate::live::VendorKind::Cisco),
    ("ciscoepc", crate::live::VendorKind::Cisco),
    ("ciscome1200", crate::live::VendorKind::Cisco),
    ("ciscosce", crate::live::VendorKind::Cisco),
    ("ciscospa", crate::live::VendorKind::Cisco),
    ("ciscosrp", crate::live::VendorKind::Cisco),
    ("comware", crate::live::VendorKind::H3c),
    ("ftd", crate::live::VendorKind::Cisco),
    ("fxos", crate::live::VendorKind::Cisco),
    ("h3c", crate::live::VendorKind::H3c),
    ("huawei", crate::live::VendorKind::Huawei),
    ("ios", crate::live::VendorKind::Cisco),
    ("iosxe", crate::live::VendorKind::Cisco),
    ("iosxr", crate::live::VendorKind::Cisco),
    ("nxos", crate::live::VendorKind::Cisco),
    ("ons", crate::live::VendorKind::Cisco),
    ("pixos", crate::live::VendorKind::Cisco),
    ("sanos", crate::live::VendorKind::Cisco),
    ("viptela", crate::live::VendorKind::Cisco),
    ("vrp", crate::live::VendorKind::Huawei),
    ("waas", crate::live::VendorKind::Cisco),
    ("yunshan", crate::live::VendorKind::Huawei),
];

mod builtin;
mod os_a_g;
mod os_h_n;
mod os_o_t;
mod os_u_z;
