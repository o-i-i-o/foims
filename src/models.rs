use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::{Validate, ValidationError};

// ==================== 验证函数 ====================

pub fn validate_room_type_string(room_type: &str) -> Result<(), ValidationError> {
    let room_type_lower = room_type.to_lowercase();
    if room_type_lower == "office" || room_type_lower == "data_center" {
        Ok(())
    } else {
        Err(ValidationError::new("房间类型必须是office或data_center"))
    }
}

pub fn validate_room_type_option(room_type: &&String) -> Result<(), ValidationError> {
    validate_room_type_string(room_type)
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

#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
}

impl<T: Serialize> PaginatedResponse<T> {
    #[must_use] 
    pub fn new(items: Vec<T>, total: i64, page: i64, page_size: i64, message: &str) -> Self {
        let total_pages = if page_size > 0 {
            (total + page_size - 1) / page_size
        } else {
            0
        };
        Self {
            success: true,
            message: message.to_string(),
            data: items,
            total,
            page,
            page_size,
            total_pages,
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

impl Position {
    pub fn x_i32(&self) -> i32 {
        self.x.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
    }

    pub fn y_i32(&self) -> i32 {
        self.y.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
    }

    pub fn width_i32(&self) -> i32 {
        self.width.round().clamp(0.0, i32::MAX as f64) as i32
    }

    pub fn height_i32(&self) -> i32 {
        self.height.round().clamp(0.0, i32::MAX as f64) as i32
    }

    pub fn rotation_i32(&self) -> i32 {
        self.rotation.round().clamp(0.0, 360.0) as i32
    }
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
    #[validate(length(min = 1, max = 20, message = "角色长度必须在1到20个字符之间"))]
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UserUpdate {
    #[validate(email(message = "请输入有效的邮箱地址"))]
    pub email: Option<String>,
    #[validate(length(min = 1, max = 20, message = "角色长度必须在1到20个字符之间"))]
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetworkRegionCreate {
    #[validate(length(min = 1, max = 20, message = "网络区域名称长度必须在1到20个字符之间"))]
    pub name: String,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetworkRegionUpdate {
    #[validate(length(min = 1, max = 20, message = "网络区域名称长度必须在1到20个字符之间"))]
    pub name: Option<String>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
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
    pub description: Option<String>,
    pub networks: Vec<NetworkInfo>,
    pub workstation_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RoomCreate {
    #[validate(length(min = 1, max = 50, message = "房间名称长度必须在1到50个字符之间"))]
    pub name: String,
    #[validate(custom(
        function = "validate_room_type_string",
        message = "房间类型必须是office或data_center"
    ))]
    pub room_type: String,
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
        message = "房间类型必须是office或data_center"
    ))]
    pub room_type: Option<String>,
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
    pub device_type: Option<String>,
    pub device_id: Option<Uuid>,
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
    pub device_type: Option<String>,
    pub device_id: Option<Uuid>,
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
    pub ips: Option<Vec<IpManagerCreate>>,
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
    pub ips: Option<Vec<IpManagerCreate>>,
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
    pub room_name: String,
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
    pub ips: Option<Vec<IpManagerCreate>>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct WorkstationUpdate {
    #[validate(length(min = 1, max = 50, message = "工位名称长度必须在1到50个字符之间"))]
    pub name: Option<String>,
    pub room_id: Option<Uuid>,
    pub manager: Option<String>,
    pub ips: Option<Vec<IpManagerCreate>>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
}

// ==================== IP 管理模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct IpManager {
    pub id: Uuid,
    pub workstation_id: Option<Uuid>,
    pub position_id: Option<Uuid>,
    pub switch_port_id: Option<Uuid>,
    pub device_type: Option<String>,
    pub network_id: Option<Uuid>,
    pub ip_address: String,
    pub ip_version: i16,
    pub mac_address: Option<String>,
    pub hostname: Option<String>,
    pub status: String,
    pub last_seen: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_mac: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct IpManagerWithNames {
    pub id: Uuid,
    pub workstation_id: Option<Uuid>,
    pub position_id: Option<Uuid>,
    pub switch_port_id: Option<Uuid>,
    pub device_type: Option<String>,
    pub device_name: Option<String>,
    pub network_id: Option<Uuid>,
    pub workstation_name: Option<String>,
    pub cabinet_position_name: Option<String>,
    pub switch_name: Option<String>,
    pub switch_port_number: Option<String>,
    pub room_name: Option<String>,
    pub cabinet_name: Option<String>,
    pub network_name: String,
    pub network_region: String,
    pub ip_address: String,
    pub ip_version: i16,
    pub mac_address: Option<String>,
    pub hostname: Option<String>,
    pub status: String,
    pub last_seen: DateTime<Utc>,
    pub last_mac: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct IpManagerCreate {
    pub workstation_id: Option<Uuid>,
    pub position_id: Option<Uuid>,
    pub switch_port_id: Option<Uuid>,
    pub device_type: Option<String>,
    pub network_region_id: Option<Uuid>,
    #[validate(custom(function = "validate_ip_address", message = "请输入有效的IP地址"))]
    pub ip_address: String,
    #[validate(length(max = 23, message = "请输入有效的MAC地址"))]
    pub mac_address: Option<String>,
    #[validate(length(max = 100, message = "主机名长度不能超过100个字符"))]
    pub hostname: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct AutoAssignIpRequest {
    pub network_id: Uuid,
    pub workstation_id: Option<Uuid>,
    pub position_id: Option<Uuid>,
    pub switch_port_id: Option<Uuid>,
    #[validate(length(max = 23, message = "请输入有效的MAC地址"))]
    pub mac_address: Option<String>,
    #[validate(length(max = 100, message = "主机名长度不能超过100个字符"))]
    pub hostname: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct PullIpManagersRequest {
    pub switch_id: Uuid,
    pub network_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct IpManagerUpdate {
    pub workstation_id: Option<Uuid>,
    pub position_id: Option<Uuid>,
    pub switch_port_id: Option<Uuid>,
    pub device_type: Option<String>,
    pub ip_address: Option<String>,
    #[validate(length(max = 23, message = "请输入有效的MAC地址"))]
    pub mac_address: Option<String>,
    #[validate(length(max = 100, message = "主机名长度不能超过100个字符"))]
    pub hostname: Option<String>,
    #[validate(length(max = 20, message = "状态长度不能超过20个字符"))]
    pub status: Option<String>,
    pub ip_version: Option<i16>,
}

// ==================== 交换机模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Switch {
    pub id: Uuid,
    pub name: String,
    pub model: Option<String>,
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
    pub position_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct SwitchWithParent {
    pub id: Uuid,
    pub name: String,
    pub model: Option<String>,
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
    pub position_id: Option<Uuid>,
    pub cabinet_id: Option<Uuid>,
    pub cabinet_name: Option<String>,
    pub room_id: Option<Uuid>,
    pub room_name: Option<String>,
    pub start_u: Option<i32>,
    pub end_u: Option<i32>,
    pub position_network_id: Option<Uuid>,
    pub network_region_id: Option<Uuid>,
    pub description: Option<String>,
    pub device_type: Option<String>,
    pub ip_address: Option<String>,
    pub mac_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SwitchPosition {
    pub id: Uuid,
    pub cabinet_id: Uuid,
    pub cabinet_name: String,
    pub start_u: i32,
    pub end_u: i32,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SwitchCreate {
    #[validate(length(min = 1, max = 100, message = "交换机名称长度必须在1到100个字符之间"))]
    pub name: String,
    #[validate(length(max = 100, message = "型号长度不能超过100个字符"))]
    pub model: Option<String>,
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
    pub ips: Option<Vec<IpManagerCreate>>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    pub position_id: Option<Uuid>,
    pub cabinet_id: Option<Uuid>,
    pub start_u: Option<i32>,
    pub end_u: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SwitchUpdate {
    #[validate(length(min = 1, max = 100, message = "交换机名称长度必须在1到100个字符之间"))]
    pub name: Option<String>,
    #[validate(length(max = 100, message = "型号长度不能超过100个字符"))]
    pub model: Option<String>,
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
    pub ips: Option<Vec<IpManagerCreate>>,
    #[validate(length(max = 255, message = "描述长度不能超过255个字符"))]
    pub description: Option<String>,
    pub position_id: Option<Uuid>,
    pub cabinet_id: Option<Uuid>,
    pub start_u: Option<i32>,
    pub end_u: Option<i32>,
}

// ==================== 交换机端口模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct SwitchPort {
    pub id: Uuid,
    pub switch_id: Uuid,
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
pub struct SwitchPortWithSwitch {
    pub id: Uuid,
    pub switch_id: Uuid,
    pub switch_name: String,
    pub switch_ip: String,
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

// ==================== SNMP 相关模型 ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct SnmpTestRequest {
    pub switch_id: Option<Uuid>,
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
pub struct SwitchMac {
    pub id: Uuid,
    pub switch_id: Uuid,
    pub ip_address: String,
    pub mac_address: String,
    pub interface: Option<String>,
    pub vlan_id: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SwitchMacCreate {
    pub ip_address: String,
    pub mac_address: String,
    pub interface: Option<String>,
    pub vlan_id: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct SwitchLldp {
    pub id: Uuid,
    pub switch_id: Uuid,
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
pub struct SwitchLldpCreate {
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
    pub user_id: Uuid,
    pub username: String,
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

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct ScheduledTask {
    pub id: Uuid,
    pub name: String,
    pub task_type: String,
    pub cron_expression: String,
    pub enabled: bool,
    pub config: serde_json::Value,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_result: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

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
pub struct TaskLog {
    pub id: Uuid,
    pub task_name: String,
    pub status: String,
    pub details: serde_json::Value,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration: Option<i32>,
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
