use actix_web::{
    HttpMessage, HttpRequest, HttpResponse, Result,
    body::MessageBody,
    cookie::{Cookie, SameSite},
    dev::{ServiceRequest, ServiceResponse},
    middleware::Next,
    web,
};
use bcrypt::verify;
use chrono::Utc;
use jsonwebtoken::errors::ErrorKind;
use lettre::message::Message;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use serde::Deserialize;
use tracing::{error, info};
use uuid::Uuid;
use validator::Validate;

use crate::auth::utils::{
    JwtUtils, extract_token_from_service_request, get_client_info_from_service_request,
};
use crate::config::Config;
use crate::crypto::{decrypt_password, encrypt_password};
use crate::db::DbPool;
use crate::models::{
    ApiResponse, EmailLoginRequest, SendLoginCodeRequest, SendTwoFactorCodeRequest,
    TwoFactorLoginRequest, User, UserLogin,
};
use crate::system::smtp::get_smtp_config_from_db;
use crate::utils::detect_user_language;
use chrono::DateTime;
use totp_rs::{Algorithm, Secret, TOTP};

// 认证中间件
pub async fn auth_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    // 1. 从请求中提取 Token
    let token = match extract_token_from_service_request(&req) {
        Some(token) => token,
        None => {
            let user_lang = detect_user_language(req.request());
            return Ok(req.into_response(
                HttpResponse::Unauthorized()
                    .json(ApiResponse::<()>::error_i18n("api.auth_failed", &user_lang))
                    .map_into_right_body(),
            ));
        }
    };

    // 2. 验证 Token
    let config = match req.app_data::<web::Data<Config>>() {
        Some(c) => c,
        None => {
            let user_lang = detect_user_language(req.request());
            return Ok(req.into_response(
                HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error_i18n(
                        "api.server_error",
                        &user_lang,
                    ))
                    .map_into_right_body(),
            ));
        }
    };
    let jwt_utils = JwtUtils::new(config);

    // 使用 validate_token 而不是 decode_token
    let claims = match jwt_utils.validate_token(&token) {
        Ok(claims) => claims,
        Err(err) => {
            let user_lang = detect_user_language(req.request());
            let error_msg = match err.kind() {
                ErrorKind::ExpiredSignature => "api.token_expired",
                _ => "api.invalid_token",
            };
            return Ok(req.into_response(
                HttpResponse::Unauthorized()
                    .json(ApiResponse::<()>::error_i18n(error_msg, &user_lang))
                    .map_into_right_body(),
            ));
        }
    };

    // 3. 验证设备指纹（始终启用）
    let (ip_address, user_agent) = get_client_info_from_service_request(&req);
    let current_fingerprint = JwtUtils::generate_device_fingerprint(&user_agent, &ip_address);

    if let Some(token_fingerprint) = &claims.device_fingerprint
        && token_fingerprint != &current_fingerprint
    {
        let user_lang = detect_user_language(req.request());
        let resp = ApiResponse::<()>::error_i18n("api.device_validation_failed", &user_lang);
        return Ok(req.into_response(
            HttpResponse::Unauthorized()
                .json(resp)
                .map_into_right_body(),
        ));
    }

    // 4. 将用户信息注入到请求扩展中，以便后续处理程序使用
    // claims 是 JwtClaims 结构体
    req.extensions_mut().insert(claims);

    let res = next.call(req).await?;
    Ok(res.map_into_left_body())
}

// 登录处理
pub async fn login(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    req: web::Json<UserLogin>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let user_lang = detect_user_language(&http_req);

    if let Err(e) = req.validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {:?}", e)))
        );
    }

    let user_row = match sqlx::query_as::<
        sqlx::Postgres,
        (Uuid, String, String, String, String, bool, bool),
    >(
        "SELECT id, username, password_hash, email, role, status, two_factor_enabled FROM users WHERE username = $1 OR email = $1",
    )
    .bind(&req.username)
    .fetch_one(&pool.pool)
    .await
    {
        Ok(row) => row,
        Err(_) => {
            let _ = log_login(&pool.pool, &req.username, &http_req, false, Some("用户未找到")).await;
            return Ok(HttpResponse::Unauthorized().json(ApiResponse::<()>::error_i18n("api.login_failed", &user_lang)));
        }
    };

    let (id, username, password_hash, email, role, status, two_factor_enabled) = user_row;

    if !status {
        let _ = log_login(
            &pool.pool,
            &username,
            &http_req,
            false,
            Some("Account disabled"),
        )
        .await;
        return Ok(
            HttpResponse::Unauthorized().json(ApiResponse::<()>::error_i18n(
                "api.account_disabled",
                &user_lang,
            )),
        );
    }

    if !verify(&req.password, &password_hash).unwrap_or(false) {
        let _ = log_login(
            &pool.pool,
            &username,
            &http_req,
            false,
            Some("Invalid password"),
        )
        .await;
        return Ok(
            HttpResponse::Unauthorized().json(ApiResponse::<()>::error_i18n(
                "api.login_failed",
                &user_lang,
            )),
        );
    }

    if two_factor_enabled {
        return Ok(HttpResponse::Ok().json(ApiResponse::success_i18n(
            serde_json::json!({ "requires_two_factor": true, "username": username }),
            "api.success",
            &user_lang,
        )));
    }

    let jwt_utils = JwtUtils::new(&config);
    let (ip_address, user_agent) = crate::auth::utils::get_client_info(&http_req);
    let device_fingerprint = JwtUtils::generate_device_fingerprint(&user_agent, &ip_address);

    let remember_me = req.remember_me.unwrap_or(false);
    let access_token = match jwt_utils.generate_access_token(
        &id,
        &username,
        &role,
        Some(&device_fingerprint),
        Some(&ip_address),
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("生成访问令牌失败: {}", e);
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error("令牌生成失败"))
            );
        }
    };
    let refresh_token = match jwt_utils.generate_refresh_token(
        &id,
        &username,
        &role,
        Some(&device_fingerprint),
        Some(&ip_address),
        remember_me,
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("生成刷新令牌失败: {}", e);
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error("令牌生成失败"))
            );
        }
    };
    let access_token_expiry = jwt_utils.get_access_token_expiry();
    let refresh_token_expiry = jwt_utils.get_actual_refresh_token_expiry(remember_me);

    let user = User {
        id,
        username: username.clone(),
        email,
        role: role.clone(),
        status,
        two_factor_enabled,
        two_factor_verified: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let _ = log_login(&pool.pool, &username, &http_req, true, None).await;

    let secure = is_secure_request(&http_req);
    let access_cookie = create_auth_cookie(
        "access_token",
        &access_token,
        access_token_expiry as i64,
        secure,
    );
    let refresh_cookie = create_auth_cookie(
        "refresh_token",
        &refresh_token,
        refresh_token_expiry as i64,
        secure,
    );

    Ok(HttpResponse::Ok()
        .cookie(access_cookie)
        .cookie(refresh_cookie)
        .json(ApiResponse::success_i18n(
            serde_json::json!({ "user": user, "expires_in": access_token_expiry }),
            "api.success",
            &user_lang,
        )))
}

