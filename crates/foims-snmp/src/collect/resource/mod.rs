//! 资源采集解释器（docs/SNMP模块重构方案.md §5.6）：唯一消费
//! ResolvedCollectPlan 的地方——候选按声明顺序尝试，首个 Success 生效，
//! 空结果继续下一条，Error 中止该族，抽象不可用跳过。
//!
//! 两条调用路径共用本解释器：
//! - 秒级实时（live/collector.rs）：探测期一次全量发现（walk），候选与行索引
//!   缓存进 [`IndexCache`]，此后每轮按缓存定向 GET；缓存失效自动重新发现；
//! - 5 分钟趋势（foims-agent-service snmp_poll）：每轮全新缓存，一遍走完。
//!
//! 候选结果五分类（§5.6）：Success / Empty（walk 无数据，试下一候选）/
//! Invalid（数值无效，试下一候选）/ Unsupported（OID 不支持，试下一候选）/
//! Error（超时/不可达，中止该族——防止一次失败把所有候选探一遍）。

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use async_snmp::{Oid, Value, VarBind};

use crate::core::SnmpReader;
use crate::vendor::PlatformCaps;
use crate::vendor::model::{Candidate, Family, Pick, SensorKind};
use crate::vendor::registry::ResolvedCollectPlan;

pub mod cpu;
pub mod device;
pub mod mem;
pub mod sensors;
pub mod storage;
pub mod temp;
pub mod transform;

// ───────────────────────── 读数输出 ─────────────────────────

/// CPU 读数。
#[derive(Debug, Clone)]
pub struct CpuReading {
    pub index: String,
    pub usage_pct: f64,
}

/// 内存/存储读数（字节口径）。
#[derive(Debug, Clone, Default)]
pub struct MemReading {
    pub index: String,
    pub name: Option<String>,
    pub used: Option<u64>,
    pub free: Option<u64>,
    pub total: Option<u64>,
    pub usage_percent: Option<f64>,
}

/// 温度读数（摄氏度）。
#[derive(Debug, Clone)]
pub struct TempReading {
    pub index: String,
    pub name: Option<String>,
    pub celsius: f64,
}

/// 传感器读数。
#[derive(Debug, Clone)]
pub struct SensorReading {
    pub index: String,
    pub kind: SensorKind,
    pub name: Option<String>,
    pub value: f64,
}

/// 设备信息读数（型号/序列号/版本字符串）。
#[derive(Debug, Clone, Default)]
pub struct DeviceReading {
    pub model: Option<String>,
    pub serial: Option<String>,
    pub version: Option<String>,
}

// ───────────────────────── 缓存 ─────────────────────────

/// 行级缓存元数据：定向 GET 后仍需要的换算信息。
#[derive(Debug, Clone)]
pub(super) enum RowMeta {
    /// 仅行索引
    Plain,
    /// 该行总量（字节；实体/百分比候选的 size 列缓存）
    Total(u64),
    /// 该行分配单元（hrStorage units 列缓存）
    Units(u64),
    /// 该行传感器标定（ENTITY-SENSOR scale/precision 缓存）
    Sensor { scale: i32, precision: i32 },
}

/// 单族已选候选：候选下标 + 行索引 → 元数据。
#[derive(Debug, Clone)]
pub(super) struct FamilySelection {
    pub candidate: usize,
    pub rows: BTreeMap<u32, RowMeta>,
}

/// 探测期缓存（候选选择 + 行索引），实时路径跨轮复用、趋势路径单轮内复用。
/// 内部可变：所有采集走 &self。
#[derive(Debug, Default)]
pub struct IndexCache {
    families: Mutex<BTreeMap<Family, FamilySelection>>,
    /// CPU 计数器差值候选的写回状态（下一轮 diff 的基准，仅趋势路径消费）。
    cpu_counters: Mutex<Option<cpu::CpuCounters>>,
}

