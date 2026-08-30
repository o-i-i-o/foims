//! 认证提取器：当前用户与管理员权限守卫（axum FromRequestParts）。

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::utils::{JwtClaims, extract_cookie_from_parts, extract_token_from_parts};
use foims_common::net::is_secure_from_parts;
use foims_common::{AppError, msg};

pub struct AuthUser {
    pub sub: String,
    pub username: String,
    pub role: String,
    pub device_fingerprint: Option<String>,
    pub ip_address: Option<String>,
}

impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match parts.extensions.get::<JwtClaims>() {
            Some(c) => Ok(AuthUser {
                sub: c.sub.clone(),
                username: c.username.clone(),
                role: c.role.clone(),
                device_fingerprint: c.device_fingerprint.clone(),
                ip_address: c.ip_address.clone(),
            }),
            None => Err(AppError::Unauthorized(msg("server.auth.auth_failed"))),
        }
    }
}

pub struct AdminUser {
    pub sub: String,
    pub username: String,
}

impl<S: Send + Sync> FromRequestParts<S> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match parts.extensions.get::<JwtClaims>() {
            Some(c) if c.role == "admin" => Ok(AdminUser {
                sub: c.sub.clone(),
                username: c.username.clone(),
            }),
            Some(_) => Err(AppError::Forbidden(msg("server.auth.admin_required"))),
            None => Err(AppError::Unauthorized(msg("server.auth.auth_failed"))),
        }
    }
}

/// 安全管理员（等保三权分立）：admin 或 secadmin 可用。
///
/// 管辖用户账户管理与安全策略（含密码策略、fail2ban、会话配置）；
/// admin 为系统管理员，具有全部权限。
pub struct SecAdminUser {
    pub sub: String,
    pub username: String,
}

impl<S: Send + Sync> FromRequestParts<S> for SecAdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match parts.extensions.get::<JwtClaims>() {
            Some(c) if c.role == "admin" || c.role == "secadmin" => Ok(SecAdminUser {
                sub: c.sub.clone(),
                username: c.username.clone(),
            }),
            Some(_) => Err(AppError::Forbidden(msg("server.auth.secadmin_required"))),
            None => Err(AppError::Unauthorized(msg("server.auth.auth_failed"))),
        }
    }
}

/// 管理员或审计员（等保三权分立）：admin 或 auditor 可用。
///
/// 供日志只读访问等审计类端点使用：admin 拥有全部权限，
/// auditor 仅获得只读审计视图（角色拦截由中间件与 handler 共同保证）。
pub struct AdminOrAuditorUser {
    pub sub: String,
    pub username: String,
}

impl<S: Send + Sync> FromRequestParts<S> for AdminOrAuditorUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match parts.extensions.get::<JwtClaims>() {
            Some(c) if c.role == "admin" || c.role == "auditor" => Ok(AdminOrAuditorUser {
                sub: c.sub.clone(),
                username: c.username.clone(),
            }),
            Some(_) => Err(AppError::Forbidden(msg(
                "server.auth.admin_or_auditor_required",
            ))),
            None => Err(AppError::Unauthorized(msg("server.auth.auth_failed"))),
        }
    }
}

/// 提取 access_token（优先 Cookie `access_token`，其次 Authorization: Bearer 头）
pub struct AccessToken(pub Option<String>);

impl<S: Send + Sync> FromRequestParts<S> for AccessToken {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(AccessToken(extract_token_from_parts(parts)))
    }
}

/// 提取 refresh_token（优先 Cookie `refresh_token`，其次回退到 access_token 提取逻辑）
pub struct RefreshToken(pub Option<String>);

impl<S: Send + Sync> FromRequestParts<S> for RefreshToken {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = extract_cookie_from_parts(parts, "refresh_token")
            .or_else(|| extract_token_from_parts(parts));
        Ok(RefreshToken(token))
    }
}

/// 提取请求是否为 HTTPS（基于 X-Forwarded-Proto 头）
pub struct SecureFlag(pub bool);

impl<S: Send + Sync> FromRequestParts<S> for SecureFlag {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(SecureFlag(is_secure_from_parts(parts)))
    }
}
