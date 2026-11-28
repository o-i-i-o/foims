//! Profile 数据模型：TOML schema（serde）→ 运行时结构（model）。
//!
//! 设计（docs/SNMP模块重构方案.md §5.2/§6）：一品牌一文件、整条替换、
//! 无字段合并。本模块只放类型与 schema→model 转换，不含 IO；
//! 加载/覆盖/校验/冻结见 [`super::registry`]，匹配语义见 [`super::identify`]。

use std::collections::BTreeMap;

use async_snmp::Oid;
use regex::Regex;
use serde::Deserialize;

// ───────────────────────── 运行时模型 ─────────────────────────

/// profile 类别：品牌族（不参与识别）与可识别 OS。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileKind {
    Group,
    Os,
}

/// 族继承模式（§5.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollectMode {
    /// 本层候选在前，父族候选作运行期回退，贯穿到 generic（默认）。
    #[default]
    Inherit,
    /// 仅本层候选，不向父族运行期回退（generic 公共链仍是最后兜底）。
    Replace,
    /// 该族彻底关闭，generic 也不兜底。
    Disable,
}

/// 行聚合（§5.6 pick 语义）：同一候选内多索引行 → 单读数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pick {
    #[default]
    Max,
    First,
    Sum,
}

/// 识别匹配器（kind=Group 恒为 None）。
#[derive(Debug)]
pub struct Matcher {
    /// sysObjectID 前缀（弧序列）：前缀通道，最长前缀胜出。
    pub sysobjectid: Vec<Vec<u32>>,
    /// sysDescr 词元关键词：描述通道（词边界全词元命中）。
    pub descr_keywords: Vec<String>,
    /// sysDescr 正则（启动期编译，失败即启动失败）：描述通道。
    pub descr_regex: Vec<Regex>,
    /// 主动探测（静态证据全未命中时逐条 GET 比对，多条 AND）。
    pub probes: Vec<Probe>,
}

/// 单条探测检查：GET `oid` 后按 `op` 比对响应值与 `value`。
#[derive(Debug, Clone)]
pub struct Probe {
    pub oid: String,
    pub op: ProbeOp,
    pub value: String,
}

/// 探测/行过滤比对方式（LibreNMS snmpget 语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOp {
    /// OID 有非异常响应即命中（value 不参与）
    Exists,
    /// 值完全相等
    Eq,
    /// 值不相等
    Ne,
    /// 值包含子串
    Has,
    /// 值以前缀开头
    Starts,
    /// 值匹配正则（Rust regex 语法）
    Regex,
}

impl ProbeOp {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "exists" => Self::Exists,
            "eq" => Self::Eq,
            "ne" => Self::Ne,
            "has" => Self::Has,
            "starts" => Self::Starts,
            "regex" => Self::Regex,
            _ => return None,
        })
    }
}

/// 品牌画像（一条 [[profile]]）。
#[derive(Debug)]
pub struct OsProfile {
    pub id: String,
    pub kind: ProfileKind,
    /// 品牌族 id（仅 kind=Os；不嵌套、不得指向 generic）。
    pub group: Option<String>,
    /// 展示/统计/前端字段（devices.brand）。
    pub brand: String,
    pub display_name: String,
    /// kind=Group 时为 None。
    pub matcher: Option<Matcher>,
    pub collect: CollectPlan,
    /// 来源文件（仅日志：内置嵌入名或现场覆盖路径）。
    pub source: String,
}

/// 采集计划：六个资源族，`None` 表示本层不表态，继续向 group/generic 找。
#[derive(Debug, Default)]
pub struct CollectPlan {
    pub cpu: Option<FamilyPlan>,
    pub mem: Option<FamilyPlan>,
    pub temp: Option<FamilyPlan>,
    pub storage: Option<FamilyPlan>,
    pub sensors: Option<FamilyPlan>,
    pub device: Option<FamilyPlan>,
}

impl CollectPlan {
    pub fn family(&self, family: Family) -> Option<&FamilyPlan> {
        match family {
            Family::Cpu => self.cpu.as_ref(),
            Family::Mem => self.mem.as_ref(),
            Family::Temp => self.temp.as_ref(),
            Family::Storage => self.storage.as_ref(),
            Family::Sensors => self.sensors.as_ref(),
            Family::Device => self.device.as_ref(),
        }
    }

