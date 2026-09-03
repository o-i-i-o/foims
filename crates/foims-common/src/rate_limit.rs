//! 请求限流。

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use serde_json::json;

use crate::net::{get_real_ip_from_parts, normalize_ipv4_address};
use crate::{log_info, log_warn, msg};

const DEFAULT_EMAIL_LIMIT: u32 = 5;
const DEFAULT_EMAIL_WINDOW_SECS: u64 = 3600;

#[derive(Debug, Clone)]
pub struct RateLimitError {
    pub message: crate::AppMessage,
    pub retry_after: u64,
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl RateLimitError {
    /// 构造 429 速率限制响应（带 Retry-After 头）
    pub fn into_response(self) -> Response {
        // 与 ApiResponse 保持一致的结构：message 为 i18n key，动态参数放 message_params
        let mut body = json!({
            "success": false,
            "message": self.message.key(),
            "error_type": "rate_limit_exceeded",
            "retry_after": self.retry_after
        });
        if let Some(params) = self.message.params_map() {
            body["message_params"] = json!(params);
        }
        (
            StatusCode::TOO_MANY_REQUESTS,
            [("Retry-After", self.retry_after.to_string())],
            Json(body),
        )
            .into_response()
    }
}

#[derive(Clone)]
struct RateLimitEntry {
    count: u32,
    window_start: Instant,
    previous_count: u32,
    previous_window_start: Option<Instant>,
}

impl RateLimitEntry {
    fn new() -> Self {
        Self {
            count: 1,
            window_start: Instant::now(),
            previous_count: 0,
            previous_window_start: None,
        }
    }

    fn increment(&mut self) -> u32 {
        self.count += 1;
        self.count
    }

    fn is_expired(&self, window_secs: u64) -> bool {
        self.window_start.elapsed() > Duration::from_secs(window_secs)
    }

    fn weighted_count(&self, window_secs: u64) -> u32 {
        let current_elapsed = self.window_start.elapsed().as_secs_f64();
        // 防御性下界：window_secs=0 会产生除零 NaN（NaN as u32 == 0 使限流
        // 永不触发），配置校验之外在此再兜底为 1 秒
        let window_f64 = (window_secs as f64).max(1.0);

        let previous_weight = if let Some(prev_start) = self.previous_window_start {
            let prev_age = prev_start.elapsed().as_secs_f64();
            let overlap = (prev_age - window_f64).max(0.0);
            (1.0 - overlap / window_f64).max(0.0)
        } else {
            0.0
        };

        let current_weight = 1.0 - (current_elapsed / window_f64).min(1.0);

        // 向上取整：浮点截断曾使加权计数恒少 1（count=5 时算出 4），
        // limit=3 实际第 5 次请求才被拒，等于放宽了一档（security-review 第六节）
        (self.previous_count as f64 * previous_weight + self.count as f64 * current_weight).ceil()
            as u32
    }

    fn reset_window(&mut self) {
        self.previous_count = self.count;
        self.previous_window_start = Some(self.window_start);
        self.count = 1;
        self.window_start = Instant::now();
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    ip_limits: Arc<DashMap<String, RateLimitEntry>>,
    user_limits: Arc<DashMap<String, RateLimitEntry>>,
    email_limits: Arc<DashMap<String, RateLimitEntry>>,
    ip_limit: u32,
    user_limit: u32,
    login_limit: u32,
    window_secs: u64,
    email_limit: u32,
    email_window_secs: u64,
}

impl RateLimiter {
    #[must_use]
    pub fn new(ip_limit: u32, user_limit: u32, login_limit: u32, window_secs: u64) -> Self {
        Self {
            ip_limits: Arc::new(DashMap::new()),
            user_limits: Arc::new(DashMap::new()),
            email_limits: Arc::new(DashMap::new()),
            ip_limit,
            user_limit,
            login_limit,
            // window_secs=0 会使窗口立即过期、限流静默失效，钳制为 1 秒兜底
            //（配置层校验见 Config 校验口径，此处防御直接构造的调用方）
            window_secs: window_secs.max(1),
            email_limit: DEFAULT_EMAIL_LIMIT,
            email_window_secs: DEFAULT_EMAIL_WINDOW_SECS,
        }
    }

