use actix_web::{HttpResponse, web};
use rand::RngExt;
use std::sync::Mutex;
use std::sync::OnceLock;
use tracing::info;

use crate::app_state::AppState;
use crate::error::AppError;
use crate::init::types::{VERIFICATION_CODE_EXPIRY_SECS, VerificationCode};
use crate::models::ApiResponse;

static VERIFICATION_CODE: OnceLock<Mutex<VerificationCode>> = OnceLock::new();

fn get_verification_code_storage() -> &'static Mutex<VerificationCode> {
    VERIFICATION_CODE
        .get_or_init(|| Mutex::new(VerificationCode::new(generate_verification_code())))
}

fn generate_verification_code() -> String {
    let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    let mut code = String::with_capacity(16);
    let mut rng = rand::rng();
    for _ in 0..16 {
        let idx = rng.random_range(0..chars.len());
        code.push(chars[idx]);
    }
    code
}

fn generate_and_print_verification_code() -> VerificationCode {
    let code = generate_verification_code();

    info!("\n======================================================================");
    info!("                         系统初始化验证码                           ");
    info!("======================================================================");
    info!("  验证码: {}", code);
    info!("  有效期: 15分钟");
    info!("  请在初始化页面输入此验证码以完成系统初始化");
    info!("======================================================================\n");

    VerificationCode::new(code)
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

pub fn verify_code(provided_code: &str) -> Result<(), String> {
    let stored_code = get_verification_code_storage()
        .lock()
        .map_err(|_| "无法访问验证码".to_string())?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|e| {
            tracing::warn!("系统时间计算警告: {}", e);
            std::time::Duration::from_secs(0)
        })
        .as_secs();

    if now - stored_code.created_at > VERIFICATION_CODE_EXPIRY_SECS {
        return Err("验证码已过期，请重新生成验证码".to_string());
    }

    if !constant_time_eq(&stored_code.code, provided_code) {
        return Err("验证码无效".to_string());
    }

    Ok(())
}

pub async fn get_verification_code(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    if !state.config.init.enabled {
        return Err(AppError::Forbidden("系统初始化已在配置中禁用".to_string()));
    }

    let verification_code = generate_and_print_verification_code();

    if let Ok(mut lock) = get_verification_code_storage().lock() {
        *lock = verification_code;
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        (),
        "验证码生成成功，请检查服务器控制台。",
    )))
}
