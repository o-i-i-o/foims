//! 初始化模块请求/响应类型。

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const VERIFICATION_CODE_EXPIRY_SECS: u64 = 15 * 60;
pub const BCRYPT_COST: u32 = 12;

/// 密码字节长度上限：bcrypt 仅处理前 72 字节，超长部分被静默截断，
/// 前 72 字节相同的口令将等价可登录，必须在入口拒绝
pub const PASSWORD_MAX_BYTES: usize = 72;

/// 密码字节长度校验（<=72 字节，multibyte 字符按 UTF-8 字节计）
fn validate_password_bytes(password: &str) -> Result<(), validator::ValidationError> {
    if password.len() <= PASSWORD_MAX_BYTES {
        Ok(())
    } else {
        Err(validator::ValidationError::new("length"))
    }
}

/// 初始管理员角色合法集合（等保三权分立：admin 系统管理员 / secadmin
/// 安全管理员 / auditor 审计管理员 / user 普通用户，与 ipma-models
/// 的 validate_role 口径一致）
fn validate_init_role(role: &str) -> Result<(), validator::ValidationError> {
    match role {
        "admin" | "user" | "secadmin" | "auditor" => Ok(()),
        _ => Err(validator::ValidationError::new("role")),
    }
}

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
            .unwrap_or_else(|e| {
                ipma_common::log_warn!("log.init.system_time_warning", error = e);
                std::time::Duration::from_secs(0)
            })
            .as_secs();
        Self {
            code,
            created_at: now,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, validator::Validate)]
pub struct InitRequest {
    #[validate(length(min = 3, max = 50, message = "server.init.validation.username_length"))]
    pub username: String,
    #[validate(length(min = 8, message = "server.init.validation.password_length"))]
    #[validate(custom(
        function = "validate_password_bytes",
        message = "server.init.validation.password_length"
    ))]
    pub password: String,
    #[validate(email(message = "server.init.validation.email_invalid"))]
    pub email: String,
    #[validate(length(min = 1, max = 20, message = "server.init.validation.role_length"))]
    #[validate(custom(
        function = "validate_init_role",
        message = "server.user.validation.role_invalid"
    ))]
    pub role: String,
    #[validate(length(
        min = 16,
        max = 16,
        message = "server.init.validation.verification_length"
    ))]
    pub verification: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDatabaseRequest {
    pub verification: String,
}

#[derive(Debug, Serialize)]
pub struct CreateDatabaseResponse {
    pub backup_file: Option<String>,
    /// 消息 key（由前端翻译），与外层 ApiResponse.message 一致
    pub message: String,
}

