//! SNMP 表列 walk 统一实现：GETBULK 降级 GETNEXT 与单弧索引列映射。
//!
//! 全域采集（端口/VLAN/LLDP/实体/聚合/POE/CDP）共用同一份 walk 策略，
//! 避免各表各自实现重复的降级与异常值过滤逻辑。

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_snmp::{Client, Value, VarBind, Version, WalkMethod, WalkOptions};
use tracing::warn;

use foims_common::log_warn;
use foims_common::msg;

use crate::core::{SnmpError, format_snmp_error, wrap_snmp_error};

/// GETBULK 单次重复数，与 net-snmp snmpwalk 默认值一致。
///
/// 部分设备（实测华为 VRP）在长文本列（lldpRemTable 等）上对大重复数的
/// GETBULK 直接丢包（既无响应也无 tooBig），客户端按 5s×4 次重传后以
/// walk_failed 结束；而 10 重复的单响应体量可正常返回。
pub(crate) const WALK_MAX_REPETITIONS: u32 = 10;

/// GETBULK 降级重试的单次重复数：首批（[`WALK_MAX_REPETITIONS`]）被设备
/// 静默丢包时先行小重复数重试。实测华为 CE12800 对 Q-BRIDGE 位图列
///（dot1qVlanStaticEgressPorts/UntaggedPorts，每行约 72 字节位图）与
/// lldpRemTable 在 10 重复下整包无响应、2 重复秒回，且小重复数 GETBULK
/// 仍显著快于 GETNEXT 逐条遍历（252 行 LLDP 约 2s）。
pub(crate) const WALK_DEGRADED_REPETITIONS: u32 = 2;

/// 单列 walk 总体耗时上限（覆盖 GETBULK 含重试约 35s 最坏 + GETNEXT 降级
/// 全程）。健康设备 walk 数秒内完成，预算仅约束故障设备（如对两种遍历
/// 方式均不响应的机型），避免同步采集接口被无界耗时拖到网关超时墙之外
///（nginx /api/ 通用读超时 60s，采集类端点单独放宽到 5m）。
pub(crate) const WALK_BUDGET_SECS: u64 = 120;

/// 降级链单个 GETBULK 阶段的耗时上限。
///
/// 部分设备（实测华为 CE12800）对大重复数请求「慢而不死」——探针单请求
/// 能回包，整段 walk 却每请求重试 1-2 次才推进（lldp 五表并发实测复现），
/// 阶段既不报错也走不完。阶段级上限保证此类阶段被及时放弃并推进到下一级；
/// 阶段被提前放弃只损失 Cr10 档的效率，数据正确性由下一级保证。取值需
/// 覆盖健康大表在轻度丢包下的耗时（几十个请求 + 少量重传），20s 实测
/// 足够；总预算扣除两级阶段预算后仍给 GETNEXT 兜底留出充足时间。
const WALK_STAGE_BUDGET_SECS: u64 = 20;

/// 降级记忆 TTL：某 OID 在小重复数档采集成功后记忆该观测，TTL 内同设备
/// 同 OID 的采集直接从小重复数档开始（跳过探针与必然失败的整段 walk）。
/// 过期后回到完整探针链（设备可能已恢复或为瞬时拥塞）。
const WALK_MEMORY_TTL_SECS: u64 = 1800;

/// 降级记忆表：键为「设备地址|OID」（见 [`walk_memory_key`]），值为最近
/// 一次小重复数档采集成功的时刻。仅存于进程内，重启后冷采一次重新观测。
static WALK_DEGRADED_MEMORY: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 单个 GETBULK 阶段内单请求无响应的中止阈值。
///
/// 实测华为 CE12800 在 lldpRemTable 上对 Cr10「慢而不死」——每请求重传 1-2 次
/// 才推进，阶段既不报错也走不完，整段 walk 磨满阶段预算（20s）才降级，冷采
/// LLDP 达 23s。而小重复数档「秒回」：单请求超过此阈值仍无任何响应（含重传
/// 等待），设备大概率处于该档不可用状态，立即中止阶段并降级，不再白等剩余
/// 预算。取值同 [`WALK_PROBE_TIMEOUT_SECS`] 口径：略大于单请求超时（默认 5s，
/// 丢一个包后响应落在重传间隔内仍能等到），小于重传链首个重试间隔结束点。
/// 误中止无正确性风险：降级 Cr2 档数据等价，仅损失 Cr10 档效率。
const WALK_STALL_SECS: u64 = 6;

