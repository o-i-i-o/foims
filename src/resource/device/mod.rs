//! 设备子模块（CRUD/统一端口接口/网卡/MAC/LLDP/SNMP/模板/通知）。

pub mod crud;
pub mod interface;
pub mod lldp;
pub mod mac;
pub mod nic;
pub mod notify;
pub mod snmp;
pub mod template;

pub use crud::*;
pub use interface::*;
pub use lldp::*;
pub use mac::*;
pub use nic::*;
pub use snmp::*;
pub use template::*;

use crate::error::{AppError, msg};

/// Valid device types matching the database CHECK constraint
const VALID_DEVICE_TYPES: [&str; 9] = [
    "desktop",
    "laptop",
    "printer",
    "server",
    "network_device",
    "switch",
    "camera",
    "phone",
    "other",
];

fn validate_device_type(device_type: &str) -> Result<(), AppError> {
    if VALID_DEVICE_TYPES.contains(&device_type) {
        Ok(())
    } else {
        Err(AppError::Validation(
            msg("server.device.type_invalid").with("types", VALID_DEVICE_TYPES.join(", ")),
        ))
    }
}
