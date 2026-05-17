use actix_web::{
    HttpMessage, HttpRequest, HttpResponse,
    body::MessageBody,
    cookie::{Cookie, SameSite},
    dev::{ServiceRequest, ServiceResponse},
    middleware::Next,
    web,
};
use bcrypt::verify;
use chrono::{DateTime, Utc};
use jsonwebtoken::errors::ErrorKind;
use rand::RngExt;
use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

use crate::auth::utils::{
    JwtUtils, extract_token_from_service_request, get_client_info_from_service_request,
    hash_password,
};
use crate::crypto::{decrypt_password, encrypt_password};
use crate::error::AppError;
use crate::models::{
    ApiResponse, EmailLoginRequest, ForgotPasswordRequest, ResetPasswordRequest,
    SendLoginCodeRequest, SendTwoFactorCodeRequest, TwoFactorLoginRequest, User, UserLogin,
};
use crate::utils::detect_user_language;
use totp_rs::{Algorithm, Secret, TOTP};

pub async fn auth_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
    let Some(token) = extract_token_from_service_request(&req) else {
        let user_lang = detect_user_language(req.request());
        return Ok(req.into_response(
            HttpResponse::Unauthorized()
                .json(ApiResponse::<()>::error_i18n("api.auth_failed", &user_lang))
                .map_into_right_body(),
        ));
    };

    let Some(state) = req.app_data::<web::Data<crate::app_state::AppState>>() else {
        let user_lang = detect_user_language(req.request());
        return Ok(req.into_response(
            HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error_i18n(
                    "api.server_error",
                    &user_lang,
                ))
                .map_into_right_body(),
        ));
    };

    let claims = match state.jwt_utils.validate_token(&token) {
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

    req.extensions_mut().insert(claims);

    let res = next.call(req).await?;
    Ok(res.map_into_left_body())
}

pub async fn login(
    state: web::Data<crate::app_state::AppState>,
    req: web::Json<UserLogin>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let user_lang = detect_user_language(&http_req);
    let conn = state.pool()?.get_conn();

    req.validate()?;

    let user_row = match sqlx::query_as::<
        sqlx::Postgres,
        (Uuid, String, String, String, String, bool, bool),
    >(
        "SELECT id, username, password_hash, email, role, status, two_factor_enabled FROM users WHERE username = $1 OR email = $1",
    )
    .bind(&req.username)
    .fetch_optional(&conn)
    .await?
    {
        Some(row) => row,
        None => {
            if let Err(e) = log_login(&conn, &req.username, &http_req, false, Some("用户未找到")).await {
                tracing::warn!("记录登录日志失败: {}", e);
            }
            return Err(AppError::Unauthorized("登录失败".to_string()));
        }
    };

    let (id, username, password_hash, email, role, status, two_factor_enabled) = user_row;

    if !status {
        if let Err(e) = log_login(
            &conn,
            &username,
            &http_req,
            false,
            Some("Account disabled"),
        )
        .await
        {
            tracing::warn!("记录登录日志失败: {}", e);
        }
        return Err(AppError::Unauthorized("账户已禁用".to_string()));
    }

    let valid = verify(&req.password, &password_hash).map_err(|e| AppError::Internal(e.to_string()))?;
    if !valid {
        if let Err(e) = log_login(
            &conn,
            &username,
            &http_req,
            false,
            Some("Invalid password"),
        )
        .await
        {
            tracing::warn!("记录登录日志失败: {}", e);
        }
        return Err(AppError::Unauthorized("登录失败".to_string()));
    }

    if two_factor_enabled {
        return Ok(HttpResponse::Ok().json(ApiResponse::success_i18n(
            serde_json::json!({ "requires_two_factor": true, "username": username }),
            "api.success",
            &user_lang,
        )));
    }

    let jwt_utils = &state.jwt_utils;
    let (ip_address, user_agent) = crate::auth::utils::get_client_info(&http_req);
    let device_fingerprint = JwtUtils::generate_device_fingerprint(&user_agent, &ip_address);

    let remember_me = req.remember_me.unwrap_or(false);
    let login_tokens = generate_login_tokens(
        &jwt_utils, &id, &username, &role, &device_fingerprint, &ip_address, remember_me,
    )?;

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

    if let Err(e) = log_login(&conn, &username, &http_req, true, None).await {
        tracing::warn!("记录登录日志失败: {}", e);
    }
    tracing::info!("用户 {} 登录成功", username);

    let secure = is_secure_request(&http_req);
    Ok(build_login_response(user, login_tokens, &user_lang, secure))
}

