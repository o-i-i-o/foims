//! 初始化操作验证码校验。

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;

use axum::extract::State;
use axum::response::Response;
use foims_common::crypto::constant_time_eq;
use foims_common::{AppMessage, msg};
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
    foims_common::log_info!("log.init.verification.banner_title");
    info!("======================================================================");
    foims_common::log_info!("log.init.verification.code", code = code);
    foims_common::log_info!(
        "log.init.verification.expiry",
        minutes = VERIFICATION_CODE_EXPIRY_SECS / 60
    );
    foims_common::log_info!("log.init.verification.hint");
    info!("======================================================================\n");

    VerificationCode::new(code)
}

pub fn verify_code(provided_code: &str) -> Result<(), AppMessage> {
    // 校验成功即作废（替换为新随机码）：验证码必须一次性使用，
    // 否则 15 分钟有效期内同一验证码可重复通过 init/clear/create/import
    // 等多个危险操作的校验（security-review I-2）
    let mut stored_code = get_verification_code_storage()
        .lock()
        .map_err(|_| msg("server.init.verification.lock_failed"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|e| {
            foims_common::log_warn!("log.init.system_time_warning", error = e);
            std::time::Duration::from_secs(0)
        })
        .as_secs();

    // 时钟回拨时 created_at 可能晚于当前时刻：saturating_sub 避免 u64 下溢 panic
    if now.saturating_sub(stored_code.created_at) > VERIFICATION_CODE_EXPIRY_SECS {
        return Err(msg("server.init.verification.expired"));
    }

    if !constant_time_eq(&stored_code.code, provided_code) {
        return Err(msg("server.init.verification.invalid"));
    }

    // 消费验证码：替换为新随机码，重放的旧码立即失效
    *stored_code = VerificationCode::new(generate_verification_code());

    Ok(())
}

pub async fn get_verification_code(
    State(ctx): State<Arc<InitContext>>,
) -> Result<Response, InitError> {
    // 初始化模式已关闭时不再发放验证码（防止关闭后凭旧验证码重新触发生成）
    if !ctx.init_enabled() {
        return Err(InitError::Forbidden(msg("server.init.disabled")));
    }

    let verification_code = generate_and_print_verification_code();

    // 锁中毒必须报错而非静默跳过：否则日志展示新码、存储仍是旧码，
    // 用户按新码校验必然失败且原因无从排查
    let mut lock = get_verification_code_storage()
        .lock()
        .map_err(|_| InitError::Internal(msg("server.init.verification.lock_failed")))?;
    *lock = verification_code;

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

    /// 串行化触及全局验证码存储的测试：verify_code 依赖进程级
    /// OnceLock<Mutex> 存储，涉及该存储的多个测试并行运行会互相
    /// 覆盖对方写入的码，导致断言偶发失败（全量并行测试下可复现）
    static STORAGE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
        let _guard = STORAGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

    /// 验证码一次性消费：校验成功后同一验证码不可再次使用（I-2）
    #[test]
    fn verify_code_成功后即作废_重放被拒绝() {
        let _guard = STORAGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        {
            let mut lock = get_verification_code_storage()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *lock = VerificationCode {
                code: "EEEEEEEEEEEEEEEE".to_string(),
                created_at: now_secs(),
            };
        }
        // 首次校验成功
        assert!(verify_code("EEEEEEEEEEEEEEEE").is_ok());
        // 同一验证码立即重放 → 失败（已被替换为新随机码）
        let Err(m) = verify_code("EEEEEEEEEEEEEEEE") else {
            panic!("已消费的验证码重放应被拒绝");
        };
        assert_eq!(m.key(), "server.init.verification.invalid");
    }

    /// 时钟回拨（now < created_at）不应 panic，且按未过期处理
    #[test]
    fn verify_code_时钟回拨不panic() {
        let _guard = STORAGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        {
            let mut lock = get_verification_code_storage()
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *lock = VerificationCode {
                code: "FFFFFFFFFFFFFFFF".to_string(),
                // created_at 晚于当前时刻 60 秒，模拟时钟回拨
                created_at: now_secs().saturating_add(60),
            };
        }
        // 未 panic 且码正确时通过（saturating_sub 归零，不判定过期）
        assert!(verify_code("FFFFFFFFFFFFFFFF").is_ok());
    }
}
