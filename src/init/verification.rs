use actix_web::{HttpResponse, Result, web};
use lazy_static::lazy_static;
use std::sync::Mutex;
use tracing::info;

use crate::config::Config;
use crate::models::ApiResponse;
use crate::init::types::{VerificationCode, VERIFICATION_CODE_EXPIRY_SECS};

lazy_static! {
    pub static ref VERIFICATION_CODE: Mutex<VerificationCode> = Mutex::new(VerificationCode::new(generate_verification_code()));
}

fn generate_verification_code() -> String {
    use rand::RngExt;
    let chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut code = String::with_capacity(16);
    let mut rng = rand::rng();

    for _ in 0..16 {
        let idx = rng.random_range(0..chars.len());
        code.push(chars.chars().nth(idx).unwrap());
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

pub fn verify_code(provided_code: &str) -> Result<(), String> {
    let stored_code = VERIFICATION_CODE.lock()
        .map_err(|_| "无法访问验证码".to_string())?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if now - stored_code.created_at > VERIFICATION_CODE_EXPIRY_SECS {
        return Err("验证码已过期，请重新生成验证码".to_string());
    }

    if stored_code.code != provided_code {
        return Err("验证码无效".to_string());
    }

    Ok(())
}

pub async fn get_verification_code(config: web::Data<Config>) -> Result<HttpResponse> {
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    let verification_code = generate_and_print_verification_code();

    if let Ok(mut lock) = VERIFICATION_CODE.lock() {
        *lock = verification_code;
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        (),
        "验证码生成成功，请检查服务器控制台。",
    )))
}
