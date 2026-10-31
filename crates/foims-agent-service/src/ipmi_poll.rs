//! IPMI 设备带外轮询采集（主机监控「IPMI 采集」）。
//!
//! 周期任务：扫描 devices 表中已配置 IPMI 凭据（username 与 password 均非空）
//! 且带管理 IP 的设备，逐台建 RMCP+ 会话采集并 upsert 为 `source='ipmi'` 的
//! monitored_hosts 行：
//! - machine_id 合成 `ipmi:{device_id}`，与 agent/snmp 命名空间隔离；
//! - 同设备的 SNMP 行与 IPMI 行并存（device_id 唯一性按 (device_id, source)）；
//! - 设备删除时经 device_id 外键级联清理。
//!
//! 采集范围：Get Device ID（厂商串落 os）+ Chassis Status（电源）+ SDR 传感器
//! 聚合 max_temp/风扇/电压（见 [`foims_ipmi::collect`]）。IPMI 无 CPU/内存/磁盘
//! 语义，这些热列保持 NULL，前端按空值渲染 "-"。

use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;
use tokio::sync::Semaphore;

use foims_ipmi::collect::collect as ipmi_collect;
use foims_ipmi::core::{DeviceForIpmi, open_session};
use foims_ipmi::truncate_ipmi_text;

/// 单轮轮询的最大并发设备数：BMC 会话资源有限，与实时路径同取 8。
const POLL_CONCURRENCY: usize = 8;
/// hostname/os 入库截断宽度（与 snmp_poll 列宽口径一致）
const MAX_HOSTNAME_LEN: usize = 255;
const MAX_OS_LEN: usize = 128;

/// IPMI 采集设备行：IPMI 配置（复用 foims-ipmi 的 DeviceForIpmi）+ 管理地址。
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct IpmiDeviceRow {
    #[sqlx(flatten)]
    pub(crate) device: DeviceForIpmi,
    /// 设备首个网口 IP（host() 文本形式，IPv6 不含方括号）
    pub(crate) ip: String,
    /// 对应 `monitored_hosts` 行 id（告警 episode 主体；基线行缺失时为 None）
    pub(crate) host_id: Option<uuid::Uuid>,
}

