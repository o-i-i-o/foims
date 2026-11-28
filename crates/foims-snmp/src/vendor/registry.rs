//! Profile 注册表：加载（内置 include_str! + 现场覆盖）、校验、三级解析、
//! 冻结（docs/SNMP模块重构方案.md §5.3/§5.5/§6.4）。
//!
//! 加载语义（硬规则）：
//! 1. 不做字段级合并——同 id 整条替换、`enabled=false` 整条删除、新 id 追加；
//! 2. 配置错误 = 启动失败（打印文件与字段路径；内置文件解析失败属编译期
//!    不变量破坏，同样 panic）；
//! 3. 不缓存到磁盘，每次启动重新解析（量级毫秒）；
//! 4. 加载结果逐条落日志（覆盖是否生效直接看启动日志）。

use std::collections::HashMap;
use std::fs;
use std::sync::LazyLock;

use super::model::{
    self, ALL_FAMILIES, BuildError, Candidate, CollectMode, Family, OsProfile, Pick, ProfileKind,
    ProfilesFile,
};
use super::profiles::FILES;

/// 现场覆盖目录候选（按优先级）：系统配置目录（服务部署）、当前目录
/// （开发环境；口径与 `foims_common::config::CONFIG_PATHS` 一致）。
const PROFILE_DIRS: [&str; 2] = ["/etc/foims/snmp-profiles.d", "./snmp-profiles.d"];

/// generic 为保留 id：常驻兜底 profile，现场文件不得覆盖/停用（§5.5）。
const GENERIC_ID: &str = "generic";

/// 单 profile 候选数上限（超过告警，§6.4）。
const MAX_CANDIDATES_PER_PROFILE: usize = 16;

/// custom 逃生口白名单（编译期注册、无动态加载，§5.6）。
/// 解释器侧的函数实现见 collect/resource；两侧名单不一致由测试兜底。
pub const CUSTOM_FUNCTIONS: &[&str] = &[];

// ───────────────────────── 三级解析产物 ─────────────────────────

/// 解析后的单族候选链：本层在前，父族次之，generic 最后（disable 为空）。
#[derive(Debug, Default)]
pub struct ResolvedFamily {
    pub mode: CollectMode,
    pub pick: Pick,
    pub candidates: Vec<Candidate>,
}

/// 解析后的采集计划（运行期消费的唯一入口；`replace`/`disable` 已在
/// 解析期裁剪链）。
#[derive(Debug, Default)]
pub struct ResolvedCollectPlan {
    families: [ResolvedFamily; ALL_FAMILIES.len()],
}

impl ResolvedCollectPlan {
    pub fn family(&self, family: Family) -> &ResolvedFamily {
        let idx = ALL_FAMILIES.iter().position(|f| *f == family).unwrap_or(0);
        &self.families[idx]
    }
}

// ───────────────────────── 注册表 ─────────────────────────

/// 冻结的全量注册表：profiles 顺序 = 描述通道识别优先级
///（内置 FILES 顺序 + 文件内声明顺序；现场覆盖同 id 保持原位、新 id 追加）。
pub struct Registry {
    profiles: Vec<OsProfile>,
    /// 解析后的采集计划（按 profile id 索引）。
    plans: HashMap<String, ResolvedCollectPlan>,
    generic_index: usize,
    /// 全部探测 OID（去重，识别预取用）。
    probe_oids: Vec<String>,
}

/// 进程级注册表（启动期一次性加载、只读冻结）。
pub fn registry() -> &'static Registry {
    static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::load);
    &REGISTRY
}

impl Registry {
    /// 按 id 查 profile。
    pub fn get(&self, id: &str) -> Option<&OsProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// 通用兜底 profile（常驻注册表，识别失败时直接返回）。
    pub fn generic(&self) -> &OsProfile {
        &self.profiles[self.generic_index]
    }

