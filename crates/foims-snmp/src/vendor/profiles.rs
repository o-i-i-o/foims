//! 厂商采集 profile 注册表：CPU/内存/温度三族私有 OID 的数据驱动绑定
//! （替代原 mibs.rs 静态 fn 指针表）。
//!
//! 数据两级：
//! - 内置默认 [`DEFAULT_PROFILES`]（profiles_default.toml，include_str!
//!   编译期嵌入，仅三家 cisco/huawei/h3c，人工核实无结构化上游）；
//! - 运行时配置文件 [`PROFILE_PATHS`]（/etc/foims/vendor_profiles.toml，
//!   开发环境回退当前目录 vendor_profiles.toml）：改文件 + 重启即生效
//!   （零编译），可覆盖内置条目（同 id）或新增厂商（含
//!   sysobjectid_prefixes 前缀兜底匹配）。
//!
//! 探测期优先级（live/profile.rs）：配置文件条目（先 OS id 后 sysObjectID
//! 最长前缀）→ LibreNMS os_discovery 规则 → 内置默认条目（仅 OS id）→
//! 公共 MIB 回退链。配置条目探测为空时不阻断，继续后续层。
//!
//! 加载语义：LazyLock 进程内一次性加载；配置文件缺失/顶层解析失败仅
//! 告警并回退内置默认；条目内单条 OID/单位无效仅丢弃该条，不致命。

use std::fs;
use std::sync::LazyLock;

use async_snmp::Oid;
use serde::Deserialize;

use crate::live::TempUnit;

/// 内置默认 profile（三家私有 OID 库，OID 来源见文件内注释）。
const DEFAULT_PROFILES: &str = include_str!("profiles_default.toml");

/// 配置文件候选路径（按优先级）：系统配置目录（服务/安装包部署）、
/// 当前目录（仅开发环境直接运行；口径与
/// `foims_common::config::CONFIG_PATHS` 一致）。
const PROFILE_PATHS: [&str; 2] = ["/etc/foims/vendor_profiles.toml", "vendor_profiles.toml"];

/// 厂商私有采集 profile（owned 值，探测期只读）。
pub struct VendorProfile {
    /// 识别规则 OS id（vendor/community 规则同源；自定义条目可为任意 id）
    pub id: String,
    /// sysObjectID 前缀（弧序列）：OS id 未命中时按最长前缀兜底匹配
    pub prefixes: Vec<Vec<u32>>,
    /// CPU 使用率列（整数百分比，按序探测，首个有值列生效）
    pub cpu_columns: Vec<Oid>,
    /// 内存方案（None 表示该 profile 不接管内存族）
    pub mem: Option<ProfileMem>,
    /// 温度列（单位随列）
    pub temp_columns: Vec<ProfileTemp>,
}

/// profile 内存方案（与原 VendorMemOids 两形态等价，列改 owned 值）。
pub enum ProfileMem {
    /// used + free，total = used + free；name 定位 pool 行（可选，缺失时
    /// 以 used 列存在行为准）。
    Pool {
        name: Option<Oid>,
        used: Oid,
        free: Oid,
    },
    /// usage 百分比 + size 列缓存总量（优先 MB 口径，字节兜底；
    /// 探测期对大内存设备经 MB 列规避 32 位回绕）。
    EntityPercent {
        usage: Oid,
        size_mega: Option<Oid>,
        size_bytes: Option<Oid>,
    },
}

/// profile 温度列。
pub struct ProfileTemp {
    pub oid: Oid,
    /// celsius：直接摄氏度；milli：毫摄氏度。
    pub unit: TempUnit,
}

/// 进程级注册表：内置默认 + 配置文件条目。
pub struct VendorProfileRegistry {
    /// 内置默认条目（仅按 OS id 匹配，探测期位于 os_discovery 之后）
    builtin: Vec<VendorProfile>,
    /// 配置文件条目（OS id → sysObjectID 前缀两级匹配，探测期最先尝试）
    custom: Vec<VendorProfile>,
}

/// 进程级注册表（LazyLock 一次性加载）。
pub fn registry() -> &'static VendorProfileRegistry {
    static REGISTRY: LazyLock<VendorProfileRegistry> = LazyLock::new(VendorProfileRegistry::load);
    &REGISTRY
}