pub async fn login_with_email_code(
    state: web::Data<crate::app_state::AppState>,
    req: web::Json<EmailLoginRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let user_lang = detect_user_language(&http_req);
    let conn = state.pool()?.get_conn();

    req.validate()?;
    let email = req.email.trim();

    let user_row = match sqlx::query_as::<
        sqlx::Postgres,
        (Uuid, String, String, String, bool, bool, Option<String>, Option<DateTime<Utc>>),
    >(
        "SELECT id, username, email, role, status, two_factor_enabled, two_factor_email_code, two_factor_email_code_expiry FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(&conn)
    .await?
    {
        Some(row) => row,
        None => {
            if let Err(e) = log_login(&conn, email, &http_req, false, Some("用户未找到")).await {
                tracing::warn!("记录登录日志失败: {}", e);
            }
            return Err(AppError::Unauthorized("邮箱或验证码无效".to_string()));
        }
    };

    let (id, username, email, role, status, two_factor_enabled, code, expiry) = user_row;

    if !status {
        if let Err(e) = log_login(
            &conn,
            &username,
            &http_req,
            false,
            Some("Account disabled"),
        )
        .await
        {
            tracing::warn!("记录登录日志失败: {}", e);
        }
        return Err(AppError::Unauthorized("账户已禁用".to_string()));
    }

    let mut verified = false;
    if let (Some(c), Some(e)) = (code, expiry) {
        let trimmed_input_code = req.code.trim();
        let trimmed_db_code = c.trim();
        if constant_time_eq(trimmed_db_code, trimmed_input_code) && e > Utc::now() {
            verified = true;
            if let Err(e) = sqlx::query("UPDATE users SET two_factor_email_code = NULL, two_factor_email_code_expiry = NULL WHERE id = $1")
                .bind(id).execute(&conn).await
            {
                tracing::warn!("清除2FA邮箱验证码失败: {}", e);
            }
        }
    }

    if !verified {
        if let Err(e) = log_login(
            &conn,
            &username,
            &http_req,
            false,
            Some("Invalid email code"),
        )
        .await
        {
            tracing::warn!("记录登录日志失败: {}", e);
        }
        return Err(AppError::Unauthorized("验证码无效或已过期".to_string()));
    }

    if two_factor_enabled {
        return Ok(HttpResponse::Ok().json(ApiResponse::success_i18n(
            serde_json::json!({ "requires_two_factor": true, "username": username }),
            "api.success",
            &user_lang,
        )));
    }

    let jwt_utils = &state.jwt_utils;
    let (ip_address, user_agent) = crate::auth::utils::get_client_info(&http_req);
    let device_fingerprint = JwtUtils::generate_device_fingerprint(&user_agent, &ip_address);

    let remember_me = req.remember_me.unwrap_or(false);
    let login_tokens = generate_login_tokens(
        &jwt_utils, &id, &username, &role, &device_fingerprint, &ip_address, remember_me,
    )?;

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

    if let Err(e) = log_login(&conn, &username, &http_req, true, None).await {
        tracing::warn!("记录登录日志失败: {}", e);
    }

    let secure = is_secure_request(&http_req);
    Ok(build_login_response(user, login_tokens, &user_lang, secure))
}

pub async fn send_login_code(
    state: web::Data<crate::app_state::AppState>,
    req: web::Json<SendLoginCodeRequest>,
) -> Result<HttpResponse, AppError> {
    let conn = state.pool()?.get_conn();
    let email = req.email.trim();

    req.validate()?;

    let user_row = match sqlx::query_as::<sqlx::Postgres, (Uuid, String, bool)>(
        "SELECT id, username, status FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(&conn)
    .await?
    {
        Some(row) => row,
        None => {
            return Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "验证码已发送")));
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

    sqlx::query("UPDATE users SET two_factor_email_code = $1, two_factor_email_code_expiry = $2 WHERE id = $3")
        .bind(&code).bind(expiry).bind(id).execute(&conn).await?;

    let email_body = format!("您的登录验证码是：{code}");
    crate::system::smtp::send_email_async(&conn, email, "登录验证码", &email_body).await?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "验证码已发送")))
}

