//! 平台识别规则（Linux/Windows/BSD/Unix）与采集能力声明。
//!
//! 关键前提（方案 §3）：
//! - OID 能力边界在 agent 软件而非 OS 内核，平台分支按 `(OS, Agent)` 细分；
//! - sysDescr OS 词是强证据，agent 企业号是弱证据——net-snmp 可装在任何
//!   OS（8072 ≠ Linux），8072 只作佐证不作平台判定（§3.3 FreeBSD+net-snmp
//!   关键案例）；
//! - 平台不冒充硬件厂商：平台 agent 企业号不作为厂商证据（§3.1）。

/// 平台识别规则。
pub struct PlatformRule {
    pub id: &'static str,
    pub display_name: &'static str,
    /// OS 专属 agent 软件的企业号（弱证据，仅 sysDescr 缺失时兜底定位平台）。
    /// 来源：IANA enterprise-numbers。注意 linux 不填：net-snmp(8072) 跨 OS，
    /// 不能作为 linux 的平台证据
    pub agent_enterprise_ids: &'static [u32],
    /// sysDescr OS 关键词（强证据，小写）：单词词精确匹配，多词按全词命中
    pub descr_keywords: &'static [&'static str],
    /// 采集能力声明：probe 按此跳过已知缺失的 MIB 族（§3.2）
    pub caps: PlatformCaps,
    /// sysDescr → 版本展示串（示例格式需实测校准，§3.2）
    pub version_parser: fn(&str) -> Option<String>,
}

/// 平台采集能力声明（保守原则：未实测的能力不声明）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformCaps {
    /// UCD-SNMP-MIB 内存标量
    pub ucd_mem: bool,
    /// UCD ssCpuRaw / laTable（probe 未用，预留给后续指标采集）
    pub ucd_cpu: bool,
    /// HOST-RESOURCES hrStorageTable
    pub hr_storage: bool,
    /// HOST-RESOURCES hrProcessorLoad
    pub hr_processor: bool,
    /// LM-SENSORS-MIB 温度
    pub lm_sensors: bool,
    /// ENTITY-SENSOR-MIB 温度
    pub entity_sensors: bool,
}

impl PlatformCaps {
    /// 全能力：Unknown 平台与识别-only 平台走现有 Generic best-effort 链
    ///（现状行为等价，不因识别不出而砍采集，§7.4）
    pub const ALL: PlatformCaps = PlatformCaps {
        ucd_mem: true,
        ucd_cpu: true,
        hr_storage: true,
        hr_processor: true,
        lm_sensors: true,
        entity_sensors: true,
    };
}

/// net-snmp 的 IANA 企业号：跨 OS 的 agent 软件，只能证明「装了 net-snmp」，
/// 不能定位平台，但可证明 UCD/LM-SENSORS 可用（随 agent 提供）。
pub const NET_SNMP_ENTERPRISE: u32 = 8072;

/// 平台注册表（顺序即 sysDescr 识别优先级）。
pub fn registry() -> &'static [PlatformRule] {
    REGISTRY
}

static REGISTRY: &[PlatformRule] = &[
    // Linux：net-snmp 口径全能力（现状 Generic 链等价）
    PlatformRule {
        id: "linux",
        display_name: "Linux",
        agent_enterprise_ids: &[],
        descr_keywords: &["linux"],
        caps: PlatformCaps::ALL,
        version_parser: linux_version,
    },
    // Windows：Microsoft SNMP 服务仅实现 hrStorageTable；
    // CPU 利用率标准 MIB 无解（hrProcessorTable 未实现、无 ssCpuRaw，
    // 需第三方 agent，FOIMS 不虚构，§3.2）
    PlatformRule {
        id: "windows",
        display_name: "Windows",
        agent_enterprise_ids: &[311],
        descr_keywords: &["microsoft windows", "windows"],
        caps: PlatformCaps {
            ucd_mem: false,
            ucd_cpu: false,
            hr_storage: true,
            hr_processor: false,
            lm_sensors: false,
            entity_sensors: false,
        },
        version_parser: windows_version,
    },
    // FreeBSD：bsnmpd 初始保守仅 hrStorage；装 net-snmp 后经
    // effective_caps 升级 UCD/LM-SENSORS（§3.3）
    PlatformRule {
        id: "freebsd",
        display_name: "FreeBSD",
        agent_enterprise_ids: &[12325],
        descr_keywords: &["freebsd"],
        caps: PlatformCaps {
            ucd_mem: false,
            ucd_cpu: false,
            hr_storage: true,
            hr_processor: false,
            lm_sensors: false,
            entity_sensors: false,
        },
        version_parser: freebsd_version,
    },
    // 以下识别-only：平台名仅展示，能力未实测，按 ALL 走现状 best-effort 链
    PlatformRule {
        id: "openbsd",
        display_name: "OpenBSD",
        agent_enterprise_ids: &[30155],
        descr_keywords: &["openbsd"],
        caps: PlatformCaps::ALL,
        version_parser: openbsd_version,
    },
    PlatformRule {
        id: "solaris",
        display_name: "Solaris",
        agent_enterprise_ids: &[42],
        descr_keywords: &["solaris", "sunos"],
        caps: PlatformCaps::ALL,
        version_parser: solaris_version,
    },
    PlatformRule {
        id: "aix",
        display_name: "AIX",
        agent_enterprise_ids: &[2],
        descr_keywords: &["aix"],
        caps: PlatformCaps::ALL,
        version_parser: aix_version,
    },
    PlatformRule {
        id: "esxi",
        display_name: "ESXi",
        agent_enterprise_ids: &[6876],
        descr_keywords: &["vmware esxi", "esxi", "vmware esx"],
        caps: PlatformCaps::ALL,
        version_parser: esxi_version,
    },
    PlatformRule {
        id: "macos",
        display_name: "macOS",
        agent_enterprise_ids: &[],
        descr_keywords: &["darwin"],
        caps: PlatformCaps::ALL,
        version_parser: macos_version,
    },
];

