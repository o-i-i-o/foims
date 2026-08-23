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

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    #[test]
    fn test_region_create_valid() -> Result<(), serde_json::Error> {
        let req: NetworkRegionCreate = serde_json::from_value(serde_json::json!({
            "name": "办公区",
            "description": "一楼办公网络",
            "ipv4_cidrs": ["10.0.0.0/8"],
            "ipv6_cidrs": ["2001:db8::/32"]
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(
            req.ipv4_cidrs.as_deref(),
            Some(&["10.0.0.0/8".to_string()][..])
        );
        Ok(())
    }

    #[test]
    fn test_region_create_name_length() -> Result<(), serde_json::Error> {
        // 区域名长度限制 1~20
        let empty: NetworkRegionCreate = serde_json::from_value(serde_json::json!({
            "name": "",
            "ipv4_cidrs": null,
            "ipv6_cidrs": null
        }))?;
        let Err(errors) = empty.validate() else {
            panic!("空区域名应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));

        let long: NetworkRegionCreate = serde_json::from_value(serde_json::json!({
            "name": "区".repeat(21),
            "ipv4_cidrs": null,
            "ipv6_cidrs": null
        }))?;
        assert!(long.validate().is_err());
        Ok(())
    }

    #[test]
    fn test_region_create_description_too_long() -> Result<(), serde_json::Error> {
        let req: NetworkRegionCreate = serde_json::from_value(serde_json::json!({
            "name": "办公区",
            "description": "长".repeat(256),
            "ipv4_cidrs": null,
            "ipv6_cidrs": null
        }))?;
        let Err(errors) = req.validate() else {
            panic!("超长描述应被拒绝");
        };
        assert!(errors.errors().contains_key("description"));
        Ok(())
    }

    #[test]
    fn test_region_update_valid_with_none() -> Result<(), serde_json::Error> {
        // 全字段缺省的更新请求应通过校验
        let req: NetworkRegionUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.name, None);
        Ok(())
    }

    #[test]
    fn test_region_update_invalid_name() -> Result<(), serde_json::Error> {
        let req: NetworkRegionUpdate = serde_json::from_value(serde_json::json!({
            "name": "x".repeat(51)
        }))?;
        let Err(errors) = req.validate() else {
            panic!("超长名称应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        Ok(())
    }

    #[test]
    fn test_network_create_valid_with_dns() -> Result<(), serde_json::Error> {
        let req: NetworkCreate = serde_json::from_value(serde_json::json!({
            "name": "办公网",
            "network_region_id": Uuid::new_v4(),
            "ipv4_cidr": "10.1.0.0/24",
            "ipv4_gateway": "10.1.0.254",
            "ipv4_dns": ["223.5.5.5", "223.6.6.6"],
            "ipv6_dns": ["2400:3200::9", "2400:3200:baba::9"],
            "description": null
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(
            req.ipv4_dns.as_deref(),
            Some(&["223.5.5.5".to_string(), "223.6.6.6".to_string()][..])
        );
        Ok(())
    }

    #[test]
    fn test_network_create_dns_count_exceeded() -> Result<(), serde_json::Error> {
        // 超过 5 个 DNS 应被拒绝（IPv4 分支）
        let dns_list: Vec<String> = (0..6).map(|i| format!("8.8.8.{i}")).collect();
        let req: NetworkCreate = serde_json::from_value(serde_json::json!({
            "name": "办公网",
            "network_region_id": Uuid::new_v4(),
            "ipv4_dns": dns_list
        }))?;
        let Err(errors) = req.validate() else {
            panic!("6 个 IPv4 DNS 应被拒绝");
        };
        assert!(errors.errors().contains_key("ipv4_dns"));
        Ok(())
    }

    #[test]
    fn test_network_create_ipv6_dns_count_exceeded() -> Result<(), serde_json::Error> {
        // 超过 5 个 IPv6 DNS 同样拒绝
        let dns_list: Vec<String> = (0..6).map(|i| format!("2400:3200::{i}")).collect();
        let req: NetworkCreate = serde_json::from_value(serde_json::json!({
            "name": "办公网",
            "network_region_id": Uuid::new_v4(),
            "ipv6_dns": dns_list
        }))?;
        let Err(errors) = req.validate() else {
            panic!("6 个 IPv6 DNS 应被拒绝");
        };
        assert!(errors.errors().contains_key("ipv6_dns"));
        Ok(())
    }

    #[test]
    fn test_network_create_dns_at_boundary_allowed() -> Result<(), serde_json::Error> {
        // 恰好 5 个 DNS 合法
        let dns_list: Vec<String> = (0..5).map(|i| format!("8.8.4.{i}")).collect();
        let req: NetworkCreate = serde_json::from_value(serde_json::json!({
            "name": "办公网",
            "network_region_id": Uuid::new_v4(),
            "ipv4_dns": dns_list
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_network_create_name_length() -> Result<(), serde_json::Error> {
        let req: NetworkCreate = serde_json::from_value(serde_json::json!({
            "name": "",
            "network_region_id": Uuid::new_v4()
        }))?;
        let Err(errors) = req.validate() else {
            panic!("空名称应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        Ok(())
    }

    #[test]
    fn test_network_update_valid() -> Result<(), serde_json::Error> {
        let req: NetworkUpdate = serde_json::from_value(serde_json::json!({
            "name": "新名称",
            "ipv4_dns": ["1.1.1.1"]
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_network_update_invalid() -> Result<(), serde_json::Error> {
        // 超长名称 + 超量 DNS 应同时报两个字段错误
        let dns_list: Vec<String> = (0..6).map(|i| format!("8.8.8.{i}")).collect();
        let req: NetworkUpdate = serde_json::from_value(serde_json::json!({
            "name": "n".repeat(51),
            "ipv4_dns": dns_list
        }))?;
        let Err(errors) = req.validate() else {
            panic!("非法更新请求应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        assert!(errors.errors().contains_key("ipv4_dns"));
        Ok(())
    }

    #[test]
    fn test_network_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let network = Network {
            id: Uuid::new_v4(),
            name: "办公网".to_string(),
            network_region_id: Uuid::new_v4(),
            network_region: "办公区".to_string(),
            ipv4_cidr: Some("10.1.0.0/24".to_string()),
            ipv6_cidr: None,
            ipv4_gateway: Some("10.1.0.254".to_string()),
            ipv6_gateway: None,
            ipv4_dns: Some(vec!["223.5.5.5".to_string()]),
            ipv6_dns: None,
            description: Some("描述".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&network)?;
        let back: Network = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_network_region_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let region = NetworkRegion {
            id: Uuid::new_v4(),
            name: "办公区".to_string(),
            description: None,
            ipv4_cidrs: Some(vec!["10.0.0.0/8".to_string()]),
            ipv6_cidrs: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&region)?;
        let back: NetworkRegion = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_network_info_serde_roundtrip() -> Result<(), serde_json::Error> {
        let info = NetworkInfo {
            id: Uuid::new_v4(),
            name: "办公网".to_string(),
            network_region: "办公区".to_string(),
            network_region_id: Uuid::new_v4(),
            ipv4_cidr: Some("10.1.0.0/24".to_string()),
            ipv6_cidr: None,
        };
        let first = serde_json::to_value(&info)?;
        let back: NetworkInfo = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }
}