// 邮箱验证码登录
pub async fn login_with_email_code(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    req: web::Json<EmailLoginRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let user_lang = detect_user_language(&http_req);
    let email = req.email.trim();

    if let Err(e) = req.validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {:?}", e)))
        );
    }

    let user_row = match sqlx::query_as::<
        sqlx::Postgres,
        (Uuid, String, String, String, bool, bool, Option<String>, Option<DateTime<Utc>>),
    >(
        "SELECT id, username, email, role, status, two_factor_enabled, two_factor_email_code, two_factor_email_code_expiry FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_one(&pool.pool)
    .await
    {
        Ok(row) => row,
        Err(_) => {
            let _ = log_login(&pool.pool, email, &http_req, false, Some("用户未找到")).await;
            return Ok(HttpResponse::Unauthorized().json(ApiResponse::<()>::error_i18n("api.invalid_email_or_code", &user_lang)));
        }
    };

    let (id, username, email, role, status, two_factor_enabled, code, expiry) = user_row;

    if !status {
        let _ = log_login(
            &pool.pool,
            &username,
            &http_req,
            false,
            Some("Account disabled"),
        )
        .await;
        return Ok(
            HttpResponse::Unauthorized().json(ApiResponse::<()>::error_i18n(
                "api.account_disabled",
                &user_lang,
            )),
        );
    }

    let mut verified = false;
    if let (Some(c), Some(e)) = (code, expiry) {
        let trimmed_input_code = req.code.trim();
        let trimmed_db_code = c.trim();
        if trimmed_db_code == trimmed_input_code && e > Utc::now() {
            verified = true;
            if let Err(e) = sqlx::query("UPDATE users SET two_factor_email_code = NULL, two_factor_email_code_expiry = NULL WHERE id = $1")
                .bind(id).execute(&pool.pool).await
            {
                tracing::warn!("清除2FA邮箱验证码失败: {}", e);
            }
        }
    }

    if !verified {
        let _ = log_login(
            &pool.pool,
            &username,
            &http_req,
            false,
            Some("Invalid email code"),
        )
        .await;
        return Ok(
            HttpResponse::Unauthorized().json(ApiResponse::<()>::error("验证码无效或已过期"))
        );
    }

    if two_factor_enabled {
        return Ok(HttpResponse::Ok().json(ApiResponse::success_i18n(
            serde_json::json!({ "requires_two_factor": true, "username": username }),
            "api.success",
            &user_lang,
        )));
    }

    let jwt_utils = JwtUtils::new(&config);
    let (ip_address, user_agent) = crate::auth::utils::get_client_info(&http_req);
    let device_fingerprint = JwtUtils::generate_device_fingerprint(&user_agent, &ip_address);

    let remember_me = req.remember_me.unwrap_or(false);
    let access_token = match jwt_utils.generate_access_token(
        &id,
        &username,
        &role,
        Some(&device_fingerprint),
        Some(&ip_address),
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("生成访问令牌失败: {}", e);
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error("令牌生成失败"))
            );
        }
    };
    let refresh_token = match jwt_utils.generate_refresh_token(
        &id,
        &username,
        &role,
        Some(&device_fingerprint),
        Some(&ip_address),
        remember_me,
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("生成刷新令牌失败: {}", e);
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error("令牌生成失败"))
            );
        }
    };
    let access_token_expiry = jwt_utils.get_access_token_expiry();
    let refresh_token_expiry = jwt_utils.get_actual_refresh_token_expiry(remember_me);

    let user = User {
        id,
        username: username.clone(),
        email,
        role: role.clone(),
        status,
        two_factor_enabled,
        two_factor_verified: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let _ = log_login(&pool.pool, &username, &http_req, true, None).await;

    let secure = is_secure_request(&http_req);
    let access_cookie = create_auth_cookie(
        "access_token",
        &access_token,
        access_token_expiry as i64,
        secure,
    );
    let refresh_cookie = create_auth_cookie(
        "refresh_token",
        &refresh_token,
        refresh_token_expiry as i64,
        secure,
    );

    Ok(HttpResponse::Ok()
        .cookie(access_cookie)
        .cookie(refresh_cookie)
        .json(ApiResponse::success_i18n(
            serde_json::json!({ "user": user, "expires_in": access_token_expiry }),
            "api.success",
            &user_lang,
        )))
}

