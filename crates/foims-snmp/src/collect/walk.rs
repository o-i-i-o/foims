//! SNMP 表列 walk 统一实现：GETBULK 降级 GETNEXT 与单弧索引列映射。
//!
//! 全域采集（端口/VLAN/LLDP/实体/聚合/POE/CDP）共用同一份 walk 策略，
//! 避免各表各自实现重复的降级与异常值过滤逻辑。

use std::collections::HashMap;
use std::time::Duration;

use async_snmp::{Client, Value, VarBind, WalkMethod, WalkOptions};
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
/// 单次请求的重传链最坏约 35s（5s 超时 × 4 次 + 3 次固定间隔）；但多表
/// 并发采集时实测部分设备（华为 CE12800）对大重复数请求「慢而不死」——
/// 每个请求都要 1-2 次重试才回包，阶段既不报错也走不完，120s 总预算会在
/// Cr10 阶段直接耗尽，降级链永远推进不到小重复数级（lldp 五表并发实测
/// 复现：无任何降级告警即 budget 超时）。阶段级上限保证过慢/无响应的
/// GETBULK 阶段被及时放弃并推进到下一级。取值须大于单请求重传链最坏值
///（默认 5s/3 次配置为 35s）；总预算扣除两级阶段预算后仍给 GETNEXT
/// 兜底留出约 30s。
const WALK_STAGE_BUDGET_SECS: u64 = 45;

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

/// walk 单个 OID：降级链为「GETBULK-10 → GETBULK-2 → GETNEXT」，前两级
/// GETBULK 各受 [`WALK_STAGE_BUDGET_SECS`] 阶段预算约束，阶段超时/失败
/// 先降重复数重试，仍失败降级 GETNEXT 逐条遍历并告警；认证/配置类错误
/// 直接冒泡（重试无意义且会翻倍失败时延）。
///
/// 全程受 [`WALK_BUDGET_SECS`] 总预算约束，超预算终止并返回
/// [`WalkError::BudgetExceeded`]；降级重试不会留下半张表的中间结果
///（超预算截断除外——截断结果不可信，一律按失败上报，不返回部分数据）。
pub(crate) async fn walk_varbinds(
    client: &Client,
    target: &async_snmp::Oid,
) -> Result<Vec<VarBind>, WalkError> {
    let walk = async {
        match walk_bulk_stage(client, target, WALK_MAX_REPETITIONS).await {
            BulkStage::Done(varbinds) => Ok(varbinds),
            BulkStage::Retry(detail) => {
                warn!(
                    "SNMP walk GETBULK 失败，降重复数重试: oid={}, error={}",
                    target, detail
                );
                match walk_bulk_stage(client, target, WALK_DEGRADED_REPETITIONS).await {
                    BulkStage::Done(varbinds) => Ok(varbinds),
                    // 小重复数仍失败：设备 bulk 实现异常或单条响应即超限，
                    // 降级 GETNEXT 逐条遍历
                    BulkStage::Retry(detail) => {
                        warn!(
                            "SNMP walk 小重复数重试仍失败，降级 GETNEXT: oid={}, error={}",
                            target, detail
                        );
                        walk_collect(
                            client,
                            target,
                            WalkOptions {
                                method: WalkMethod::GetNext,
                                ..WalkOptions::default()
                            },
                        )
                        .await
                        .map_err(|e| *e)
                    }
                    BulkStage::Fatal(err) => Err(*err),
                }
            }
            BulkStage::Fatal(err) => Err(*err),
        }
    };

    match tokio::time::timeout(Duration::from_secs(WALK_BUDGET_SECS), walk).await {
        Ok(result) => result.map_err(|e| WalkError::Snmp(Box::new(e))),
        Err(_) => Err(WalkError::BudgetExceeded),
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
/// 阶段整体超 [`WALK_STAGE_BUDGET_SECS`] 未完成即放弃（返回可重试），
/// 避免设备「慢而不死」时把总预算耗在单级上。
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
        walk_collect(client, target, options),
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
) -> Result<Vec<VarBind>, Box<async_snmp::Error>> {
    let mut walk = client.walk_with(target.clone(), options)?;

    let mut varbinds = Vec::new();

    while let Some(result) = walk.next().await {
        let vb = result?;

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
    }
}
