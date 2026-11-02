//! 设备 IP 信息邮件通知。
//!
//! 责任人只取「设备所在工位的管理人」：房间本身没有责任人，责任下沉到工位
//! （`workstations.manager_employee_id`，可为空），因此未挂工位（如机房内
//! 的网络设备）或工位未配管理人的设备不产生通知。通知是尽力而为的旁路
//! 动作：SMTP 未配置或发送失败仅记录日志，不影响设备操作结果。

use foims_common::AppError;
use foims_common::{log_debug, log_warn};
use foims_notify::LocalizedText;
use sqlx::Row;
use uuid::Uuid;

/// 设备空间归属与责任人信息。
///
/// 责任人是设备所在工位的管理人；联不到带邮箱的工位管理人即无通知对象。
struct MatchInfo {
    device_name: String,
    workstation_name: String,
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
    // 责任人只来自工位管理人（可为空：未配管理人的工位无收件人），
    // 故以 JOIN 而非 LEFT JOIN：联不到管理人即视为无需通知
    let info = sqlx::query(
        r"SELECT d.name AS device_name,
                  w.name AS workstation_name,
                  e.email AS manager_email, e.name AS manager_name
         FROM devices d
         JOIN workstations w ON d.workstation_id = w.id
         JOIN employees e ON e.id = w.manager_employee_id
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
        // 设备未挂工位 / 工位未配管理人 / 管理人未填邮箱：无需通知
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

    // 中英双语文案：收件人为工位管理人，不参与「通知配置」卡片上的语言
    // 设置（该设置仅作用于通知邮件收件人与 Webhook 目标），收件人语言不可
    // 知，故正文以中英双语并列呈现，标题同口径
    let ip_lines = |label_ip: &str, label_subnet: &str, subnet_end: &str| -> Vec<String> {
        ip_rows
            .iter()
            .map(|row| {
                let ip: String = row.get("ip");
                let network: String = row.get("network");
                if network.is_empty() {
                    format!("{label_ip}{ip}")
                } else {
                    format!("{label_ip}{ip}{label_subnet}{network}{subnet_end}")
                }
            })
            .collect()
    };
    let zh_lines = ip_lines("IP 地址：", "（网段：", "）");
    let en_lines = ip_lines("IP address: ", " (subnet: ", ")");

    let greeting = |name: &Option<String>, zh: bool| -> String {
        match (zh, name) {
            (true, Some(name)) => format!("{name}，您好："),
            (true, None) => "您好：".to_string(),
            (false, Some(name)) => format!("Dear {name},"),
            (false, None) => "Hello,".to_string(),
        }
    };

    let message = LocalizedText {
        zh: format!(
            "{}\n\n设备已登记到您负责的工位，IP 信息如下：\n设备名称：{}\n所属工位：{}\n{}\n\n此邮件由 FOIMS 系统自动发送，请勿回复。\n",
            greeting(&info.manager_name, true),
            info.device_name,
            info.workstation_name,
            zh_lines.join("\n")
        ),
        en: format!(
            "{}\n\nThe device has been registered to the workstation you manage. IP information:\nDevice name: {}\nWorkstation: {}\n{}\n\nThis email was sent automatically by the FOIMS system, please do not reply.\n",
            greeting(&info.manager_name, false),
            info.device_name,
            info.workstation_name,
            en_lines.join("\n")
        ),
    };
    let title = LocalizedText {
        zh: format!("设备 IP 通知 - {}（工位: {}）", info.device_name, info.workstation_name),
        en: format!(
            "Device IP Notice - {} (Workstation: {})",
            info.device_name, info.workstation_name
        ),
    };

    let subject = format!("{} / {}", title.zh, title.en);
    let body = format!("{}\n\n----------\n\n{}", message.zh, message.en);

    // SMTP 未配置属于业务状态：静默跳过，不产生错误日志刷屏；
    // 其余发送失败需以 warn 呈现，便于排查通知未送达问题
    if let Err(e) =
        foims_notify::smtp::send_email_async(pool, &info.manager_email, &subject, &body).await
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
