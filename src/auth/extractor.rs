use std::future::Ready;

use actix_web::{FromRequest, HttpMessage, HttpRequest, dev::Payload};

use crate::auth::utils::JwtClaims;
use crate::error::AppError;

pub struct AuthUser {
    pub sub: String,
    pub username: String,
    pub role: String,
    pub device_fingerprint: Option<String>,
    pub ip_address: Option<String>,
}

impl FromRequest for AuthUser {
    type Error = AppError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let extensions = req.extensions();
        match extensions.get::<JwtClaims>() {
            Some(c) => std::future::ready(Ok(AuthUser {
                sub: c.sub.clone(),
                username: c.username.clone(),
                role: c.role.clone(),
                device_fingerprint: c.device_fingerprint.clone(),
                ip_address: c.ip_address.clone(),
            })),
            None => std::future::ready(Err(AppError::Unauthorized("未授权访问".to_string()))),
        }
    }
}

pub struct AdminUser {
    pub sub: String,
    pub username: String,
}

impl FromRequest for AdminUser {
    type Error = AppError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let extensions = req.extensions();
        match extensions.get::<JwtClaims>() {
            Some(c) if c.role == "admin" => std::future::ready(Ok(AdminUser {
                sub: c.sub.clone(),
                username: c.username.clone(),
            })),
            Some(_) => std::future::ready(Err(AppError::Forbidden("需要管理员权限".to_string()))),
            None => std::future::ready(Err(AppError::Unauthorized("未授权访问".to_string()))),
        }
    }
}
