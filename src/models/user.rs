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
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct EmailLoginRequest {
    #[validate(email(message = "server.user.validation.email_invalid"))]
    pub email: String,
    #[validate(length(min = 6, max = 6, message = "server.auth.validation.code_length"))]
    pub code: String,
    pub remember_me: Option<bool>,
}