// 发送登录验证码
pub async fn send_login_code(
    pool: web::Data<DbPool>,
    req: web::Json<SendLoginCodeRequest>,
) -> Result<HttpResponse> {
    use rand::RngExt;
    let email = req.email.trim();

    if let Err(e) = req.validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {:?}", e)))
        );
    }

    let user_row = match sqlx::query_as::<sqlx::Postgres, (Uuid, String, bool)>(
        "SELECT id, username, status FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(&pool.pool)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            return Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "验证码已发送")));
        }
        Err(_) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error("数据库错误"))
            );
        }
    };

    let (id, _username, status) = user_row;

    if !status {
        return Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "验证码已发送")));
    }

    let mut rng = rand::rng();
    let code: String = (0..6)
        .map(|_| rng.random_range(0..10).to_string())
        .collect();
    let expiry = Utc::now() + chrono::Duration::minutes(5);

    if let Err(e) = sqlx::query("UPDATE users SET two_factor_email_code = $1, two_factor_email_code_expiry = $2 WHERE id = $3")
        .bind(&code).bind(expiry).bind(id).execute(&pool.pool).await
    {
        tracing::error!("保存2FA邮箱验证码失败: {}", e);
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("保存验证码失败")));
    }

    let smtp_config = get_smtp_config_from_db(&pool.pool).await;
    if let Some(smtp_config) = smtp_config {
        info!(
            "Sending login code email to {} using host: {}",
            email, smtp_config.host
        );
        let email_body = format!("您的登录验证码是：{}", code);
        let email_msg = match Message::builder()
            .from(match smtp_config.from.parse() {
                Ok(addr) => addr,
                Err(e) => {
                    error!("解析发件人地址失败: {}", e);
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error("邮件配置错误")));
                }
            })
            .to(match email.parse() {
                Ok(addr) => addr,
                Err(e) => {
                    error!("解析收件人地址失败: {}", e);
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error("邮箱地址无效")));
                }
            })
            .subject("登录验证码")
            .body(email_body)
        {
            Ok(msg) => msg,
            Err(e) => {
                error!("构建邮件失败: {}", e);
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error("邮件构建失败")));
            }
        };

        let mut smtp_builder = if smtp_config.secure || smtp_config.host == "smtp.qq.com" {
            match AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_config.host) {
                Ok(b) => b,
                Err(e) => {
                    error!("创建SMTP连接失败: {}", e);
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error("邮件服务连接失败")));
                }
            }
        } else {
            match AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp_config.host) {
                Ok(b) => b,
                Err(e) => {
                    error!("创建SMTP连接失败: {}", e);
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error("邮件服务连接失败")));
                }
            }
        };

        if smtp_config.port != 0 {
            smtp_builder = smtp_builder.port(smtp_config.port);
        }
        let smtp = smtp_builder
            .credentials(Credentials::new(smtp_config.username, smtp_config.password))
            .build();

        match smtp.send(email_msg).await {
            Ok(_) => {
                info!("Login code email sent successfully");
            }
            Err(e) => {
                error!("Failed to send login code email: {:?}", e);
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error("发送验证码失败")));
            }
        }
    } else {
        error!("SMTP configuration not found in database");
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("SMTP未配置")));
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "验证码已发送")))
}

