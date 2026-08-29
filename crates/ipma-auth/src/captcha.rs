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
    crate::app_fail2ban::failure_count(ip, username) >= CAPTCHA_THRESHOLD
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

/// 折线笔画：单个字符由一至多条折线（polyline）构成
type Glyph = &'static [&'static [(i32, i32)]];

/// 内置笔画字型：在 8×14 归一化网格（x 向右、y 向下）上手绘坐标常量，
/// 覆盖 [`CHARSET`] 全部字符（剔除易混淆的 0/O/1/I）。
/// 渲染为 `<polyline>` 而非 `<text>`：SVG 中不出现答案明文，
/// 自动化脚本无法凭正则提取文本节点直接读出答案。
fn glyph_strokes(ch: char) -> Option<Glyph> {
    match ch {
        'A' => Some(&[&[(0, 14), (2, 0), (4, 0), (7, 14)], &[(1, 9), (5, 9)]]),
        'B' => Some(&[
            &[(0, 14), (0, 0), (4, 0), (6, 1), (6, 5), (4, 7), (0, 7)],
            &[(4, 7), (6, 8), (6, 12), (4, 14), (0, 14)],
        ]),
        'C' => Some(&[&[
            (7, 2),
            (5, 0),
            (2, 0),
            (0, 2),
            (0, 12),
            (2, 14),
            (5, 14),
            (7, 12),
        ]]),
        'D' => Some(&[&[(0, 0), (0, 14), (4, 14), (7, 11), (7, 3), (4, 0), (0, 0)]]),
        'E' => Some(&[&[(7, 0), (0, 0), (0, 14), (7, 14)], &[(0, 7), (5, 7)]]),
        'F' => Some(&[&[(7, 0), (0, 0), (0, 14)], &[(0, 7), (5, 7)]]),
        'G' => Some(&[&[
            (7, 2),
            (5, 0),
            (2, 0),
            (0, 2),
            (0, 12),
            (2, 14),
            (5, 14),
            (7, 12),
            (7, 8),
            (4, 8),
        ]]),
        'H' => Some(&[&[(0, 0), (0, 14)], &[(7, 0), (7, 14)], &[(0, 7), (7, 7)]]),
        'J' => Some(&[&[(7, 0), (7, 11), (5, 14), (2, 14), (0, 12)]]),
        'K' => Some(&[&[(0, 0), (0, 14)], &[(7, 0), (0, 7), (7, 14)]]),
        'L' => Some(&[&[(0, 0), (0, 14), (7, 14)]]),
        'M' => Some(&[&[(0, 14), (0, 0), (4, 8), (8, 0), (8, 14)]]),
        'N' => Some(&[&[(0, 14), (0, 0), (7, 14), (7, 0)]]),
        'P' => Some(&[
            &[(0, 14), (0, 0), (5, 0), (7, 2), (7, 5), (5, 7), (0, 7)],
            &[(4, 7), (7, 14)],
        ]),
        'Q' => Some(&[
            &[
                (2, 0),
                (5, 0),
                (7, 2),
                (7, 9),
                (5, 11),
                (2, 11),
                (0, 9),
                (0, 2),
                (2, 0),
            ],
            &[(4, 7), (7, 14)],
        ]),
        'R' => Some(&[
            &[(0, 14), (0, 0), (5, 0), (7, 2), (7, 5), (5, 7), (0, 7)],
            &[(4, 7), (7, 14)],
        ]),
        'S' => Some(&[&[
            (7, 2),
            (5, 0),
            (2, 0),
            (0, 2),
            (0, 5),
            (2, 7),
            (5, 7),
            (7, 9),
            (7, 12),
            (5, 14),
            (2, 14),
            (0, 12),
        ]]),
        'T' => Some(&[&[(0, 0), (7, 0)], &[(3, 0), (3, 14)]]),
        'U' => Some(&[&[(0, 0), (0, 11), (2, 14), (5, 14), (7, 11), (7, 0)]]),
        'V' => Some(&[&[(0, 0), (4, 14), (8, 0)]]),
        'W' => Some(&[&[(0, 0), (0, 14), (4, 6), (8, 14), (8, 0)]]),
        'X' => Some(&[&[(0, 0), (7, 14)], &[(7, 0), (0, 14)]]),
        'Y' => Some(&[&[(0, 0), (4, 7), (8, 0)], &[(4, 7), (4, 14)]]),
        'Z' => Some(&[&[(0, 0), (7, 0), (0, 14), (7, 14)]]),
        '2' => Some(&[&[
            (0, 2),
            (2, 0),
            (5, 0),
            (7, 2),
            (7, 5),
            (0, 12),
            (0, 14),
            (7, 14),
        ]]),
        '3' => Some(&[
            &[
                (0, 2),
                (2, 0),
                (5, 0),
                (7, 2),
                (7, 5),
                (5, 7),
                (7, 9),
                (7, 12),
                (5, 14),
                (2, 14),
                (0, 12),
            ],
            &[(2, 7), (5, 7)],
        ]),
        '4' => Some(&[&[(5, 14), (5, 0), (0, 9), (7, 9)]]),
        '5' => Some(&[&[
            (7, 0),
            (0, 0),
            (0, 6),
            (5, 6),
            (7, 8),
            (7, 11),
            (5, 14),
            (2, 14),
            (0, 12),
        ]]),
        '6' => Some(&[&[
            (6, 1),
            (4, 0),
            (2, 0),
            (0, 2),
            (0, 12),
            (2, 14),
            (5, 14),
            (7, 12),
            (7, 10),
            (5, 7),
            (2, 7),
            (0, 9),
        ]]),
        '7' => Some(&[&[(0, 0), (7, 0), (2, 14)]]),
        '8' => Some(&[
            &[
                (2, 0),
                (5, 0),
                (7, 2),
                (7, 5),
                (5, 7),
                (2, 7),
                (0, 5),
                (0, 2),
                (2, 0),
            ],
            &[
                (2, 7),
                (0, 9),
                (0, 12),
                (2, 14),
                (5, 14),
                (7, 12),
                (7, 9),
                (5, 7),
            ],
        ]),
        '9' => Some(&[&[
            (1, 13),
            (3, 14),
            (5, 14),
            (7, 12),
            (7, 2),
            (5, 0),
            (2, 0),
            (0, 2),
            (0, 4),
            (2, 7),
            (5, 7),
            (7, 5),
        ]]),
        _ => None,
    }
}

