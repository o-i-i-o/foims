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
        "desktop" | "laptop" | "printer" | "server" | "network_device" | "switch" | "camera"
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

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// 用于验证 deserialize_some 三态语义的最小测试载体
    #[derive(Debug, Deserialize, PartialEq)]
    struct PatchDoc {
        #[serde(default, deserialize_with = "deserialize_some")]
        value: Option<Option<u32>>,
    }

    // ---------- 设备类型校验 ----------

    #[test]
    fn test_validate_device_type_string_all_valid() {
        // 全部合法设备类型逐一通过
        for valid in [
            "desktop",
            "laptop",
            "printer",
            "server",
            "network_device",
            "switch",
            "camera",
            "phone",
            "other",
        ] {
            assert!(
                validate_device_type_string(valid).is_ok(),
                "类型 {valid} 应合法"
            );
        }
    }

    #[test]
    fn test_validate_device_type_string_invalid() {
        // 非法类型 / 空串 / 大小写变体均被拒绝（设备类型区分大小写）
        for invalid in ["vm", "", "Server", "NETWORK_DEVICE", "desktop "] {
            let err = validate_device_type_string(invalid)
                .err()
                .unwrap_or_else(|| panic!("类型 {invalid} 应被拒绝"));
            assert_eq!(err.code, "server.device.validation.type_invalid");
        }
    }

    #[test]
    fn test_validate_device_type_option_delegates() {
        // option 变体直接复用 string 校验逻辑
        let ok_value = String::from("server");
        let ok_ref: &String = &ok_value;
        assert!(validate_device_type_option(&ok_ref).is_ok());

        let bad_value = String::from("virtual_machine");
        let bad_ref: &String = &bad_value;
        assert!(validate_device_type_option(&bad_ref).is_err());
    }

    // ---------- 房间类型校验（大小写不敏感） ----------

    #[test]
    fn test_validate_room_type_string_valid() {
        // 全部合法房间类型通过
        for valid in [
            "office",
            "lobby",
            "reception",
            "data_center",
            "telecom_closet",
            "other",
        ] {
            assert!(
                validate_room_type_string(valid).is_ok(),
                "类型 {valid} 应合法"
            );
        }
    }

    #[test]
    fn test_validate_room_type_string_case_insensitive() {
        // 房间类型先转小写再比较，任意大小写变体均通过
        for valid in [
            "OFFICE",
            "Office",
            "DATA_CENTER",
            "Data_Center",
            "TELECOM_CLOSET",
            "OTHER",
        ] {
            assert!(
                validate_room_type_string(valid).is_ok(),
                "类型 {valid} 应因大小写不敏感而合法"
            );
        }
    }

    #[test]
    fn test_validate_room_type_string_invalid() {
        // 非法房间类型被拒绝，且错误 code 正确
        for invalid in ["kitchen", "", "warehouse", "datacenter"] {
            let err = validate_room_type_string(invalid)
                .err()
                .unwrap_or_else(|| panic!("类型 {invalid} 应被拒绝"));
            assert_eq!(err.code, "server.room.validation.type_invalid");
        }
    }

    #[test]
    fn test_validate_room_type_option_delegates() {
        let ok_value = String::from("Data_Center");
        let ok_ref: &String = &ok_value;
        assert!(validate_room_type_option(&ok_ref).is_ok());

        let bad_value = String::from("balcony");
        let bad_ref: &String = &bad_value;
        assert!(validate_room_type_option(&bad_ref).is_err());
    }

    // ---------- 角色校验（大小写敏感） ----------

    #[test]
    fn test_validate_role_valid() {
        assert!(validate_role("admin").is_ok());
        assert!(validate_role("user").is_ok());
    }

    #[test]
    fn test_validate_role_invalid() {
        // 角色严格区分大小写，仅允许 admin/user
        for invalid in ["Admin", "ADMIN", "root", "", "superuser"] {
            let err = validate_role(invalid)
                .err()
                .unwrap_or_else(|| panic!("角色 {invalid} 应被拒绝"));
            assert_eq!(err.code, "server.user.validation.role_invalid");
        }
    }

    #[test]
    fn test_validate_role_option_delegates() {
        let ok_value = String::from("user");
        let ok_ref: &String = &ok_value;
        assert!(validate_role_option(&ok_ref).is_ok());

        let bad_value = String::from("guest");
        let bad_ref: &String = &bad_value;
        assert!(validate_role_option(&bad_ref).is_err());
    }

    // ---------- DNS 数量校验 ----------

    #[test]
    fn test_validate_dns_count_within_limit() {
        // 0 ~ 5 个 DNS 均合法
        for count in [0usize, 1, 3, 5] {
            let list: Vec<String> = (0..count).map(|i| format!("8.8.{i}.8")).collect();
            assert!(validate_dns_count(&list).is_ok(), "{count} 个 DNS 应合法");
        }
    }

    #[test]
    fn test_validate_dns_count_exceeded() {
        // 超过 5 个被拒绝
        let list: Vec<String> = (0..6).map(|i| format!("8.8.{i}.8")).collect();
        let err = validate_dns_count(&list)
            .err()
            .unwrap_or_else(|| panic!("6 个 DNS 应被拒绝"));
        assert_eq!(err.code, "dns_count_exceeded");
    }

    // ---------- IP 地址校验 ----------

    #[test]
    fn test_validate_ip_address_valid() {
        // 合法 IPv4
        for valid in ["0.0.0.0", "192.168.1.1", "255.255.255.255"] {
            assert!(validate_ip_address(valid).is_ok(), "地址 {valid} 应合法");
        }
        // 合法 IPv6（完整 / 压缩 / 回环 / IPv4 映射）
        for valid in [
            "2001:db8::1",
            "::1",
            "fe80::1",
            "::ffff:192.168.1.1",
            "2001:0db8:0000:0000:0000:0000:0000:0001",
        ] {
            assert!(validate_ip_address(valid).is_ok(), "地址 {valid} 应合法");
        }
    }

    #[test]
    fn test_validate_ip_address_invalid() {
        // 各类非法输入均被拒绝：缺段、多段、非数字、掩码后缀、空串、纯文本
        for invalid in [
            "1.2.3",
            "1.2.3.4.5",
            "256.1.1.1",
            "1.2.3.a",
            "1.2.3.4/24",
            "",
            "not-an-ip",
            "2001:db8:::1",
        ] {
            let err = validate_ip_address(invalid)
                .err()
                .unwrap_or_else(|| panic!("地址 {invalid} 应被拒绝"));
            assert_eq!(err.code, "invalid_ip_address");
        }
    }

    // ---------- deserialize_some 三态语义 ----------

    #[test]
    fn test_deserialize_some_field_missing_is_none() {
        // 字段缺失 → None（表示"不修改"）
        let doc: PatchDoc =
            serde_json::from_str("{}").unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(doc.value, None);
    }

    #[test]
    fn test_deserialize_some_json_null_is_some_none() {
        // JSON null → Some(None)（表示"清除值"）
        let doc: PatchDoc = serde_json::from_str(r#"{"value": null}"#)
            .unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(doc.value, Some(None));
    }

    #[test]
    fn test_deserialize_some_json_value_is_some_some() {
        // JSON 值 → Some(Some(value))（表示"设置新值"）
        let doc: PatchDoc =
            serde_json::from_str(r#"{"value": 7}"#).unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(doc.value, Some(Some(7)));
    }
}
