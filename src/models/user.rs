//! 由 models.rs 按资源域拆分而来，字段与校验规则未变。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

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
    #[validate(length(min = 3, max = 50, message = "server.user.validation.username_length"))]
    pub username: String,
    #[validate(length(min = 8, message = "server.user.validation.password_length"))]
    pub password: String,
    #[validate(email(message = "server.user.validation.email_invalid"))]
    #[validate(length(max = 100, message = "server.user.validation.email_length"))]
    pub email: String,
    #[validate(custom(
        function = "crate::models::validate_role",
        message = "server.user.validation.role_invalid"
    ))]
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UserUpdate {
    #[validate(email(message = "server.user.validation.email_invalid"))]
    #[validate(length(max = 100, message = "server.user.validation.email_length"))]
    pub email: Option<String>,
    #[validate(custom(
        function = "crate::models::validate_role_option",
        message = "server.user.validation.role_invalid"
    ))]
    pub role: Option<String>,
    pub status: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UserLogin {
    #[validate(length(min = 3, max = 50, message = "server.user.validation.username_length"))]
    pub username: String,
    #[validate(length(min = 8, message = "server.user.validation.password_length"))]
    pub password: String,
    pub remember_me: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct ForgotPasswordRequest {
    #[validate(email(message = "server.user.validation.email_invalid"))]
    #[validate(length(max = 100, message = "server.user.validation.email_length"))]
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct ResetPasswordRequest {
    #[validate(length(min = 1))]
    pub token: String,
    #[validate(length(min = 8, message = "server.user.validation.password_length"))]
    pub new_password: String,
}

// ==================== 2FA 模型 ====================

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct TwoFactorLoginRequest {
    #[validate(length(min = 3, max = 50, message = "server.user.validation.username_length"))]
    pub username: String,
    #[validate(length(min = 8, message = "server.user.validation.password_length"))]
    pub password: Option<String>,
    #[validate(length(min = 6, max = 6, message = "server.auth.validation.code_length"))]
    pub two_factor_code: String,
    pub remember_me: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SendTwoFactorCodeRequest {
    #[validate(length(min = 3, max = 50, message = "server.user.validation.username_length"))]
    pub username: String,
    pub password: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SendLoginCodeRequest {
    #[validate(email(message = "server.user.validation.email_invalid"))]
    #[validate(length(max = 100, message = "server.user.validation.email_length"))]
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct EmailLoginRequest {
    #[validate(email(message = "server.common.validation.email_format"))]
    pub email: String,
    #[validate(length(min = 6, max = 6, message = "server.auth.validation.code_length"))]
    pub code: String,
    pub remember_me: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct LdapLoginRequest {
    #[validate(length(min = 1, max = 50, message = "server.user.validation.username_length"))]
    pub username: String,
    #[validate(length(min = 1, message = "server.auth.validation.password_required"))]
    pub password: String,
    pub remember_me: Option<bool>,
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    /// 构造合法的 UserCreate JSON
    fn valid_user_create_json() -> serde_json::Value {
        serde_json::json!({
            "username": "alice",
            "password": "password123",
            "email": "alice@example.com",
            "role": "admin"
        })
    }

    #[test]
    fn test_user_create_valid() -> Result<(), serde_json::Error> {
        let req: UserCreate = serde_json::from_value(valid_user_create_json())?;
        assert!(req.validate().is_ok(), "合法输入应通过校验");
        assert_eq!(req.username, "alice");
        assert_eq!(req.role, "admin");
        Ok(())
    }

    #[test]
    fn test_user_create_username_too_short() -> Result<(), serde_json::Error> {
        let mut json = valid_user_create_json();
        json["username"] = serde_json::json!("ab"); // 少于 3 位
        let req: UserCreate = serde_json::from_value(json)?;
        let Err(errors) = req.validate() else {
            panic!("过短用户名应被拒绝");
        };
        assert!(errors.errors().contains_key("username"));
        Ok(())
    }

    #[test]
    fn test_user_create_username_too_long() -> Result<(), serde_json::Error> {
        let mut json = valid_user_create_json();
        json["username"] = serde_json::json!("a".repeat(51)); // 超过 50 位
        let req: UserCreate = serde_json::from_value(json)?;
        assert!(req.validate().is_err());
        Ok(())
    }

    #[test]
    fn test_user_create_password_too_short() -> Result<(), serde_json::Error> {
        let mut json = valid_user_create_json();
        json["password"] = serde_json::json!("1234567"); // 少于 8 位
        let req: UserCreate = serde_json::from_value(json)?;
        let Err(errors) = req.validate() else {
            panic!("过短密码应被拒绝");
        };
        assert!(errors.errors().contains_key("password"));
        Ok(())
    }

    #[test]
    fn test_user_create_email_invalid() -> Result<(), serde_json::Error> {
        let mut json = valid_user_create_json();
        json["email"] = serde_json::json!("not-an-email");
        let req: UserCreate = serde_json::from_value(json)?;
        let Err(errors) = req.validate() else {
            panic!("非法邮箱应被拒绝");
        };
        assert!(errors.errors().contains_key("email"));
        Ok(())
    }

    #[test]
    fn test_user_create_role_invalid() -> Result<(), serde_json::Error> {
        let mut json = valid_user_create_json();
        json["role"] = serde_json::json!("superadmin");
        let req: UserCreate = serde_json::from_value(json)?;
        let Err(errors) = req.validate() else {
            panic!("非法角色应被拒绝");
        };
        assert!(errors.errors().contains_key("role"));
        Ok(())
    }

    #[test]
    fn test_user_create_serde_roundtrip() -> Result<(), serde_json::Error> {
        // 序列化 → 反序列化 → 再序列化，两次 JSON 应一致
        let req: UserCreate = serde_json::from_value(valid_user_create_json())?;
        let first = serde_json::to_value(&req)?;
        let back: UserCreate = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn test_user_update_all_none_valid() -> Result<(), serde_json::Error> {
        // 空对象：全部字段缺省为 None，校验通过
        let req: UserUpdate = serde_json::from_value(serde_json::json!({}))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.email, None);
        assert_eq!(req.role, None);
        assert_eq!(req.status, None);
        Ok(())
    }

    #[test]
    fn test_user_update_full_valid() -> Result<(), serde_json::Error> {
        let req: UserUpdate = serde_json::from_value(serde_json::json!({
            "email": "bob@example.com",
            "role": "user",
            "status": false
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.status, Some(false));
        Ok(())
    }

    #[test]
    fn test_user_update_invalid_fields() -> Result<(), serde_json::Error> {
        let mut json = serde_json::json!({
            "email": "bad-email",
            "role": "guest"
        });
        let req: UserUpdate = serde_json::from_value(json.clone())?;
        let Err(errors) = req.validate() else {
            panic!("非法邮箱与角色应被拒绝");
        };
        assert!(errors.errors().contains_key("email"));
        assert!(errors.errors().contains_key("role"));

        // 单独非法角色也拒绝
        json["email"] = serde_json::json!("bob@example.com");
        let req2: UserUpdate = serde_json::from_value(json)?;
        let Err(errors2) = req2.validate() else {
            panic!("非法角色应被拒绝");
        };
        assert!(errors2.errors().contains_key("role"));
        Ok(())
    }

    #[test]
    fn test_user_login_valid_and_invalid() -> Result<(), serde_json::Error> {
        let req: UserLogin = serde_json::from_value(serde_json::json!({
            "username": "alice",
            "password": "password123",
            "remember_me": true
        }))?;
        assert!(req.validate().is_ok());
        assert_eq!(req.remember_me, Some(true));

        // 用户名过短拒绝
        let short: UserLogin = serde_json::from_value(serde_json::json!({
            "username": "ab",
            "password": "password123"
        }))?;
        let Err(errors) = short.validate() else {
            panic!("过短用户名应被拒绝");
        };
        assert!(errors.errors().contains_key("username"));

        // 密码过短拒绝
        let weak: UserLogin = serde_json::from_value(serde_json::json!({
            "username": "alice",
            "password": "short"
        }))?;
        let Err(errors2) = weak.validate() else {
            panic!("过短密码应被拒绝");
        };
        assert!(errors2.errors().contains_key("password"));
        Ok(())
    }

    #[test]
    fn test_user_login_remember_me_default_none() -> Result<(), serde_json::Error> {
        // remember_me 可省略，反序列化为 None
        let req: UserLogin = serde_json::from_value(serde_json::json!({
            "username": "alice",
            "password": "password123"
        }))?;
        assert_eq!(req.remember_me, None);
        assert!(req.validate().is_ok());
        Ok(())
    }

    #[test]
    fn test_forgot_password_request_email() -> Result<(), serde_json::Error> {
        let ok: ForgotPasswordRequest =
            serde_json::from_value(serde_json::json!({ "email": "a@b.com" }))?;
        assert!(ok.validate().is_ok());

        let bad: ForgotPasswordRequest =
            serde_json::from_value(serde_json::json!({ "email": "no-at-sign" }))?;
        let Err(errors) = bad.validate() else {
            panic!("非法邮箱应被拒绝");
        };
        assert!(errors.errors().contains_key("email"));
        Ok(())
    }

    #[test]
    fn test_reset_password_request_validation() -> Result<(), serde_json::Error> {
        let ok: ResetPasswordRequest = serde_json::from_value(serde_json::json!({
            "token": "some-token",
            "new_password": "newpassword8"
        }))?;
        assert!(ok.validate().is_ok());

        // 空 token 拒绝
        let empty_token: ResetPasswordRequest = serde_json::from_value(serde_json::json!({
            "token": "",
            "new_password": "newpassword8"
        }))?;
        let Err(errors) = empty_token.validate() else {
            panic!("空 token 应被拒绝");
        };
        assert!(errors.errors().contains_key("token"));

        // 过短新密码拒绝
        let weak: ResetPasswordRequest = serde_json::from_value(serde_json::json!({
            "token": "some-token",
            "new_password": "short"
        }))?;
        let Err(errors2) = weak.validate() else {
            panic!("过短新密码应被拒绝");
        };
        assert!(errors2.errors().contains_key("new_password"));
        Ok(())
    }

    #[test]
    fn test_two_factor_login_code_length() -> Result<(), serde_json::Error> {
        let ok: TwoFactorLoginRequest = serde_json::from_value(serde_json::json!({
            "username": "alice",
            "two_factor_code": "123456"
        }))?;
        assert!(ok.validate().is_ok());

        // 5 位验证码拒绝
        let short: TwoFactorLoginRequest = serde_json::from_value(serde_json::json!({
            "username": "alice",
            "two_factor_code": "12345"
        }))?;
        let Err(errors) = short.validate() else {
            panic!("5 位验证码应被拒绝");
        };
        assert!(errors.errors().contains_key("two_factor_code"));
        Ok(())
    }

    #[test]
    fn test_send_two_factor_code_request() -> Result<(), serde_json::Error> {
        let ok: SendTwoFactorCodeRequest = serde_json::from_value(serde_json::json!({
            "username": "alice"
        }))?;
        assert!(ok.validate().is_ok());
        assert_eq!(ok.password, None); // password 可选

        let bad: SendTwoFactorCodeRequest =
            serde_json::from_value(serde_json::json!({ "username": "ab" }))?;
        assert!(bad.validate().is_err());
        Ok(())
    }

    #[test]
    fn test_send_login_code_and_email_login() -> Result<(), serde_json::Error> {
        let send: SendLoginCodeRequest = serde_json::from_value(serde_json::json!({
            "email": "a@b.com"
        }))?;
        assert!(send.validate().is_ok());

        let login: EmailLoginRequest = serde_json::from_value(serde_json::json!({
            "email": "a@b.com",
            "code": "123456"
        }))?;
        assert!(login.validate().is_ok());

        // 邮箱非法与验证码长度不符分别拒绝
        let bad_email: SendLoginCodeRequest =
            serde_json::from_value(serde_json::json!({ "email": "bad" }))?;
        assert!(bad_email.validate().is_err());

        let bad_code: EmailLoginRequest = serde_json::from_value(serde_json::json!({
            "email": "a@b.com",
            "code": "12345"
        }))?;
        let Err(errors) = bad_code.validate() else {
            panic!("5 位验证码应被拒绝");
        };
        assert!(errors.errors().contains_key("code"));
        Ok(())
    }

    #[test]
    fn test_ldap_login_request_validation() -> Result<(), serde_json::Error> {
        let ok: LdapLoginRequest = serde_json::from_value(serde_json::json!({
            "username": "alice",
            "password": "secret"
        }))?;
        assert!(ok.validate().is_ok());

        // LDAP 密码仅要求非空（min = 1），短密码也应通过
        let short: LdapLoginRequest = serde_json::from_value(serde_json::json!({
            "username": "alice",
            "password": "x"
        }))?;
        assert!(short.validate().is_ok());

        // 空密码拒绝
        let empty: LdapLoginRequest = serde_json::from_value(serde_json::json!({
            "username": "alice",
            "password": ""
        }))?;
        let Err(errors) = empty.validate() else {
            panic!("空密码应被拒绝");
        };
        assert!(errors.errors().contains_key("password"));

        // 空用户名拒绝
        let no_user: LdapLoginRequest =
            serde_json::from_value(serde_json::json!({ "username": "", "password": "x" }))?;
        assert!(no_user.validate().is_err());
        Ok(())
    }

    #[test]
    fn test_user_entity_serde_roundtrip() -> Result<(), serde_json::Error> {
        // User 实体含时间戳，往返序列化应保持一致
        let user = User {
            id: Uuid::new_v4(),
            username: "alice".to_string(),
            email: "alice@example.com".to_string(),
            role: "admin".to_string(),
            status: true,
            two_factor_enabled: false,
            two_factor_verified: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let first = serde_json::to_value(&user)?;
        let back: User = serde_json::from_value(first.clone())?;
        let second = serde_json::to_value(&back)?;
        assert_eq!(first, second);
        Ok(())
    }
}
