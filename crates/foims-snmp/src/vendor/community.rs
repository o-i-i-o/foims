//! 社区 OS 级识别规则：LibreNMS os_detection 全量转换（GPL 上游，
//! 只提取识别事实：sysObjectID 前缀 + sysDescr 关键词）。
//! 由 da-scripts/gen_vendor_community.py 于 2026-11-04 生成，**勿手改**：
//! 更新上游后重跑脚本再生成；新增规则结构变化须同步 vendor/mod.rs。
//!
//! 上游 822 个定义 → 本表 686 条 OS 规则。转换口径：
//! - 平台类（type=server/unix，走 platform.rs）29 个；与内置品牌 id 重复 1 个（内置优先）
//! - 无有效前缀/关键词（仅正则/主动探测，模型表达不了）跳过 106 个
//! - 丢弃平台 agent 号前缀 21 条（8072 net-snmp 等，§3.1 不作厂商证据）
//! - 丢弃高风险关键词（单词元 ≤2 字符）0 条、孤儿关键词（AND 伙伴失效，块内前缀为平台号/含主动探测）65 条
//! - 正则证据（sysDescr_regex/except，语义不同不转换）177 条、主动探测 84 条
//! - 多词元关键词保留（本项目全词元命中，比上游子串严格，漏报方向安全）

use super::{VendorMib, VendorRule, only};

