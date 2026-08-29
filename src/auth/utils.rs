//! 认证辅助工具。

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{Duration, Utc};
use dashmap::DashMap;
use ipma_common::config::{Config, parse_duration};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};
use uuid::Uuid;

use ipma_common::{AppError, msg};
use ipma_common::{log_error, log_info, log_warn};

static GLOBAL_TOKEN_CACHE: LazyLock<Arc<DashMap<String, TokenCacheValue>>> =
    LazyLock::new(|| Arc::new(DashMap::new()));

type TokenCacheValue = (JwtClaims, chrono::DateTime<Utc>);

// 增强的 JWT 声明结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JwtClaims {
    pub sub: String,                        // 用户 ID
    pub username: String,                   // 用户名
    pub role: String,                       // 用户角色
    pub exp: usize,                         // 过期时间戳（秒）
    pub iat: usize,                         // 签发时间戳（秒）
    pub iss: String,                        // 签发者
    pub jti: String,                        // JWT ID，用于撤销令牌
    pub aud: String,                        // 受众
    pub token_type: String,                 // 令牌类型："access" 或 "refresh"
    pub device_fingerprint: Option<String>, // 设备指纹
    pub ip_address: Option<String>,         // IP 地址
}

// JWT 配置结构体
#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub secret: String,
    pub access_token_expiry: u64,
    pub refresh_token_expiry: u64,
    pub algorithm: Algorithm,
    pub issuer: String,
    pub audience: String,
    pub leeway: u64, // 时间误差容忍（秒）
}

// JWT 工具结构体
#[derive(Debug, Clone)]
pub struct JwtUtils {
    config: JwtConfig,
    decoding_key: DecodingKey,
    encoding_key: EncodingKey,
    token_cache: Arc<DashMap<String, TokenCacheValue>>,
}

impl JwtUtils {
    pub fn new(config: &Config) -> Result<Self, String> {
        // 时长格式错误属配置错误：启动时直接失败，避免静默回退默认时长
        let access_token_expiry = parse_duration(&config.jwt.access_token_expiry)
            .map_err(|e| format!("access_token_expiry 配置无效: {e}"))?;

        let refresh_token_expiry = parse_duration(&config.jwt.refresh_token_expiry)
            .map_err(|e| format!("refresh_token_expiry 配置无效: {e}"))?;

        let algorithm = Algorithm::HS256;

        let secret = Self::get_jwt_secret(&config.jwt.secret);

        Self::validate_secret_strength(&secret).map_err(|e| {
            log_error!("log.auth.jwt_secret_invalid", detail = e);
            e
        })?;

        Ok(Self {
            config: JwtConfig {
                secret: secret.clone(),
                access_token_expiry,
                refresh_token_expiry,
                algorithm,
                issuer: "ipma-server".to_string(),
                audience: "ipma-client".to_string(),
                leeway: 30,
            },
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            token_cache: GLOBAL_TOKEN_CACHE.clone(),
        })
    }

    // 从环境变量或配置文件获取JWT密钥
    fn get_jwt_secret(config_secret: &str) -> String {
        // 优先从环境变量读取
        if let Ok(env_secret) = std::env::var("IPMA_JWT_SECRET")
            && !env_secret.is_empty()
        {
            log_info!("log.auth.jwt_secret_from_env");
            return env_secret;
        }

        // 如果环境变量未设置，使用配置文件中的密钥
        if config_secret.is_empty() {
            // 如果配置文件中也没有密钥，生成一个临时密钥（仅用于开发）
            let temp_secret = Self::generate_secure_secret();
            log_error!("log.auth.jwt_secret_missing_temp");
            temp_secret
        } else {
            config_secret.to_string()
        }
    }

    // 验证密钥强度
    fn validate_secret_strength(secret: &str) -> Result<(), String> {
        if secret.len() < 32 {
            return Err(format!(
                "JWT密钥长度不足32个字符，请配置更强的密钥。当前长度: {}",
                secret.len()
            ));
        }

        let has_uppercase = secret.chars().any(char::is_uppercase);
        let has_lowercase = secret.chars().any(char::is_lowercase);
        let has_digit = secret.chars().any(|c| c.is_ascii_digit());
        let has_special = secret.chars().any(|c| !c.is_alphanumeric());

        if !has_uppercase || !has_lowercase || !has_digit || !has_special {
            log_warn!("log.auth.jwt_secret_weak");
        }

        Ok(())
    }

