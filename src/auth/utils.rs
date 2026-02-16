use crate::config::{Config, parse_duration};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use lru::LruCache;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

// 定义缓存值类型
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
    // 添加令牌验证缓存，使用线程安全的 LruCache
    token_cache: Arc<RwLock<LruCache<String, TokenCacheValue>>>,
}

impl JwtUtils {
    pub fn new(config: &Config) -> Self {
        let access_token_expiry = parse_duration(&config.jwt.access_token_expiry).unwrap_or(3600); // 默认1小时

        let refresh_token_expiry =
            parse_duration(&config.jwt.refresh_token_expiry).unwrap_or(604800); // 默认7天

        let algorithm = Algorithm::HS256;

        // 优先从环境变量读取密钥
        let secret = Self::get_jwt_secret(&config.jwt.secret);

        // 验证密钥强度
        Self::validate_secret_strength(&secret);

        Self {
            config: JwtConfig {
                secret: secret.clone(),
                access_token_expiry,
                refresh_token_expiry,
                algorithm,
                issuer: "ipma-server".to_string(),
                audience: "ipma-client".to_string(),
                leeway: 30, // 默认30秒时间误差容忍
            },
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            // 初始化令牌缓存，容量为1000
            token_cache: Arc::new(RwLock::new(LruCache::new(
                std::num::NonZeroUsize::new(1000).unwrap(),
            ))),
        }
    }

    // 从环境变量或配置文件获取JWT密钥
    fn get_jwt_secret(config_secret: &str) -> String {
        // 优先从环境变量读取
        if let Ok(env_secret) = std::env::var("IPMA_JWT_SECRET")
            && !env_secret.is_empty()
        {
            info!("从环境变量获取JWT密钥");
            return env_secret;
        }

        // 如果环境变量未设置，使用配置文件中的密钥
        if config_secret.is_empty() {
            // 如果配置文件中也没有密钥，生成一个临时密钥（仅用于开发）
            let temp_secret = Self::generate_secure_secret();
            error!("JWT密钥未配置，生成临时密钥（仅用于开发环境）");
            temp_secret
        } else {
            config_secret.to_string()
        }
    }

    // 验证密钥强度
    fn validate_secret_strength(secret: &str) {
        if secret.len() < 32 {
            warn!("JWT密钥长度不足32个字符，建议使用更长的密钥以提高安全性");
        }

        // 检查密钥复杂度
        let has_uppercase = secret.chars().any(|c| c.is_uppercase());
        let has_lowercase = secret.chars().any(|c| c.is_lowercase());
        let has_digit = secret.chars().any(|c| c.is_ascii_digit());
        let has_special = secret.chars().any(|c| !c.is_alphanumeric());

        if !has_uppercase || !has_lowercase || !has_digit || !has_special {
            warn!("JWT密钥复杂度不足，建议包含大小写字母、数字和特殊字符");
        }
    }

    // 生成安全的随机密钥（用于HMAC算法）
    pub fn generate_secure_secret() -> String {
        let mut bytes = [0u8; 64]; // 64字节 = 512位
        let mut rng = rand::rng();
        rng.fill(&mut bytes);
        STANDARD.encode(bytes)
    }

    // 验证令牌签名
    pub fn verify_signature(&self, token: &str) -> bool {
        self.validate_token(token).is_ok()
    }

    // 检查令牌是否即将过期（例如，在5分钟内）
    pub fn is_token_about_to_expire(&self, claims: &JwtClaims) -> bool {
        let now = Utc::now().timestamp() as usize;
        let exp = claims.exp;
        exp.saturating_sub(now) < 300 // 5分钟
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
            device_fingerprint: device_fingerprint.map(|s| s.to_string()),
            ip_address: ip_address.map(|s| s.to_string()),
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
            device_fingerprint: device_fingerprint.map(|s| s.to_string()),
            ip_address: ip_address.map(|s| s.to_string()),
        };

