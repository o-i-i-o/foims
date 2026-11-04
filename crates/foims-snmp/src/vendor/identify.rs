//! 双证据聚合纯函数：sysObjectID 企业号 + sysDescr 词边界关键词
//! → 平台/厂商/置信度/证据（§6.1）。无 IO，全部输入即可测试。

use async_snmp::Oid;

use super::platform::{self, PlatformRule};
use super::{Confidence, SnmpIdentity, VendorRule, registry};

/// 识别入口：输入 sysObjectID 与 sysDescr（均可缺失）。
pub fn identify(object_id: Option<&Oid>, descr: Option<&str>) -> SnmpIdentity {
    let descr = descr.map(str::trim).filter(|s| !s.is_empty());
    let agent = object_id.and_then(enterprise_of);
    let mut evidence = Vec::new();

    // —— 平台（§3）：sysDescr OS 词强证据；sysDescr 缺失时 OS 专属
    //    agent 企业号弱证据兜底 ——
    let platform_match = platform::detect(descr, agent);
    if let Some(m) = &platform_match {
        if m.strong {
            evidence.push(format!("sysDescr 命中平台 {} 关键词", m.rule.id));
        } else if let Some(enterprise) = agent {
            evidence.push(format!(
                "agent 企业号 {enterprise} 命中平台 {}（弱证据）",
                m.rule.id
            ));
        }
    } else if agent == Some(platform::NET_SNMP_ENTERPRISE) {
        evidence.push("agent 为 net-snmp（企业号 8072），跨 OS，不能定位平台".to_string());
    }
    let os_version = platform_match.and_then(|m| descr.and_then(|d| (m.rule.version_parser)(d)));

    // —— 厂商（§6.1）：平台 agent 企业号不作厂商证据（§3.1）——
    // sysObjectID 前缀匹配：最长前缀胜出（LibreNMS 语义，同企业号下
    // 区分产品线，如 9=Cisco 的 ios/iosxe 深前缀优先于品牌级 7 弧前缀）
    let prefix_match = object_id.and_then(prefix_vendor);
    if let Some((m, prefix)) = &prefix_match {
        let dotted = prefix
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".");
        evidence.push(format!(
            "sysObjectID 前缀 {dotted} 命中 {}",
            m.rule.display_name
        ));
    }
    let ent_vendor = prefix_match.map(|(m, _)| m);
    let tokens = descr.map(vendor_tokens);
    let descr_vendor = tokens.as_ref().and_then(|tk| {
        registry().find(|m| m.rule.descr_keywords.iter().any(|k| keyword_hit(tk, k)))
    });
    if let Some(m) = descr_vendor {
        evidence.push(format!("sysDescr 命中 {}", m.rule.display_name));
    }

    let (vendor_guess, confidence) = match (ent_vendor, descr_vendor) {
        (Some(a), Some(b)) if a.rule.id == b.rule.id => {
            // 双证据一致；排除词命中（OEM 上报他人企业号）降级 Low
            let demoted = tokens.as_ref().is_some_and(|tk| {
                b.rule
                    .descr_keywords_except
                    .iter()
                    .any(|k| keyword_hit(tk, k))
            });
            if demoted {
                evidence.push("命中排除关键词，置信度降级".to_string());
                (Some(b.rule.id), Confidence::Low)
            } else {
                (Some(a.rule.id), Confidence::High)
            }
        }
        (Some(a), Some(b)) => {
            // 冲突：描述词胜出（如 11 + comware → H3C，§6.1）
            evidence.push(format!(
                "企业号指向 {} 与 sysDescr 指向 {} 冲突，描述词胜出",
                a.rule.display_name, b.rule.display_name
            ));
            (Some(b.rule.id), Confidence::Low)
        }
        (Some(a), None) => (Some(a.rule.id), Confidence::Low),
        (None, Some(b)) => (Some(b.rule.id), Confidence::Low),
        (None, None) => (None, Confidence::Low),
    };

    let caps = platform::effective_caps(platform_match.map(|m| m.rule), agent);
    SnmpIdentity {
        platform: platform_match.map(|m| m.rule.id),
        os_version,
        vendor_guess,
        confidence,
        caps,
        evidence,
    }
}

/// sysObjectID → IANA 企业号（第 7 弧，须位于 1.3.6.1.4.1 之下）。
fn enterprise_of(object_id: &Oid) -> Option<u32> {
    let arcs = object_id.arcs();
    (arcs.len() >= 7 && arcs.starts_with(&[1, 3, 6, 1, 4, 1])).then(|| arcs[6])
}

