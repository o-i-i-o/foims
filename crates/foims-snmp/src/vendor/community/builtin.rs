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

use super::super::{VendorMib, VendorRule};

pub(super) static BUILTIN: &[VendorMib] = &[
    // IANA enterprise-numbers：9 = Cisco Systems
    VendorMib {
        rule: &VendorRule {
            id: "cisco",
            display_name: "Cisco",
            aliases: &["cisco"],
            sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9]],
            descr_keywords: &["cisco"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: Some(&super::super::mibs::CISCO),
    },
    // IANA enterprise-numbers：2011 = Huawei Technologies；「versatile routing platform」为华为 VRP 系统多词关键词
    VendorMib {
        rule: &VendorRule {
            id: "huawei",
            display_name: "Huawei",
            aliases: &["huawei"],
            sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2011]],
            descr_keywords: &["huawei", "versatile routing platform"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: Some(&super::super::mibs::HUAWEI),
    },
    // IANA enterprise-numbers：25506 = H3C（原 3Com 归并）
    VendorMib {
        rule: &VendorRule {
            id: "h3c",
            display_name: "H3C",
            aliases: &["h3c", "3com"],
            sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 25506]],
            descr_keywords: &["3com", "comware", "h3c"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: Some(&super::super::mibs::H3C),
    },
    // 识别-only（无采集库）
    VendorMib {
        rule: &VendorRule {
            id: "juniper",
            display_name: "Juniper",
            aliases: &["juniper"],
            sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2636]],
            descr_keywords: &["juniper", "junos"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: None,
    },
    // 识别-only
    VendorMib {
        rule: &VendorRule {
            id: "dell",
            display_name: "Dell",
            aliases: &["dell"],
            sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 674]],
            descr_keywords: &["dell"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: None,
    },
    // 企业号 11 同时覆盖 Comware（→h3c 冲突，描述词胜出）与 ProVision/Aruba（本条），冲突组合一律 Low + 人工确认（§6.1）
    VendorMib {
        rule: &VendorRule {
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
            descr_keywords: &["hewlett packard", "hp", "hpe", "procurve"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: None,
    },
    // 识别-only
    VendorMib {
        rule: &VendorRule {
            id: "aruba",
            display_name: "Aruba",
            aliases: &["aruba"],
            sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 14823]],
            descr_keywords: &["aruba", "arubaos"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: None,
    },
    // 识别-only
    VendorMib {
        rule: &VendorRule {
            id: "netgear",
            display_name: "Netgear",
            aliases: &["netgear"],
            sysobjectid_prefixes: &[
                &[1, 3, 6, 1, 4, 1, 89, 1, 1, 1, 3, 6, 1, 4, 1, 4526, 100, 4],
                &[1, 3, 6, 1, 4, 1, 4526],
            ],
            descr_keywords: &["netgear", "prosafe"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: None,
    },
    // 企业号未核实，留空不虚构（§5.3.1）；识别-only
    VendorMib {
        rule: &VendorRule {
            id: "tp-link",
            display_name: "TP-Link",
            aliases: &["tp-link", "tplink", "tp link"],
            sysobjectid_prefixes: &[],
            descr_keywords: &["tp-link", "tplink"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: None,
    },
    // 企业号未核实，留空不虚构；识别-only
    VendorMib {
        rule: &VendorRule {
            id: "linksys",
            display_name: "Linksys",
            aliases: &["linksys"],
            sysobjectid_prefixes: &[],
            descr_keywords: &["linksys"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: None,
    },
    // 识别-only
    VendorMib {
        rule: &VendorRule {
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
        },
        oid_set: None,
    },
    // 识别-only
    VendorMib {
        rule: &VendorRule {
            id: "mikrotik",
            display_name: "MikroTik",
            aliases: &["mikrotik", "routeros"],
            sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 14988]],
            descr_keywords: &["mikrotik", "routeros"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: None,
    },
    // 识别-only
    VendorMib {
        rule: &VendorRule {
            id: "extreme",
            display_name: "Extreme Networks",
            aliases: &["extreme", "extremenetworks", "extreme networks"],
            sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1916]],
            descr_keywords: &["extreme", "extremexos"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: None,
    },
    // 识别-only
    VendorMib {
        rule: &VendorRule {
            id: "alcatel",
            display_name: "Alcatel",
            aliases: &["alcatel", "alcatel-lucent"],
            sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6486]],
            descr_keywords: &["alcatel"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: None,
    },
    // 企业号未核实，留空不虚构；识别-only
    VendorMib {
        rule: &VendorRule {
            id: "zyxel",
            display_name: "ZyXEL",
            aliases: &["zyxel"],
            sysobjectid_prefixes: &[],
            descr_keywords: &["zyxel"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: None,
    },
    // 识别-only
    VendorMib {
        rule: &VendorRule {
            id: "d-link",
            display_name: "D-Link",
            aliases: &["d-link", "dlink"],
            sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 171]],
            descr_keywords: &["d-link", "dlink"],
            descr_keywords_except: &[],
            descr_regexes: &[],
            probe_checks: &[],
        },
        oid_set: None,
    },
];