pub(super) static COMMUNITY: &[VendorMib] = &[
    // LibreNMS os_detection/3com.yaml
    only(&VendorRule {
        id: "3com",
        display_name: "3Com",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 43, 1], &[1, 3, 6, 1, 4, 1, 43, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/aaron.yaml
    only(&VendorRule {
        id: "aaron",
        display_name: "Aaron FM Rebroadcaster",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 42111, 640],
            &[1, 3, 6, 1, 4, 1, 42111, 650],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/acos.yaml
    only(&VendorRule {
        id: "acos",
        display_name: "A10 Networks",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 22610, 1, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/acs.yaml
    only(&VendorRule {
        id: "acs",
        display_name: "Cisco ACS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9, 1, 1117]],
        descr_keywords: &["cisco secure access control system"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/acsw.yaml
    only(&VendorRule {
        id: "acsw",
        display_name: "Cisco ACE",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9, 1, 1291]],
        descr_keywords: &[
            "application control engine",
            "cisco application control software",
        ],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/adtran-aos.yaml
    only(&VendorRule {
        id: "adtran-aos",
        display_name: "Adtran AOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 664]],
        descr_keywords: &["netvanta"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/adva-alm.yaml
    only(&VendorRule {
        id: "adva-alm",
        display_name: "Adtran ALM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2544, 1, 14]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/adva-aos.yaml
    only(&VendorRule {
        id: "adva-aos",
        display_name: "ADVA AOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2544, 1, 20, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/adva-fsp150cp.yaml
    only(&VendorRule {
        id: "adva-fsp150cp",
        display_name: "ADVA FSP150CP",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2544, 1, 10, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/adva-osa.yaml
    only(&VendorRule {
        id: "adva-osa",
        display_name: "ADVA OSA",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 5551, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/adva_fsp150.yaml
    only(&VendorRule {
        id: "adva_fsp150",
        display_name: "ADVA FSP150CC",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2544, 1, 12, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/adva_fsp3kr7.yaml
    only(&VendorRule {
        id: "adva_fsp3kr7",
        display_name: "ADVA FSP3000 R7",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2544, 1, 11, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/adva_xg300.yaml
    only(&VendorRule {
        id: "adva_xg300",
        display_name: "ADVA OptiSwitch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 629, 22]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/advantech.yaml
    only(&VendorRule {
        id: "advantech",
        display_name: "Advantech",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 10297, 202]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/airconsole.yaml
    only(&VendorRule {
        id: "airconsole",
        display_name: "Airconsole Server",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 14358, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/airos-af.yaml
    only(&VendorRule {
        id: "airos-af",
        display_name: "Ubiquiti AirFiber",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 10002, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/airos-af60.yaml
    only(&VendorRule {
        id: "airos-af60",
        display_name: "Ubiquiti AirFiber 60",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 10002, 1], &[1, 3, 6, 1, 4, 1, 41112]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/airos.yaml
    only(&VendorRule {
        id: "airos",
        display_name: "Ubiquiti AirOS",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 10002, 1],
            &[1, 3, 6, 1, 4, 1, 41112, 1, 4],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/airport.yaml
    only(&VendorRule {
        id: "airport",
        display_name: "Apple AirPort",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["apple airport", "apple base station", "base station v3.84"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/akcp.yaml
    only(&VendorRule {
        id: "akcp",
        display_name: "AKCP SensorProbe",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3854]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/alaxala.yaml
    only(&VendorRule {
        id: "alaxala",
        display_name: "ALAXALA",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 21839, 1, 1],
            &[1, 3, 6, 1, 4, 1, 21839, 1, 2],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/albentiaaos.yaml
    only(&VendorRule {
        id: "albentiaaos",
        display_name: "Albentia AOS9",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 28087]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/alcoma-almp.yaml
    only(&VendorRule {
        id: "alcoma-almp",
        display_name: "Alcoma",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12140, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/alfo80hd.yaml
    only(&VendorRule {
        id: "alfo80hd",
        display_name: "SIAE Alfoplus 80HD",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3373, 1103]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/algcom-dc-ups.yaml
    only(&VendorRule {
        id: "algcom-dc-ups",
        display_name: "ALGCOM DC UPS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 49136]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/algcom-pdu.yaml
    only(&VendorRule {
        id: "algcom-pdu",
        display_name: "ALGCOM PDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 49136, 200]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/algcom-sm.yaml
    only(&VendorRule {
        id: "algcom-sm",
        display_name: "ALGCOM SM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 49136, 100]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/allied-tq.yaml
    only(&VendorRule {
        id: "allied-tq",
        display_name: "Allied Telesis Wireless (TQ)",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 207, 1, 13]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/allied.yaml
    only(&VendorRule {
        id: "allied",
        display_name: "Alliedware",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 207]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/allworx_voip.yaml
    only(&VendorRule {
        id: "allworx_voip",
        display_name: "Allworx VoIP",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 38516, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/alpha-power.yaml
    only(&VendorRule {
        id: "alpha-power",
        display_name: "Alpha Power",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 926, 1, 99, 1, 4, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/alpineoe-tdcmedfa.yaml
    only(&VendorRule {
        id: "alpineoe-tdcmedfa",
        display_name: "Alpine OptoElectronics TDCM-EDFA platform",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 52326]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/alteonos.yaml
    only(&VendorRule {
        id: "alteonos",
        display_name: "AlteonOS",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 1872, 1, 13, 3, 5],
            &[1, 3, 6, 1, 4, 1, 1872, 2],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/anue.yaml
    only(&VendorRule {
        id: "anue",
        display_name: "Anue",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 32620]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/anyos.yaml
    only(&VendorRule {
        id: "anyos",
        display_name: "AnyOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4413]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/aos-emu2.yaml
    only(&VendorRule {
        id: "aos-emu2",
        display_name: "APC Environmental Monitoring Unit",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 318, 1, 3, 8, 2],
            &[1, 3, 6, 1, 4, 1, 318, 1, 3, 8, 3],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/aos.yaml
    only(&VendorRule {
        id: "aos",
        display_name: "Alcatel-Lucent OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6486, 800, 1, 1, 2, 2, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/aos6.yaml
    only(&VendorRule {
        id: "aos6",
        display_name: "Alcatel-Lucent OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6486, 800, 1, 1, 2, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/aos7.yaml
    only(&VendorRule {
        id: "aos7",
        display_name: "Alcatel-Lucent OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6486, 801]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/apc-cpdu.yaml
    only(&VendorRule {
        id: "apc-cpdu",
        display_name: "APC cPDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 318, 1, 1, 32, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/apc-epdu.yaml
    only(&VendorRule {
        id: "apc-epdu",
        display_name: "APC ePDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 0], &[1, 3, 6, 1, 4, 1, 318, 1, 3, 4, 9]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/apc-inrow.yaml
    only(&VendorRule {
        id: "apc-inrow",
        display_name: "APC InRow",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 318, 1, 3, 14, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/apc-mgeups.yaml
    only(&VendorRule {
        id: "apc-mgeups",
        display_name: "APC MGE UPS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 705, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/apc-netbotz.yaml
    only(&VendorRule {
        id: "apc-netbotz",
        display_name: "NetBotz Environment Sensor",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 52674, 500]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/apc.yaml
    only(&VendorRule {
        id: "apc",
        display_name: "APC Management Module",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 318, 1]],
        descr_keywords: &[
            "apc embedded powernet",
            "apc environmental manager",
            "apc infrastruxure ats",
            "apc inrow dx",
            "apc masterswitch pdu",
            "apc metered rack pdu",
            "apc snmp agent",
            "apc switched rack pdu",
            "apc uniflair sp",
            "apc web/snmp management card",
        ],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/apic.yaml
    only(&VendorRule {
        id: "apic",
        display_name: "Cisco APIC",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9, 1, 2238]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/applicationsware.yaml
    only(&VendorRule {
        id: "applicationsware",
        display_name: "Vanguard ApplicationsWare",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 449, 2, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/aprisa.yaml
    only(&VendorRule {
        id: "aprisa",
        display_name: "Aprisa",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 14817, 7, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/apsoluteos.yaml
    only(&VendorRule {
        id: "apsoluteos",
        display_name: "ApsoluteOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 89, 1, 1, 62]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/arbos.yaml
    only(&VendorRule {
        id: "arbos",
        display_name: "ArbOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9694, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/areca.yaml
    only(&VendorRule {
        id: "areca",
        display_name: "Areca RAID Subsystem",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["raid subsystem v"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/arista-mos.yaml
    only(&VendorRule {
        id: "arista-mos",
        display_name: "Arista MOS",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 43191, 1, 2, 5],
            &[1, 3, 6, 1, 4, 1, 43191, 1, 2, 7],
            &[1, 3, 6, 1, 4, 1, 43191, 1, 2, 8],
            &[1, 3, 6, 1, 4, 1, 43191, 1, 6, 6],
            &[1, 3, 6, 1, 4, 1, 43191, 1, 6, 7],
            &[1, 3, 6, 1, 4, 1, 43191, 1, 6, 9],
            &[1, 3, 6, 1, 4, 1, 43191, 1, 6, 10],
            &[1, 3, 6, 1, 4, 1, 43191, 1, 7, 5],
            &[1, 3, 6, 1, 4, 1, 43191, 1, 7, 6],
            &[1, 3, 6, 1, 4, 1, 43191, 1, 7, 7],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/arista_eos.yaml
    only(&VendorRule {
        id: "arista_eos",
        display_name: "Arista EOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 30065, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/arrayos.yaml
    only(&VendorRule {
        id: "arrayos",
        display_name: "Array Networks",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7564]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/arris-apex.yaml
    only(&VendorRule {
        id: "arris-apex",
        display_name: "Arris Apex",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1166, 1, 31]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/arris-c3.yaml
    only(&VendorRule {
        id: "arris-c3",
        display_name: "Arris CMTS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4115, 1, 4, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/arris-c4.yaml
    only(&VendorRule {
        id: "arris-c4",
        display_name: "Arris CMTS",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 4115, 1, 9, 1],
            &[1, 3, 6, 1, 4, 1, 4998, 2, 1],
            &[1, 3, 6, 1, 4, 1, 4998, 2, 2],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/arris-cm.yaml
    only(&VendorRule {
        id: "arris-cm",
        display_name: "ARRIS DOCSIS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4115]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/arris-d5.yaml
    only(&VendorRule {
        id: "arris-d5",
        display_name: "Arris D5 Universal EdgeQAM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4115, 1, 8]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/arris-dsr4410md.yaml
    only(&VendorRule {
        id: "arris-dsr4410md",
        display_name: "Arris Satellite Receiver",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1166, 1, 621]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/aruba-instant.yaml
    only(&VendorRule {
        id: "aruba-instant",
        display_name: "Aruba Instant",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 14823, 1, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/arubaos-cx.yaml
    only(&VendorRule {
        id: "arubaos-cx",
        display_name: "ArubaOS-CX",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 47196, 4, 1, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/arubaos.yaml
    only(&VendorRule {
        id: "arubaos",
        display_name: "ArubaOS",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 6486, 800, 1, 1, 2, 2, 2],
            &[1, 3, 6, 1, 4, 1, 14823],
        ],
        descr_keywords: &["arubaos"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ascom.yaml
    only(&VendorRule {
        id: "ascom",
        display_name: "Ascom",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 27614]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/asyncos.yaml
    only(&VendorRule {
        id: "asyncos",
        display_name: "Cisco AsyncOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 15497, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/atenpdu.yaml
    only(&VendorRule {
        id: "atenpdu",
        display_name: "Aten PDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21317]],
        descr_keywords: &["aten"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ats.yaml
    only(&VendorRule {
        id: "ats",
        display_name: "Automatic Transfer Switch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 37662, 1, 2, 2, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/audiocodes.yaml
    only(&VendorRule {
        id: "audiocodes",
        display_name: "Audiocodes",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 5003, 8, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/avaya-ipo.yaml
    only(&VendorRule {
        id: "avaya-ipo",
        display_name: "IP Office Firmware",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6889]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/aviat-wtm.yaml
    only(&VendorRule {
        id: "aviat-wtm",
        display_name: "Aviat WTM",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 2509, 11, 1, 1, 7],
            &[1, 3, 6, 1, 4, 1, 2509, 11, 1, 1, 8],
            &[1, 3, 6, 1, 4, 1, 2509, 11, 1, 1, 9],
            &[1, 3, 6, 1, 4, 1, 2509, 11, 1, 1, 10],
            &[1, 3, 6, 1, 4, 1, 2509, 11, 1, 1, 12],
            &[1, 3, 6, 1, 4, 1, 2509, 11, 1, 1, 13],
            &[1, 3, 6, 1, 4, 1, 2509, 11, 1, 1, 14],
            &[1, 3, 6, 1, 4, 1, 2509, 11, 1, 1, 15],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/avocent-mp.yaml
    only(&VendorRule {
        id: "avocent-mp",
        display_name: "Vertiv Avocent MergePoint Unity",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 10418, 18, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/avocent.yaml
    only(&VendorRule {
        id: "avocent",
        display_name: "Avocent",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 10418, 16]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/avr-hd.yaml
    only(&VendorRule {
        id: "avr-hd",
        display_name: "AvediaPlayer Receivers",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 28194]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/avtech.yaml
    only(&VendorRule {
        id: "avtech",
        display_name: "Avtech Environment Sensor",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 20916], &[1, 3, 6, 1, 4, 1, 20916, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/awplus.yaml
    only(&VendorRule {
        id: "awplus",
        display_name: "Alliedware Plus",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 207]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/axos.yaml
    only(&VendorRule {
        id: "axos",
        display_name: "Calix AXOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6321, 1, 2, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/baicells-od04.yaml
    only(&VendorRule {
        id: "baicells-od04",
        display_name: "Baicells CPE",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["baicpe"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/barracudaloadbalancer.yaml
    only(&VendorRule {
        id: "barracudaloadbalancer",
        display_name: "Barracuda Load Balancer",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["barracuda load balancer"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/barracudangfirewall.yaml
    only(&VendorRule {
        id: "barracudangfirewall",
        display_name: "Barracuda NG Firewall",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 10704, 1]],
        descr_keywords: &["barracuda firewall", "barracuda ng firewall"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/barracudaspamfirewall.yaml
    only(&VendorRule {
        id: "barracudaspamfirewall",
        display_name: "Barracuda Email Security Gateway",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &[
            "barracuda email security gateway",
            "barracuda spam firewall",
        ],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/barracudawafirewall.yaml
    only(&VendorRule {
        id: "barracudawafirewall",
        display_name: "Barracuda Web Application Firewall",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["barracuda web application firewall"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/bats.yaml
    only(&VendorRule {
        id: "bats",
        display_name: "BATS AATS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 37069]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/bcm963.yaml
    only(&VendorRule {
        id: "bcm963",
        display_name: "Broadcom BCM963xx",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["bcm963"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/bdcom.yaml
    only(&VendorRule {
        id: "bdcom",
        display_name: "BDCOM(tm) Software",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3320, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/bdt-flexstor.yaml
    only(&VendorRule {
        id: "bdt-flexstor",
        display_name: "BDT FlexStor II",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 20884, 10893, 2, 101]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/benuos.yaml
    only(&VendorRule {
        id: "benuos",
        display_name: "Benu",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 39406, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/binos.yaml
    only(&VendorRule {
        id: "binos",
        display_name: "Telco Systems BiNOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 738, 1, 5, 100]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/binox.yaml
    only(&VendorRule {
        id: "binox",
        display_name: "Telco Systems BiNOX",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 738, 10, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/bintec-beip-plus.yaml
    only(&VendorRule {
        id: "bintec-beip-plus",
        display_name: "Bintec Be.IP Plus",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 272, 4, 201, 66, 69, 50, 48]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/bintec-smart.yaml
    only(&VendorRule {
        id: "bintec-smart",
        display_name: "Bintec Smart Router",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 272, 4, 201, 82, 78, 79, 48],
            &[1, 3, 6, 1, 4, 1, 272, 4, 201, 83, 82, 49, 48],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/bke.yaml
    only(&VendorRule {
        id: "bke",
        display_name: "BKE",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 27533]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/bktel-oa.yaml
    only(&VendorRule {
        id: "bktel-oa",
        display_name: "BKtel",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 7501, 1, 2, 99],
            &[1, 3, 6, 1, 4, 1, 7501, 1, 2, 100],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/bluecatnetworks.yaml
    only(&VendorRule {
        id: "bluecatnetworks",
        display_name: "Bluecat Networks",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 13315, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/boschcpp.yaml
    only(&VendorRule {
        id: "boschcpp",
        display_name: "Bosch IP Camera",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3967, 1, 1, 0]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/boss.yaml
    only(&VendorRule {
        id: "boss",
        display_name: "Extreme BOSS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 45], &[1, 3, 6, 1, 4, 1, 2272]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/breeze.yaml
    only(&VendorRule {
        id: "breeze",
        display_name: "Alvarion Breeze",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12394, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/broadworks.yaml
    only(&VendorRule {
        id: "broadworks",
        display_name: "Cisco BroadWorks",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6431]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/bti800.yaml
    only(&VendorRule {
        id: "bti800",
        display_name: "BTI SA-800",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 30005, 1, 7]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/calix.yaml
    only(&VendorRule {
        id: "calix",
        display_name: "Calix Legacy",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 6321, 1, 2, 3],
            &[1, 3, 6, 1, 4, 1, 6321, 2, 1, 1],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/canonprinter.yaml
    only(&VendorRule {
        id: "canonprinter",
        display_name: "Canon Printer",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 1602, 4, 2],
            &[1, 3, 6, 1, 4, 1, 1602, 4, 3],
            &[1, 3, 6, 1, 4, 1, 1602, 4, 7],
        ],
        descr_keywords: &["canon ip", "canon ir", "canon mf", "canon tm", "canon tx"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cat1900.yaml
    only(&VendorRule {
        id: "cat1900",
        display_name: "Cisco Catalyst 1900",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["cisco systems catalyst 1900"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/catos.yaml
    only(&VendorRule {
        id: "catos",
        display_name: "Cisco CatOS",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["cisco catalyst operating system software"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cdata.yaml
    only(&VendorRule {
        id: "cdata",
        display_name: "cdata",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17409]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ceraos.yaml
    only(&VendorRule {
        id: "ceraos",
        display_name: "Ceragon CeraOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2281]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cetpower.yaml
    only(&VendorRule {
        id: "cetpower",
        display_name: "CET TSI Controller",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12551, 4, 0]],
        descr_keywords: &["t2s-eth supervisor for tsi snmp v1.0"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/chatsworth-pdu.yaml
    only(&VendorRule {
        id: "chatsworth-pdu",
        display_name: "Chatsworth PDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 30932]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/christie-projector.yaml
    only(&VendorRule {
        id: "christie-projector",
        display_name: "Christie Projector",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 25766]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ciena-rls.yaml
    only(&VendorRule {
        id: "ciena-rls",
        display_name: "Ciena Reconfigurable Line System",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1271, 1, 2, 12]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ciena-saos.yaml
    only(&VendorRule {
        id: "ciena-saos",
        display_name: "Ciena SAOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 562, 68, 11]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ciena-sds.yaml
    only(&VendorRule {
        id: "ciena-sds",
        display_name: "Ciena Service Delivery Switch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1271, 1], &[1, 3, 6, 1, 4, 1, 6141, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ciena-waveserver.yaml
    only(&VendorRule {
        id: "ciena-waveserver",
        display_name: "Ciena Waveserver",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1271, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cimc.yaml
    only(&VendorRule {
        id: "cimc",
        display_name: "Cisco Integrated Management Controller",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["cisco integrated management controller"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ciscoepc.yaml
    only(&VendorRule {
        id: "ciscoepc",
        display_name: "Cisco EPC",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1429, 3940]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ciscome1200.yaml
    only(&VendorRule {
        id: "ciscome1200",
        display_name: "Cisco ME1200",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9, 1, 1899]],
        descr_keywords: &["me1200"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ciscosat.yaml
    only(&VendorRule {
        id: "ciscosat",
        display_name: "Cisco Satellite Receiver",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1429, 2, 2, 6, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ciscosb.yaml
    only(&VendorRule {
        id: "ciscosb",
        display_name: "Cisco Small Business",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9, 6, 1], &[1, 3, 6, 1, 4, 1, 3955, 6]],
        descr_keywords: &[
            "catalyst 1200 series",
            "catalyst 1300 series",
            "catalyst 1300x series",
        ],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ciscosce.yaml
    only(&VendorRule {
        id: "ciscosce",
        display_name: "Cisco SCE",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["cisco service control"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ciscospa.yaml
    only(&VendorRule {
        id: "ciscospa",
        display_name: "Cisco Voice Gateway",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9, 6, 1, 23, 1, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ciscosrp.yaml
    only(&VendorRule {
        id: "ciscosrp",
        display_name: "Cisco Services Ready Platform",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9, 1, 1157]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ciscowap.yaml
    only(&VendorRule {
        id: "ciscowap",
        display_name: "Cisco Wireless Access Point",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 9, 1, 3280],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3281],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3282],
            &[1, 3, 6, 1, 4, 1, 9, 6, 1, 31],
            &[1, 3, 6, 1, 4, 1, 9, 6, 1, 32],
            &[1, 3, 6, 1, 4, 1, 9, 6, 1, 34],
        ],
        descr_keywords: &["cisco small business wap"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ciscowlc.yaml
    only(&VendorRule {
        id: "ciscowlc",
        display_name: "Cisco WLC",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["cisco business wireless", "cisco controller"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/clearpass.yaml
    only(&VendorRule {
        id: "clearpass",
        display_name: "Aruba Clearpass",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 14823, 1, 6]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/clickshare.yaml
    only(&VendorRule {
        id: "clickshare",
        display_name: "Barco Clickshare",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7312]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cmm.yaml
    only(&VendorRule {
        id: "cmm",
        display_name: "Cambium CMM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 161, 19]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cms.yaml
    only(&VendorRule {
        id: "cms",
        display_name: "Corero CMS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 41036]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cnmatrix.yaml
    only(&VendorRule {
        id: "cnmatrix",
        display_name: "Cambium cnMatrix",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17713, 24]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cnos.yaml
    only(&VendorRule {
        id: "cnos",
        display_name: "Lenovo Cloud Network Operating System",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 19046, 1, 7]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cnpilote.yaml
    only(&VendorRule {
        id: "cnpilote",
        display_name: "Cambium cnPilot",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17713, 22]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cnwave60.yaml
    only(&VendorRule {
        id: "cnwave60",
        display_name: "Cambium cnWave60",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17713, 60, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/commander-plus.yaml
    only(&VendorRule {
        id: "commander-plus",
        display_name: "C&C Power Commander plus",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 18642]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/compas.yaml
    only(&VendorRule {
        id: "compas",
        display_name: "Alpha Comp@s",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 26854, 3, 2, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/comtrol.yaml
    only(&VendorRule {
        id: "comtrol",
        display_name: "Comtrol Industrial Switch",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 2882, 2, 1, 1],
            &[1, 3, 6, 1, 4, 1, 2882, 2, 1, 8],
            &[1, 3, 6, 1, 4, 1, 2882, 2, 4, 1],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/comware.yaml
    only(&VendorRule {
        id: "comware",
        display_name: "HPE Comware",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 25506]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/conteg-pdu.yaml
    only(&VendorRule {
        id: "conteg-pdu",
        display_name: "Conteg PDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 28402, 11, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/controlbox.yaml
    only(&VendorRule {
        id: "controlbox",
        display_name: "Controlbox TH-332B",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17095, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/coriant.yaml
    only(&VendorRule {
        id: "coriant",
        display_name: "Coriant TNMS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 42229, 6, 22]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ctcu.yaml
    only(&VendorRule {
        id: "ctcu",
        display_name: "CTC Union",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4756]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ctm.yaml
    only(&VendorRule {
        id: "ctm",
        display_name: "Last Mile Gear CTM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 25868]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cts.yaml
    only(&VendorRule {
        id: "cts",
        display_name: "Connection Technology Systems",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9304]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cubo.yaml
    only(&VendorRule {
        id: "cubo",
        display_name: "Huber+Suhner Cubo",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 27894, 10, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cumulus.yaml
    only(&VendorRule {
        id: "cumulus",
        display_name: "Cumulus Linux",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 40310]],
        descr_keywords: &["cumulus"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cxc.yaml
    only(&VendorRule {
        id: "cxc",
        display_name: "Alpha CXC HP Controller",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7309, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cxr-ts.yaml
    only(&VendorRule {
        id: "cxr-ts",
        display_name: "CXR Networks TS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1425]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cxrc.yaml
    only(&VendorRule {
        id: "cxrc",
        display_name: "Alpha CXRC Controller",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7309, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cyan.yaml
    only(&VendorRule {
        id: "cyan",
        display_name: "Ciena Z-Series",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 28533]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cyberoam-utm.yaml
    only(&VendorRule {
        id: "cyberoam-utm",
        display_name: "Cyberoam UTM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21067, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/cyberpower.yaml
    only(&VendorRule {
        id: "cyberpower",
        display_name: "Cyberpower",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3808]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dahua-nvr.yaml
    only(&VendorRule {
        id: "dahua-nvr",
        display_name: "Dahua NVR",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1004849, 3, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dantherm-os.yaml
    only(&VendorRule {
        id: "dantherm-os",
        display_name: "Dantherm",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["freecooling controller, snmp v1.0"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dasan-nos.yaml
    only(&VendorRule {
        id: "dasan-nos",
        display_name: "Dasan NOS",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 6296, 1, 2],
            &[1, 3, 6, 1, 4, 1, 6296, 1, 21],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/datacom-dmos.yaml
    only(&VendorRule {
        id: "datacom-dmos",
        display_name: "DmOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3709, 1, 2, 137]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/datacom.yaml
    only(&VendorRule {
        id: "datacom",
        display_name: "Datacom",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3709], &[1, 3, 6, 1, 4, 1, 4413]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/datadomain.yaml
    only(&VendorRule {
        id: "datadomain",
        display_name: "EMC Data Domain",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 19746, 3, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dcn-software.yaml
    only(&VendorRule {
        id: "dcn-software",
        display_name: "Digital China Networks",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 6339, 1, 1, 1],
            &[1, 3, 6, 1, 4, 1, 6339, 1, 1, 2],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/deliberant.yaml
    only(&VendorRule {
        id: "deliberant",
        display_name: "Deliberant OS",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["deliberant"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dell-laser.yaml
    only(&VendorRule {
        id: "dell-laser",
        display_name: "Dell Laser",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 674, 10898]],
        descr_keywords: &["dell color laser", "dell laser printer"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dell-ome-m.yaml
    only(&VendorRule {
        id: "dell-ome-m",
        display_name: "Dell OpenManage Enterprise Modular",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 674, 10892, 6]],
        descr_keywords: &["dell openmanage enterprise modular"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dell-os10.yaml
    only(&VendorRule {
        id: "dell-os10",
        display_name: "Dell EMC Networking OS10 Enterprise",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 674, 11000, 5000, 100, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dell-powervault.yaml
    only(&VendorRule {
        id: "dell-powervault",
        display_name: "Dell PowerVault",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 674],
            &[1, 3, 6, 1, 4, 1, 674, 10893, 2, 31],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dell-rcs.yaml
    only(&VendorRule {
        id: "dell-rcs",
        display_name: "Dell Remote Console",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 10418, 18, 21, 17]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dell-rpdu.yaml
    only(&VendorRule {
        id: "dell-rpdu",
        display_name: "Dell Rack PDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 674, 10903, 200, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dell-sonic.yaml
    only(&VendorRule {
        id: "dell-sonic",
        display_name: "Dell Enterprise SONiC",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 674, 10895, 5000, 300, 1, 9],
            &[1, 3, 6, 1, 4, 1, 4413, 1, 2, 1, 2],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dell-ups.yaml
    only(&VendorRule {
        id: "dell-ups",
        display_name: "Dell UPS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 674, 10902, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/deltaorion.yaml
    only(&VendorRule {
        id: "deltaorion",
        display_name: "Delta Orion Controller",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 20246]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/deltaups.yaml
    only(&VendorRule {
        id: "deltaups",
        display_name: "Delta UPS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2254, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/deva-db7001.yaml
    only(&VendorRule {
        id: "deva-db7001",
        display_name: "DB7001 - FM Radio Re-Broadcast Receiver",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 35833, 37]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/developprinter.yaml
    only(&VendorRule {
        id: "developprinter",
        display_name: "Develop Printer",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 18334, 1, 2, 1, 2, 1, 50, 2, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/didactum.yaml
    only(&VendorRule {
        id: "didactum",
        display_name: "Didactum",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 46501]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/digipower.yaml
    only(&VendorRule {
        id: "digipower",
        display_name: "Digipower",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17420]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dkt.yaml
    only(&VendorRule {
        id: "dkt",
        display_name: "DKT Comega",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 27304]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dlink-dgs1250.yaml
    only(&VendorRule {
        id: "dlink-dgs1250",
        display_name: "D-Link Switch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 171, 10, 165, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dlink.yaml
    only(&VendorRule {
        id: "dlink",
        display_name: "D-Link Switch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 171, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dlinkap.yaml
    only(&VendorRule {
        id: "dlinkap",
        display_name: "D-Link Access Point",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 171, 10, 37]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dnos.yaml
    only(&VendorRule {
        id: "dnos",
        display_name: "Dell Networking OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6027, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dpi.yaml
    only(&VendorRule {
        id: "dpi",
        display_name: "IBM DPI",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["ibm dpi"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/drac.yaml
    only(&VendorRule {
        id: "drac",
        display_name: "Dell DRAC",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 674, 10892, 2],
            &[1, 3, 6, 1, 4, 1, 674, 10892, 5],
        ],
        descr_keywords: &["dell out-of-band snmp agent for remote access controller"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/draytek.yaml
    only(&VendorRule {
        id: "draytek",
        display_name: "DrayTek",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7367]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/dxmos.yaml
    only(&VendorRule {
        id: "dxmos",
        display_name: "DXM Operating System",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21150]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/e3meter.yaml
    only(&VendorRule {
        id: "e3meter",
        display_name: "E3 Meter",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21695, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/e3meterdc.yaml
    only(&VendorRule {
        id: "e3meterdc",
        display_name: "E3 Meter DataConcentrator",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21695, 1401]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eaton-matrix.yaml
    only(&VendorRule {
        id: "eaton-matrix",
        display_name: "Eaton Matrix",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 935, 10, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eaton-mgeups.yaml
    only(&VendorRule {
        id: "eaton-mgeups",
        display_name: "Eaton UPS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 705, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eaton-sc200.yaml
    only(&VendorRule {
        id: "eaton-sc200",
        display_name: "Eaton SC200 Controller",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1918, 2, 13]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eaton-sc300.yaml
    only(&VendorRule {
        id: "eaton-sc300",
        display_name: "Eaton SC300 Controller",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1918, 2, 14]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eatonpdu.yaml
    only(&VendorRule {
        id: "eatonpdu",
        display_name: "Eaton PDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 534, 6]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eatonups.yaml
    only(&VendorRule {
        id: "eatonups",
        display_name: "Eaton UPS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 534, 1], &[1, 3, 6, 1, 4, 1, 534, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eatonupsm2.yaml
    only(&VendorRule {
        id: "eatonupsm2",
        display_name: "Eaton UPS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 534, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/edfa.yaml
    only(&VendorRule {
        id: "edfa",
        display_name: "EDFA",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17409, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/edfamux.yaml
    only(&VendorRule {
        id: "edfamux",
        display_name: "Solid Optics EDFAMUX",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 51628]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/edgecos.yaml
    only(&VendorRule {
        id: "edgecos",
        display_name: "Edgecore",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 259, 6, 10, 94],
            &[1, 3, 6, 1, 4, 1, 259, 8, 1, 4],
            &[1, 3, 6, 1, 4, 1, 259, 8, 1, 5],
            &[1, 3, 6, 1, 4, 1, 259, 8, 1, 11],
            &[1, 3, 6, 1, 4, 1, 259, 8, 2, 4],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 5],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 22],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 24],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 27],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 27, 101],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 39],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 42],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 43],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 45],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 45, 103],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 46],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 46, 104],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 51],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 55],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 57],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 58],
            &[1, 3, 6, 1, 4, 1, 259, 10, 1, 74],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/edgeos-ep.yaml
    only(&VendorRule {
        id: "edgeos-ep",
        display_name: "Ubiquiti Edgepower",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 10002, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/edgeos.yaml
    only(&VendorRule {
        id: "edgeos",
        display_name: "EdgeOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 41112, 1, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/edgeosolt.yaml
    only(&VendorRule {
        id: "edgeosolt",
        display_name: "UFiber",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4413], &[1, 3, 6, 1, 4, 1, 41112, 1, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/edgeswitch.yaml
    only(&VendorRule {
        id: "edgeswitch",
        display_name: "EdgeSwitch",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 4413],
            &[1, 3, 6, 1, 4, 1, 10002],
            &[1, 3, 6, 1, 4, 1, 27282, 3, 2, 10],
            &[1, 3, 6, 1, 4, 1, 41112],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eds.yaml
    only(&VendorRule {
        id: "eds",
        display_name: "EDS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 31440]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ees.yaml
    only(&VendorRule {
        id: "ees",
        display_name: "Emerson Energy System",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["linux at9263", "linux es_controller"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ekinops.yaml
    only(&VendorRule {
        id: "ekinops",
        display_name: "Ekinops Optical",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 20044]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/elg-ipecs-es.yaml
    only(&VendorRule {
        id: "elg-ipecs-es",
        display_name: "EricssonLG IPECS ES",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 572, 17389]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/elg-ipecs-ucp.yaml
    only(&VendorRule {
        id: "elg-ipecs-ucp",
        display_name: "Ericsson LG iPECS UCP",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 572, 16838]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eltek-webpower.yaml
    only(&VendorRule {
        id: "eltek-webpower",
        display_name: "Eltek WebPower",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12148, 9]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eltex-mes21xx.yaml
    only(&VendorRule {
        id: "eltex-mes21xx",
        display_name: "Eltex",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 35265, 1, 24],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 43],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 52],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 143],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 154],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eltex-mes23xx.yaml
    only(&VendorRule {
        id: "eltex-mes23xx",
        display_name: "Eltex",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 35265, 1, 74],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 76],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 78],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 80],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 81],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 83],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 86],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 89],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 92],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 96],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 107],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 112],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 116],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 117],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 120],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 137],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 142],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 190],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 192],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 225],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 233],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 234],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 235],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 262],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 266],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 282],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 290],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 313],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 318],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eltex-mes24xx.yaml
    only(&VendorRule {
        id: "eltex-mes24xx",
        display_name: "Eltex",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 35265, 1, 158],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 176],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 217],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 250],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eltex-olt.yaml
    only(&VendorRule {
        id: "eltex-olt",
        display_name: "Eltex OLT",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 35265, 1, 13],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 22],
            &[1, 3, 6, 1, 4, 1, 35265, 1, 70],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/enexus.yaml
    only(&VendorRule {
        id: "enexus",
        display_name: "Eltek Valere eNexus",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12148, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/engenius-switch.yaml
    only(&VendorRule {
        id: "engenius-switch",
        display_name: "EnGenius Switch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 44194]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/engenius.yaml
    only(&VendorRule {
        id: "engenius",
        display_name: "EnGenius Access Point",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 14125],
            &[1, 3, 6, 1, 4, 1, 14125, 100, 1, 3],
            &[1, 3, 6, 1, 4, 1, 14125, 101, 1, 3],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/enlogic-pdu.yaml
    only(&VendorRule {
        id: "enlogic-pdu",
        display_name: "enLogic PDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 38446]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/enterasys.yaml
    only(&VendorRule {
        id: "enterasys",
        display_name: "Enterasys",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 5624, 2, 1]],
        descr_keywords: &["enterasys networks"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/epmp.yaml
    only(&VendorRule {
        id: "epmp",
        display_name: "Cambium epmp",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17713, 21]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/epson-projector.yaml
    only(&VendorRule {
        id: "epson-projector",
        display_name: "Epson Projector",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1248, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/epson-scanner.yaml
    only(&VendorRule {
        id: "epson-scanner",
        display_name: "Epson Scanner",
        aliases: &[],
        sysobjectid_prefixes: &[&[
            1, 3, 6, 1, 4, 1, 1248, 1, 1, 2, 1, 3, 22, 78, 101, 116, 119, 111, 114, 107, 32, 73,
            110, 116, 101, 114, 102, 97, 99, 101, 32, 85, 110, 105, 116,
        ]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/epson.yaml
    only(&VendorRule {
        id: "epson",
        display_name: "Epson Printer",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["epson built-in"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/equallogic.yaml
    only(&VendorRule {
        id: "equallogic",
        display_name: "Dell EqualLogic",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 12740, 12, 1],
            &[1, 3, 6, 1, 4, 1, 12740, 17, 1],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ericsson-6600.yaml
    only(&VendorRule {
        id: "ericsson-6600",
        display_name: "Ericsson 6600 series",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 193, 81, 1, 1, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ericsson-ipos.yaml
    only(&VendorRule {
        id: "ericsson-ipos",
        display_name: "Ericsson IPOS",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 193, 218, 1, 17],
            &[1, 3, 6, 1, 4, 1, 193, 218, 1, 18],
            &[1, 3, 6, 1, 4, 1, 193, 218, 1, 19],
            &[1, 3, 6, 1, 4, 1, 193, 218, 1, 40],
            &[1, 3, 6, 1, 4, 1, 193, 218, 1, 41],
            &[1, 3, 6, 1, 4, 1, 193, 218, 1, 42],
            &[1, 3, 6, 1, 4, 1, 193, 218, 1, 43],
            &[1, 3, 6, 1, 4, 1, 193, 218, 1, 44],
            &[1, 3, 6, 1, 4, 1, 193, 218, 1, 45],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ericsson-ml.yaml
    only(&VendorRule {
        id: "ericsson-ml",
        display_name: "Ericsson MINI-LINK",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 193, 223, 2, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ericsson-tn.yaml
    only(&VendorRule {
        id: "ericsson-tn",
        display_name: "Ericsson Traffic Node",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 193, 81, 1, 1, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/esphome.yaml
    only(&VendorRule {
        id: "esphome",
        display_name: "ESP-Home",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["esphome"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/etherwan.yaml
    only(&VendorRule {
        id: "etherwan",
        display_name: "Etherwan Managed Switch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2736]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ets.yaml
    only(&VendorRule {
        id: "ets",
        display_name: "Gude Expert Transfer Switch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 28507, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/eurostor.yaml
    only(&VendorRule {
        id: "eurostor",
        display_name: "EUROstor RAID Subsystem",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 14752, 1, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ewc.yaml
    only(&VendorRule {
        id: "ewc",
        display_name: "Extreme Wireless Convergence",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4329, 15, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/exa.yaml
    only(&VendorRule {
        id: "exa",
        display_name: "Calix EXA",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6321, 1, 2, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/exagridos.yaml
    only(&VendorRule {
        id: "exagridos",
        display_name: "Exagrid",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["exagrid"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/exalink-fusion.yaml
    only(&VendorRule {
        id: "exalink-fusion",
        display_name: "Cisco Nexus 3550 Series",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 43296]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/exinda.yaml
    only(&VendorRule {
        id: "exinda",
        display_name: "Exinda",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21091]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/exos.yaml
    only(&VendorRule {
        id: "exos",
        display_name: "Exos",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 347, 2, 5730]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/extendair.yaml
    only(&VendorRule {
        id: "extendair",
        display_name: "Exalt ExtendAir",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 25651, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/extremeware.yaml
    only(&VendorRule {
        id: "extremeware",
        display_name: "Extremeware",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1916, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/f5.yaml
    only(&VendorRule {
        id: "f5",
        display_name: "F5 Big IP",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3375, 2, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/f5os.yaml
    only(&VendorRule {
        id: "f5os",
        display_name: "F5OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12276]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fabos.yaml
    only(&VendorRule {
        id: "fabos",
        display_name: "Brocade FabricOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1588, 2, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/feos.yaml
    only(&VendorRule {
        id: "feos",
        display_name: "FireEye OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 25597, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fiberhome-switch.yaml
    only(&VendorRule {
        id: "fiberhome-switch",
        display_name: "FiberHome Switch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3807, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fibernet-xmux4-plus.yaml
    only(&VendorRule {
        id: "fibernet-xmux4-plus",
        display_name: "Fibernet XMUX 4+",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 29003, 2, 44, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fibrolan-falcon.yaml
    only(&VendorRule {
        id: "fibrolan-falcon",
        display_name: "FibroLAN Falcon OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4467, 1000]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fibrolan-metrostar.yaml
    only(&VendorRule {
        id: "fibrolan-metrostar",
        display_name: "FibroLAN MetroStar OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4467, 100, 100, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/firebrick.yaml
    only(&VendorRule {
        id: "firebrick",
        display_name: "Firebrick",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 24693, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fireware.yaml
    only(&VendorRule {
        id: "fireware",
        display_name: "Watchguard Fireware",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 3097, 1, 4],
            &[1, 3, 6, 1, 4, 1, 3097, 1, 5],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/flareos.yaml
    only(&VendorRule {
        id: "flareos",
        display_name: "EMC Flare OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1981, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/flexbng.yaml
    only(&VendorRule {
        id: "flexbng",
        display_name: "NetElastic Flex BNG",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 54268]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/forcepoint-fe.yaml
    only(&VendorRule {
        id: "forcepoint-fe",
        display_name: "Forcepoint FlexEdge Secure SD-WAN",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 47565, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/forcepoint.yaml
    only(&VendorRule {
        id: "forcepoint",
        display_name: "Forcepoint NGFW",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1369, 5, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fortiadc.yaml
    only(&VendorRule {
        id: "fortiadc",
        display_name: "Fortinet Application Deliver Controller",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12356, 112, 100]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fortiap.yaml
    only(&VendorRule {
        id: "fortiap",
        display_name: "Fortinet FortiAP",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12356, 120, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fortiauthenticator.yaml
    only(&VendorRule {
        id: "fortiauthenticator",
        display_name: "Fortinet FortiAuthenticator",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12356, 113, 100]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fortiextender.yaml
    only(&VendorRule {
        id: "fortiextender",
        display_name: "Fortinet FortiExtender",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12356, 121]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fortigate.yaml
    only(&VendorRule {
        id: "fortigate",
        display_name: "Fortinet Fortigate",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 12356, 15],
            &[1, 3, 6, 1, 4, 1, 12356, 101, 1],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fortimail.yaml
    only(&VendorRule {
        id: "fortimail",
        display_name: "Fortinet FortiMail",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12356, 105]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fortios.yaml
    only(&VendorRule {
        id: "fortios",
        display_name: "FortiOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12356, 103]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fortisandbox.yaml
    only(&VendorRule {
        id: "fortisandbox",
        display_name: "Fortinet FortiSandbox",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12356, 118, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fortiswitch.yaml
    only(&VendorRule {
        id: "fortiswitch",
        display_name: "Fortinet FortiSwitch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12356, 106]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fortivoice.yaml
    only(&VendorRule {
        id: "fortivoice",
        display_name: "Fortinet FortiVoice",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12356, 115]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fortiweb.yaml
    only(&VendorRule {
        id: "fortiweb",
        display_name: "Fortinet FortiWeb",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12356, 107, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fortiwlc.yaml
    only(&VendorRule {
        id: "fortiwlc",
        display_name: "Fortinet FortiWLC",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 15983, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/foundryos.yaml
    only(&VendorRule {
        id: "foundryos",
        display_name: "Foundry Networking",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["foundry networks"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fs-bdcom.yaml
    only(&VendorRule {
        id: "fs-bdcom",
        display_name: "Fiberstore (BDCOM)",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 52642, 1, 445, 0],
            &[1, 3, 6, 1, 4, 1, 52642, 1, 446, 0],
            &[1, 3, 6, 1, 4, 1, 52642, 1, 458, 0],
            &[1, 3, 6, 1, 4, 1, 52642, 1, 504, 0],
            &[1, 3, 6, 1, 4, 1, 52642, 1, 2039, 0],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fs-centec.yaml
    only(&VendorRule {
        id: "fs-centec",
        display_name: "Fiberstore (Centec CNOS)",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 52642, 1, 99]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fs-gbn.yaml
    only(&VendorRule {
        id: "fs-gbn",
        display_name: "Fiberstore GBN",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 13464]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fs-net-pdu.yaml
    only(&VendorRule {
        id: "fs-net-pdu",
        display_name: "FS.COM monitored pdu",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 30966]],
        descr_keywords: &["net-pdu"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fs-ruijie.yaml
    only(&VendorRule {
        id: "fs-ruijie",
        display_name: "Fiberstore (Ruijie)",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fs-switch-dc.yaml
    only(&VendorRule {
        id: "fs-switch-dc",
        display_name: "Fiberstore Datacenter Switch",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 1, 514],
            &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 1, 516],
            &[1, 3, 6, 1, 4, 1, 52642, 1, 1, 10, 1, 805],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fs-switch.yaml
    only(&VendorRule {
        id: "fs-switch",
        display_name: "Fiberstore Switch",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 12345],
            &[1, 3, 6, 1, 4, 1, 27975],
            &[1, 3, 6, 1, 4, 1, 51134],
            &[1, 3, 6, 1, 4, 1, 52642],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ftd.yaml
    only(&VendorRule {
        id: "ftd",
        display_name: "Cisco FTD",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 9, 1, 1902],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2294],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2295],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2313],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2315],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2319],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2404],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2405],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2406],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2407],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2409],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2483],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2662],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2663],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2774],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2775],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2778],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2870],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2991],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3041],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3043],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3053],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3054],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3055],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3056],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3057],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3166],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3257],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3305],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3422],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ftos.yaml
    only(&VendorRule {
        id: "ftos",
        display_name: "Force10 FTOS",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &[
            "force10 networks real time operating system software",
            "force10 operating system",
        ],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fujifilmprinter.yaml
    only(&VendorRule {
        id: "fujifilmprinter",
        display_name: "Fujifilm Printer",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 297, 1, 11, 93, 2, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fujitsu-irmc.yaml
    only(&VendorRule {
        id: "fujitsu-irmc",
        display_name: "FUJITSU iRMC",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 231]],
        descr_keywords: &["fujitsu out-of-band snmp agent for irmc"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fujitsueternusos.yaml
    only(&VendorRule {
        id: "fujitsueternusos",
        display_name: "Fujitsu ETERNUS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 211, 1, 21, 1, 150]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fujitsupyos.yaml
    only(&VendorRule {
        id: "fujitsupyos",
        display_name: "Fujitsu",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["py cb eth switch"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fusion.yaml
    only(&VendorRule {
        id: "fusion",
        display_name: "IgniteNet FusionSwitch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 259, 6, 10, 120]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fusionhub.yaml
    only(&VendorRule {
        id: "fusionhub",
        display_name: "FusionHub",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 23695]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fxm.yaml
    only(&VendorRule {
        id: "fxm",
        display_name: "Alpha FXM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7309, 6]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/fxos.yaml
    only(&VendorRule {
        id: "fxos",
        display_name: "Cisco FX-OS",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["cisco fx-os"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/gaia.yaml
    only(&VendorRule {
        id: "gaia",
        display_name: "Check Point GAiA",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 2620, 1, 1],
            &[1, 3, 6, 1, 4, 1, 2620, 1, 6, 123, 1],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/gam.yaml
    only(&VendorRule {
        id: "gam",
        display_name: "Positron Access Solutions GAM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 20095, 2000, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/gamatronicups.yaml
    only(&VendorRule {
        id: "gamatronicups",
        display_name: "Gamatronic UPS Stack",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6050, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ge-ups.yaml
    only(&VendorRule {
        id: "ge-ups",
        display_name: "GE Digital Energy UPS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 818, 1, 100, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/geist-pdu.yaml
    only(&VendorRule {
        id: "geist-pdu",
        display_name: "Geist PDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21239, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/geist-watchdog.yaml
    only(&VendorRule {
        id: "geist-watchdog",
        display_name: "Geist Watchdog",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21239]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/gemds.yaml
    only(&VendorRule {
        id: "gemds",
        display_name: "GE MDS Orbit network Operating System",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4130, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/genieatm.yaml
    only(&VendorRule {
        id: "genieatm",
        display_name: "GenieATM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9926, 1, 1, 21]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/gepulsar.yaml
    only(&VendorRule {
        id: "gepulsar",
        display_name: "GE Pulsar",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 10520, 2, 1, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/gestetner.yaml
    only(&VendorRule {
        id: "gestetner",
        display_name: "Gestetner Printer",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["gestetner network printer"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/gigavue.yaml
    only(&VendorRule {
        id: "gigavue",
        display_name: "GigaVUE",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 26866, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/gos.yaml
    only(&VendorRule {
        id: "gos",
        display_name: "Greenbone OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 35847, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/grandstream-sw.yaml
    only(&VendorRule {
        id: "grandstream-sw",
        display_name: "Grandstream Switch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 42397]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/gude-epc.yaml
    only(&VendorRule {
        id: "gude-epc",
        display_name: "Gude Expert Power Control",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 28507, 1],
            &[1, 3, 6, 1, 4, 1, 28507, 43],
            &[1, 3, 6, 1, 4, 1, 28507, 50],
            &[1, 3, 6, 1, 4, 1, 28507, 56],
            &[1, 3, 6, 1, 4, 1, 28507, 58],
            &[1, 3, 6, 1, 4, 1, 28507, 64],
            &[1, 3, 6, 1, 4, 1, 28507, 68],
            &[1, 3, 6, 1, 4, 1, 28507, 69],
            &[1, 3, 6, 1, 4, 1, 28507, 77],
            &[1, 3, 6, 1, 4, 1, 28507, 81],
            &[1, 3, 6, 1, 4, 1, 28507, 82],
            &[1, 3, 6, 1, 4, 1, 28507, 83],
            &[1, 3, 6, 1, 4, 1, 28507, 85],
            &[1, 3, 6, 1, 4, 1, 28507, 87],
            &[1, 3, 6, 1, 4, 1, 28507, 97],
            &[1, 3, 6, 1, 4, 1, 28507, 98],
            &[1, 3, 6, 1, 4, 1, 28507, 100],
            &[1, 3, 6, 1, 4, 1, 28507, 106],
            &[1, 3, 6, 1, 4, 1, 28507, 107],
            &[1, 3, 6, 1, 4, 1, 28507, 111],
            &[1, 3, 6, 1, 4, 1, 28507, 112],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/gude-esb.yaml
    only(&VendorRule {
        id: "gude-esb",
        display_name: "Expert Sensor Box",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 28507, 66],
            &[1, 3, 6, 1, 4, 1, 28507, 67],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/gw-eydfa.yaml
    only(&VendorRule {
        id: "gw-eydfa",
        display_name: "Glass Way WDM EYDFA",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17409, 1, 11]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/gwd.yaml
    only(&VendorRule {
        id: "gwd",
        display_name: "gwd",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 10072, 2, 20]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/halon.yaml
    only(&VendorRule {
        id: "halon",
        display_name: "Halon",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["halon"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/harmony-enhanced.yaml
    only(&VendorRule {
        id: "harmony-enhanced",
        display_name: "Dragonwave Harmony Enhanced",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7262, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hatteras.yaml
    only(&VendorRule {
        id: "hatteras",
        display_name: "Overture Hatteras DSLAM",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["hn4000-i", "hn4000e"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/helios.yaml
    only(&VendorRule {
        id: "helios",
        display_name: "IgniteNet HeliOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 47307]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/heliosip.yaml
    only(&VendorRule {
        id: "heliosip",
        display_name: "Helios IP",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6530]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hikvision-nvr.yaml
    only(&VendorRule {
        id: "hikvision-nvr",
        display_name: "Hikvision NVR",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 50001]],
        descr_keywords: &["hikvision company products"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/himoinsa-gensets.yaml
    only(&VendorRule {
        id: "himoinsa-gensets",
        display_name: "Himoinsa Generator Sets",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["c2lan snmp"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hirschmann.yaml
    only(&VendorRule {
        id: "hirschmann",
        display_name: "Hirschmann",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 248, 14, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hiveos-network.yaml
    only(&VendorRule {
        id: "hiveos-network",
        display_name: "Aerohive HiveOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4413], &[1, 3, 6, 1, 4, 1, 26928, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/horizon-compact.yaml
    only(&VendorRule {
        id: "horizon-compact",
        display_name: "Dragonwave Horizon Compact",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7262, 2, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/horizon-compactplus.yaml
    only(&VendorRule {
        id: "horizon-compactplus",
        display_name: "Dragonwave Horizon Compact Plus",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7262, 2, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/horizon-duo.yaml
    only(&VendorRule {
        id: "horizon-duo",
        display_name: "Dragonwave Horizon Duo",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7262, 2, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/horizon-quantum.yaml
    only(&VendorRule {
        id: "horizon-quantum",
        display_name: "Dragonwave Horizon Quantum",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7262, 2, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hpblmos.yaml
    only(&VendorRule {
        id: "hpblmos",
        display_name: "HP Blade Management",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 11, 5, 7, 1, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hpe-ilo.yaml
    only(&VendorRule {
        id: "hpe-ilo",
        display_name: "HPE Integrated Lights Out",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 232, 9, 4, 10],
            &[1, 3, 6, 1, 4, 1, 232, 9, 4, 11],
            &[1, 3, 6, 1, 4, 1, 232, 9, 4, 12],
            &[1, 3, 6, 1, 4, 1, 232, 9, 4, 13],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hpe-ipdu.yaml
    only(&VendorRule {
        id: "hpe-ipdu",
        display_name: "HPE iPDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 232, 165, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hpe-mapdu.yaml
    only(&VendorRule {
        id: "hpe-mapdu",
        display_name: "HPE Managed Power Distribution Unit",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 232, 165, 7],
            &[1, 3, 6, 1, 4, 1, 232, 165, 11],
        ],
        descr_keywords: &["outlet metered, outlet switched", "p9s18a"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hpe-msa.yaml
    only(&VendorRule {
        id: "hpe-msa",
        display_name: "HPE MSA",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 11, 2, 51]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hpe-msl.yaml
    only(&VendorRule {
        id: "hpe-msl",
        display_name: "HPE StoreEver MSL",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 11, 10, 2, 1, 3, 25]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hpe-pdumm.yaml
    only(&VendorRule {
        id: "hpe-pdumm",
        display_name: "HP PDU Management Module",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["hp pdu management module"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hpe-rtups.yaml
    only(&VendorRule {
        id: "hpe-rtups",
        display_name: "HP UPS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 232, 165, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hpmsm.yaml
    only(&VendorRule {
        id: "hpmsm",
        display_name: "HP MSM",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 8744, 1, 40],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 41],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 42],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 43],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 44],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 45],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 46],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 47],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 48],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 49],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 50],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 51],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 52],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 53],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 54],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 55],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 56],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 57],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 59],
            &[1, 3, 6, 1, 4, 1, 8744, 1, 67],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hpvc.yaml
    only(&VendorRule {
        id: "hpvc",
        display_name: "HP Virtual Connect",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 11, 5, 7, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/huawei-optixrtn.yaml
    only(&VendorRule {
        id: "huawei-optixrtn",
        display_name: "Huawei OptiX RTN",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2011, 2, 25]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/huawei-smu.yaml
    only(&VendorRule {
        id: "huawei-smu",
        display_name: "Huawei SMU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2011, 6, 164]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hwg-perseus.yaml
    only(&VendorRule {
        id: "hwg-perseus",
        display_name: "HWg Perseus",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21796, 5, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hwg-poseidon.yaml
    only(&VendorRule {
        id: "hwg-poseidon",
        display_name: "HWg Poseidon",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21796, 3, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hwg-ste.yaml
    only(&VendorRule {
        id: "hwg-ste",
        display_name: "HWg STE",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21796, 4, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hwg-ste2.yaml
    only(&VendorRule {
        id: "hwg-ste2",
        display_name: "HWg STE2",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21796, 4, 9]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hwg-wld.yaml
    only(&VendorRule {
        id: "hwg-wld",
        display_name: "HWg WLD",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 21796, 4, 1],
            &[1, 3, 6, 1, 4, 1, 21796, 4, 9],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hyperion.yaml
    only(&VendorRule {
        id: "hyperion",
        display_name: "Bitstream Hyperion",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6603]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/hytera.yaml
    only(&VendorRule {
        id: "hytera",
        display_name: "Hytera Repeater",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 26381]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ibm-amm.yaml
    only(&VendorRule {
        id: "ibm-amm",
        display_name: "IBM AMM",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["bladecenter advanced management module"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ibmc.yaml
    only(&VendorRule {
        id: "ibmc",
        display_name: "Huawei iBMC Management Console",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2011, 2, 235]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ibmnos.yaml
    only(&VendorRule {
        id: "ibmnos",
        display_name: "IBM Networking Operating System",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &[
            "flex system fabric",
            "ibm networking os",
            "networking operating system",
        ],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ibmtl.yaml
    only(&VendorRule {
        id: "ibmtl",
        display_name: "IBM Tape Library",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 14851]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ibos.yaml
    only(&VendorRule {
        id: "ibos",
        display_name: "Waystream iBOS",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 9303, 1, 1],
            &[1, 3, 6, 1, 4, 1, 9303, 1, 5],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/icotera.yaml
    only(&VendorRule {
        id: "icotera",
        display_name: "Icotera OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 29865, 11]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/icr-os.yaml
    only(&VendorRule {
        id: "icr-os",
        display_name: "ICR-OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 30140, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ict-mps.yaml
    only(&VendorRule {
        id: "ict-mps",
        display_name: "ICT Modular Power System",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 39145, 13]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ict-pdu.yaml
    only(&VendorRule {
        id: "ict-pdu",
        display_name: "ICT Distribution Series",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 39145, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ict-psu.yaml
    only(&VendorRule {
        id: "ict-psu",
        display_name: "ICT Digital Series Power Supply",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 39145, 11]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ict-swi.yaml
    only(&VendorRule {
        id: "ict-swi",
        display_name: "ICT Sine Wave Inverter",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 39145, 12]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ies.yaml
    only(&VendorRule {
        id: "ies",
        display_name: "ZyXEL DSLAM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 890]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ies5000.yaml
    only(&VendorRule {
        id: "ies5000",
        display_name: "ZyXEL IES-5000 DSLAM",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 890, 1, 5, 13, 7],
            &[1, 3, 6, 1, 4, 1, 890, 1, 5, 13, 10],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ies52xxM.yaml
    only(&VendorRule {
        id: "ies52xxM",
        display_name: "ZyXEL IES MSAN",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 890, 1, 5, 13, 13],
            &[1, 3, 6, 1, 4, 1, 890, 1, 5, 13, 15],
            &[1, 3, 6, 1, 4, 1, 890, 1, 5, 13, 16],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ifotec.yaml
    only(&VendorRule {
        id: "ifotec",
        display_name: "Ifotec",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21362]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/illustra.yaml
    only(&VendorRule {
        id: "illustra",
        display_name: "Illustra Network Camera",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["illustra"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/imcopower-big.yaml
    only(&VendorRule {
        id: "imcopower-big",
        display_name: "Imco Power",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 818, 32, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/imcopower-ls110.yaml
    only(&VendorRule {
        id: "imcopower-ls110",
        display_name: "Imco Power LS/PS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 33283, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/infinera-groove.yaml
    only(&VendorRule {
        id: "infinera-groove",
        display_name: "Infinera Groove",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 42229, 1, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/infinera-xtm.yaml
    only(&VendorRule {
        id: "infinera-xtm",
        display_name: "Infinera XTM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 8708]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/infinity.yaml
    only(&VendorRule {
        id: "infinity",
        display_name: "LigoWave Infinity",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 10002, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/informos.yaml
    only(&VendorRule {
        id: "informos",
        display_name: "HPE 3PAR",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12925, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/infortrend.yaml
    only(&VendorRule {
        id: "infortrend",
        display_name: "EonStor GS DS-3016",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 1714, 1],
            &[1, 3, 6, 1, 4, 1, 2021, 4, 6],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/innovaphone.yaml
    only(&VendorRule {
        id: "innovaphone",
        display_name: "Innovaphone ISDN",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6666]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/intelliflash.yaml
    only(&VendorRule {
        id: "intelliflash",
        display_name: "Tegile IntelliFlash",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 43906, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/inteno.yaml
    only(&VendorRule {
        id: "inteno",
        display_name: "Inteno GW",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4413]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/interseptor.yaml
    only(&VendorRule {
        id: "interseptor",
        display_name: "Jacarta InterSeptor",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 19011, 1, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ionodes.yaml
    only(&VendorRule {
        id: "ionodes",
        display_name: "IONODES",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 40748, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ionos.yaml
    only(&VendorRule {
        id: "ionos",
        display_name: "Sagem ADR IONOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1038]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ios.yaml
    only(&VendorRule {
        id: "ios",
        display_name: "Cisco IOS",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 9, 1, 2330],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2331],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2332],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2333],
        ],
        descr_keywords: &[
            "cisco internetwork operating system software",
            "cisco ios software",
            "global site selector",
            "ios (tm)",
        ],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/iosxe.yaml
    only(&VendorRule {
        id: "iosxe",
        display_name: "Cisco IOS-XE",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 9, 1, 2683],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2684],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2685],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2686],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2687],
        ],
        descr_keywords: &["cat3k_caa", "ios-xe", "iosxe", "linux_iosd"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/iosxr.yaml
    only(&VendorRule {
        id: "iosxr",
        display_name: "Cisco IOS-XR",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["ios xr"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ipolis.yaml
    only(&VendorRule {
        id: "ipolis",
        display_name: "Hanwha Techwin",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 36849, 1, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ipoman.yaml
    only(&VendorRule {
        id: "ipoman",
        display_name: "Ingrasys iPoMan",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2468, 1, 4, 2, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/iqnos.yaml
    only(&VendorRule {
        id: "iqnos",
        display_name: "Infinera IQNOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21296]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ird.yaml
    only(&VendorRule {
        id: "ird",
        display_name: "PBI Digital Decoder",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2021, 250, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ironware.yaml
    only(&VendorRule {
        id: "ironware",
        display_name: "Brocade IronWare",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["ironware"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ise.yaml
    only(&VendorRule {
        id: "ise",
        display_name: "Cisco Identity Services Engine",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 9, 1, 1425],
            &[1, 3, 6, 1, 4, 1, 9, 1, 1426],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2139],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2266],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2784],
            &[1, 3, 6, 1, 4, 1, 9, 1, 2785],
            &[1, 3, 6, 1, 4, 1, 9, 1, 3438],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/iskratel-lumia.yaml
    only(&VendorRule {
        id: "iskratel-lumia",
        display_name: "Iskratel Lumia",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1332, 1, 1, 1, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/istars.yaml
    only(&VendorRule {
        id: "istars",
        display_name: "East",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 40614]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/janitza-umg96.yaml
    only(&VendorRule {
        id: "janitza-umg96",
        display_name: "Janitza UMG96",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 34278, 8]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/janitza.yaml
    only(&VendorRule {
        id: "janitza",
        display_name: "Janitza",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 34278, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/jetdirect.yaml
    only(&VendorRule {
        id: "jetdirect",
        display_name: "HP Print server",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 11, 2, 3, 9, 1]],
        descr_keywords: &["hp ethernet multi-environment", "hp laser mfp", "jetdirect"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/jetstream.yaml
    only(&VendorRule {
        id: "jetstream",
        display_name: "TP-Link JetStream",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 11863, 5]],
        descr_keywords: &["jetstream"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/juniper-mss.yaml
    only(&VendorRule {
        id: "juniper-mss",
        display_name: "Juniper MSS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 14525, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/juniperex2500os.yaml
    only(&VendorRule {
        id: "juniperex2500os",
        display_name: "Juniper EX2500",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1411, 102]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/junos.yaml
    only(&VendorRule {
        id: "junos",
        display_name: "Juniper JunOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2636]],
        descr_keywords: &["kernel junos"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/junose.yaml
    only(&VendorRule {
        id: "junose",
        display_name: "Juniper JunOSe",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4874]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/jwos.yaml
    only(&VendorRule {
        id: "jwos",
        display_name: "Juniper JWOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 8239, 1, 2, 9]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/kemp.yaml
    only(&VendorRule {
        id: "kemp",
        display_name: "Kemp Loadbalancer",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12196, 250, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/konica.yaml
    only(&VendorRule {
        id: "konica",
        display_name: "Konica-Minolta Printer",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 18334, 1]],
        descr_keywords: &["konica minolta"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/kymata.yaml
    only(&VendorRule {
        id: "kymata",
        display_name: "Kymata",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 60429, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/lambdatrail.yaml
    only(&VendorRule {
        id: "lambdatrail",
        display_name: "Lambdatrail",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 35616]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/lanier.yaml
    only(&VendorRule {
        id: "lanier",
        display_name: "Lanier Printer",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["lanier"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/lantronix-slc.yaml
    only(&VendorRule {
        id: "lantronix-slc",
        display_name: "Lantronix SLC",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 244, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/lantronix-uds.yaml
    only(&VendorRule {
        id: "lantronix-uds",
        display_name: "Lantronix UDS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1723, 2, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/lcos.yaml
    only(&VendorRule {
        id: "lcos",
        display_name: "Lancom OS",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 2356, 11, 8],
            &[1, 3, 6, 1, 4, 1, 2356, 600, 6],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/lcoslx.yaml
    only(&VendorRule {
        id: "lcoslx",
        display_name: "Lancom Systems LCOS LX",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2356, 13, 8]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/lcossx.yaml
    only(&VendorRule {
        id: "lcossx",
        display_name: "Lancom Systems LCOS SX",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 2356, 14, 8],
            &[1, 3, 6, 1, 4, 1, 2356, 16, 8],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/lenovoemc.yaml
    only(&VendorRule {
        id: "lenovoemc",
        display_name: "LenovoEMC",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["emc soho-nas storage."],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/lexmarkprinter.yaml
    only(&VendorRule {
        id: "lexmarkprinter",
        display_name: "Lexmark Printer",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["lexmark"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/liebert.yaml
    only(&VendorRule {
        id: "liebert",
        display_name: "Liebert",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 476, 1, 42]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/linksys-ss.yaml
    only(&VendorRule {
        id: "linksys-ss",
        display_name: "Linksys Smart Switch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3955, 1000]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/logmaster.yaml
    only(&VendorRule {
        id: "logmaster",
        display_name: "LogMaster",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 935]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/loop-telecom.yaml
    only(&VendorRule {
        id: "loop-telecom",
        display_name: "Loop Telecom Operating System",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 823, 34441]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/luminato.yaml
    only(&VendorRule {
        id: "luminato",
        display_name: "Teleste Luminato",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3715]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/magnum.yaml
    only(&VendorRule {
        id: "magnum",
        display_name: "Tandberg Magnum",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3073, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/marathonups.yaml
    only(&VendorRule {
        id: "marathonups",
        display_name: "Marathon UPS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2021, 1, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/mbg-ltos6.yaml
    only(&VendorRule {
        id: "mbg-ltos6",
        display_name: "LANTIME v6",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 5597, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/mbg-meinbergos.yaml
    only(&VendorRule {
        id: "mbg-meinbergos",
        display_name: "Meinberg microSync",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 5597, 7]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/mcafeewebgateway.yaml
    only(&VendorRule {
        id: "mcafeewebgateway",
        display_name: "Mcafee Web Gateway",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1230, 2, 7, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/mediornet.yaml
    only(&VendorRule {
        id: "mediornet",
        display_name: "Riedel MediorNet",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 23056, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/mellanox.yaml
    only(&VendorRule {
        id: "mellanox",
        display_name: "Mellanox",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 33049, 1, 1, 1],
            &[1, 3, 6, 1, 4, 1, 33049, 2],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/meraki-cloud-controller.yaml
    only(&VendorRule {
        id: "meraki-cloud-controller",
        display_name: "Meraki Cloud Controller",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 29671, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/merakims.yaml
    only(&VendorRule {
        id: "merakims",
        display_name: "Meraki Switch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 29671, 2, 388]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/mgepdu.yaml
    only(&VendorRule {
        id: "mgepdu",
        display_name: "Eaton MGE PDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 705, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/microsemipdsine.yaml
    only(&VendorRule {
        id: "microsemipdsine",
        display_name: "Microsemi PowerDsine Midspan PoE",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7428, 1, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/microsemitime.yaml
    only(&VendorRule {
        id: "microsemitime",
        display_name: "SyncServer",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 9070, 1, 2],
            &[1, 3, 6, 1, 4, 1, 39165, 1, 6],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/microsens-g6.yaml
    only(&VendorRule {
        id: "microsens-g6",
        display_name: "G6",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3181, 10, 6]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/mimosa.yaml
    only(&VendorRule {
        id: "mimosa",
        display_name: "Mimosa",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 43356]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/minkelsrms.yaml
    only(&VendorRule {
        id: "minkelsrms",
        display_name: "Minkels RMS",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["8vd-x20"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/mni.yaml
    only(&VendorRule {
        id: "mni",
        display_name: "MNI Microwave Radio",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3323, 11, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/monowall.yaml
    only(&VendorRule {
        id: "monowall",
        display_name: "m0n0wall",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["m0n0wall"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/morningstar-mppt.yaml
    only(&VendorRule {
        id: "morningstar-mppt",
        display_name: "MorningStar MPPT",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 33333]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/morningstar-pwm.yaml
    only(&VendorRule {
        id: "morningstar-pwm",
        display_name: "MorningStar PWM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 33333]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/motorola-cm.yaml
    only(&VendorRule {
        id: "motorola-cm",
        display_name: "Motorola DOCSIS Cable Modem",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1166, 1, 450, 12, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/moxa-awk.yaml
    only(&VendorRule {
        id: "moxa-awk",
        display_name: "Moxa AWK",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 8691, 15, 34]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/moxa-etherdevice.yaml
    only(&VendorRule {
        id: "moxa-etherdevice",
        display_name: "Moxa",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 8691, 7]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/moxa-nport.yaml
    only(&VendorRule {
        id: "moxa-nport",
        display_name: "Moxa",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 8691, 2, 7],
            &[1, 3, 6, 1, 4, 1, 8691, 2, 8],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/mrv-od.yaml
    only(&VendorRule {
        id: "mrv-od",
        display_name: "MRV OptiDriver",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 629, 200]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/mts-com.yaml
    only(&VendorRule {
        id: "mts-com",
        display_name: "Majorpower Majortel DC Power System",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 40865]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/mx-nos.yaml
    only(&VendorRule {
        id: "mx-nos",
        display_name: "MX-NOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 8691, 600]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/mypoweros.yaml
    only(&VendorRule {
        id: "mypoweros",
        display_name: "Maipu MyPower",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 5651, 1, 101],
            &[1, 3, 6, 1, 4, 1, 5651, 1, 102],
            &[1, 3, 6, 1, 4, 1, 5651, 1, 103],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/neptune.yaml
    only(&VendorRule {
        id: "neptune",
        display_name: "Neptune",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1286, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/netagent2.yaml
    only(&VendorRule {
        id: "netagent2",
        display_name: "MegaTec NetAgent II",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 935]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/netapp.yaml
    only(&VendorRule {
        id: "netapp",
        display_name: "NetApp",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["netapp"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/netbotz.yaml
    only(&VendorRule {
        id: "netbotz",
        display_name: "NetBotz Environment Sensor",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 5528, 100, 20, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/netguardian.yaml
    only(&VendorRule {
        id: "netguardian",
        display_name: "DPS Telecom NetGuardian",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2682]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/netmanplus.yaml
    only(&VendorRule {
        id: "netmanplus",
        display_name: "NetMan Plus",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 5491]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/netmodule.yaml
    only(&VendorRule {
        id: "netmodule",
        display_name: "NetModule",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 31496, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/netonix.yaml
    only(&VendorRule {
        id: "netonix",
        display_name: "Netonix",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 46242]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/netscaler-sdwan.yaml
    only(&VendorRule {
        id: "netscaler-sdwan",
        display_name: "NetScaler SD-WAN",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3845, 31]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/netscaler.yaml
    only(&VendorRule {
        id: "netscaler",
        display_name: "Citrix Netscaler",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 5951, 1], &[1, 3, 6, 1, 4, 1, 5951, 6]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/netspire.yaml
    only(&VendorRule {
        id: "netspire",
        display_name: "Open Access Netspire",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1732]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/netsure.yaml
    only(&VendorRule {
        id: "netsure",
        display_name: "Emerson Netsure",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 885, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/nexans.yaml
    only(&VendorRule {
        id: "nexans",
        display_name: "Nexans",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 266]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/nimbleos.yaml
    only(&VendorRule {
        id: "nimbleos",
        display_name: "Nimble OS",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["nimble storage"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/nios.yaml
    only(&VendorRule {
        id: "nios",
        display_name: "Infoblox",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7779]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/nitro.yaml
    only(&VendorRule {
        id: "nitro",
        display_name: "McAfee SIEM Nitro",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 23128, 1000, 1, 1],
            &[1, 3, 6, 1, 4, 1, 23128, 1000, 3, 1],
            &[1, 3, 6, 1, 4, 1, 23128, 1000, 7, 1],
            &[1, 3, 6, 1, 4, 1, 23128, 1000, 11, 1],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/nokia-1830.yaml
    only(&VendorRule {
        id: "nokia-1830",
        display_name: "Nokia 1830 PSS",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 7483, 1, 3, 1, 2],
            &[1, 3, 6, 1, 4, 1, 7483, 1, 3, 1, 6],
            &[1, 3, 6, 1, 4, 1, 7483, 1, 3, 1, 8],
            &[1, 3, 6, 1, 4, 1, 7483, 1, 3, 1, 12],
            &[1, 3, 6, 1, 4, 1, 7483, 1, 3, 1, 13],
            &[1, 3, 6, 1, 4, 1, 7483, 1, 3, 1, 14],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/nokia-isam.yaml
    only(&VendorRule {
        id: "nokia-isam",
        display_name: "NOKIA ISAM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 637, 61]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/nos.yaml
    only(&VendorRule {
        id: "nos",
        display_name: "Brocade NOS",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 1588, 2, 2],
            &[1, 3, 6, 1, 4, 1, 1588, 3, 3],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/nrg.yaml
    only(&VendorRule {
        id: "nrg",
        display_name: "NRG Printer",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["nrg network printer"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ns-bsd.yaml
    only(&VendorRule {
        id: "ns-bsd",
        display_name: "Stormshield NS-BSD",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 11256, 1, 20],
            &[1, 3, 6, 1, 4, 1, 11256, 2, 0],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/nti.yaml
    only(&VendorRule {
        id: "nti",
        display_name: "NTI",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 3699, 1, 1, 9],
            &[1, 3, 6, 1, 4, 1, 3699, 1, 1, 10],
            &[1, 3, 6, 1, 4, 1, 3699, 1, 1, 11],
            &[1, 3, 6, 1, 4, 1, 3699, 1, 1, 12],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ntron.yaml
    only(&VendorRule {
        id: "ntron",
        display_name: "Redlion N-Tron",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 28381]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/nxos.yaml
    only(&VendorRule {
        id: "nxos",
        display_name: "Cisco NX-OS",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["cisco nx-os", "nx-os(tm)"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/occamos.yaml
    only(&VendorRule {
        id: "occamos",
        display_name: "Calix B6 System",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6066, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/oceanstor.yaml
    only(&VendorRule {
        id: "oceanstor",
        display_name: "Huawei OceanStor",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2011, 2, 91]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ocnos.yaml
    only(&VendorRule {
        id: "ocnos",
        display_name: "IP Infusion OcNOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 36673], &[1, 3, 6, 1, 4, 1, 52587]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/okilan.yaml
    only(&VendorRule {
        id: "okilan",
        display_name: "OKI Printer",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["oki okilan"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/omnipcx.yaml
    only(&VendorRule {
        id: "omnipcx",
        display_name: "Alcatel OmniPCX",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 637, 64, 4400, 1, 1, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/omnitron-iconverter.yaml
    only(&VendorRule {
        id: "omnitron-iconverter",
        display_name: "Omnitron iConverter",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7342, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/oneaccess.yaml
    only(&VendorRule {
        id: "oneaccess",
        display_name: "OneAccess",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 13191]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/onefs.yaml
    only(&VendorRule {
        id: "onefs",
        display_name: "EMC Isilon OneFS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12124, 1]],
        descr_keywords: &["isilon onefs"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ons.yaml
    only(&VendorRule {
        id: "ons",
        display_name: "Cisco ONS",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["cisco ons"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/open-e.yaml
    only(&VendorRule {
        id: "open-e",
        display_name: "Open-E",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["open-e"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/opengear-lighthouse.yaml
    only(&VendorRule {
        id: "opengear-lighthouse",
        display_name: "Opengear Lighthouse",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 25049, 1, 42]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/opengear.yaml
    only(&VendorRule {
        id: "opengear",
        display_name: "Opengear",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 25049, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/opnsense.yaml
    only(&VendorRule {
        id: "opnsense",
        display_name: "OPNsense",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["opnsense", "sensey"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/orolia-securesync.yaml
    only(&VendorRule {
        id: "orolia-securesync",
        display_name: "Orolia SecureSync",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 18837, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/orvaldi-ups.yaml
    only(&VendorRule {
        id: "orvaldi-ups",
        display_name: "Orvaldi UPS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 43943]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/osag.yaml
    only(&VendorRule {
        id: "osag",
        display_name: "OpenSystems",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3996, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/osm.yaml
    only(&VendorRule {
        id: "osm",
        display_name: "endace OSm",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 18418]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/packetlight.yaml
    only(&VendorRule {
        id: "packetlight",
        display_name: "Packetlight",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4515, 100, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/packetpower-eg.yaml
    only(&VendorRule {
        id: "packetpower-eg",
        display_name: "Packet Power Enterprise Gateway",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 33688]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/packetshaper.yaml
    only(&VendorRule {
        id: "packetshaper",
        display_name: "Blue Coat PacketShaper",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["packetshaper"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/panasonic-ns.yaml
    only(&VendorRule {
        id: "panasonic-ns",
        display_name: "Panasonic KX-NS Series",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 258, 601, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/pandacom.yaml
    only(&VendorRule {
        id: "pandacom",
        display_name: "PanDacom",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3652]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/panduit-eagle.yaml
    only(&VendorRule {
        id: "panduit-eagle",
        display_name: "Eagle-I",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3711, 24]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/panduit.yaml
    only(&VendorRule {
        id: "panduit",
        display_name: "Panduit PDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 19536]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/panos.yaml
    only(&VendorRule {
        id: "panos",
        display_name: "PanOS",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["palo alto networks"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/paradyne.yaml
    only(&VendorRule {
        id: "paradyne",
        display_name: "Paradyne (by Zhone)",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3278, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/parks-switch.yaml
    only(&VendorRule {
        id: "parks-switch",
        display_name: "PK700",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 50224, 3, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/patton-sn.yaml
    only(&VendorRule {
        id: "patton-sn",
        display_name: "Patton SmartNode",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1768, 100]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/pbn-cp.yaml
    only(&VendorRule {
        id: "pbn-cp",
        display_name: "PBN P2P CP100 Series Platform",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 11606, 24]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/pbn.yaml
    only(&VendorRule {
        id: "pbn",
        display_name: "PBN",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 11606, 10],
            &[1, 3, 6, 1, 4, 1, 11606, 11],
            &[1, 3, 6, 1, 4, 1, 11606, 25],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/pcoweb-rittal-lcp-3311.yaml
    only(&VendorRule {
        id: "pcoweb-rittal-lcp-3311",
        display_name: "Rittal LCP DX Chiller",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9839, 2606, 3311]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/pcoweb-rittalchiller.yaml
    only(&VendorRule {
        id: "pcoweb-rittalchiller",
        display_name: "Rittal IT Chiller",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9839, 2606, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/pcoweb.yaml
    only(&VendorRule {
        id: "pcoweb",
        display_name: "Carel pCOWeb",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["pco gateway"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/pegasus.yaml
    only(&VendorRule {
        id: "pegasus",
        display_name: "Pegasus",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6368, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/pepwave.yaml
    only(&VendorRule {
        id: "pepwave",
        display_name: "Pepwave",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 27662]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/perle.yaml
    only(&VendorRule {
        id: "perle",
        display_name: "Perle",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1966]],
        descr_keywords: &["perle mcr-mgt"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/phybridge-flex.yaml
    only(&VendorRule {
        id: "phybridge-flex",
        display_name: "NVT Phybridge",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 54743, 1, 7]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/picos.yaml
    only(&VendorRule {
        id: "picos",
        display_name: "Pica8 OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 35098]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/pixos.yaml
    only(&VendorRule {
        id: "pixos",
        display_name: "Cisco PIX-OS",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["cisco firewall services module", "cisco pix"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/planet-mc.yaml
    only(&VendorRule {
        id: "planet-mc",
        display_name: "Planet Media Converter Chassis",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 10456, 2, 625]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/planetos.yaml
    only(&VendorRule {
        id: "planetos",
        display_name: "PLANET",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 10456, 1],
            &[1, 3, 6, 1, 4, 1, 10456, 6],
            &[1, 3, 6, 1, 4, 1, 10456, 9],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/pmp.yaml
    only(&VendorRule {
        id: "pmp",
        display_name: "Cambium",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 161, 19]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/polycomLens.yaml
    only(&VendorRule {
        id: "polycomLens",
        display_name: "Polycom Videoconferencing System",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2684]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/poweralert.yaml
    only(&VendorRule {
        id: "poweralert",
        display_name: "Tripp Lite PowerAlert",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 850]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/powercode.yaml
    only(&VendorRule {
        id: "powercode",
        display_name: "Powercode BMU",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["powercode bmu"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/powerconnect.yaml
    only(&VendorRule {
        id: "powerconnect",
        display_name: "Dell PowerConnect",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 674, 10895]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/powerlogic.yaml
    only(&VendorRule {
        id: "powerlogic",
        display_name: "Schneider PowerLogic",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3833, 1, 7, 255, 15]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/powervault.yaml
    only(&VendorRule {
        id: "powervault",
        display_name: "Dell PowerVault",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 674, 10893, 2, 102]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/powerwalker-vfi.yaml
    only(&VendorRule {
        id: "powerwalker-vfi",
        display_name: "PowerWalker VFI",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3808, 1, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/powerwalker.yaml
    only(&VendorRule {
        id: "powerwalker",
        display_name: "PowerWalker UPS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 935, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/prismasdwan.yaml
    only(&VendorRule {
        id: "prismasdwan",
        display_name: "Prisma SD-WAN",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 50114, 11, 1, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/procera.yaml
    only(&VendorRule {
        id: "procera",
        display_name: "Procera Networks",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 15397, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/procurve.yaml
    only(&VendorRule {
        id: "procurve",
        display_name: "HP ProCurve",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 11, 2, 3, 7, 8, 5],
            &[1, 3, 6, 1, 4, 1, 11, 2, 3, 7, 11],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/profline-sfd.yaml
    only(&VendorRule {
        id: "profline-sfd",
        display_name: "PROFline SFD FM Receiver",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21222, 1, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/protelevision-t1.yaml
    only(&VendorRule {
        id: "protelevision-t1",
        display_name: "DVB-T Transmitter",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 18086, 8]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/proxim.yaml
    only(&VendorRule {
        id: "proxim",
        display_name: "Proxim",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 11898, 2, 4, 9]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ptp250.yaml
    only(&VendorRule {
        id: "ptp250",
        display_name: "Cambium PTP 250",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17713, 250]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ptp500.yaml
    only(&VendorRule {
        id: "ptp500",
        display_name: "Cambium PTP 300/500",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17713, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ptp600.yaml
    only(&VendorRule {
        id: "ptp600",
        display_name: "Cambium PTP 600",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17713, 6]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ptp650.yaml
    only(&VendorRule {
        id: "ptp650",
        display_name: "Cambium PTP 650",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17713, 7]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ptp670.yaml
    only(&VendorRule {
        id: "ptp670",
        display_name: "Cambium PTP 670",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17713, 11]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ptp800.yaml
    only(&VendorRule {
        id: "ptp800",
        display_name: "Cambium PTP 800",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17713, 8]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/pulse.yaml
    only(&VendorRule {
        id: "pulse",
        display_name: "Pulse Secure",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 12532]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/purestorage.yaml
    only(&VendorRule {
        id: "purestorage",
        display_name: "Pure Storage",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 40482]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/qnap.yaml
    only(&VendorRule {
        id: "qnap",
        display_name: "QNAP TurboNAS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 24681]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/qtech.yaml
    only(&VendorRule {
        id: "qtech",
        display_name: "QTECH",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 27514]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/quanta.yaml
    only(&VendorRule {
        id: "quanta",
        display_name: "Quanta",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4413], &[1, 3, 6, 1, 4, 1, 7244]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/quantum.yaml
    only(&VendorRule {
        id: "quantum",
        display_name: "Quantum",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3764, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/rad-rici.yaml
    only(&VendorRule {
        id: "rad-rici",
        display_name: "RAD RICi",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 164, 6, 1, 6]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/radlan.yaml
    only(&VendorRule {
        id: "radlan",
        display_name: "Radlan",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 207, 1, 4, 125],
            &[1, 3, 6, 1, 4, 1, 207, 1, 4, 126],
            &[1, 3, 6, 1, 4, 1, 207, 1, 4, 127],
            &[1, 3, 6, 1, 4, 1, 207, 1, 4, 128],
            &[1, 3, 6, 1, 4, 1, 207, 1, 4, 143],
            &[1, 3, 6, 1, 4, 1, 207, 1, 4, 144],
            &[1, 3, 6, 1, 4, 1, 207, 1, 4, 145],
        ],
        descr_keywords: &["at-8000"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/radwin.yaml
    only(&VendorRule {
        id: "radwin",
        display_name: "RADWIN",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 4458, 20, 3],
            &[1, 3, 6, 1, 4, 1, 4458, 20, 5],
            &[1, 3, 6, 1, 4, 1, 4458, 20, 6],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/raisecom-ros.yaml
    only(&VendorRule {
        id: "raisecom-ros",
        display_name: "Raisecom ROS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 8886]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/raisecom.yaml
    only(&VendorRule {
        id: "raisecom",
        display_name: "Raisecom ROAP",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 8886]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/rajant.yaml
    only(&VendorRule {
        id: "rajant",
        display_name: "Rajant Breadcrumb",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 34861, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/raritan-emx.yaml
    only(&VendorRule {
        id: "raritan-emx",
        display_name: "Raritan EMX",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 13742, 8]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/raritan-kvm.yaml
    only(&VendorRule {
        id: "raritan-kvm",
        display_name: "Raritan KVM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 13742, 3, 2, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ray.yaml
    only(&VendorRule {
        id: "ray",
        display_name: "RAy",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 33555, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ray3.yaml
    only(&VendorRule {
        id: "ray3",
        display_name: "RAy3",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 33555, 4, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ricoh.yaml
    only(&VendorRule {
        id: "ricoh",
        display_name: "Ricoh Printer",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["ricoh aficio", "ricoh network printer"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/rittal-cmc-iii-pu.yaml
    only(&VendorRule {
        id: "rittal-cmc-iii-pu",
        display_name: "Rittal CMC III PU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2606, 7]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/rittal-cmc.yaml
    only(&VendorRule {
        id: "rittal-cmc",
        display_name: "Rittal CMC",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2606, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/rittal-lcp.yaml
    only(&VendorRule {
        id: "rittal-lcp",
        display_name: "Rittal LCP",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2606, 7]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/riverbed.yaml
    only(&VendorRule {
        id: "riverbed",
        display_name: "Riverbed",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17163]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/rnx-updu.yaml
    only(&VendorRule {
        id: "rnx-updu",
        display_name: "RNX UPDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 55108, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ros.yaml
    only(&VendorRule {
        id: "ros",
        display_name: "Siemens Ruggedcom Switches (ROS)",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 15004, 2, 1],
            &[1, 3, 6, 1, 4, 1, 15004, 2, 2],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/routeros.yaml
    only(&VendorRule {
        id: "routeros",
        display_name: "Mikrotik RouterOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 14988, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/rs.yaml
    only(&VendorRule {
        id: "rs",
        display_name: "Rohde & Schwarz",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2566, 127, 1, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ruckuswireless-hotzone.yaml
    only(&VendorRule {
        id: "ruckuswireless-hotzone",
        display_name: "Ruckus Wireless HotZone",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 25053, 3, 1, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ruckuswireless-sz.yaml
    only(&VendorRule {
        id: "ruckuswireless-sz",
        display_name: "Ruckus Wireless SmartZone",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 25053, 3, 1, 10],
            &[1, 3, 6, 1, 4, 1, 25053, 3, 1, 11, 1],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ruckuswireless-unleashed.yaml
    only(&VendorRule {
        id: "ruckuswireless-unleashed",
        display_name: "Ruckus Wireless Unleashed",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 25053, 3, 1, 5, 15],
            &[1, 3, 6, 1, 4, 1, 25053, 3, 1, 5, 20],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ruckuswireless.yaml
    only(&VendorRule {
        id: "ruckuswireless",
        display_name: "Ruckus Wireless ZoneDirector",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 25053, 3, 1, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ruijie.yaml
    only(&VendorRule {
        id: "ruijie",
        display_name: "Ruijie Networks",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4881, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/rutos-rutm.yaml
    only(&VendorRule {
        id: "rutos-rutm",
        display_name: "Teltonika RutOS RUTM Series",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 48690]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/saf-cfm.yaml
    only(&VendorRule {
        id: "saf-cfm",
        display_name: "SAF CFM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 2, 22]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/saf-integra-b.yaml
    only(&VendorRule {
        id: "saf-integra-b",
        display_name: "SAF Integra B",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/saf-integra-e.yaml
    only(&VendorRule {
        id: "saf-integra-e",
        display_name: "SAF Integra E",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 9]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/saf-integra-w.yaml
    only(&VendorRule {
        id: "saf-integra-w",
        display_name: "SAF Integra W",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/saf-integra-x.yaml
    only(&VendorRule {
        id: "saf-integra-x",
        display_name: "SAF Integra X",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 7, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/saf.yaml
    only(&VendorRule {
        id: "saf",
        display_name: "SAF Tehnika",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 5],
            &[1, 3, 6, 1, 4, 1, 7571, 100, 1, 1, 12],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/samlex.yaml
    only(&VendorRule {
        id: "samlex",
        display_name: "Samlex Inverter",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 49075]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/samsungprinter.yaml
    only(&VendorRule {
        id: "samsungprinter",
        display_name: "Samsung Printer",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &[
            "samsung c",
            "samsung m4080fx",
            "samsung ml",
            "samsung s",
            "samsung x",
        ],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sanos.yaml
    only(&VendorRule {
        id: "sanos",
        display_name: "Cisco SAN-OS",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["san-os"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/savin.yaml
    only(&VendorRule {
        id: "savin",
        display_name: "Savin Printer",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["savin"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/scalance.yaml
    only(&VendorRule {
        id: "scalance",
        display_name: "Siemens SCALANCE",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 4329, 6, 1, 2, 1],
            &[1, 3, 6, 1, 4, 1, 4329, 6, 1, 2, 3],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/schleifenbauer-pdu5.yaml
    only(&VendorRule {
        id: "schleifenbauer-pdu5",
        display_name: "Schleifenbauer PDU5",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 44568, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/schleifenbauer.yaml
    only(&VendorRule {
        id: "schleifenbauer",
        display_name: "Schleifenbauer SPDM",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 31034, 11, 1],
            &[1, 3, 6, 1, 4, 1, 31034, 11, 3],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/scopia.yaml
    only(&VendorRule {
        id: "scopia",
        display_name: "Avaya Scopia",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 903, 31]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/screenos.yaml
    only(&VendorRule {
        id: "screenos",
        display_name: "Juniper ScreenOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3224]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/scs-ks.yaml
    only(&VendorRule {
        id: "scs-ks",
        display_name: "SCS KS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 9839]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/seh-uds.yaml
    only(&VendorRule {
        id: "seh-uds",
        display_name: "SEH",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1229]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sensatronics-em1.yaml
    only(&VendorRule {
        id: "sensatronics-em1",
        display_name: "Sensatronics EM1",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 16174, 1, 1, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sensatronics-ittm.yaml
    only(&VendorRule {
        id: "sensatronics-ittm",
        display_name: "Sensatronics ITTM",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 16174, 1, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sentry3.yaml
    only(&VendorRule {
        id: "sentry3",
        display_name: "ServerTech Sentry3",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1718, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sentry4.yaml
    only(&VendorRule {
        id: "sentry4",
        display_name: "ServerTech Sentry4",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1718, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/seos.yaml
    only(&VendorRule {
        id: "seos",
        display_name: "Redback Networks SmartEdge",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2352, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/serveriron.yaml
    only(&VendorRule {
        id: "serveriron",
        display_name: "Brocade ServerIron",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["serveriron"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/serverscheck.yaml
    only(&VendorRule {
        id: "serverscheck",
        display_name: "ServersCheck",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 17095]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sgos.yaml
    only(&VendorRule {
        id: "sgos",
        display_name: "BlueCoat ProxySG",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["sgos"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/siklu-mhtg.yaml
    only(&VendorRule {
        id: "siklu-mhtg",
        display_name: "Siklu MultiHaul Terragraph",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 31926, 35]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/siklu.yaml
    only(&VendorRule {
        id: "siklu",
        display_name: "Siklu Wireless",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 31926]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/siteboss.yaml
    only(&VendorRule {
        id: "siteboss",
        display_name: "Asentria SiteBoss",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3052, 12]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/siteboss360.yaml
    only(&VendorRule {
        id: "siteboss360",
        display_name: "Asentria SiteBoss",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3052, 23, 3, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/siteboss550.yaml
    only(&VendorRule {
        id: "siteboss550",
        display_name: "Asentria SiteBoss",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3052, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/siteboss571.yaml
    only(&VendorRule {
        id: "siteboss571",
        display_name: "Asentria SiteBoss",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3052, 14]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/siteboss572.yaml
    only(&VendorRule {
        id: "siteboss572",
        display_name: "Asentria SiteBoss",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3052, 20]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sitemonitor.yaml
    only(&VendorRule {
        id: "sitemonitor",
        display_name: "Packetflux SiteMonitor",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 32050]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sitemonitor3.yaml
    only(&VendorRule {
        id: "sitemonitor3",
        display_name: "Packetflux SiteMonitor Base 3",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 32050, 1, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sixnet.yaml
    only(&VendorRule {
        id: "sixnet",
        display_name: "Red Lion Sixnet",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 20540, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/skyhighwebprotect.yaml
    only(&VendorRule {
        id: "skyhighwebprotect",
        display_name: "Skyhigh Secure Web Gateway",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 1230, 2, 7, 1, 1],
            &[1, 3, 6, 1, 4, 1, 59732, 2, 7, 1, 1],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/slms.yaml
    only(&VendorRule {
        id: "slms",
        display_name: "Zhone MXK",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 5504, 1, 7],
            &[1, 3, 6, 1, 4, 1, 5504, 1, 8],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/slxos.yaml
    only(&VendorRule {
        id: "slxos",
        display_name: "Extreme SLX-OS",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 1588, 3, 3, 1],
            &[1, 3, 6, 1, 4, 1, 1916, 2],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sm-os.yaml
    only(&VendorRule {
        id: "sm-os",
        display_name: "SM-OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3373, 1103]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/smartax-mdu.yaml
    only(&VendorRule {
        id: "smartax-mdu",
        display_name: "Huawei SmartAX MDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2011, 2, 185]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/smartax.yaml
    only(&VendorRule {
        id: "smartax",
        display_name: "Huawei SmartAX",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 2011, 2, 80, 8],
            &[1, 3, 6, 1, 4, 1, 2011, 2, 115],
            &[1, 3, 6, 1, 4, 1, 2011, 2, 123],
            &[1, 3, 6, 1, 4, 1, 2011, 2, 133],
            &[1, 3, 6, 1, 4, 1, 2011, 2, 248],
        ],
        descr_keywords: &["huawei integrated access software"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/smartbyte.yaml
    only(&VendorRule {
        id: "smartbyte",
        display_name: "SmartByte",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 56166]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/smartos-dcp-m.yaml
    only(&VendorRule {
        id: "smartos-dcp-m",
        display_name: "SmartOptics DCP-M Series",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 30826, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/smartos-t.yaml
    only(&VendorRule {
        id: "smartos-t",
        display_name: "SmartOptics T-Series",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4515, 100, 1, 1000]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/smartos.yaml
    only(&VendorRule {
        id: "smartos",
        display_name: "SmartOptics M-Series",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 30826, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/snr-erd.yaml
    only(&VendorRule {
        id: "snr-erd",
        display_name: "snr-erd",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 40418, 2, 6]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/snr-olt.yaml
    only(&VendorRule {
        id: "snr-olt",
        display_name: "SNR OLT",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 65268, 1, 3, 34]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/snr.yaml
    only(&VendorRule {
        id: "snr",
        display_name: "snr",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 40418, 7],
            &[1, 3, 6, 1, 4, 1, 40418, 8, 67],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/socomec-ups.yaml
    only(&VendorRule {
        id: "socomec-ups",
        display_name: "Socomec UPS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4555, 1, 1, 7]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/socomec.yaml
    only(&VendorRule {
        id: "socomec",
        display_name: "Socomec PDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4555, 2, 30]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/solidserver.yaml
    only(&VendorRule {
        id: "solidserver",
        display_name: "EfficientIP SOLIDserver",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2440]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sonicwall.yaml
    only(&VendorRule {
        id: "sonicwall",
        display_name: "SonicWALL",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 8741]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sonus-gsx.yaml
    only(&VendorRule {
        id: "sonus-gsx",
        display_name: "Ribbon GSX",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2879, 1, 1, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sonus-sbc.yaml
    only(&VendorRule {
        id: "sonus-sbc",
        display_name: "Ribbon SBC",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 177, 15, 1, 1],
            &[1, 3, 6, 1, 4, 1, 2879, 1, 9],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sophos-xg.yaml
    only(&VendorRule {
        id: "sophos-xg",
        display_name: "Sophos XG",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2604, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/speedtouch.yaml
    only(&VendorRule {
        id: "speedtouch",
        display_name: "Thomson Speedtouch",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["speedtouch ", "tg585v7"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/srlinux.yaml
    only(&VendorRule {
        id: "srlinux",
        display_name: "Nokia SR Linux",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6527, 1, 20]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sslvis.yaml
    only(&VendorRule {
        id: "sslvis",
        display_name: "Blue Coat SSL Visibility",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3417, 1, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ssu2000.yaml
    only(&VendorRule {
        id: "ssu2000",
        display_name: "Microsemi Synchronization Supply Unit",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 601, 3, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/stellar.yaml
    only(&VendorRule {
        id: "stellar",
        display_name: "Alcatel-Lucent Enterprise Stellar Wireless OS (AWOS)",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6486, 802]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/stoneos.yaml
    only(&VendorRule {
        id: "stoneos",
        display_name: "Hillstone StoneOs",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 28557, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/stulz.yaml
    only(&VendorRule {
        id: "stulz",
        display_name: "Stulz GMBH Klimatechnik",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 29462]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/sub10.yaml
    only(&VendorRule {
        id: "sub10",
        display_name: "Sub10 Systems",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 39003]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/svos.yaml
    only(&VendorRule {
        id: "svos",
        display_name: "Hitachi Storage Virtualization Operating System (SVOS)",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 116, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/swos.yaml
    only(&VendorRule {
        id: "swos",
        display_name: "Mikrotik SwOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 14988, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/symbol.yaml
    only(&VendorRule {
        id: "symbol",
        display_name: "Symbol AP",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 388]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/tachyon.yaml
    only(&VendorRule {
        id: "tachyon",
        display_name: "Tachyon",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 57344]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/tailyn.yaml
    only(&VendorRule {
        id: "tailyn",
        display_name: "Tailyn",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 5833, 2017]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/tait-infra93.yaml
    only(&VendorRule {
        id: "tait-infra93",
        display_name: "Tait Infra93 Series",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3570, 2, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/tait-tnadmin.yaml
    only(&VendorRule {
        id: "tait-tnadmin",
        display_name: "Tait TN Admin OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3570, 3, 15]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/teldat.yaml
    only(&VendorRule {
        id: "teldat",
        display_name: "Teldat",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2007]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/televes-fs.yaml
    only(&VendorRule {
        id: "televes-fs",
        display_name: "Fourth Series [TRedess]",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 23180, 2, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/teracom.yaml
    only(&VendorRule {
        id: "teracom",
        display_name: "TeraCom",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 38783]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/teradici-pcoip.yaml
    only(&VendorRule {
        id: "teradici-pcoip",
        display_name: "Teradici PCoIP",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 25071, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/terra-receivers.yaml
    only(&VendorRule {
        id: "terra-receivers",
        display_name: "Terra",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 30631, 1, 8],
            &[1, 3, 6, 1, 4, 1, 30631, 1, 17],
            &[1, 3, 6, 1, 4, 1, 30631, 1, 18],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/tgos.yaml
    only(&VendorRule {
        id: "tgos",
        display_name: "technicolor TG MediaAcces",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2863, 405, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/thomson-cm.yaml
    only(&VendorRule {
        id: "thomson-cm",
        display_name: "Thomson DOCSIS Cable Modem",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2863, 205, 43014, 420, 4, 0, 82, 4, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/timos.yaml
    only(&VendorRule {
        id: "timos",
        display_name: "Nokia SR OS (TiMOS)",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 6527]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/topvision.yaml
    only(&VendorRule {
        id: "topvision",
        display_name: "TopVision",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 32285, 11]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/toshiba-tec.yaml
    only(&VendorRule {
        id: "toshiba-tec",
        display_name: "Toshiba Printer",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1129, 1, 2]],
        descr_keywords: &["toshiba e-studio"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/tpconductor.yaml
    only(&VendorRule {
        id: "tpconductor",
        display_name: "TelePresence Conductor",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 5596, 180, 6, 4, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/tpdin.yaml
    only(&VendorRule {
        id: "tpdin",
        display_name: "Tycon Systems TPDIN",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 45621, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/tpdin3.yaml
    only(&VendorRule {
        id: "tpdin3",
        display_name: "Tycon Systems TPDIN",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 45621, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/tplink.yaml
    only(&VendorRule {
        id: "tplink",
        display_name: "TP-Link Switch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 11863], &[1, 3, 6, 1, 4, 1, 16972]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/transition.yaml
    only(&VendorRule {
        id: "transition",
        display_name: "Transition",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 868]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/trendnet.yaml
    only(&VendorRule {
        id: "trendnet",
        display_name: "TRENDnet Switch",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 28866, 2, 34],
            &[1, 3, 6, 1, 4, 1, 28866, 2, 46],
            &[1, 3, 6, 1, 4, 1, 28866, 2, 48],
            &[1, 3, 6, 1, 4, 1, 28866, 2, 49],
            &[1, 3, 6, 1, 4, 1, 28866, 2, 51],
            &[1, 3, 6, 1, 4, 1, 28866, 2, 64],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/trippliteconsole.yaml
    only(&VendorRule {
        id: "trippliteconsole",
        display_name: "TrippLite console server",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 850, 1, 5],
            &[1, 3, 6, 1, 4, 1, 850, 1, 7],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/truenas-scale.yaml
    only(&VendorRule {
        id: "truenas-scale",
        display_name: "TrueNAS-SCALE",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 50536, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/truenas.yaml
    only(&VendorRule {
        id: "truenas",
        display_name: "TrueNAS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 50536, 3]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ts-bspdu.yaml
    only(&VendorRule {
        id: "ts-bspdu",
        display_name: "BladeShelter PDU by PowerTek",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 42610, 1, 4, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/tsc.yaml
    only(&VendorRule {
        id: "tsc",
        display_name: "TSC Printer",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 51966]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ubiquoss-pon.yaml
    only(&VendorRule {
        id: "ubiquoss-pon",
        display_name: "Ubiquoss PON",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7800, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ucopia.yaml
    only(&VendorRule {
        id: "ucopia",
        display_name: "Ucopia",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 31218]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ucos.yaml
    only(&VendorRule {
        id: "ucos",
        display_name: "Cisco Unified Communications Manager",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 9, 1, 1348],
            &[1, 3, 6, 1, 4, 1, 9, 12, 3, 1, 3, 1062],
        ],
        descr_keywords: &["cisco nx-os(tm) ucs", "software:ucos"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/uhp.yaml
    only(&VendorRule {
        id: "uhp",
        display_name: "UHP Networks VSAT Terminal",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 8000]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/ulaf.yaml
    only(&VendorRule {
        id: "ulaf",
        display_name: "ULAF+",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1887, 1, 1, 1, 2, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/unifi.yaml
    only(&VendorRule {
        id: "unifi",
        display_name: "Ubiquiti UniFi",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 10002, 1], &[1, 3, 6, 1, 4, 1, 41112]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/uniping.yaml
    only(&VendorRule {
        id: "uniping",
        display_name: "UniPing",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 25728]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/univerge.yaml
    only(&VendorRule {
        id: "univerge",
        display_name: "NEC Univerge",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 119, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/utax.yaml
    only(&VendorRule {
        id: "utax",
        display_name: "UTAX Printer",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1347, 41]],
        descr_keywords: &["ta_utax printing system"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/valere.yaml
    only(&VendorRule {
        id: "valere",
        display_name: "Eltek Valere",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 13858]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vccodec.yaml
    only(&VendorRule {
        id: "vccodec",
        display_name: "TelePresence Codec",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 5596, 150, 6, 4, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vcs.yaml
    only(&VendorRule {
        id: "vcs",
        display_name: "Video Communication Server",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 5596, 130, 6, 4, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vega.yaml
    only(&VendorRule {
        id: "vega",
        display_name: "Sangoma Vega",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 4686]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/velocloud.yaml
    only(&VendorRule {
        id: "velocloud",
        display_name: "VMware SD-WAN",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 45346, 1, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vertiv-dcs.yaml
    only(&VendorRule {
        id: "vertiv-dcs",
        display_name: "DCS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2021, 250, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vertiv-ita2.yaml
    only(&VendorRule {
        id: "vertiv-ita2",
        display_name: "ITA2",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 13400, 2, 54]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vertiv-pdu.yaml
    only(&VendorRule {
        id: "vertiv-pdu",
        display_name: "Vertiv PDU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 21239, 42, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vigintos.yaml
    only(&VendorRule {
        id: "vigintos",
        display_name: "DVB Modulator & Ampiflier",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 27993, 5]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/viprinux.yaml
    only(&VendorRule {
        id: "viprinux",
        display_name: "Viprinux",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["viprinet vpn router"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/viptela.yaml
    only(&VendorRule {
        id: "viptela",
        display_name: "Viptela",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 41916]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vivotek.yaml
    only(&VendorRule {
        id: "vivotek",
        display_name: "Vivotek Camera",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 23465]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/volius.yaml
    only(&VendorRule {
        id: "volius",
        display_name: "Volius",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 34652]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vos.yaml
    only(&VendorRule {
        id: "vos",
        display_name: "Versa Operating System",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 42359, 2, 2, 1, 1, 4, 1, 2]],
        descr_keywords: &["versa appliance"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/voss.yaml
    only(&VendorRule {
        id: "voss",
        display_name: "Extreme VOSS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1916, 2], &[1, 3, 6, 1, 4, 1, 2272]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/voswall.yaml
    only(&VendorRule {
        id: "voswall",
        display_name: "Voswall",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["voswall"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vrp.yaml
    only(&VendorRule {
        id: "vrp",
        display_name: "Huawei VRP",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &[
            "software version vrp",
            "versatile routing platform software",
            "vrp (r) software",
            "vrp software version",
        ],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vsolution.yaml
    only(&VendorRule {
        id: "vsolution",
        display_name: "V-Solution",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 37950]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vubiq.yaml
    only(&VendorRule {
        id: "vubiq",
        display_name: "Vubiq Networks",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 46330, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vutlan.yaml
    only(&VendorRule {
        id: "vutlan",
        display_name: "Vutlan",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 39052]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vxoa.yaml
    only(&VendorRule {
        id: "vxoa",
        display_name: "Silver Peak VXOA",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 23867, 1, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/vyos.yaml
    only(&VendorRule {
        id: "vyos",
        display_name: "VyOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 44641]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/wavence.yaml
    only(&VendorRule {
        id: "wavence",
        display_name: "Nokia Wavence",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 637, 54, 1, 10, 90, 1],
            &[1, 3, 6, 1, 4, 1, 637, 54, 1, 10, 90, 13],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/webmon.yaml
    only(&VendorRule {
        id: "webmon",
        display_name: "Dantel Webmon",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 994, 3, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/webpower-smart2.yaml
    only(&VendorRule {
        id: "webpower-smart2",
        display_name: "WebPower-smart2",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2468, 1, 2, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/webpower.yaml
    only(&VendorRule {
        id: "webpower",
        display_name: "WebPower",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2468, 1, 2, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/websensor.yaml
    only(&VendorRule {
        id: "websensor",
        display_name: "Comet System Web Sensor",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 22626, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/weos.yaml
    only(&VendorRule {
        id: "weos",
        display_name: "Westermo WeOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 16177]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/westmountainradio.yaml
    only(&VendorRule {
        id: "westmountainradio",
        display_name: "West Mountain RMCU",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 15117, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/wipipe.yaml
    only(&VendorRule {
        id: "wipipe",
        display_name: "CradlePoint WiPipe",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 20992]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/wis-bridge.yaml
    only(&VendorRule {
        id: "wis-bridge",
        display_name: "WIS Bridge",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 62821, 1, 4]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/wiseway.yaml
    only(&VendorRule {
        id: "wiseway",
        display_name: "Wiseway UPS adapter",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["wiseway network adapter"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/wisi-tangram.yaml
    only(&VendorRule {
        id: "wisi-tangram",
        display_name: "WISI Tangram",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 7465]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/witchos.yaml
    only(&VendorRule {
        id: "witchos",
        display_name: "Teltonika WitchOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 24686]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/wti-console.yaml
    only(&VendorRule {
        id: "wti-console",
        display_name: "WTI CONSOLE",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2634, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/wti-power.yaml
    only(&VendorRule {
        id: "wti-power",
        display_name: "WTI POWER",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2634, 6]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/wut.yaml
    only(&VendorRule {
        id: "wut",
        display_name: "Web-Thermo-Hygrometer",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 5040, 1, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/wxgoos.yaml
    only(&VendorRule {
        id: "wxgoos",
        display_name: "ITWatchDogs Goose",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 901, 1], &[1, 3, 6, 1, 4, 1, 17373]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/xclarity.yaml
    only(&VendorRule {
        id: "xclarity",
        display_name: "Lenovo XCC IMPI",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 19046, 11, 1]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/xerox.yaml
    only(&VendorRule {
        id: "xerox",
        display_name: "Xerox Printer",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 253, 8, 62, 1],
            &[1, 3, 6, 1, 4, 1, 297, 1, 11, 93, 1],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/xirrus_aos.yaml
    only(&VendorRule {
        id: "xirrus_aos",
        display_name: "Xirrus ArrayOS",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["arrayos", "xirrus"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/xos.yaml
    only(&VendorRule {
        id: "xos",
        display_name: "Extreme XOS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 1916, 2]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/yunshan.yaml
    only(&VendorRule {
        id: "yunshan",
        display_name: "Huawei YunShan OS",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 2011, 2, 23]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/zebra.yaml
    only(&VendorRule {
        id: "zebra",
        display_name: "ZebraNet",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 683], &[1, 3, 6, 1, 4, 1, 10642]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/zmtel.yaml
    only(&VendorRule {
        id: "zmtel",
        display_name: "Zmtel Greenpacket",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 38595, 10]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/zxa10.yaml
    only(&VendorRule {
        id: "zxa10",
        display_name: "ZTE ZXA10",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 3902, 1082]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/zxdsl.yaml
    only(&VendorRule {
        id: "zxdsl",
        display_name: "ZXDSL",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 3902, 1004],
            &[1, 3, 6, 1, 4, 1, 3902, 1015],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/zxr10.yaml
    only(&VendorRule {
        id: "zxr10",
        display_name: "ZTE ZXR10",
        aliases: &[],
        sysobjectid_prefixes: &[],
        descr_keywords: &["zte 5950", "zte ethernet switch", "zxr10"],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/zynos.yaml
    only(&VendorRule {
        id: "zynos",
        display_name: "ZyXEL Ethernet Switch",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 890]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/zywall.yaml
    only(&VendorRule {
        id: "zywall",
        display_name: "ZyXEL ZyWALL",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 890, 1, 6],
            &[1, 3, 6, 1, 4, 1, 890, 1, 15],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/zyxelac.yaml
    only(&VendorRule {
        id: "zyxelac",
        display_name: "ZyXEL AC",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 890]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/zyxelnwa.yaml
    only(&VendorRule {
        id: "zyxelnwa",
        display_name: "ZyXEL NWA",
        aliases: &[],
        sysobjectid_prefixes: &[&[1, 3, 6, 1, 4, 1, 890, 1, 15]],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
    // LibreNMS os_detection/zyxelwlc.yaml
    only(&VendorRule {
        id: "zyxelwlc",
        display_name: "Zyxel Zyxelwlc",
        aliases: &[],
        sysobjectid_prefixes: &[
            &[1, 3, 6, 1, 4, 1, 890, 1, 6],
            &[1, 3, 6, 1, 4, 1, 890, 1, 15],
        ],
        descr_keywords: &[],
        descr_keywords_except: &[],
    }),
];