    /// profile 对应的解析后采集计划（未登记 id 返回 None）。
    pub fn plan(&self, id: &str) -> Option<&ResolvedCollectPlan> {
        self.plans.get(id)
    }

    /// 全部 profile（描述通道顺序）。
    pub fn iter(&self) -> impl Iterator<Item = &OsProfile> {
        self.profiles.iter()
    }

    /// 全部探测 OID（去重；识别层预取 GET 用）。
    pub fn probe_oids(&self) -> &[String] {
        &self.probe_oids
    }

    // ── 加载 ──

    fn load() -> Self {
        // 1. 内置文件（编译期不变量，解析失败 = 数据损坏）
        let mut entries: Vec<(String, model::ProfileToml)> = Vec::new();
        for (file, text) in FILES {
            let parsed: ProfilesFile = toml::from_str(text).unwrap_or_else(|e| {
                panic!("内置 SNMP profile 数据损坏（不变量破坏）: {file}: {e}");
            });
            for profile in parsed.profile {
                entries.push(((*file).to_string(), profile));
            }
        }

        // 2. 现场覆盖：同 id 整条替换（保持原位）/ enabled=false 删除 / 新 id 追加
        for (path, text) in load_site_files() {
            let parsed: ProfilesFile = match toml::from_str(&text) {
                Ok(p) => p,
                Err(e) => panic!("SNMP profile 现场配置错误（{path}）: {e}"),
            };
            for toml_profile in parsed.profile {
                let id = toml_profile.id.clone();
                let enabled = toml_profile.enabled.unwrap_or(true);
                let pos = entries.iter().position(|(_, p)| p.id == id);
                match (pos, enabled) {
                    (Some(idx), true) => {
                        entries[idx] = (path.clone(), toml_profile);
                        tracing::info!("SNMP profile {id}: source={path} action=replaced");
                    }
                    (Some(idx), false) => {
                        entries.remove(idx);
                        tracing::info!("SNMP profile {id}: source={path} action=disabled");
                    }
                    (None, true) => {
                        entries.push((path.clone(), toml_profile));
                        tracing::info!("SNMP profile {id}: source={path} action=added");
                    }
                    (None, false) => {
                        tracing::info!("SNMP profile {id}: source={path} action=skipped(不存在)");
                    }
                }
            }
        }

        // 3. schema → model 转换（失败 = 启动失败）
        let mut profiles: Vec<OsProfile> = Vec::with_capacity(entries.len());
        for (file, toml_profile) in entries {
            let id = toml_profile.id.clone();
            match model::build_profile(&file, toml_profile) {
                Ok(p) => profiles.push(p),
                Err(BuildError(msg)) => {
                    panic!("SNMP profile 配置错误（{file}）: {id}: {msg}");
                }
            }
        }

        // 4. 语义校验 + 解析（失败 = 启动失败）
        let plans = validate_and_resolve(&profiles);
        let generic_index = profiles
            .iter()
            .position(|p| p.id == GENERIC_ID)
            .unwrap_or_else(|| {
                panic!("SNMP profile 注册表缺少 generic 兜底 profile（不变量破坏）")
            });
        let probe_oids = collect_probe_oids(&profiles);
        tracing::info!(
            "SNMP profile 注册表加载完成: {} 条 profile（generic 兜底）",
            profiles.len()
        );
        Self {
            profiles,
            plans,
            generic_index,
            probe_oids,
        }
    }
}

/// 读取现场覆盖目录（首个存在者；缺失返回空——未部署即全内置）。
fn load_site_files() -> Vec<(String, String)> {
    let dir = match PROFILE_DIRS.iter().find(|d| fs::metadata(d).is_ok()) {
        Some(d) => d,
        None => return Vec::new(),
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "toml"))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|name| {
            let path = format!("{dir}/{name}");
            match fs::read_to_string(&path) {
                Ok(text) => (path, text),
                Err(e) => {
                    panic!("SNMP profile 现场配置读取失败（{path}）: {e}");
                }
            }
        })
        .collect()
}