    fn family_mut(&mut self, family: Family) -> &mut Option<FamilyPlan> {
        match family {
            Family::Cpu => &mut self.cpu,
            Family::Mem => &mut self.mem,
            Family::Temp => &mut self.temp,
            Family::Storage => &mut self.storage,
            Family::Sensors => &mut self.sensors,
            Family::Device => &mut self.device,
        }
    }
}

/// 资源族（CollectPlan 的键 + 解释器的调度维度）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Family {
    Cpu,
    Mem,
    Temp,
    Storage,
    Sensors,
    Device,
}

pub const ALL_FAMILIES: [Family; 6] = [
    Family::Cpu,
    Family::Mem,
    Family::Temp,
    Family::Storage,
    Family::Sensors,
    Family::Device,
];

impl Family {
    pub fn key(self) -> &'static str {
        match self {
            Family::Cpu => "cpu",
            Family::Mem => "mem",
            Family::Temp => "temp",
            Family::Storage => "storage",
            Family::Sensors => "sensors",
            Family::Device => "device",
        }
    }
}

/// 单族计划：模式 + 行聚合 + 有序候选（首个 Success 生效，§5.6）。
#[derive(Debug, Default)]
pub struct FamilyPlan {
    pub mode: CollectMode,
    pub pick: Pick,
    pub candidates: Vec<Candidate>,
}

/// 采集能力门控（平台域吸收代码推导的 caps，§4.3/§5.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapsKey {
    UcdMem,
    UcdCpu,
    HrStorage,
    HrProcessor,
    LmSensors,
    EntitySensors,
}

impl CapsKey {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "ucd_mem" => Self::UcdMem,
            "ucd_cpu" => Self::UcdCpu,
            "hr_storage" => Self::HrStorage,
            "hr_processor" => Self::HrProcessor,
            "lm_sensors" => Self::LmSensors,
            "entity_sensors" => Self::EntitySensors,
            _ => return None,
        })
    }
}

/// 行过滤（字段 + 比较符 + 值）。
#[derive(Debug, Clone)]
pub struct SkipRule {
    pub field: SkipField,
    pub op: SkipOp,
    pub value: String,
}

/// 行过滤目标字段：行名（pool name / 传感器标签）或原始值文本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipField {
    Name,
    Value,
}

impl SkipField {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "name" => Self::Name,
            "value" => Self::Value,
            _ => return None,
        })
    }
}

/// 行过滤比较符。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipOp {
    Eq,
    Ne,
    Has,
    /// 前缀整词命中：值本身或值后跟 `/`（伪文件系统挂载点子树排除，
    /// `/run` 命中 `/run` 与 `/run/user/1000`，不误伤 `/data/run`）
    Prefix,
}

impl SkipOp {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "eq" => Self::Eq,
            "ne" => Self::Ne,
            "has" => Self::Has,
            "prefix" => Self::Prefix,
            _ => return None,
        })
    }
}

/// 阈值（low/warn/high），供告警评估消费。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Limits {
    pub low: Option<f64>,
    pub warn: Option<f64>,
    pub high: Option<f64>,
}

/// 单个采集候选：门控 + 行过滤 + 变换 + 形态。
#[derive(Debug, Clone)]
pub struct Candidate {
    /// 平台能力门控（全部满足才尝试；未声明不受限）。
    pub require_caps: Vec<CapsKey>,
    /// 需要上一轮差值状态（UCD CPU 计数器等；实时路径自动跳过）。
    pub require_prev: bool,
    pub skip: Vec<SkipRule>,
    pub limits: Option<Limits>,
    /// 数值 → 语义映射（风扇/电源/温度状态）。
    pub states: BTreeMap<i64, String>,
    /// 传感器类型标记（sensors 族列候选必填；entity_sensors 缺省 = 全类型）。
    pub sensor: Option<SensorKind>,
    pub kind: CandidateKind,
}