    // 生成安全的随机密钥（用于HMAC算法）
    #[must_use]
    pub fn generate_secure_secret() -> String {
        let mut bytes = [0u8; 64]; // 64字节 = 512位
        let mut rng = rand::rng();
        rng.fill(&mut bytes);
        STANDARD.encode(bytes)
    }

    // 生成访问令牌
    pub fn generate_access_token(
        &self,
        user_id: &Uuid,
        username: &str,
        role: &str,
        device_fingerprint: Option<&str>,
        ip_address: Option<&str>,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let now = Utc::now();
        let exp =
            (now + Duration::seconds(self.config.access_token_expiry as i64)).timestamp() as usize;
        let iat = now.timestamp() as usize;
        let jti = Uuid::new_v4().to_string();

        let claims = JwtClaims {
            sub: user_id.to_string(),
            username: username.to_string(),
            role: role.to_string(),
            exp,
            iat,
            iss: self.config.issuer.clone(),
            jti,
            aud: self.config.audience.clone(),
            token_type: "access".to_string(),
            device_fingerprint: device_fingerprint.map(std::string::ToString::to_string),
            ip_address: ip_address.map(std::string::ToString::to_string),
        };

        encode(
            &Header::new(self.config.algorithm),
            &claims,
            &self.encoding_key,
        )
    }

    // 生成刷新令牌
    pub fn generate_refresh_token(
        &self,
        user_id: &Uuid,
        username: &str,
        role: &str,
        device_fingerprint: Option<&str>,
        ip_address: Option<&str>,
        remember_me: bool,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let now = Utc::now();
        // 如果勾选保持登录，使用配置的 Refresh Token 过期时间（通常较长，如7天）
        // 如果未勾选，使用较短的时间（例如24小时），或者与 Access Token 相同
        let expiry_seconds = if remember_me {
            self.config.refresh_token_expiry
        } else {
            // 未勾选保持登录，Refresh Token 有效期设为 24 小时
            86400
        };

        let exp = (now + Duration::seconds(expiry_seconds as i64)).timestamp() as usize;
        let iat = now.timestamp() as usize;
        let jti = Uuid::new_v4().to_string();

        let claims = JwtClaims {
            sub: user_id.to_string(),
            username: username.to_string(),
            role: role.to_string(),
            exp,
            iat,
            iss: self.config.issuer.clone(),
            jti,
            aud: self.config.audience.clone(),
            token_type: "refresh".to_string(),
            device_fingerprint: device_fingerprint.map(std::string::ToString::to_string),
            ip_address: ip_address.map(std::string::ToString::to_string),
        };

        encode(
            &Header::new(self.config.algorithm),
            &claims,
            &self.encoding_key,
        )
    }

    // 生成设备指纹
    #[must_use]
    pub fn generate_device_fingerprint(user_agent: &str, ip_address: &str) -> String {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(user_agent);
        hasher.update(ip_address);

        let hash = hasher.finalize();
        let hash_bytes: &[u8] = hash.as_ref();
        hex::encode(hash_bytes)
    }

    pub fn validate_token(&self, token: &str) -> Result<JwtClaims, jsonwebtoken::errors::Error> {
        if let Some(entry) = self.token_cache.get(token) {
            let (claims, cached_at) = entry.value();
            let cache_expiry = *cached_at + Duration::seconds(60);
            if Utc::now() < cache_expiry {
                let now = Utc::now().timestamp() as usize;
                if claims.exp > now {
                    return Ok(claims.clone());
                }
            }
        }

        let mut validation = Validation::new(self.config.algorithm);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.config.audience]);
        validation.leeway = self.config.leeway;
        validation.validate_exp = true;
        validation.validate_nbf = false;

        let decoded = decode::<JwtClaims>(token, &self.decoding_key, &validation)?;

        self.token_cache
            .insert(token.to_string(), (decoded.claims.clone(), Utc::now()));

        Ok(decoded.claims)
    }

    pub fn start_cache_cleanup_task(&self, mut shutdown_rx: tokio::sync::broadcast::Receiver<()>) {
        let cache = self.token_cache.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(120));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let now = Utc::now();
                        cache.retain(|_, (claims, cached_at)| {
                            let cache_expiry = *cached_at + Duration::seconds(120);
                            now < cache_expiry && claims.exp > now.timestamp() as usize
                        });
                    }
                    _ = shutdown_rx.recv() => {
                        log_info!("log.auth.jwt_cache_cleanup_stopped");
                        break;
                    }
                }
            }
        });
    }

    // 获取访问令牌过期时间
    #[must_use]
    pub const fn get_access_token_expiry(&self) -> u64 {
        self.config.access_token_expiry
    }

    // 获取基于 remember_me 的实际刷新令牌过期时间
    #[must_use]
    pub const fn get_actual_refresh_token_expiry(&self, remember_me: bool) -> u64 {
        if remember_me {
            self.config.refresh_token_expiry
        } else {
            86400 // 未勾选保持登录，24 小时
        }
    }
}

