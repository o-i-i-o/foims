//! 识别纯函数（docs/SNMP模块重构方案.md §5.4）：双通道聚合
//!（sysObjectID 前缀 + sysDescr 描述）+ 主动探测裁决 + 平台域吸收
//!（原 platform.rs 词表/能力推导/版本解析并入本模块）。
//!
//! 无 IO：探测 GET 由采集层执行（值经 `probe_values` 闭包注入），
//! 本模块只做给定结果后的裁决。识别失败返回 generic 兜底 profile，
//! `Identity.profile` 直接进采集，不再有裸字符串 id 查表。

use std::sync::LazyLock;

use async_snmp::Oid;
use regex::Regex;

use super::model::{CapsKey, OsProfile, Probe, ProbeOp};
use super::registry::registry;

// ───────────────────────── 平台域（吸收代码） ─────────────────────────

/// net-snmp 的 IANA 企业号：跨 OS 的 agent 软件，只能证明「装了 net-snmp」，
/// 不能定位平台，但可证明 UCD/LM-SENSORS 可用（随 agent 提供）。
pub const NET_SNMP_ENTERPRISE: u32 = 8072;

/// 平台采集能力声明（§4.3 吸收：probe/采集按此跳过已知缺失的 MIB 族）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformCaps {
    /// UCD-SNMP-MIB 内存标量
    pub ucd_mem: bool,
    /// UCD ssCpuRaw / laTable
    pub ucd_cpu: bool,
    /// HOST-RESOURCES hrStorageTable
    pub hr_storage: bool,
    /// HOST-RESOURCES hrProcessorLoad
    pub hr_processor: bool,
    /// LM-SENSORS-MIB
    pub lm_sensors: bool,
    /// ENTITY-SENSOR-MIB
    pub entity_sensors: bool,
}

impl PlatformCaps {
    /// 全能力：Unknown 平台与识别-only 平台走全链 best-effort（现状等价）。
    pub const ALL: Self = Self {
        ucd_mem: true,
        ucd_cpu: true,
        hr_storage: true,
        hr_processor: true,
        lm_sensors: true,
        entity_sensors: true,
    };

    /// 能力门控判定（候选 require_caps 消费）。
    pub fn has(self, key: CapsKey) -> bool {
        match key {
            CapsKey::UcdMem => self.ucd_mem,
            CapsKey::UcdCpu => self.ucd_cpu,
            CapsKey::HrStorage => self.hr_storage,
            CapsKey::HrProcessor => self.hr_processor,
            CapsKey::LmSensors => self.lm_sensors,
            CapsKey::EntitySensors => self.entity_sensors,
        }
    }
}

/// 平台识别规则（词表吸收，顺序即 sysDescr 识别优先级）。
struct PlatformRule {
    id: &'static str,
    /// OS 专属 agent 软件企业号（弱证据，仅 sysDescr 缺失时兜底；linux 不填：
    /// net-snmp 跨 OS，不能作为 linux 证据）。
    agent_enterprise_ids: &'static [u32],
    /// sysDescr OS 关键词（强证据，多词按全词命中）。
    descr_keywords: &'static [&'static str],
    caps: PlatformCaps,
    version_parser: fn(&str) -> Option<String>,
}

