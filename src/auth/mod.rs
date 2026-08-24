//! 认证与用户管理模块。

pub mod captcha;
pub mod extractor;
pub mod ldap;
pub mod login;
pub mod password_policy;
pub mod sso;
pub mod user;
pub mod utils;

/// 获取图形验证码（公开端点，登录页按需加载）
pub async fn get_captcha() -> Result<axum::response::Response, crate::error::AppError> {
    let challenge = captcha::generate();
    Ok(crate::error::ok_json(
        serde_json::json!({
            "captcha_id": challenge.captcha_id,
            "svg": challenge.svg,
        }),
        "server.common.success",
    ))
}