/// GETBULK 能力探针超时：整段 walk 前先用单请求 GETBULK 探测设备对大重复
/// 数的响应能力，探针超过此上限即判定该重复数不可用，直接进入下一级，
/// 避免整段 walk 白等重传链（约 35s，实测主机命令秒回而采集 30s+ 的主因）。
/// 取值须略大于单请求超时（默认 5s）、小于重传链首个重试间隔结束点。
const WALK_PROBE_TIMEOUT_SECS: u64 = 6;

/// [`walk_varbinds`] 失败类型：底层 SNMP 错误或超出总预算。
#[derive(Debug)]
pub(crate) enum WalkError {
    /// 客户端底层错误（请求超时/网络/协议/认证等）
    Snmp(Box<async_snmp::Error>),
    /// 超出 [`WALK_BUDGET_SECS`] 总预算
    BudgetExceeded,
}

impl WalkError {
    /// 归一为 SnmpError：底层错误走 walk_failed 文案，预算超限按超时文案
    /// 上报（前端语义一致：请求超时而非失败）。
    pub(crate) fn into_snmp_error(self) -> SnmpError {
        match self {
            WalkError::Snmp(e) => wrap_snmp_error("server.snmp.error.walk_failed", e),
            WalkError::BudgetExceeded => SnmpError::Message(msg("server.snmp.error.timeout")),
        }
    }

    /// 告警日志用的底层错误详情串。
    pub(crate) fn log_detail(self) -> String {
        match self {
            WalkError::Snmp(e) => format_snmp_error(e).log_string(),
            WalkError::BudgetExceeded => msg("server.snmp.error.timeout")
                .with("detail", "超过 walk 总预算")
                .log_string(),
        }
    }
}

/// walk 单个 OID：若降级记忆命中（[`WALK_DEGRADED_MEMORY`]，TTL 内该列
/// 已确认 Cr10 档不可用）直接从小重复数档采集；否则先发单请求 GETBULK
/// 探针（[`WALK_PROBE_TIMEOUT_SECS`]）探测设备对 [`WALK_MAX_REPETITIONS`]
/// 的响应能力——探针通过才执行整段 walk（受 [`WALK_STAGE_BUDGET_SECS`]
/// 阶段预算与 [`WALK_STALL_SECS`] 单请求 stall 中止约束），探针无响应直接降
/// [`WALK_DEGRADED_REPETITIONS`] 采集，不再白等必然失败的整段重传链。小重复
/// 数成功写入降级记忆，仍失败降级 GETNEXT 逐条遍历并告警；认证/配置类错误
/// 直接冒泡（重试无意义且会翻倍失败时延）。v1 无 GETBULK（WalkOptions 默认
/// 即 GETNEXT），跳过探针。
///
/// 全程受 [`WALK_BUDGET_SECS`] 总预算约束，超预算终止并返回
/// [`WalkError::BudgetExceeded`]；降级重试不会留下半张表的中间结果
///（超预算截断除外——截断结果不可信，一律按失败上报，不返回部分数据）。
pub(crate) async fn walk_varbinds(
    client: &Client,
    target: &async_snmp::Oid,
) -> Result<Vec<VarBind>, WalkError> {
    let memory_key = walk_memory_key(client, target);
    let walk = async {
        // 降级记忆命中：该 OID 近期已确认 Cr10 档不可用，直接小重复数采集，
        // 跳过探针与必然失败的整段 walk（冷采实测 20s+，命中后秒级）
        if walk_memory_hit(&memory_key) {
            return match walk_bulk_stage(client, target, WALK_DEGRADED_REPETITIONS).await {
                BulkStage::Done(varbinds) => {
                    walk_memory_confirm(&memory_key);
                    Ok(varbinds)
                }
                BulkStage::Retry(detail) => {
                    warn!(
                        "SNMP walk 小重复数采集仍失败，降级 GETNEXT: oid={}, error={}",
                        target, detail
                    );
                    walk_memory_evict(&memory_key);
                    walk_getnext(client, target).await
                }
                BulkStage::Fatal(err) => Err(*err),
            };
        }
        let probe = if client.version() == Version::V1 {
            BulkProbe::Ok
        } else {
            probe_bulk(client, target, WALK_MAX_REPETITIONS).await
        };
        match probe {
            BulkProbe::Ok => {
                match walk_bulk_stage(client, target, WALK_MAX_REPETITIONS).await {
                    BulkStage::Done(varbinds) => {
                        // Cr10 档采集成功：清除该 OID 的降级记忆（若设备曾
                        // 拥塞现已恢复）
                        walk_memory_evict(&memory_key);
                        Ok(varbinds)
                    }
                    // 探针通过但整段 walk 中途失败/过慢（如部分行后设备
                    // 磨蹭）：降重复数重试
                    BulkStage::Retry(detail) => {
                        warn!(
                            "SNMP walk GETBULK 失败，降重复数重试: oid={}, error={}",
                            target, detail
                        );
                        walk_degraded(&memory_key, client, target).await
                    }
                    BulkStage::Fatal(err) => Err(*err),
                }
            }
            // 探针无响应：设备静默丢大重复数包，直接小重复数采集
            BulkProbe::Retry(detail) => {
                warn!(
                    "SNMP GETBULK-{WALK_MAX_REPETITIONS} 探针无响应，直接降重复数采集: oid={}, error={}",
                    target, detail
                );
                walk_degraded(&memory_key, client, target).await
            }
            BulkProbe::Fatal(err) => Err(*err),
        }
    };

    match tokio::time::timeout(Duration::from_secs(WALK_BUDGET_SECS), walk).await {
        Ok(result) => result.map_err(|e| WalkError::Snmp(Box::new(e))),
        Err(_) => Err(WalkError::BudgetExceeded),
    }
}