impl VendorProfileRegistry {
    fn load() -> Self {
        // 内置默认为编译期不变量：解析失败仅记错误并置空（三家回退公共链）
        let builtin = parse_profiles(DEFAULT_PROFILES).unwrap_or_else(|e| {
            tracing::error!(
                "SNMP 内置厂商 profile 解析失败（不变量破坏，请检查 profiles_default.toml）: {e}"
            );
            Vec::new()
        });
        // 配置文件：首个可读候选生效；缺失静默（未部署即全内置默认），
        // 解析失败/无有效条目告警回退内置默认
        let custom = PROFILE_PATHS
            .iter()
            .find_map(|path| fs::read_to_string(path).ok().map(|text| (*path, text)))
            .map_or_else(Vec::new, |(path, text)| match parse_profiles(&text) {
                Ok(profiles) if !profiles.is_empty() => {
                    tracing::info!(
                        "SNMP 厂商 profile 配置加载: {} 条（{path}）",
                        profiles.len()
                    );
                    profiles
                }
                Ok(_) => {
                    tracing::warn!("SNMP 厂商 profile 配置无有效条目（{path}），回退内置默认");
                    Vec::new()
                }
                Err(e) => {
                    tracing::warn!("SNMP 厂商 profile 配置解析失败（{path}）: {e}，回退内置默认");
                    Vec::new()
                }
            });
        Self { builtin, custom }
    }

    /// 配置文件条目匹配（探测期最先尝试）：先按识别 OS id 精确匹配
    ///（含内置三家同 id 覆盖），未命中再按 sysObjectID 最长前缀兜底
    ///（识别-only/未识别设备可经前缀挂接私有采集库）。
    pub fn match_custom(
        &self,
        object_id: Option<&Oid>,
        os_id: Option<&str>,
    ) -> Option<&VendorProfile> {
        if let Some(id) = os_id
            && let Some(profile) = self.custom.iter().find(|p| p.id == id)
        {
            return Some(profile);
        }
        let arcs = object_id?.arcs();
        self.custom
            .iter()
            .filter_map(|p| {
                p.prefixes
                    .iter()
                    .filter(|prefix| arcs.starts_with(prefix.as_slice()))
                    .map(|prefix| prefix.len())
                    .max()
                    .map(|len| (len, p))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, p)| p)
    }

    /// 内置默认条目（仅按 OS id 精确匹配；替代原 vendor::oid_set 静态表）。
    pub fn builtin(&self, os_id: Option<&str>) -> Option<&VendorProfile> {
        os_id.and_then(|id| self.builtin.iter().find(|p| p.id == id))
    }
}

/// 解析一份 profile TOML 文本：顶层语法错误整体报错；条目内单条 OID/
/// 单位无效仅丢弃该条并告警；无任何有效采集项的条目整体跳过。
fn parse_profiles(text: &str) -> Result<Vec<VendorProfile>, String> {
    let file: ProfilesFile = toml::from_str(text).map_err(|e| e.to_string())?;
    Ok(file.profile.into_iter().filter_map(build_profile).collect())
}

// —— TOML schema（serde）——

#[derive(Deserialize)]
struct ProfilesFile {
    #[serde(default)]
    profile: Vec<ProfileEntry>,
}

#[derive(Deserialize)]
struct ProfileEntry {
    id: String,
    #[serde(default)]
    sysobjectid_prefixes: Vec<String>,
    #[serde(default)]
    cpu: Vec<OidEntry>,
    #[serde(default)]
    mem: Option<MemEntry>,
    #[serde(default)]
    temp: Vec<TempEntry>,
}

#[derive(Deserialize)]
struct OidEntry {
    oid: String,
}

#[derive(Deserialize)]
struct MemEntry {
    /// pool | entity
    kind: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    used: Option<String>,
    #[serde(default)]
    free: Option<String>,
    #[serde(default)]
    usage: Option<String>,
    #[serde(default)]
    size_mega: Option<String>,
    #[serde(default)]
    size_bytes: Option<String>,
}

#[derive(Deserialize)]
struct TempEntry {
    oid: String,
    /// celsius（默认）| milli
    #[serde(default)]
    unit: Option<String>,
}

// —— schema → 运行时结构（逐条校验，无效条目告警丢弃）——