impl IndexCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock_families(&self) -> MutexGuard<'_, BTreeMap<Family, FamilySelection>> {
        self.families.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn get(&self, family: Family) -> Option<FamilySelection> {
        self.lock_families().get(&family).cloned()
    }

    pub(super) fn put(&self, family: Family, selection: FamilySelection) {
        self.lock_families().insert(family, selection);
    }

    pub(super) fn drop_family(&self, family: Family) {
        self.lock_families().remove(&family);
    }

    /// 取出并清空 CPU 计数器写回状态（趋势路径每轮消费）。
    pub fn take_cpu_counters(&self) -> Option<cpu::CpuCounters> {
        let mut guard = self
            .cpu_counters
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        guard.take()
    }

    pub(super) fn set_cpu_counters(&self, counters: cpu::CpuCounters) {
        *self
            .cpu_counters
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(counters);
    }
}

// ───────────────────────── 解释器入口 ─────────────────────────

/// 解释器入口：绑定 reader、解析后计划与运行上下文。
/// `prev_cpu` 为上一轮 UCD 计数器（趋势路径传入；实时路径 None，
/// require_prev 候选自动跳过）。
pub struct ResourceReader<'a, R: SnmpReader> {
    reader: &'a R,
    plan: &'a ResolvedCollectPlan,
    caps: PlatformCaps,
    prev_cpu: Option<&'a cpu::CpuCounters>,
    cache: &'a IndexCache,
}

impl<'a, R: SnmpReader> ResourceReader<'a, R> {
    pub fn new(
        reader: &'a R,
        plan: &'a ResolvedCollectPlan,
        caps: PlatformCaps,
        prev_cpu: Option<&'a cpu::CpuCounters>,
        cache: &'a IndexCache,
    ) -> Self {
        Self {
            reader,
            plan,
            caps,
            prev_cpu,
            cache,
        }
    }

