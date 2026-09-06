//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::{Validate, ValidationError};

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
    #[validate(length(
        min = 1,
        max = 100,
        message = "server.org_template.validation.name_length"
    ))]
    pub name: String,
    pub levels: serde_json::Value,
    pub icons: Option<serde_json::Value>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct OrgTemplateUpdate {
    #[validate(length(
        min = 1,
        max = 100,
        message = "server.org_template.validation.name_length"
    ))]
    pub name: Option<String>,
    pub levels: Option<serde_json::Value>,
    pub icons: Option<serde_json::Value>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub type_path: String,
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

/// 校验类型路径：非空、格式合法（点分隔的数字索引）且总长不超过 50
///（organizations.type_path 列为 VARCHAR(50)，超长直写数据库报错）
pub fn validate_type_path(type_path: &str) -> Result<(), ValidationError> {
    if type_path.trim().is_empty() {
        // code 仅作错误标识；实际返回给前端的消息 key 由调用点的 message 属性覆盖
        return Err(ValidationError::new(
            "server.organization.validation.type_path_required",
        ));
    }
    if type_path.chars().count() > 50 {
        return Err(ValidationError::new(
            "server.organization.validation.type_path_length",
        ));
    }
    for segment in type_path.split('.') {
        if segment.parse::<usize>().is_err() {
            return Err(ValidationError::new(
                "server.organization.validation.type_path_invalid",
            ));
        }
    }
    Ok(())
}

