//! 初始化模块请求/响应类型。

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const VERIFICATION_CODE_EXPIRY_SECS: u64 = 15 * 60;
pub const BCRYPT_COST: u32 = 12;

/// 密码字节长度上限：bcrypt 仅处理前 72 字节，超长部分被静默截断，
/// 前 72 字节相同的口令将等价可登录，必须在入口拒绝
pub const PASSWORD_MAX_BYTES: usize = foims_common::validation::PASSWORD_MAX_BYTES;

/// 密码字节长度校验（委托 foims-common 唯一定义）
fn validate_password_bytes(password: &str) -> Result<(), validator::ValidationError> {
    foims_common::validation::validate_password_max_bytes(password)
}

/// 初始管理员角色校验（委托 foims-common 唯一定义：等保三权分立）
fn validate_init_role(role: &str) -> Result<(), validator::ValidationError> {
    foims_common::validation::validate_role(role)
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
                foims_common::log_warn!("log.init.system_time_warning", error = e);
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

/// 数据库配置页请求：连接测试与创建数据库共用同一结构。
///
/// 供初始化向导第 2 步（数据库配置）使用，用户在页面填写连接要素，
/// 由「连接测试」验证通过后写入配置文件。
#[derive(Debug, Serialize, Deserialize, validator::Validate)]
pub struct DatabaseSetupRequest {
    /// 数据库类型（下拉框，为未来扩展预留；当前仅支持 pgsql）
    #[serde(rename = "type")]
    #[validate(custom(
        function = "validate_setup_db_type",
        message = "server.init.dbcfg.type_unsupported"
    ))]
    pub db_type: String,
    #[validate(length(min = 1, max = 255, message = "server.init.dbcfg.host_invalid"))]
    pub host: String,
    /// 端口以字符串接收：若用 u16，超出范围的数字会被 serde 直接拒绝，
    /// 返回 axum 默认 422 纯文本而非项目统一的消息 key
    #[validate(custom(
        function = "validate_setup_port",
        message = "server.init.dbcfg.port_invalid"
    ))]
    pub port: String,
    #[validate(length(min = 1, max = 63, message = "server.init.db.identifier_invalid"))]
    #[validate(custom(
        function = "validate_setup_db_name",
        message = "server.init.db.identifier_invalid"
    ))]
    pub database: String,
    #[validate(length(min = 1, max = 63, message = "server.init.dbcfg.username_invalid"))]
    pub username: String,
    #[validate(length(min = 1, max = 256, message = "server.init.dbcfg.password_invalid"))]
    pub password: String,
}

/// 数据库配置页类型下拉框当前唯一支持的取值
pub const SUPPORTED_DB_TYPE: &str = "pgsql";

fn validate_setup_db_type(db_type: &str) -> Result<(), validator::ValidationError> {
    if db_type == SUPPORTED_DB_TYPE {
        Ok(())
    } else {
        Err(validator::ValidationError::new("db_type"))
    }
}

fn validate_setup_port(port: &str) -> Result<(), validator::ValidationError> {
    // 端口 0 保留不分配，与 PG 实际可监听范围一致
    match port.parse::<u16>() {
        Ok(n) if n >= 1 => Ok(()),
        _ => Err(validator::ValidationError::new("port")),
    }
}

/// 数据库名规则与 operations::validate_identifier 一致：仅字母、数字、下划线
fn validate_setup_db_name(name: &str) -> Result<(), validator::ValidationError> {
    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(validator::ValidationError::new("database"))
    }
}

impl DatabaseSetupRequest {
    /// 转换为完整数据库配置：仅连接五要素来自页面输入，池参数沿用
    /// 当前配置文件加载的值（连接测试通过后只更新连接信息，
    /// 池参数仍以配置文件为准）
    #[must_use]
    pub fn into_database_config(self, pool_template: &DatabaseConfig) -> DatabaseConfig {
        // port 合法性已由 validate_setup_port 在 handler 校验阶段保证，
        // 此处兜底回退默认端口，避免解析失败路径
        let port = self.port.parse::<u16>().unwrap_or(5432);
        DatabaseConfig {
            host: self.host,
            port,
            database: self.database,
            username: self.username,
            password: self.password,
            max_connections: pool_template.max_connections,
            min_connections: pool_template.min_connections,
            acquire_timeout_secs: pool_template.acquire_timeout_secs,
            idle_timeout_secs: pool_template.idle_timeout_secs,
            max_lifetime_secs: pool_template.max_lifetime_secs,
            query_timeout_secs: pool_template.query_timeout_secs,
            health_check_interval_secs: pool_template.health_check_interval_secs,
        }
    }
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

/// 数据库连接配置：直接复用 foims-common 的唯一定义
///（原先在此处逐字段复制了一份，违反"跨 crate 共享类型放 foims-common"规范）
pub use foims_common::config::DatabaseConfig;

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
            "database": "foims",
            "username": "foims",
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
            "database": "foims2",
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
            ("db.example.com", 5433, "foims2")
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