/// 候选形态。标准 MIB 表（hrStorage / ENTITY-SENSOR / UCD 计数器）为
/// 吸收型内建候选（§4.3），其余走声明式列/角色。
#[derive(Debug, Clone)]
pub enum CandidateKind {
    /// 值列（CPU 百分比 / 温度 / 传感器值），逐列一条候选由 TOML 拆分，
    /// 本枚举恒持单列。
    Columns(Box<Column>),
    /// 内存/存储池：name/used/free/total 角色列，语义按出现角色推导：
    /// used+free → total=used+free；total+free → used=total−free；
    /// total+used → free=total−used。
    Pool(PoolFields),
    /// 实体表：usage 百分比 + size（优先 MB 口径，字节兜底）。
    Entity(EntityFields),
    /// 百分比（+可选 total 缓存）：只报使用率，无总量时 total 为空。
    Percent(PercentFields),
    /// HOST-RESOURCES hrStorageTable 行（Ram/FixedDisk）。
    HrStorage { storage_type: StorageType },
    /// ENTITY-SENSOR-MIB 标准传感器表（内建吸收；sensor=None 读取全部类型）。
    EntitySensors,
    /// UCD ssCpuRaw 计数器差值（需要上一轮状态）。
    UcdCpuCounters,
    /// 设备信息：model/serial/version 角色列（按序尝试，首个非空生效）。
    Device(DeviceFields),
    /// 代码逃生口：编译期注册的静态函数名（§5.6）。
    Custom(String),
}

/// 设备信息角色列。
#[derive(Debug, Default, Clone)]
pub struct DeviceFields {
    pub model: Vec<Column>,
    pub serial: Vec<Column>,
    pub version: Vec<Column>,
}

/// 值列定义。
#[derive(Debug, Clone)]
pub struct Column {
    pub oid: Oid,
    /// true = 表列（walk 单弧索引行）；false = 标量（OID 含完整实例）。
    pub indexed: bool,
    /// 语义标记（percent/count/celsius…；CPU 列为 percent）。
    pub value: Option<String>,
    /// 单位换算（transform 原语，§5.6）。
    pub unit: Option<Unit>,
    /// CPU 列：LibreNMS processor_precision 语义（负值 = 上游上报空闲率）。
    /// 内存/存储列：字节乘数（unit 不足表达时使用）。
    pub precision: Option<i32>,
    pub divisor: u64,
    pub multiplier: u64,
}

/// 池角色列。
#[derive(Debug, Default, Clone)]
pub struct PoolFields {
    pub name: Option<Column>,
    pub used: Option<Column>,
    pub free: Option<Column>,
    pub total: Option<Column>,
}

/// 实体表内存列。
#[derive(Debug, Clone)]
pub struct EntityFields {
    pub usage: Column,
    pub size_mega: Option<Column>,
    pub size_bytes: Option<Column>,
}

/// 百分比列（+可选总量列缓存）。
#[derive(Debug, Clone)]
pub struct PercentFields {
    pub percent: Column,
    pub total: Option<Column>,
}

/// hrStorage 行类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageType {
    /// 物理内存行（实时内存兜底口径）。
    Ram,
    /// 本地固定磁盘行（5 分钟磁盘采集）。
    FixedDisk,
}

impl StorageType {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "ram" => Self::Ram,
            "fixed_disk" => Self::FixedDisk,
            _ => return None,
        })
    }

    /// hrStorageType OID 末弧（Ram=2 / FixedDisk=4）。
    pub fn type_arc(self) -> u32 {
        match self {
            StorageType::Ram => 2,
            StorageType::FixedDisk => 4,
        }
    }
}

/// 传感器类型（§4.2 一次做全）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorKind {
    Temperature,
    Voltage,
    Current,
    Power,
    Fan,
    Status,
    Percent,
    Frequency,
    Load,
}