pub fn validate_type_path_option(type_path: &&String) -> Result<(), ValidationError> {
    validate_type_path(type_path)
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct OrganizationCreate {
    #[validate(length(
        min = 1,
        max = 100,
        message = "server.organization.validation.name_length"
    ))]
    pub name: String,
    #[validate(custom(
        function = "crate::models::validate_type_path",
        message = "server.organization.validation.type_path_invalid"
    ))]
    pub type_path: String,
    pub parent_id: Option<Uuid>,
    pub template_id: Option<Uuid>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct OrganizationUpdate {
    #[validate(length(
        min = 1,
        max = 100,
        message = "server.organization.validation.name_length"
    ))]
    pub name: Option<String>,
    #[validate(custom(
        function = "crate::models::validate_type_path_option",
        message = "server.organization.validation.type_path_invalid"
    ))]
    pub type_path: Option<String>,
    #[validate(length(max = 255, message = "server.common.validation.description_length"))]
    pub description: Option<String>,
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    // ---------- 类型路径校验 ----------

    #[test]
    fn test_validate_type_path_valid() {
        // 点分隔的数字索引为合法
        for valid in ["0", "1.2", "0.1.2.3", "10.20.30"] {
            assert!(validate_type_path(valid).is_ok(), "路径 {valid} 应合法");
        }
    }

    #[test]
    fn test_validate_type_path_empty_rejected() {
        // 空串 / 纯空白被拒绝
        for invalid in ["", "   ", "\t"] {
            let err = validate_type_path(invalid)
                .err()
                .unwrap_or_else(|| panic!("路径 {invalid:?} 应被拒绝"));
            assert_eq!(
                err.code,
                "server.organization.validation.type_path_required"
            );
        }
    }

    #[test]
    fn test_validate_type_path_invalid_segments() {
        // 非数字段 / 负数 / 空段（连续点、首尾点）均被拒绝
        for invalid in ["a.b", "1.x.2", "-1.2", "1..2", ".1", "1.", "1.2.3a"] {
            let err = validate_type_path(invalid)
                .err()
                .unwrap_or_else(|| panic!("路径 {invalid} 应被拒绝"));
            assert_eq!(err.code, "server.organization.validation.type_path_invalid");
        }
    }

    #[test]
    fn test_validate_type_path_option_delegates() {
        let ok_value = String::from("1.2.3");
        let ok_ref: &String = &ok_value;
        assert!(validate_type_path_option(&ok_ref).is_ok());

        let bad_value = String::from("1.two");
        let bad_ref: &String = &bad_value;
        assert!(validate_type_path_option(&bad_ref).is_err());
    }

    // ---------- 模板请求 ----------

    #[test]
    fn test_org_template_create_valid() -> Result<(), serde_json::Error> {
        let req: OrgTemplateCreate = serde_json::from_value(serde_json::json!({
            "name": "默认模板",
            "levels": [
                { "index": 0, "name": "公司", "type": "company" },
                { "index": 1, "name": "部门", "type": "department" }
            ],
            "icons": {},
            "description": "内置模板"
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_org_template_create_invalid() -> Result<(), serde_json::Error> {
        // 空名称与超长描述分别拒绝
        let req: OrgTemplateCreate = serde_json::from_value(serde_json::json!({
            "name": "",
            "levels": [],
            "description": "长".repeat(256)
        }))?;
        let Err(errors) = req.validate() else {
            panic!("空模板名应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        assert!(errors.errors().contains_key("description"));
        Ok(())
    }

    #[test]
    fn test_org_template_update_valid() -> Result<(), serde_json::Error> {
        // 全字段可缺省
        let req: OrgTemplateUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert!(req.validate().is_ok());

        let full: OrgTemplateUpdate = serde_json::from_value(serde_json::json!({
            "name": "新模板名",
            "levels": [],
            "icons": null
        }))?;
        assert!(full.validate().is_ok());
        Ok(())
    }

    // ---------- 组织请求 ----------

    #[test]
    fn test_organization_create_valid() -> Result<(), serde_json::Error> {
        let req: OrganizationCreate = serde_json::from_value(serde_json::json!({
            "name": "信息技术部",
            "type_path": "0.1",
            "description": "IT 部门"
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.parent_id, None);
        Ok(())
    }

    #[test]
    fn test_organization_create_invalid_type_path() -> Result<(), serde_json::Error> {
        let req: OrganizationCreate = serde_json::from_value(serde_json::json!({
            "name": "信息技术部",
            "type_path": "0.company"
        }))?;
        let Err(errors) = req.validate() else {
            panic!("非法类型路径应被拒绝");
        };
        assert!(errors.errors().contains_key("type_path"));
        Ok(())
    }

    #[test]
    fn test_organization_create_name_length() -> Result<(), serde_json::Error> {
        let req: OrganizationCreate = serde_json::from_value(serde_json::json!({
            "name": "N".repeat(101),
            "type_path": "0"
        }))?;
        let Err(errors) = req.validate() else {
            panic!("超长组织名应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        Ok(())
    }

    #[test]
    fn test_organization_update_valid_and_invalid() -> Result<(), serde_json::Error> {
        // 合法更新
        let ok: OrganizationUpdate = serde_json::from_value(serde_json::json!({
            "name": "新部门名",
            "type_path": "0.2"
        }))?;
        assert!(ok.validate().is_ok());

        // 非法类型路径拒绝
        let bad: OrganizationUpdate = serde_json::from_value(serde_json::json!({
            "type_path": "x.y"
        }))?;
        let Err(errors) = bad.validate() else {
            panic!("非法类型路径应被拒绝");
        };
        assert!(errors.errors().contains_key("type_path"));

        // 全缺省通过
        let empty: OrganizationUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert!(empty.validate().is_ok());
        Ok(())
    }

    // ---------- 实体序列化往返 ----------

    #[test]
    fn test_organization_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let org = Organization {
            id: Uuid::new_v4(),
            name: "总部".to_string(),
            type_path: "0".to_string(),
            parent_id: None,
            description: Some("根节点".to_string()),
            template_id: None,
            level_index: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&org)?;
        let back: Organization = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_organization_tree_node_nested_roundtrip() -> Result<(), serde_json::Error> {
        // 嵌套 children 树结构往返应保持一致
        let leaf = OrganizationTreeNode {
            id: Uuid::new_v4(),
            name: "子部门".to_string(),
            org_type: "department".to_string(),
            parent_id: None,
            description: None,
            template_id: None,
            level_index: 1,
            children: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let root = OrganizationTreeNode {
            id: Uuid::new_v4(),
            name: "公司".to_string(),
            org_type: "company".to_string(),
            parent_id: None,
            description: None,
            template_id: None,
            level_index: 0,
            children: vec![leaf],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&root)?;
        assert_eq!(
            first["children"].as_array().map(Vec::len),
            Some(1),
            "子节点应被序列化"
        );
        let back: OrganizationTreeNode = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_org_template_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let template = OrgTemplate {
            id: Uuid::new_v4(),
            name: "默认模板".to_string(),
            levels: serde_json::json!([{"index": 0, "name": "公司"}]),
            icons: serde_json::json!({}),
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&template)?;
        let back: OrgTemplate = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }
}