pub async fn login_with_two_factor(
    state: web::Data<crate::app_state::AppState>,
    req: web::Json<TwoFactorLoginRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let user_lang = detect_user_language(&http_req);
    let conn = state.pool()?.get_conn();

    let user_row = match sqlx::query_as::<
        sqlx::Postgres,
        (Uuid, String, String, String, String, bool, bool, Option<String>),
    >(
        "SELECT id, username, password_hash, email, role, status, two_factor_enabled, two_factor_secret FROM users WHERE username = $1",
    )
    .bind(&req.username)
    .fetch_optional(&conn)
    .await?
    {
        Some(row) => row,
        None => {
            return Err(AppError::Unauthorized("登录失败".to_string()));
        }
    };

    let (id, username, password_hash, email, role, status, two_factor_enabled, secret) = user_row;

    let password = req.password.as_deref().ok_or_else(|| {
        AppError::Validation("2FA登录必须提供密码".to_string())
    })?;
    let valid = verify(password, &password_hash).map_err(|e| AppError::Internal(e.to_string()))?;
    if !valid {
        return Err(AppError::Unauthorized("登录失败".to_string()));
    }

    if !status {
        return Err(AppError::Unauthorized("账户已禁用".to_string()));
    }

    if !two_factor_enabled {
        return Err(AppError::Validation("2FA not enabled".to_string()));
    }

    let mut verified = false;
    if let Some(encrypted_secret) = secret {
        let secret = decrypt_password(&encrypted_secret).map_err(|e| AppError::Internal(format!("2FA密钥解密失败: {e}")))?;
        let secret_bytes = match Secret::Encoded(secret.clone()).to_bytes() {
            Ok(bytes) => bytes,
            Err(e) => {
                if let Err(e) = log_login(&conn, &username, &http_req, false, Some("Invalid 2FA secret format")).await {
                    tracing::warn!("记录登录日志失败: {}", e);
                }
                return Err(AppError::Internal(format!("2FA密钥格式错误: {e}")));
            }
        };
        match TOTP::new(Algorithm::SHA1, 6, 1, 30, secret_bytes, None, String::new()) {
            Ok(totp) => {
                let valid = totp.check_current(&req.two_factor_code).map_err(|e| AppError::Internal(e.to_string()))?;
                if valid {
                    verified = true;
                }
            }
            Err(e) => {
                if let Err(e) = log_login(&conn, &username, &http_req, false, Some("2FA secret too short")).await {
                    tracing::warn!("记录登录日志失败: {}", e);
                }
                return Err(AppError::Internal(format!("2FA密钥长度不足，请重新设置: {e}")));
            }
        }
    }

    if !verified {
        if let Err(e) = log_login(
            &conn,
            &username,
            &http_req,
            false,
            Some("Invalid 2FA code"),
        )
        .await
        {
            tracing::warn!("记录登录日志失败: {}", e);
        }
        return Err(AppError::Unauthorized("验证码无效".to_string()));
    }

    let jwt_utils = &state.jwt_utils;
    let (ip_address, user_agent) = crate::auth::utils::get_client_info(&http_req);
    let device_fingerprint = JwtUtils::generate_device_fingerprint(&user_agent, &ip_address);
    let remember_me = req.remember_me.unwrap_or(false);
    let login_tokens = generate_login_tokens(
        &jwt_utils, &id, &username, &role, &device_fingerprint, &ip_address, remember_me,
    )?;

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

    if let Err(e) = log_login(&conn, &user.username, &http_req, true, None).await {
        tracing::warn!("记录登录日志失败: {}", e);
    }
    tracing::info!("用户 {} 2FA验证登录成功", user.username);

    let secure = is_secure_request(&http_req);
    Ok(build_login_response(user, login_tokens, &user_lang, secure))
}