// 从 axum 请求 parts 中提取令牌（优先从 Cookie，其次从 Authorization 头）
#[must_use]
pub fn extract_token_from_parts(parts: &axum::http::request::Parts) -> Option<String> {
    // 优先从 Cookie 中获取 access_token
    if let Some(token) = extract_cookie_from_parts(parts, "access_token")
        && !token.is_empty()
    {
        return Some(token);
    }

    // 回退到 Authorization 头（用于向后兼容或 API 调用）
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|auth_str| {
            if auth_str.starts_with("Bearer ") {
                // "Bearer "（空令牌）视为未携带令牌，而非返回空串
                //（A-11：空串令牌应走未认证路径而不是进入验签）
                auth_str
                    .strip_prefix("Bearer ")
                    .filter(|token| !token.is_empty())
                    .map(std::string::ToString::to_string)
            } else {
                None
            }
        })
}

/// 从 axum 请求 parts 中按名称提取 Cookie 值
#[must_use]
pub fn extract_cookie_from_parts(parts: &axum::http::request::Parts, name: &str) -> Option<String> {
    let cookie_header = parts.headers.get(axum::http::header::COOKIE)?;
    let cookie_str = cookie_header.to_str().ok()?;
    let prefix = format!("{name}=");
    for cookie_pair in cookie_str.split(';') {
        let pair = cookie_pair.trim();
        if let Some(rest) = pair.strip_prefix(&prefix) {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

// 从 axum 请求 parts 中获取客户端信息（IP、User-Agent）
pub fn get_client_info_from_parts(parts: &axum::http::request::Parts) -> (String, String) {
    let ip_address = ipma_common::net::get_real_ip_from_parts(parts);
    let user_agent = ipma_common::net::get_user_agent_from_parts(parts);
    (ip_address, user_agent)
}

// 异步密码哈希函数，使用 spawn_blocking 避免阻塞 tokio 线程
pub async fn hash_password(password: &str) -> Result<String, AppError> {
    let password = password.to_string();
    tokio::task::spawn_blocking(move || bcrypt::hash(&password, bcrypt::DEFAULT_COST))
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.auth.password_hash_task_failed").with("error", e))
        })?
        .map_err(|err| {
            AppError::Internal(msg("server.auth.password_hash_failed").with("error", err))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ipma_common::config::{
        Config, DatabaseConfig, InitConfig, JwtConfig as FileJwtConfig, ListenConfig,
        RateLimitConfig, ServerConfig, SnmpConfig,
    };

    /// 测试专用密钥（满足 32 字符下限）
    const TEST_SECRET: &str = "unit-test-jwt-secret-0123456789abcdef";

    /// 构造仅填充必要字段的测试配置（数据库等字段不参与 JWT 逻辑）
    fn make_test_config(secret: &str) -> Config {
        Config {
            database: DatabaseConfig {
                host: "127.0.0.1".to_string(),
                port: 5432,
                database: "ipma_test".to_string(),
                username: "ipma".to_string(),
                password: String::new(),
                max_connections: 1,
                min_connections: 1,
                acquire_timeout_secs: 1,
                idle_timeout_secs: 1,
                max_lifetime_secs: 1,
                query_timeout_secs: 1,
                health_check_interval_secs: 1,
            },
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                host_ipv6: None,
                public_url: "http://127.0.0.1".to_string(),
                session_timeout: None,
                page_timeout: None,
                cors_allowed_origins: Vec::new(),
                allow_localhost_cors: false,
                listen: ListenConfig::default(),
            },
            jwt: FileJwtConfig {
                secret: secret.to_string(),
                access_token_expiry: "15m".to_string(),
                refresh_token_expiry: "7d".to_string(),
            },
            init: InitConfig { enabled: false },
            i18n: None,
            rate_limit: RateLimitConfig::default(),
            snmp: SnmpConfig::default(),
        }
    }

    /// 构造基于测试密钥的 JwtUtils 实例
    fn make_jwt_utils() -> JwtUtils {
        let utils = JwtUtils::new(&make_test_config(TEST_SECRET));
        assert!(utils.is_ok(), "强密钥下 JwtUtils 构造应成功");
        utils.unwrap_or_else(|e| panic!("JwtUtils 构造失败: {e}"))
    }

    /// 用指定密钥与声明手工签发一个 token（绕过配置的过期时长）
    fn sign_token(secret: &str, claims: &JwtClaims) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap_or_else(|e| panic!("测试 token 签发失败: {e}"))
    }

    /// 构造一份自定义时间的声明（签发者/受众与生产一致）
    fn make_claims(exp: usize, iat: usize) -> JwtClaims {
        JwtClaims {
            sub: Uuid::new_v4().to_string(),
            username: "admin".to_string(),
            role: "admin".to_string(),
            exp,
            iat,
            iss: "ipma-server".to_string(),
            jti: Uuid::new_v4().to_string(),
            aud: "ipma-client".to_string(),
            token_type: "access".to_string(),
            device_fingerprint: None,
            ip_address: None,
        }
    }

    // ==================== 密钥校验 ====================

    #[test]
    fn test_new_rejects_secret_shorter_than_32_chars() {
        // 31 字符：低于下限应拒绝
        let short = "a".repeat(31);
        let result = JwtUtils::new(&make_test_config(&short));
        assert!(result.is_err(), "31 字符密钥应被拒绝");
        let err = result.err().unwrap_or_default();
        assert!(err.contains("32"), "错误信息应说明长度下限: {err}");
    }

    #[test]
    fn test_validate_secret_strength_boundary() {
        // 边界：31 字符拒绝、32 字符通过（字节长度）
        assert!(JwtUtils::validate_secret_strength(&"x".repeat(31)).is_err());
        assert!(JwtUtils::validate_secret_strength(&"x".repeat(32)).is_ok());
        // 空密钥拒绝
        assert!(JwtUtils::validate_secret_strength("").is_err());
        // 多字节字符按字节计数：10 个中文 = 30 字节，低于下限应拒绝
        assert!(JwtUtils::validate_secret_strength("一二三四五六七八九十").is_err());
    }

    #[test]
    fn test_generate_secure_secret_is_random_and_512bit() {
        let secret1 = JwtUtils::generate_secure_secret();
        let secret2 = JwtUtils::generate_secure_secret();
        assert_ne!(secret1, secret2, "两次生成的密钥不应相同");
        // 64 字节随机数经 Base64 编码后固定 88 字符（512 位熵）
        assert_eq!(secret1.len(), 88, "Base64(64B) 长度应为 88");
        let decoded = STANDARD
            .decode(&secret1)
            .unwrap_or_else(|e| panic!("生成的密钥应为合法 Base64: {e}"));
        assert_eq!(decoded.len(), 64, "解码后应为 64 字节");
    }

    // ==================== 令牌签发与验签往返 ====================

    #[test]
    fn test_access_token_roundtrip_claims() {
        let utils = make_jwt_utils();
        let user_id = Uuid::new_v4();
        let token = utils
            .generate_access_token(
                &user_id,
                "alice",
                "user",
                Some("fp-123"),
                Some("192.168.1.10"),
            )
            .unwrap_or_else(|e| panic!("访问令牌签发失败: {e}"));

        let claims = utils
            .validate_token(&token)
            .unwrap_or_else(|e| panic!("访问令牌验签失败: {e}"));
        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.username, "alice");
        assert_eq!(claims.role, "user");
        assert_eq!(claims.iss, "ipma-server");
        assert_eq!(claims.aud, "ipma-client");
        assert_eq!(claims.token_type, "access");
        assert_eq!(claims.device_fingerprint.as_deref(), Some("fp-123"));
        assert_eq!(claims.ip_address.as_deref(), Some("192.168.1.10"));
        // jti 应为合法 UUID 且 exp 晚于 iat（15m 配置）
        assert!(Uuid::parse_str(&claims.jti).is_ok(), "jti 应为合法 UUID");
        assert!(claims.exp > claims.iat, "exp 应晚于 iat");
        assert!(claims.exp <= claims.iat + 900, "exp 不应超过配置的 15 分钟");
    }

    #[test]
    fn test_refresh_token_roundtrip_claims() {
        let utils = make_jwt_utils();
        let user_id = Uuid::new_v4();
        let token = utils
            .generate_refresh_token(&user_id, "bob", "admin", None, None, true)
            .unwrap_or_else(|e| panic!("刷新令牌签发失败: {e}"));

        let claims = utils
            .validate_token(&token)
            .unwrap_or_else(|e| panic!("刷新令牌验签失败: {e}"));
        assert_eq!(claims.token_type, "refresh");
        assert_eq!(claims.username, "bob");
        assert_eq!(claims.role, "admin");
        assert_eq!(claims.device_fingerprint, None);
        assert_eq!(claims.ip_address, None);
        // remember_me=true 使用配置的 7 天过期
        assert!(claims.exp <= claims.iat + 7 * 86400);
    }

    #[test]
    fn test_refresh_token_expiry_without_remember_me() {
        let utils = make_jwt_utils();
        assert_eq!(utils.get_actual_refresh_token_expiry(true), 604800);
        // 未勾选保持登录固定 24 小时
        assert_eq!(utils.get_actual_refresh_token_expiry(false), 86400);
        // 访问令牌过期时间来自配置（15m = 900 秒）
        assert_eq!(utils.get_access_token_expiry(), 900);
    }

    #[test]
    fn test_expired_token_rejected() {
        let utils = make_jwt_utils();
        let now = Utc::now().timestamp() as usize;
        // 过期超过 leeway(30s)：1 小时前过期应被拒绝
        let claims = make_claims(now - 3600, now - 7200);
        let token = sign_token(TEST_SECRET, &claims);
        let result = utils.validate_token(&token);
        assert!(result.is_err(), "过期 token 应被拒绝");
        assert!(
            matches!(
                result.err().map(|e| e.kind().clone()),
                Some(jsonwebtoken::errors::ErrorKind::ExpiredSignature)
            ),
            "错误种类应为 ExpiredSignature"
        );
    }

    #[test]
    fn test_token_within_leeway_accepted() {
        let utils = make_jwt_utils();
        let now = Utc::now().timestamp() as usize;
        // 过期时间在 30 秒容差内应放行
        let claims = make_claims(now - 5, now - 60);
        let token = sign_token(TEST_SECRET, &claims);
        assert!(utils.validate_token(&token).is_ok(), "容差内 token 应通过");
    }

    #[test]
    fn test_wrong_signature_rejected() {
        let utils = make_jwt_utils();
        let now = Utc::now().timestamp() as usize;
        let claims = make_claims(now + 600, now);
        // 用另一密钥签发，验签必须失败
        let token = sign_token("another-secret-key-0123456789abcdef!!", &claims);
        let result = utils.validate_token(&token);
        assert!(result.is_err(), "错误签名的 token 应被拒绝");
        assert!(
            matches!(
                result.err().map(|e| e.kind().clone()),
                Some(jsonwebtoken::errors::ErrorKind::InvalidSignature)
            ),
            "错误种类应为 InvalidSignature"
        );
    }

    #[test]
    fn test_wrong_issuer_rejected() {
        let utils = make_jwt_utils();
        let now = Utc::now().timestamp() as usize;
        let mut claims = make_claims(now + 600, now);
        claims.iss = "evil-issuer".to_string();
        let token = sign_token(TEST_SECRET, &claims);
        assert!(utils.validate_token(&token).is_err(), "签发者不符应被拒绝");
    }

    #[test]
    fn test_wrong_audience_rejected() {
        let utils = make_jwt_utils();
        let now = Utc::now().timestamp() as usize;
        let mut claims = make_claims(now + 600, now);
        claims.aud = "evil-audience".to_string();
        let token = sign_token(TEST_SECRET, &claims);
        assert!(utils.validate_token(&token).is_err(), "受众不符应被拒绝");
    }

    #[test]
    fn test_malformed_token_rejected() {
        let utils = make_jwt_utils();
        assert!(
            utils.validate_token("not-a-jwt").is_err(),
            "非 JWT 文本应被拒绝"
        );
        assert!(utils.validate_token("").is_err(), "空 token 应被拒绝");
        // 三段式但负载不是合法 JSON
        assert!(
            utils.validate_token("a.b.c").is_err(),
            "损坏 token 应被拒绝"
        );
    }

    #[test]
    fn test_claims_serde_roundtrip() {
        // 声明结构体 JSON 序列化/反序列化往返应保持字段不变
        let now = Utc::now().timestamp() as usize;
        let claims = JwtClaims {
            sub: "00000000-0000-0000-0000-000000000001".to_string(),
            username: "中文用户".to_string(),
            role: "user".to_string(),
            exp: now,
            iat: now - 1,
            iss: "ipma-server".to_string(),
            jti: "00000000-0000-0000-0000-000000000002".to_string(),
            aud: "ipma-client".to_string(),
            token_type: "refresh".to_string(),
            device_fingerprint: Some("指纹".to_string()),
            ip_address: Some("2001:db8::1".to_string()),
        };
        let json = serde_json::to_string(&claims).unwrap_or_else(|e| panic!("序列化失败: {e}"));
        let parsed: JwtClaims =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(parsed.sub, claims.sub);
        assert_eq!(parsed.username, claims.username);
        assert_eq!(parsed.device_fingerprint, claims.device_fingerprint);
        assert_eq!(parsed.ip_address, claims.ip_address);
        assert_eq!(parsed.token_type, claims.token_type);
    }

    // ==================== 设备指纹 ====================

    #[test]
    fn test_device_fingerprint_deterministic_and_known_vector() {
        // 相同输入应得到相同指纹（SHA-256 已知向量）
        let fp1 = JwtUtils::generate_device_fingerprint("Mozilla/5.0", "192.168.1.1");
        let fp2 = JwtUtils::generate_device_fingerprint("Mozilla/5.0", "192.168.1.1");
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 64, "SHA-256 十六进制应为 64 字符");
        assert!(
            fp1.chars().all(|c| c.is_ascii_hexdigit()),
            "指纹应为十六进制字符串"
        );
        assert_eq!(
            fp1,
            "7199a92e8178e8f9c1b4935e8f0681ff7f2d038b56d3f1617d276e054bbe115d"
        );
    }

    #[test]
    fn test_device_fingerprint_input_order_semantics() {
        // 指纹按 user_agent 与 ip_address 字符串拼接计算（UA 在前）
        let base = JwtUtils::generate_device_fingerprint("ua-ip", "");
        // 已知向量：sha256("ua-ip")
        assert_eq!(
            base,
            "e19ee49f350b9c9deb19dcb1f76c92b1612dc5661e960a0846eeb8518039a708"
        );
        // 拼接等价：("a","bc") 与 ("ab","c") 均哈希 "abc"，指纹相同
        assert_eq!(
            JwtUtils::generate_device_fingerprint("a", "bc"),
            JwtUtils::generate_device_fingerprint("ab", "c")
        );
        // 任一输入变化指纹即变化
        let changed_ip = JwtUtils::generate_device_fingerprint("ua-ip", "10.0.0.1");
        assert_ne!(base, changed_ip);
        let changed_ua = JwtUtils::generate_device_fingerprint("ua-ip2", "");
        assert_ne!(base, changed_ua);
    }

    // ==================== 请求头/Cookie 提取 ====================

    /// 构造带指定请求头的 parts
    fn make_parts(headers: &[(&str, &str)]) -> axum::http::request::Parts {
        let mut builder = axum::http::Request::builder().uri("/");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let request = builder
            .body(())
            .unwrap_or_else(|e| panic!("构造测试请求失败: {e}"));
        // http 1.x 无 split()，用 into_parts 取 Parts（元组第一位）
        request.into_parts().0
    }

    #[test]
    fn test_extract_token_prefers_cookie_over_bearer() {
        // 同时存在 Cookie 与 Authorization 时优先 Cookie
        let parts = make_parts(&[
            ("Cookie", "access_token=cookie-token; other=1"),
            ("Authorization", "Bearer bearer-token"),
        ]);
        assert_eq!(
            extract_token_from_parts(&parts).as_deref(),
            Some("cookie-token")
        );
    }

    #[test]
    fn test_extract_token_falls_back_to_bearer() {
        let parts = make_parts(&[("Authorization", "Bearer bearer-token")]);
        assert_eq!(
            extract_token_from_parts(&parts).as_deref(),
            Some("bearer-token")
        );
    }

    #[test]
    fn test_extract_token_rejects_non_bearer_scheme_and_missing() {
        // 非 Bearer 前缀不识别
        let parts = make_parts(&[("Authorization", "Basic dXNlcjpwYXNz")]);
        assert_eq!(extract_token_from_parts(&parts), None);
        // 完全缺失认证信息
        let empty = make_parts(&[]);
        assert_eq!(extract_token_from_parts(&empty), None);
        // 空字符串 Bearer：视为未携带令牌返回 None（A-11 修复）
        let blank = make_parts(&[("Authorization", "Bearer ")]);
        assert_eq!(
            extract_token_from_parts(&blank),
            None,
            "空 Bearer 应返回 None 而非空串"
        );
    }

    #[test]
    fn test_extract_token_empty_cookie_falls_back_to_header() {
        // Cookie 中 access_token 为空时应回退到 Authorization
        let parts = make_parts(&[
            ("Cookie", "access_token=; other=2"),
            ("Authorization", "Bearer fallback-token"),
        ]);
        assert_eq!(
            extract_token_from_parts(&parts).as_deref(),
            Some("fallback-token")
        );
    }

    #[test]
    fn test_extract_cookie_multiple_pairs_and_trim() {
        // 多 Cookie 分号分隔 + 键值对外侧空白应正确处理（'=' 两侧不留空格）
        let parts = make_parts(&[("Cookie", "a=1; access_token=tok-xyz ; b=2")]);
        assert_eq!(
            extract_cookie_from_parts(&parts, "access_token").as_deref(),
            Some("tok-xyz")
        );
        // '=' 前带空格（"access_token = v"）按键名不匹配处理，返回 None
        let spaced = make_parts(&[("Cookie", "access_token = v")]);
        assert_eq!(extract_cookie_from_parts(&spaced, "access_token"), None);
        // 前缀匹配但非完整键名（access_token_extra）不应误匹配
        let parts2 = make_parts(&[("Cookie", "access_token_extra=wrong")]);
        assert_eq!(extract_cookie_from_parts(&parts2, "access_token"), None);
        // 值为空返回 None
        let parts3 = make_parts(&[("Cookie", "access_token=")]);
        assert_eq!(extract_cookie_from_parts(&parts3, "access_token"), None);
        // 键不存在
        let parts4 = make_parts(&[("Cookie", "other=1")]);
        assert_eq!(extract_cookie_from_parts(&parts4, "access_token"), None);
        // 无 Cookie 头
        let parts5 = make_parts(&[]);
        assert_eq!(extract_cookie_from_parts(&parts5, "access_token"), None);
    }

    #[test]
    fn test_get_client_info_from_parts() {
        // 无 ConnectInfo 扩展时视为可信代理：读取 X-Real-IP 与 User-Agent
        let parts = make_parts(&[
            ("X-Real-IP", "203.0.113.5"),
            ("User-Agent", "UnitTestAgent/1.0"),
        ]);
        let (ip, ua) = get_client_info_from_parts(&parts);
        assert_eq!(ip, "203.0.113.5");
        assert_eq!(ua, "UnitTestAgent/1.0");

        // 缺失 User-Agent 时回落 "unknown"
        let parts2 = make_parts(&[("X-Real-IP", "203.0.113.6")]);
        let (_, ua2) = get_client_info_from_parts(&parts2);
        assert_eq!(ua2, "unknown");
    }

    // ==================== 密码哈希 ====================

    #[tokio::test]
    async fn test_hash_password_and_verify_roundtrip() {
        let hashed = hash_password("s3cret-p@ss")
            .await
            .unwrap_or_else(|e| panic!("密码哈希失败: {e}"));
        assert!(
            hashed.starts_with("$2"),
            "bcrypt 输出应带版本前缀: {hashed}"
        );
        assert_ne!(hashed, "s3cret-p@ss", "哈希结果不应等于明文");
        // bcrypt 校验通过；错误密码校验失败
        assert!(bcrypt::verify("s3cret-p@ss", &hashed).unwrap_or(false));
        assert!(!bcrypt::verify("wrong-password", &hashed).unwrap_or(true));
        // 相同密码两次哈希（盐随机）结果不同
        let hashed2 = hash_password("s3cret-p@ss")
            .await
            .unwrap_or_else(|e| panic!("密码哈希失败: {e}"));
        assert_ne!(hashed, hashed2);
    }
}
