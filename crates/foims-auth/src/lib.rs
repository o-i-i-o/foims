//! FOIMS 认证与用户管理。
//!
//! 登录（本地/LDAP/SSO/2FA）、JWT 签发与校验、用户管理、等保密码策略、
//! 应用层 fail2ban、SMTP 邮件与审计操作日志。handler 面向 [`provider::AuthProvider`]
//! 泛型编写，由主程序 `AppState` 实现以提供连接池、JWT 工具与配置。

// 测试代码允许 unwrap/expect（AGENTS.md「测试代码除外」）；生产代码仍由
// workspace lints + CI `-D warnings` 强制禁止。cfg(test) 仅作用于测试构建。
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod app_fail2ban;
pub mod captcha;
pub mod extractor;
pub mod jwt;
pub mod ldap;
pub mod login;
pub mod meta;
pub mod password_policy;
pub mod provider;
pub mod smtp;
pub mod sso;
pub mod user;

// 供主程序直接引用的常用提取器（完整集合见 extractor 模块）
pub use extractor::AccountAdminUser;
pub use extractor::AdminOrAuditorUser;
pub use extractor::AdminOrSecAdminUser;
pub use extractor::SysAdminUser;

/// GET /api/auth/captcha：公开的图形验证码（返回 captcha_id 与内联 SVG；
/// 5 分钟有效、一次性使用；连续登录失败达阈值后登录端点才要求携带）。
pub async fn get_captcha() -> Result<axum::response::Response, foims_common::AppError> {
    let challenge = captcha::generate();
    Ok(foims_common::ok_json(
        serde_json::json!({
            "captcha_id": challenge.captcha_id,
            "svg": challenge.svg,
        }),
        "server.common.success",
    ))
}