/// 语义校验（§6.4）+ 三级解析（§5.5）。校验失败 panic（配置错误 = 启动失败）。
fn validate_and_resolve(profiles: &[OsProfile]) -> HashMap<String, ResolvedCollectPlan> {
    // generic 保留 id 校验
    for profile in profiles.iter() {
        if profile.id == GENERIC_ID && profile.kind != ProfileKind::Group {
            panic!("SNMP profile 配置错误: generic 为保留 id，必须为 group 类别");
        }
    }
    // group 归属合法性
    let groups: HashMap<&str, &OsProfile> = profiles
        .iter()
        .filter(|p| p.kind == ProfileKind::Group)
        .map(|p| (p.id.as_str(), p))
        .collect();
    for profile in profiles.iter() {
        match (&profile.group, profile.kind) {
            (Some(_), ProfileKind::Group) => {
                panic!(
                    "SNMP profile 配置错误: {}（kind=group 声明了 group 字段，group 不嵌套）",
                    profile.id
                );
            }
            (Some(group_id), ProfileKind::Os) => {
                if group_id == GENERIC_ID {
                    panic!(
                        "SNMP profile 配置错误: {}（group 不得指向 generic 保留层）",
                        profile.id
                    );
                }
                if !groups.contains_key(group_id.as_str()) {
                    panic!(
                        "SNMP profile 配置错误: {}（group {group_id} 不存在或非 group 类别）",
                        profile.id
                    );
                }
            }
            (None, ProfileKind::Os) if profile.brand.is_empty() => {
                panic!(
                    "SNMP profile 配置错误: {}（kind=os 必须声明 group 或 brand）",
                    profile.id
                );
            }
            _ => {}
        }
        // replace 族无候选 = 隔离意图与空声明矛盾；custom 白名单
        for family in ALL_FAMILIES {
            let Some(family_plan) = profile.collect.family(family) else {
                continue;
            };
            if family_plan.mode == CollectMode::Replace && family_plan.candidates.is_empty() {
                panic!(
                    "SNMP profile 配置错误: {}（collect.{} mode=replace 但未声明候选）",
                    profile.id,
                    family.key()
                );
            }
            for candidate in &family_plan.candidates {
                if let super::model::CandidateKind::Custom(name) = &candidate.kind
                    && !CUSTOM_FUNCTIONS.contains(&name.as_str())
                {
                    panic!(
                        "SNMP profile 配置错误: {}（custom 函数 {name} 未注册）",
                        profile.id
                    );
                }
            }
        }
        // sysobjectid 前缀：完全重复 = 告警（一次性导入的 LibreNMS 数据存在
        // 29 组重复，属上游常态；裁决由 identify 的注册表顺序兜底，行为确定）；
        // 包含关系 = 信息（层级匹配常态）
        let mut prefixes: Vec<(Vec<u32>, &str)> = Vec::new();
        for profile in profiles.iter() {
            let Some(matcher) = &profile.matcher else {
                continue;
            };
            for prefix in &matcher.sysobjectid {
                if let Some((_, owner)) = prefixes.iter().find(|(p, _)| p == prefix) {
                    tracing::warn!(
                        "SNMP profile 前缀重复: {} 与 {owner} 完全重复（注册表序在前者胜出，请人工确认）",
                        profile.id
                    );
                }
                for (other, other_owner) in &prefixes {
                    let contained = prefix.starts_with(other.as_slice())
                        || other.starts_with(prefix.as_slice());
                    if contained {
                        tracing::info!(
                            "SNMP profile 前缀层级: {} 与 {other_owner} 存在包含关系（正常层级匹配）",
                            profile.id
                        );
                    }
                }
                prefixes.push((prefix.clone(), profile.id.as_str()));
            }
        }
        // 候选数告警（取消截断后候选数显式可审，§6.4）
        let count: usize = ALL_FAMILIES
            .iter()
            .map(|f| {
                profile
                    .collect
                    .family(*f)
                    .map_or(0, |fp| fp.candidates.len())
            })
            .sum();
        if count > MAX_CANDIDATES_PER_PROFILE {
            tracing::warn!(
                "SNMP profile {} 候选数 {count} 超过 {MAX_CANDIDATES_PER_PROFILE}（探测成本随候选数线性增长）",
                profile.id
            );
        }
    }

    // 三级解析：本层 → 父族 → generic（§5.5）
    let mut plans = HashMap::new();
    for profile in profiles.iter() {
        plans.insert(profile.id.clone(), resolve_profile(profile, &groups));
    }
    plans
}