impl SensorKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "temperature" => Self::Temperature,
            "voltage" => Self::Voltage,
            "current" => Self::Current,
            "power" => Self::Power,
            "fan" => Self::Fan,
            "status" => Self::Status,
            "percent" => Self::Percent,
            "frequency" => Self::Frequency,
            "load" => Self::Load,
            _ => return None,
        })
    }

    pub fn key(self) -> &'static str {
        match self {
            SensorKind::Temperature => "temperature",
            SensorKind::Voltage => "voltage",
            SensorKind::Current => "current",
            SensorKind::Power => "power",
            SensorKind::Fan => "fan",
            SensorKind::Status => "status",
            SensorKind::Percent => "percent",
            SensorKind::Frequency => "frequency",
            SensorKind::Load => "load",
        }
    }
}

/// 数值单位换算（transform 原语内部统一换算到基准单位：字节/摄氏度）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Bytes,
    Kb,
    Mb,
    Blocks,
    Celsius,
    Milli,
    Micro,
    Fahrenheit,
}

impl Unit {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "bytes" => Self::Bytes,
            "kb" => Self::Kb,
            "mb" => Self::Mb,
            "blocks" => Self::Blocks,
            "celsius" => Self::Celsius,
            "milli" => Self::Milli,
            "micro" => Self::Micro,
            "fahrenheit" => Self::Fahrenheit,
            _ => return None,
        })
    }

    /// 换算到基准单位（字节：×factor；摄氏度：温度换算在 transform 内）。
    pub fn byte_factor(self) -> u64 {
        match self {
            Unit::Bytes | Unit::Blocks => 1,
            Unit::Kb => 1024,
            Unit::Mb => 1024 * 1024,
            _ => 1,
        }
    }
}

// ───────────────────────── TOML schema（serde） ─────────────────────────

#[derive(Deserialize)]
pub(super) struct ProfilesFile {
    #[serde(default)]
    pub profile: Vec<ProfileToml>,
}

#[derive(Deserialize)]
pub(super) struct ProfileToml {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub brand: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    /// false 在加载后整条删除（现场覆盖停用，§5.3）。
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(rename = "match")]
    #[serde(default)]
    pub match_: Option<MatchToml>,
    #[serde(default)]
    pub collect: Option<CollectToml>,
}

#[derive(Deserialize)]
pub(super) struct MatchToml {
    #[serde(default)]
    pub sysobjectid: Vec<String>,
    #[serde(default)]
    pub descr_keywords: Vec<String>,
    #[serde(default)]
    pub descr_regex: Vec<String>,
    #[serde(default)]
    pub probes: Vec<ProbeToml>,
}