// 2FA 登录
pub async fn login_with_two_factor(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    req: web::Json<TwoFactorLoginRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let user_lang = detect_user_language(&http_req);

    // 验证用户
    let user_row = match sqlx::query_as::<
        sqlx::Postgres,
        (Uuid, String, String, String, String, bool, bool, Option<String>),
    >(
        "SELECT id, username, password_hash, email, role, status, two_factor_enabled, two_factor_secret FROM users WHERE username = $1",
    )
    .bind(&req.username)
    .fetch_one(&pool.pool)
    .await
    {
        Ok(row) => row,
        Err(_) => return Ok(HttpResponse::Unauthorized().json(ApiResponse::<()>::error_i18n("api.login_failed", &user_lang))),
    };

    let (id, username, password_hash, email, role, status, two_factor_enabled, secret) = user_row;

    // 如果密码不为空，验证密码
    if !req.password.is_empty() && !verify(&req.password, &password_hash).unwrap_or(false) {
        return Ok(
            HttpResponse::Unauthorized().json(ApiResponse::<()>::error_i18n(
                "api.login_failed",
                &user_lang,
            )),
        );
    }

    if !status {
        return Ok(
            HttpResponse::Unauthorized().json(ApiResponse::<()>::error_i18n(
                "api.account_disabled",
                &user_lang,
            )),
        );
    }

    if !two_factor_enabled {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("2FA not enabled")));
    }

    let mut verified = false;
    // 验证 TOTP (RFC 4226: 密钥至少128 bits)
    if let Some(encrypted_secret) = secret {
        // 解密密钥
        let secret = decrypt_password(&encrypted_secret);
        // 将 Base32 编码的密钥解码为字节
        let secret_bytes = match Secret::Encoded(secret.clone()).to_bytes() {
            Ok(bytes) => bytes,
            Err(_) => {
                // Base32 解码失败，密钥格式错误
                let _ = log_login(
                    &pool.pool,
                    &username,
                    &http_req,
                    false,
                    Some("Invalid 2FA secret format"),
                )
                .await;
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error("2FA密钥格式错误")));
            }
        };
        // 使用 TOTP::new 验证密钥长度符合 RFC 4226 规范
        match TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            secret_bytes,
            None,
            "".to_string(),
        ) {
            Ok(totp) => {
                if totp.check_current(&req.two_factor_code).unwrap_or(false) {
                    verified = true;
                }
            }
            Err(_) => {
                // 密钥长度不符合规范（需要至少16字节）
                let _ = log_login(
                    &pool.pool,
                    &username,
                    &http_req,
                    false,
                    Some("2FA secret too short"),
                )
                .await;
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error("2FA密钥长度不足，请重新设置")));
            }
        }
    }

    if !verified {
        let _ = log_login(
            &pool.pool,
            &username,
            &http_req,
            false,
            Some("Invalid 2FA code"),
        )
        .await;
        return Ok(HttpResponse::Unauthorized().json(ApiResponse::<()>::error("验证码无效")));
    }

    let jwt_utils = JwtUtils::new(&config);
    let (ip_address, user_agent) = crate::auth::utils::get_client_info(&http_req);
    let device_fingerprint = JwtUtils::generate_device_fingerprint(&user_agent, &ip_address);
    let remember_me = req.remember_me.unwrap_or(false);
    let access_token = match jwt_utils.generate_access_token(
        &id,
        &username,
        &role,
        Some(&device_fingerprint),
        Some(&ip_address),
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("生成访问令牌失败: {}", e);
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error("令牌生成失败"))
            );
        }
    };
    let refresh_token = match jwt_utils.generate_refresh_token(
        &id,
        &username,
        &role,
        Some(&device_fingerprint),
        Some(&ip_address),
        remember_me,
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("生成刷新令牌失败: {}", e);
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error("令牌生成失败"))
            );
        }
    };
    let access_token_expiry = jwt_utils.get_access_token_expiry();
    let refresh_token_expiry = jwt_utils.get_actual_refresh_token_expiry(remember_me);

    let user = User {
        id,
        username,
        email,
        role,
        status,
        two_factor_enabled,
        two_factor_verified: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let _ = log_login(&pool.pool, &user.username, &http_req, true, None).await;

    let secure = is_secure_request(&http_req);
    let access_cookie = create_auth_cookie(
        "access_token",
        &access_token,
        access_token_expiry as i64,
        secure,
    );
    let refresh_cookie = create_auth_cookie(
        "refresh_token",
        &refresh_token,
        refresh_token_expiry as i64,
        secure,
    );

    Ok(HttpResponse::Ok()
        .cookie(access_cookie)
        .cookie(refresh_cookie)
        .json(ApiResponse::success_i18n(
            serde_json::json!({ "user": user, "expires_in": access_token_expiry }),
            "api.success",
            &user_lang,
        )))
}

// 发送 2FA 码
pub async fn send_two_factor_code(
    pool: web::Data<DbPool>,
    req: web::Json<SendTwoFactorCodeRequest>,
) -> Result<HttpResponse> {
    // 复用 send_login_code 逻辑，或者简单重写
    let user = match sqlx::query_as::<sqlx::Postgres, (Uuid, String, String)>(
        "SELECT id, username, email FROM users WHERE username = $1",
    )
    .bind(&req.username)
    .fetch_optional(&pool.pool)
    .await
    {
        Ok(Some(u)) => u,
        _ => return Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "发送成功"))),
    };

    // 生成并发送代码... (简化)
    use rand::RngExt;
    let mut rng = rand::rng();
    let code: String = (0..6)
        .map(|_| rng.random_range(0..10).to_string())
        .collect();
    let expiry = Utc::now() + chrono::Duration::minutes(5);
    if let Err(e) = sqlx::query("UPDATE users SET two_factor_email_code = $1, two_factor_email_code_expiry = $2 WHERE id = $3").bind(&code).bind(expiry).bind(user.0).execute(&pool.pool).await {
        tracing::error!("保存2FA邮箱验证码失败: {}", e);
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("保存验证码失败")));
    }

    let smtp_config = get_smtp_config_from_db(&pool.pool).await;
    if let Some(smtp_config) = smtp_config {
        let email_body = format!("您的两步验证码是：{}", code);
        let email_msg = match Message::builder()
            .from(match smtp_config.from.parse() {
                Ok(addr) => addr,
                Err(e) => {
                    error!("解析发件人地址失败: {}", e);
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error("邮件配置错误")));
                }
            })
            .to(match user.2.parse() {
                Ok(addr) => addr,
                Err(e) => {
                    error!("解析收件人地址失败: {}", e);
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error("邮箱地址无效")));
                }
            })
            .subject("两步验证码")
            .body(email_body)
        {
            Ok(msg) => msg,
            Err(e) => {
                error!("构建邮件失败: {}", e);
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error("邮件构建失败")));
            }
        };

        let smtp_builder = if smtp_config.secure || smtp_config.host == "smtp.qq.com" {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_config.host)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp_config.host)
        };

        match smtp_builder {
            Ok(builder) => {
                let smtp = builder
                    .port(smtp_config.port)
                    .credentials(Credentials::new(smtp_config.username, smtp_config.password))
                    .build();

                if let Err(e) = smtp.send(email_msg).await {
                    error!("发送2FA验证码邮件失败: {:?}", e);
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error("发送验证码失败")));
                }
            }
            Err(e) => {
                error!("创建SMTP连接失败: {}", e);
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error("邮件服务连接失败")));
            }
        }
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "验证码已发送")))
}