/// 数据库连接配置：直接复用 ipma-common 的唯一定义
///（原先在此处逐字段复制了一份，违反"跨 crate 共享类型放 ipma-common"规范）
pub use ipma_common::config::DatabaseConfig;

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    /// 构造各字段均合法的初始化请求
    fn valid_request() -> InitRequest {
        InitRequest {
            username: "admin".to_string(),
            password: "admin123".to_string(),
            email: "admin@example.com".to_string(),
            role: "admin".to_string(),
            verification: "A".repeat(16),
        }
    }

    /// 断言指定字段校验失败且返回既定消息 key
    fn assert_field_error(req: &InitRequest, field: &str, key: &str) {
        let Err(errors) = req.validate() else {
            panic!("字段 {field} 非法时校验应失败");
        };
        let field_errs = errors.field_errors();
        let errs = field_errs
            .get(field)
            .unwrap_or_else(|| panic!("应包含字段 {field} 的错误"));
        let first = errs
            .first()
            .unwrap_or_else(|| panic!("字段 {field} 应至少有一个错误"));
        let Some(message) = &first.message else {
            panic!("字段 {field} 的错误应携带消息 key");
        };
        assert_eq!(message.as_ref(), key);
    }

    #[test]
    fn init请求_全部字段合法时校验通过() {
        let Ok(()) = valid_request().validate() else {
            panic!("合法请求不应产生校验错误");
        };
    }

    #[test]
    fn init请求_用户名长度校验() {
        // 下界：2 位过短；上界：51 位过长；边界值 3 与 50 合法
        let mut req = valid_request();
        req.username = "ab".to_string();
        assert_field_error(&req, "username", "server.init.validation.username_length");

        let mut req = valid_request();
        req.username = "a".repeat(51);
        assert_field_error(&req, "username", "server.init.validation.username_length");

        let mut req = valid_request();
        req.username = "a".repeat(3);
        assert!(req.validate().is_ok());
        let mut req = valid_request();
        req.username = "a".repeat(50);
        assert!(req.validate().is_ok());
    }

    #[test]
    fn init请求_密码长度校验_下界与字节上界() {
        let mut req = valid_request();
        req.password = "1234567".to_string();
        assert_field_error(&req, "password", "server.init.validation.password_length");

        let mut req = valid_request();
        req.password = "12345678".to_string();
        assert!(req.validate().is_ok());

        // 字节上界：恰 72 字节合法（bcrypt 处理上限），73 字节拒绝
        let mut req = valid_request();
        req.password = "x".repeat(72);
        assert!(req.validate().is_ok(), "72 字节密码应合法");
        let mut req = valid_request();
        req.password = "x".repeat(73);
        assert_field_error(&req, "password", "server.init.validation.password_length");

        // multibyte 字符按字节计：36 个中文字符 = 108 字节，超限拒绝
        let mut req = valid_request();
        req.password = "密".repeat(36);
        assert_field_error(&req, "password", "server.init.validation.password_length");
    }

    #[test]
    fn init请求_邮箱格式校验() {
        let mut req = valid_request();
        req.email = "not-an-email".to_string();
        assert_field_error(&req, "email", "server.init.validation.email_invalid");

        let mut req = valid_request();
        req.email = "a@b".to_string();
        assert!(req.validate().is_ok(), "最简邮箱 a@b 应视为合法");
    }

    #[test]
    fn init请求_角色长度与合法集合校验() {
        let mut req = valid_request();
        req.role = String::new();
        assert_field_error(&req, "role", "server.init.validation.role_length");

        let mut req = valid_request();
        req.role = "r".repeat(21);
        assert_field_error(&req, "role", "server.init.validation.role_length");

        // 合法集合：admin/user/secadmin/auditor
        for role in ["admin", "user", "secadmin", "auditor"] {
            let mut req = valid_request();
            req.role = role.to_string();
            assert!(req.validate().is_ok(), "角色 {role} 应合法");
        }

        // 集合外的值即使长度合法也必须拒绝（如 "r".repeat(20)、超级管理员）
        for role in [
            "r".repeat(20),
            "superadmin".to_string(),
            "root".to_string(),
            "Admin".to_string(),
        ] {
            let mut req = valid_request();
            req.role = role.clone();
            assert_field_error(&req, "role", "server.user.validation.role_invalid");
        }
    }

    #[test]
    fn init请求_验证码长度必须为16() {
        let mut req = valid_request();
        req.verification = "A".repeat(15);
        assert_field_error(
            &req,
            "verification",
            "server.init.validation.verification_length",
        );

        let mut req = valid_request();
        req.verification = "A".repeat(17);
        assert_field_error(
            &req,
            "verification",
            "server.init.validation.verification_length",
        );
    }

    #[test]
    fn verification_code_new_记录创建时间() {
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let code = VerificationCode::new("X".repeat(16));
        let after = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        assert_eq!(code.code, "X".repeat(16));
        assert!(
            code.created_at >= before && code.created_at <= after,
            "created_at 应为构造时刻的 Unix 秒"
        );
    }

    #[test]
    fn database_config_缺省字段使用默认值() {
        let json = r#"{
            "host": "127.0.0.1",
            "port": 5432,
            "database": "ipma",
            "username": "ipma",
            "password": "secret"
        }"#;
        let cfg: DatabaseConfig = serde_json::from_str(json).unwrap_or_else(|e| {
            panic!("反序列化失败: {e}");
        });
        assert_eq!(cfg.max_connections, 10);
        assert_eq!(cfg.min_connections, 5);
        assert_eq!(cfg.acquire_timeout_secs, 15);
        assert_eq!(cfg.idle_timeout_secs, 60);
        assert_eq!(cfg.max_lifetime_secs, 1800);
        assert_eq!(cfg.query_timeout_secs, 30);
        assert_eq!(cfg.health_check_interval_secs, 30);
    }

    #[test]
    fn database_config_显式字段覆盖默认值() {
        let json = r#"{
            "host": "db.example.com",
            "port": 5433,
            "database": "ipma2",
            "username": "u",
            "password": "p",
            "max_connections": 20,
            "min_connections": 1,
            "acquire_timeout_secs": 5,
            "idle_timeout_secs": 10,
            "max_lifetime_secs": 600,
            "query_timeout_secs": 8,
            "health_check_interval_secs": 15
        }"#;
        let cfg: DatabaseConfig = serde_json::from_str(json).unwrap_or_else(|e| {
            panic!("反序列化失败: {e}");
        });
        assert_eq!(
            (cfg.host.as_str(), cfg.port, cfg.database.as_str()),
            ("db.example.com", 5433, "ipma2")
        );
        assert_eq!(cfg.max_connections, 20);
        assert_eq!(cfg.min_connections, 1);
        assert_eq!(cfg.acquire_timeout_secs, 5);
        assert_eq!(cfg.idle_timeout_secs, 10);
        assert_eq!(cfg.max_lifetime_secs, 600);
        assert_eq!(cfg.query_timeout_secs, 8);
        assert_eq!(cfg.health_check_interval_secs, 15);
    }

    #[test]
    fn create请求_反序列化() {
        let req: CreateDatabaseRequest =
            serde_json::from_str(r#"{"verification": "ABCDEFGHIJKLMNOP"}"#)
                .unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(req.verification, "ABCDEFGHIJKLMNOP");
    }
}