        encode(
            &Header::new(self.config.algorithm),
            &claims,
            &self.encoding_key,
        )
    }

    // 生成设备指纹
    pub fn generate_device_fingerprint(user_agent: &str, ip_address: &str) -> String {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(user_agent);
        hasher.update(ip_address);

        let hash = hasher.finalize();
        format!("{:x}", hash)
    }

    // 验证设备指纹
    pub fn validate_device_fingerprint(
        &self,
        claims: &JwtClaims,
        device_fingerprint: &str,
        ip_address: &str,
    ) -> bool {
        // 检查设备指纹是否匹配
        if let Some(ref claim_fingerprint) = claims.device_fingerprint
            && claim_fingerprint != device_fingerprint
        {
            return false;
        }

        // 检查 IP 地址是否匹配
        if let Some(ref claim_ip) = claims.ip_address
            && claim_ip != ip_address
        {
            return false;
        }

        true
    }

    // 验证并解码令牌
    pub fn validate_token(&self, token: &str) -> Result<JwtClaims, jsonwebtoken::errors::Error> {
        // 1. 尝试从缓存中获取
        if let Ok(mut cache) = self.token_cache.write()
            && let Some((claims, cached_at)) = cache.get(token)
        {
            // 检查缓存是否过期（缓存有效期为1分钟）
            let cache_expiry = *cached_at + Duration::seconds(60);
            if Utc::now() < cache_expiry {
                // 检查令牌是否已过期
                let now = Utc::now().timestamp() as usize;
                if claims.exp > now {
                    return Ok(claims.clone());
                }
            }
            // 如果缓存过期或令牌过期，从缓存中移除
            cache.pop(token);
        }

        // 2. 缓存未命中，执行正常验证
        let mut validation = Validation::new(self.config.algorithm);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.config.audience]);
        validation.leeway = self.config.leeway; // 添加时间误差容忍
        validation.validate_exp = true;
        validation.validate_nbf = false; // 不验证not before时间

        let decoded = decode::<JwtClaims>(token, &self.decoding_key, &validation)?;

        // 3. 将验证结果存入缓存
        if let Ok(mut cache) = self.token_cache.write() {
            cache.put(token.to_string(), (decoded.claims.clone(), Utc::now()));
        }

        Ok(decoded.claims)
    }

    // 解码令牌但不验证过期时间（用于刷新令牌等场景）
    pub fn decode_token_without_expiration(
        &self,
        token: &str,
    ) -> Result<JwtClaims, jsonwebtoken::errors::Error> {
        let mut validation = Validation::new(self.config.algorithm);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.config.audience]);
        validation.validate_exp = false; // 不验证过期时间

        let decoded = decode::<JwtClaims>(token, &self.decoding_key, &validation)?;

        Ok(decoded.claims)
    }

    // 获取访问令牌过期时间
    pub fn get_access_token_expiry(&self) -> u64 {
        self.config.access_token_expiry
    }

    // 获取刷新令牌过期时间
    pub fn get_refresh_token_expiry(&self) -> u64 {
        self.config.refresh_token_expiry
    }

    // 获取基于 remember_me 的实际刷新令牌过期时间
    pub fn get_actual_refresh_token_expiry(&self, remember_me: bool) -> u64 {
        if remember_me {
            self.config.refresh_token_expiry
        } else {
            86400 // 未勾选保持登录，24 小时
        }
    }
}

// 从请求中提取令牌（优先从 Cookie，其次从 Authorization 头）
pub fn extract_token_from_request(req: &actix_web::HttpRequest) -> Option<String> {
    // 优先从 Cookie 中获取 access_token
    if let Some(cookie) = req.cookie("access_token") {
        let token = cookie.value();
        if !token.is_empty() {
            return Some(token.to_string());
        }
    }
    
    // 回退到 Authorization 头（用于向后兼容或 API 调用）
    req.headers()
        .get("Authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|auth_str| {
            if auth_str.starts_with("Bearer ") {
                Some(auth_str.strip_prefix("Bearer ").unwrap_or("").to_string())
            } else {
                None
            }
        })
}

// 从请求中获取客户端信息
pub fn get_client_info(req: &actix_web::HttpRequest) -> (String, String) {
    let ip_address = req
        .connection_info()
        .realip_remote_addr()
        .unwrap_or("unknown")
        .to_string();

    let user_agent = req
        .headers()
        .get(actix_web::http::header::USER_AGENT)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    (ip_address, user_agent)
}

// 从ServiceRequest中提取令牌
pub fn extract_token_from_service_request(req: &actix_web::dev::ServiceRequest) -> Option<String> {
    extract_token_from_request(req.request())
}

// 从ServiceRequest中获取客户端信息
pub fn get_client_info_from_service_request(
    req: &actix_web::dev::ServiceRequest,
) -> (String, String) {
    get_client_info(req.request())
}

// 检查令牌是否被撤销
pub async fn is_token_revoked(pool: &sqlx::PgPool, token: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM revoked_tokens WHERE jti = $1 AND expires_at > NOW())",
    )
    .bind(token)
    .fetch_optional(pool)
    .await;

    match result {
        Ok(Some(revoked)) => Ok(revoked),
        _ => Ok(false),
    }
}

// 撤销令牌
pub async fn revoke_token(
    pool: &sqlx::PgPool,
    token_jti: &str,
    user_id: &Uuid,
    expiry: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO revoked_tokens (jti, user_id, expires_at, created_at)
         VALUES ($1, $2, $3, NOW())
         ON CONFLICT (jti) DO NOTHING",
    )
    .bind(token_jti)
    .bind(user_id)
    .bind(expiry)
    .execute(pool)
    .await
    .map(|_| ())
}

// 记录令牌使用情况
pub async fn record_token_usage(
    pool: &sqlx::PgPool,
    token_jti: &str,
    user_id: &Uuid,
    ip_address: &str,
    user_agent: &str,
    request_path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO token_usage_logs (jti, user_id, ip_address, user_agent, request_path, created_at)
         VALUES ($1, $2, $3, $4, $5, NOW())"
    )
    .bind(token_jti)
    .bind(user_id)
    .bind(ip_address)
    .bind(user_agent)
    .bind(request_path)
    .execute(pool)
    .await
    .map(|_| ())
}

// 检查令牌使用频率限制
pub async fn check_token_usage_limit(
    pool: &sqlx::PgPool,
    token_jti: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM token_usage_logs WHERE jti = $1 AND created_at > NOW() - INTERVAL '1 minute'"
    )
    .bind(token_jti)
    .fetch_optional(pool)
    .await;

    match result {
        Ok(Some(count)) => Ok(count > 100), // 每分钟最多100次请求
        _ => Ok(false),
    }
}