#[derive(Deserialize)]
pub(super) struct ProbeToml {
    pub oid: String,
    #[serde(default)]
    pub op: Option<String>,
    /// op=exists 时可不带 value。
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct CollectToml {
    #[serde(default)]
    pub cpu: Option<FamilyToml>,
    #[serde(default)]
    pub mem: Option<FamilyToml>,
    #[serde(default)]
    pub temp: Option<FamilyToml>,
    #[serde(default)]
    pub storage: Option<FamilyToml>,
    #[serde(default)]
    pub sensors: Option<FamilyToml>,
    #[serde(default)]
    pub device: Option<FamilyToml>,
}

#[derive(Deserialize)]
pub(super) struct FamilyToml {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub pick: Option<String>,
    #[serde(default)]
    pub candidate: Vec<CandidateToml>,
}

#[derive(Deserialize)]
pub(super) struct CandidateToml {
    /// 平台能力门控（如 ["ucd_mem"]）。
    #[serde(default)]
    pub require_caps: Vec<String>,
    /// 需要上一轮差值状态。
    #[serde(default)]
    pub require_prev: bool,
    #[serde(default)]
    pub skip: Vec<SkipToml>,
    #[serde(default)]
    pub limits: Option<LimitsToml>,
    /// 数值 → 语义（TOML 键为字符串数字）。
    #[serde(default)]
    pub states: Option<BTreeMap<String, String>>,
    /// 代码逃生口。
    #[serde(default)]
    pub custom: Option<String>,
    /// 形态：pool | entity | percent | storage | entity_sensors | counters。
    #[serde(default)]
    pub kind: Option<String>,
    /// kind=storage 的行类型：ram | fixed_disk。
    #[serde(default)]
    pub hr_type: Option<String>,
    /// kind=entity_sensors 的传感器类型。
    #[serde(default)]
    pub sensor: Option<String>,
    #[serde(default)]
    pub column: Vec<ColumnToml>,
    #[serde(default)]
    pub field: Vec<FieldToml>,
}

#[derive(Deserialize)]
pub(super) struct SkipToml {
    pub field: String,
    pub op: String,
    pub value: String,
}

#[derive(Deserialize)]
pub(super) struct LimitsToml {
    #[serde(default)]
    pub low: Option<f64>,
    #[serde(default)]
    pub warn: Option<f64>,
    #[serde(default)]
    pub high: Option<f64>,
}

#[derive(Deserialize)]
pub(super) struct ColumnToml {
    pub oid: String,
    /// 默认 true（表列）；false = 标量。
    #[serde(default)]
    pub indexed: Option<bool>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub precision: Option<i32>,
    #[serde(default)]
    pub divisor: Option<u64>,
    #[serde(default)]
    pub multiplier: Option<u64>,
}

#[derive(Deserialize)]
pub(super) struct FieldToml {
    /// 语义角色：name/used/free/total/usage/size_mega/size_bytes/percent。
    pub role: String,
    pub oid: String,
    #[serde(default)]
    pub indexed: Option<bool>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub precision: Option<i32>,
    #[serde(default)]
    pub divisor: Option<u64>,
    #[serde(default)]
    pub multiplier: Option<u64>,
}

// ───────────────────────── schema → model 转换 ─────────────────────────

/// 转换错误：携带字段路径，启动失败时输出。
#[derive(Debug)]
pub(super) struct BuildError(pub String);

impl BuildError {
    fn new(context: &str, path: &str, msg: impl std::fmt::Display) -> Self {
        Self(format!("{context}: {path}: {msg}"))
    }
}

type BuildResult<T> = Result<T, BuildError>;

/// 点分 OID 字符串 → Oid。
fn parse_oid(context: &str, path: &str, s: &str) -> BuildResult<Oid> {
    let arcs: Vec<u32> = s
        .split('.')
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.parse::<u32>()
                .map_err(|_| BuildError::new(context, path, format!("OID 段非法: {s}")))
        })
        .collect::<BuildResult<_>>()?;
    if arcs.is_empty() {
        return Err(BuildError::new(context, path, format!("OID 为空: {s}")));
    }
    Ok(Oid::new(arcs))
}

fn parse_probe(context: &str, path: &str, p: ProbeToml) -> BuildResult<Probe> {
    parse_oid(context, &format!("{path}.oid"), &p.oid)?;
    let op = match p.op.as_deref() {
        None => ProbeOp::Exists,
        Some(s) => ProbeOp::parse(s).ok_or_else(|| {
            BuildError::new(context, &format!("{path}.op"), format!("未知 op: {s}"))
        })?,
    };
    let value = p.value.unwrap_or_default();
    if op != ProbeOp::Exists && value.is_empty() {
        return Err(BuildError::new(
            context,
            &format!("{path}.value"),
            "该 op 需要 value",
        ));
    }
    Ok(Probe {
        oid: p.oid,
        op,
        value,
    })
}

pub(super) fn build_matcher(context: &str, m: MatchToml) -> BuildResult<Matcher> {
    let mut sysobjectid = Vec::new();
    for (i, s) in m.sysobjectid.iter().enumerate() {
        sysobjectid.push(
            parse_oid(context, &format!("match.sysobjectid[{i}]"), s)?
                .arcs()
                .to_vec(),
        );
    }
    let mut descr_regex = Vec::new();
    for (i, s) in m.descr_regex.iter().enumerate() {
        descr_regex.push(Regex::new(s).map_err(|e| {
            BuildError::new(
                context,
                &format!("match.descr_regex[{i}]"),
                format!("正则编译失败: {e}"),
            )
        })?);
    }
    let mut probes = Vec::new();
    for (i, p) in m.probes.into_iter().enumerate() {
        probes.push(parse_probe(context, &format!("match.probes[{i}]"), p)?);
    }
    Ok(Matcher {
        sysobjectid,
        descr_keywords: m.descr_keywords,
        descr_regex,
        probes,
    })
}

