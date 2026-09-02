//! 设备与工位匹配后的邮件通知。
//!
//! 设备创建/编辑完成且匹配了工位时，若工位管理人关联了员工且员工
//! 配置了邮箱，则把设备当前 IP 信息发送给管理人。通知是尽力而为的
//! 旁路动作：SMTP 未配置或发送失败仅记录日志，不影响设备操作结果。

use foims_common::{log_debug, log_warn};
use sqlx::Row;
use uuid::Uuid;

/// 设备工位匹配信息（管理人 + 邮箱）
struct MatchInfo {
    device_name: String,
    workstation_name: String,
    manager_email: String,
    manager_name: Option<String>,
}

/// 异步通知工位管理人（fire-and-forget，不阻塞设备操作）。
pub fn spawn_ip_notification(pool: sqlx::PgPool, device_id: Uuid) {
    tokio::spawn(async move {
        if let Err(e) = notify_device_ips(&pool, device_id).await {
            log_warn!("log.smtp.device_notify_failed", error = e);
        }
    });
}

/// 查询匹配信息并发送邮件；跳过场景（未匹配工位/无邮箱/无 IP）
/// 静默返回 Ok(())。
async fn notify_device_ips(pool: &sqlx::PgPool, device_id: Uuid) -> Result<(), String> {
    let info = sqlx::query(
        r"SELECT d.name AS device_name, w.name AS workstation_name,
                  e.email AS manager_email, e.name AS manager_name
         FROM devices d
         JOIN workstations w ON d.workstation_id = w.id
         LEFT JOIN employees e ON w.manager_employee_id = e.id
         WHERE d.id = $1 AND e.email IS NOT NULL",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .map(|row| MatchInfo {
        device_name: row.get("device_name"),
        workstation_name: row.get("workstation_name"),
        manager_email: row.get("manager_email"),
        manager_name: row.get("manager_name"),
    });

    let Some(info) = info else {
        // 未匹配工位或管理人无邮箱：无需通知
        log_debug!(
            "log.smtp.device_notify_skipped",
            device_id = device_id.to_string()
        );
        return Ok(());
    };

    let ip_rows = sqlx::query(
        r"SELECT host(m.ip_address) AS ip, COALESCE(nc.name, '') AS network
           FROM ips m
           JOIN device_interfaces di ON m.device_interface_id = di.id
           LEFT JOIN network_cidrs nc ON m.subnet_id = nc.id
           WHERE di.device_id = $1
           ORDER BY m.ip_address",
    )
    .bind(device_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    // 暂无 IP 信息时不必打扰管理人
    if ip_rows.is_empty() {
        return Ok(());
    }

    let ip_lines: Vec<String> = ip_rows
        .iter()
        .map(|row| {
            let ip: String = row.get("ip");
            let network: String = row.get("network");
            if network.is_empty() {
                format!("IP 地址：{ip}")
            } else {
                format!("IP 地址：{ip}（网段：{network}）")
            }
        })
        .collect();

    let manager_greeting = info
        .manager_name
        .map(|name| format!("{name}，您好："))
        .unwrap_or_else(|| "您好：".to_string());

    let body = format!(
        "{manager_greeting}\n\n设备已匹配到您管理的工位，IP 信息如下：\n设备名称：{}\n所属工位：{}\n{}\n\n此邮件由 FOIMS 系统自动发送，请勿回复。\n",
        info.device_name,
        info.workstation_name,
        ip_lines.join("\n")
    );

    let subject = format!(
        "设备 IP 通知 - {}（工位: {}）",
        info.device_name, info.workstation_name
    );

    // SMTP 未配置属于业务状态：静默跳过，不产生错误日志刷屏
    if foims_auth::smtp::send_email_async(pool, &info.manager_email, &subject, &body)
        .await
        .is_err()
    {
        log_debug!(
            "log.smtp.device_notify_skipped",
            device_id = device_id.to_string()
        );
    }
    Ok(())
}
