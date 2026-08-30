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
        // 等保三权分立：admin 系统管理员 / secadmin 安全管理员 / auditor 审计管理员 / user 普通用户
        "admin" | "secadmin" | "auditor" | "user" => Ok(()),
        _ => Err(ValidationError::new("server.user.validation.role_invalid")),
    }
}

pub fn validate_role_option(role: &&String) -> Result<(), ValidationError> {
    validate_role(role)
}

/// SNMP 版本枚举（devices.snmp_version 为 VARCHAR(3)，DB 无 CHECK）：
/// 非法值直写数据库报 500，此处前置拦截（db-schema-review 第五节）
pub fn validate_snmp_version_string(version: &str) -> Result<(), ValidationError> {
    match version {
        "v1" | "v2c" | "v3" => Ok(()),
        _ => Err(ValidationError::new(
            "server.device.validation.snmp_version_invalid",
        )),
    }
}

pub fn validate_snmp_version_option(version: &&String) -> Result<(), ValidationError> {
    validate_snmp_version_string(version)
}

pub fn validate_dns_count(dns_list: &[String]) -> Result<(), ValidationError> {
    if dns_list.len() > 5 {
        return Err(ValidationError::new("dns_count_exceeded"));
    }
    Ok(())
}

/// IPv4 DNS 列表逐条格式校验：每条都必须是合法 IPv4 地址。
/// 数量校验见 [`validate_dns_count`]，两者叠加使用。
pub fn validate_ipv4_dns_entries(dns_list: &[String]) -> Result<(), ValidationError> {
    if dns_list
        .iter()
        .all(|dns| dns.parse::<std::net::Ipv4Addr>().is_ok())
    {
        Ok(())
    } else {
        Err(ValidationError::new(
            "server.network.validation.dns_invalid",
        ))
    }
}

/// IPv6 DNS 列表逐条格式校验：每条都必须是合法 IPv6 地址。
/// 数量校验见 [`validate_dns_count`]，两者叠加使用。
pub fn validate_ipv6_dns_entries(dns_list: &[String]) -> Result<(), ValidationError> {
    if dns_list
        .iter()
        .all(|dns| dns.parse::<std::net::Ipv6Addr>().is_ok())
    {
        Ok(())
    } else {
        Err(ValidationError::new(
            "server.network.validation.dns_invalid",
        ))
    }
}

pub fn validate_ip_address(ip: &str) -> Result<(), ValidationError> {
    if ip.parse::<std::net::IpAddr>().is_err() {
        return Err(ValidationError::new("invalid_ip_address"));
    }
    Ok(())
}

/// Option 字段版 IP 地址校验（有值才校验，None 跳过）
pub fn validate_ip_address_option(ip: &&String) -> Result<(), ValidationError> {
    validate_ip_address(ip)
}

/// IP 版本仅允许 4 或 6（INET 写入侧与 ip_version 语义对齐）。
/// 数值字段（含 Option/双 Option 包裹）由 validator derive 按值传入
pub fn validate_ip_version_option(version: i16) -> Result<(), ValidationError> {
    if version == 4 || version == 6 {
        Ok(())
    } else {
        Err(ValidationError::new("invalid_ip_version"))
    }
}

/// IP 地址格式（IPv4/IPv6）校验：供 MAC/LLDP 记录等 INET 列字段使用
pub fn validate_ip_address_string(value: &str) -> Result<(), ValidationError> {
    if value.parse::<std::net::IpAddr>().is_ok() {
        Ok(())
    } else {
        Err(ValidationError::new("server.common.validation.ip_format"))
    }
}

/// MAC 地址格式校验：冒号/横线分隔的 6 组十六进制（如 aa:bb:cc:dd:ee:ff）
pub fn validate_mac_address_string(value: &str) -> Result<(), ValidationError> {
    let normalized = value.replace('-', ":");
    let parts: Vec<&str> = normalized.split(':').collect();
    let valid = parts.len() == 6
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 2 && p.bytes().all(|b| b.is_ascii_hexdigit()));
    if valid {
        Ok(())
    } else {
        Err(ValidationError::new("server.device.validation.mac_format"))
    }
}

