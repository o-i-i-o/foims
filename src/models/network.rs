//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

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
    #[validate(length(
        min = 1,
        max = 20,
        message = "server.network.validation.region_name_length"
    ))]
    pub name: String,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
    pub ipv4_cidrs: Option<Vec<String>>,
    pub ipv6_cidrs: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetworkRegionUpdate {
    #[validate(length(
        min = 1,
        max = 20,
        message = "server.network.validation.region_name_length"
    ))]
    pub name: Option<String>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
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
    #[validate(length(min = 1, max = 50, message = "server.network.validation.name_length"))]
    pub name: String,
    pub network_region_id: Uuid,
    pub ipv4_cidr: Option<String>,
    pub ipv6_cidr: Option<String>,
    pub ipv4_gateway: Option<String>,
    pub ipv6_gateway: Option<String>,
    #[validate(custom(
        function = "crate::models::validate_dns_count",
        message = "server.network.validation.dns_count"
    ))]
    pub ipv4_dns: Option<Vec<String>>,
    #[validate(custom(
        function = "crate::models::validate_dns_count",
        message = "server.network.validation.dns_count"
    ))]
    pub ipv6_dns: Option<Vec<String>>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct NetworkUpdate {
    #[validate(length(min = 1, max = 50, message = "server.network.validation.name_length"))]
    pub name: Option<String>,
    pub network_region_id: Option<Uuid>,
    pub ipv4_cidr: Option<String>,
    pub ipv6_cidr: Option<String>,
    pub ipv4_gateway: Option<String>,
    pub ipv6_gateway: Option<String>,
    #[validate(custom(
        function = "crate::models::validate_dns_count",
        message = "server.network.validation.dns_count"
    ))]
    pub ipv4_dns: Option<Vec<String>>,
    #[validate(custom(
        function = "crate::models::validate_dns_count",
        message = "server.network.validation.dns_count"
    ))]
    pub ipv6_dns: Option<Vec<String>>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}
