//! Profile 注册表域：模型 + 注册表 + 识别纯函数（docs/SNMP模块重构方案.md §5）。
//!
//! 设计约束：
//! - 本域是「纯数据 + 纯函数」层：无 IO、无 async、不依赖 collect。
//!   `match.probes` 的 GET 执行在采集层（live 探测入口 / sysinfo）完成，
//!   本域只声明 probe 清单（[`registry::Registry::probe_oids`]）并消费注入值；
//! - 注册表冻结于 `static LazyLock<Registry>`（[`registry`]），数据长期存活，
//!   经其取得的引用即 'static；配置错误 = 启动失败（§5.3）；
//! - 识别（[`identify`]）返回 [`Identity`]，`profile` 直接进采集，
//!   generic 是常驻兜底 profile，采集层不再有 None 分支（§5.2）。

pub mod model;
mod profiles;
pub mod registry;

pub mod identify;

pub use identify::{identify, Identity, NET_SNMP_ENTERPRISE, PlatformCaps};
pub use model::{
    Candidate, CandidateKind, CapsKey, CollectMode, CollectPlan, Column, EntityFields,
    Family, FamilyPlan, ALL_FAMILIES, Limits, Matcher, OsProfile, PercentFields, Pick,
    PoolFields, Probe, ProbeOp, ProfileKind, SensorKind, SkipField, SkipOp, SkipRule,
    StorageType, Unit,
};
pub use registry::{
    registry, CUSTOM_FUNCTIONS, Registry, ResolvedCollectPlan, ResolvedFamily,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 注册表_品牌匹配口径() {
        let reg = registry();
        // 品牌族与品牌级规则同源（导入自 community/）
        assert_eq!(reg.get("cisco").map(|p| p.kind), Some(ProfileKind::Group));
        assert_eq!(reg.get("cisco-brand").map(|p| p.brand.as_str()), Some("Cisco"));
        assert_eq!(reg.get("huawei").map(|p| p.brand.as_str()), Some("Huawei"));
        assert_eq!(reg.get("h3c").map(|p| p.brand.as_str()), Some("H3C"));
        // 识别-only 品牌：kind=os 无 group，三级解析贯穿到 generic 公共链
        let juniper = reg.get("juniper").unwrap();
        assert_eq!(juniper.group, None);
        let plan = reg.plan("juniper").unwrap();
        assert!(
            !plan.family(Family::Cpu).candidates.is_empty(),
            "juniper 采集贯穿到 generic 公共链兜底"
        );
    }

    #[test]
    fn 注册表_覆盖路径常量() {
        // generic 兜底 profile 恒在且为 group
        let generic = registry().generic();
        assert_eq!(generic.id, "generic");
        assert_eq!(generic.kind, ProfileKind::Group);
        assert!(generic.matcher.is_none(), "generic 不参与识别");
    }
}
