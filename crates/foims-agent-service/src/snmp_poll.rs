//! SNMP 设备轮询采集（设计 docs/agent-design.md「SNMP 设备纳入主机监控」）。
//!
//! 周期任务：扫描 devices 表中已配置 SNMP 凭据（community/username）且带
//! 管理 IP 的设备，逐台采集并 upsert 为 `source='snmp'` 的 agents 行：
//! - machine_id 合成 `snmp:{device_id}`（与 agent machine_id 命名空间隔离）；
//! - 无下发 token，token_hash 置空（agent 上报路径凭 token 查行，天然隔离）；
//! - 单台成功即 last_seen=NOW() 并翻转 offline → active；连续失败超过
//!   离线阈值（轮询间隔 × 2，即连续两轮未成功）由本模块统一置 offline，
//!   agent_offline 任务仅对 source='agent' 生效，互不干扰；
//! - 设备删除时 agents 行经 device_id 外键级联清理。
//!
//! 采集范围（二期）：一期系统组（sysName/sysDescr/sysUpTime）+ 二期性能
//! 指标（CPU/内存/磁盘/温度/流量/负载，见 [`snmp_metrics`]）。热列
//! （cpu_usage/mem_usage_pct/disk_usage_pct/max_temp）仅在采得新值时
//! 覆盖（COALESCE 保留旧值），raw_metrics 总是整体写回（含
//! `_snmp_state` 差值状态）；同时向 agent_metrics_history 追加曲线
//! 快照（cpu/mem/disk/temp/rx_bps/tx_bps）。

use std::sync::Arc;
use std::time::Duration;

use async_snmp::transport::UdpHandle;
use async_snmp::{Client, Retry, oid};
use chrono::Utc;
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::Semaphore;

use foims_resource::DeviceForSnmp;
use foims_resource::device::snmp::{
    build_auth, format_snmp_error, snmp_target, truncate_to_column_width,
};

use crate::snmp_metrics;

/// 单设备 SNMP 单次请求超时（秒），与设备测试路径（get_device_info_via_snmp）一致
const SNMP_TIMEOUT_SECS: u64 = 5;
/// 单轮轮询的最大并发设备数：避免大量离线设备同时超时拖垮运行时
const POLL_CONCURRENCY: usize = 8;
/// hostname/os 入库截断宽度（与 ingest 文本校验上限对齐）
const MAX_HOSTNAME_LEN: usize = 255;
const MAX_OS_LEN: usize = 128;

/// SNMP 采集设备行：SNMP 配置（复用 foims-resource 的 DeviceForSnmp）+ 管理地址。
#[derive(Debug, sqlx::FromRow)]
struct SnmpDeviceRow {
    #[sqlx(flatten)]
    device: DeviceForSnmp,
    /// 设备首个网口 IP（host() 文本形式，IPv6 不含方括号）
    ip: String,
}

/// 单台设备本轮 SNMP 采集结果（MIB-II 系统组）。
struct SnmpSysInfo {
    /// sysName（trim 后为空时回落设备名，由调用方处理）
    hostname: Option<String>,
    /// sysDescr（设备描述串）
    os: Option<String>,
    /// sysUpTime 换算秒（TimeTicks 百分之一秒 → 秒）
    uptime_secs: Option<i64>,
}

/// 单轮 SNMP 轮询入口：采集全部已配置设备并回写 agents，
/// 返回 (成功台数, 失败台数)。
pub async fn poll_all(pool: &PgPool, interval_secs: u64) -> (usize, usize) {
    let devices = match load_snmp_devices(pool).await {
        Ok(d) => d,
        Err(e) => {
            foims_common::log_error!("log.task.agent_snmp_poll_failed", error = e);
            return (0, 0);
        }
    };
    if devices.is_empty() {
        return (0, 0);
    }

    let semaphore = Arc::new(Semaphore::new(POLL_CONCURRENCY));
    let mut tasks = tokio::task::JoinSet::new();
    for row in devices {
        let semaphore = semaphore.clone();
        let pool = pool.clone();
        tasks.spawn(async move {
            // 先取许可再采集，限制单轮并发
            let _permit = semaphore.acquire_owned().await;
            poll_and_upsert(&pool, &row).await
        });
    }

    let mut ok = 0;
    let mut fail = 0;
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(Ok(())) => ok += 1,
            _ => fail += 1,
        }
    }

    // 离线翻转：连续两轮（间隔 × 2）未成功的 active SNMP 行置 offline
    let threshold = interval_secs.saturating_mul(2);
    let _ = sqlx::query(
        r"UPDATE agents SET status = 'offline'
           WHERE source = 'snmp'
             AND status = 'active'
             AND last_seen < NOW() - make_interval(secs => $1)",
    )
    .bind(threshold as f64)
    .execute(pool)
    .await
    .inspect_err(|e| {
        foims_common::log_error!("log.task.agent_snmp_poll_failed", error = e);
    });

    (ok, fail)
}