/// Option 字段版 MAC 地址格式校验（有值才校验，None 跳过）：
/// 供 DeviceInterfaceCreate.mac_address 等可空 MAC 字段使用
pub fn validate_mac_address_option(value: &&String) -> Result<(), ValidationError> {
    validate_mac_address_string(value)
}

/// VLAN ID / Trunk Native VLAN ID 取值范围（1..=4094，IEEE 802.1Q）。
/// 数值字段（含 Option/双 Option 包裹）由 validator derive 按值传入
pub fn validate_vlan_id_option(vlan_id: i32) -> Result<(), ValidationError> {
    if (1..=4094).contains(&vlan_id) {
        Ok(())
    } else {
        Err(ValidationError::new("vlan_id_range"))
    }
}

/// 密码字节长度上限：bcrypt 仅使用前 72 字节，超长部分被静默截断——
/// 前 72 字节相同的口令将得到等价哈希，必须在入库前拒绝（按字节计）。
pub fn validate_password_max_bytes(password: &str) -> Result<(), ValidationError> {
    if password.len() <= 72 {
        Ok(())
    } else {
        Err(ValidationError::new("password_max_length"))
    }
}

/// Option 字段版密码字节长度校验（有值才校验，None 跳过）
pub fn validate_password_max_bytes_option(password: &&String) -> Result<(), ValidationError> {
    validate_password_max_bytes(password)
}

// ==================== 双层 Option 长度校验 ====================
//
// validator 的 #[validate(length)] 对 Option<Option<String>> 字段完全不生效
// （security-review 第六节）：外层 Some 时不校验内层字符串长度，超长值直写
// 数据库撞 VARCHAR 上限报 500。custom 函数会被 derive 自动解包双层 Option
// （Some(None) 跳过校验、Some(Some(v)) 传入 &&String），以下按此签名实现。

/// CableLinkUpdate.cable_label（VARCHAR(50)）
pub fn validate_cable_label_opt(value: &&String) -> Result<(), ValidationError> {
    validate_length_str(value, 50, "server.cable_link.validation.cable_label_length")
}

/// 设备接口 MAC 地址（VARCHAR(20)）
pub fn validate_iface_mac_opt(value: &&String) -> Result<(), ValidationError> {
    validate_length_str(value, 20, "server.device.validation.mac_length")
}

/// 通用描述字段（VARCHAR/TEXT 上限 255）
pub fn validate_description_opt(value: &&String) -> Result<(), ValidationError> {
    validate_length_str(value, 255, "server.common.validation.description_length")
}

/// WorkstationUpdate.manager（VARCHAR(50)）
pub fn validate_manager_opt(value: &&String) -> Result<(), ValidationError> {
    validate_length_str(value, 50, "server.workstation.validation.manager_length")
}

/// EmployeeUpdate.phone（VARCHAR(20)）
pub fn validate_phone_opt(value: &&String) -> Result<(), ValidationError> {
    validate_length_str(value, 20, "server.employee.validation.phone_length")
}

/// EmployeeUpdate.email：双层 Option 包裹时 custom 函数自动解包，
/// Some(Some(v)) 才做格式校验（与 #[validate(email)] 单层口径一致）
pub fn validate_email_opt(value: &&String) -> Result<(), ValidationError> {
    use validator::ValidateEmail;
    if value.validate_email() {
        Ok(())
    } else {
        Err(ValidationError::new(
            "server.common.validation.email_format",
        ))
    }
}

/// 员工邮箱校验（EmployeeCreate/EmployeeUpdate 共用）：trim 后为空串
/// 先放行——handler 侧 blank_to_none 会把空白值规范化为 NULL 入库；
/// 非空才做邮箱格式校验，避免空串先撞 #[validate(email)] 报格式错误。
/// 单层与双层 Option 包裹的 custom 函数均按 &&String 传入。
pub fn validate_email_blankable_opt(value: &&String) -> Result<(), ValidationError> {
    use validator::ValidateEmail;
    if value.trim().is_empty() || value.validate_email() {
        Ok(())
    } else {
        Err(ValidationError::new(
            "server.common.validation.email_format",
        ))
    }
}