    #[must_use]
    pub fn with_email_limit(mut self, limit: u32, window_secs: u64) -> Self {
        self.email_limit = limit;
        self.email_window_secs = window_secs.max(1);
        self
    }

    /// 预鉴权限流检查：仅按 IP 维度（登录路径独立键与限额）与邮箱路径
    /// 邮箱桶计数。用户维度不在此处理（JWT 未验签不可信），已认证请求由
    /// [`RateLimiter::charge_user`] 在鉴权后补记。
    pub fn check_rate_limit(
        &self,
        ip: &str,
        is_login: bool,
        is_email: bool,
    ) -> Result<(), RateLimitError> {
        if is_email {
            let email_key = format!("email:{ip}");
            self.check_and_increment_with_window(
                &self.email_limits,
                &email_key,
                self.email_limit,
                self.email_window_secs,
            )?;
        }

        let ip_key = if is_login {
            format!("login:{ip}")
        } else {
            format!("ip:{ip}")
        };
        let ip_limit = if is_login {
            self.login_limit
        } else {
            self.ip_limit
        };
        self.check_and_increment(&self.ip_limits, &ip_key, ip_limit)?;

        Ok(())
    }

    /// 对已认证请求补记用户维度限流桶（超限返回 429 错误）。
    ///
    /// 预鉴权限流中间件无法安全取得用户身份（JWT 未验签的 `sub` 可伪造，
    /// 可借此填满受害者限流桶或绕开用户维度限流），因此该中间件路径只按
    /// IP 计数；`user` 桶由鉴权中间件之后的钩子对已验证 claims 的请求
    /// 调用本方法补记。限流语义：匿名按 IP，已认证 IP+用户双桶。
    pub fn charge_user(&self, user_id: &str) -> Result<(), RateLimitError> {
        self.check_and_increment(
            &self.user_limits,
            &format!("user:{user_id}"),
            self.user_limit,
        )
    }

    fn check_and_increment(
        &self,
        limits: &DashMap<String, RateLimitEntry>,
        key: &str,
        limit: u32,
    ) -> Result<(), RateLimitError> {
        self.check_and_increment_with_window(limits, key, limit, self.window_secs)
    }

    fn check_and_increment_with_window(
        &self,
        limits: &DashMap<String, RateLimitEntry>,
        key: &str,
        limit: u32,
        window_secs: u64,
    ) -> Result<(), RateLimitError> {
        if let Some(mut entry) = limits.get_mut(key) {
            if entry.is_expired(window_secs) {
                entry.reset_window();
            } else {
                let weighted = entry.weighted_count(window_secs);
                if weighted >= limit {
                    let retry_after = window_secs - entry.window_start.elapsed().as_secs();
                    log_warn!(
                        "log.rate_limit.triggered",
                        key = key,
                        count = entry.count,
                        weighted = weighted,
                        limit = limit,
                        window = window_secs,
                        retry_after = retry_after
                    );
                    let retry_after = retry_after.max(1);
                    return Err(RateLimitError {
                        message: msg("server.common.rate_limited").with("seconds", retry_after),
                        retry_after,
                    });
                }
                entry.increment();
            }
        } else {
            limits.insert(key.to_string(), RateLimitEntry::new());
        }

        Ok(())
    }