    /// 组装候选执行入参。
    fn args<'b>(
        &'b self,
        candidate: &'b Candidate,
        selection: Option<&'b FamilySelection>,
    ) -> ExecArgs<'a, 'b, R> {
        ExecArgs {
            reader: self.reader,
            candidate,
            selection,
            prev_cpu: self.prev_cpu,
            cache: Some(self.cache),
        }
    }

    /// CPU 读数（族 pick 已聚合）。
    pub async fn cpu(&self) -> Vec<CpuReading> {
        let resolved = self.plan.family(Family::Cpu);
        for (idx, candidate) in resolved.candidates.iter().enumerate() {
            if self.gated(candidate) {
                continue;
            }
            let cached = self.cached(idx, Family::Cpu);
            match cpu::exec(self.args(candidate, cached.as_ref())).await {
                Ok((rows, cached_rows)) => {
                    self.store(Family::Cpu, idx, cached_rows);
                    return aggregate(resolved.pick, rows, |r| r.usage_pct);
                }
                Err(failure) => {
                    if !self.on_failure(Family::Cpu, idx, cached.is_some(), failure) {
                        return Vec::new();
                    }
                }
            }
        }
        Vec::new()
    }

    /// 内存读数（首个成功候选；多行读数（实体多 DIMM）按使用率最高保留为主读数）。
    pub async fn mem(&self) -> Option<MemReading> {
        let resolved = self.plan.family(Family::Mem);
        for (idx, candidate) in resolved.candidates.iter().enumerate() {
            if self.gated(candidate) {
                continue;
            }
            let cached = self.cached(idx, Family::Mem);
            match mem::exec(self.args(candidate, cached.as_ref())).await {
                Ok((mut readings, cached_rows)) => {
                    self.store(Family::Mem, idx, cached_rows);
                    readings.sort_by(|a, b| {
                        b.usage_percent
                            .unwrap_or(f64::NEG_INFINITY)
                            .total_cmp(&a.usage_percent.unwrap_or(f64::NEG_INFINITY))
                    });
                    return readings.into_iter().next();
                }
                Err(failure) => {
                    if !self.on_failure(Family::Mem, idx, cached.is_some(), failure) {
                        return None;
                    }
                }
            }
        }
        None
    }

    /// 温度读数（全部有效行；族 pick=max 时取最热一行）。
    pub async fn temp(&self) -> Vec<TempReading> {
        let resolved = self.plan.family(Family::Temp);
        for (idx, candidate) in resolved.candidates.iter().enumerate() {
            if self.gated(candidate) {
                continue;
            }
            let cached = self.cached(idx, Family::Temp);
            match temp::exec(self.args(candidate, cached.as_ref())).await {
                Ok((rows, cached_rows)) => {
                    self.store(Family::Temp, idx, cached_rows);
                    return aggregate(resolved.pick, rows, |r| r.celsius);
                }
                Err(failure) => {
                    if !self.on_failure(Family::Temp, idx, cached.is_some(), failure) {
                        return Vec::new();
                    }
                }
            }
        }
        Vec::new()
    }

    /// 存储读数（全部有效磁盘行）。
    pub async fn storage(&self) -> Vec<MemReading> {
        let resolved = self.plan.family(Family::Storage);
        for (idx, candidate) in resolved.candidates.iter().enumerate() {
            if self.gated(candidate) {
                continue;
            }
            let cached = self.cached(idx, Family::Storage);
            match storage::exec(self.args(candidate, cached.as_ref())).await {
                Ok((rows, cached_rows)) => {
                    self.store(Family::Storage, idx, cached_rows);
                    return rows;
                }
                Err(failure) => {
                    if !self.on_failure(Family::Storage, idx, cached.is_some(), failure) {
                        return Vec::new();
                    }
                }
            }
        }
        Vec::new()
    }

    /// 传感器读数（全部有效行）。
    pub async fn sensors(&self) -> Vec<SensorReading> {
        let resolved = self.plan.family(Family::Sensors);
        for (idx, candidate) in resolved.candidates.iter().enumerate() {
            if self.gated(candidate) {
                continue;
            }
            let cached = self.cached(idx, Family::Sensors);
            match sensors::exec(self.args(candidate, cached.as_ref())).await {
                Ok((rows, cached_rows)) => {
                    self.store(Family::Sensors, idx, cached_rows);
                    return rows;
                }
                Err(failure) => {
                    if !self.on_failure(Family::Sensors, idx, cached.is_some(), failure) {
                        return Vec::new();
                    }
                }
            }
        }
        Vec::new()
    }

    /// 设备信息读数（型号/序列号/版本；无缓存——三项各一个标量 GET）。
    pub async fn device(&self) -> Vec<DeviceReading> {
        let resolved = self.plan.family(Family::Device);
        for (idx, candidate) in resolved.candidates.iter().enumerate() {
            if self.gated(candidate) {
                continue;
            }
            match device::exec(self.args(candidate, None)).await {
                Ok((rows, _)) => {
                    if rows.is_empty() {
                        continue;
                    }
                    return rows;
                }
                Err(failure) => {
                    if !self.on_failure(Family::Device, idx, false, failure) {
                        return Vec::new();
                    }
                }
            }
        }
        Vec::new()
    }

    // ── 驱动辅助 ──

    /// 门控：能力缺失（已知不可用，跳过而非空走）或需要上一轮状态而实时
    /// 路径没有。
    fn gated(&self, candidate: &Candidate) -> bool {
        candidate
            .require_caps
            .iter()
            .any(|cap| !self.caps.has(*cap))
            || (candidate.require_prev && self.prev_cpu.is_none())
    }

    fn cached(&self, idx: usize, family: Family) -> Option<FamilySelection> {
        self.cache.get(family).filter(|sel| sel.candidate == idx)
    }

    /// 缓存写回：仅发现模式产出行缓存的候选入缓存（标量/计数器候选无需缓存，
    /// 空选择入缓存会让下轮误走定向路径）。
    fn store(&self, family: Family, idx: usize, rows: Option<SelectionRows>) {
        if let Some(rows) = rows {
            self.cache.put(
                family,
                FamilySelection {
                    candidate: idx,
                    rows,
                },
            );
        }
    }

    /// 失败处置：返回 true 表示继续试下一候选。Error（超时/不可达）中止
    /// 该族，防止一次故障把全部候选探一遍（§5.6）。
    fn on_failure(
        &self,
        family: Family,
        idx: usize,
        was_cached: bool,
        failure: CandidateFailure,
    ) -> bool {
        match failure {
            CandidateFailure::Error(msg) => {
                tracing::warn!(
                    "SNMP 采集 {} 候选 {idx} 失败（{msg}），本族中止",
                    family.key()
                );
                false
            }
            CandidateFailure::Empty => {
                if was_cached {
                    // 缓存行全空：索引可能已变，丢弃缓存（本候选重发现/下候选接管）
                    self.cache.drop_family(family);
                }
                true
            }
            CandidateFailure::Invalid | CandidateFailure::Unsupported => {
                tracing::debug!(
                    "SNMP 采集 {} 候选 {idx} 无效/不支持，尝试下一候选",
                    family.key()
                );
                true
            }
        }
    }
}

