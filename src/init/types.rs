use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const VERIFICATION_CODE_EXPIRY_SECS: u64 = 15 * 60;
pub const BCRYPT_COST: u32 = 12;

#[derive(Debug, Clone)]
pub struct VerificationCode {
    pub code: String,
    pub created_at: u64,
}

impl VerificationCode {
    #[must_use] 
    pub fn new(code: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            code,
            created_at: now,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, validator::Validate)]
pub struct InitRequest {
    #[validate(length(min = 3, max = 50, message = "用户名长度必须在3到50个字符之间"))]
    pub username: String,
    #[validate(length(min = 8, message = "密码长度必须至少8个字符"))]
    pub password: String,
    #[validate(email(message = "请输入有效的邮箱地址"))]
    pub email: String,
    #[validate(length(min = 1, max = 20, message = "角色长度必须在1到20个字符之间"))]
    pub role: String,
    #[validate(length(min = 16, max = 16, message = "验证码长度必须为16个字符"))]
    pub verification: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDatabaseRequest {
    pub verification: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportDatabaseRequest {
    pub verification: String,
}

#[derive(Debug, Serialize)]
pub struct CreateDatabaseResponse {
    pub backup_file: Option<String>,
    pub message: String,
}