/// 查询全部「已配置 SNMP 凭据 + 已知管理地址」的设备。
///
/// 配置判定与设备列表 snmp_configured 字段同口径：community 或 username
/// 任一非空（snmp_version 列有 DEFAULT 'v2c'，不能作依据）；管理地址取
/// 设备首个网口的首个 IP（与设备 SNMP 测试路径同源）。
async fn load_snmp_devices(pool: &PgPool) -> Result<Vec<SnmpDeviceRow>, sqlx::Error> {
    sqlx::query_as::<_, SnmpDeviceRow>(
        r"SELECT d.id, d.name, d.snmp_version, d.snmp_community,
                  d.snmp_username, d.snmp_auth_protocol, d.snmp_auth_password,
                  d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
                  first_ip.ip AS ip
             FROM devices d
             LEFT JOIN LATERAL (
                 SELECT host(i.ip_address) AS ip
                   FROM ips i
                   JOIN device_interfaces di ON i.device_interface_id = di.id
                  WHERE di.device_id = d.id
                  ORDER BY i.created_at
                  LIMIT 1
             ) first_ip ON true
            WHERE (d.snmp_community IS NOT NULL OR d.snmp_username IS NOT NULL)
              AND first_ip.ip IS NOT NULL",
    )
    .fetch_all(pool)
    .await
}

/// 采集单台设备并 upsert 对应 agents 行 + 追加历史快照：任一步失败仅
/// 记录日志，由调用方计入失败数（该行 last_seen 不刷新，累积触发离线
/// 翻转）。
async fn poll_and_upsert(pool: &PgPool, row: &SnmpDeviceRow) -> Result<(), ()> {
    let device_id = row.device.id;
    let device_name = row.device.name.as_str();
    let ip = row.ip.as_str();

    // SNMP 参数构造（含密文解密与 v1/v2c/v3 认证装配，复用设备测试路径）
    let params = match row.device.to_snmp_params_async(ip).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("SNMP 轮询参数构造失败: 设备 {device_name}（{device_id}）: {e}");
            return Err(());
        }
    };

    let client = match connect_snmp(&params).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("SNMP 轮询失败: 设备 {device_name}（{device_id}，{ip}）: {e}");
            return Err(());
        }
    };

    // 系统组（一期）：sysName/sysDescr/sysUpTime，缺失容忍
    let info = read_system_info(&client).await;
    // sysName 缺失/为空时回落设备名，保证列表可读
    let hostname = info.hostname.unwrap_or_else(|| device_name.to_string());
    let hostname = truncate_to_column_width(&hostname, MAX_HOSTNAME_LEN);
    let os = info.os.map(|s| truncate_to_column_width(&s, MAX_OS_LEN));
    // machine_id 合成命名空间：'snmp:{device_id}'，与 agent 上报的机器指纹隔离
    let machine_id = format!("snmp:{device_id}");

    // 性能指标（二期）：差值依赖上一轮 raw_metrics._snmp_state
    let now = Utc::now();
    let prev_state = load_snmp_state(pool, &machine_id).await;
    let snap = snmp_metrics::collect(&client, prev_state.as_ref(), now).await;
    let raw_metrics = snmp_metrics::build_report_json(
        &machine_id,
        &hostname,
        os.as_deref(),
        info.uptime_secs,
        &snap,
        now,
    );

    // 热列输入：使用率/最高温度（内存/磁盘取百分比聚合）
    let cpu_usage = snap.cpu_usage_pct;
    let mem_pct = snmp_metrics::mem_usage_pct(snap.mem_total.zip(snap.mem_used));
    let disk_pct = snmp_metrics::disk_usage_pct(&snap.disks);
    let max_temp = snmp_metrics::max_temp(&snap.sensors);

    // upsert：热列仅在采得新值时覆盖（COALESCE），raw_metrics 整体写回
    let upsert = sqlx::query_scalar::<_, uuid::Uuid>(
        r"INSERT INTO agents (machine_id, token_hash, source, device_id, status,
                              hostname, ip, os, uptime_secs,
                              cpu_usage, mem_usage_pct, disk_usage_pct, max_temp,
                              raw_metrics, first_seen, last_seen)
          VALUES ($1, NULL, 'snmp', $2, 'active', $3, $4, $5, $6, $7, $8, $9, $10, $11, NOW(), NOW())
          ON CONFLICT (machine_id) WHERE machine_id IS NOT NULL DO UPDATE SET
              status = CASE WHEN agents.status = 'offline' THEN 'active' ELSE agents.status END,
              hostname = EXCLUDED.hostname,
              ip = EXCLUDED.ip,
              os = EXCLUDED.os,
              uptime_secs = EXCLUDED.uptime_secs,
              cpu_usage = COALESCE(EXCLUDED.cpu_usage, agents.cpu_usage),
              mem_usage_pct = COALESCE(EXCLUDED.mem_usage_pct, agents.mem_usage_pct),
              disk_usage_pct = COALESCE(EXCLUDED.disk_usage_pct, agents.disk_usage_pct),
              max_temp = COALESCE(EXCLUDED.max_temp, agents.max_temp),
              raw_metrics = EXCLUDED.raw_metrics,
              last_seen = NOW()
          RETURNING id",
    )
    .bind(&machine_id)
    .bind(device_id)
    .bind(&hostname)
    .bind(ip)
    .bind(&os)
    .bind(info.uptime_secs)
    .bind(cpu_usage)
    .bind(mem_pct)
    .bind(disk_pct)
    .bind(max_temp)
    .bind(&raw_metrics)
    .fetch_one(pool)
    .await;

    let agent_id = match upsert {
        Ok(id) => id,
        Err(e) => {
            foims_common::log_error!(
                "log.task.agent_snmp_poll_failed",
                error = format!("设备 {device_name}（{device_id}）入库失败: {e}")
            );
            return Err(());
        }
    };

    // 历史曲线快照：cpu/mem/disk/temp + 流量合计（与 agent 上报路径
    // ingest 的快照键保持一致）；冲突（同秒重复轮询）幂等跳过
    let (rx_bps, tx_bps) = snmp_metrics::net_sums(&snap.nets);
    let history = serde_json::json!({
        "cpu": cpu_usage,
        "mem": mem_pct,
        "disk": disk_pct,
        "temp": max_temp,
        "rx_bps": rx_bps,
        "tx_bps": tx_bps,
    });
    let insert = sqlx::query(
        r"INSERT INTO agent_metrics_history (agent_id, collected_at, metrics)
          VALUES ($1, $2, $3) ON CONFLICT (agent_id, collected_at) DO NOTHING",
    )
    .bind(agent_id)
    .bind(now)
    .bind(&history)
    .execute(pool)
    .await;
    match insert {
        Ok(_) => Ok(()),
        Err(e) => {
            foims_common::log_error!(
                "log.task.agent_snmp_poll_failed",
                error = format!("设备 {device_name}（{device_id}）历史写入失败: {e}")
            );
            Err(())
        }
    }
}

