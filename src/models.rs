use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::{Validate, ValidationError};

// ==================== 验证函数 ====================

pub fn validate_device_type_string(device_type: &str) -> Result<(), ValidationError> {
    match device_type {
        "pc" | "laptop" | "printer" | "server" | "network_device" | "switch" | "camera"
        | "phone" | "other" => Ok(()),
        _ => Err(ValidationError::new(
            "设备类型必须是pc/laptop/printer/server/network_device/switch/camera/phone/other",
        )),
    }
}

fn validate_device_type_option(device_type: &&String) -> Result<(), ValidationError> {
    validate_device_type_string(device_type)
}

pub fn validate_room_type_string(room_type: &str) -> Result<(), ValidationError> {
    let room_type_lower = room_type.to_lowercase();
    if room_type_lower == "office"
        || room_type_lower == "data_center"
        || room_type_lower == "telecom_closet"
    {
        Ok(())
    } else {
        Err(ValidationError::new(
            "房间类型必须是office、data_center或telecom_closet",
        ))
    }
}

pub fn validate_room_type_option(room_type: &&String) -> Result<(), ValidationError> {
    validate_room_type_string(room_type)
}

fn validate_role(role: &str) -> Result<(), ValidationError> {
    match role {
        "admin" | "user" => Ok(()),
        _ => Err(ValidationError::new("角色必须是admin或user")),
    }
}

fn validate_role_option(role: &&String) -> Result<(), ValidationError> {
    validate_role(role)
}

pub fn validate_dns_count(dns_list: &[String]) -> Result<(), ValidationError> {
    if dns_list.len() > 5 {
        return Err(ValidationError::new("dns_count_exceeded"));
    }
    Ok(())
}

fn validate_ip_address(ip: &str) -> Result<(), ValidationError> {
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T, message: &str) -> Self {
        Self {
            success: true,
            message: message.to_string(),
            data: Some(data),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
        }
    }

    pub fn success_i18n(data: T, message_key: &str, lang: &str) -> Self {
        rust_i18n::set_locale(lang);
        let message = rust_i18n::t!(message_key);

        Self {
            success: true,
            message: message.to_string(),
            data: Some(data),
        }
    }

    #[must_use]
    pub fn error_i18n(message_key: &str, lang: &str) -> Self {
        rust_i18n::set_locale(lang);
        let message = rust_i18n::t!(message_key);

        Self {
            success: false,
            message: message.to_string(),
            data: None,
        }
    }
}

// ==================== 布局模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct LayoutSaveRequest {
    pub r#type: String,
    pub room_id: Option<Uuid>,
    pub network_region_id: Option<Uuid>,
    pub cabinet_id: Option<Uuid>,
    pub layout: Vec<LayoutItem>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct LayoutItem {
    pub id: Uuid,
    pub position: Position,
    pub element_type: String,
}