// 登出
pub async fn logout(http_req: HttpRequest) -> Result<HttpResponse> {
    let user_lang = detect_user_language(&http_req);
    let secure = is_secure_request(&http_req);

    let access_cookie = create_clear_cookie("access_token", secure);
    let refresh_cookie = create_clear_cookie("refresh_token", secure);

    Ok(HttpResponse::Ok()
        .cookie(access_cookie)
        .cookie(refresh_cookie)
        .json(ApiResponse::<()>::success_i18n(
            (),
            "api.success",
            &user_lang,
        )))
}

// 刷新 Token
pub async fn refresh_token(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let user_lang = detect_user_language(&http_req);

    // 优先从 Cookie 中获取 refresh_token
    let token = if let Some(cookie) = http_req.cookie("refresh_token") {
        cookie.value().to_string()
    } else {
        // 回退到 Authorization 头
        match crate::auth::utils::extract_token_from_request(&http_req) {
            Some(t) => t,
            None => {
                return Ok(HttpResponse::Unauthorized()
                    .json(ApiResponse::<()>::error_i18n("api.auth_failed", &user_lang)));
            }
        }
    };

    let jwt_utils = JwtUtils::new(&config);

    // 1. 完整验证 token（包括过期检查）
    let claims = match jwt_utils.validate_token(&token) {
        Ok(c) => c,
        Err(err) => {
            let error_msg = match err.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => "api.token_expired",
                _ => "api.invalid_token",
            };
            return Ok(HttpResponse::Unauthorized()
                .json(ApiResponse::<()>::error_i18n(error_msg, &user_lang)));
        }
    };

    // 2. 检查 token 是否已被撤销
    if crate::utils::is_token_revoked(&pool.pool, &token)
        .await
        .unwrap_or(false)
    {
        return Ok(
            HttpResponse::Unauthorized().json(ApiResponse::<()>::error_i18n(
                "api.token_revoked",
                &user_lang,
            )),
        );
    }

    // 3. 验证设备指纹
    let (ip_address, user_agent) = crate::auth::utils::get_client_info(&http_req);
    let current_fingerprint = JwtUtils::generate_device_fingerprint(&user_agent, &ip_address);

    if let Some(ref token_fingerprint) = claims.device_fingerprint
        && token_fingerprint != &current_fingerprint
    {
        return Ok(
            HttpResponse::Unauthorized().json(ApiResponse::<()>::error_i18n(
                "api.device_validation_failed",
                &user_lang,
            )),
        );
    }

    // 4. 将旧的 refresh_token 加入撤销列表
    let user_id = match Uuid::parse_str(&claims.sub) {
        Ok(id) => id,
        Err(e) => {
            error!("解析用户ID失败: {}", e);
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error("无效的用户标识")));
        }
    };
    let token_expiry =
        chrono::DateTime::from_timestamp(claims.exp as i64, 0).unwrap_or_else(Utc::now);
    let _ = crate::utils::revoke_token(&pool.pool, &token, &user_id, token_expiry).await;

    // 5. 判断是否保持登录（如果 refresh_token 有效期大于 24 小时，说明用户选择了保持登录）
    let token_duration = claims.exp.saturating_sub(claims.iat);
    let remember_me = token_duration > 86400; // 24小时 = 86400秒

    // 6. 生成新的 token
    let access_token = match jwt_utils.generate_access_token(
        &user_id,
        &claims.username,
        &claims.role,
        Some(&current_fingerprint),
        Some(&ip_address),
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("生成访问令牌失败: {}", e);
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error("令牌生成失败"))
            );
        }
    };
    let new_refresh_token = match jwt_utils.generate_refresh_token(
        &user_id,
        &claims.username,
        &claims.role,
        Some(&current_fingerprint),
        Some(&ip_address),
        remember_me,
    ) {
        Ok(t) => t,
        Err(e) => {
            error!("生成刷新令牌失败: {}", e);
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error("令牌生成失败"))
            );
        }
    };

    let access_token_expiry = jwt_utils.get_access_token_expiry();
    let refresh_token_expiry = jwt_utils.get_actual_refresh_token_expiry(remember_me);

    let secure = is_secure_request(&http_req);
    let access_cookie = create_auth_cookie(
        "access_token",
        &access_token,
        access_token_expiry as i64,
        secure,
    );
    let refresh_cookie = create_auth_cookie(
        "refresh_token",
        &new_refresh_token,
        refresh_token_expiry as i64,
        secure,
    );

    Ok(HttpResponse::Ok()
        .cookie(access_cookie)
        .cookie(refresh_cookie)
        .json(ApiResponse::success_i18n(
            serde_json::json!({ "expires_in": access_token_expiry, "remember_me": remember_me }),
            "api.success",
            &user_lang,
        )))
}