fn build_column(context: &str, path: &str, c: ColumnToml) -> BuildResult<Column> {
    let unit = match &c.unit {
        None => None,
        Some(s) => Some(Unit::parse(s).ok_or_else(|| {
            BuildError::new(context, &format!("{path}.unit"), format!("未知单位: {s}"))
        })?),
    };
    Ok(Column {
        oid: parse_oid(context, &format!("{path}.oid"), &c.oid)?,
        indexed: c.indexed.unwrap_or(true),
        value: c.value,
        unit,
        precision: c.precision,
        divisor: c.divisor.unwrap_or(1),
        multiplier: c.multiplier.unwrap_or(1),
    })
}

fn build_fields(
    context: &str,
    path: &str,
    fields: Vec<FieldToml>,
) -> BuildResult<Vec<(String, Column)>> {
    fields
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            let path = format!("{path}.field[{i}]({})", f.role);
            Ok((
                f.role,
                build_column(
                    context,
                    &path,
                    ColumnToml {
                        oid: f.oid,
                        indexed: f.indexed,
                        value: None,
                        unit: f.unit,
                        precision: f.precision,
                        divisor: f.divisor,
                        multiplier: f.multiplier,
                    },
                )?,
            ))
        })
        .collect()
}

fn take_role(
    context: &str,
    fields: &mut Vec<(String, Column)>,
    role: &str,
    path: &str,
    required: bool,
) -> BuildResult<Option<Column>> {
    let idx = fields.iter().position(|(r, _)| r == role);
    match idx {
        Some(i) => Ok(Some(fields.remove(i).1)),
        None if required => Err(BuildError::new(
            context,
            path,
            format!("缺少 role={role} 列"),
        )),
        None => Ok(None),
    }
}

