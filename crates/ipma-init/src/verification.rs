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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 验证码字符集（大小写字母与数字）
    const CODE_CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

    /// 当前 Unix 秒（用于构造未过期的验证码）
    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    #[test]
    fn 生成验证码_长度16且字符集合法() {
        for _ in 0..8 {
            let code = generate_verification_code();
            assert_eq!(code.chars().count(), 16, "验证码应为 16 位: {code}");
            assert!(
                code.chars().all(|c| CODE_CHARS.contains(c)),
                "验证码含非法字符: {code}"
            );
        }
    }

    #[test]
    fn 生成验证码_两次生成结果不同() {
        let a = generate_verification_code();
        let b = generate_verification_code();
        assert_ne!(a, b, "随机生成器应产生不同验证码");
    }

    #[test]
    fn 常量时间比较_各分支() {
        assert!(constant_time_eq("abcdef", "abcdef"), "相同字符串应相等");
        assert!(!constant_time_eq("abcdef", "abcdeX"), "同长度不同内容");
        assert!(!constant_time_eq("abc", "abcd"), "长度不同直接不等");
        assert!(!constant_time_eq("", "a"), "空串与非空串");
        assert!(constant_time_eq("", ""), "两个空串相等");
    }

    /// verify_code 依赖进程级全局验证码存储（OnceLock<Mutex>），
    /// 所有涉及该存储的分支集中在一个测试内串行执行避免并行互扰
    #[test]
    fn verify_code_过期_错误码_成功_边界分支() {
        // 覆盖过期分支：写入 created_at=0 的历史验证码，即使码正确也应报过期
        {
            let mut lock = get_verification_code_storage()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *lock = VerificationCode {
                code: "AAAAAAAAAAAAAAAA".to_string(),
                created_at: 0,
            };
        }
        let Err(m) = verify_code("AAAAAAAAAAAAAAAA") else {
            panic!("已过期的验证码应校验失败");
        };
        assert_eq!(m.key(), "server.init.verification.expired");

        // 覆盖错误码分支：未过期但提供的码不匹配
        {
            let mut lock = get_verification_code_storage()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *lock = VerificationCode {
                code: "BBBBBBBBBBBBBBBB".to_string(),
                created_at: now_secs(),
            };
        }
        let Err(m) = verify_code("CCCCCCCCCCCCCCCC") else {
            panic!("错误验证码应校验失败");
        };
        assert_eq!(m.key(), "server.init.verification.invalid");
        // 长度不匹配的码同样落入错误码分支
        let Err(m) = verify_code("short") else {
            panic!("长度不符的验证码应校验失败");
        };
        assert_eq!(m.key(), "server.init.verification.invalid");

        // 覆盖成功分支：未过期且码一致
        let Ok(()) = verify_code("BBBBBBBBBBBBBBBB") else {
            panic!("正确的验证码应校验通过");
        };

        // 有效期边界内：created_at 距今 10 分钟（< 15 分钟），码正确应通过
        {
            let mut lock = get_verification_code_storage()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *lock = VerificationCode {
                code: "DDDDDDDDDDDDDDDD".to_string(),
                created_at: now_secs().saturating_sub(10 * 60),
            };
        }
        assert!(verify_code("DDDDDDDDDDDDDDDD").is_ok());
    }
}
