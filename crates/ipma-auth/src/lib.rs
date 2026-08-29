//! IPMA 认证与用户管理。
//!
//! 登录（本地/LDAP/SSO/2FA）、JWT 签发与校验、用户管理、等保密码策略、
//! 应用层 fail2ban、SMTP 邮件与审计操作日志。handler 面向 [`provider::AuthProvider`]
//! 泛型编写，由主程序 `AppState` 实现以提供连接池、JWT 工具与配置。

pub mod app_fail2ban;
pub mod captcha;
pub mod extractor;
pub mod ldap;
pub mod login;
pub mod meta;
pub mod password_policy;
pub mod provider;
pub mod smtp;
pub mod sso;
pub mod user;
pub mod utils;

/// 获取图形验证码（公开端点，登录页按需加载）
pub async fn get_captcha() -> Result<axum::response::Response, ipma_common::AppError> {
    let challenge = captcha::generate();
    Ok(ipma_common::ok_json(
        serde_json::json!({
            "captcha_id": challenge.captcha_id,
            "svg": challenge.svg,
        }),
        "server.common.success",
    ))
}
