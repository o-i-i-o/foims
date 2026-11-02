//! 工位 MAC/IP 异动告警（IP 变更 / 设备更换）：站内通知 + 邮件/Webhook 外发。
//!
//! 属资源域业务：文案构造与工位名解析贴近 IP 台账业务语义，留在本 crate；
//! 站内通知原语与邮件/Webhook 渠道外发经 foims-notify（通知域 crate）完成。

use uuid::Uuid;

use foims_common::{log_error, log_info, log_warn};
use foims_notify::{
    NotifyKind, active_admin_ids, create_notification, encode_notification_content,
};

/// 解析工位名（站内通知与邮件文案用）；查询失败/缺行降级为 UUID 字符串。
async fn resolve_workstation_name(pool: &sqlx::PgPool, workstation_id: &Uuid) -> String {
    match sqlx::query_scalar::<_, String>("SELECT name FROM workstations WHERE id = $1")
        .bind(workstation_id)
        .fetch_optional(pool)
        .await
    {
        Ok(Some(name)) => name,
        Ok(None) => {
            log_warn!("log.workstation.not_found", id = workstation_id);
            workstation_id.to_string()
        }
        Err(e) => {
            log_warn!("log.workstation.query_name_failed", error = e);
            workstation_id.to_string()
        }
    }
}

/// IP 地址变更通知文案（标题, 正文），邮件与 Webhook 渠道共用。
///
/// MAC 为物理实体、IP 为虚拟实体：本通知表达“设备（{mac}）从登记 IP
/// 移动到了新的观测 IP”，`source_device` 为拉取到该观测的来源设备名。
fn ip_change_message(
    source_device: &str,
    workstation_name: &str,
    mac_address: &str,
    registered_ip: &str,
    observed_ip: &str,
) -> (String, String) {
    let title = format!("IP地址变更通知 - 工位: {workstation_name}");
    let body = format!(
        "尊敬的管理员：\n\n工位 {workstation_name} 的设备 IP 地址发生变更，详情如下：\n工位名称：{workstation_name}\n设备MAC：{mac_address}\n登记 IP：{registered_ip}\n观测 IP：{observed_ip}\n数据来源设备：{source_device}\n\n请确认此变更是否为授权操作。\n\n此致，\nFOIMS系统"
    );
    (title, body)
}

/// 工位 IP 上设备更换通知文案（登记 IP 不变、学到新 MAC），邮件与 Webhook 渠道共用。
///
/// 与 IP 变更通知同属“MAC/IP 对应关系异动”类，受 ip_change 开关门控；
/// `source_device` 为拉取到该观测的来源设备名。
fn device_swap_message(
    source_device: &str,
    workstation_name: &str,
    ip_address: &str,
    old_mac: &str,
    new_mac: &str,
) -> (String, String) {
    let title = format!("工位设备更换通知 - 工位: {workstation_name}");
    let body = format!(
        "尊敬的管理员：\n\n工位 {workstation_name} 的 IP {ip_address} 上设备更换，详情如下：\n工位名称：{workstation_name}\nIP地址：{ip_address}\n旧MAC地址：{old_mac}\n新MAC地址：{new_mac}\n数据来源设备：{source_device}\n\n请确认此变更是否为授权操作。\n\n此致，\nFOIMS系统"
    );
    (title, body)
}

/// 工位设备 IP 变更告警（MAC 物理设备从登记 IP 移到了新的观测 IP）：
/// 站内通知全部管理员；邮件与 Webhook 按「通知配置」中 IP 地址变更的订阅
/// 决定是否发送（SMTP 未配置时降级）。`source_device` 为拉取到该观测的来源设备名。
pub async fn send_ip_change_notification(
    pool: &sqlx::PgPool,
    workstation_id: &Uuid,
    mac_address: &str,
    registered_ip: &str,
    observed_ip: &str,
    source_device: &str,
) -> Result<(), sqlx::Error> {
    let workstation_name = resolve_workstation_name(pool, workstation_id).await;

    let content = encode_notification_content(
        "server.notification.ip_change.body",
        &[
            ("workstation", &workstation_name),
            ("mac", mac_address),
            ("registered_ip", registered_ip),
            ("observed_ip", observed_ip),
            ("device", source_device),
        ],
    );
    let admin_ids = active_admin_ids(pool).await?;
    for admin_id in &admin_ids {
        create_notification(
            pool,
            "server.notification.ip_change.title",
            &content,
            "ip_change",
            Some(admin_id),
        )
        .await?;
    }
    log_info!(
        "log.ip_change.notification_created",
        workstation = workstation_name,
        observed_ip = observed_ip,
        recipients = admin_ids.len()
    );

    // 邮件 + Webhook 双渠道按订阅过滤外发：邮件结果记日志，Webhook 内部
    // 按各目标订阅过滤逐条尽力推送
    let (title, body) = ip_change_message(
        source_device,
        &workstation_name,
        mac_address,
        registered_ip,
        observed_ip,
    );
    match foims_notify::push(pool, NotifyKind::IpChange, &title, &body).await {
        Ok(()) => log_info!("log.ip_change.email_sent", workstation = workstation_name),
        Err(e) => log_error!(
            "log.ip_change.email_send_failed",
            workstation = workstation_name,
            error = e
        ),
    }

    Ok(())
}

/// 工位 IP 上设备更换告警（登记 IP 不变、学到新 MAC，即旧“MAC 变更”语义）。
/// 与 IP 变更同属“MAC/IP 对应关系异动”类，邮件同样受 ip_change 开关门控。
/// `source_device` 为拉取到该观测的来源设备名。
pub async fn send_device_swap_notification(
    pool: &sqlx::PgPool,
    workstation_id: &Uuid,
    ip_address: &str,
    old_mac: &str,
    new_mac: &str,
    source_device: &str,
) -> Result<(), sqlx::Error> {
    let workstation_name = resolve_workstation_name(pool, workstation_id).await;

    let content = encode_notification_content(
        "server.notification.device_swap.body",
        &[
            ("workstation", &workstation_name),
            ("ip", ip_address),
            ("old_mac", old_mac),
            ("new_mac", new_mac),
            ("device", source_device),
        ],
    );
    let admin_ids = active_admin_ids(pool).await?;
    for admin_id in &admin_ids {
        create_notification(
            pool,
            "server.notification.device_swap.title",
            &content,
            "ip_change",
            Some(admin_id),
        )
        .await?;
    }
    log_info!(
        "log.ip_change.device_swap_created",
        workstation = workstation_name,
        ip = ip_address,
        recipients = admin_ids.len()
    );

    // 邮件 + Webhook 双渠道按订阅过滤外发：邮件结果记日志，Webhook 内部
    // 按各目标订阅过滤逐条尽力推送
    let (title, body) = device_swap_message(
        source_device,
        &workstation_name,
        ip_address,
        old_mac,
        new_mac,
    );
    match foims_notify::push(pool, NotifyKind::IpChange, &title, &body).await {
        Ok(()) => log_info!(
            "log.ip_change.device_swap_email_sent",
            workstation = workstation_name
        ),
        Err(e) => log_error!(
            "log.ip_change.device_swap_email_failed",
            workstation = workstation_name,
            error = e
        ),
    }

    Ok(())
}
