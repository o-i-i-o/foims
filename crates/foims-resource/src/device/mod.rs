//! 设备子模块（CRUD/统一端口接口/网卡/MAC/LLDP/SNMP/模板/通知，
//! 以及第二层网络侧台账 ENTITY/VLAN/CDP/LAG/PoE）。

pub mod cdp;
pub mod crud;
pub mod entity;
pub mod host_monitor;
pub mod interface;
pub mod lag;
pub mod lldp;
pub mod mac;
pub mod nic;
pub mod notify;
pub mod poe;
pub mod template;
pub mod vlan;

pub use cdp::*;
pub use crud::*;
pub use entity::*;
pub use host_monitor::sync_snmp_host_ip;
pub use interface::*;
pub use lag::*;
pub use lldp::*;
pub use mac::*;
pub use nic::*;
pub use poe::*;
pub use template::*;
pub use vlan::*;

use foims_common::{AppError, msg};

/// 设备类型白名单，与数据库 CHECK 约束保持一致
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

/// 设备类型白名单校验，越界返回 400 并附有效取值列表。
fn validate_device_type(device_type: &str) -> Result<(), AppError> {
    if VALID_DEVICE_TYPES.contains(&device_type) {
        Ok(())
    } else {
        Err(AppError::Validation(
            msg("server.device.type_invalid").with("types", VALID_DEVICE_TYPES.join(", ")),
        ))
    }
}
