//! 设备台账 → 主机监控（SNMP 来源）主机行的管理地址联动。
//!
//! `monitored_hosts` 中 `source='snmp'` 行的管理地址由设备台账唯一提供：
//! `ip` 恒取设备首个网口的首个 IP，与 SNMP 轮询采集、设备 SNMP 测试同一
//! 取址口径。设备侧任何改变 IP 归属的写入（网卡配置同步、单条/批量/自动
//! 分配、网口删除、设备迁房）都在同一事务内调用 [`sync_snmp_host_ip`] 即时
//! 刷新；轮询侧只写采集结果（status/指标/hostname 等），不再回写 `ip`，
//! 避免同一字段出现两处来源。

use sqlx::PgConnection;
use uuid::Uuid;

use foims_common::AppError;

/// 按设备当前管理地址刷新其 SNMP 来源主机行的 `ip`。
///
/// 设备已无任何 IP 时置 NULL：SNMP 采集目标消失，主机行与历史保留、状态由
/// 轮询离线翻转维护；设备未纳入 SNMP 采集（无 `source='snmp'` 行）或无变更
/// 时为无操作。
pub async fn sync_snmp_host_ip(conn: &mut PgConnection, device_id: Uuid) -> Result<(), AppError> {
    sqlx::query(
        r"UPDATE monitored_hosts mh
             SET ip = addr.ip
            FROM (
                SELECT d.id AS device_id,
                       (SELECT host(i.ip_address)
                          FROM interface_ips i
                          JOIN device_interfaces di ON i.device_interface_id = di.id
                         WHERE di.device_id = d.id
                         ORDER BY i.created_at
                         LIMIT 1) AS ip
                  FROM devices d
                 WHERE d.id = $1
            ) addr
           WHERE mh.device_id = addr.device_id
             AND mh.source = 'snmp'
             AND mh.ip IS DISTINCT FROM addr.ip",
    )
    .bind(device_id)
    .execute(conn)
    .await?;
    Ok(())
}
