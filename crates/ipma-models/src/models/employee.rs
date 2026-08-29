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
    #[validate(email(message = "server.common.validation.email_format"))]
    pub email: Option<String>,
    pub hire_date: Option<NaiveDate>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct EmployeeUpdate {
    #[validate(length(min = 1, max = 50, message = "server.employee.validation.name_length"))]
    pub name: Option<String>,
    #[validate(length(max = 10, message = "server.employee.validation.gender_invalid"))]
    pub gender: Option<String>,
    #[validate(length(max = 20, message = "server.employee.validation.phone_length"))]
    pub phone: Option<String>,
    #[validate(email(message = "server.common.validation.email_format"))]
    pub email: Option<String>,
    pub hire_date: Option<NaiveDate>,
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
}