/// 降级记忆键：设备地址 + OID 唯一标识一个「设备对该列的 bulk 能力」观测。
fn walk_memory_key(client: &Client, target: &async_snmp::Oid) -> String {
    format!("{}|{target}", client.peer_addr())
}

/// 记忆命中判定：TTL 内有确认记录即命中（惰性清理过期项）。
fn walk_memory_hit(key: &str) -> bool {
    let mut map = WALK_DEGRADED_MEMORY
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    match map.get(key) {
        Some(confirmed_at)
            if confirmed_at.elapsed() <= Duration::from_secs(WALK_MEMORY_TTL_SECS) =>
        {
            true
        }
        Some(_) => {
            map.remove(key);
            false
        }
        None => false,
    }
}

/// 记录/刷新降级记忆。
fn walk_memory_confirm(key: &str) {
    WALK_DEGRADED_MEMORY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key.to_string(), Instant::now());
}

/// 清除降级记忆（Cr10 档恢复或该档位彻底不可用需重走探针链）。
fn walk_memory_evict(key: &str) {
    WALK_DEGRADED_MEMORY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(key);
}

/// 小重复数 GETBULK 采集：成功即写入降级记忆，仍失败降级 GETNEXT 兜底
///（受总预算约束）并清除记忆（下次重走完整探针链）。
async fn walk_degraded(
    memory_key: &str,
    client: &Client,
    target: &async_snmp::Oid,
) -> Result<Vec<VarBind>, async_snmp::Error> {
    match walk_bulk_stage(client, target, WALK_DEGRADED_REPETITIONS).await {
        BulkStage::Done(varbinds) => {
            walk_memory_confirm(memory_key);
            Ok(varbinds)
        }
        // 小重复数仍失败：设备 bulk 实现异常或单条响应即超限，
        // 降级 GETNEXT 逐条遍历
        BulkStage::Retry(detail) => {
            warn!(
                "SNMP walk 小重复数重试仍失败，降级 GETNEXT: oid={}, error={}",
                target, detail
            );
            walk_memory_evict(memory_key);
            walk_getnext(client, target).await
        }
        BulkStage::Fatal(err) => Err(*err),
    }
}

/// GETNEXT 逐条遍历兜底（受总预算约束；不启用单请求 stall 中止）。
async fn walk_getnext(
    client: &Client,
    target: &async_snmp::Oid,
) -> Result<Vec<VarBind>, async_snmp::Error> {
    walk_collect(
        client,
        target,
        WalkOptions {
            method: WalkMethod::GetNext,
            ..WalkOptions::default()
        },
        None,
    )
    .await
    .map_err(|e| *e)
}

/// GETBULK 能力探针结果。
enum BulkProbe {
    /// 探针有响应：设备可处理该重复数，可执行整段 walk
    Ok,
    /// 探针无响应或协议层失败（应降重复数），附告警详情
    Retry(String),
    /// 不可重试错误（认证/配置类），直接冒泡
    Fatal(Box<async_snmp::Error>),
}

