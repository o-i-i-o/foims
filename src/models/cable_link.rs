//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

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
    /// A 端点所属作用域（用于编辑时回填级联选择器）
    pub a_room_id: Option<Uuid>,
    pub a_cabinet_id: Option<Uuid>,
    pub a_device_id: Option<Uuid>,
    pub b_endpoint_type: String,
    pub b_endpoint_id: Uuid,
    pub b_endpoint_label: Option<String>,
    /// B 端点所属作用域（用于编辑时回填级联选择器）
    pub b_room_id: Option<Uuid>,
    pub b_cabinet_id: Option<Uuid>,
    pub b_device_id: Option<Uuid>,
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
    #[validate(length(max = 50, message = "server.cable_link.validation.cable_label_length"))]
    pub cable_label: Option<String>,
    pub length_m: Option<f64>,
    pub tested: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CableLinkUpdate {
    pub link_type: Option<String>,
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    #[validate(custom(
        function = "crate::models::validate_cable_label_opt",
        message = "server.cable_link.validation.cable_label_length"
    ))]
    pub cable_label: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    pub length_m: Option<Option<f64>>,
    pub tested: Option<bool>,
    // 端点可选更新（编辑模态框复用新建表单时一并提交）
    pub a_endpoint_type: Option<String>,
    pub a_endpoint_id: Option<Uuid>,
    pub b_endpoint_type: Option<String>,
    pub b_endpoint_id: Option<Uuid>,
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

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    #[test]
    fn test_cable_link_create_valid() -> Result<(), serde_json::Error> {
        let req: CableLinkCreate = serde_json::from_value(serde_json::json!({
            "a_endpoint_type": "net_outlet",
            "a_endpoint_id": Uuid::new_v4(),
            "b_endpoint_type": "patch_panel",
            "b_endpoint_id": Uuid::new_v4(),
            "link_type": "cat6",
            "cable_label": "L-101",
            "length_m": 12.5,
            "tested": true
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.length_m, Some(12.5));
        Ok(())
    }

    #[test]
    fn test_cable_link_create_label_too_long() -> Result<(), serde_json::Error> {
        // 线缆标签最长 50 字符
        let req: CableLinkCreate = serde_json::from_value(serde_json::json!({
            "a_endpoint_type": "net_outlet",
            "a_endpoint_id": Uuid::new_v4(),
            "b_endpoint_type": "patch_panel",
            "b_endpoint_id": Uuid::new_v4(),
            "cable_label": "L".repeat(51)
        }))?;
        let Err(errors) = req.validate() else {
            panic!("超长标签应被拒绝");
        };
        assert!(errors.errors().contains_key("cable_label"));
        Ok(())
    }

    #[test]
    fn test_cable_link_update_null_clear_semantics() -> Result<(), serde_json::Error> {
        // cable_label / length_m 双层 Option：null 表示清除、缺失表示不修改
        let missing: CableLinkUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert_eq!(missing.cable_label, None);
        assert_eq!(missing.length_m, None);

        let cleared: CableLinkUpdate = serde_json::from_value(serde_json::json!({
            "cable_label": null,
            "length_m": null
        }))?;
        assert_eq!(cleared.cable_label, Some(None));
        assert_eq!(cleared.length_m, Some(None));

        let set: CableLinkUpdate = serde_json::from_value(serde_json::json!({
            "cable_label": "L-202",
            "length_m": 30.0
        }))?;
        assert_eq!(set.cable_label, Some(Some("L-202".to_string())));
        assert_eq!(set.length_m, Some(Some(30.0)));
        assert!(set.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_cable_link_update_label_too_long_rejected() -> Result<(), serde_json::Error> {
        // 修复后：双层 Option 的长度经 custom 函数校验，超长标签被拒绝
        let req: CableLinkUpdate = serde_json::from_value(serde_json::json!({
            "cable_label": "L".repeat(51)
        }))?;
        let Err(errors) = req.validate() else {
            panic!("超长标签应被拒绝");
        };
        assert!(errors.errors().contains_key("cable_label"));

        // 50 字符边界值合法；null（清除）与缺失均合法
        let ok: CableLinkUpdate = serde_json::from_value(serde_json::json!({
            "cable_label": "L".repeat(50)
        }))?;
        assert!(ok.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_cable_link_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        let link = CableLink {
            id: Uuid::new_v4(),
            a_endpoint_type: "net_outlet".to_string(),
            a_endpoint_id: Uuid::new_v4(),
            b_endpoint_type: "device_interface".to_string(),
            b_endpoint_id: Uuid::new_v4(),
            link_type: "cat6".to_string(),
            cable_label: Some("L-101".to_string()),
            length_m: Some(12.5),
            tested: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&link)?;
        let back: CableLink = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_cable_path_node_serde_roundtrip() -> Result<(), serde_json::Error> {
        let node = CablePathNode {
            hop_idx: 0,
            node_type: "net_outlet".to_string(),
            node_id: Uuid::new_v4(),
            node_label: Some("D101".to_string()),
            cable_id: Some(Uuid::new_v4()),
            cable_label: None,
            hop_type: "start".to_string(),
        };
        let first = serde_json::to_value(&node)?;
        let back: CablePathNode = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }
}