pub(super) fn build_candidate(
    context: &str,
    family: Family,
    c: CandidateToml,
) -> BuildResult<Candidate> {
    let path = format!("{context}:collect.{}.candidate", family.key());
    let mut require_caps = Vec::new();
    for (i, s) in c.require_caps.iter().enumerate() {
        require_caps.push(CapsKey::parse(s).ok_or_else(|| {
            BuildError::new(
                context,
                &format!("{path}.require_caps[{i}]"),
                format!("未知能力: {s}"),
            )
        })?);
    }
    let mut skip = Vec::new();
    for (i, s) in c.skip.into_iter().enumerate() {
        let field = SkipField::parse(&s.field).ok_or_else(|| {
            BuildError::new(
                context,
                &format!("{path}.skip[{i}].field"),
                format!("未知字段: {}", s.field),
            )
        })?;
        let op = SkipOp::parse(&s.op).ok_or_else(|| {
            BuildError::new(
                context,
                &format!("{path}.skip[{i}].op"),
                format!("未知比较符: {}", s.op),
            )
        })?;
        skip.push(SkipRule {
            field,
            op,
            value: s.value,
        });
    }
    let limits = c.limits.map(|l| Limits {
        low: l.low,
        warn: l.warn,
        high: l.high,
    });
    let mut states = BTreeMap::new();
    if let Some(map) = c.states {
        for (k, v) in map {
            let key = k.parse::<i64>().map_err(|_| {
                BuildError::new(context, &format!("{path}.states.{k}"), "状态键必须为整数")
            })?;
            states.insert(key, v);
        }
    }

    let kind = if let Some(name) = &c.custom {
        CandidateKind::Custom(name.clone())
    } else {
        match c.kind.as_deref() {
            None => {
                let mut columns = c.column.into_iter();
                let first = columns.next();
                if columns.next().is_some() {
                    return Err(BuildError::new(
                        context,
                        &path,
                        "值列候选需要恰好 1 个 column",
                    ));
                }
                match first {
                    Some(col) => CandidateKind::Columns(Box::new(build_column(
                        context,
                        &format!("{path}.column[0]"),
                        col,
                    )?)),
                    None => {
                        return Err(BuildError::new(context, &path, "值列候选缺少 column"));
                    }
                }
            }
            Some("pool") => {
                let mut fields = build_fields(context, &path, c.field)?;
                CandidateKind::Pool(PoolFields {
                    name: take_role(context, &mut fields, "name", &path, false)?,
                    used: take_role(context, &mut fields, "used", &path, false)?,
                    free: take_role(context, &mut fields, "free", &path, false)?,
                    total: take_role(context, &mut fields, "total", &path, false)?,
                })
            }
            Some("entity") => {
                let mut fields = build_fields(context, &path, c.field)?;
                let usage = take_role(context, &mut fields, "usage", &path, true)?;
                CandidateKind::Entity(EntityFields {
                    usage: usage.unwrap_or_else(placeholder_column),
                    size_mega: take_role(context, &mut fields, "size_mega", &path, false)?,
                    size_bytes: take_role(context, &mut fields, "size_bytes", &path, false)?,
                })
            }
            Some("percent") => {
                let mut fields = build_fields(context, &path, c.field)?;
                let percent = take_role(context, &mut fields, "percent", &path, true)?;
                CandidateKind::Percent(PercentFields {
                    percent: percent.unwrap_or_else(placeholder_column),
                    total: take_role(context, &mut fields, "total", &path, false)?,
                })
            }
            Some("storage") => CandidateKind::HrStorage {
                storage_type: StorageType::parse(c.hr_type.as_deref().unwrap_or("ram"))
                    .ok_or_else(|| {
                        BuildError::new(
                            context,
                            &format!("{path}.hr_type"),
                            "行类型须为 ram|fixed_disk",
                        )
                    })?,
            },
            Some("entity_sensors") => CandidateKind::EntitySensors,
            Some("counters") => CandidateKind::UcdCpuCounters,
            Some("device") => {
                let mut fields = build_fields(context, &path, c.field)?;
                let take_vec =
                    |role: &str, fields: &mut Vec<(String, Column)>| -> Vec<Column> {
                        let mut out = Vec::new();
                        let mut i = 0;
                        while i < fields.len() {
                            if fields[i].0 == role {
                                out.push(fields.remove(i).1);
                            } else {
                                i += 1;
                            }
                        }
                        out
                    };
                let model = take_vec("model", &mut fields);
                let serial = take_vec("serial", &mut fields);
                let version = take_vec("version", &mut fields);
                if model.is_empty() && serial.is_empty() && version.is_empty() {
                    return Err(BuildError::new(
                        context,
                        &path,
                        "device 候选至少声明 model/serial/version 之一",
                    ));
                }
                CandidateKind::Device(DeviceFields {
                    model,
                    serial,
                    version,
                })
            }
            Some(other) => {
                return Err(BuildError::new(
                    context,
                    &format!("{path}.kind"),
                    format!("未知候选形态: {other}"),
                ));
            }
        }
    };
    let sensor = match c.sensor.as_deref() {
        None => None,
        Some(s) => Some(SensorKind::parse(s).ok_or_else(|| {
            BuildError::new(
                context,
                &format!("{path}.sensor"),
                format!("未知传感器类型: {s}"),
            )
        })?),
    };
    if matches!(kind, CandidateKind::Columns(_)) && family == Family::Sensors && sensor.is_none() {
        return Err(BuildError::new(
            context,
            &format!("{path}.sensor"),
            "传感器列候选必须声明 sensor 类型",
        ));
    }

    Ok(Candidate {
        require_caps,
        require_prev: c.require_prev,
        skip,
        limits,
        states,
        sensor,
        kind,
    })
}

/// take_role(required=true) 失败已提前返回错误后的占位列（不会被使用）；
/// 不用 unreachable!（clippy 禁用），空 Oid 无副作用。
fn placeholder_column() -> Column {
    Column {
        oid: Oid::new([0]),
        indexed: false,
        value: None,
        unit: None,
        precision: None,
        divisor: 1,
        multiplier: 1,
    }
}

