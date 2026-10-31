//! 主机监控调试：一次性 SNMP GET/WALK 探测与 MIB 名称对照（admin/secadmin）。
//!
//! 与 `handlers::test_snmp_connection` 的区别：调试入口手动输入目标（主机名或
//! IP）与凭据、不读设备库，且可对任意 OID 取值/遍历，返回逐行原始值供排障。
//! 目标地址解析后走 `foims_common::net::guard_probe_ip` 同一 SSRF 口径；本端点
//! 挂在 `/api/agents` nest 内，角色由 `agent_admin_guard` 强制，处理器不再重复取令牌。

pub mod mib;

use std::str::FromStr;
use std::time::Duration;

use axum::response::Response;
use async_snmp::Client;

use foims_common::net::resolve_probe_ip;
use foims_common::{AppError, AppJson, msg, ok_json};
use foims_models::{MibEntry, MibQueryRequest, SnmpDebugRequest, SnmpDebugRow};

use crate::core::{SnmpParamsLegacy, build_auth, format_snmp_error, snmp_target};

/// GET 单次最大 OID 数：防御异常输入放大请求规模。
const MAX_GET_OIDS: usize = 60;
/// WALK 单次最大返回行数：有界遍历，避免超大表把响应撑爆。
const MAX_WALK_ROWS: usize = 500;
/// bulk walk 每请求最大重复数。
const WALK_MAX_REPETITIONS: u32 = 20;
/// 调试探测固定短超时（秒），与 test-snmp 口径一致，不读设备侧周期采集超时。
const DEBUG_TIMEOUT_SECS: u64 = 5;

/// POST /api/agents/debug/snmp：对目标执行一次性 SNMP GET 或 WALK。
///
/// `op=get` 对 `oids` 逐个取值；`op=walk` 以首个 OID 为基做有界 bulk 遍历。
/// 目标解析失败/非法/组播/链路本地 400，OID 非法/为空 400，会话或采集失败 400。
pub async fn debug_snmp(
    AppJson(req): AppJson<SnmpDebugRequest>,
) -> Result<Response, AppError> {
    let ip = resolve_probe_ip(&req.target).await?;

    let op = req.op.trim().to_ascii_lowercase();
    let oids: Vec<async_snmp::Oid> = parse_oids(&req.oids)?;
    if oids.is_empty() {
        return Err(AppError::Validation(msg("server.debug.snmp.oids_empty")));
    }

    let version = req
        .snmp_version
        .clone()
        .unwrap_or_else(|| "v2c".to_string());
    let params = SnmpParamsLegacy {
        ip: ip.to_string(),
        port: req.snmp_port.unwrap_or(161),
        version: version.clone(),
        community: req.snmp_community.clone(),
        username: req.snmp_username.clone(),
        auth_proto: req.snmp_auth_protocol.clone(),
        auth_pass: req.snmp_auth_password.clone(),
        priv_proto: req.snmp_priv_protocol.clone(),
        priv_pass: req.snmp_priv_password.clone(),
        timeout_secs: DEBUG_TIMEOUT_SECS,
    };

    foims_common::log_info!(
        "log.device.snmp.test_request",
        device_id = "",
        ip = ip.to_string(),
        version = version
    );

    let auth = build_auth(&params).map_err(|e| AppError::Snmp(msg("server.debug.snmp.failed").with("error", e)))?;
    let addr = snmp_target(&params.ip, params.port);
    let timeout = Duration::from_secs(params.timeout_secs);
    let client = Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .connect()
        .await
        .map_err(|e| {
            AppError::Snmp(
                msg("server.debug.snmp.session_failed").with("error", format_snmp_error(e)),
            )
        })?;

    let rows = match op.as_str() {
        "get" => run_get(&client, &oids).await?,
        "walk" => run_walk(&client, oids[0].clone()).await?,
        other => {
            return Err(AppError::Validation(
                msg("server.debug.snmp.op_invalid").with("op", other),
            ));
        }
    };

    Ok(ok_json(rows, "server.debug.snmp.success"))
}

/// POST /api/agents/debug/mib：MIB 名称/OID 双向对照（纯静态表，不触网）。
pub async fn debug_mib(AppJson(req): AppJson<MibQueryRequest>) -> Result<Response, AppError> {
    let entries: Vec<MibEntry> = mib::lookup(&req.query);
    Ok(ok_json(entries, "server.debug.mib.success"))
}

/// 解析 OID 列表：逐个 `Oid::from_str`，任一非法即 400；规模超上限即 400。
fn parse_oids(oids: &[String]) -> Result<Vec<async_snmp::Oid>, AppError> {
    if oids.len() > MAX_GET_OIDS {
        return Err(AppError::Validation(
            msg("server.debug.snmp.oids_too_many").with("max", MAX_GET_OIDS),
        ));
    }
    oids.iter()
        .map(|raw| {
            async_snmp::Oid::from_str(raw.trim()).map_err(|_| {
                AppError::Validation(msg("server.debug.snmp.oid_invalid").with("oid", raw))
            })
        })
        .collect()
}

/// 逐个 GET：get_many 批量下发，响应与请求等长有序（FixedCardinality）。
async fn run_get(
    client: &Client<async_snmp::transport::UdpHandle>,
    oids: &[async_snmp::Oid],
) -> Result<Vec<SnmpDebugRow>, AppError> {
    let resp = client.get_many(oids).await.map_err(|e| {
        AppError::Snmp(msg("server.debug.snmp.failed").with("error", format_snmp_error(e)))
    })?;
    Ok(resp.varbinds.iter().map(row_of).collect())
}

/// 有界 bulk walk：从基 OID 遍历，收满 `MAX_WALK_ROWS` 即停；单条错误中断保留已收。
async fn run_walk(
    client: &Client<async_snmp::transport::UdpHandle>,
    base: async_snmp::Oid,
) -> Result<Vec<SnmpDebugRow>, AppError> {
    let mut stream = client
        .bulk_walk(base, WALK_MAX_REPETITIONS)
        .map_err(|e| {
            AppError::Snmp(msg("server.debug.snmp.failed").with("error", format_snmp_error(e)))
        })?;
    let mut rows = Vec::new();
    while let Some(item) = stream.next().await {
        match item {
            Ok(vb) => {
                rows.push(row_of(&vb));
                if rows.len() >= MAX_WALK_ROWS {
                    break;
                }
            }
            // 遍历到底或中途异常：保留已收，视为成功（排障场景部分结果仍有用）
            Err(_) => break,
        }
    }
    Ok(rows)
}

/// 变量绑定 → 调试行：OID 点分文本、命中 MIB 表的符号名、SNMP 类型与可读值。
fn row_of(vb: &async_snmp::VarBind) -> SnmpDebugRow {
    let oid = vb.oid.to_string();
    SnmpDebugRow {
        name: mib::name_of(&oid),
        value_type: vb.value.kind().to_string(),
        value: vb.value.to_string(),
        oid,
    }
}