fn build_profile(entry: ProfileEntry) -> Option<VendorProfile> {
    let id = entry.id;
    let prefixes: Vec<Vec<u32>> = entry
        .sysobjectid_prefixes
        .into_iter()
        .filter_map(|s| match parse_arcs(&s) {
            Some(arcs) if !arcs.is_empty() => Some(arcs),
            _ => {
                tracing::warn!("SNMP 厂商 profile {id} 的 sysobjectid_prefixes 无效，已跳过: {s}");
                None
            }
        })
        .collect();
    let cpu_columns: Vec<Oid> = entry
        .cpu
        .into_iter()
        .filter_map(|c| parse_oid_field(&id, "cpu.oid", c.oid))
        .collect();
    let mem = entry.mem.and_then(|m| build_mem(&id, m));
    let temp_columns: Vec<ProfileTemp> = entry
        .temp
        .into_iter()
        .filter_map(|t| build_temp(&id, t))
        .collect();
    let profile = VendorProfile {
        id,
        prefixes,
        cpu_columns,
        mem,
        temp_columns,
    };
    // 空条目（识别/匹配得了但没有可采集项）无意义，跳过
    if profile.cpu_columns.is_empty() && profile.mem.is_none() && profile.temp_columns.is_empty() {
        tracing::warn!("SNMP 厂商 profile {} 无任何有效采集项，已跳过", profile.id);
        return None;
    }
    Some(profile)
}

fn build_mem(id: &str, m: MemEntry) -> Option<ProfileMem> {
    match m.kind.as_str() {
        "pool" => {
            let used = m.used.and_then(|s| parse_oid_field(id, "mem.used", s))?;
            let free = m.free.and_then(|s| parse_oid_field(id, "mem.free", s))?;
            let name = m.name.and_then(|s| parse_oid_field(id, "mem.name", s));
            Some(ProfileMem::Pool { name, used, free })
        }
        "entity" => {
            let usage = m.usage.and_then(|s| parse_oid_field(id, "mem.usage", s))?;
            let size_mega = m
                .size_mega
                .and_then(|s| parse_oid_field(id, "mem.size_mega", s));
            let size_bytes = m
                .size_bytes
                .and_then(|s| parse_oid_field(id, "mem.size_bytes", s));
            Some(ProfileMem::EntityPercent {
                usage,
                size_mega,
                size_bytes,
            })
        }
        other => {
            tracing::warn!("SNMP 厂商 profile {id} 的 mem.kind 未知（{other}），内存方案已丢弃");
            None
        }
    }
}

fn build_temp(id: &str, entry: TempEntry) -> Option<ProfileTemp> {
    let oid = parse_oid_field(id, "temp.oid", entry.oid)?;
    let unit = match entry.unit.as_deref().map(str::trim) {
        None | Some("") | Some("celsius") => TempUnit::Celsius,
        Some("milli") => TempUnit::MilliCelsius,
        Some(other) => {
            tracing::warn!("SNMP 厂商 profile {id} 的 temp.unit 未知（{other}），温度列已跳过");
            return None;
        }
    };
    Some(ProfileTemp { oid, unit })
}

/// 点分 OID 字符串 → Oid（无效仅告警返回 None，配置错误不致命）。
fn parse_oid_field(id: &str, field: &str, s: String) -> Option<Oid> {
    match Oid::parse(&s) {
        Ok(oid) if !oid.arcs().is_empty() => Some(oid),
        _ => {
            tracing::warn!("SNMP 厂商 profile {id} 的 {field} OID 无效，已跳过: {s}");
            None
        }
    }
}