pub async fn send_two_factor_code(
    state: web::Data<crate::app_state::AppState>,
    req: web::Json<SendTwoFactorCodeRequest>,
) -> Result<HttpResponse, AppError> {
    let conn = state.pool()?.get_conn();

    let Some(user) = sqlx::query_as::<sqlx::Postgres, (Uuid, String, String)>(
        "SELECT id, username, email FROM users WHERE username = $1",
    )
    .bind(&req.username)
    .fetch_optional(&conn)
    .await?
    else {
        return Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "发送成功")));
    };

    let mut rng = rand::rng();
    let code: String = (0..6)
        .map(|_| rng.random_range(0..10).to_string())
        .collect();
    let expiry = Utc::now() + chrono::Duration::minutes(5);
    sqlx::query("UPDATE users SET two_factor_email_code = $1, two_factor_email_code_expiry = $2 WHERE id = $3")
        .bind(&code).bind(expiry).bind(user.0).execute(&conn).await?;

    let email_body = format!("您的两步验证码是：{code}");
    crate::system::smtp::send_email_async(&conn, &user.2, "两步验证码", &email_body).await?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "验证码已发送")))
}

pub async fn logout(http_req: HttpRequest) -> Result<HttpResponse, AppError> {
    let secure = is_secure_request(&http_req);

    let access_cookie = create_clear_cookie("access_token", secure);
    let refresh_cookie = create_clear_cookie("refresh_token", secure);

    Ok(HttpResponse::Ok()
        .cookie(access_cookie)
        .cookie(refresh_cookie)
        .json(ApiResponse::<()>::success((), "Success")))
}

pub async fn refresh_token(
    state: web::Data<crate::app_state::AppState>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let conn = state.pool()?.get_conn();

    let token = if let Some(cookie) = http_req.cookie("refresh_token") {
        cookie.value().to_string()
    } else {
        match crate::auth::utils::extract_token_from_request(&http_req) {
            Some(t) => t,
            None => {
                return Err(AppError::Unauthorized("认证失败".to_string()));
            }
        }
    };

    let jwt_utils = &state.jwt_utils;

    let claims = jwt_utils.validate_token(&token).map_err(|err| {
        let msg = match err.kind() {
            ErrorKind::ExpiredSignature => "令牌已过期",
            _ => "无效令牌",
        };
        AppError::Unauthorized(msg.to_string())
    })?;

    if crate::utils::is_token_revoked(&conn, &token)
        .await
        .map_err(|e| { tracing::error!("检查令牌撤销状态失败: {}", e); AppError::Database(e.to_string()) })?
    {
        return Err(AppError::Unauthorized("令牌已撤销".to_string()));
    }

    let (ip_address, user_agent) = crate::auth::utils::get_client_info(&http_req);
    let current_fingerprint = JwtUtils::generate_device_fingerprint(&user_agent, &ip_address);

    if let Some(ref token_fingerprint) = claims.device_fingerprint
        && token_fingerprint != &current_fingerprint
    {
        return Err(AppError::Unauthorized("设备验证失败".to_string()));
    }

    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|e| AppError::Internal(format!("无效的用户标识: {e}")))?;
    let token_expiry =
        chrono::DateTime::from_timestamp(claims.exp as i64, 0).unwrap_or_else(Utc::now);
    if let Err(e) = crate::utils::revoke_token(&conn, &token, &user_id, token_expiry).await {
        tracing::error!("撤销令牌失败: {}", e);
    }

    let token_duration = claims.exp.saturating_sub(claims.iat);
    let remember_me = token_duration > 86400;

    let access_token = jwt_utils
        .generate_access_token(&user_id, &claims.username, &claims.role, Some(&current_fingerprint), Some(&ip_address))
        .map_err(|e| AppError::Internal(format!("令牌生成失败: {e}")))?;
    let new_refresh_token = jwt_utils
        .generate_refresh_token(&user_id, &claims.username, &claims.role, Some(&current_fingerprint), Some(&ip_address), remember_me)
        .map_err(|e| AppError::Internal(format!("令牌生成失败: {e}")))?;

    let access_token_expiry = jwt_utils.get_access_token_expiry();
    let refresh_token_expiry = jwt_utils.get_actual_refresh_token_expiry(remember_me);

    tracing::info!("用户 {} 令牌刷新成功", claims.username);

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
        .json(ApiResponse::success(
            serde_json::json!({ "expires_in": access_token_expiry, "remember_me": remember_me }),
            "Success",
        )))
}