/// 单请求 GETBULK 探测：与整段 walk 的首个请求完全同参（同列同重复数），
/// 响应体量一致，可准确预测设备是否会静默丢包。探针被中途取消仅废弃在途
/// 请求（async-snmp 请求注册随取消清理），不影响后续请求。
async fn probe_bulk(client: &Client, target: &async_snmp::Oid, max_repetitions: u32) -> BulkProbe {
    match tokio::time::timeout(
        Duration::from_secs(WALK_PROBE_TIMEOUT_SECS),
        client.get_bulk(std::slice::from_ref(target), 0, max_repetitions),
    )
    .await
    {
        Ok(Ok(_)) => BulkProbe::Ok,
        Ok(Err(err)) if is_bulk_retryable(&err) => {
            BulkProbe::Retry(format_snmp_error(err).log_string())
        }
        Ok(Err(err)) => BulkProbe::Fatal(err),
        Err(_) => BulkProbe::Retry(
            msg("server.snmp.error.timeout")
                .with(
                    "detail",
                    format!("GETBULK-{max_repetitions} 探针 {WALK_PROBE_TIMEOUT_SECS}s 内无响应"),
                )
                .log_string(),
        ),
    }
}

/// 单个 GETBULK 阶段的产出。
enum BulkStage {
    /// 阶段内收齐全表
    Done(Vec<VarBind>),
    /// 阶段超时或可重试错误，附告警日志详情，应推进到下一级
    Retry(String),
    /// 不可重试错误（认证/配置类），直接冒泡
    Fatal(Box<async_snmp::Error>),
}

/// 带阶段预算的单次 GETBULK 收集：阶段内每请求仍按客户端重试策略重传，
/// 阶段整体超 [`WALK_STAGE_BUDGET_SECS`] 未完成或单请求超 [`WALK_STALL_SECS`]
/// 无响应即放弃（返回可重试），避免设备「慢而不死」时把总预算耗在单级上。
async fn walk_bulk_stage(
    client: &Client,
    target: &async_snmp::Oid,
    max_repetitions: u32,
) -> BulkStage {
    let options = WalkOptions {
        max_repetitions,
        ..WalkOptions::default()
    };
    match tokio::time::timeout(
        Duration::from_secs(WALK_STAGE_BUDGET_SECS),
        walk_collect(client, target, options, Some(Duration::from_secs(WALK_STALL_SECS))),
    )
    .await
    {
        Ok(Ok(varbinds)) => BulkStage::Done(varbinds),
        Ok(Err(err)) if is_bulk_retryable(&err) => {
            BulkStage::Retry(format_snmp_error(err).log_string())
        }
        Ok(Err(err)) => BulkStage::Fatal(err),
        Err(_) => BulkStage::Retry(
            msg("server.snmp.error.timeout")
                .with(
                    "detail",
                    format!("GETBULK-{max_repetitions} 阶段超过 {WALK_STAGE_BUDGET_SECS}s 未完成"),
                )
                .log_string(),
        ),
    }
}

/// 是否值得改用 GETNEXT 重试：超时、协议错误、walk 中断与畸形响应都可能
/// 源于设备对大重复数 GETBULK 的实现缺陷；认证/配置/网络错误不在此列。
fn is_bulk_retryable(err: &async_snmp::Error) -> bool {
    matches!(
        err,
        async_snmp::Error::Timeout { .. }
            | async_snmp::Error::Snmp { .. }
            | async_snmp::Error::WalkAborted { .. }
            | async_snmp::Error::MalformedResponse { .. }
    )
}

