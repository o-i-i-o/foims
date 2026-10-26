//! 设备 IP 信息邮件通知。
//!
//! 责任人按「工位管理人 → 房间管理人」取首个非空：设备匹配了工位则由
//! 工位管理人接收，否则（如机房里的网络设备）由房间管理人接收。通知是
//! 尽力而为的旁路动作：SMTP 未配置或发送失败仅记录日志，不影响设备
//! 操作结果。

use foims_common::AppError;
use foims_common::{log_debug, log_warn};
use sqlx::Row;
use uuid::Uuid;

/// 设备空间归属与责任人信息。
///
/// `workstation_name` 有值表示责任人来自工位管理人，无值表示回退到
/// 房间管理人（设备只有房间归属）。
struct MatchInfo {
    device_name: String,
    workstation_name: Option<String>,
    room_name: String,
    manager_email: String,
    manager_name: Option<String>,
}

/// 异步通知设备责任人（fire-and-forget，不阻塞设备操作）。
pub fn spawn_ip_notification(pool: sqlx::PgPool, device_id: Uuid) {
    tokio::spawn(async move {
        if let Err(e) = notify_device_ips(&pool, device_id).await {
            log_warn!("log.smtp.device_notify_failed", error = e);
        }
    });
}

/// 查询责任人与空间信息并发送邮件；跳过场景（责任人无邮箱/无 IP）
/// 静默返回 Ok(())。
async fn notify_device_ips(pool: &sqlx::PgPool, device_id: Uuid) -> Result<(), String> {
    // room_id 为 NOT NULL 且与 workstation/position 的房间由触发器保持一致，
    // 故房间恒可联到；责任人取工位管理人优先，其次房间管理人
    let info = sqlx::query(
        r"SELECT d.name AS device_name,
                  w.name AS workstation_name,
                  r.name AS room_name,
                  e.email AS manager_email, e.name AS manager_name
         FROM devices d
         JOIN rooms r ON d.room_id = r.id
         LEFT JOIN workstations w ON d.workstation_id = w.id
         JOIN employees e ON e.id = COALESCE(w.manager_employee_id, r.manager_employee_id)
         WHERE d.id = $1 AND e.email IS NOT NULL",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .map(|row| MatchInfo {
        device_name: row.get("device_name"),
        workstation_name: row.get("workstation_name"),
        room_name: row.get("room_name"),
        manager_email: row.get("manager_email"),
        manager_name: row.get("manager_name"),
    });

    let Some(info) = info else {
        // 责任人未配置邮箱：无需通知
        log_debug!(
            "log.smtp.device_notify_skipped",
            device_id = device_id.to_string()
        );
        return Ok(());
    };

    let ip_rows = sqlx::query(
        r"SELECT host(m.ip_address) AS ip, COALESCE(nc.name, '') AS network
           FROM interface_ips m
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

    // 责任来源决定措辞：工位管理人收到「工位」，房间管理人收到「房间」
    let (space_kind, space_name) = match &info.workstation_name {
        Some(workstation) => ("工位", workstation),
        None => ("房间", &info.room_name),
    };

    let body = format!(
        "{manager_greeting}\n\n设备已登记到您负责的{space_kind}，IP 信息如下：\n设备名称：{}\n所属{space_kind}：{space_name}\n{}\n\n此邮件由 FOIMS 系统自动发送，请勿回复。\n",
        info.device_name,
        ip_lines.join("\n")
    );

    let subject = format!(
        "设备 IP 通知 - {}（{space_kind}: {space_name}）",
        info.device_name
    );

    // SMTP 未配置属于业务状态：静默跳过，不产生错误日志刷屏；
    // 其余发送失败需以 warn 呈现，便于排查通知未送达问题
    if let Err(e) =
        foims_auth::smtp::send_email_async(pool, &info.manager_email, &subject, &body).await
    {
        if matches!(&e, AppError::Validation(m) if m.key() == "server.smtp.not_configured") {
            log_debug!(
                "log.smtp.device_notify_skipped",
                device_id = device_id.to_string()
            );
        } else {
            log_warn!("log.smtp.device_notify_failed", error = e);
        }
    }
    Ok(())
}