pub async fn get_current_user(
    auth: crate::auth::extractor::AuthUser,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::<serde_json::Value>::success(
        serde_json::json!({ "id": auth.sub, "username": auth.username, "role": auth.role }),
        "Success",
    )))
}

pub async fn forgot_password(
    state: web::Data<crate::app_state::AppState>,
    req: web::Json<ForgotPasswordRequest>,
) -> Result<HttpResponse, AppError> {
    let conn = state.pool()?.get_conn();

    req.validate()?;
    let email = req.email.trim();

    let user_result = sqlx::query_as::<_, (Uuid, String, bool)>(
        "SELECT id, username, two_factor_enabled FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(&conn)
    .await;

    let success_msg = "如果该邮箱已注册，重置邮件已发送";

    if let Ok(Some((_user_id, _username, _two_factor_enabled))) = user_result {
        let reset_token = Uuid::new_v4().to_string();
        let expiry = Utc::now() + chrono::Duration::hours(1);

        if let Err(e) = sqlx::query(
            "UPDATE users SET reset_token = $1, reset_token_expiry = $2 WHERE email = $3",
        )
        .bind(&reset_token)
        .bind(expiry)
        .bind(email)
        .execute(&conn)
        .await
        {
            tracing::error!("保存重置令牌失败: {}", e);
        } else {
            let smtp_config = crate::system::smtp::get_smtp_config_from_db(&conn).await;
            if let Some(ref config) = smtp_config {
                let reset_link = format!("{}/reset-password?token={}", config.host, reset_token);
                let email_body = format!("请点击以下链接重置密码：{reset_link}");
                if let Err(e) = crate::system::smtp::send_email_async(&conn, email, "密码重置", &email_body).await {
                    tracing::error!("发送重置邮件失败: {}", e);
                }
            }
        }
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), success_msg)))
}

pub async fn reset_password(
    state: web::Data<crate::app_state::AppState>,
    req: web::Json<ResetPasswordRequest>,
) -> Result<HttpResponse, AppError> {
    let mut tx = state.pool()?.begin().await?;

    req.validate()?;

    let user_result = sqlx::query_as::<_, (Uuid,)>(
        "SELECT id FROM users WHERE reset_token = $1 AND reset_token_expiry > NOW() FOR UPDATE",
    )
    .bind(&req.token)
    .fetch_optional(&mut *tx)
    .await?;

    match user_result {
        Some((user_id,)) => {
            let hashed_password = hash_password(&req.new_password).await?;

            sqlx::query(
                "UPDATE users SET password = $1, reset_token = NULL, reset_token_expiry = NULL WHERE id = $2",
            )
            .bind(&hashed_password)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "密码重置成功")))
        }
        None => {
            Err(AppError::Validation("重置链接无效或已过期".to_string()))
        }
    }
}

