pub mod crud;
pub mod interface;
pub mod lldp;
pub mod mac;
pub mod network_card;
pub mod snmp;
pub mod switch_port;
pub mod template;

pub use crud::*;
pub use interface::*;
pub use lldp::*;
pub use mac::*;
pub use network_card::*;
pub use snmp::*;
pub use switch_port::*;
pub use template::*;

use crate::error::AppError;

/// Valid device types matching the database CHECK constraint
const VALID_DEVICE_TYPES: [&str; 9] = [
    "pc",
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
        Err(AppError::Validation(format!(
            "设备类型必须是以下之一: {}",
            VALID_DEVICE_TYPES.join(", ")
        )))
    }
}
