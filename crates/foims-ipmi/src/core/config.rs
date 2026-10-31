//! IPMI 设备配置只读查询（凭据行与探测用 IP，口径与 SNMP 域一致，
//! 不依赖资源域代码，供 IPMI 域内采集与 handler 使用）。

use thiserror::Error;
use uuid::Uuid;

use foims_common::{AppError, AppMessage, msg};

use super::DeviceForIpmi;

#[derive(Error, Debug)]
pub enum IpmiConfigError {
    #[error("{}", .0.key())]
    NotFound(AppMessage),
    #[error("{}", .0.key())]
    Database(AppMessage),
}

impl From<IpmiConfigError> for AppError {
    fn from(e: IpmiConfigError) -> Self {
        match e {
            IpmiConfigError::NotFound(m) => AppError::NotFound(m),
            IpmiConfigError::Database(m) => AppError::Database(m),
        }
    }
}

/// 读设备 IPMI 配置行与探测用 IP（IP 口径见 [`get_device_ip_address`]）。
pub async fn get_device_ipmi_config(
    pool: &sqlx::PgPool,
    device_id: &Uuid,
) -> Result<(DeviceForIpmi, Option<String>), IpmiConfigError> {
    let device = sqlx::query_as::<_, DeviceForIpmi>(
        r"SELECT
            id, name, ipmi_username, ipmi_password,
            ipmi_port, ipmi_auth_algo
        FROM devices WHERE id = $1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| IpmiConfigError::Database(msg("server.error.database").with("error", e)))?
    .ok_or_else(|| IpmiConfigError::NotFound(msg("server.device.not_found")))?;

    let ip_address = get_device_ip_address(pool, device_id).await?;

    Ok((device, ip_address))
}

/// 设备探测用 IP：该设备网口的 IP 按 `created_at` 升序取首个（多地址时
/// 口径固定为最早登记的一条），未登记返回 None。与 SNMP 域同口径。
pub async fn get_device_ip_address(
    pool: &sqlx::PgPool,
    device_id: &Uuid,
) -> Result<Option<String>, IpmiConfigError> {
    let ip_address: Option<String> = sqlx::query_scalar(
        r"SELECT host(i.ip_address) FROM interface_ips i
           JOIN device_interfaces di ON i.device_interface_id = di.id
           WHERE di.device_id = $1
           ORDER BY i.created_at LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| IpmiConfigError::Database(msg("server.error.database").with("error", e)))?
    .flatten();

    Ok(ip_address.filter(|ip| !ip.is_empty()))
}