/// 平台识别结果（规则引用 + 是否强证据命中）。
#[derive(Clone, Copy)]
pub(crate) struct PlatformMatch<'a> {
    pub rule: &'a PlatformRule,
    pub strong: bool,
}

/// 平台识别：sysDescr OS 词优先（强证据）；sysDescr 缺失时以 OS 专属
/// agent 企业号弱证据兜底（net-snmp 8072 跨 OS，不作平台证据）。
pub(crate) fn detect(
    descr: Option<&str>,
    agent_enterprise: Option<u32>,
) -> Option<PlatformMatch<'static>> {
    if let Some(descr) = descr {
        for rule in registry() {
            if rule
                .descr_keywords
                .iter()
                .any(|k| descr_has_words(descr, k))
            {
                return Some(PlatformMatch { rule, strong: true });
            }
        }
    }
    let enterprise = agent_enterprise?;
    registry()
        .iter()
        .find(|r| r.agent_enterprise_ids.contains(&enterprise))
        .map(|rule| PlatformMatch {
            rule,
            strong: false,
        })
}

/// 平台有效能力：规则声明 + net-snmp 修正（UCD/LM-SENSORS 随 agent 提供，
/// BSD/Solaris 装了 net-snmp 即具备）。Unknown 平台 → 全能力（现状等价）。
pub fn effective_caps(rule: Option<&PlatformRule>, agent_enterprise: Option<u32>) -> PlatformCaps {
    let Some(rule) = rule else {
        return PlatformCaps::ALL;
    };
    let mut caps = rule.caps;
    if agent_enterprise == Some(NET_SNMP_ENTERPRISE) {
        caps.ucd_mem = true;
        caps.ucd_cpu = true;
        caps.lm_sensors = true;
    }
    caps
}

/// 该企业号是否属于平台 agent（不作为硬件厂商证据，§3.1）。
pub fn is_platform_agent(enterprise: u32) -> bool {
    enterprise == NET_SNMP_ENTERPRISE
        || registry()
            .iter()
            .any(|r| r.agent_enterprise_ids.contains(&enterprise))
}

// 版本解析（无正则依赖，示例格式需实测校准）

/// Linux sysDescr：`Linux <host> <内核版本> …`
fn linux_version(descr: &str) -> Option<String> {
    token_after(descr, "linux")
}

/// Windows sysDescr（§3.2，两种格式都要）：
/// 新版 `Microsoft Windows Version 10.0 (Build 19045) …`、
/// 旧版 `Hardware: … Software: Windows Version 6.3 (Build 9600)`
fn windows_version(descr: &str) -> Option<String> {
    let tokens: Vec<&str> = descr.split_whitespace().collect();
    let idx = tokens
        .iter()
        .position(|t| t.eq_ignore_ascii_case("version"))?;
    let version = *tokens.get(idx + 1)?;
    if !version.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut out = version.to_string();
    // Build 号词元形如 `(Build` + `19045)`（新旧格式一致）
    if tokens
        .get(idx + 2)
        .is_some_and(|t| t.eq_ignore_ascii_case("(build"))
        && let Some(build) = tokens.get(idx + 3)
        && build.chars().any(|c| c.is_ascii_digit())
    {
        out.push_str(" (Build ");
        out.push_str(build.trim_end_matches(')'));
        out.push(')');
    }
    Some(out)
}

fn freebsd_version(descr: &str) -> Option<String> {
    token_after(descr, "freebsd")
}

fn openbsd_version(descr: &str) -> Option<String> {
    token_after(descr, "openbsd")
}

/// Solaris sysDescr 以 `SunOS <host> <版本>` 居多
fn solaris_version(descr: &str) -> Option<String> {
    token_after(descr, "sunos").or_else(|| token_after(descr, "solaris"))
}

/// AIX sysDescr 常见 `IBM AIX Version 7.2.0.0`，亦有 `AIX <host> <主> <次>` 变体
fn aix_version(descr: &str) -> Option<String> {
    token_after(descr, "aix").or_else(|| token_after(descr, "version"))
}

fn esxi_version(descr: &str) -> Option<String> {
    token_after(descr, "esxi").or_else(|| token_after(descr, "esx"))
}