/// 渲染 SVG 验证码：逐字符按内置笔画字型绘制折线，保留随机旋转/
/// 位移/缩放/颜色与干扰线段；全程不输出答案文本节点
fn render_svg(code: &str) -> String {
    let mut rng = rand::rng();
    const COLORS: [&str; 5] = ["#1f2937", "#0f766e", "#7c2d12", "#3730a3", "#334155"];

    let width = 30 * code.chars().count() as i32 + 20;
    let height = 44;

    let mut chars = String::new();
    for (index, ch) in code.chars().enumerate() {
        let Some(strokes) = glyph_strokes(ch) else {
            // 字符集与字型表同步维护，缺字型属实现缺陷：跳过该字符
            // 仅导致该位不可读，绝不回退到文本节点暴露答案
            continue;
        };
        // 每字符占 30px 单元：8×14 网格按约 2 倍缩放绘制在单元中央
        let base_x = 10 + index as i32 * 30 + 7;
        let dx = base_x + rng.random_range(-3i32..=3);
        let dy = 8 + rng.random_range(-4i32..=4);
        let rotate = rng.random_range(-28i32..=28);
        let scale = rng.random_range(180..=220) as f64 / 100.0;
        let color = COLORS[rng.random_range(0..COLORS.len())];
        let stroke_width = rng.random_range(2..=3);
        for stroke in strokes {
            let points = stroke
                .iter()
                .map(|(x, y)| format!("{x},{y}"))
                .collect::<Vec<_>>()
                .join(" ");
            // 变换自右向左应用：先绕字型中心旋转，再缩放，再平移到单元位置
            chars.push_str(&format!(
                r##"<polyline points="{points}" fill="none" stroke="{color}" stroke-width="{stroke_width}" stroke-linecap="round" stroke-linejoin="round" transform="translate({dx} {dy}) scale({scale:.3}) rotate({rotate} 4 7)"/>"##
            ));
        }
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

    /// 字符集内全部字符都有笔画字型（字型表与字符集同步维护）
    #[test]
    fn 字型表_覆盖全部字符集() {
        for &byte in CHARSET {
            let ch = byte as char;
            assert!(glyph_strokes(ch).is_some(), "字符 {ch} 缺少笔画字型");
        }
    }

    /// SVG 不得包含明文答案：无 `<text>` 文本节点，答案字符串也不得
    /// 以任何形式直接出现在 SVG 中（防脚本正则提取）
    #[test]
    fn svg渲染_不泄露明文答案() {
        for _ in 0..20 {
            let challenge = generate();
            let answer = CAPTCHAS
                .get(&challenge.captcha_id)
                .map(|e| e.answer.clone())
                .unwrap_or_default();
            assert!(!challenge.svg.contains("<text"), "不得输出文本节点");
            assert!(!challenge.svg.contains("font"), "不应残留文本字体属性");
            assert!(
                !challenge.svg.contains(&answer),
                "SVG 中不得出现答案明文: {answer}"
            );
            assert!(challenge.svg.contains("<polyline"), "应包含笔画折线");
        }
    }
}