pub async fn init_two_factor(
    state: web::Data<crate::app_state::AppState>,
    auth: crate::auth::extractor::AuthUser,
    req: web::Json<TwoFactorInitRequest>,
) -> Result<HttpResponse, AppError> {
    let conn = state.pool()?.get_conn();

    req.validate()?;

    let target_user_id = if let Some(user_id) = req.user_id {
        if auth.sub != user_id.to_string() && auth.role != "admin" {
            return Err(AppError::Forbidden("只有管理员可以为其他用户初始化2FA".to_string()));
        }
        user_id
    } else {
        Uuid::parse_str(&auth.sub)
            .map_err(|e| AppError::Validation(format!("无效的用户ID: {e}")))?
    };

    let target_username: String =
        match sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&conn)
            .await?
        {
            Some(name) => name,
            None => {
                return Err(AppError::NotFound("用户不存在".to_string()));
            }
        };

    let secret_bytes: Vec<u8> = {
        use rand::Rng;
        let mut bytes = vec![0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        bytes
    };
    let secret = Secret::Raw(secret_bytes);
    let secret_base32 = secret.to_encoded().to_string();

    let secret_bytes_for_totp = secret.to_bytes()
        .map_err(|e| AppError::Internal(format!("TOTP密钥转换失败: {e}")))?;
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes_for_totp,
        Some("IPMA".to_string()),
        target_username.clone(),
    ).map_err(|e| AppError::Internal(format!("生成TOTP失败: {e}")))?;

    let encrypted_secret = encrypt_password(&secret_base32)
        .ok_or_else(|| AppError::Internal("2FA密钥加密失败".to_string()))?;
    sqlx::query("UPDATE users SET two_factor_secret = $1 WHERE id = $2")
        .bind(&encrypted_secret)
        .bind(target_user_id)
        .execute(&conn)
        .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "otpauth_url": totp.get_url(),
            "qr_code_base64": totp.get_qr_base64().unwrap_or_default(),
        }),
        "2FA初始化成功，请使用认证器应用扫描二维码",
    )))
}

pub async fn enable_two_factor(
    state: web::Data<crate::app_state::AppState>,
    auth: crate::auth::extractor::AuthUser,
    req: web::Json<TwoFactorEnableRequest>,
) -> Result<HttpResponse, AppError> {
    let conn = state.pool()?.get_conn();

    req.validate()?;

    let target_user_id = if let Some(user_id) = req.user_id {
        if auth.sub != user_id.to_string() && auth.role != "admin" {
            return Err(AppError::Forbidden("只有管理员可以为其他用户启用2FA".to_string()));
        }
        user_id
    } else {
        Uuid::parse_str(&auth.sub)
            .map_err(|e| AppError::Validation(format!("无效的用户ID: {e}")))?
    };

    let secret: Option<String> =
        match sqlx::query_scalar("SELECT two_factor_secret FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&conn)
            .await?
        {
            Some(s) => s,
            None => {
                return Err(AppError::Validation("请先初始化2FA".to_string()));
            }
        };

    let Some(encrypted_secret) = secret else {
        return Err(AppError::Validation("请先初始化2FA".to_string()));
    };

    let secret = decrypt_password(&encrypted_secret).map_err(|e| AppError::Internal(format!("2FA密钥解密失败: {e}")))?;

    let target_username: String =
        match sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&conn)
            .await?
        {
            Some(name) => name,
            None => {
                return Err(AppError::NotFound("用户不存在".to_string()));
            }
        };

    let secret_bytes = Secret::Encoded(secret.clone())
        .to_bytes()
        .map_err(|_| AppError::Validation("密钥格式错误".to_string()))?;

    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some("IPMA".to_string()),
        target_username,
    ).map_err(|e| AppError::Internal(format!("TOTP创建失败: {e}")))?;

    let valid = totp.check_current(&req.code)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    if !valid {
        return Err(AppError::Validation("验证码错误".to_string()));
    }

    sqlx::query(
        "UPDATE users SET two_factor_enabled = true, two_factor_verified = true WHERE id = $1",
    )
    .bind(target_user_id)
    .execute(&conn)
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "2FA已启用")))
}

