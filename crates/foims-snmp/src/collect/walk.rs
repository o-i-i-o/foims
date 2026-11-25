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

/// 单列 walk 总体耗时上限（覆盖 GETBULK 含重试约 35s 最坏 + GETNEXT 降级
/// 全程）。健康设备 walk 数秒内完成，预算仅约束故障设备（如对两种遍历
/// 方式均不响应的机型），避免同步采集接口被无界耗时拖到网关超时墙之外
///（nginx /api/ 通用读超时 60s，采集类端点单独放宽到 5m）。
pub(crate) const WALK_BUDGET_SECS: u64 = 120;

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
    fn log_detail(self) -> String {
        match self {
            WalkError::Snmp(e) => format_snmp_error(e).log_string(),
            WalkError::BudgetExceeded => msg("server.snmp.error.timeout")
                .with("detail", "超过 walk 总预算")
                .log_string(),
        }
    }
}

/// walk 单个 OID：先按 [`WALK_MAX_REPETITIONS`] 做 GETBULK；失败时对
/// 「可能由单响应过大/设备 bulk 实现异常导致」的错误降级为 GETNEXT 逐条
/// 遍历并告警，认证/配置类错误直接冒泡（重试无意义且会翻倍失败时延）。
///
/// 全程受 [`WALK_BUDGET_SECS`] 总预算约束，超预算终止并返回
/// [`WalkError::BudgetExceeded`]；降级重试不会留下半张表的中间结果
///（超预算截断除外——截断结果不可信，一律按失败上报，不返回部分数据）。
pub(crate) async fn walk_varbinds(
    client: &Client,
    target: &async_snmp::Oid,
) -> Result<Vec<VarBind>, WalkError> {
    let walk = async {
        let bulk = WalkOptions {
            max_repetitions: WALK_MAX_REPETITIONS,
            ..WalkOptions::default()
        };

        match walk_collect(client, target, bulk).await {
            Ok(varbinds) => Ok(varbinds),
            Err(err) if is_bulk_retryable(&err) => {
                warn!(
                    "SNMP walk 失败，降级 GETNEXT 重试: oid={}, error={:?}",
                    target, err
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
            }
            Err(err) => Err(err),
        }
    };

    match tokio::time::timeout(Duration::from_secs(WALK_BUDGET_SECS), walk).await {
        Ok(result) => result.map_err(WalkError::Snmp),
        Err(_) => Err(WalkError::BudgetExceeded),
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
    }
}