/// 读取上一轮差值状态（raw_metrics 私有键 `_snmp_state`）；行不存在/
/// 键缺失/结构不合法时返回 None（首轮不出差值指标，状态本轮重建）。
async fn load_snmp_state(pool: &PgPool, machine_id: &str) -> Option<snmp_metrics::SnmpState> {
    let row: Option<Option<Value>> =
        sqlx::query_scalar("SELECT raw_metrics -> '_snmp_state' FROM agents WHERE machine_id = $1")
            .bind(machine_id)
            .fetch_optional(pool)
            .await
            .ok()?;
    row.flatten().and_then(|v| serde_json::from_value(v).ok())
}

/// 建立设备 SNMP 会话（周期轮询关闭内建重试：默认 Retry（3 次 × 5s +
/// 1s 退避）会把单个 OID 拖到 23s，足以挤占下一轮调度窗口）。
async fn connect_snmp(
    params: &foims_resource::SnmpParamsLegacy,
) -> Result<Client<UdpHandle>, String> {
    let addr = snmp_target(&params.ip, params.port);
    let timeout = Duration::from_secs(SNMP_TIMEOUT_SECS);
    let auth = build_auth(params)?;
    Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .retry(Retry::none())
        .connect()
        .await
        .map_err(|e| format!("创建SNMP会话失败: {}", format_snmp_error(e)))
}

/// 读取系统组三个 OID（同一会话顺序请求，一期语义：缺失容忍）。
async fn read_system_info(client: &Client<UdpHandle>) -> SnmpSysInfo {
    // sysName → 主机名；设备不支持该 OID 时容忍缺失
    let hostname = match client.get(&oid!(1, 3, 6, 1, 2, 1, 1, 5, 0)).await {
        Ok(v) => v
            .varbinds
            .first()
            .filter(|vb| !vb.value.is_exception())
            .and_then(|vb| vb.value.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        Err(e) => {
            tracing::debug!("sysName 读取失败（容忍缺失）: {}", format_snmp_error(e));
            None
        }
    };

    // sysDescr → 系统描述（os 列）；异常时容忍缺失
    let os = match client.get(&oid!(1, 3, 6, 1, 2, 1, 1, 1, 0)).await {
        Ok(v) => v
            .varbinds
            .first()
            .filter(|vb| !vb.value.is_exception())
            .and_then(|vb| vb.value.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        Err(e) => {
            tracing::debug!("sysDescr 读取失败（容忍缺失）: {}", format_snmp_error(e));
            None
        }
    };

    // sysUpTime → 运行时长（TimeTicks 百分之一秒）；读取/换算失败容忍缺失
    let uptime_secs = match client.get(&oid!(1, 3, 6, 1, 2, 1, 1, 3, 0)).await {
        Ok(v) => v
            .varbinds
            .first()
            .filter(|vb| !vb.value.is_exception())
            .and_then(|vb| vb.value.as_u32())
            .map(|ticks| i64::from(ticks) / 100),
        Err(e) => {
            tracing::debug!("sysUpTime 读取失败（容忍缺失）: {}", format_snmp_error(e));
            None
        }
    };

    SnmpSysInfo {
        hostname,
        os,
        uptime_secs,
    }
}
