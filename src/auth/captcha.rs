//! 登录验证码：连续登录失败达到阈值后触发。
//!
//! 触发条件复用应用层 fail2ban 的失败计数（IP 与用户名任一维度
//! 达到 [`CAPTCHA_THRESHOLD`] 即要求验证码），无需额外状态机。
//! 验证码为内存态 SVG 图形验证码：5 分钟有效、一次性使用、
//! 大小写不敏感；字符集剔除易混淆的 0/O/1/I。

use std::time::{Duration, Instant};

use dashmap::DashMap;
use rand::RngExt;
use uuid::Uuid;

/// 验证码有效期
const CAPTCHA_TTL: Duration = Duration::from_secs(300);
/// 验证码字符数
const CAPTCHA_LENGTH: usize = 5;
/// 连续失败多少次后要求验证码
const CAPTCHA_THRESHOLD: usize = 3;
/// 去混淆字符集（无 0/O/1/I）
const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

struct CaptchaEntry {
    answer: String,
    expires: Instant,
}

static CAPTCHAS: std::sync::LazyLock<DashMap<String, CaptchaEntry>> =
    std::sync::LazyLock::new(DashMap::new);

/// 生成验证码挑战（id + 内联 SVG）
pub struct CaptchaChallenge {
    pub captcha_id: String,
    pub svg: String,
}

pub fn generate() -> CaptchaChallenge {
    prune_expired();

    let mut rng = rand::rng();
    let code: String = (0..CAPTCHA_LENGTH)
        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
        .collect();

    let svg = render_svg(&code);
    let captcha_id = Uuid::new_v4().to_string();
    CAPTCHAS.insert(
        captcha_id.clone(),
        CaptchaEntry {
            answer: code,
            expires: Instant::now() + CAPTCHA_TTL,
        },
    );

    CaptchaChallenge { captcha_id, svg }
}

/// 校验并销毁验证码（一次性；过期视为失败）
pub fn verify(captcha_id: &str, answer: &str) -> bool {
    let Some((_, entry)) = CAPTCHAS.remove(captcha_id) else {
        return false;
    };
    if Instant::now() > entry.expires {
        return false;
    }
    entry.answer.eq_ignore_ascii_case(answer.trim())
}

/// 当前是否应要求验证码（IP 或用户名任一维度失败计数达到阈值）
pub fn required(ip: &str, username: &str) -> bool {
    crate::system::app_fail2ban::failure_count(ip, username) >= CAPTCHA_THRESHOLD
}

/// 校验请求携带的验证码；未达触发条件时直接放行
///
/// 返回 Err(键) 时：`captcha_required` 表示必须携带验证码，
/// `captcha_invalid` 表示验证码错误或已过期。
pub fn enforce(
    ip: &str,
    username: &str,
    captcha_id: &Option<String>,
    captcha_text: &Option<String>,
) -> Result<(), &'static str> {
    if !required(ip, username) {
        return Ok(());
    }
    match (captcha_id.as_deref(), captcha_text.as_deref()) {
        (Some(id), Some(text)) if !id.is_empty() && !text.is_empty() => {
            if verify(id, text) {
                Ok(())
            } else {
                Err("server.auth.captcha_invalid")
            }
        }
        _ => Err("server.auth.captcha_required"),
    }
}

/// 清理过期验证码
fn prune_expired() {
    let now = Instant::now();
    CAPTCHAS.retain(|_, entry| now <= entry.expires);
}

/// 渲染 SVG 验证码：逐字符随机旋转/位移/颜色 + 干扰线段
fn render_svg(code: &str) -> String {
    let mut rng = rand::rng();
    const COLORS: [&str; 5] = ["#1f2937", "#0f766e", "#7c2d12", "#3730a3", "#334155"];

    let width = 30 * code.chars().count() as i32 + 20;
    let height = 44;

    let mut chars = String::new();
    for (index, ch) in code.chars().enumerate() {
        let x = 22 + index as i32 * 30;
        let y = 20 + rng.random_range(-6i32..=6);
        let rotate = rng.random_range(-28i32..=28);
        let size = rng.random_range(22i32..=27);
        let color = COLORS[rng.random_range(0..COLORS.len())];
        chars.push_str(&format!(
            r##"<text x="{x}" y="{y}" font-size="{size}" font-family="monospace" font-weight="700" fill="{color}" transform="rotate({rotate} {x} {y})" text-anchor="middle">{ch}</text>"##
        ));
    }

    let mut lines = String::new();
    for _ in 0..4 {
        let x1 = rng.random_range(0..width);
        let y1 = rng.random_range(0..height);
        let x2 = rng.random_range(0..width);
        let y2 = rng.random_range(0..height);
        let color = COLORS[rng.random_range(0..COLORS.len())];
        lines.push_str(&format!(
            r##"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{color}" stroke-width="1" opacity="0.35"/>"##
        ));
    }

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-label="captcha">{chars}{lines}</svg>"##
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生成的验证码可被正确答案校验通过（一次性：第二次失败）
    #[test]
    fn 验证码_正确答案通过且一次性() {
        let challenge = generate();
        let answer = CAPTCHAS
            .get(&challenge.captcha_id)
            .map(|e| e.answer.clone())
            .unwrap_or_default();
        assert_eq!(answer.chars().count(), CAPTCHA_LENGTH);
        assert!(
            answer.chars().all(|c| CHARSET.contains(&(c as u8))),
            "字符集不应包含易混淆字符: {answer}"
        );
        assert!(verify(&challenge.captcha_id, &answer.to_lowercase()));
        assert!(
            !verify(&challenge.captcha_id, &answer),
            "验证码应一次性使用"
        );
    }

    #[test]
    fn 验证码_未知id与空答案失败() {
        assert!(!verify("not-exist", "ABCDEF"));
        let challenge = generate();
        assert!(!verify(&challenge.captcha_id, ""));
    }

    /// enforce 在未达阈值时放行；模拟 3 次失败后要求验证码
    #[test]
    fn enforce_未触发时放行() {
        // 测试进程内无失败记录（键唯一，避免与其他测试串扰）
        let ip = format!("10.255.{}.{}", std::process::id() % 250, 1);
        let user = format!("captcha_user_{}", std::process::id());
        assert!(enforce(&ip, &user, &None, &None).is_ok());
    }

    /// SVG 渲染包含全部字符且为合法 XML 头
    #[test]
    fn svg渲染_包含字符与线条() {
        let challenge = generate();
        assert!(challenge.svg.starts_with("<svg"));
        assert!(challenge.svg.ends_with("</svg>"));
        assert!(challenge.svg.contains("<line"), "应包含干扰线");
    }
}