/// pick 行聚合（§5.6）：多索引行 → 单读数（Max/First；Sum 仅对可加读数有意义，
/// CPU/温度族退化按 Max）。
fn aggregate<T>(pick: Pick, rows: Vec<T>, score: impl Fn(&T) -> f64) -> Vec<T> {
    match pick {
        Pick::Max | Pick::Sum => rows
            .into_iter()
            .max_by(|a, b| score(a).total_cmp(&score(b)))
            .into_iter()
            .collect(),
        Pick::First => rows.into_iter().take(1).collect(),
    }
}

// ───────────────────────── 执行器公共协议 ─────────────────────────

/// 候选失败分类（§5.6）。
#[derive(Debug)]
pub(super) enum CandidateFailure {
    /// walk 无数据 / 候选行全空
    Empty,
    /// 数值全部无效（变换后过滤掉）
    Invalid,
    /// OID 无响应（设备不支持该 MIB）
    Unsupported,
    /// 超时/网络/协议错误——中止该族
    Error(String),
}

/// 行缓存元数据表（发现模式的产物；None = 该候选无需缓存）。
pub(super) type SelectionRows = BTreeMap<u32, RowMeta>;

/// 执行器输出：读数 + 可选行缓存元数据。FamilySelection（含候选下标）
/// 由驱动层组装。
pub(super) type ExecOutput<T> = Result<(Vec<T>, Option<SelectionRows>), CandidateFailure>;

/// 执行器入参：reader + 候选 + 缓存选择（None = 发现模式，Some = 定向采样）。
pub(super) struct ExecArgs<'r, 'c, R: SnmpReader> {
    pub reader: &'r R,
    pub candidate: &'c Candidate,
    /// Some → 按缓存行定向 GET；None → 全量发现
    pub selection: Option<&'c FamilySelection>,
    /// 上一轮 UCD 计数器（counters 候选用）
    pub prev_cpu: Option<&'r cpu::CpuCounters>,
    /// 缓存句柄（counters 候选写回新计数器）
    pub cache: Option<&'r IndexCache>,
}

/// 响应值过滤：异常值（noSuchObject/noSuchInstance 等）视为无数据。
pub(super) fn varbind_value(vb: &VarBind) -> Option<Value> {
    match &vb.value {
        Value::NoSuchObject | Value::NoSuchInstance => None,
        value if value.is_exception() => None,
        _ => Some(vb.value.clone()),
    }
}

/// 响应值文本（探测比对 / 行名 / 状态映射用）。
pub(super) fn varbind_text(vb: &VarBind) -> Option<String> {
    match &vb.value {
        Value::OctetString(s) => Some(String::from_utf8_lossy(s).trim().to_string()),
        Value::Integer(i) => Some(i.to_string()),
        Value::Counter32(v) | Value::Gauge32(v) | Value::UInteger32(v) | Value::TimeTicks(v) => {
            Some(v.to_string())
        }
        Value::Counter64(v) => Some(v.to_string()),
        Value::ObjectIdentifier(o) => Some(o.to_string()),
        Value::IpAddress(v) => Some(v.iter().map(u8::to_string).collect::<Vec<_>>().join(".")),
        _ => None,
    }
}