/// 单 profile 三级解析。generic 自身不再向自己回退（防环）。
fn resolve_profile(profile: &OsProfile, groups: &HashMap<&str, &OsProfile>) -> ResolvedCollectPlan {
    let is_generic = profile.id == GENERIC_ID;
    let parent = profile
        .group
        .as_deref()
        .and_then(|g| groups.get(g))
        .copied();
    let generic = groups
        .get(GENERIC_ID)
        .copied()
        .unwrap_or_else(|| panic!("SNMP profile 注册表缺少 generic 兜底 profile（不变量破坏）"));

    let mut out = ResolvedCollectPlan::default();
    for (idx, family) in ALL_FAMILIES.iter().enumerate() {
        let own = profile.collect.family(*family);
        let parent_plan: Option<&super::model::FamilyPlan> =
            parent.and_then(|p| p.collect.family(*family));
        let generic_plan = generic.collect.family(*family);

        let (mode, pick, mut candidates) = match own {
            Some(own) if own.mode == CollectMode::Disable => {
                out.families[idx] = ResolvedFamily {
                    mode: CollectMode::Disable,
                    pick: own.pick,
                    candidates: Vec::new(),
                };
                continue;
            }
            Some(own) => {
                let mut candidates = own.candidates.clone();
                if own.mode == CollectMode::Inherit
                    && let Some(p) = parent_plan
                {
                    candidates.extend(p.candidates.iter().cloned());
                }
                (own.mode, own.pick, candidates)
            }
            None => {
                let (pick, base) = match parent_plan {
                    Some(p) => (p.pick, p.candidates.clone()),
                    None => (generic_plan.map_or(Pick::Max, |g| g.pick), Vec::new()),
                };
                (CollectMode::Inherit, pick, base)
            }
        };
        // generic 公共链始终是最后兜底（disable 已提前返回；generic 自身除外）
        if !is_generic && let Some(g) = generic_plan {
            candidates.extend(g.candidates.iter().cloned());
        }
        out.families[idx] = ResolvedFamily {
            mode,
            pick,
            candidates,
        };
    }
    out
}