/// CableLinkUpdate.length_m：0..=10000 米。
/// 双层 Option 的数值字段由 derive 解包后按值传入（Some(None) 已被跳过）；
/// NaN 不满足区间比较即拒绝。
pub fn validate_length_m_opt(len: f64) -> Result<(), ValidationError> {
    if (0.0..=10000.0).contains(&len) {
        Ok(())
    } else {
        Err(ValidationError::new(
            "server.cable_link.validation.length_m_range",
        ))
    }
}

/// IP 状态取值白名单：active（使用中）/ inactive（停用）/ reserved（保留）。
/// ips.status 为 VARCHAR(20) 且 DB 无 CHECK 约束，应用层前置拦截非法值；
/// 合法值集合与前端 formatter 的状态文案键保持一致。
pub fn validate_ip_status_string(status: &str) -> Result<(), ValidationError> {
    match status {
        "active" | "inactive" | "reserved" => Ok(()),
        _ => Err(ValidationError::new("server.ip.validation.status_invalid")),
    }
}

/// Option 字段版 IP 状态白名单（有值才校验，None 跳过）
pub fn validate_ip_status_option(status: &&String) -> Result<(), ValidationError> {
    validate_ip_status_string(status)
}

/// DeviceUpdate.hostname（VARCHAR(100)）
pub fn validate_hostname_opt(value: &&String) -> Result<(), ValidationError> {
    validate_length_str(value, 100, "server.device.validation.hostname_length")
}

/// DeviceUpdate.snmp_community（VARCHAR(100)）
pub fn validate_snmp_community_opt(value: &&String) -> Result<(), ValidationError> {
    validate_length_str(value, 100, "server.device.validation.snmp_community_length")
}

/// DeviceUpdate.snmp_username（VARCHAR(50)）
pub fn validate_snmp_username_opt(value: &&String) -> Result<(), ValidationError> {
    validate_length_str(value, 50, "server.device.validation.snmp_username_length")
}

/// DeviceUpdate.snmp_auth_protocol（VARCHAR(10)）
pub fn validate_snmp_auth_protocol_opt(value: &&String) -> Result<(), ValidationError> {
    validate_length_str(
        value,
        10,
        "server.device.validation.snmp_auth_protocol_length",
    )
}

/// DeviceUpdate.snmp_auth_password（VARCHAR(100)）
pub fn validate_snmp_auth_password_opt(value: &&String) -> Result<(), ValidationError> {
    validate_length_str(
        value,
        100,
        "server.device.validation.snmp_auth_password_length",
    )
}

/// DeviceUpdate.snmp_priv_protocol（VARCHAR(10)）
pub fn validate_snmp_priv_protocol_opt(value: &&String) -> Result<(), ValidationError> {
    validate_length_str(
        value,
        10,
        "server.device.validation.snmp_priv_protocol_length",
    )
}

/// DeviceUpdate.snmp_priv_password（VARCHAR(100)）
pub fn validate_snmp_priv_password_opt(value: &&String) -> Result<(), ValidationError> {
    validate_length_str(
        value,
        100,
        "server.device.validation.snmp_priv_password_length",
    )
}

/// 设备接口速率（VARCHAR(20)）
pub fn validate_speed_opt(value: &&String) -> Result<(), ValidationError> {
    validate_length_str(value, 20, "server.device.validation.speed_length")
}

