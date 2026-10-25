//! 设备 SNMP 配置与 IP 的只读查询（仅读 devices/interface_ips 表，
//! 不依赖资源域代码，供 SNMP 域内采集与 handler 使用）。

use thiserror::Error;
use uuid::Uuid;

use foims_common::{AppError, AppMessage, msg};

use super::DeviceForSnmp;

#[derive(Error, Debug)]
pub enum SwitchConfigError {
    #[error("{}", .0.key())]
    NotFound(AppMessage),
    #[error("{}", .0.key())]
    Database(AppMessage),
}

impl From<SwitchConfigError> for AppError {
    fn from(e: SwitchConfigError) -> Self {
        match e {
            SwitchConfigError::NotFound(m) => AppError::NotFound(m),
            SwitchConfigError::Database(m) => AppError::Database(m),
        }
    }
}

impl From<SwitchConfigError> for super::SnmpError {
    fn from(e: SwitchConfigError) -> Self {
        match e {
            SwitchConfigError::NotFound(m) | SwitchConfigError::Database(m) => {
                super::SnmpError::Message(m.log_string())
            }
        }
    }
}

pub async fn get_device_snmp_config(
    pool: &sqlx::PgPool,
    device_id: &Uuid,
) -> Result<(DeviceForSnmp, Option<String>), SwitchConfigError> {
    let switch = sqlx::query_as::<_, DeviceForSnmp>(
        r"SELECT
            id, name, snmp_version, snmp_community,
            snmp_username, snmp_auth_protocol,
            snmp_auth_password, snmp_priv_protocol,
            snmp_priv_password, snmp_port
        FROM devices WHERE id = $1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SwitchConfigError::Database(msg("server.error.database").with("error", e)))?
    .ok_or_else(|| SwitchConfigError::NotFound(msg("server.device.not_found")))?;

    let ip_address = get_device_ip_address(pool, device_id).await?;

    Ok((switch, ip_address))
}

pub async fn get_device_ip_address(
    pool: &sqlx::PgPool,
    device_id: &Uuid,
) -> Result<Option<String>, SwitchConfigError> {
    let ip_address: Option<String> = sqlx::query_scalar(
        r"SELECT host(i.ip_address) FROM interface_ips i
           JOIN device_interfaces di ON i.device_interface_id = di.id
           WHERE di.device_id = $1
           ORDER BY i.created_at LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SwitchConfigError::Database(msg("server.error.database").with("error", e)))?
    .flatten();

    Ok(ip_address.filter(|ip| !ip.is_empty()))
}