// 获取当前用户
pub async fn get_current_user(http_req: HttpRequest) -> Result<HttpResponse> {
    let extensions = http_req.extensions();
    let claims = match extensions.get::<crate::auth::utils::JwtClaims>() {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(ApiResponse::<()>::error("未授权访问")));
        }
    };
    // 使用 serde_json::Value 作为泛型
    Ok(HttpResponse::Ok().json(ApiResponse::<serde_json::Value>::success(serde_json::json!({ "id": claims.sub, "username": claims.username, "role": claims.role }), "Success")))
}

// 忘记密码
pub async fn forgot_password(
    pool: web::Data<DbPool>,
    req: web::Json<serde_json::Value>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let user_lang = detect_user_language(&http_req);

    let email = match req.get("email").and_then(|v| v.as_str()) {
        Some(e) => e,
        None => {
            return Ok(
                HttpResponse::BadRequest().json(ApiResponse::<()>::error_i18n(
                    "api.invalid_request",
                    &user_lang,
                )),
            );
        }
    };

    let user_result = sqlx::query_as::<_, (Uuid, String, bool)>(
        "SELECT id, username, two_factor_enabled FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(&pool.pool)
    .await;

    match user_result {
        Ok(Some((_user_id, _username, _two_factor_enabled))) => {
            let smtp_config = get_smtp_config_from_db(&pool.pool).await;
            if let Some(smtp_config) = smtp_config {
                let reset_token = Uuid::new_v4().to_string();
                let expiry = Utc::now() + chrono::Duration::hours(1);

                if let Err(e) = sqlx::query(
                    "UPDATE users SET reset_token = $1, reset_token_expiry = $2 WHERE email = $3",
                )
                .bind(&reset_token)
                .bind(expiry)
                .bind(email)
                .execute(&pool.pool)
                .await
                {
                    error!("保存重置令牌失败: {}", e);
                } else {
                    let reset_link =
                        format!("{}/reset-password?token={}", smtp_config.host, reset_token);
                    let email_body = format!("请点击以下链接重置密码：{}", reset_link);
                    let email_msg = Message::builder()
                        .from(match smtp_config.from.parse() {
                            Ok(addr) => addr,
                            Err(e) => {
                                error!("解析发件人地址失败: {}", e);
                                return Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
                                    (),
                                    "如果该邮箱已注册，重置邮件已发送",
                                )));
                            }
                        })
                        .to(match email.parse() {
                            Ok(addr) => addr,
                            Err(e) => {
                                error!("解析收件人地址失败: {}", e);
                                return Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
                                    (),
                                    "如果该邮箱已注册，重置邮件已发送",
                                )));
                            }
                        })
                        .subject("密码重置")
                        .body(email_body);

                    match email_msg {
                        Ok(msg) => {
                            let smtp_builder =
                                if smtp_config.secure || smtp_config.host == "smtp.qq.com" {
                                    AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_config.host)
                                } else {
                                    AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(
                                        &smtp_config.host,
                                    )
                                };

                            match smtp_builder {
                                Ok(builder) => {
                                    let smtp = builder
                                        .port(smtp_config.port)
                                        .credentials(Credentials::new(
                                            smtp_config.username,
                                            smtp_config.password,
                                        ))
                                        .build();

                                    if let Err(e) = smtp.send(msg).await {
                                        error!("发送重置邮件失败: {:?}", e);
                                    }
                                }
                                Err(e) => {
                                    error!("创建SMTP连接失败: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("构建邮件失败: {}", e);
                        }
                    }
                }
            }
            Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
                (),
                "如果该邮箱已注册，重置邮件已发送",
            )))
        }
        Ok(None) => Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
            (),
            "如果该邮箱已注册，重置邮件已发送",
        ))),
        Err(e) => {
            error!("查询用户失败: {}", e);
            Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
                (),
                "如果该邮箱已注册，重置邮件已发送",
            )))
        }
    }
}

pub async fn reset_password(
    pool: web::Data<DbPool>,
    req: web::Json<serde_json::Value>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let user_lang = detect_user_language(&http_req);

    let token = match req.get("token").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => {
            return Ok(
                HttpResponse::BadRequest().json(ApiResponse::<()>::error_i18n(
                    "api.invalid_request",
                    &user_lang,
                )),
            );
        }
    };

    let new_password = match req.get("new_password").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return Ok(
                HttpResponse::BadRequest().json(ApiResponse::<()>::error_i18n(
                    "api.invalid_request",
                    &user_lang,
                )),
            );
        }
    };

    if new_password.len() < 8 {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error("密码长度不能少于8个字符"))
        );
    }

    let user_result = sqlx::query_as::<_, (Uuid,)>(
        "SELECT id FROM users WHERE reset_token = $1 AND reset_token_expiry > NOW()",
    )
    .bind(token)
    .fetch_optional(&pool.pool)
    .await;

    match user_result {
        Ok(Some((user_id,))) => {
            let hashed_password = match bcrypt::hash(new_password, bcrypt::DEFAULT_COST) {
                Ok(h) => h,
                Err(e) => {
                    error!("密码哈希失败: {}", e);
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error("密码重置失败")));
                }
            };

            if let Err(e) = sqlx::query(
                "UPDATE users SET password = $1, reset_token = NULL, reset_token_expiry = NULL WHERE id = $2",
            )
            .bind(&hashed_password)
            .bind(user_id)
            .execute(&pool.pool)
            .await
            {
                error!("更新密码失败: {}", e);
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error("密码重置失败")));
            }

            Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "密码重置成功")))
        }
        Ok(None) => {
            Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("重置链接无效或已过期")))
        }
        Err(e) => {
            error!("查询重置令牌失败: {}", e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("密码重置失败")))
        }
    }
}

