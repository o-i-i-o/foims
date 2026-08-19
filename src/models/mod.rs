//! 领域模型定义。
//!
//! 按资源域拆分为子模块（layout/user/network/room/...），统一经本模块
//! glob 再导出，调用方继续使用 `crate::models::X` 路径。校验函数与
//! Serde 辅助函数为各子模块共用，保留在此。

use serde::Deserialize;
use validator::ValidationError;

// ==================== 验证函数 ====================

pub fn validate_device_type_string(device_type: &str) -> Result<(), ValidationError> {
    match device_type {
        "pc" | "laptop" | "printer" | "server" | "network_device" | "switch" | "camera"
        | "phone" | "other" => Ok(()),
        // code 仅作错误标识；实际返回给前端的消息 key 由调用点的 message 属性覆盖
        _ => Err(ValidationError::new(
            "server.device.validation.type_invalid",
        )),
    }
}

pub fn validate_device_type_option(device_type: &&String) -> Result<(), ValidationError> {
    validate_device_type_string(device_type)
}

pub fn validate_room_type_string(room_type: &str) -> Result<(), ValidationError> {
    let room_type_lower = room_type.to_lowercase();
    if matches!(
        room_type_lower.as_str(),
        "office" | "lobby" | "reception" | "data_center" | "telecom_closet" | "other"
    ) {
        Ok(())
    } else {
        Err(ValidationError::new("server.room.validation.type_invalid"))
    }
}

pub fn validate_room_type_option(room_type: &&String) -> Result<(), ValidationError> {
    validate_room_type_string(room_type)
}

pub fn validate_role(role: &str) -> Result<(), ValidationError> {
    match role {
        "admin" | "user" => Ok(()),
        _ => Err(ValidationError::new("server.user.validation.role_invalid")),
    }
}

pub fn validate_role_option(role: &&String) -> Result<(), ValidationError> {
    validate_role(role)
}

pub fn validate_dns_count(dns_list: &[String]) -> Result<(), ValidationError> {
    if dns_list.len() > 5 {
        return Err(ValidationError::new("dns_count_exceeded"));
    }
    Ok(())
}

pub fn validate_ip_address(ip: &str) -> Result<(), ValidationError> {
    if ip.parse::<std::net::IpAddr>().is_err() {
        return Err(ValidationError::new("invalid_ip_address"));
    }
    Ok(())
}

// ==================== Serde 辅助函数 ====================

/// 反序列化 Option<Option<T>>，区分三种状态：
/// - 字段缺失 → None（不修改）
/// - JSON null → Some(None)（清除值）
/// - JSON 值 → Some(Some(value))（设置新值）
pub(crate) fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    struct OptionOptionVisitor<T>(std::marker::PhantomData<T>);

    impl<'de, T> serde::de::Visitor<'de> for OptionOptionVisitor<T>
    where
        T: Deserialize<'de>,
    {
        type Value = Option<Option<T>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "null 或一个值")
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(Some(None))
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(Some(None))
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            T::deserialize(deserializer).map(|v| Some(Some(v)))
        }
    }

    deserializer.deserialize_option(OptionOptionVisitor(std::marker::PhantomData))
}

// ==================== API 响应模型 ====================

pub use ipma_common::ApiResponse;

pub mod cabinet;
pub mod cable_link;
pub mod device;
pub mod ip;
pub mod layout;
pub mod log;
pub mod net_outlet;
pub mod network;
pub mod organization;
pub mod patch_panel;
pub mod position;
pub mod room;
pub mod user;
pub mod workstation;

pub use cabinet::*;
pub use cable_link::*;
pub use device::*;
pub use ip::*;
pub use layout::*;
pub use log::*;
pub use net_outlet::*;
pub use network::*;
pub use organization::*;
pub use patch_panel::*;
pub use position::*;
pub use room::*;
pub use user::*;
pub use workstation::*;
