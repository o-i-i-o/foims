//! 认证提取器：当前用户与管理员权限守卫（axum FromRequestParts）。

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::jwt::{JwtClaims, extract_cookie_from_parts, extract_token_from_parts};
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

/// 超级管理员：拥有全部权限，居等保三权分立之上。
///
/// 系统功能与资源数据的全部读写、用户与安全策略管理、审计日志均可用。
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

/// 系统管理员（等保三权分立）：admin 或 sysadmin 可用。
///
/// 管辖系统功能：系统设置、SMTP/LDAP/SSO、证书、服务、定时任务、导入导出等；
/// 资源类数据（设备/IP/组织等）写操作不在其列（sysadmin 对资源只读）。
pub struct SysAdminUser {
    pub sub: String,
    pub username: String,
}

impl<S: Send + Sync> FromRequestParts<S> for SysAdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match parts.extensions.get::<JwtClaims>() {
            Some(c) if c.role == "admin" || c.role == "sysadmin" => Ok(SysAdminUser {
                sub: c.sub.clone(),
                username: c.username.clone(),
            }),
            Some(_) => Err(AppError::Forbidden(msg("server.auth.sysadmin_required"))),
            None => Err(AppError::Unauthorized(msg("server.auth.auth_failed"))),
        }
    }
}

/// 账户管理员（用户管理）：admin / sysadmin / secadmin 可用。
///
/// 仅供用户账户（users）的增删改查端点使用；admin 为超级管理员，
/// sysadmin 系统管理员与 secadmin 安全管理员按三权分立共管账户。
/// `role` 携带操作者角色，供 handler 做提权防护判定（超管账户仅超管可管）。
pub struct AccountAdminUser {
    pub sub: String,
    pub username: String,
    pub role: String,
}

impl<S: Send + Sync> FromRequestParts<S> for AccountAdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match parts.extensions.get::<JwtClaims>() {
            Some(c) if c.role == "admin" || c.role == "sysadmin" || c.role == "secadmin" => {
                Ok(AccountAdminUser {
                    sub: c.sub.clone(),
                    username: c.username.clone(),
                    role: c.role.clone(),
                })
            }
            Some(_) => Err(AppError::Forbidden(msg(
                "server.auth.account_admin_required",
            ))),
            None => Err(AppError::Unauthorized(msg("server.auth.auth_failed"))),
        }
    }
}

/// 安全管理员（等保三权分立）：admin 或 secadmin 可用。
///
/// 管辖安全策略：密码策略、fail2ban、日志外发；admin 为超级管理员，
/// 具有全部权限。
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
/// 供日志只读访问等审计类端点使用：admin 为超级管理员拥有全部权限，
/// auditor 登录后全站只读（角色拦截由中间件与 handler 共同保证）。
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