fn validate_length_str(
    value: &str,
    max_chars: usize,
    key: &'static str,
) -> Result<(), ValidationError> {
    if value.chars().count() > max_chars {
        return Err(ValidationError::new(key));
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

pub use foims_common::ApiResponse;

pub mod cabinet;
pub mod cable_link;
pub mod device;
pub mod employee;
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
pub use employee::*;
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
        assert!(validate_role("secadmin").is_ok(), "等保安全管理员");
        assert!(validate_role("auditor").is_ok(), "等保审计管理员");
    }

    #[test]
    fn test_validate_role_invalid() {
        // 角色严格区分大小写，仅允许 admin/secadmin/auditor/user
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

    // ---------- VLAN / IP 版本 / 密码字节上限校验 ----------

    #[test]
    fn test_validate_vlan_id_option_range() {
        // 1..=4094 合法，越界拒绝（数值字段按值传入）
        for ok_vlan in [1i32, 100, 4094] {
            assert!(
                validate_vlan_id_option(ok_vlan).is_ok(),
                "VLAN {ok_vlan} 应合法"
            );
        }
        for bad_vlan in [0i32, -1, 4095, 65535] {
            assert!(
                validate_vlan_id_option(bad_vlan).is_err(),
                "VLAN {bad_vlan} 应被拒绝"
            );
        }
    }

    #[test]
    fn test_validate_ip_version_option() {
        // 仅 4 / 6 合法（数值字段按值传入）
        assert!(validate_ip_version_option(4i16).is_ok());
        assert!(validate_ip_version_option(6i16).is_ok());
        for bad in [0i16, 5, -1, 7] {
            assert!(
                validate_ip_version_option(bad).is_err(),
                "版本 {bad} 应被拒绝"
            );
        }
    }

    #[test]
    fn test_validate_ip_address_option_delegates() {
        let ok_value = String::from("192.168.1.1");
        let ok_ref: &String = &ok_value;
        assert!(validate_ip_address_option(&ok_ref).is_ok());

        let bad_value = String::from("10.0.0.1/24");
        let bad_ref: &String = &bad_value;
        assert!(validate_ip_address_option(&bad_ref).is_err());
    }

    #[test]
    fn test_validate_password_max_bytes() {
        // 恰 72 字节合法（bcrypt 输入上限），73 字节拒绝
        assert!(validate_password_max_bytes(&"a".repeat(72)).is_ok());
        assert!(validate_password_max_bytes(&"a".repeat(73)).is_err());
        // 多字节字符按字节计
        assert!(
            validate_password_max_bytes(&"密".repeat(24)).is_ok(),
            "72 字节汉字应合法"
        );
        assert!(
            validate_password_max_bytes(&"密".repeat(25)).is_err(),
            "75 字节汉字应拒绝"
        );
    }

    // ---------- 新增双层 Option / 白名单校验函数 ----------

    #[test]
    fn test_validate_length_m_opt_range() {
        // 双层 Option 数值字段按值传入：0 与 10000 合法，负数/越上限/NaN 拒绝
        assert!(validate_length_m_opt(0.0).is_ok());
        assert!(validate_length_m_opt(12.5).is_ok());
        assert!(validate_length_m_opt(10000.0).is_ok());
        assert!(validate_length_m_opt(-0.1).is_err());
        assert!(validate_length_m_opt(10000.1).is_err());
        assert!(validate_length_m_opt(f64::NAN).is_err());
    }

    #[test]
    fn test_validate_email_opt() {
        // Some(Some(v)) 解包后按引用传入：合法与非法邮箱各一
        assert!(validate_email_opt(&&"a@b.com".to_string()).is_ok());
        assert!(validate_email_opt(&&"not-an-email".to_string()).is_err());
    }

    #[test]
    fn test_validate_ip_status_option_whitelist() {
        // 白名单：active / inactive / reserved
        for ok_status in ["active", "inactive", "reserved"] {
            assert!(
                validate_ip_status_option(&&ok_status.to_string()).is_ok(),
                "状态 {ok_status} 应合法"
            );
        }
        for bad_status in ["", "enabled", "ACTIVE"] {
            assert!(
                validate_ip_status_option(&&bad_status.to_string()).is_err(),
                "状态 {bad_status} 应被拒绝"
            );
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