/// 点分弧序列字符串 → 弧向量（空段容忍；任一段非数字 → None）。
fn parse_arcs(s: &str) -> Option<Vec<u32>> {
    s.split('.')
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u32>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 内置默认_三家齐备() {
        let profiles = parse_profiles(DEFAULT_PROFILES).expect("内置默认应可解析");
        assert_eq!(profiles.len(), 3);
        let cisco = profiles.iter().find(|p| p.id == "cisco").expect("cisco");
        assert_eq!(cisco.cpu_columns.len(), 2);
        assert!(matches!(
            cisco.mem,
            Some(ProfileMem::Pool { name: Some(_), .. })
        ));
        assert_eq!(cisco.temp_columns.len(), 1);
        assert_eq!(cisco.temp_columns[0].unit, TempUnit::Celsius);
        let huawei = profiles.iter().find(|p| p.id == "huawei").expect("huawei");
        assert_eq!(huawei.cpu_columns.len(), 1);
        assert!(matches!(
            &huawei.mem,
            Some(ProfileMem::EntityPercent {
                size_mega: Some(_),
                size_bytes: Some(_),
                ..
            })
        ));
        let h3c = profiles.iter().find(|p| p.id == "h3c").expect("h3c");
        assert!(matches!(
            &h3c.mem,
            Some(ProfileMem::EntityPercent {
                size_mega: None,
                ..
            })
        ));
    }

    #[test]
    fn 自定义条目_id与最长前缀匹配() {
        let text = r#"
schema = 1

[[profile]]
id = "juniper"
sysobjectid_prefixes = ["1.3.6.1.4.1.2636"]

[[profile.cpu]]
oid = "1.3.6.1.4.1.2636.3.1.13.1.8"

[profile.mem]
kind = "entity"
usage = "1.3.6.1.4.1.2636.3.9.1.1.1.1.4"

[[profile.temp]]
oid = "1.3.6.1.4.1.2636.3.9.1.1.1.1.3"
unit = "milli"

[[profile]]
id = "vendor-a"
sysobjectid_prefixes = ["1.3.6.1.4.1.40000"]
[[profile.cpu]]
oid = "1.3.6.1.4.1.40000.1.0"

[[profile]]
id = "vendor-a-deep"
sysobjectid_prefixes = ["1.3.6.1.4.1.40000.2"]
[[profile.cpu]]
oid = "1.3.6.1.4.1.40000.2.1.0"
"#;
        let registry = VendorProfileRegistry {
            builtin: Vec::new(),
            custom: parse_profiles(text).expect("自定义条目应可解析"),
        };
        assert_eq!(registry.custom.len(), 3);
        // OS id 精确匹配
        assert_eq!(
            registry
                .match_custom(None, Some("juniper"))
                .map(|p| p.id.as_str()),
            Some("juniper")
        );
        // sysObjectID 前缀匹配（识别未命中 OS id 时兜底）
        let oid = Oid::parse("1.3.6.1.4.1.2636.3.50.1").expect("OID 应可解析");
        assert_eq!(
            registry
                .match_custom(Some(&oid), None)
                .map(|p| p.id.as_str()),
            Some("juniper")
        );
        // 最长前缀胜出：同企业号下更深弧命中子条目
        let deep = Oid::parse("1.3.6.1.4.1.40000.2.3.1").expect("OID 应可解析");
        assert_eq!(
            registry
                .match_custom(Some(&deep), None)
                .map(|p| p.id.as_str()),
            Some("vendor-a-deep")
        );
        let shallow = Oid::parse("1.3.6.1.4.1.40000.7.1").expect("OID 应可解析");
        assert_eq!(
            registry
                .match_custom(Some(&shallow), None)
                .map(|p| p.id.as_str()),
            Some("vendor-a")
        );
        // milli 单位落表
        let juniper = registry
            .match_custom(None, Some("juniper"))
            .expect("juniper");
        assert_eq!(juniper.temp_columns[0].unit, TempUnit::MilliCelsius);
    }

    #[test]
    fn 同id覆盖_文件条目优先于内置() {
        let text = r#"
[[profile]]
id = "huawei"
[[profile.cpu]]
oid = "1.3.6.1.4.1.99999.1.0"
"#;
        let registry = VendorProfileRegistry {
            builtin: parse_profiles(DEFAULT_PROFILES).expect("内置默认应可解析"),
            custom: parse_profiles(text).expect("自定义条目应可解析"),
        };
        let matched = registry
            .match_custom(None, Some("huawei"))
            .expect("文件条目应命中");
        assert_eq!(matched.cpu_columns.len(), 1);
        assert_eq!(matched.cpu_columns[0].to_string(), "1.3.6.1.4.1.99999.1.0");
        // 未命中文件的 id 仍回落内置默认
        assert_eq!(
            registry.builtin(Some("cisco")).map(|p| p.id.as_str()),
            Some("cisco")
        );
    }

    #[test]
    fn 非法条目_逐条丢弃不致命() {
        let text = r#"
[[profile]]
id = "broken"
[[profile.cpu]]
oid = "not-an-oid"
[profile.mem]
kind = "weird"
usage = "1.3.6.1.4.1"
[[profile.temp]]
oid = "1.3.6.1.4.1.100.1"
unit = "fahrenheit"

[[profile]]
id = "partial"
[[profile.cpu]]
oid = "1.3.6.1.4.1.100.2.0"
[[profile.temp]]
oid = "1.3.6.1.4.1.100.3.0"
unit = "milli"
"#;
        let profiles = parse_profiles(text).expect("仅条目级无效不应整体报错");
        // broken 三族全无效 → 整体跳过；partial 保留有效列
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, "partial");
        assert_eq!(profiles[0].cpu_columns.len(), 1);
        assert_eq!(profiles[0].temp_columns[0].unit, TempUnit::MilliCelsius);
        assert!(profiles[0].mem.is_none());
    }

    #[test]
    fn 顶层语法错误_整体报错() {
        assert!(parse_profiles("[[profile]\nid = 1").is_err());
    }
}