/// 单轮 IPMI 轮询入口：采集全部已配置设备并 upsert `monitored_hosts`（source='ipmi'）行，
/// 返回 (成功台数, 失败台数)。
pub async fn poll_all(pool: &PgPool, interval_secs: u64) -> (usize, usize) {
    let devices = match load_ipmi_devices(pool).await {
        Ok(d) => d,
        Err(e) => {
            foims_common::log_error!("log.task.agent_ipmi_poll_failed", error = e);
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
            let permit = match semaphore.acquire_owned().await {
                Ok(permit) => permit,
                Err(e) => {
                    foims_common::log_warn!("log.task.ipmi_poll_permit_failed", error = e);
                    return Err(());
                }
            };
            let _permit = permit;
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

    // 离线翻转：连续两轮（间隔 × 2）未成功的 active IPMI 行置 offline
    let threshold = interval_secs.saturating_mul(2);
    let _ = sqlx::query(
        r"UPDATE monitored_hosts SET status = 'offline'
           WHERE source = 'ipmi'
             AND status = 'active'
             AND last_seen < NOW() - make_interval(secs => $1)",
    )
    .bind(threshold as f64)
    .execute(pool)
    .await
    .inspect_err(|e| {
        foims_common::log_error!("log.task.agent_ipmi_poll_failed", error = e);
    });

    (ok, fail)
}

/// 查询全部「已配置 IPMI 凭据 + 已知管理地址」的设备。
///
/// 配置判定与设备列表 ipmi_configured 字段同口径：username 与 password 均
/// 非空；管理地址取设备首个网口的首个 IP；`host_id` 经
/// `machine_id = 'ipmi:{device_id}'` 关联出对应监控主机行，供实时路径落告警
/// episode（基线行由 [`ensure_ipmi_host_present`] 补建）。
pub(crate) async fn load_ipmi_devices(pool: &PgPool) -> Result<Vec<IpmiDeviceRow>, sqlx::Error> {
    sqlx::query_as::<_, IpmiDeviceRow>(
        r"SELECT d.id, d.name, d.ipmi_username, d.ipmi_password,
                  d.ipmi_port, d.ipmi_auth_algo,
                  first_ip.ip AS ip,
                  mh.id AS host_id
             FROM devices d
             LEFT JOIN LATERAL (
                 SELECT host(i.ip_address) AS ip
                   FROM interface_ips i
                   JOIN device_interfaces di ON i.device_interface_id = di.id
                  WHERE di.device_id = d.id
                  ORDER BY i.created_at
                  LIMIT 1
             ) first_ip ON true
             LEFT JOIN monitored_hosts mh
                        ON mh.machine_id = 'ipmi:' || d.id::text
            WHERE d.ipmi_username IS NOT NULL
              AND d.ipmi_password IS NOT NULL
              AND first_ip.ip IS NOT NULL",
    )
    .fetch_all(pool)
    .await
}

/// IPMI 行的 machine_id 命名空间：`ipmi:{device_id}`，与 agent/snmp 隔离。
pub(crate) fn ipmi_machine_id(device_id: uuid::Uuid) -> String {
    format!("ipmi:{device_id}")
}

/// 取设备对应的 `source='ipmi'` 监控主机行 id（实时路径在基线行刚补建后回填）。
pub(crate) async fn load_ipmi_host_id(pool: &PgPool, device_id: uuid::Uuid) -> Option<uuid::Uuid> {
    let result =
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM monitored_hosts WHERE machine_id = $1")
            .bind(ipmi_machine_id(device_id))
            .fetch_optional(pool)
            .await;
    match result {
        Ok(id) => id,
        Err(e) => {
            foims_common::log_error!(
                "system.ipmi_live_host_id_failed",
                device_id = device_id,
                error = e
            );
            None
        }
    }
}

/// 为已配置 IPMI 且带管理地址的设备确保一条 `source='ipmi'` 基线行存在：
/// 设备一经纳入轮询即在主机监控出现，首轮以台账名与 offline 落位，可达性由
/// 后续采集成功路径翻转为 active。仅在行缺失时创建，不触碰 status/last_seen，
/// 失败仅记日志、不阻断采集。
pub(crate) async fn ensure_ipmi_host_present(
    pool: &PgPool,
    machine_id: &str,
    device_id: uuid::Uuid,
    device_name: &str,
    ip: &str,
) {
    let hostname = truncate_ipmi_text(device_name, MAX_HOSTNAME_LEN);
    let result = sqlx::query(
        r"INSERT INTO monitored_hosts (machine_id, source, device_id, status,
                                      hostname, ip, first_seen, last_seen)
          VALUES ($1, 'ipmi', $2, 'offline', $3, $4, NOW(), NOW())
          ON CONFLICT DO NOTHING",
    )
    .bind(machine_id)
    .bind(device_id)
    .bind(&hostname)
    .bind(ip)
    .execute(pool)
    .await;
    if let Err(e) = result {
        foims_common::log_error!(
            "log.task.agent_ipmi_poll_failed",
            error = format!("设备 {device_name}（{device_id}）IPMI 基线入库失败: {e}")
        );
    }
}

/// 采集单台设备并 upsert 对应 monitored_hosts 行 + 追加历史快照（仅 temp 键）：
/// 基线行由 [`ensure_ipmi_host_present`] 先行保证存在，采集任一步失败仅记录
/// 日志并提前返回（不标记 active、不刷新 last_seen，由调用方计入失败数，累积
/// 触发离线翻转）。
async fn poll_and_upsert(pool: &PgPool, row: &IpmiDeviceRow) -> Result<(), ()> {
    let device_id = row.device.id;
    let device_name = row.device.name.as_str();
    let ip = row.ip.as_str();
    let machine_id = ipmi_machine_id(device_id);

    ensure_ipmi_host_present(pool, &machine_id, device_id, device_name, ip).await;

    // 会话参数构造（含口令解密）：未配置（用户名/口令缺失）→ 跳过本轮（计失败）
    let params = match row.device.to_ipmi_params_async(ip).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            tracing::debug!("IPMI 未配置凭据，跳过采集: 设备 {device_name}（{device_id}）");
            return Err(());
        }
        Err(e) => {
            tracing::warn!("IPMI 轮询参数构造失败: 设备 {device_name}（{device_id}）: {e}");
            return Err(());
        }
    };

    let reading = match collect_device(&params).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("IPMI 轮询失败: 设备 {device_name}（{device_id}，{ip}）: {e}");
            return Err(());
        }
    };

    let now = Utc::now();
    // hostname 回落台账名，os 取 BMC 厂商可读串（IPMI 无 OS 语义）
    let hostname = truncate_ipmi_text(device_name, MAX_HOSTNAME_LEN);
    let os = truncate_ipmi_text(&reading.device.vendor_label(), MAX_OS_LEN);
    let raw_metrics = reading.to_report();
    let max_temp = reading.max_temp;

    // upsert：热列仅 temp 有值，cpu/mem/disk 保持原值（IPMI 无此语义）
    let upsert = sqlx::query_scalar::<_, uuid::Uuid>(
        r"INSERT INTO monitored_hosts (machine_id, source, device_id, status,
                              hostname, ip, os, max_temp,
                              raw_metrics, first_seen, last_seen)
          VALUES ($1, 'ipmi', $2, 'active', $3, $4, $5, $6, $7, NOW(), NOW())
          ON CONFLICT (machine_id) WHERE machine_id IS NOT NULL DO UPDATE SET
              status = CASE WHEN monitored_hosts.status = 'offline' THEN 'active' ELSE monitored_hosts.status END,
              hostname = EXCLUDED.hostname,
              os = EXCLUDED.os,
              max_temp = COALESCE(EXCLUDED.max_temp, monitored_hosts.max_temp),
              raw_metrics = EXCLUDED.raw_metrics,
              last_seen = NOW()
          RETURNING id",
    )
    .bind(&machine_id)
    .bind(device_id)
    .bind(&hostname)
    .bind(ip)
    .bind(&os)
    .bind(max_temp)
    .bind(&raw_metrics)
    .fetch_one(pool)
    .await;

    let agent_id = match upsert {
        Ok(id) => id,
        Err(e) => {
            foims_common::log_error!(
                "log.task.agent_ipmi_poll_failed",
                error = format!("设备 {device_name}（{device_id}）IPMI 入库失败: {e}")
            );
            return Err(());
        }
    };

    // 历史曲线快照：IPMI 只有温度可取（cpu/mem/disk 无语义，不落键）
    let history = serde_json::json!({
        "temp": max_temp,
    });
    let insert = sqlx::query(
        r"INSERT INTO host_metrics_history (agent_id, collected_at, metrics)
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
                "log.task.agent_ipmi_poll_failed",
                error = format!("设备 {device_name}（{device_id}）IPMI 历史写入失败: {e}")
            );
            Err(())
        }
    }
}