/// macOS（net-snmp）sysDescr：`Darwin <host> <版本> Darwin Kernel …`
fn macos_version(descr: &str) -> Option<String> {
    token_after(descr, "darwin")
}

/// 从关键词词元之后向后扫描，取首个「数字开头且含小数点」的词元——
/// 各平台版本号的常见形态（5.15.0-91-generic / 13.2-RELEASE / 7.4），
/// 天然跳过占据关键词后一位的主机名（web01 / gw.example.com）。
fn token_after(descr: &str, keyword: &str) -> Option<String> {
    let tokens: Vec<&str> = descr.split_whitespace().collect();
    let idx = tokens
        .iter()
        .position(|t| t.eq_ignore_ascii_case(keyword))?;
    tokens[idx + 1..]
        .iter()
        .find(|t| t.starts_with(|c: char| c.is_ascii_digit()) && t.contains('.'))
        .map(|t| t.to_string())
}

/// sysDescr 多词关键词命中：所有词均按空白词元出现（大小写不敏感）。
fn descr_has_words(descr: &str, keyword: &str) -> bool {
    keyword.split_whitespace().all(|k| {
        descr
            .split_whitespace()
            .any(|word| word.eq_ignore_ascii_case(k))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 平台识别_强证据优先() {
        // FreeBSD + net-snmp：8072 ≠ Linux（§3.3 关键案例）
        let m = detect(Some("FreeBSD gw 13.2-RELEASE"), Some(NET_SNMP_ENTERPRISE));
        assert_eq!(m.map(|m| m.rule.id), Some("freebsd"));
        assert!(m.is_some_and(|m| m.strong));
        // 8072 单独出现不能定位平台（net-snmp 跨 OS）
        assert!(detect(None, Some(NET_SNMP_ENTERPRISE)).is_none());
        // sysDescr 缺失时 OS 专属 agent 企业号弱证据兜底
        let m = detect(None, Some(311));
        assert_eq!(m.map(|m| m.rule.id), Some("windows"));
        assert!(m.is_some_and(|m| !m.strong));
    }

    #[test]
    fn 平台能力_net_snmp升级与unknown全能力() {
        let freebsd = registry().iter().find(|r| r.id == "freebsd");
        // bsnmpd（12325）：仅 hrStorage
        let caps = effective_caps(freebsd, Some(12325));
        assert_eq!(
            caps,
            PlatformCaps {
                ucd_mem: false,
                ucd_cpu: false,
                hr_storage: true,
                hr_processor: false,
                lm_sensors: false,
                entity_sensors: false,
            }
        );
        // 装 net-snmp 后升级 UCD/LM-SENSORS
        let caps = effective_caps(freebsd, Some(NET_SNMP_ENTERPRISE));
        assert!(caps.ucd_mem && caps.ucd_cpu && caps.lm_sensors && caps.hr_storage);
        assert!(!caps.entity_sensors);
        // Unknown 平台：现状 Generic 链等价
        assert_eq!(effective_caps(None, None), PlatformCaps::ALL);
    }

    #[test]
    fn 平台agent企业号_不作厂商证据() {
        for enterprise in [8072, 311, 12325, 30155, 42, 2, 6876] {
            assert!(is_platform_agent(enterprise));
        }
        // 厂商企业号不受影响
        for enterprise in [9, 2011, 25506, 11] {
            assert!(!is_platform_agent(enterprise));
        }
    }

    #[test]
    fn 版本解析_各平台() {
        assert_eq!(
            linux_version("Linux web01 5.15.0-91-generic x86_64"),
            Some("5.15.0-91-generic".to_string())
        );
        assert_eq!(
            windows_version("Microsoft Windows Version 10.0 (Build 19045)"),
            Some("10.0 (Build 19045)".to_string())
        );
        assert_eq!(
            windows_version(
                "Hardware: Intel64 Family 6 Software: Windows Version 6.3 (Build 9600)"
            ),
            Some("6.3 (Build 9600)".to_string())
        );
        // 无 Build 号 / 无版本词
        assert_eq!(
            windows_version("Windows Version 10.0"),
            Some("10.0".to_string())
        );
        assert_eq!(windows_version("Windows Server"), None);
        assert_eq!(
            freebsd_version("FreeBSD gw.example.com 13.2-RELEASE FreeBSD"),
            Some("13.2-RELEASE".to_string())
        );
        assert_eq!(
            openbsd_version("OpenBSD gw 7.4 GENERIC"),
            Some("7.4".to_string())
        );
        assert_eq!(
            solaris_version("SunOS srv 5.10 Generic"),
            Some("5.10".to_string())
        );
        assert_eq!(
            aix_version("IBM AIX Version 7.2.0.0"),
            Some("7.2.0.0".to_string())
        );
        assert_eq!(
            esxi_version("VMware ESXi 7.0.3 build-20328354"),
            Some("7.0.3".to_string())
        );
        assert_eq!(
            macos_version("Darwin macbook 22.6.0 Darwin Kernel Version 22.6.0"),
            Some("22.6.0".to_string())
        );
    }
}