pub async fn disable_two_factor(
    state: web::Data<crate::app_state::AppState>,
    auth: crate::auth::extractor::AuthUser,
    req: web::Json<TwoFactorDisableRequest>,
) -> Result<HttpResponse, AppError> {
    let conn = state.pool()?.get_conn();

    req.validate()?;

    let target_user_id = if let Some(user_id) = req.user_id {
        if auth.sub != user_id.to_string() && auth.role != "admin" {
            return Err(AppError::Forbidden("只有管理员可以为其他用户禁用2FA".to_string()));
        }
        user_id
    } else {
        Uuid::parse_str(&auth.sub)
            .map_err(|e| AppError::Validation(format!("无效的用户ID: {e}")))?
    };

    let (secret, two_factor_enabled): (Option<String>, bool) = sqlx::query_as(
        "SELECT two_factor_secret, two_factor_enabled FROM users WHERE id = $1",
    )
    .bind(target_user_id)
    .fetch_one(&conn)
    .await?;

    if !two_factor_enabled {
        return Err(AppError::Validation("2FA未启用".to_string()));
    }

    let target_username: String =
        match sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(target_user_id)
            .fetch_optional(&conn)
            .await?
        {
            Some(name) => name,
            None => {
                return Err(AppError::NotFound("用户不存在".to_string()));
            }
        };

    let mut verified = false;

    if let Some(encrypted_secret) = secret {
        let secret = decrypt_password(&encrypted_secret).map_err(|e| AppError::Internal(format!("2FA密钥解密失败: {e}")))?;
        let secret_bytes = Secret::Encoded(secret)
            .to_bytes()
            .map_err(|e| AppError::Internal(format!("2FA密钥格式错误: {e}")))?;
        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            secret_bytes,
            Some("IPMA".to_string()),
            target_username,
        ).map_err(|e| AppError::Internal(format!("2FA密钥长度不足: {e}")))?;
        let valid = totp.check_current(&req.code)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        if valid {
            verified = true;
        }
    }

    if !verified {
        return Err(AppError::Validation("验证码错误".to_string()));
    }

    sqlx::query(
        "UPDATE users SET two_factor_enabled = false, two_factor_secret = NULL, two_factor_verified = false WHERE id = $1"
    )
    .bind(target_user_id)
    .execute(&conn)
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "2FA已禁用")))
}

#[derive(Debug, Deserialize, Validate)]
pub struct TwoFactorInitRequest {
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct TwoFactorEnableRequest {
    #[validate(length(min = 6, max = 6, message = "验证码长度必须为6个字符"))]
    pub code: String,
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct TwoFactorDisableRequest {
    #[validate(length(min = 6, max = 6, message = "验证码长度必须为6个字符"))]
    pub code: String,
    pub user_id: Option<Uuid>,
}

struct LoginTokens {
    access_token: String,
    refresh_token: String,
    access_token_expiry: u64,
    refresh_token_expiry: u64,
}

fn build_login_response(
    user: User,
    login_tokens: LoginTokens,
    user_lang: &str,
    secure: bool,
) -> HttpResponse {
    let access_cookie = create_auth_cookie(
        "access_token",
        &login_tokens.access_token,
        login_tokens.access_token_expiry as i64,
        secure,
    );
    let refresh_cookie = create_auth_cookie(
        "refresh_token",
        &login_tokens.refresh_token,
        login_tokens.refresh_token_expiry as i64,
        secure,
    );

    HttpResponse::Ok()
        .cookie(access_cookie)
        .cookie(refresh_cookie)
        .json(ApiResponse::success_i18n(
            serde_json::json!({ "user": user, "expires_in": login_tokens.access_token_expiry }),
            "api.success",
            user_lang,
        ))
}

fn generate_login_tokens(
    jwt_utils: &JwtUtils,
    id: &Uuid,
    username: &str,
    role: &str,
    device_fingerprint: &str,
    ip_address: &str,
    remember_me: bool,
) -> Result<LoginTokens, AppError> {
    let access_token = jwt_utils
        .generate_access_token(id, username, role, Some(device_fingerprint), Some(ip_address))
        .map_err(|e| AppError::Internal(format!("令牌生成失败: {e}")))?;
    let refresh_token = jwt_utils
        .generate_refresh_token(id, username, role, Some(device_fingerprint), Some(ip_address), remember_me)
        .map_err(|e| AppError::Internal(format!("令牌生成失败: {e}")))?;
    let access_token_expiry = jwt_utils.get_access_token_expiry();
    let refresh_token_expiry = jwt_utils.get_actual_refresh_token_expiry(remember_me);

    Ok(LoginTokens {
        access_token,
        refresh_token,
        access_token_expiry,
        refresh_token_expiry,
    })
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

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

fn is_secure_request(req: &HttpRequest) -> bool {
    req.connection_info().scheme() == "https"
}
