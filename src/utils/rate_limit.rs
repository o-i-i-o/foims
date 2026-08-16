//! 请求限流。

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use serde_json::json;

use crate::utils::{get_real_ip_from_parts, normalize_ipv4_address};

const DEFAULT_EMAIL_LIMIT: u32 = 5;
const DEFAULT_EMAIL_WINDOW_SECS: u64 = 3600;

fn extract_user_id_from_parts(parts: &Parts) -> Option<String> {
    let auth_header = parts.headers.get(AUTHORIZATION)?.to_str().ok()?;

    if !auth_header.starts_with("Bearer ") {
        return None;
    }

    let token = auth_header.strip_prefix("Bearer ")?;

    let jwt_parts: Vec<&str> = token.split('.').collect();
    if jwt_parts.len() != 3 {
        return None;
    }

    use base64::Engine;
    let payload = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(jwt_parts[1]) {
        Ok(p) => p,
        Err(e) => {
            tracing::trace!("Base64解码JWT payload失败: {}", e);
            return None;
        }
    };
    let claims: serde_json::Value = match serde_json::from_slice(&payload) {
        Ok(c) => c,
        Err(e) => {
            tracing::trace!("解析JWT claims失败: {}", e);
            return None;
        }
    };

    claims
        .get("sub")?
        .as_str()
        .map(std::string::ToString::to_string)
}

#[derive(Debug, Clone)]
pub struct RateLimitError {
    pub message: String,
    pub retry_after: u64,
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl RateLimitError {
    /// 构造 429 速率限制响应（带 Retry-After 头）
    fn into_response(self) -> Response {
        (
            StatusCode::TOO_MANY_REQUESTS,
            [("Retry-After", self.retry_after.to_string())],
            Json(json!({
                "success": false,
                "message": &self.message,
                "error_type": "rate_limit_exceeded",
                "retry_after": self.retry_after
            })),
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
        let window_f64 = window_secs as f64;

        let previous_weight = if let Some(prev_start) = self.previous_window_start {
            let prev_age = prev_start.elapsed().as_secs_f64();
            let overlap = (prev_age - window_f64).max(0.0);
            (1.0 - overlap / window_f64).max(0.0)
        } else {
            0.0
        };

        let current_weight = 1.0 - (current_elapsed / window_f64).min(1.0);

        (self.previous_count as f64 * previous_weight + self.count as f64 * current_weight) as u32
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
    trusted_proxies: Vec<String>,
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
            window_secs,
            email_limit: DEFAULT_EMAIL_LIMIT,
            email_window_secs: DEFAULT_EMAIL_WINDOW_SECS,
            trusted_proxies: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_email_limit(mut self, limit: u32, window_secs: u64) -> Self {
        self.email_limit = limit;
        self.email_window_secs = window_secs;
        self
    }

    pub fn check_rate_limit(
        &self,
        ip: &str,
        user_id: Option<&str>,
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

        if let Some(uid) = user_id {
            self.check_and_increment(&self.user_limits, &format!("user:{uid}"), self.user_limit)?;
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
                    tracing::warn!(
                        "速率限制触发: key={}, 当前计数={}, 加权计数={}, 限制={}, 窗口={}s, 重试等待={}s",
                        key,
                        entry.count,
                        weighted,
                        limit,
                        window_secs,
                        retry_after
                    );
                    return Err(RateLimitError {
                        message: format!("请求过于频繁，请在 {retry_after} 秒后重试"),
                        retry_after: retry_after.max(1),
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

    fn is_trusted_proxy(&self, ip: &str) -> bool {
        self.trusted_proxies.iter().any(|p| p == ip)
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
    let strict_paths = [
        "/api/auth/login",
        "/api/auth/login/email",
        "/api/auth/login/two-factor",
        "/api/auth/login/send-code",
        "/api/auth/login/send-2fa-code",
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

    let ip = if state.limiter.is_trusted_proxy(&direct_ip) {
        direct_ip
    } else {
        normalize_ipv4_address(&direct_ip)
    };

    let user_id = if is_strict {
        None
    } else {
        extract_user_id_from_parts(&parts)
    };

    if let Err(e) = state
        .limiter
        .check_rate_limit(&ip, user_id.as_deref(), is_strict, is_email)
    {
        tracing::warn!(
            "请求被速率限制拦截: method={}, path={}, ip={}, user_id={}, is_strict={}, is_email={}, retry_after={}s",
            method,
            path,
            ip,
            user_id.as_deref().unwrap_or("-"),
            is_strict,
            is_email,
            e.retry_after
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
                    tracing::info!("速率限制清理任务收到关闭信号，停止运行");
                    break;
                }
            }
        }
    });
}
