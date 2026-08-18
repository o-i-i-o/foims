//! 初始化操作验证码校验。

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;

use axum::extract::State;
use axum::response::Response;
use ipma_common::{AppMessage, msg};
use rand::RngExt;
use tracing::info;

use crate::context::InitContext;
use crate::error::{InitError, ok_json};
use crate::types::{VERIFICATION_CODE_EXPIRY_SECS, VerificationCode};

static VERIFICATION_CODE: OnceLock<Mutex<VerificationCode>> = OnceLock::new();

fn get_verification_code_storage() -> &'static Mutex<VerificationCode> {
    VERIFICATION_CODE
        .get_or_init(|| Mutex::new(VerificationCode::new(generate_verification_code())))
}

fn generate_verification_code() -> String {
    let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
        .chars()
        .collect();
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

    // 分隔线为纯装饰性技术输出，内容行按 i18n 宏输出
    info!("\n======================================================================");
    ipma_common::log_info!("log.init.verification.banner_title");
    info!("======================================================================");
    ipma_common::log_info!("log.init.verification.code", code = code);
    ipma_common::log_info!(
        "log.init.verification.expiry",
        minutes = VERIFICATION_CODE_EXPIRY_SECS / 60
    );
    ipma_common::log_info!("log.init.verification.hint");
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

pub fn verify_code(provided_code: &str) -> Result<(), AppMessage> {
    let stored_code = get_verification_code_storage()
        .lock()
        .map_err(|_| msg("server.init.verification.lock_failed"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|e| {
            ipma_common::log_warn!("log.init.system_time_warning", error = e);
            std::time::Duration::from_secs(0)
        })
        .as_secs();

    if now - stored_code.created_at > VERIFICATION_CODE_EXPIRY_SECS {
        return Err(msg("server.init.verification.expired"));
    }

    if !constant_time_eq(&stored_code.code, provided_code) {
        return Err(msg("server.init.verification.invalid"));
    }

    Ok(())
}

pub async fn get_verification_code(
    State(_ctx): State<Arc<InitContext>>,
) -> Result<Response, InitError> {
    let verification_code = generate_and_print_verification_code();

    if let Ok(mut lock) = get_verification_code_storage().lock() {
        *lock = verification_code;
    }

    Ok(ok_json((), "server.init.verification.generated"))
}