static PLATFORM_RULES: &[PlatformRule] = &[
    // Linux：net-snmp 口径全能力
    PlatformRule {
        id: "linux",
        agent_enterprise_ids: &[],
        descr_keywords: &["linux"],
        caps: PlatformCaps::ALL,
        version_parser: linux_version,
    },
    // Windows：Microsoft SNMP 服务仅实现 hrStorageTable
    PlatformRule {
        id: "windows",
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
    // FreeBSD：bsnmpd 保守仅 hrStorage；装 net-snmp 后经 effective_caps 升级
    PlatformRule {
        id: "freebsd",
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
    // 以下识别-only：平台名仅展示，能力未实测，按 ALL 走 best-effort 链
    PlatformRule {
        id: "openbsd",
        agent_enterprise_ids: &[30155],
        descr_keywords: &["openbsd"],
        caps: PlatformCaps::ALL,
        version_parser: openbsd_version,
    },
    PlatformRule {
        id: "solaris",
        agent_enterprise_ids: &[42],
        descr_keywords: &["solaris", "sunos"],
        caps: PlatformCaps::ALL,
        version_parser: solaris_version,
    },
    PlatformRule {
        id: "aix",
        agent_enterprise_ids: &[2],
        descr_keywords: &["aix"],
        caps: PlatformCaps::ALL,
        version_parser: aix_version,
    },
    PlatformRule {
        id: "esxi",
        agent_enterprise_ids: &[6876],
        descr_keywords: &["vmware esxi", "esxi", "vmware esx"],
        caps: PlatformCaps::ALL,
        version_parser: esxi_version,
    },
    PlatformRule {
        id: "macos",
        agent_enterprise_ids: &[],
        descr_keywords: &["darwin"],
        caps: PlatformCaps::ALL,
        version_parser: macos_version,
    },
];

/// 平台识别：sysDescr OS 词优先（强证据）；sysDescr 缺失时以 OS 专属
/// agent 企业号弱证据兜底（net-snmp 8072 跨 OS，不作平台证据）。
fn detect_platform(
    descr: Option<&str>,
    agent_enterprise: Option<u32>,
) -> Option<(&'static PlatformRule, bool)> {
    if let Some(descr) = descr {
        for rule in PLATFORM_RULES {
            if rule
                .descr_keywords
                .iter()
                .any(|k| descr_has_words(descr, k))
            {
                return Some((rule, true));
            }
        }
    }
    let enterprise = agent_enterprise?;
    PLATFORM_RULES
        .iter()
        .find(|r| r.agent_enterprise_ids.contains(&enterprise))
        .map(|rule| (rule, false))
}

/// 平台有效能力：规则声明 + net-snmp 修正（UCD/LM-SENSORS 随 agent 提供）。
/// Unknown 平台 → 全能力（现状等价，不因识别不出而砍采集）。
fn effective_caps(rule: Option<&PlatformRule>, agent_enterprise: Option<u32>) -> PlatformCaps {
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

/// 该企业号是否属于平台 agent（不作为硬件厂商证据）。
fn is_platform_agent(enterprise: u32) -> bool {
    enterprise == NET_SNMP_ENTERPRISE
        || PLATFORM_RULES
            .iter()
            .any(|r| r.agent_enterprise_ids.contains(&enterprise))
}

// 版本解析（无正则依赖，示例格式需实测校准）

/// Linux sysDescr：`Linux <host> <内核版本> …`
fn linux_version(descr: &str) -> Option<String> {
    token_after(descr, "linux")
}

/// Windows sysDescr 两种格式：新版 `Microsoft Windows Version 10.0
/// (Build 19045)`、旧版 `Hardware: … Software: Windows Version 6.3 (Build 9600)`
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

/// AIX sysDescr 常见 `IBM AIX Version 7.2.0.0`
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

/// 关键词词元之后首个「数字开头且含小数点」的词元——天然跳过主机名
///（web01 / gw.example.com）。
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

// ───────────────────────── 识别结果 ─────────────────────────

/// 识别结果：`profile` 直接进采集（识别失败 = generic 兜底），
/// `brand` / `platform` / `os_version` 用于展示与日志，
/// `caps` 供采集链按平台能力裁剪。
#[derive(Debug)]
pub struct Identity {
    pub profile: &'static OsProfile,
    /// 识别结果的解析后采集计划（注册表冻结数据，直接进解释器）。
    pub plan: &'static super::registry::ResolvedCollectPlan,
    pub brand: &'static str,
    /// 平台规则 id（linux/windows/…，Unknown 为 None）
    pub platform: Option<&'static str>,
    /// 内核/系统版本（展示用，示例格式需实测校准）
    pub os_version: Option<String>,
    pub caps: PlatformCaps,
}

// 描述通道正则编译缓存（registry 顺序对齐；注册表冻结后一次性编译）
static DESCR_REGEXES: LazyLock<Vec<Vec<Regex>>> = LazyLock::new(|| {
    registry()
        .iter()
        .map(|p| {
            p.matcher
                .as_ref()
                .map(|m| m.descr_regex.clone())
                .unwrap_or_default()
        })
        .collect()
});

/// 识别入口：输入 sysObjectID 与 sysDescr（均可缺失）。
///
/// `probe_values` 为探测取值闭包（oid → 响应值文本）：静态证据（前缀/
/// 关键词/正则）全部未命中且注册表存在探测规则时由调用方预取 GET 值后
/// 注入；传 `None` 跳过探测通道（仅静态识别的调用方，如台账采集）。
pub fn identify(
    object_id: Option<&Oid>,
    descr: Option<&str>,
    probe_values: Option<&dyn Fn(&str) -> Option<String>>,
) -> Identity {
    let descr = descr.map(str::trim).filter(|s| !s.is_empty());
    let agent = object_id.and_then(enterprise_of);

    // —— 平台（吸收代码）：sysDescr OS 词强证据；agent 企业号弱证据 ——
    let platform_match = detect_platform(descr, agent);
    if let Some((rule, strong)) = &platform_match {
        if !*strong {
            tracing::debug!("SNMP 平台识别（弱证据）: agent 企业号命中 {}", rule.id);
        }
    } else if agent == Some(NET_SNMP_ENTERPRISE) {
        tracing::debug!("SNMP 平台识别: agent 为 net-snmp，跨 OS，不能定位平台");
    }
    let os_version =
        platform_match.and_then(|(rule, _)| descr.and_then(|d| (rule.version_parser)(d)));

    // —— 前缀通道：最长前缀胜出；同长并列按关键词计数 → id 字典序 ——
    let prefix_match = object_id.and_then(|oid| prefix_profile(oid, descr));
    if let Some((profile, prefix)) = &prefix_match {
        let dotted = prefix
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".");
        tracing::debug!("SNMP 识别前缀通道: {dotted} 命中 {}", profile.id);
    }

    // —— 描述通道：注册表顺序首个命中（词元关键词或正则）——
    let descr_match = descr.and_then(descr_profile);

    // —— 聚合：双通道一致 → 该 profile；
    //   同品牌（前缀命中父族/品牌级、描述词命中其 OS 级细分或反之）
    //   视为包含关系而非冲突，前缀更具体者胜出（华为深前缀 + Huawei 描述词
    //   → 云山细分，不被品牌级描述词截胡）；
    //   跨品牌才是真冲突 → 描述词胜出（OEM 设备 sysObjectID 不可信，
    //   沿用 11 + comware → H3C 语义）——
    let matched = match (prefix_match, descr_match) {
        (Some((a, _)), Some(b)) if a.id == b.id => Some(a),
        (Some((a, _)), Some(b)) if a.brand == b.brand => Some(a),
        (Some((a, _)), Some(b)) => {
            tracing::info!(
                "SNMP 识别冲突: 前缀指向 {}（{}），描述词指向 {}（{}），描述词胜出",
                a.id,
                a.brand,
                b.id,
                b.brand
            );
            Some(b)
        }
        (Some((a, _)), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    // —— 探测通道：静态证据全未命中时按声明顺序逐规则裁决（多条 AND）——
    let matched = matched.or_else(|| probe_profile(probe_values));

    let profile = matched.unwrap_or_else(|| {
        tracing::debug!("SNMP 识别未命中，回落 generic 兜底 profile");
        registry().generic()
    });
    // 采集计划与 profile 一一注册（validate_and_resolve 为每条 profile 建链，
    // 缺失属不变量破坏）
    let plan = registry().plan(&profile.id).unwrap_or_else(|| {
        panic!(
            "SNMP profile {} 采集计划缺失（注册表不变量破坏）",
            profile.id
        )
    });
    let brand = brand_of(profile);
    let caps = effective_caps(platform_match.map(|(rule, _)| rule), agent);
    Identity {
        profile,
        plan,
        brand,
        platform: platform_match.map(|(rule, _)| rule.id),
        os_version,
        caps,
    }
}

/// brand 字符串静态化：Registry 冻结于 static（LazyLock），经其取得的
/// 内部借用即 'static（§5.3 所有权模型），无需任何不安全代码。
fn brand_of(profile: &'static OsProfile) -> &'static str {
    profile.brand.as_str()
}

/// sysObjectID → IANA 企业号（第 7 弧，须位于 1.3.6.1.4.1 之下）。
fn enterprise_of(object_id: &Oid) -> Option<u32> {
    let arcs = object_id.arcs();
    (arcs.len() >= 7 && arcs.starts_with(&[1, 3, 6, 1, 4, 1])).then(|| arcs[6])
}

/// 前缀通道：全部 kind=os profile 中找 sysobjectid 前缀命中者。
/// 最长前缀胜出；同长并列 → descr_keywords 命中词数多者 → 注册表顺序
///（先声明者优先，见 tie_break）。
fn prefix_profile(
    object_id: &Oid,
    descr: Option<&str>,
) -> Option<(&'static OsProfile, &'static [u32])> {
    let arcs = object_id.arcs();
    if arcs.len() < 7 || !arcs.starts_with(&[1, 3, 6, 1, 4, 1]) {
        return None;
    }
    if is_platform_agent(arcs[6]) {
        return None;
    }
    let tokens = descr.map(vendor_tokens);
    let mut best: Option<(&'static OsProfile, &'static [u32], usize)> = None;
    for profile in registry().iter() {
        let Some(matcher) = &profile.matcher else {
            continue;
        };
        let Some(prefix) = matcher.sysobjectid.iter().find(|p| arcs.starts_with(p)) else {
            continue;
        };
        let keyword_hits = tokens
            .as_ref()
            .map(|tk| {
                matcher
                    .descr_keywords
                    .iter()
                    .filter(|k| keyword_hit(tk, k))
                    .count()
            })
            .unwrap_or(0);
        let candidate = (profile, prefix.as_slice(), keyword_hits);
        best = Some(match best {
            None => candidate,
            Some(current) => tie_break(current, candidate),
        });
    }
    best.map(|(profile, prefix, _)| (profile, prefix))
}

/// 同长并列裁决：关键词命中数多者 → 注册表顺序先声明者优先。
/// 注册表顺序与旧 identify() 遍历序一致（内置品牌文件在前、社区文件按名序），
/// 精确吸收旧「同长时先遍历者保留」语义（一次性导入数据存在 29 组重复前缀，
/// 旧代码中同长死规则不生效，此处保持同一裁决）。
fn tie_break(
    current: (&'static OsProfile, &'static [u32], usize),
    candidate: (&'static OsProfile, &'static [u32], usize),
) -> (&'static OsProfile, &'static [u32], usize) {
    match current.1.len().cmp(&candidate.1.len()) {
        std::cmp::Ordering::Greater => current,
        std::cmp::Ordering::Less => candidate,
        std::cmp::Ordering::Equal => match current.2.cmp(&candidate.2) {
            std::cmp::Ordering::Greater => current,
            std::cmp::Ordering::Less => candidate,
            // 并列保持先声明者（注册表顺序确定性，替代旧内置优先语义）
            std::cmp::Ordering::Equal => current,
        },
    }
}

/// 描述通道：注册表顺序首个命中者（词元关键词全词命中或正则命中）。
fn descr_profile(descr: &str) -> Option<&'static OsProfile> {
    let lowered = descr.to_lowercase();
    let tokens: Vec<String> = vendor_tokens(&lowered);
    for (idx, profile) in registry().iter().enumerate() {
        let Some(matcher) = &profile.matcher else {
            continue;
        };
        let keyword_hit = matcher
            .descr_keywords
            .iter()
            .any(|k| keyword_hit(&tokens, k));
        if keyword_hit {
            return Some(profile);
        }
        let regex_hit = DESCR_REGEXES
            .get(idx)
            .is_some_and(|regexes| regexes.iter().any(|re| re.is_match(descr)));
        if regex_hit {
            return Some(profile);
        }
    }
    None
}

/// 探测通道：按注册表顺序逐条评估带探测规则的 profile（多条 AND），
/// 首个全命中者生效。
fn probe_profile(
    probe_values: Option<&dyn Fn(&str) -> Option<String>>,
) -> Option<&'static OsProfile> {
    let get_value = probe_values?;
    for profile in registry().iter() {
        let Some(matcher) = &profile.matcher else {
            continue;
        };
        if matcher.probes.is_empty() {
            continue;
        }
        let hit = matcher
            .probes
            .iter()
            .all(|probe| probe_hit(probe, get_value));
        if hit {
            tracing::debug!(
                "SNMP 识别探测通道命中 {}（{} 项检查）",
                profile.id,
                matcher.probes.len()
            );
            return Some(profile);
        }
    }
    None
}

/// 单条探测比对：Exists 只看有值，其余按值文本比对（响应值文本化由
/// 采集层闭包完成）。
fn probe_hit(probe: &Probe, get_value: &dyn Fn(&str) -> Option<String>) -> bool {
    let value = match get_value(&probe.oid) {
        Some(v) => v,
        None => return false,
    };
    match probe.op {
        ProbeOp::Exists => true,
        ProbeOp::Eq => value == probe.value,
        ProbeOp::Ne => value != probe.value,
        ProbeOp::Has => value.contains(&probe.value),
        ProbeOp::Starts => value.starts_with(&probe.value),
        ProbeOp::Regex => Regex::new(&probe.value).is_ok_and(|re| re.is_match(&value)),
    }
}

/// sysDescr 词边界分词（小写，非字母数字切分）：子串匹配会被
/// "ship"/"Sharp" 之类无关词误命中。
fn vendor_tokens(descr: &str) -> Vec<String> {
    descr
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// 厂商关键词命中：关键词按同规则分词后全词元出现——单词词即词边界
/// 精确匹配；连字符/多词品牌（tp-link、versatile routing platform）
/// 按全词元命中。
fn keyword_hit(tokens: &[String], keyword: &str) -> bool {
    keyword
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|k| !k.is_empty())
        .all(|k| tokens.iter().any(|t| t == k))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(s: &str) -> Oid {
        Oid::parse(s).unwrap()
    }

    fn vendor_of(descr: &str) -> String {
        identify(None, Some(descr), None).profile.id.clone()
    }

    #[test]
    fn 识别_品牌级规则() {
        assert_eq!(vendor_of("Cisco IOS Software"), "cisco-brand");
        assert_eq!(
            vendor_of("Huawei Versatile Routing Platform"),
            "huawei-brand"
        );
        assert_eq!(vendor_of("H3C Comware Software"), "comware");
        assert_eq!(vendor_of("3Com SuperStack Switch"), "h3c-brand");
        assert_eq!(vendor_of("Juniper Networks Junos"), "juniper");
        assert_eq!(vendor_of("HP ProCurve Switch"), "hp");
        assert_eq!(vendor_of("ProCurve J9021A"), "hp");
        assert_eq!(vendor_of("HPE OfficeConnect"), "hp");
        assert_eq!(vendor_of("ArubaOS-CX Switch"), "aruba");
        assert_eq!(vendor_of("NETGEAR ProSafe GS724T"), "netgear");
        assert_eq!(vendor_of("TP-Link JetStream T2600G"), "tp-link");
        assert_eq!(vendor_of("Linksys LGS318"), "linksys");
        assert_eq!(vendor_of("Ubiquiti UniFi Switch"), "ubiquiti");
        assert_eq!(vendor_of("UBNT EdgeSwitch"), "ubiquiti");
        assert_eq!(vendor_of("MikroTik RouterOS"), "mikrotik");
        assert_eq!(vendor_of("ExtremeXOS Switch"), "extreme");
        assert_eq!(vendor_of("Alcatel-Lucent OS6850"), "alcatel");
        assert_eq!(vendor_of("ZyXEL GS1900"), "zyxel");
        assert_eq!(vendor_of("D-Link DGS-1210"), "d-link");
        assert_eq!(vendor_of("Dlink Smart Switch"), "d-link");
    }

    #[test]
    fn 识别_词边界无子串误命中() {
        // 子串误配回归："ship"/"Sharp" 含 "hp" 子串，词边界匹配后不得命中
        assert_eq!(vendor_of("Friendship Leader 3000"), GENERIC_ID_TEST);
        assert_eq!(vendor_of("Sharp LCD Panel"), GENERIC_ID_TEST);
        assert_eq!(vendor_of("hp 2920 Switch"), "hp");
    }

    #[test]
    fn 识别_前缀通道与深前缀() {
        // 华为云山深前缀 → os 级细分
        let id = identify(Some(&oid("1.3.6.1.4.1.2011.2.23.1")), None, None);
        assert_eq!(id.profile.id, "yunshan");
        assert_eq!(id.profile.group.as_deref(), Some("huawei"));
        // 品牌级 7 弧前缀兜底
        let id = identify(Some(&oid("1.3.6.1.4.1.2011.5.1")), None, None);
        assert_eq!(id.profile.id, "huawei-brand");
        // Cisco 深前缀 OS 级细分
        let id = identify(Some(&oid("1.3.6.1.4.1.9.1.2330.1")), None, None);
        assert_eq!(id.profile.id, "ios");
        // 非细分型号 → 品牌级
        let id = identify(Some(&oid("1.3.6.1.4.1.9.1.716")), None, None);
        assert_eq!(id.profile.id, "cisco-brand");
        // 平台 agent 企业号不作厂商证据
        let id = identify(Some(&oid("1.3.6.1.4.1.8072.3.2.10")), None, None);
        assert_eq!(id.profile.id, "generic");
        assert_eq!(id.platform, None);
    }

    #[test]
    fn 识别_双通道冲突描述词胜出() {
        // 企业号 11 + comware → H3C（OEM 语义）
        let id = identify(
            Some(&oid("1.3.6.1.4.1.11.2.3.7")),
            Some("HP Comware Platform Software"),
            None,
        );
        assert_eq!(id.profile.id, "comware");
        // 11 + aruba → aruba
        let id = identify(
            Some(&oid("1.3.6.1.4.1.11.2.3.7")),
            Some("ArubaOS Switch Software"),
            None,
        );
        assert_eq!(id.profile.id, "aruba");
        // 11 + HP → 双证据一致
        let id = identify(
            Some(&oid("1.3.6.1.4.1.11.2.3.7")),
            Some("HP LaserJet"),
            None,
        );
        assert_eq!(id.profile.id, "hp");
    }

    #[test]
    fn 识别_探测通道() {
        // 静态证据全未命中 → 探测裁决（abbups：has "ABB"）
        let values = |oid: &str| {
            (oid == "1.3.6.1.4.1.53973.1.1.1.1.1.1.0").then(|| "ABB UPS agent v1".to_string())
        };
        let id = identify(None, None, Some(&values));
        assert_eq!(id.profile.id, "abbups");
        // 值不命中 → generic
        let miss = |_oid: &str| None::<String>;
        let id = identify(None, None, Some(&miss));
        assert_eq!(id.profile.id, "generic");
        // 无探测闭包 → generic（静态识别调用方口径）
        assert_eq!(identify(None, None, None).profile.id, "generic");
    }

    #[test]
    fn 识别_未知设备走generic兜底() {
        let id = identify(None, Some("Some Generic Device"), None);
        assert_eq!(id.profile.id, "generic");
        assert_eq!(id.brand, "generic");
        assert_eq!(id.caps, PlatformCaps::ALL);
    }

    #[test]
    fn 平台识别_linux与caps() {
        let id = identify(
            Some(&oid("1.3.6.1.4.1.8072.3.2.10")),
            Some("Linux web01 5.15.0-91-generic x86_64"),
            None,
        );
        assert_eq!(id.platform, Some("linux"));
        assert_eq!(id.os_version.as_deref(), Some("5.15.0-91-generic"));
        assert!(id.caps.ucd_mem && id.caps.hr_processor);
    }

    #[test]
    fn 平台识别_windows两种格式() {
        let id = identify(
            Some(&oid("1.3.6.1.4.1.311.1.1.3.1")),
            Some("Microsoft Windows Version 10.0 (Build 19045)"),
            None,
        );
        assert_eq!(id.platform, Some("windows"));
        assert_eq!(id.os_version.as_deref(), Some("10.0 (Build 19045)"));
        assert!(!id.caps.ucd_mem && !id.caps.hr_processor && id.caps.hr_storage);
        let id = identify(
            Some(&oid("1.3.6.1.4.1.311.1.1.3.1")),
            Some("Hardware: Intel64 Family 6 Software: Windows Version 6.3 (Build 9600)"),
            None,
        );
        assert_eq!(id.os_version.as_deref(), Some("6.3 (Build 9600)"));
    }

    #[test]
    fn 平台识别_freebsd_net_snmp组合() {
        // 8072 + "FreeBSD"：sysDescr 优先，不得判 linux
        let id = identify(
            Some(&oid("1.3.6.1.4.1.8072.3.2.10")),
            Some("FreeBSD gw.example.com 13.2-RELEASE"),
            None,
        );
        assert_eq!(id.platform, Some("freebsd"));
        assert!(id.caps.ucd_mem && id.caps.lm_sensors && id.caps.hr_storage);
        // bsnmpd（12325）：保守仅 hrStorage
        let id = identify(
            Some(&oid("1.3.6.1.4.1.12325.1.1")),
            Some("FreeBSD gw 13.2-RELEASE"),
            None,
        );
        assert_eq!(id.platform, Some("freebsd"));
        assert!(!id.caps.ucd_mem && id.caps.hr_storage);
    }

    #[test]
    fn 平台识别_识别only平台() {
        let id = identify(None, Some("VMware ESXi 7.0.3 build-20328354"), None);
        assert_eq!(id.platform, Some("esxi"));
        assert_eq!(id.caps, PlatformCaps::ALL);
        let id = identify(None, Some("SunOS srv 5.10 Generic"), None);
        assert_eq!(id.platform, Some("solaris"));
        let id = identify(None, Some("IBM AIX Version 7.2.0.0"), None);
        assert_eq!(id.platform, Some("aix"));
        let id = identify(None, Some("Darwin macbook 22.6.0 Darwin Kernel"), None);
        assert_eq!(id.platform, Some("macos"));
        let id = identify(
            Some(&oid("1.3.6.1.4.1.30155.1.1")),
            Some("OpenBSD gw.example.com 7.4 GENERIC"),
            None,
        );
        assert_eq!(id.platform, Some("openbsd"));
    }

    #[test]
    fn 识别_社区规则与正则通道() {
        let id = identify(Some(&oid("1.3.6.1.4.1.12356.101.1.1000")), None, None);
        assert_eq!(id.profile.id, "fortigate");
        let id = identify(Some(&oid("1.3.6.1.4.1.4881.1.50")), None, None);
        assert_eq!(id.profile.id, "ruijie");
        // sysDescr 正则通道（LibreNMS buffalo ^BUFFALO）
        let id = identify(None, Some("BUFFALO BS-GS2024"), None);
        assert_eq!(id.profile.id, "buffalo");
        // ^ 锚定语义：非行首不命中
        assert_eq!(vendor_of("Wireless BUFFALO router"), GENERIC_ID_TEST);
    }

    #[test]
    fn 识别_usg上报net_snmp企业号() {
        let id = identify(
            Some(&oid("1.3.6.1.4.1.8072.3.2.10")),
            Some("Huawei USG6000V Software"),
            None,
        );
        assert_eq!(id.profile.id, "huawei-brand");
        assert_eq!(id.platform, None, "8072 不能定位平台");
    }

    #[test]
    fn 识别_展示品牌() {
        let id = identify(None, Some("Cisco IOS Software"), None);
        assert_eq!(id.brand, "Cisco");
        let id = identify(None, Some("Fortinet FortiGate-100F v7.4"), None);
        assert_eq!(id.profile.id, "fortigate");
        assert_eq!(id.brand, "Fortinet Fortigate");
    }
}

/// 测试常量：generic 兜底 id（避免硬编码漂移）。
#[cfg(test)]
const GENERIC_ID_TEST: &str = "generic";