/// 数值提取：有符号（温度可能为负）优先，其次无符号计数。
pub(super) fn numeric(value: &Value) -> Option<f64> {
    value
        .as_i32()
        .map(f64::from)
        .or_else(|| value.as_u64().map(|v| v as f64))
}

/// 无符号提取（计数器/总量列）。
pub(super) fn numeric_u64(value: &Value) -> Option<u64> {
    value.as_u64()
}

// ───────────────────────── 共享 IO 原语 ─────────────────────────

/// 单弧索引 walk：base 为列基 OID，返回「索引 → 响应值」。
pub(super) async fn walk_single_arc<R: SnmpReader>(reader: &R, base: &Oid) -> BTreeMap<u32, Value> {
    let mut rows = BTreeMap::new();
    for vb in reader.walk(base).await {
        let Some(value) = varbind_value(&vb) else {
            continue;
        };
        let Some(index) = index_after(&vb.oid, base) else {
            continue;
        };
        rows.insert(index, value);
    }
    rows
}

/// 标量 GET（OID 已含实例后缀）。
pub(super) async fn get_one<R: SnmpReader>(reader: &R, oid: &Oid) -> Option<Value> {
    let binds = reader.get_many(std::slice::from_ref(oid)).await.ok()?;
    binds.first().and_then(varbind_value)
}

/// 标量 GET 的文本值。
pub(super) async fn get_one_text<R: SnmpReader>(reader: &R, oid: &Oid) -> Option<String> {
    let binds = reader.get_many(std::slice::from_ref(oid)).await.ok()?;
    binds.first().and_then(varbind_text)
}

/// base + 索引 → 实例 OID。
pub(super) fn child_oid(base: &Oid, index: u32) -> Oid {
    let mut arcs = base.arcs().to_vec();
    arcs.push(index);
    Oid::new(arcs)
}

/// 响应 OID 相对 base 的索引弧。
pub(super) fn index_after(response: &Oid, base: &Oid) -> Option<u32> {
    let base_arcs = base.arcs();
    let arcs = response.arcs();
    if arcs.len() == base_arcs.len() + 1 && arcs.starts_with(base_arcs) {
        Some(arcs[base_arcs.len()])
    } else {
        None
    }
}

/// 批量定向 GET：列 OID × 行索引 → 实例值表（键为 (列下标, 行索引)）。
/// 传输失败返回候选 Error。
pub(super) async fn get_rows<R: SnmpReader>(
    reader: &R,
    columns: &[Oid],
    indexes: impl Iterator<Item = u32>,
) -> Result<BTreeMap<(usize, u32), Value>, CandidateFailure> {
    let mut oids = Vec::new();
    let mut keys = Vec::new();
    for index in indexes {
        for (ci, base) in columns.iter().enumerate() {
            oids.push(child_oid(base, index));
            keys.push((ci, index));
        }
    }
    let binds = reader
        .get_many(&oids)
        .await
        .map_err(CandidateFailure::Error)?;
    let mut out = BTreeMap::new();
    for (key, vb) in keys.into_iter().zip(binds) {
        if let Some(value) = varbind_value(&vb) {
            out.insert(key, value);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick聚合_max_first() {
        let rows = vec![1.0, 5.0, 3.0];
        let got = aggregate(Pick::Max, rows.clone(), |v| *v);
        assert_eq!(got, vec![5.0]);
        let got = aggregate(Pick::First, rows.clone(), |v| *v);
        assert_eq!(got, vec![1.0]);
    }

    #[test]
    fn index提取() {
        let base = Oid::new([1, 3, 6, 1, 2, 1, 25, 3, 3, 1, 2]);
        let resp = Oid::new([1, 3, 6, 1, 2, 1, 25, 3, 3, 1, 2, 768]);
        assert_eq!(index_after(&resp, &base), Some(768));
        assert_eq!(index_after(&base, &base), None);
    }
}