pub(super) fn build_family(
    context: &str,
    family: Family,
    f: FamilyToml,
) -> BuildResult<FamilyPlan> {
    let mode = match f.mode.as_deref() {
        None => CollectMode::Inherit,
        Some("inherit") => CollectMode::Inherit,
        Some("replace") => CollectMode::Replace,
        Some("disable") => CollectMode::Disable,
        Some(other) => {
            return Err(BuildError::new(
                context,
                &format!("collect.{}.mode", family.key()),
                format!("未知模式: {other}"),
            ));
        }
    };
    let pick = match f.pick.as_deref() {
        None => Pick::Max,
        Some("max") => Pick::Max,
        Some("first") => Pick::First,
        Some("sum") => Pick::Sum,
        Some(other) => {
            return Err(BuildError::new(
                context,
                &format!("collect.{}.pick", family.key()),
                format!("未知行聚合: {other}"),
            ));
        }
    };
    let mut candidates = Vec::new();
    for c in f.candidate {
        candidates.push(build_candidate(context, family, c)?);
    }
    Ok(FamilyPlan {
        mode,
        pick,
        candidates,
    })
}

pub(super) fn build_collect(context: &str, c: CollectToml) -> BuildResult<CollectPlan> {
    let mut plan = CollectPlan::default();
    let entries = [
        (Family::Cpu, c.cpu),
        (Family::Mem, c.mem),
        (Family::Temp, c.temp),
        (Family::Storage, c.storage),
        (Family::Sensors, c.sensors),
        (Family::Device, c.device),
    ];
    for (family, toml) in entries {
        if let Some(f) = toml {
            let built = build_family(context, family, f)?;
            *plan.family_mut(family) = Some(built);
        }
    }
    Ok(plan)
}

pub(super) fn build_profile(context: &str, p: ProfileToml) -> BuildResult<OsProfile> {
    let kind = match p.kind.as_str() {
        "group" => ProfileKind::Group,
        "os" => ProfileKind::Os,
        other => {
            return Err(BuildError::new(
                context,
                "kind",
                format!("未知类别: {other}"),
            ));
        }
    };
    let matcher = match p.match_ {
        None => None,
        Some(m) => Some(build_matcher(context, m)?),
    };
    let collect = match p.collect {
        None => CollectPlan::default(),
        Some(c) => build_collect(context, c)?,
    };
    let display_name = p.display_name.clone().unwrap_or_else(|| p.id.clone());
    Ok(OsProfile {
        id: p.id,
        kind,
        group: p.group,
        brand: p.brand.unwrap_or_default(),
        display_name,
        matcher,
        collect,
        source: context.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_列候选解析() {
        let text = r#"
[[profile]]
id = "t"
kind = "os"
brand = "T"
[profile.match]
sysobjectid = ["1.3.6.1.4.1.9"]
descr_keywords = ["t"]
[profile.collect.cpu]
pick = "max"
[[profile.collect.cpu.candidate]]
[[profile.collect.cpu.candidate.column]]
oid = "1.3.6.1.4.1.9.9.1"
value = "percent"
precision = -1
"#;
        let file: ProfilesFile = toml::from_str(text).unwrap();
        let profile = build_profile("test", file.profile.into_iter().next().unwrap()).unwrap();
        let cpu = profile.collect.cpu.unwrap();
        assert_eq!(cpu.pick, Pick::Max);
        assert_eq!(cpu.candidates.len(), 1);
        assert!(matches!(cpu.candidates[0].kind, CandidateKind::Columns(_)));
    }

    #[test]
    fn schema_非法oid报错带路径() {
        let text = r#"
[[profile]]
id = "t"
kind = "os"
brand = "T"
[[profile.collect.cpu.candidate]]
[[profile.collect.cpu.candidate.column]]
oid = "1.x.3"
"#;
        let file: ProfilesFile = toml::from_str(text).unwrap();
        let err = build_profile("test", file.profile.into_iter().next().unwrap()).unwrap_err();
        assert!(
            err.0.contains("collect.cpu.candidate.column[0].oid"),
            "{}",
            err.0
        );
    }

    #[test]
    fn schema_正则编译失败报错() {
        let text = r#"
[[profile]]
id = "t"
kind = "os"
brand = "T"
[profile.match]
descr_regex = ["([a"]
"#;
        let file: ProfilesFile = toml::from_str(text).unwrap();
        let err = build_profile("test", file.profile.into_iter().next().unwrap()).unwrap_err();
        assert!(err.0.contains("descr_regex"), "{}", err.0);
    }
}