/// 建会话 → 采集 → 关会话（单台一轮）。会话异常同样返回 Err（供 poll 失败计数）。
/// 实时路径（`live_poll`）复用本函数：每轮采样即建会话、结束后关闭，避免 BMC
/// 会话资源长期占用。
pub(crate) async fn collect_device(
    params: &foims_ipmi::IpmiParams,
) -> Result<foims_ipmi::IpmiReading, String> {
    let mut client = open_session(params)
        .await
        .map_err(|e| format!("IPMI 会话建立失败: {e}"))?;
    let result = ipmi_collect(&mut client).await.map_err(|e| e.to_string());
    let _ = client.close().await;
    result
}

#[cfg(test)]
mod tests {
    use super::ipmi_machine_id;

    #[test]
    fn ipmi_machine_id_namespaces_by_device_uuid() {
        let id = uuid::Uuid::parse_str("0e8a1c34-5b6f-4d7e-8a9b-0c1d2e3f4a5b")
            .unwrap_or_else(|_| panic!("合法测试 UUID"));
        // 与 agent（裸 machine_id）/snmp（snmp:{id}）隔离的同构命名空间
        assert_eq!(
            ipmi_machine_id(id),
            "ipmi:0e8a1c34-5b6f-4d7e-8a9b-0c1d2e3f4a5b"
        );
    }
}