    /// 构造各字段均合法的数据库配置页请求
    fn valid_setup_request() -> DatabaseSetupRequest {
        DatabaseSetupRequest {
            db_type: "pgsql".to_string(),
            host: "127.0.0.1".to_string(),
            port: "5432".to_string(),
            database: "foims".to_string(),
            username: "foims".to_string(),
            password: "secret".to_string(),
        }
    }

    /// 断言配置页请求指定字段校验失败且返回既定消息 key
    fn assert_setup_field_error(req: &DatabaseSetupRequest, field: &str, key: &str) {
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
    fn 配置页请求_全部字段合法时校验通过() {
        let Ok(()) = valid_setup_request().validate() else {
            panic!("合法请求不应产生校验错误");
        };
    }

    #[test]
    fn 配置页请求_json字段名使用type() {
        // serde rename：前端提交 "type" 字段映射到 db_type
        let req: DatabaseSetupRequest = serde_json::from_str(
            r#"{"type":"pgsql","host":"h","port":"5432","database":"d","username":"u","password":"p"}"#,
        )
        .unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(req.db_type, "pgsql");
        assert_eq!(req.host, "h");
    }

    #[test]
    fn 配置页请求_类型仅支持pgsql() {
        let mut req = valid_setup_request();
        req.db_type = "mysql".to_string();
        assert_setup_field_error(&req, "db_type", "server.init.dbcfg.type_unsupported");

        let mut req = valid_setup_request();
        req.db_type = String::new();
        assert_setup_field_error(&req, "db_type", "server.init.dbcfg.type_unsupported");
    }

    #[test]
    fn 配置页请求_主机非空且限长() {
        let mut req = valid_setup_request();
        req.host = String::new();
        assert_setup_field_error(&req, "host", "server.init.dbcfg.host_invalid");

        let mut req = valid_setup_request();
        req.host = "h".repeat(256);
        assert_setup_field_error(&req, "host", "server.init.dbcfg.host_invalid");
    }

    #[test]
    fn 配置页请求_端口范围校验() {
        // 非数字 / 0 / 超出 u16 均拒绝；边界值 1 与 65535 合法
        for bad in ["abc", "", "0", "65536", "-1", "54.5", " 5432"] {
            let mut req = valid_setup_request();
            req.port = bad.to_string();
            assert_setup_field_error(&req, "port", "server.init.dbcfg.port_invalid");
        }
        for good in ["1", "5432", "65535"] {
            let mut req = valid_setup_request();
            req.port = good.to_string();
            assert!(req.validate().is_ok(), "端口 {good} 应合法",);
        }
    }

    #[test]
    fn 配置页请求_数据库名规则与标识符校验一致() {
        for bad in ["", "bad-name", "db;DROP", "名 称", "a".repeat(64).as_str()] {
            let mut req = valid_setup_request();
            req.database = bad.to_string();
            assert_setup_field_error(&req, "database", "server.init.db.identifier_invalid");
        }
        for good in ["foims", "db_2024", "A1_b"] {
            let mut req = valid_setup_request();
            req.database = good.to_string();
            assert!(req.validate().is_ok(), "库名 {good} 应合法");
        }
    }

    #[test]
    fn 配置页请求_用户名与密码非空限长() {
        let mut req = valid_setup_request();
        req.username = String::new();
        assert_setup_field_error(&req, "username", "server.init.dbcfg.username_invalid");

        let mut req = valid_setup_request();
        req.username = "u".repeat(64);
        assert_setup_field_error(&req, "username", "server.init.dbcfg.username_invalid");

        let mut req = valid_setup_request();
        req.password = String::new();
        assert_setup_field_error(&req, "password", "server.init.dbcfg.password_invalid");

        let mut req = valid_setup_request();
        req.password = "p".repeat(257);
        assert_setup_field_error(&req, "password", "server.init.dbcfg.password_invalid");
    }

    #[test]
    fn 配置页请求_转换保留池参数模板值() {
        let req = valid_setup_request();
        // 修改连接要素，池参数取模板值
        let template: DatabaseConfig = serde_json::from_str(
            r#"{"host":"old","port":1,"database":"old","username":"old","password":"old","max_connections":33,"min_connections":7,"acquire_timeout_secs":9,"idle_timeout_secs":11,"max_lifetime_secs":77,"query_timeout_secs":5,"health_check_interval_secs":13}"#,
        )
        .unwrap_or_else(|e| panic!("反序列化失败: {e}"));

        let cfg = req.into_database_config(&template);
        assert_eq!(
            (cfg.host.as_str(), cfg.port, cfg.database.as_str()),
            ("127.0.0.1", 5432, "foims")
        );
        assert_eq!(cfg.username, "foims");
        assert_eq!(cfg.password, "secret");
        assert_eq!(cfg.max_connections, 33);
        assert_eq!(cfg.min_connections, 7);
        assert_eq!(cfg.acquire_timeout_secs, 9);
        assert_eq!(cfg.idle_timeout_secs, 11);
        assert_eq!(cfg.max_lifetime_secs, 77);
        assert_eq!(cfg.query_timeout_secs, 5);
        assert_eq!(cfg.health_check_interval_secs, 13);
    }
}