// ==================== 用户模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    pub status: bool,
    pub two_factor_enabled: bool,
    pub two_factor_verified: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UserCreate {
    #[validate(length(min = 3, max = 50, message = "用户名长度必须在3到50个字符之间"))]
    pub username: String,
    #[validate(length(min = 8, message = "密码长度必须至少8个字符"))]
    pub password: String,
    #[validate(email(message = "请输入有效的邮箱地址"))]
    pub email: String,
    #[validate(custom(function = "validate_role", message = "角色必须是admin或user"))]
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UserUpdate {
    #[validate(email(message = "请输入有效的邮箱地址"))]
    pub email: Option<String>,
    #[validate(custom(function = "validate_role_option", message = "角色必须是admin或user"))]
    pub role: Option<String>,
    pub status: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UserLogin {
    #[validate(length(min = 3, max = 50, message = "用户名长度必须在3到50个字符之间"))]
    pub username: String,
    #[validate(length(min = 8, message = "登录密码长度必须至少8个字符"))]
    pub password: String,
    pub remember_me: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct ForgotPasswordRequest {
    #[validate(email(message = "请输入有效的邮箱地址"))]
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct ResetPasswordRequest {
    #[validate(length(min = 1))]
    pub token: String,
    #[validate(length(min = 8, message = "密码长度必须至少8个字符"))]
    pub new_password: String,
}

// ==================== 2FA 模型 ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct TwoFactorConfigResponse {
    pub secret: String,
    pub qr_code: String,
    pub uri: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct TwoFactorLoginRequest {
    #[validate(length(min = 3, max = 50, message = "用户名长度必须在3到50个字符之间"))]
    pub username: String,
    #[validate(length(min = 8, message = "密码长度必须至少8个字符"))]
    pub password: Option<String>,
    #[validate(length(min = 6, max = 6, message = "验证码长度必须为6个字符"))]
    pub two_factor_code: String,
    pub remember_me: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SendTwoFactorCodeRequest {
    #[validate(length(min = 3, max = 50, message = "用户名长度必须在3到50个字符之间"))]
    pub username: String,
    pub password: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SendLoginCodeRequest {
    #[validate(email(message = "请输入有效的邮箱地址"))]
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct EmailLoginRequest {
    #[validate(email(message = "请输入有效的邮箱地址"))]
    pub email: String,
    #[validate(length(min = 6, max = 6, message = "验证码长度必须为6个字符"))]
    pub code: String,
    pub remember_me: Option<bool>,
}

// ==================== 网络区域模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct NetworkRegion {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    #[sqlx(json)]
    pub ipv4_cidrs: Option<Vec<String>>,
    #[sqlx(json)]
    pub ipv6_cidrs: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetworkRegionCreate {
    #[validate(length(min = 1, max = 20, message = "网络区域名称长度必须在1到20个字符之间"))]
    pub name: String,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    pub ipv4_cidrs: Option<Vec<String>>,
    pub ipv6_cidrs: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetworkRegionUpdate {
    #[validate(length(min = 1, max = 20, message = "网络区域名称长度必须在1到20个字符之间"))]
    pub name: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    pub ipv4_cidrs: Option<Vec<String>>,
    pub ipv6_cidrs: Option<Vec<String>>,
}

// ==================== 网络模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Network {
    pub id: Uuid,
    pub name: String,
    pub network_region_id: Uuid,
    pub network_region: String,
    pub ipv4_cidr: Option<String>,
    pub ipv6_cidr: Option<String>,
    pub ipv4_gateway: Option<String>,
    pub ipv6_gateway: Option<String>,
    #[sqlx(json)]
    pub ipv4_dns: Option<Vec<String>>,
    #[sqlx(json)]
    pub ipv6_dns: Option<Vec<String>>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct NetworkInfo {
    pub id: Uuid,
    pub name: String,
    pub network_region: String,
    pub network_region_id: Uuid,
    pub ipv4_cidr: Option<String>,
    pub ipv6_cidr: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetworkCreate {
    #[validate(length(min = 1, max = 50, message = "网络名称长度必须在1到50个字符之间"))]
    pub name: String,
    pub network_region_id: Uuid,
    pub ipv4_cidr: Option<String>,
    pub ipv6_cidr: Option<String>,
    pub ipv4_gateway: Option<String>,
    pub ipv6_gateway: Option<String>,
    #[validate(custom(function = "validate_dns_count", message = "DNS服务器数量不能超过5个"))]
    pub ipv4_dns: Option<Vec<String>>,
    #[validate(custom(function = "validate_dns_count", message = "DNS服务器数量不能超过5个"))]
    pub ipv6_dns: Option<Vec<String>>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetworkUpdate {
    #[validate(length(min = 1, max = 50, message = "网络名称长度必须在1到50个字符之间"))]
    pub name: Option<String>,
    pub network_region_id: Option<Uuid>,
    pub ipv4_cidr: Option<String>,
    pub ipv6_cidr: Option<String>,
    pub ipv4_gateway: Option<String>,
    pub ipv6_gateway: Option<String>,
    #[validate(custom(function = "validate_dns_count", message = "DNS服务器数量不能超过5个"))]
    pub ipv4_dns: Option<Vec<String>>,
    #[validate(custom(function = "validate_dns_count", message = "DNS服务器数量不能超过5个"))]
    pub ipv6_dns: Option<Vec<String>>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

// ==================== 房间模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Room {
    pub id: Uuid,
    pub name: String,
    pub room_type: String,
    pub org_id: Option<Uuid>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct RoomNetwork {
    pub id: Uuid,
    pub room_id: Uuid,
    pub network_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct RoomNetworkDetail {
    pub room_id: Uuid,
    pub room_name: String,
    pub room_type: String,
    pub description: Option<String>,
    pub network_id: Option<Uuid>,
    pub network_name: Option<String>,
    pub network_region: Option<String>,
    pub network_region_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RoomWithNetworks {
    pub id: Uuid,
    pub name: String,
    pub room_type: String,
    pub org_id: Option<Uuid>,
    pub org_name: Option<String>,
    pub description: Option<String>,
    pub networks: Vec<NetworkInfo>,
    pub workstation_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workstations: Option<Vec<WorkstationBrief>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cabinets: Option<Vec<CabinetBrief>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkstationBrief {
    pub id: Uuid,
    pub name: String,
    pub manager: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CabinetBrief {
    pub id: Uuid,
    pub name: String,
    pub capacity: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PositionBrief {
    pub id: Uuid,
    pub name: String,
    pub start_u: i32,
    pub end_u: i32,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RoomCreate {
    #[validate(length(min = 1, max = 50, message = "房间名称长度必须在1到50个字符之间"))]
    pub name: String,
    #[validate(custom(
        function = "validate_room_type_string",
        message = "房间类型必须是office、data_center或telecom_closet"
    ))]
    pub room_type: String,
    pub org_id: Option<Uuid>,
    pub network_ids: Vec<Uuid>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RoomUpdate {
    #[validate(length(min = 1, max = 50, message = "房间名称长度必须在1到50个字符之间"))]
    pub name: Option<String>,
    #[validate(custom(
        function = "validate_room_type_option",
        message = "房间类型必须是office、data_center或telecom_closet"
    ))]
    pub room_type: Option<String>,
    pub org_id: Option<Option<Uuid>>,
    pub network_ids: Option<Vec<Uuid>>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

// ==================== 机柜模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Cabinet {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub capacity: i32,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CabinetWithNetworks {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub capacity: i32,
    pub position_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub positions: Option<Vec<PositionBrief>>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetCreate {
    #[validate(length(min = 1, max = 50, message = "机柜名称长度必须在1到50个字符之间"))]
    pub name: String,
    pub room_id: Uuid,
    #[validate(range(min = 1, max = 48, message = "机柜容量必须在1到48U之间"))]
    pub capacity: i32,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetUpdate {
    #[validate(length(min = 1, max = 50, message = "机柜名称长度必须在1到50个字符之间"))]
    pub name: Option<String>,
    pub room_id: Option<Uuid>,
    #[validate(range(min = 1, max = 48, message = "机柜容量必须在1到48U之间"))]
    pub capacity: Option<i32>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

// ==================== 机位模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct CabinetPosition {
    pub id: Uuid,
    pub name: String,
    pub cabinet_id: Option<Uuid>,
    pub start_u: i32,
    pub end_u: i32,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CabinetPositionWithDetails {
    pub id: Uuid,
    pub name: String,
    pub cabinet_id: Option<Uuid>,
    pub cabinet_name: Option<String>,
    pub room_id: Option<Uuid>,
    pub room_name: Option<String>,
    pub start_u: i32,
    pub end_u: i32,
    pub ips: Vec<IpManager>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetPositionCreate {
    #[validate(length(min = 1, max = 50, message = "机位名称长度必须在1到50个字符之间"))]
    pub name: String,
    pub cabinet_id: Option<Uuid>,
    #[validate(range(min = 1, max = 48, message = "起始U位必须在1到48之间"))]
    pub start_u: i32,
    #[validate(range(min = 1, max = 48, message = "结束U位必须在1到48之间"))]
    pub end_u: i32,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetPositionUpdate {
    #[validate(length(min = 1, max = 50, message = "机位名称长度必须在1到50个字符之间"))]
    pub name: Option<String>,
    pub cabinet_id: Option<Uuid>,
    #[validate(range(min = 1, max = 48, message = "起始U位必须在1到48之间"))]
    pub start_u: Option<i32>,
    #[validate(range(min = 1, max = 48, message = "结束U位必须在1到48之间"))]
    pub end_u: Option<i32>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

// ==================== 工位模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Workstation {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub manager: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkstationWithDetails {
    pub id: Uuid,
    pub name: String,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub manager: Option<String>,
    pub ips: Vec<IpManager>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct WorkstationCreate {
    #[validate(length(min = 1, max = 50, message = "工位名称长度必须在1到50个字符之间"))]
    pub name: String,
    pub room_id: Uuid,
    #[validate(length(max = 50, message = "管理人长度不能超过50个字符"))]
    pub manager: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct WorkstationUpdate {
    #[validate(length(min = 1, max = 50, message = "工位名称长度必须在1到50个字符之间"))]
    pub name: Option<String>,
    pub room_id: Option<Uuid>,
    pub manager: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

// ==================== 批量同步模型 ====================

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct WorkstationSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 50, message = "工位名称长度必须在1到50个字符之间"))]
    pub name: String,
    #[validate(length(max = 50, message = "管理人长度不能超过50个字符"))]
    pub manager: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 50, message = "机柜名称长度必须在1到50个字符之间"))]
    pub name: String,
    #[validate(range(min = 1, max = 48, message = "机柜容量必须在1到48U之间"))]
    pub capacity: i32,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct PositionSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 50, message = "机位名称长度必须在1到50个字符之间"))]
    pub name: String,
    #[validate(range(min = 1, max = 48, message = "起始U位必须在1到48之间"))]
    pub start_u: i32,
    #[validate(range(min = 1, max = 48, message = "结束U位必须在1到48之间"))]
    pub end_u: i32,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RoomChildrenSync {
    pub workstations: Option<Vec<WorkstationSyncItem>>,
    pub cabinets: Option<Vec<CabinetSyncItem>>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CabinetPositionsSync {
    pub positions: Vec<PositionSyncItem>,
}

// ==================== IP 管理模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct IpManager {
    pub id: Uuid,
    pub device_interface_id: Uuid,
    pub device_id: Uuid,
    pub network_id: Option<Uuid>,
    pub ip_address: String,
    pub ip_version: i16,
    pub mac_address: Option<String>,
    pub hostname: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub last_seen: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_mac: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct IpManagerWithNames {
    pub id: Uuid,
    pub device_interface_id: Uuid,
    pub device_id: Uuid,
    pub device_type: Option<String>,
    pub device_name: Option<String>,
    pub network_id: Option<Uuid>,
    pub workstation_name: Option<String>,
    pub cabinet_position_name: Option<String>,
    pub interface_name: Option<String>,
    pub interface_type: Option<String>,
    pub room_name: Option<String>,
    pub cabinet_name: Option<String>,
    pub org_name: Option<String>,
    pub network_name: String,
    pub network_region: String,
    pub ip_address: String,
    pub ip_version: i16,
    pub mac_address: Option<String>,
    pub hostname: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub last_seen: DateTime<Utc>,
    pub last_mac: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct IpManagerCreate {
    pub device_interface_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub network_id: Option<Uuid>,
    #[validate(custom(function = "validate_ip_address", message = "请输入有效的IP地址"))]
    pub ip_address: String,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

// ==================== 设备网卡配置同步模型（网卡 → 网口 → IP） ====================

#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct IpSyncItem {
    pub id: Option<Uuid>,
    pub network_id: Option<Uuid>,
    #[validate(custom(function = "validate_ip_address", message = "请输入有效的IP地址"))]
    pub ip_address: String,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct PortSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 50, message = "网口名称长度必须在1到50个字符之间"))]
    pub name: String,
    pub interface_type: Option<String>,
    #[validate(length(max = 20, message = "MAC地址长度不能超过20个字符"))]
    pub mac_address: Option<String>,
    pub vlan_id: Option<i32>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    pub switch_id: Option<Uuid>,
    pub uplink_interface_id: Option<Uuid>,
    #[serde(default)]
    pub net_outlet_ids: Vec<Uuid>,
    #[serde(default)]
    pub ips: Vec<IpSyncItem>,
}

#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct NetworkCardSyncItem {
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 50, message = "网卡名称长度必须在1到50个字符之间"))]
    pub name: String,
    pub card_type: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    #[serde(default)]
    pub ports: Vec<PortSyncItem>,
}

#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct DeviceNetworkConfigSync {
    #[serde(default)]
    pub cards: Vec<NetworkCardSyncItem>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct AutoAssignIpRequest {
    pub network_id: Uuid,
    pub device_interface_id: Option<Uuid>,
    pub device_id: Uuid,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct PullIpManagersRequest {
    pub device_id: Uuid,
    pub network_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct IpManagerUpdate {
    #[serde(default)]
    pub device_interface_id: Option<Option<Uuid>>,
    pub ip_address: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    #[validate(length(max = 20, message = "状态长度不能超过20个字符"))]
    pub status: Option<String>,
    pub ip_version: Option<i16>,
}

// ==================== 交换机端口模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct SwitchPort {
    pub id: Uuid,
    pub device_id: Uuid,
    pub port_number: String,
    pub port_name: Option<String>,
    pub port_type: String,
    pub vlan_id: Option<i32>,
    pub status: String,
    pub speed: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct SwitchPortWithDevice {
    pub id: Uuid,
    pub device_id: Uuid,
    pub device_name: String,
    pub device_ip: Option<String>,
    pub port_number: String,
    pub port_name: Option<String>,
    pub port_type: String,
    pub vlan_id: Option<i32>,
    pub status: String,
    pub speed: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SwitchPortCreate {
    #[validate(length(min = 1, max = 30, message = "端口号长度必须在1到30个字符之间"))]
    pub port_number: String,
    #[validate(length(max = 50, message = "端口名称长度不能超过50个字符"))]
    pub port_name: Option<String>,
    pub port_type: Option<String>,
    pub vlan_id: Option<i32>,
    pub status: Option<String>,
    #[validate(length(max = 20, message = "速率长度不能超过20个字符"))]
    pub speed: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SwitchPortUpdate {
    #[validate(length(min = 1, max = 30, message = "端口号长度必须在1到30个字符之间"))]
    pub port_number: Option<String>,
    #[validate(length(max = 50, message = "端口名称长度不能超过50个字符"))]
    pub port_name: Option<String>,
    pub port_type: Option<String>,
    pub vlan_id: Option<i32>,
    pub status: Option<String>,
    #[validate(length(max = 20, message = "速率长度不能超过20个字符"))]
    pub speed: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

// ==================== 设备网卡模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct NetworkCard {
    pub id: Uuid,
    pub device_id: Uuid,
    pub name: String,
    pub card_type: String,
    pub description: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetworkCardCreate {
    #[validate(length(min = 1, max = 50, message = "网卡名称长度必须在1到50个字符之间"))]
    pub name: String,
    pub card_type: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetworkCardUpdate {
    #[validate(length(min = 1, max = 50, message = "网卡名称长度必须在1到50个字符之间"))]
    pub name: Option<String>,
    pub card_type: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub description: Option<Option<String>>,
}

// ==================== 设备三层接口/网口模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceInterface {
    pub id: Uuid,
    pub device_id: Uuid,
    pub network_card_id: Option<Uuid>,
    pub name: String,
    pub interface_type: String,
    pub mac_address: Option<String>,
    pub vlan_id: Option<i32>,
    pub description: Option<String>,
    pub switch_id: Option<Uuid>,
    pub uplink_interface_id: Option<Uuid>,
    #[serde(default)]
    pub net_outlet_ids: Vec<Uuid>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceInterfaceWithDevice {
    pub id: Uuid,
    pub device_id: Uuid,
    pub device_name: String,
    pub network_card_id: Option<Uuid>,
    pub name: String,
    pub interface_type: String,
    pub mac_address: Option<String>,
    pub vlan_id: Option<i32>,
    pub description: Option<String>,
    pub switch_id: Option<Uuid>,
    pub uplink_interface_id: Option<Uuid>,
    #[serde(default)]
    pub net_outlet_ids: Vec<Uuid>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct DeviceInterfaceCreate {
    #[validate(length(min = 1, max = 50, message = "接口名称长度必须在1到50个字符之间"))]
    pub name: String,
    pub interface_type: Option<String>,
    #[validate(length(max = 20, message = "MAC地址长度不能超过20个字符"))]
    pub mac_address: Option<String>,
    pub vlan_id: Option<i32>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    pub switch_id: Option<Uuid>,
    pub uplink_interface_id: Option<Uuid>,
    #[serde(default)]
    pub net_outlet_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct DeviceInterfaceUpdate {
    #[validate(length(min = 1, max = 50, message = "接口名称长度必须在1到50个字符之间"))]
    pub name: Option<String>,
    pub interface_type: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub mac_address: Option<Option<String>>,
    pub vlan_id: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub switch_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub uplink_interface_id: Option<Option<Uuid>>,
    #[serde(default)]
    pub net_outlet_ids: Option<Vec<Uuid>>,
}

// ==================== 物理链路模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct CableLink {
    pub id: Uuid,
    pub a_endpoint_type: String,
    pub a_endpoint_id: Uuid,
    pub b_endpoint_type: String,
    pub b_endpoint_id: Uuid,
    pub link_type: String,
    pub cable_label: Option<String>,
    pub length_m: Option<f64>,
    pub tested: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct CableLinkWithDetails {
    pub id: Uuid,
    pub a_endpoint_type: String,
    pub a_endpoint_id: Uuid,
    pub a_endpoint_label: Option<String>,
    pub b_endpoint_type: String,
    pub b_endpoint_id: Uuid,
    pub b_endpoint_label: Option<String>,
    pub link_type: String,
    pub cable_label: Option<String>,
    pub length_m: Option<f64>,
    pub tested: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CableLinkCreate {
    pub a_endpoint_type: String,
    pub a_endpoint_id: Uuid,
    pub b_endpoint_type: String,
    pub b_endpoint_id: Uuid,
    pub link_type: Option<String>,
    #[validate(length(max = 50, message = "线缆标签长度不能超过50个字符"))]
    pub cable_label: Option<String>,
    pub length_m: Option<f64>,
    pub tested: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CableLinkUpdate {
    pub link_type: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub cable_label: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub length_m: Option<Option<f64>>,
    pub tested: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CablePathNode {
    pub hop_idx: i32,
    pub node_type: String,
    pub node_id: Uuid,
    pub node_label: Option<String>,
    pub cable_id: Option<Uuid>,
    pub cable_label: Option<String>,
    pub hop_type: String,
}

// ==================== SNMP 相关模型 ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct SnmpTestRequest {
    pub device_id: Option<Uuid>,
    pub ip_address: Option<String>,
    pub snmp_version: Option<String>,
    pub snmp_community: Option<String>,
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    pub snmp_priv_password: Option<String>,
    pub snmp_port: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ArpEntry {
    pub ip_address: String,
    pub mac_address: String,
    pub interface: Option<String>,
    pub vlan_id: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LldpNeighbor {
    pub local_port: String,
    pub neighbor_chassis_id: Option<String>,
    pub neighbor_port_id: Option<String>,
    pub neighbor_port_desc: Option<String>,
    pub neighbor_sys_name: Option<String>,
    pub neighbor_sys_desc: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceMac {
    pub id: Uuid,
    pub device_id: Uuid,
    pub ip_address: String,
    pub mac_address: String,
    pub interface: Option<String>,
    pub vlan_id: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceMacCreate {
    pub ip_address: String,
    pub mac_address: String,
    pub interface: Option<String>,
    pub vlan_id: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceLldp {
    pub id: Uuid,
    pub device_id: Uuid,
    pub local_port: String,
    pub neighbor_chassis_id: Option<String>,
    pub neighbor_port_id: Option<String>,
    pub neighbor_port_desc: Option<String>,
    pub neighbor_sys_name: Option<String>,
    pub neighbor_sys_desc: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceLldpCreate {
    pub local_port: String,
    pub neighbor_chassis_id: Option<String>,
    pub neighbor_port_id: Option<String>,
    pub neighbor_port_desc: Option<String>,
    pub neighbor_sys_name: Option<String>,
    pub neighbor_sys_desc: Option<String>,
}

// ==================== 日志模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct OperationLog {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub username: Option<String>,
    pub action: String,
    pub operation_type: String,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub details: Option<serde_json::Value>,
    pub result: bool,
    pub ip_address: String,
    pub created_at: DateTime<Utc>,
}

// ==================== 定时任务模型 ====================

pub use ipma_scheduler::{ScheduledTask, TaskLog};

#[derive(Debug, Serialize, Deserialize, Clone, Validate)]
pub struct ScheduledTaskCreate {
    #[validate(length(min = 1, max = 100, message = "任务名称长度必须在1到100个字符之间"))]
    pub name: String,
    #[validate(length(min = 1, max = 50, message = "任务类型长度必须在1到50个字符之间"))]
    pub task_type: String,
    #[validate(length(min = 1, max = 100, message = "cron表达式长度必须在1到100个字符之间"))]
    pub cron_expression: String,
    pub enabled: Option<bool>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Validate)]
pub struct ScheduledTaskUpdate {
    #[validate(length(min = 1, max = 100, message = "任务名称长度必须在1到100个字符之间"))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 50, message = "任务类型长度必须在1到50个字符之间"))]
    pub task_type: Option<String>,
    #[validate(length(min = 1, max = 100, message = "cron表达式长度必须在1到100个字符之间"))]
    pub cron_expression: Option<String>,
    pub enabled: Option<bool>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct LoginLog {
    pub id: Uuid,
    pub username: String,
    pub ip_address: String,
    pub user_agent: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ==================== 通知模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub title: String,
    pub content: String,
    pub notification_type: String,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

// ==================== 组织管理模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct OrgTemplate {
    pub id: Uuid,
    pub name: String,
    pub levels: serde_json::Value,
    pub icons: serde_json::Value,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct OrgTemplateSummary {
    pub id: Uuid,
    pub name: String,
    pub levels: serde_json::Value,
    pub icons: serde_json::Value,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct OrgTemplateCreate {
    #[validate(length(min = 1, max = 100, message = "模板名称长度必须在1到100个字符之间"))]
    pub name: String,
    pub levels: serde_json::Value,
    pub icons: Option<serde_json::Value>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct OrgTemplateUpdate {
    #[validate(length(min = 1, max = 100, message = "模板名称长度必须在1到100个字符之间"))]
    pub name: Option<String>,
    pub levels: Option<serde_json::Value>,
    pub icons: Option<serde_json::Value>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub org_type: String,
    pub parent_id: Option<Uuid>,
    pub description: Option<String>,
    pub template_id: Option<Uuid>,
    pub level_index: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrganizationTreeNode {
    pub id: Uuid,
    pub name: String,
    pub org_type: String,
    pub parent_id: Option<Uuid>,
    pub description: Option<String>,
    pub template_id: Option<Uuid>,
    pub level_index: i32,
    pub children: Vec<OrganizationTreeNode>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrganizationWithChildren {
    pub id: Uuid,
    pub name: String,
    pub org_type: String,
    pub parent_id: Option<Uuid>,
    pub parent_name: Option<String>,
    pub description: Option<String>,
    pub template_id: Option<Uuid>,
    pub level_index: i32,
    pub children: Vec<Organization>,
    pub child_count: i64,
    pub room_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 校验组织类型字符串：非空（trim 后）且长度不超过 50
pub fn validate_org_type_string(org_type: &str) -> Result<(), ValidationError> {
    if org_type.trim().is_empty() {
        return Err(ValidationError::new("组织类型不能为空"));
    }
    if org_type.len() > 50 {
        return Err(ValidationError::new("组织类型长度不能超过50个字符"));
    }
    Ok(())
}

fn validate_org_type_option(org_type: &&String) -> Result<(), ValidationError> {
    validate_org_type_string(org_type)
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct OrganizationCreate {
    #[validate(length(min = 1, max = 100, message = "组织名称长度必须在1到100个字符之间"))]
    pub name: String,
    #[validate(custom(
        function = "validate_org_type_string",
        message = "组织类型长度必须在1到50个字符之间且不能为空"
    ))]
    pub org_type: String,
    pub parent_id: Option<Uuid>,
    pub template_id: Option<Uuid>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct OrganizationUpdate {
    #[validate(length(min = 1, max = 100, message = "组织名称长度必须在1到100个字符之间"))]
    pub name: Option<String>,
    #[validate(custom(
        function = "validate_org_type_option",
        message = "组织类型长度必须在1到50个字符之间且不能为空"
    ))]
    pub org_type: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

// ==================== 信息点模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct NetOutlet {
    pub id: Uuid,
    pub name: String,
    pub outlet_type: String,
    pub room_id: Uuid,
    pub cabinet_id: Option<Uuid>,
    pub description: Option<String>,
    pub peer_type: Option<String>,
    pub peer_room_id: Option<Uuid>,
    pub peer_outlet_id: Option<Uuid>,
    pub peer_switch_port_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct NetOutletWithDetails {
    pub id: Uuid,
    pub name: String,
    pub outlet_type: String,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub cabinet_id: Option<Uuid>,
    pub cabinet_name: Option<String>,
    pub description: Option<String>,
    pub peer_type: Option<String>,
    pub peer_room_id: Option<Uuid>,
    pub peer_outlet_id: Option<Uuid>,
    pub peer_switch_port_id: Option<Uuid>,
    pub peer_room_name: Option<String>,
    pub peer_outlet_name: Option<String>,
    pub peer_switch_port_label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetOutletCreate {
    #[validate(length(min = 1, max = 100, message = "信息点名称长度必须在1到100个字符之间"))]
    pub name: String,
    pub outlet_type: Option<String>,
    pub room_id: Uuid,
    pub cabinet_id: Option<Uuid>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    pub peer_type: Option<String>,
    pub peer_room_id: Option<Uuid>,
    pub peer_outlet_id: Option<Uuid>,
    pub peer_switch_port_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetOutletUpdate {
    #[validate(length(min = 1, max = 100, message = "信息点名称长度必须在1到100个字符之间"))]
    pub name: Option<String>,
    pub outlet_type: Option<String>,
    pub room_id: Option<Uuid>,
    pub cabinet_id: Option<Option<Uuid>>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub peer_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub peer_room_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub peer_outlet_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub peer_switch_port_id: Option<Option<Uuid>>,
}

// ==================== 设备模板模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceTemplate {
    pub id: Uuid,
    pub name: String,
    pub device_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceTemplateSummary {
    pub id: Uuid,
    pub name: String,
    pub device_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateDeviceTemplateRequest {
    #[validate(length(min = 1, max = 100, message = "模板名称不能为空且不超过100个字符"))]
    pub name: String,
    #[validate(length(min = 1, max = 30, message = "设备类型不能为空"))]
    pub device_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub description: Option<String>,
}

// ==================== 设备模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Device {
    pub id: Uuid,
    pub name: String,
    pub device_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub workstation_id: Option<Uuid>,
    pub position_id: Option<Uuid>,
    pub room_id: Uuid,
    pub template_id: Option<Uuid>,
    pub vendor: Option<String>,
    pub location: Option<String>,
    pub snmp_version: String,
    pub snmp_community: Option<String>,
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    pub snmp_priv_password: Option<String>,
    pub snmp_port: i32,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct DeviceWithDetails {
    pub id: Uuid,
    pub name: String,
    pub device_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub workstation_id: Option<Uuid>,
    pub workstation_name: Option<String>,
    pub position_id: Option<Uuid>,
    pub room_id: Uuid,
    pub room_name: Option<String>,
    pub cabinet_id: Option<Uuid>,
    pub cabinet_name: Option<String>,
    pub start_u: Option<i32>,
    pub end_u: Option<i32>,
    pub template_id: Option<Uuid>,
    pub template_name: Option<String>,
    pub vendor: Option<String>,
    pub location: Option<String>,
    pub snmp_version: Option<String>,
    pub snmp_community: Option<String>,
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    pub snmp_priv_password: Option<String>,
    pub snmp_port: Option<i32>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct DeviceCreate {
    #[validate(length(min = 1, max = 100, message = "设备名称长度必须在1到100个字符之间"))]
    pub name: String,
    #[validate(custom(
        function = "validate_device_type_option",
        message = "设备类型必须是pc/laptop/printer/server/network_device/switch/camera/phone/other"
    ))]
    pub device_type: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub workstation_id: Option<Uuid>,
    pub position_id: Option<Uuid>,
    pub room_id: Uuid,
    pub template_id: Option<Uuid>,
    #[validate(length(max = 50, message = "厂商长度不能超过50个字符"))]
    pub vendor: Option<String>,
    #[validate(length(max = 100, message = "位置长度不能超过100个字符"))]
    pub location: Option<String>,
    pub snmp_version: Option<String>,
    #[validate(length(max = 100, message = "SNMP团体字符串长度不能超过100个字符"))]
    pub snmp_community: Option<String>,
    #[validate(length(max = 50, message = "SNMP用户名长度不能超过50个字符"))]
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    #[validate(length(max = 100, message = "SNMP认证密码长度不能超过100个字符"))]
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    #[validate(length(max = 100, message = "SNMP隐私密码长度不能超过100个字符"))]
    pub snmp_priv_password: Option<String>,
    pub snmp_port: Option<i32>,
    pub cards: Option<Vec<NetworkCardSyncItem>>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    pub save_as_template: Option<bool>,
    #[validate(length(max = 100, message = "模板名称长度不能超过100个字符"))]
    pub template_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct DeviceUpdate {
    #[validate(length(min = 1, max = 100, message = "设备名称长度必须在1到100个字符之间"))]
    pub name: Option<String>,
    #[validate(custom(
        function = "validate_device_type_option",
        message = "设备类型必须是pc/laptop/printer/server/network_device/switch/camera/phone/other"
    ))]
    pub device_type: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub workstation_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub position_id: Option<Option<Uuid>>,
    pub room_id: Option<Uuid>,
    #[validate(length(max = 50, message = "厂商长度不能超过50个字符"))]
    pub vendor: Option<String>,
    #[validate(length(max = 100, message = "位置长度不能超过100个字符"))]
    pub location: Option<String>,
    pub snmp_version: Option<String>,
    #[validate(length(max = 100, message = "SNMP团体字符串长度不能超过100个字符"))]
    pub snmp_community: Option<String>,
    #[validate(length(max = 50, message = "SNMP用户名长度不能超过50个字符"))]
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    #[validate(length(max = 100, message = "SNMP认证密码长度不能超过100个字符"))]
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    #[validate(length(max = 100, message = "SNMP隐私密码长度不能超过100个字符"))]
    pub snmp_priv_password: Option<String>,
    pub snmp_port: Option<i32>,
    pub cards: Option<Vec<NetworkCardSyncItem>>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    pub save_as_template: Option<bool>,
    #[validate(length(max = 100, message = "模板名称长度不能超过100个字符"))]
    pub template_name: Option<String>,
}
