use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::{
    HttpResponse, ResponseError,
    body::EitherBody,
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
};
use dashmap::DashMap;
use futures_util::future::LocalBoxFuture;
use serde_json::json;

use crate::utils::get_real_ip_from_request;
use actix_web::http::header::AUTHORIZATION;

const DEFAULT_IP_LIMIT: u32 = 100;
const DEFAULT_USER_LIMIT: u32 = 200;
const DEFAULT_LOGIN_LIMIT: u32 = 5;
const DEFAULT_WINDOW_SECS: u64 = 60;
const DEFAULT_EMAIL_LIMIT: u32 = 5;
const DEFAULT_EMAIL_WINDOW_SECS: u64 = 3600;

fn extract_user_id_from_token(req: &ServiceRequest) -> Option<String> {
    let auth_header = req.headers().get(AUTHORIZATION)?.to_str().ok()?;

    if !auth_header.starts_with("Bearer ") {
        return None;
    }

    let token = auth_header.strip_prefix("Bearer ")?;

    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    use base64::Engine;
    let payload = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]) {
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

    claims.get("sub")?.as_str().map(std::string::ToString::to_string)
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

impl ResponseError for RateLimitError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::TooManyRequests()
            .insert_header(("Retry-After", self.retry_after.to_string()))
            .json(json!({
                "success": false,
                "message": &self.message,
                "error_type": "rate_limit_exceeded",
                "retry_after": self.retry_after
            }))
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

    #[must_use]
    pub fn default_limiter() -> Self {
        Self::new(
            DEFAULT_IP_LIMIT,
            DEFAULT_USER_LIMIT,
            DEFAULT_LOGIN_LIMIT,
            DEFAULT_WINDOW_SECS,
        )
    }

    pub fn with_trusted_proxies(mut self, proxies: Vec<String>) -> Self {
        self.trusted_proxies = proxies;
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
                        key, entry.count, weighted, limit, window_secs, retry_after
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
        self.ip_limits.retain(|_, entry| !entry.is_expired(window_secs));

        self.user_limits.retain(|_, entry| !entry.is_expired(window_secs));

        let email_window_secs = self.email_window_secs;
        self.email_limits.retain(|_, entry| !entry.is_expired(email_window_secs));
    }

    fn is_trusted_proxy(&self, ip: &str) -> bool {
        self.trusted_proxies.iter().any(|p| p == ip)
    }
}

pub struct RateLimitMiddleware {
    limiter: RateLimiter,
    enabled: bool,
}

impl RateLimitMiddleware {
    #[must_use]
    pub const fn new(limiter: RateLimiter, enabled: bool) -> Self {
        Self { limiter, enabled }
    }
}

impl<S, B> Transform<S, ServiceRequest> for RateLimitMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = actix_web::Error;
    type Transform = RateLimitMiddlewareService<S>;
    type InitError = ();
    type Future = LocalBoxFuture<'static, Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        let limiter = self.limiter.clone();
        let enabled = self.enabled;
        Box::pin(async move {
            Ok(RateLimitMiddlewareService {
                service,
                limiter,
                enabled,
            })
        })
    }
}

pub struct RateLimitMiddlewareService<S> {
    service: S,
    limiter: RateLimiter,
    enabled: bool,
}

impl<S> RateLimitMiddlewareService<S> {
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
}

impl<S, B> Service<ServiceRequest> for RateLimitMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let limiter = self.limiter.clone();
        let enabled = self.enabled;
        let is_strict = Self::is_strict_path(req.path());
        let is_email = Self::is_email_path(req.path());

        let direct_ip = req
            .connection_info()
            .realip_remote_addr()
            .unwrap_or("unknown")
            .to_string();

        let ip = if limiter.is_trusted_proxy(&direct_ip) {
            get_real_ip_from_request(req.request())
        } else {
            crate::utils::normalize_ipv4_address(&direct_ip)
        };

        let user_id = if is_strict {
            None
        } else {
            extract_user_id_from_token(&req)
        };

        let path = req.path().to_string();
        let method = req.method().to_string();

        if enabled {
            if let Err(e) = limiter.check_rate_limit(&ip, user_id.as_deref(), is_strict, is_email) {
                tracing::warn!(
                    "请求被速率限制拦截: method={}, path={}, ip={}, user_id={}, is_strict={}, is_email={}, retry_after={}s",
                    method, path, ip, user_id.as_deref().unwrap_or("-"), is_strict, is_email, e.retry_after
                );
                return Box::pin(async move { Err(e.into()) });
            }
        }

        let fut = self.service.call(req);

        Box::pin(async move {
            let res = fut.await?;
            Ok(res.map_into_left_body())
        })
    }
}

pub fn start_cleanup_task(limiter: RateLimiter, mut shutdown_rx: tokio::sync::broadcast::Receiver<()>) {
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