// 初始化2FA - 生成符合RFC 4226规范的TOTP密钥
pub async fn init_two_factor(
    pool: web::Data<DbPool>,
    req: web::Json<TwoFactorInitRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let (user_sub, user_role) = {
        let extensions = http_req.extensions();
        match extensions.get::<crate::auth::utils::JwtClaims>() {
            Some(c) => (c.sub.clone(), c.role.clone()),
            None => {
                return Ok(HttpResponse::Unauthorized().json(ApiResponse::<()>::error("未授权")));
            }
        }
    };

    // 确定目标用户ID
    let target_user_id = if let Some(user_id) = req.user_id {
        // 如果指定了user_id，检查权限
        if user_sub != user_id.to_string() && user_role != "admin" {
            return Ok(HttpResponse::Forbidden().json(ApiResponse::<()>::error(
                "只有管理员可以为其他用户初始化2FA",
            )));
        }
        user_id
    } else {
        // 默认使用当前用户
        match Uuid::parse_str(&user_sub) {
            Ok(id) => id,
            Err(_) => {
                return Ok(
                    HttpResponse::BadRequest().json(ApiResponse::<()>::error("无效的用户ID"))
                );
            }
        }
    };

    // 获取目标用户信息
    let target_username: String =
        match sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&pool.pool)
            .await
        {
            Ok(Some(name)) => name,
            Ok(None) => {
                return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("用户不存在")));
            }
            Err(_) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error("数据库错误")));
            }
        };

    // 生成32字节（256 bits）的密钥，比RFC 4226推荐的160 bits更安全
    let secret_bytes: Vec<u8> = {
        use rand::Rng;
        let mut bytes = vec![0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        bytes
    };
    let secret = Secret::Raw(secret_bytes);
    let secret_base32 = secret.to_encoded().to_string();

    // 创建TOTP实例用于生成URL（SHA-1算法保证兼容性）
    let totp = match TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret.to_bytes().unwrap(),
        Some("IPMA".to_string()),
        target_username.clone(),
    ) {
        Ok(t) => t,
        Err(_) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error("生成TOTP失败"))
            );
        }
    };

    // 将密钥加密后存储到数据库（尚未启用）
    let encrypted_secret = match encrypt_password(&secret_base32) {
        Some(encrypted) => encrypted,
        None => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error("2FA密钥加密失败")));
        }
    };
    if let Err(e) = sqlx::query("UPDATE users SET two_factor_secret = $1 WHERE id = $2")
        .bind(&encrypted_secret)
        .bind(target_user_id)
        .execute(&pool.pool)
        .await
    {
        tracing::error!("保存2FA密钥失败: {}", e);
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error("保存2FA密钥失败"))
        );
    }

    // 返回密钥和otpauth URL
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "secret": secret_base32,
            "otpauth_url": totp.get_url(),
            "qr_code_base64": totp.get_qr_base64().unwrap_or_default(),
        }),
        "2FA初始化成功，请使用认证器应用扫描二维码",
    )))
}

// 启用2FA - 验证TOTP码并启用
pub async fn enable_two_factor(
    pool: web::Data<DbPool>,
    req: web::Json<TwoFactorEnableRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let (user_sub, user_role) = {
        let extensions = http_req.extensions();
        match extensions.get::<crate::auth::utils::JwtClaims>() {
            Some(c) => (c.sub.clone(), c.role.clone()),
            None => {
                return Ok(HttpResponse::Unauthorized().json(ApiResponse::<()>::error("未授权")));
            }
        }
    };

    // 确定目标用户ID
    let target_user_id = if let Some(user_id) = req.user_id {
        // 如果指定了user_id，检查权限
        if user_sub != user_id.to_string() && user_role != "admin" {
            return Ok(HttpResponse::Forbidden()
                .json(ApiResponse::<()>::error("只有管理员可以为其他用户启用2FA")));
        }
        user_id
    } else {
        // 默认使用当前用户
        match Uuid::parse_str(&user_sub) {
            Ok(id) => id,
            Err(_) => {
                return Ok(
                    HttpResponse::BadRequest().json(ApiResponse::<()>::error("无效的用户ID"))
                );
            }
        }
    };

    // 获取存储的密钥
    let secret: Option<String> =
        match sqlx::query_scalar("SELECT two_factor_secret FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&pool.pool)
            .await
        {
            Ok(Some(s)) => s,
            Ok(None) => {
                return Ok(
                    HttpResponse::BadRequest().json(ApiResponse::<()>::error("请先初始化2FA"))
                );
            }
            Err(_) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error("数据库错误")));
            }
        };

    let encrypted_secret = match secret {
        Some(s) => s,
        None => {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("请先初始化2FA")));
        }
    };

    // 解密密钥
    let secret = decrypt_password(&encrypted_secret);

    // 获取目标用户名
    let target_username: String =
        match sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&pool.pool)
            .await
        {
            Ok(Some(name)) => name,
            Ok(None) => {
                return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("用户不存在")));
            }
            Err(_) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error("数据库错误")));
            }
        };

    // 验证TOTP码
    let secret_bytes = match Secret::Encoded(secret.clone()).to_bytes() {
        Ok(bytes) => bytes,
        Err(_) => {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("密钥格式错误")));
        }
    };

    let totp = match TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some("IPMA".to_string()),
        target_username,
    ) {
        Ok(t) => t,
        Err(_) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error("TOTP创建失败"))
            );
        }
    };

    if !totp.check_current(&req.code).unwrap_or(false) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("验证码错误")));
    }

    // 启用2FA
    if let Err(e) = sqlx::query(
        "UPDATE users SET two_factor_enabled = true, two_factor_verified = true WHERE id = $1",
    )
    .bind(target_user_id)
    .execute(&pool.pool)
    .await
    {
        tracing::error!("启用2FA失败: {}", e);
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error("启用2FA失败"))
        );
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "2FA已启用")))
}