    pub fn cleanup_expired(&self) {
        let window_secs = self.window_secs;
        self.ip_limits
            .retain(|_, entry| !entry.is_expired(window_secs));

        self.user_limits
            .retain(|_, entry| !entry.is_expired(window_secs));

        let email_window_secs = self.email_window_secs;
        self.email_limits
            .retain(|_, entry| !entry.is_expired(email_window_secs));
    }
}

/// 速率限制中间件状态
#[derive(Clone)]
pub struct RateLimitState {
    pub limiter: RateLimiter,
    pub enabled: bool,
}

impl RateLimitState {
    #[must_use]
    pub const fn new(limiter: RateLimiter, enabled: bool) -> Self {
        Self { limiter, enabled }
    }
}

fn is_strict_path(path: &str) -> bool {
    // LDAP/SSO 回调同为爆破入口，纳入严格限流（A-9）
    let strict_paths = [
        "/api/auth/login",
        "/api/auth/login/email",
        "/api/auth/login/two-factor",
        "/api/auth/login/send-code",
        "/api/auth/login/send-2fa-code",
        "/api/auth/login/ldap",
        "/api/auth/forgot-password",
        "/api/auth/reset-password",
    ];
    strict_paths.contains(&path)
}

fn is_email_path(path: &str) -> bool {
    let email_paths = [
        "/api/auth/login/send-code",
        "/api/auth/login/send-2fa-code",
        "/api/auth/forgot-password",
    ];
    email_paths.contains(&path)
}

/// 速率限制中间件（axum from_fn_with_state 风格）
///
/// 用法：`.route_layer(middleware::from_fn_with_state(rate_limit_state, rate_limit_middleware))`
pub async fn rate_limit_middleware(
    State(state): State<RateLimitState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    if !state.enabled {
        return next.run(req).await;
    }

    let (parts, body) = req.into_parts();
    let path = parts.uri.path().to_string();
    let method = parts.method.clone();

    let is_strict = is_strict_path(&path);
    let is_email = is_email_path(&path);

    let direct_ip = get_real_ip_from_parts(&parts);

    // 统一做 IPv4 归一化（原 trusted_proxies 白名单无任何赋值入口恒为空，
    // if/else 两分支语义重复，属死逻辑，已删除）
    let ip = normalize_ipv4_address(&direct_ip);

    // 仅按 IP 维度预限流：JWT payload 未验签可伪造 sub，不能作为限流键；
    // 已认证请求的用户桶由鉴权中间件之后的 charge_user 钩子补记
    if let Err(e) = state.limiter.check_rate_limit(&ip, is_strict, is_email) {
        log_warn!(
            "log.rate_limit.blocked",
            method = method,
            path = path,
            ip = ip,
            user_id = "-",
            is_strict = is_strict,
            is_email = is_email,
            retry_after = e.retry_after
        );
        return e.into_response();
    }

    let req = axum::extract::Request::from_parts(parts, body);
    next.run(req).await
}

pub fn start_cleanup_task(
    limiter: RateLimiter,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(60)) => {
                    limiter.cleanup_expired();
                }
                _ = shutdown_rx.recv() => {
                    log_info!("log.rate_limit.cleanup_stopped");
                    break;
                }
            }
        }
    });
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- 滑动窗口条目 ----------

    #[test]
    fn test_rate_limit_entry_new_and_increment() {
        let mut entry = RateLimitEntry::new();
        assert_eq!(entry.count, 1);
        assert_eq!(entry.previous_count, 0);
        assert!(entry.previous_window_start.is_none());

        assert_eq!(entry.increment(), 2);
        assert_eq!(entry.increment(), 3);
        assert_eq!(entry.count, 3);
    }

    #[test]
    fn test_rate_limit_entry_is_expired() {
        // 新窗口未过期
        let entry = RateLimitEntry::new();
        assert!(!entry.is_expired(60));

        // 手工构造过期的窗口起点
        let mut stale = RateLimitEntry::new();
        stale.window_start = Instant::now() - Duration::from_secs(61);
        assert!(stale.is_expired(60));

        // 恰好 60 秒（未超过窗口）不算过期
        let mut boundary = RateLimitEntry::new();
        boundary.window_start = Instant::now() - Duration::from_secs(59);
        assert!(!boundary.is_expired(60));
    }

    #[test]
    fn test_rate_limit_entry_weighted_count_current_only() {
        // 无历史窗口：仅按当前窗口剩余权重折算，向上取整后等于实际计数
        let mut entry = RateLimitEntry::new();
        entry.count = 5;
        let weighted = entry.weighted_count(10);
        assert_eq!(weighted, 5, "当前窗口计数应向上取整为 5");
    }

    #[test]
    fn test_rate_limit_entry_weighted_count_with_previous() {
        // 上一窗口计数 10，已过去 15 秒（窗口 10 秒）→ 重叠 5 秒，权重约 0.5；
        // 当前窗口计数 0 → 加权计数约 5，向上取整为 5
        let entry = RateLimitEntry {
            count: 0,
            window_start: Instant::now(),
            previous_count: 10,
            previous_window_start: Some(Instant::now() - Duration::from_secs(15)),
        };
        let weighted = entry.weighted_count(10);
        assert!(
            (4..=5).contains(&weighted),
            "跨窗口加权计数应在 4~5 之间，实际 {weighted}"
        );
    }

    #[test]
    fn test_rate_limit_entry_weighted_count_expired_previous_zero() {
        // 上一窗口已远去（30 秒 > 窗口 10 秒的两倍）→ 权重归零；
        // 当前窗口计数 3、elapsed≈0 → 向上取整为 3
        let entry = RateLimitEntry {
            count: 3,
            window_start: Instant::now(),
            previous_count: 10,
            previous_window_start: Some(Instant::now() - Duration::from_secs(30)),
        };
        let weighted = entry.weighted_count(10);
        assert_eq!(weighted, 3, "历史窗口失效后加权计数应等于当前窗口计数");
    }

    #[test]
    fn test_rate_limit_entry_reset_window() {
        // 重置后：旧计数转入 previous，当前计数回到 1
        let mut entry = RateLimitEntry::new();
        entry.increment();
        entry.increment(); // count = 3
        let old_start = entry.window_start;
        entry.reset_window();

        assert_eq!(entry.count, 1);
        assert_eq!(entry.previous_count, 3);
        let Some(prev_start) = entry.previous_window_start else {
            panic!("重置后应记录上一窗口起点");
        };
        assert_eq!(prev_start, old_start);
        assert!(!entry.is_expired(60));
    }

    // ---------- 限流器整体行为 ----------

    #[test]
    fn test_check_rate_limit_ip_limit_enforced() {
        // ip_limit=3：恰好 3 次通过，第 4 次拒绝
        //（首次请求走新建条目路径不检查，第 2/3 次加权计数 1/2 < 3）
        let limiter = RateLimiter::new(3, 100, 100, 60);
        for round in 1..=3 {
            assert!(
                limiter.check_rate_limit("1.1.1.1", false, false).is_ok(),
                "第 {round} 次请求应通过"
            );
        }
        let Err(err) = limiter.check_rate_limit("1.1.1.1", false, false) else {
            panic!("达到 IP 限制后应被拒绝");
        };
        assert_eq!(err.message.key(), "server.common.rate_limited");
        assert!(err.retry_after >= 1);
    }

    #[test]
    fn test_check_rate_limit_distinct_ips_isolated() {
        // 不同 IP 计数相互隔离
        let limiter = RateLimiter::new(2, 100, 100, 60);
        assert!(limiter.check_rate_limit("1.1.1.1", false, false).is_ok());
        assert!(limiter.check_rate_limit("1.1.1.1", false, false).is_ok());
        assert!(limiter.check_rate_limit("2.2.2.2", false, false).is_ok());
    }

    #[test]
    fn test_check_rate_limit_login_key_isolated() {
        // 登录路径使用独立键与独立限额：login_limit=1 时第 2 次拒绝，
        // 且不影响普通路径的计数
        let limiter = RateLimiter::new(100, 100, 1, 60);
        assert!(limiter.check_rate_limit("3.3.3.3", true, false).is_ok());
        assert!(limiter.check_rate_limit("3.3.3.3", true, false).is_err());

        // 普通路径未受限
        assert!(limiter.check_rate_limit("3.3.3.3", false, false).is_ok());
    }

    #[test]
    fn test_charge_user_limit() {
        // user_limit=1：同一用户第 2 次经 charge_user 补记时被用户维度拒绝；
        // 不同用户互不影响
        let limiter = RateLimiter::new(100, 1, 100, 60);
        assert!(limiter.charge_user("user-1").is_ok());
        let Err(err) = limiter.charge_user("user-1") else {
            panic!("超过用户限制后应被拒绝");
        };
        assert_eq!(err.message.key(), "server.common.rate_limited");
        assert!(limiter.charge_user("user-2").is_ok());
    }

    #[test]
    fn test_check_rate_limit_email_limit() {
        // 邮箱验证码路径使用独立邮箱限额（默认窗口 3600 秒）
        let limiter = RateLimiter::new(100, 100, 100, 60).with_email_limit(1, 3600);
        assert!(limiter.check_rate_limit("5.5.5.5", false, true).is_ok());
        let Err(err) = limiter.check_rate_limit("5.5.5.5", false, true) else {
            panic!("超过邮箱发送限制后应被拒绝");
        };
        assert!(err.retry_after >= 1);
    }

    #[test]
    fn test_cleanup_expired_removes_stale_entries() {
        // 手工注入过期与未过期条目，验证清理只移除过期项
        let limiter = RateLimiter::new(100, 100, 100, 60);

        let mut stale = RateLimitEntry::new();
        stale.window_start = Instant::now() - Duration::from_secs(120);
        limiter.ip_limits.insert("ip:stale".to_string(), stale);

        limiter
            .user_limits
            .insert("user:fresh".to_string(), RateLimitEntry::new());

        let mut stale_email = RateLimitEntry::new();
        stale_email.window_start = Instant::now() - Duration::from_secs(7200);
        limiter
            .email_limits
            .insert("email:old".to_string(), stale_email);

        limiter.cleanup_expired();

        assert!(
            !limiter.ip_limits.contains_key("ip:stale"),
            "过期 IP 条目应被清理"
        );
        assert!(
            limiter.user_limits.contains_key("user:fresh"),
            "未过期用户条目应保留"
        );
        assert!(
            !limiter.email_limits.contains_key("email:old"),
            "过期邮箱条目应被清理"
        );
    }

    // ---------- 路径分类 ----------

    #[test]
    fn test_is_strict_path() {
        assert!(is_strict_path("/api/auth/login"));
        assert!(is_strict_path("/api/auth/login/ldap"));
        assert!(is_strict_path("/api/auth/forgot-password"));
        assert!(is_strict_path("/api/auth/reset-password"));
        assert!(!is_strict_path("/api/auth/logout"));
        assert!(!is_strict_path("/api/devices"));
        assert!(!is_strict_path(""));
    }

    #[test]
    fn test_is_email_path() {
        assert!(is_email_path("/api/auth/login/send-code"));
        assert!(is_email_path("/api/auth/login/send-2fa-code"));
        assert!(is_email_path("/api/auth/forgot-password"));
        // 登录主路径不是邮箱路径
        assert!(!is_email_path("/api/auth/login"));
        assert!(!is_email_path("/api/auth/reset-password"));
    }

    // ---------- 状态构造与 429 响应 ----------

    #[test]
    fn test_rate_limit_state_new() {
        let limiter = RateLimiter::new(10, 20, 5, 60);
        let state = RateLimitState::new(limiter, true);
        assert!(state.enabled);
        assert!(
            state
                .limiter
                .check_rate_limit("6.6.6.6", false, false)
                .is_ok()
        );

        let disabled = RateLimitState::new(RateLimiter::new(1, 1, 1, 1), false);
        assert!(!disabled.enabled);
    }

    #[test]
    fn test_rate_limit_error_display() {
        let err = RateLimitError {
            message: msg("server.common.rate_limited").with("seconds", 30),
            retry_after: 30,
        };
        assert_eq!(err.to_string(), err.message.to_string());
    }

    #[tokio::test]
    async fn test_rate_limit_error_into_response() {
        use axum::body::to_bytes;

        let err = RateLimitError {
            message: msg("server.common.rate_limited").with("seconds", 42),
            retry_after: 42,
        };
        let resp = err.into_response();

        // 状态码 429 + Retry-After 头
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            resp.headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok()),
            Some("42")
        );

        // 响应体结构与 ApiResponse 保持一致，动态参数进 message_params
        let body = to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap_or_else(|e| panic!("读取响应体失败: {e}"));
        let json: serde_json::Value =
            serde_json::from_slice(&body).unwrap_or_else(|e| panic!("响应体应为 JSON: {e}"));
        assert_eq!(json["success"], serde_json::json!(false));
        assert_eq!(
            json["message"],
            serde_json::json!("server.common.rate_limited")
        );
        assert_eq!(json["error_type"], serde_json::json!("rate_limit_exceeded"));
        assert_eq!(json["retry_after"], serde_json::json!(42));
        assert_eq!(json["message_params"]["seconds"], serde_json::json!("42"));
    }
}