async fn walk_collect(
    client: &Client,
    target: &async_snmp::Oid,
    options: WalkOptions,
    stall: Option<Duration>,
) -> Result<Vec<VarBind>, Box<async_snmp::Error>> {
    let mut walk = client.walk_with(target.clone(), options)?;

    let mut varbinds = Vec::new();

    loop {
        // 单请求 stall 中止：GETBULK 阶段内某请求超过阈值仍无响应（含重传
        // 等待）即合成超时错误返回，调用方降级小重复数档；合成 Timeout 属
        // [`is_bulk_retryable`] 可降级错误。取消请求仅废弃在途报文（async-snmp
        // 请求注册随 Future 取消清理）。GETNEXT 兜底不启用（None）——兜底档
        // 必须允许慢响应推进到底，否则降级链尽头变成必然失败。
        let next = if let Some(limit) = stall {
            match tokio::time::timeout(limit, walk.next()).await {
                Ok(outcome) => outcome,
                Err(_) => {
                    return Err(Box::new(async_snmp::Error::Timeout {
                        target: client.peer_addr(),
                        elapsed: limit,
                        retries: 0,
                    }));
                }
            }
        } else {
            walk.next().await
        };

        let vb = match next {
            Some(item) => item?,
            None => break,
        };

        if vb.value.is_exception() {
            if matches!(vb.value, async_snmp::Value::EndOfMibView) {
                break;
            }
            continue;
        }

        varbinds.push(vb);
    }

    Ok(varbinds)
}

/// walk 单弧索引标量表并聚合为（实例尾弧 → 值）映射。
///
/// 适用于 ifTable（ifIndex）、BRIDGE-MIB（dot1dBasePort）、
/// Q-BRIDGE-MIB 静态 VLAN 表（dot1qVlanIndex）等单弧索引表。
///
/// 容错口径：walk 建立或遍历失败（含降级后仍失败）记告警并返回空映射，
/// 由调用方把相关字段降级为缺失（NULL），不阻断同步——设备无此表属正常
/// 场景（如非交换设备无 BRIDGE-MIB），失败与缺失共用空集语义。
pub(crate) async fn walk_index_value_map(
    client: &Client,
    base: async_snmp::Oid,
    warn_key: &'static str,
) -> HashMap<u32, Value> {
    match walk_varbinds(client, &base).await {
        Ok(varbinds) => index_value_map(varbinds),
        Err(e) => {
            // 留痕底层错误详情：调用方需区分"设备无此表"与"采集失败"时
            // 应改用 [`walk_index_value_map_checked`]
            log_warn!(warn_key, error = e.log_detail());
            HashMap::new()
        }
    }
}

/// [`walk_index_value_map`] 的校验版：walk 最终失败时冒泡错误，供
/// 「空台账与采集失败必须区分上报」的调用方（VLAN 台账）使用；
/// 设备无此表时 walk 以 EndOfMibView 正常终止，返回空映射而非错误。
pub(crate) async fn walk_index_value_map_checked(
    client: &Client,
    base: async_snmp::Oid,
) -> Result<HashMap<u32, Value>, SnmpError> {
    let varbinds = walk_varbinds(client, &base)
        .await
        .map_err(WalkError::into_snmp_error)?;

    Ok(index_value_map(varbinds))
}

/// 把 walk 结果聚合为（实例尾弧 → 值）映射。
fn index_value_map(varbinds: Vec<VarBind>) -> HashMap<u32, Value> {
    let mut map = HashMap::new();
    for vb in varbinds {
        if let Some(&index) = vb.oid.arcs().last() {
            map.insert(index, vb.value);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_bulk_retryable_设备实现缺陷类错误可降级() {
        use async_snmp::{ErrorStatus, ResponseMetadata};
        use std::net::SocketAddr;
        use std::time::Duration;

        let target: SocketAddr = "192.0.2.10:161"
            .parse()
            .unwrap_or_else(|e| panic!("测试地址解析失败: {e}"));
        assert!(is_bulk_retryable(&async_snmp::Error::Timeout {
            target,
            elapsed: Duration::from_secs(5),
            retries: 3,
        }));
        assert!(is_bulk_retryable(&async_snmp::Error::MalformedResponse {
            target
        }));
        assert!(is_bulk_retryable(&async_snmp::Error::Snmp {
            target,
            status: ErrorStatus::GenErr,
            index: 1,
            oid: None,
            metadata: Box::new(ResponseMetadata::default()),
        }));
        // 认证失败重试无意义，不降级
        assert!(!is_bulk_retryable(&async_snmp::Error::Auth { target }));
    }

    #[test]
    fn walk_repetitions_与net_snmp默认一致() {
        assert_eq!(WALK_MAX_REPETITIONS, 10);
        assert_eq!(WALK_DEGRADED_REPETITIONS, 2);
        assert_eq!(WALK_PROBE_TIMEOUT_SECS, 6);
        assert_eq!(WALK_STALL_SECS, 6);
        assert_eq!(WALK_STAGE_BUDGET_SECS, 20);
        assert_eq!(WALK_MEMORY_TTL_SECS, 1800);
    }
}