/// sysObjectID →（规则, 命中前缀）：全部规则（内置 + 社区）按最长前缀
/// 胜出；**同长度时内置品牌级优先**（先遍历者保留——内置规则是人工
/// 核实的品牌归并，社区 OS 级仅在更深前缀时细分）。
/// 前缀第 7 弧为平台 agent 号（8072 net-snmp 等）的不作厂商证据（§3.1）。
fn prefix_vendor(object_id: &Oid) -> Option<(&'static super::VendorMib, &'static [u32])> {
    let arcs = object_id.arcs();
    if arcs.len() < 7 || !arcs.starts_with(&[1, 3, 6, 1, 4, 1]) {
        return None;
    }
    if platform::is_platform_agent(arcs[6]) {
        return None;
    }
    registry()
        .filter_map(|m| {
            m.rule
                .sysobjectid_prefixes
                .iter()
                .find(|p| arcs.starts_with(p))
                .map(|p| (m, *p))
        })
        .fold(None, |best, cur| match best {
            // 仅严格更长才替换：保证同长度时内置优先
            Some((_, p)) if p.len() >= cur.1.len() => best,
            _ => Some(cur),
        })
}

/// sysDescr 词边界分词（小写，非字母数字切分）：子串匹配会被
/// "ship"/"Sharp" 之类无关词误命中（沿用原 sysinfo.rs 既有口径）。
fn vendor_tokens(descr: &str) -> Vec<String> {
    descr
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// 厂商关键词命中：关键词按同规则分词后全词元出现——
/// 单词词即词边界精确匹配；连字符/多词品牌（tp-link、
/// versatile routing platform）按全词元命中。
fn keyword_hit(tokens: &[String], keyword: &str) -> bool {
    keyword
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|k| !k.is_empty())
        .all(|k| tokens.iter().any(|t| t == k))
}

/// 平台规则按 id 查找（供 UI/调试展示）。
pub fn platform_rule(id: &str) -> Option<&'static PlatformRule> {
    platform::registry().iter().find(|r| r.id == id)
}

