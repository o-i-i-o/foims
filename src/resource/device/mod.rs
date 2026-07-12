pub mod connection;
pub mod crud;
pub mod lldp;
pub mod mac;
pub mod port;
pub mod snmp;
pub mod template;

pub use connection::*;
pub use crud::*;
pub use lldp::*;
pub use mac::*;
pub use port::*;
pub use snmp::*;
pub use template::*;

use crate::error::AppError;

/// Valid device types matching the database CHECK constraint
const VALID_DEVICE_TYPES: [&str; 10] = [
    "pc",
    "laptop",
    "printer",
    "server",
    "network_device",
    "switch",
    "camera",
    "phone",
    "ap",
    "other",
];

fn validate_device_type(device_type: &str) -> Result<(), AppError> {
    if VALID_DEVICE_TYPES.contains(&device_type) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "设备类型必须是以下之一: {}",
            VALID_DEVICE_TYPES.join(", ")
        )))
    }
}