/// 收集全部探测 OID（去重，保序）。
fn collect_probe_oids(profiles: &[OsProfile]) -> Vec<String> {
    let mut oids = Vec::new();
    for profile in profiles {
        let Some(matcher) = &profile.matcher else {
            continue;
        };
        for probe in &matcher.probes {
            if !oids.contains(&probe.oid) {
                oids.push(probe.oid.clone());
            }
        }
    }
    oids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_from(texts: &[(&str, &str)]) -> Registry {
        // 测试直连 build 流程：内置 generic + 注入条目
        let mut profiles = Vec::new();
        for (file, text) in texts {
            let parsed: ProfilesFile = toml::from_str(text).unwrap();
            for toml_profile in parsed.profile {
                profiles.push(model::build_profile(file, toml_profile).unwrap());
            }
        }
        let mut generic_toml: ProfilesFile = toml::from_str(
            super::super::profiles::FILES
                .iter()
                .find(|(f, _)| *f == "generic.toml")
                .map(|(_, t)| *t)
                .unwrap(),
        )
        .unwrap();
        for p in generic_toml.profile.drain(..) {
            profiles.push(model::build_profile("generic.toml", p).unwrap());
        }
        let plans = validate_and_resolve(&profiles);
        let generic_index = profiles.iter().position(|p| p.id == GENERIC_ID).unwrap();
        Registry {
            profiles,
            plans,
            generic_index,
            probe_oids: Vec::new(),
        }
    }

    const OS_TEMPLATE: &str = r#"
[[profile]]
id = "testos"
kind = "os"
group = "testgrp"
brand = "T"
display_name = "T"
[profile.match]
sysobjectid = ["1.3.6.1.4.1.99999"]
[[profile.collect.cpu.candidate]]
[[profile.collect.cpu.candidate.column]]
oid = "1.3.6.1.4.1.99999.1.0"
indexed = false
value = "percent"
"#;

    const GROUP_TEMPLATE: &str = r#"
[[profile]]
id = "testgrp"
kind = "group"
brand = "T"
[[profile.collect.mem.candidate]]
kind = "pool"
[[profile.collect.mem.candidate.field]]
role = "used"
oid = "1.3.6.1.4.1.99999.2.0"
indexed = false
[[profile.collect.mem.candidate.field]]
role = "free"
oid = "1.3.6.1.4.1.99999.3.0"
indexed = false
"#;

    #[test]
    fn 语义矩阵_inherit链() {
        let reg = load_from(&[("test.toml", GROUP_TEMPLATE), ("test.toml", OS_TEMPLATE)]);
        let plan = reg.plan("testos").unwrap();
        // 本层 cpu → group（无 cpu）→ generic 公共链
        let cpu = plan.family(Family::Cpu);
        assert!(!cpu.candidates.is_empty());
        assert!(
            cpu.candidates
                .iter()
                .any(|c| matches!(c.kind, super::super::model::CandidateKind::Columns(_))),
            "本层候选在前"
        );
        assert!(
            cpu.candidates
                .iter()
                .any(|c| matches!(c.kind, super::super::model::CandidateKind::UcdCpuCounters)),
            "generic 公共链兜底（UCD 计数器 + hrProcessor）"
        );
        // 本层未声明 mem → 父族 pool → generic
        let mem = plan.family(Family::Mem);
        assert!(
            mem.candidates
                .iter()
                .any(|c| matches!(c.kind, super::super::model::CandidateKind::Pool(_)))
        );
    }

    #[test]
    fn 语义矩阵_replace与disable() {
        let replace_os = r#"
[[profile]]
id = "repl"
kind = "os"
group = "testgrp"
brand = "T"
[profile.collect.cpu]
mode = "replace"
[[profile.collect.cpu.candidate]]
[[profile.collect.cpu.candidate.column]]
oid = "1.3.6.1.4.1.99999.9.0"
indexed = false
value = "percent"
[profile.collect.temp]
mode = "disable"
"#;
        let reg = load_from(&[("test.toml", GROUP_TEMPLATE), ("test.toml", replace_os)]);
        let plan = reg.plan("repl").unwrap();
        let cpu = plan.family(Family::Cpu);
        assert_eq!(cpu.mode, CollectMode::Replace);
        // replace：本层候选在前 + generic 公共链最后兜底（不继承父族）
        assert!(
            matches!(
                &cpu.candidates[0].kind,
                super::model::CandidateKind::Columns(c) if c.oid.to_string() == "1.3.6.1.4.1.99999.9.0"
            ),
            "本层候选在前"
        );
        assert!(cpu.candidates.len() > 1, "generic 公共链仍兜底");
        assert_eq!(plan.family(Family::Temp).mode, CollectMode::Disable);
        assert!(plan.family(Family::Temp).candidates.is_empty());
    }

    #[test]
    fn 校验_group嵌套拒绝() {
        let bad = r#"
[[profile]]
id = "nested"
kind = "group"
brand = "T"
group = "testgrp"
"#;
        let result = std::panic::catch_unwind(|| {
            load_from(&[("test.toml", GROUP_TEMPLATE), ("test.toml", bad)])
        });
        assert!(result.is_err());
    }

    #[test]
    fn 校验_group指向generic拒绝() {
        let bad = r#"
[[profile]]
id = "bados"
kind = "os"
group = "generic"
brand = "T"
"#;
        let result = std::panic::catch_unwind(|| load_from(&[("test.toml", bad)]));
        assert!(result.is_err());
    }

    #[test]
    fn 校验_replace空候选拒绝() {
        let bad = r#"
[[profile]]
id = "badreplace"
kind = "os"
group = "testgrp"
brand = "T"
[profile.collect.cpu]
mode = "replace"
"#;
        let result = std::panic::catch_unwind(|| {
            load_from(&[("test.toml", GROUP_TEMPLATE), ("test.toml", bad)])
        });
        assert!(result.is_err());
    }

    #[test]
    fn 校验_前缀完全重复告警不拒绝() {
        // 一次性导入的 LibreNMS 数据存在重复前缀（上游常态）：不拒绝加载，
        // 仅启动告警；裁决由 identify 注册表顺序兜底（先声明者优先）
        let dup = r#"
[[profile]]
id = "dupos"
kind = "os"
group = "testgrp"
brand = "T"
[profile.match]
sysobjectid = ["1.3.6.1.4.1.99999"]
"#;
        let result = std::panic::catch_unwind(|| {
            load_from(&[
                ("test.toml", GROUP_TEMPLATE),
                ("test.toml", OS_TEMPLATE),
                ("test.toml", dup),
            ])
        });
        assert!(result.is_ok());
    }

    #[test]
    fn 校验_custom未注册拒绝() {
        let bad = r#"
[[profile]]
id = "badcustom"
kind = "os"
group = "testgrp"
brand = "T"
[[profile.collect.cpu.candidate]]
custom = "not_registered_fn"
"#;
        let result = std::panic::catch_unwind(|| {
            load_from(&[("test.toml", GROUP_TEMPLATE), ("test.toml", bad)])
        });
        assert!(result.is_err());
    }

    #[test]
    fn 内置注册表_加载与兜底() {
        let reg = registry();
        assert!(reg.iter().count() > 700, "存量导入后应有 700+ 条 profile");
        // generic 兜底 profile
        assert_eq!(reg.generic().id, GENERIC_ID);
        assert!(reg.get("generic").is_some());
        assert!(reg.get("cisco").is_some(), "品牌族常驻");
        assert!(reg.get("iosxe").is_some());
        assert!(reg.get("fortigate").is_some());
        assert!(reg.get("__not_an_os__").is_none());
        // 内置三家采集计划就位
        for id in ["cisco", "huawei", "h3c"] {
            let plan = reg.plan(id).unwrap_or_else(|| panic!("{id} 采集计划缺失"));
            assert!(
                !plan.family(Family::Cpu).candidates.is_empty(),
                "{id} cpu 链"
            );
            assert!(
                !plan.family(Family::Mem).candidates.is_empty(),
                "{id} mem 链"
            );
            assert!(
                !plan.family(Family::Temp).candidates.is_empty(),
                "{id} temp 链"
            );
        }
        // generic 公共链：cpu/mem/temp/storage/sensors 均有候选
        let g = reg.plan(GENERIC_ID).unwrap();
        for family in [
            Family::Cpu,
            Family::Mem,
            Family::Temp,
            Family::Storage,
            Family::Sensors,
        ] {
            assert!(
                !g.family(family).candidates.is_empty(),
                "generic {family:?} 链缺失"
            );
        }
    }

    #[test]
    fn 内置注册表_探测oid去重收集() {
        let reg = registry();
        let oids = reg.probe_oids();
        assert!(!oids.is_empty(), "存量导入含 31 条探测检查");
        let unique: std::collections::HashSet<&String> = oids.iter().collect();
        assert_eq!(unique.len(), oids.len(), "探测 OID 应去重");
    }
}
