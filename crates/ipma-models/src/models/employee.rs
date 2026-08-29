//! 员工（employees）模型：挂在组织节点下的基础人员信息，
//! 供工位管理人选择、设备 IP 分配邮件通知等场景使用。

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Employee {
    pub id: Uuid,
    pub org_id: Uuid,
    pub org_name: Option<String>,
    pub name: String,
    pub gender: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub hire_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct EmployeeCreate {
    pub org_id: Uuid,
    #[validate(length(min = 1, max = 50, message = "server.employee.validation.name_length"))]
    pub name: String,
    /// male / female / unknown，缺省 unknown
    #[validate(length(max = 10, message = "server.employee.validation.gender_invalid"))]
    pub gender: Option<String>,
    #[validate(length(max = 20, message = "server.employee.validation.phone_length"))]
    pub phone: Option<String>,
    #[validate(custom(
        function = "crate::models::validate_email_blankable_opt",
        message = "server.common.validation.email_format"
    ))]
    pub email: Option<String>,
    pub hire_date: Option<NaiveDate>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct EmployeeUpdate {
    #[validate(length(min = 1, max = 50, message = "server.employee.validation.name_length"))]
    pub name: Option<String>,
    #[validate(length(max = 10, message = "server.employee.validation.gender_invalid"))]
    pub gender: Option<String>,
    /// 双层 Option：字段缺失不修改、JSON null 清空（SET NULL）、值设置新值
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    #[validate(custom(
        function = "crate::models::validate_phone_opt",
        message = "server.employee.validation.phone_length"
    ))]
    pub phone: Option<Option<String>>,
    /// 双层 Option：null 清空邮箱（custom 函数自动解包，Some(Some(v)) 才校验格式；
    /// 空串经 trim 后放行，handler 侧 blank_to_none 规范化为 NULL）
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    #[validate(custom(
        function = "crate::models::validate_email_blankable_opt",
        message = "server.common.validation.email_format"
    ))]
    pub email: Option<Option<String>>,
    /// 双层 Option：null 清空入职日期
    #[serde(default, deserialize_with = "crate::models::deserialize_some")]
    pub hire_date: Option<Option<NaiveDate>>,
}

impl EmployeeCreate {
    /// 规范化性别取值（非法值视为 unknown）
    pub fn normalized_gender(&self) -> String {
        match self.gender.as_deref() {
            Some("male") | Some("female") => {
                self.gender.clone().unwrap_or_else(|| "unknown".to_string())
            }
            _ => "unknown".to_string(),
        }
    }
}

impl EmployeeUpdate {
    pub fn normalized_gender(&self) -> Option<String> {
        self.gender.as_deref().map(|g| {
            if g == "male" || g == "female" {
                g.to_string()
            } else {
                "unknown".to_string()
            }
        })
    }
}

/// 手机号仅允许数字与 + - 空格（在 handler 中对非空值校验）
#[must_use]
pub fn is_valid_phone(phone: &str) -> bool {
    !phone.is_empty()
        && phone.len() <= 20
        && phone
            .chars()
            .all(|c| c.is_ascii_digit() || c == '+' || c == '-' || c == ' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 员工创建_合法输入通过() -> Result<(), serde_json::Error> {
        let req: EmployeeCreate = serde_json::from_value(serde_json::json!({
            "org_id": Uuid::new_v4(),
            "name": "张三",
            "gender": "male",
            "phone": "13800138000",
            "email": "zhangsan@example.com",
            "hire_date": "2024-01-15"
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.normalized_gender(), "male");
        Ok(())
    }

    #[test]
    fn 员工创建_缺省字段规范化为unknown() -> Result<(), serde_json::Error> {
        let req: EmployeeCreate = serde_json::from_value(serde_json::json!({
            "org_id": Uuid::new_v4(),
            "name": "李四"
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.normalized_gender(), "unknown");
        Ok(())
    }

    #[test]
    fn 员工创建_非法字段被拒绝() -> Result<(), serde_json::Error> {
        let req: EmployeeCreate = serde_json::from_value(serde_json::json!({
            "org_id": Uuid::new_v4(),
            "name": "",
            "email": "not-an-email"
        }))?;
        let Err(errors) = req.validate() else {
            panic!("非法员工应被拒绝");
        };
        assert!(errors.errors().contains_key("name"));
        assert!(errors.errors().contains_key("email"));
        Ok(())
    }

    #[test]
    fn 员工创建_空白邮箱放行() -> Result<(), serde_json::Error> {
        // 空串 / 纯空白邮箱先放行：handler 侧 blank_to_none 规范化为 NULL，
        // 非空但非法的格式仍拒绝
        for blank in ["", "   "] {
            let req: EmployeeCreate = serde_json::from_value(serde_json::json!({
                "org_id": Uuid::new_v4(),
                "name": "张三",
                "email": blank
            }))?;
            assert!(req.validate().is_ok(), "空白邮箱 {blank:?} 应放行");
        }
        let req: EmployeeCreate = serde_json::from_value(serde_json::json!({
            "org_id": Uuid::new_v4(),
            "name": "张三",
            "email": "zhangsan@example.com"
        }))?;
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn 手机号校验_合法与非法() {
        assert!(is_valid_phone("13800138000"));
        assert!(is_valid_phone("+86 138-0013-8000"));
        assert!(!is_valid_phone(""));
        assert!(!is_valid_phone("13800138000; DROP TABLE"));
        assert!(!is_valid_phone("abcd"));
    }

    #[test]
    fn 员工更新_性别规范化() -> Result<(), serde_json::Error> {
        let req: EmployeeUpdate = serde_json::from_value(serde_json::json!({ "gender": "other" }))?;
        assert_eq!(req.normalized_gender().as_deref(), Some("unknown"));

        let req: EmployeeUpdate =
            serde_json::from_value(serde_json::json!({ "gender": "female" }))?;
        assert_eq!(req.normalized_gender().as_deref(), Some("female"));
        Ok(())
    }

    #[test]
    fn 员工更新_可空字段三态语义() -> Result<(), serde_json::Error> {
        // phone/email/hire_date 双层 Option：缺失不修改、null 清空、值设置新值
        let missing: EmployeeUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert_eq!(missing.phone, None);
        assert_eq!(missing.email, None);
        assert_eq!(missing.hire_date, None);
        assert!(missing.validate().is_ok());

        let cleared: EmployeeUpdate = serde_json::from_value(serde_json::json!({
            "phone": null,
            "email": null,
            "hire_date": null
        }))?;
        assert_eq!(cleared.phone, Some(None));
        assert_eq!(cleared.email, Some(None));
        assert_eq!(cleared.hire_date, Some(None));
        assert!(cleared.validate().is_ok());

        // 非法邮箱经 custom 函数拒绝
        let bad: EmployeeUpdate =
            serde_json::from_value(serde_json::json!({ "email": "not-an-email" }))?;
        let Err(errors) = bad.validate() else {
            panic!("非法邮箱应被拒绝");
        };
        assert!(errors.errors().contains_key("email"));

        // 空串 / 纯空白邮箱放行（规范化为 NULL 的语义交由 handler 处理）
        for blank in ["", "   "] {
            let blanked: EmployeeUpdate =
                serde_json::from_value(serde_json::json!({ "email": blank }))?;
            assert_eq!(blanked.email, Some(Some(blank.to_string())));
            assert!(blanked.validate().is_ok(), "空白邮箱 {blank:?} 应放行");
        }
        Ok(())
    }
}