/// 厂商规则按 id 查找（供 UI/调试展示）。
pub fn vendor_rule(id: &str) -> Option<&'static VendorRule> {
    registry().find(|m| m.rule.id == id).map(|m| m.rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(s: &str) -> Oid {
        Oid::parse(s).unwrap()
    }

    fn vendor_of(descr: &str) -> Option<&'static str> {
        identify(None, Some(descr)).vendor_guess
    }

    #[test]
    fn 厂商识别_词表收编与优先级() {
        // 原 sysinfo::identify_vendor 全量用例收编（22 个已知品牌）
        let cases = [
            ("Cisco IOS Software", "cisco"),
            ("Huawei Versatile Routing Platform", "huawei"),
            ("H3C Comware Software", "h3c"),
            ("3Com SuperStack Switch", "h3c"),
            ("Juniper Networks Junos", "juniper"),
            ("Dell EMC Networking", "dell"),
            ("HP ProCurve Switch", "hp"),
            ("HPE OfficeConnect", "hp"),
            ("ProCurve J9021A", "hp"),
            ("ArubaOS-CX Switch", "aruba"),
            ("NETGEAR ProSafe GS724T", "netgear"),
            ("TP-Link JetStream T2600G", "tp-link"),
            ("TPLink Web Smart Switch", "tp-link"),
            ("Linksys LGS318", "linksys"),
            ("Ubiquiti UniFi Switch", "ubiquiti"),
            ("UBNT EdgeSwitch", "ubiquiti"),
            ("MikroTik RouterOS", "mikrotik"),
            ("ExtremeXOS Switch", "extreme"),
            ("Alcatel-Lucent OS6850", "alcatel"),
            ("ZyXEL GS1900", "zyxel"),
            ("D-Link DGS-1210", "d-link"),
            ("Dlink Smart Switch", "d-link"),
        ];
        for (sys_descr, expected) in cases {
            assert_eq!(
                vendor_of(sys_descr),
                Some(expected),
                "sysDescr={sys_descr} 应识别为 {expected}"
            );
        }
        // 判定顺序即优先级：cisco 分支先于 huawei 命中
        assert_eq!(vendor_of("cisco device by huawei oem"), Some("cisco"));
    }

    #[test]
    fn 厂商识别_词边界无子串误命中() {
        // 子串误配回归："ship"/"Sharp" 含 "hp" 子串，词边界匹配后不得命中
        assert_eq!(vendor_of("Friendship Leader 3000"), None);
        assert_eq!(vendor_of("Sharp LCD Panel"), None);
        assert_eq!(vendor_of("hp 2920 Switch"), Some("hp"));
        assert_eq!(vendor_of("Some Generic Device"), None);
        assert_eq!(vendor_of(""), None);
        assert_eq!(identify(None, None).vendor_guess, None);
    }

    #[test]
    fn 厂商识别_企业号与描述双证据() {
        // 双证据一致 → High
        let id = identify(
            Some(&oid("1.3.6.1.4.1.9.1.1208")),
            Some("Cisco IOS Software"),
        );
        assert_eq!(id.vendor_guess, Some("cisco"));
        assert_eq!(id.confidence, Confidence::High);
        // 仅前缀 → Low；深前缀细分到 OS 级（LibreNMS 语义：云山=CE 交换机），
        // vendor_kind 归属华为采集库
        let id = identify(Some(&oid("1.3.6.1.4.1.2011.2.23.1")), None);
        assert_eq!(id.vendor_guess, Some("yunshan"));
        assert_eq!(
            crate::vendor::vendor_kind(id.vendor_guess),
            crate::live::VendorKind::Huawei
        );
        assert_eq!(id.confidence, Confidence::Low);
        // 仅品牌级 7 弧前缀（非 OS 深前缀覆盖）→ 品牌级规则
        let id = identify(Some(&oid("1.3.6.1.4.1.2011.5.1")), None);
        assert_eq!(id.vendor_guess, Some("huawei"));
        assert_eq!(id.confidence, Confidence::Low);
        // 仅描述词 → Low
        let id = identify(None, Some("Huawei VRP"));
        assert_eq!(id.vendor_guess, Some("huawei"));
        assert_eq!(id.confidence, Confidence::Low);
        // 识别-only 品牌双证据亦可 High（仅驱动绑定建议，采集仍走 Generic）
        let id = identify(Some(&oid("1.3.6.1.4.1.2636.1.1")), Some("Junos"));
        assert_eq!(id.vendor_guess, Some("juniper"));
        assert_eq!(id.confidence, Confidence::High);
    }

    #[test]
    fn 厂商识别_企业号11冲突矩阵() {
        let hp = oid("1.3.6.1.4.1.11.2.3.7");
        // 11 + comware：描述胜出 H3C，Low（沿用原 detect_vendor 语义）
        let id = identify(Some(&hp), Some("HP Comware Platform Software"));
        assert_eq!(id.vendor_guess, Some("h3c"));
        assert_eq!(id.confidence, Confidence::Low);
        // 11 + aruba：同企业号下的 ProVision 系，Low（§6.1）
        let id = identify(Some(&hp), Some("ArubaOS Switch Software"));
        assert_eq!(id.vendor_guess, Some("aruba"));
        assert_eq!(id.confidence, Confidence::Low);
        // 11 + HP：双证据一致 → High（直连 HP 设备如打印机）
        let id = identify(Some(&hp), Some("HP LaserJet"));
        assert_eq!(id.vendor_guess, Some("hp"));
        assert_eq!(id.confidence, Confidence::High);
    }

    #[test]
    fn 厂商识别_usg上报net_snmp企业号() {
        // 华为 USG 防火墙上报 net-snmp sysObjectID（LibreNMS linux.yaml
        // sysDescr_except '/^USG/' 同款案例）：厂商按 sysDescr 判定，Low
        let id = identify(
            Some(&oid("1.3.6.1.4.1.8072.3.2.10")),
            Some("Huawei USG6000V Software"),
        );
        assert_eq!(id.vendor_guess, Some("huawei"));
        assert_eq!(id.confidence, Confidence::Low);
        assert_eq!(id.platform, None, "8072 不能定位平台");
    }

    #[test]
    fn 平台识别_linux() {
        let id = identify(
            Some(&oid("1.3.6.1.4.1.8072.3.2.10")),
            Some("Linux web01 5.15.0-91-generic x86_64"),
        );
        assert_eq!(id.platform, Some("linux"));
        assert_eq!(id.os_version.as_deref(), Some("5.15.0-91-generic"));
        assert!(id.caps.ucd_mem && id.caps.hr_processor);
        assert_eq!(id.vendor_guess, None, "平台层不判硬件厂商（§3.1）");
    }

    #[test]
    fn 平台识别_windows两种格式() {
        // 新版格式
        let id = identify(
            Some(&oid("1.3.6.1.4.1.311.1.1.3.1")),
            Some("Microsoft Windows Version 10.0 (Build 19045)"),
        );
        assert_eq!(id.platform, Some("windows"));
        assert_eq!(id.os_version.as_deref(), Some("10.0 (Build 19045)"));
        assert!(!id.caps.ucd_mem && !id.caps.hr_processor && id.caps.hr_storage);
        assert_eq!(id.vendor_guess, None);
        // 旧版格式（示例格式，需实测校准）
        let id = identify(
            Some(&oid("1.3.6.1.4.1.311.1.1.3.1")),
            Some("Hardware: Intel64 Family 6 Software: Windows Version 6.3 (Build 9600)"),
        );
        assert_eq!(id.platform, Some("windows"));
        assert_eq!(id.os_version.as_deref(), Some("6.3 (Build 9600)"));
    }

    #[test]
    fn 平台识别_freebsd_net_snmp组合() {
        // §3.3 关键案例：8072 + "FreeBSD"，sysDescr 优先，不得判 linux
        let id = identify(
            Some(&oid("1.3.6.1.4.1.8072.3.2.10")),
            Some("FreeBSD gw.example.com 13.2-RELEASE"),
        );
        assert_eq!(id.platform, Some("freebsd"));
        assert_eq!(id.os_version.as_deref(), Some("13.2-RELEASE"));
        // net-snmp agent → UCD/LM-SENSORS 能力升级
        assert!(id.caps.ucd_mem && id.caps.ucd_cpu && id.caps.lm_sensors && id.caps.hr_storage);
        // bsnmpd（12325）：初始保守仅 hrStorage
        let id = identify(
            Some(&oid("1.3.6.1.4.1.12325.1.1")),
            Some("FreeBSD gw 13.2-RELEASE"),
        );
        assert_eq!(id.platform, Some("freebsd"));
        assert!(!id.caps.ucd_mem && id.caps.hr_storage);
    }

    #[test]
    fn 平台识别_bsd与识别only平台() {
        let id = identify(
            Some(&oid("1.3.6.1.4.1.30155.1.1")),
            Some("OpenBSD gw.example.com 7.4 GENERIC"),
        );
        assert_eq!(id.platform, Some("openbsd"));
        assert_eq!(id.os_version.as_deref(), Some("7.4"));
        // 识别-only：平台名仅展示，能力按现状 best-effort 全链（§3.4）
        let id = identify(None, Some("VMware ESXi 7.0.3 build-20328354"));
        assert_eq!(id.platform, Some("esxi"));
        assert_eq!(id.caps, platform::PlatformCaps::ALL);
        let id = identify(None, Some("SunOS srv 5.10 Generic"));
        assert_eq!(id.platform, Some("solaris"));
        let id = identify(None, Some("IBM AIX Version 7.2.0.0"));
        assert_eq!(id.platform, Some("aix"));
        let id = identify(None, Some("Darwin macbook 22.6.0 Darwin Kernel"));
        assert_eq!(id.platform, Some("macos"));
    }

    #[test]
    fn 识别_未知设备走通用链() {
        let id = identify(None, Some("Some Generic Device"));
        assert_eq!(id.platform, None);
        assert_eq!(id.vendor_guess, None);
        assert_eq!(id.confidence, Confidence::Low);
        assert_eq!(id.caps, platform::PlatformCaps::ALL, "现状 Generic 链等价");
    }

    #[test]
    fn 社区规则_os级前缀识别与采集库归属() {
        // Fortinet（社区规则，企业号 12356）
        let id = identify(Some(&oid("1.3.6.1.4.1.12356.101.1.1000")), None);
        assert_eq!(id.vendor_guess, Some("fortigate"));
        // 锐捷（国内品牌，企业号 4881）
        let id = identify(Some(&oid("1.3.6.1.4.1.4881.1.50")), None);
        assert_eq!(id.vendor_guess, Some("ruijie"));
        // Cisco 特定型号深前缀 → OS 级细分，采集库归 Cisco；
        // 常规型号（9.1.716 等）不在社区列表 → 内置品牌级规则兜底
        let id = identify(Some(&oid("1.3.6.1.4.1.9.1.2330.1")), None);
        assert_eq!(id.vendor_guess, Some("ios"));
        assert_eq!(
            crate::vendor::vendor_kind(id.vendor_guess),
            crate::live::VendorKind::Cisco
        );
        let id = identify(Some(&oid("1.3.6.1.4.1.9.1.716")), None);
        assert_eq!(id.vendor_guess, Some("cisco"));
    }

    #[test]
    fn 证据链_记录命中与冲突() {
        let id = identify(
            Some(&oid("1.3.6.1.4.1.11.2.3.7")),
            Some("HP Comware Platform Software"),
        );
        assert!(
            id.evidence
                .iter()
                .any(|e| e.contains("冲突") && e.contains("描述词胜出"))
        );
        assert!(
            id.evidence
                .iter()
                .any(|e| e.contains("企业号") && e.contains("HP/HPE"))
        );
        assert!(
            id.evidence
                .iter()
                .any(|e| e.contains("sysDescr") && e.contains("H3C"))
        );
    }
}