// 禁用2FA
pub async fn disable_two_factor(
    pool: web::Data<DbPool>,
    req: web::Json<TwoFactorDisableRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let (user_sub, user_role) = {
        let extensions = http_req.extensions();
        match extensions.get::<crate::auth::utils::JwtClaims>() {
            Some(c) => (c.sub.clone(), c.role.clone()),
            None => {
                return Ok(HttpResponse::Unauthorized().json(ApiResponse::<()>::error("未授权")));
            }
        }
    };

    // 确定目标用户ID
    let target_user_id = if let Some(user_id) = req.user_id {
        // 如果指定了user_id，检查权限
        if user_sub != user_id.to_string() && user_role != "admin" {
            return Ok(HttpResponse::Forbidden()
                .json(ApiResponse::<()>::error("只有管理员可以为其他用户禁用2FA")));
        }
        user_id
    } else {
        // 默认使用当前用户
        match Uuid::parse_str(&user_sub) {
            Ok(id) => id,
            Err(_) => {
                return Ok(
                    HttpResponse::BadRequest().json(ApiResponse::<()>::error("无效的用户ID"))
                );
            }
        }
    };

    // 获取存储的密钥
    let (secret, two_factor_enabled): (Option<String>, bool) = match sqlx::query_as(
        "SELECT two_factor_secret, two_factor_enabled FROM users WHERE id = $1",
    )
    .bind(target_user_id)
    .fetch_one(&pool.pool)
    .await
    {
        Ok(row) => row,
        Err(_) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error("数据库错误"))
            );
        }
    };

    if !two_factor_enabled {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("2FA未启用")));
    }

    // 获取目标用户名
    let target_username: String =
        match sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&pool.pool)
            .await
        {
            Ok(Some(name)) => name,
            Ok(None) => {
                return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("用户不存在")));
            }
            Err(_) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error("数据库错误")));
            }
        };

    // 验证TOTP码
    let mut verified = false;

    if let Some(encrypted_secret) = secret {
        // 解密密钥
        let secret = decrypt_password(&encrypted_secret);
        if let Ok(secret_bytes) = Secret::Encoded(secret).to_bytes()
            && let Ok(totp) = TOTP::new(
                Algorithm::SHA1,
                6,
                1,
                30,
                secret_bytes,
                Some("IPMA".to_string()),
                target_username,
            )
            && totp.check_current(&req.code).unwrap_or(false)
        {
            verified = true;
        }
    }

    if !verified {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("验证码错误")));
    }

    // 禁用2FA并清除密钥
    if let Err(e) = sqlx::query(
        "UPDATE users SET two_factor_enabled = false, two_factor_secret = NULL, two_factor_verified = false WHERE id = $1"
    )
    .bind(target_user_id)
    .execute(&pool.pool)
    .await
    {
        tracing::error!("禁用2FA失败: {}", e);
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("禁用2FA失败")));
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "2FA已禁用")))
}

// 2FA请求结构体
#[derive(Debug, Deserialize)]
pub struct TwoFactorInitRequest {
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct TwoFactorEnableRequest {
    pub code: String,
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct TwoFactorDisableRequest {
    pub code: String,
    pub user_id: Option<Uuid>,
}

// 辅助函数：记录登录日志
async fn log_login(
    pool: &sqlx::PgPool,
    username: &str,
    req: &HttpRequest,
    success: bool,
    error_message: Option<&str>,
) -> Result<(), sqlx::Error> {
    let (ip_address, user_agent) = crate::auth::utils::get_client_info(req);

    sqlx::query(
        "INSERT INTO login_logs (id, username, ip_address, user_agent, success, error_message, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(Uuid::new_v4())
    .bind(username)
    .bind(ip_address)
    .bind(user_agent)
    .bind(success)
    .bind(error_message)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    Ok(())
}

// 辅助函数：创建 HttpOnly Cookie
fn create_auth_cookie(name: &str, value: &str, max_age: i64, secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::build(name, value)
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(actix_web::cookie::time::Duration::seconds(max_age))
        .finish()
        .into_owned();

    if secure {
        cookie.set_secure(true);
    }

    cookie
}

// 辅助函数：创建清除 Cookie
fn create_clear_cookie(name: &str, secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::build(name, "")
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(actix_web::cookie::time::Duration::seconds(0))
        .finish()
        .into_owned();

    if secure {
        cookie.set_secure(true);
    }

    cookie
}

// 辅助函数：判断是否使用 HTTPS
fn is_secure_request(req: &HttpRequest) -> bool {
    req.connection_info().scheme() == "https"
}
