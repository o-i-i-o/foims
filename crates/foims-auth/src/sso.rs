//! SSO 外部认证：OIDC（OpenID Connect）授权码模式。
//!
//! 流程（符合 OIDC Core 1.0 / OAuth 2.1 最佳实践）：
//! 1. `GET /api/auth/sso/login`：通过 Issuer 发现文档
//!    （`/.well-known/openid-configuration`）构建客户端，生成
//!    state（CSRF 防护）、nonce（重放防护）与 PKCE S256 挑战码，
//!    302 跳转 IdP 授权端点；state→(nonce, verifier) 存于内存（10 分钟）。
//! 2. `GET /api/auth/sso/callback`：一次性消费 state，以授权码 +
//!    PKCE verifier 换取令牌，校验 ID Token 签名/iss/aud/exp/nonce，
//!    按声明查找或自动建户（auth_provider='sso'），签发本系统 Cookie
//!    并跳转前端。
//!
//! 配置存于 `system_configs`（config_type='sso'，client_secret 加密存储）。

use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use validator::Validate;

use crate::login::{
    append_cookie_to_response, create_auth_cookie, find_or_create_external_user,
    issue_external_login_tokens,
};
use crate::meta::RequestMeta;
use crate::provider::AuthProvider;
use foims_common::AppError;
use foims_common::AppJson;
use foims_common::crypto::{decrypt_password_async, encrypt_password_async};
use foims_common::{log_info, msg};
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::reqwest;
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet,
    EndpointSet, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
};

/// 未完成授权的会话状态有效期
const SSO_STATE_TTL: Duration = Duration::from_secs(600);
/// 发现文档缓存有效期
const SSO_METADATA_TTL: Duration = Duration::from_secs(3600);

struct SsoPendingAuth {
    nonce: String,
    pkce_verifier: String,
    created: Instant,
}

type SsoStateStore = DashMap<String, SsoPendingAuth>;
static SSO_STATE_STORE: OnceLock<SsoStateStore> = OnceLock::new();

fn sso_state_store() -> &'static SsoStateStore {
    SSO_STATE_STORE.get_or_init(DashMap::new)
}

struct CachedMetadata {
    metadata: CoreProviderMetadata,
    fetched: Instant,
}

type MetadataCache = DashMap<String, CachedMetadata>;
static SSO_METADATA_CACHE: OnceLock<MetadataCache> = OnceLock::new();

fn metadata_cache() -> &'static MetadataCache {
    SSO_METADATA_CACHE.get_or_init(DashMap::new)
}

static SSO_HTTP_CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();

/// OIDC 出站请求共享 HTTP 客户端（openidconnect 4.0 起由调用方持有）。
///
/// 禁止跟随重定向（防 SSRF），并设置固定超时；构建失败时返回错误
/// 而非 panic（遵循项目禁用 unwrap 的规范）。
fn sso_http_client() -> Result<&'static reqwest::Client, AppError> {
    SSO_HTTP_CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(15))
                .build()
                .ok()
        })
        .as_ref()
        .ok_or_else(|| AppError::Internal(msg("server.sso.http_client_failed")))
}

// ==================== 配置模型 ====================

/// SSO（OIDC）配置（`client_secret` 内存中为明文，落库前加密）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoConfig {
    pub enabled: bool,
    /// IdP 签发者地址，如 https://sso.example.com（不含 /.well-known 路径）
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    /// 回调地址（必填的绝对 URL，如 https://foims.example.com/api/auth/sso/callback）；
    /// 留空时 SSO 登录返回明确配置错误（不基于请求 Host 推导）
    pub redirect_uri: String,
    /// 首次登录自动建户的默认角色（admin/user）
    pub default_role: String,
}

impl Default for SsoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            issuer_url: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            redirect_uri: String::new(),
            default_role: "user".to_string(),
        }
    }
}

/// 读取 SSO 配置；未配置时返回 None（读取失败记录日志）。
pub async fn get_sso_config_from_db(pool: &PgPool) -> Option<SsoConfig> {
    let rows = match sqlx::query(
        "SELECT key, value FROM system_configs WHERE config_type = 'sso' ORDER BY key",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            foims_common::log_error!("log.sso.query_failed", error = e);
            return None;
        }
    };

    let mut config = SsoConfig::default();
    let mut found = false;
    for row in rows {
        let key: String = row.get("key");
        let value: Option<String> = row.get("value");
        let Some(value) = value else { continue };
        found = true;
        match key.as_str() {
            "enabled" => config.enabled = value == "true",
            "issuer_url" => config.issuer_url = value,
            "client_id" => config.client_id = value,
            "client_secret" => {
                // 解密失败视为配置不可用：记日志并整体返回 None，
                // 避免以空 secret 与 IdP 交互掩盖故障
                match decrypt_password_async(value).await {
                    Ok(plain) => config.client_secret = plain,
                    Err(e) => {
                        foims_common::log_error!("log.sso.secret_decrypt_failed", error = e);
                        return None;
                    }
                }
            }
            "redirect_uri" => config.redirect_uri = value,
            "default_role" => config.default_role = value,
            _ => {}
        }
    }

    if found && !config.issuer_url.is_empty() && !config.client_id.is_empty() {
        Some(config)
    } else {
        None
    }
}

/// 保存 SSO 配置（client_secret 加密后逐 key 写入）。
pub async fn save_sso_config_to_db(pool: &PgPool, config: &SsoConfig) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;

    let encrypted = encrypt_password_async(config.client_secret.clone())
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.sso.secret_encrypt_failed").with("error", e))
        })?;

    let entries = [
        ("enabled", config.enabled.to_string()),
        ("issuer_url", config.issuer_url.clone()),
        ("client_id", config.client_id.clone()),
        ("client_secret", encrypted),
        ("redirect_uri", config.redirect_uri.clone()),
        ("default_role", config.default_role.clone()),
    ];

    for (key, value) in entries {
        sqlx::query(
            "INSERT INTO system_configs (config_type, key, value)
                     VALUES ('sso', $1, $2)
                     ON CONFLICT (config_type, key)
                     DO UPDATE SET value = $2, updated_at = NOW()",
        )
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

// ==================== OIDC 客户端构建 ====================

/// 解析回调地址：仅使用显式配置的 redirect_uri。
///
/// 不做任何基于请求 Host / X-Forwarded-Proto 的推导：Host 头是客户端可控
/// 输入，推导出的回调地址会放大 Host 头注入面（缓存污染/密码重置类问题）。
/// 未配置时返回明确的配置错误。
fn resolve_redirect_uri(config: &SsoConfig) -> Result<String, AppError> {
    let uri = config.redirect_uri.trim();
    if uri.is_empty() {
        return Err(AppError::Validation(msg(
            "server.sso.redirect_uri_not_configured",
        )));
    }
    Ok(uri.to_string())
}

/// 发现文档获取（带 1 小时缓存）。
async fn discover_metadata(issuer_url: &str) -> Result<CoreProviderMetadata, AppError> {
    if let Some(entry) = metadata_cache().get(issuer_url)
        && entry.fetched.elapsed() < SSO_METADATA_TTL
    {
        return Ok(entry.metadata.clone());
    }

    let issuer = IssuerUrl::new(issuer_url.to_string())
        .map_err(|e| AppError::Validation(msg("server.sso.issuer_invalid").with("error", e)))?;
    let metadata = CoreProviderMetadata::discover_async(issuer, sso_http_client()?)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.sso.discovery_failed").with("error", format!("{e:?}")))
        })?;

    metadata_cache().insert(
        issuer_url.to_string(),
        CachedMetadata {
            metadata: metadata.clone(),
            fetched: Instant::now(),
        },
    );
    Ok(metadata)
}

/// 构建已设置回调地址的 OIDC 客户端。
///
/// 返回类型携带端点状态标记：授权端点必已就绪（`EndpointSet`），
/// token/userinfo 端点取决于发现文档是否提供（`EndpointMaybeSet`）。
async fn build_oidc_client(
    config: &SsoConfig,
    redirect_uri: &str,
) -> Result<
    CoreClient<
        EndpointSet,
        EndpointNotSet,
        EndpointNotSet,
        EndpointNotSet,
        EndpointMaybeSet,
        EndpointMaybeSet,
    >,
    AppError,
> {
    let metadata = discover_metadata(&config.issuer_url).await?;
    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(config.client_id.clone()),
        Some(ClientSecret::new(config.client_secret.clone())),
    )
    .set_redirect_uri(RedirectUrl::new(redirect_uri.to_string()).map_err(|e| {
        AppError::Validation(msg("server.sso.redirect_uri_invalid").with("error", e))
    })?);
    Ok(client)
}

// ==================== 登录 / 回调 ====================

/// 发起 OIDC 授权码流程：302 跳转 IdP 授权端点。
pub async fn sso_login<P: AuthProvider>(State(state): State<Arc<P>>) -> Response {
    match sso_login_inner(state).await {
        Ok(response) => response,
        Err(error) => sso_error_redirect(error.message().key()),
    }
}

async fn sso_login_inner<P: AuthProvider>(state: Arc<P>) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let config = get_sso_config_from_db(&conn)
        .await
        .ok_or_else(|| AppError::NotFound(msg("server.sso.not_configured")))?;
    if !config.enabled {
        return Err(AppError::Forbidden(msg("server.sso.disabled")));
    }

    let redirect_uri = resolve_redirect_uri(&config)?;
    let client = build_oidc_client(&config, &redirect_uri).await?;

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_state, nonce) = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        // openid scope 由 crate 自动附加，此处仅补充 email/profile
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    // 保存 state → (nonce, verifier)，供回调一次性消费
    let store = sso_state_store();
    store.retain(|_, entry| entry.created.elapsed() < SSO_STATE_TTL);
    store.insert(
        csrf_state.secret().to_string(),
        SsoPendingAuth {
            nonce: nonce.secret().to_string(),
            pkce_verifier: pkce_verifier.secret().to_string(),
            created: Instant::now(),
        },
    );

    log_info!("log.sso.login_redirect", issuer = config.issuer_url);
    Ok(Redirect::to(auth_url.as_str()).into_response())
}

#[derive(Debug, Deserialize)]
pub struct SsoCallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    /// 启用 2FA 的外部账户回调重试时携带的 TOTP 动态码。
    /// 仅 GET 兼容路径经 query 接收（记录迁移告警）；新接入应使用
    /// POST 回调将动态码置于请求体（见 [`sso_callback_post`]）
    totp_code: Option<String>,
}

/// POST 回调请求体：TOTP 动态码经请求体传输，不落入
/// URL query（反代日志/浏览器历史/Referer）
#[derive(Debug, Deserialize)]
pub struct SsoCallbackPostBody {
    /// 启用 2FA 的外部账户回调重试时携带的 TOTP 动态码
    pub totp_code: Option<String>,
}

/// OIDC 回调（GET）：IdP 重定向浏览器回到本端点，换令牌、验 ID Token、
/// 建户/登录、写 Cookie 后跳转前端。
///
/// 动态码仅接受 [`sso_callback_post`] 的请求体传输：query 会落入反向
/// 代理访问日志、浏览器历史与 Referer，本入口对其显式拒绝。
pub async fn sso_callback<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    Query(params): Query<SsoCallbackParams>,
) -> Response {
    // TOTP 动态码不再接受 URL query 传输（会落入反代访问日志/浏览器历史/
    // Referer）：带码 GET 一律按"需要动态码"拒绝，重试改走 POST 请求体
    let totp_code = if params.totp_code.is_some() {
        return sso_error_redirect("server.auth.two_factor_required");
    } else {
        None
    };
    match sso_callback_inner(state, meta, params, totp_code).await {
        Ok(response) => response,
        Err(error) => sso_error_redirect(error.message().key()),
    }
}

/// OIDC 回调（POST）：动态码经 JSON 请求体传输。
///
/// 与 GET 回调共用同一路径的其余参数（code/state/error 仍取 query；
/// IdP 的 302 重定向固定为 GET，POST 路径供前端携带动态码重试时使用）。
pub async fn sso_callback_post<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    Query(params): Query<SsoCallbackParams>,
    AppJson(body): AppJson<SsoCallbackPostBody>,
) -> Response {
    let totp_code = body.totp_code;
    match sso_callback_inner(state, meta, params, totp_code).await {
        Ok(response) => response,
        Err(error) => sso_error_redirect(error.message().key()),
    }
}

async fn sso_callback_inner<P: AuthProvider>(
    state: Arc<P>,
    meta: RequestMeta,
    params: SsoCallbackParams,
    totp_code: Option<String>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    if let Some(error_code) = &params.error {
        foims_common::log_warn!("log.sso.provider_error", error = error_code);
        return Err(AppError::Unauthorized(msg("server.sso.provider_error")));
    }

    let config = get_sso_config_from_db(&conn)
        .await
        .ok_or_else(|| AppError::NotFound(msg("server.sso.not_configured")))?;
    if !config.enabled {
        return Err(AppError::Forbidden(msg("server.sso.disabled")));
    }

    // 一次性消费 state（不存在/过期均拒绝），防 CSRF 与重放
    let state_value = params
        .state
        .ok_or_else(|| AppError::Unauthorized(msg("server.sso.state_invalid")))?;
    let pending = sso_state_store()
        .remove(&state_value)
        .map(|(_, entry)| entry)
        .filter(|entry| entry.created.elapsed() < SSO_STATE_TTL)
        .ok_or_else(|| AppError::Unauthorized(msg("server.sso.state_invalid")))?;

    let code = params
        .code
        .ok_or_else(|| AppError::Unauthorized(msg("server.sso.code_missing")))?;

    let redirect_uri = resolve_redirect_uri(&config)?;
    let client = build_oidc_client(&config, &redirect_uri).await?;

    // 授权码 + PKCE verifier 换取令牌
    // （openidconnect 4.0 中 token 端点为可选配置，缺失时返回配置错误）
    let token_request = client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(|e| {
            AppError::Unauthorized(
                msg("server.sso.token_endpoint_missing").with("error", format!("{e:?}")),
            )
        })?;
    let token_response = token_request
        .set_pkce_verifier(PkceCodeVerifier::new(pending.pkce_verifier.clone()))
        .request_async(sso_http_client()?)
        .await
        .map_err(|e| {
            AppError::Unauthorized(
                msg("server.sso.exchange_failed").with("error", format!("{e:?}")),
            )
        })?;

    // 校验 ID Token：签名（JWKS）、iss、aud、exp 与 nonce
    let id_token = token_response
        .extra_fields()
        .id_token()
        .ok_or_else(|| AppError::Unauthorized(msg("server.sso.id_token_missing")))?
        .clone();
    let nonce = Nonce::new(pending.nonce.clone());
    let claims = id_token
        .claims(&client.id_token_verifier(), &nonce)
        .map_err(|e| {
            AppError::Unauthorized(
                msg("server.sso.id_token_invalid").with("error", format!("{e:?}")),
            )
        })?;

    // 声明取用户名/邮箱：preferred_username → email 本地部分 → sub
    let subject = claims.subject().to_string();
    let email = claims.email().and_then(|e| {
        let value = e.to_string();
        value.contains('@').then_some(value)
    });
    let username = claims
        .preferred_username()
        .map(|name| name.to_string())
        .or_else(|| {
            email
                .as_ref()
                .and_then(|e| e.split('@').next())
                .map(String::from)
        })
        .unwrap_or(subject);

    let external = find_or_create_external_user(
        &conn,
        "sso",
        &username,
        email.as_deref(),
        &config.default_role,
    )
    .await?;

    // 签发前补应用层 fail2ban 双维封禁检查（与其余登录端点口径一致，
    // 唯此路径此前完全绕过封禁）。重定向流无法交互式展示图形验证码，
    // 故不强制 captcha——验证码递进兜底由密码/TOTP 登录路径承担；
    // 封禁中返回 Err 走外层统一 sso_error 跳转
    if crate::app_fail2ban::is_ip_banned(&meta.ip_address) {
        let remaining = crate::app_fail2ban::get_ban_remaining(&meta.ip_address);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }
    if crate::app_fail2ban::is_user_banned(&external.username) {
        let remaining = crate::app_fail2ban::get_user_ban_remaining(&external.username);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }

    // 2FA：外部账户启用 TOTP 时不得直接签发令牌。浏览器重定向流无法在
    // 回调中收集动态码：未携带 totp_code 时按未授权处理（外层统一跳转
    // 登录页并携带 server.auth.two_factor_required 提示）；
    // 携带时校验 TOTP（复用本地 claim/verify 逻辑，含已用码占用）后才签发
    let mut totp_verified = false;
    if external.two_factor_enabled {
        match totp_code
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
        {
            None => {
                return Err(AppError::Unauthorized(msg(
                    "server.auth.two_factor_required",
                )));
            }
            Some(totp_code) => {
                let verified =
                    crate::login::verify_external_totp(&conn, &external, totp_code).await?;
                if !verified {
                    crate::app_fail2ban::record_login_failure(
                        &meta.ip_address,
                        &username,
                        "server.login_log.invalid_2fa_code",
                    );
                    return Err(AppError::Unauthorized(msg("server.auth.code_invalid")));
                }
                totp_verified = true;
            }
        }
    }

    // 签发令牌并写入 Cookie，随后跳转前端首页
    let login_tokens =
        issue_external_login_tokens(&state, &meta, &external, true, totp_verified).await?;
    let mut redirect = Redirect::to("/main.html").into_response();
    let secure = meta.is_secure;
    let access_cookie = create_auth_cookie(
        "access_token",
        &login_tokens.access_token,
        login_tokens.access_token_expiry as i64,
        secure,
    );
    let refresh_cookie = create_auth_cookie(
        "refresh_token",
        &login_tokens.refresh_token,
        login_tokens.refresh_token_expiry as i64,
        secure,
    );
    append_cookie_to_response(&mut redirect, &access_cookie)?;
    append_cookie_to_response(&mut redirect, &refresh_cookie)?;

    Ok(redirect)
}

/// 失败时跳转登录页并携带错误 key，由前端翻译展示。
fn sso_error_redirect(error_key: &str) -> Response {
    Redirect::to(&format!("/index.html?sso_error={error_key}")).into_response()
}

// ==================== 认证方式查询 ====================

/// 公开的认证方式开关：登录页据此显示/隐藏 LDAP 与 SSO 标签。
///
/// 邮箱登录依赖 SMTP：未配置时该入口静默隐藏（模块视为未运行），
/// 避免用户触发必然失败的发码请求。
pub async fn get_auth_methods<P: AuthProvider>(
    State(state): State<Arc<P>>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let ldap_enabled = get_ldap_config_enabled(&conn, "ldap").await;
    let sso_enabled = get_ldap_config_enabled(&conn, "sso").await;
    let email_enabled = crate::smtp::smtp_configured(&conn).await;

    Ok(foims_common::ok_json(
        serde_json::json!({
            "password": true,
            "email": email_enabled,
            "ldap": ldap_enabled,
            "sso": sso_enabled,
        }),
        "server.common.success",
    ))
}

/// 读取指定 config_type 的 enabled 开关（复用 LDAP/SSO 的存储格式）。
async fn get_ldap_config_enabled(pool: &PgPool, config_type: &str) -> bool {
    match sqlx::query_scalar::<_, String>(
        "SELECT value FROM system_configs WHERE config_type = $1 AND key = 'enabled'",
    )
    .bind(config_type)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(value)) => value == "true",
        _ => false,
    }
}

// ==================== 配置管理接口 ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct SsoConfigResponse {
    pub enabled: bool,
    pub issuer_url: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub default_role: String,
    pub has_secret: bool,
}

pub async fn get_sso_config<P: AuthProvider>(
    State(state): State<Arc<P>>,
    _admin: crate::extractor::AdminUser,
) -> Result<Response, AppError> {
    let resp = match get_sso_config_from_db(&state.pool()?.get_conn()).await {
        Some(config) => SsoConfigResponse {
            enabled: config.enabled,
            issuer_url: config.issuer_url,
            client_id: config.client_id,
            redirect_uri: config.redirect_uri,
            default_role: config.default_role,
            has_secret: !config.client_secret.is_empty(),
        },
        None => SsoConfigResponse {
            enabled: false,
            issuer_url: String::new(),
            client_id: String::new(),
            redirect_uri: String::new(),
            default_role: "user".to_string(),
            has_secret: false,
        },
    };

    Ok(foims_common::ok_json(resp, "server.sso.config_retrieved"))
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateSsoConfigRequest {
    pub enabled: bool,
    #[validate(url(message = "server.sso.validation.issuer_format"))]
    pub issuer_url: String,
    #[validate(length(min = 1, max = 100, message = "server.sso.validation.client_id_length"))]
    pub client_id: String,
    #[validate(length(max = 200, message = "server.sso.validation.secret_length"))]
    pub client_secret: String,
    #[validate(length(
        min = 1,
        max = 255,
        message = "server.sso.validation.redirect_uri_length"
    ))]
    pub redirect_uri: String,
    #[validate(custom(
        function = "foims_models::validate_role",
        message = "server.user.validation.role_invalid"
    ))]
    pub default_role: String,
}

pub async fn update_sso_config<P: AuthProvider>(
    State(state): State<Arc<P>>,
    _admin: crate::extractor::AdminUser,
    AppJson(req): AppJson<UpdateSsoConfigRequest>,
) -> Result<Response, AppError> {
    req.validate()?;

    let pool = &state.pool()?.get_conn();
    let client_secret = if req.client_secret.is_empty() {
        get_sso_config_from_db(pool)
            .await
            .ok_or_else(|| AppError::NotFound(msg("server.sso.not_configured_secret")))?
            .client_secret
    } else {
        req.client_secret
    };

    // 变更签发者后旧发现文档缓存失效
    metadata_cache().clear();

    let config = SsoConfig {
        enabled: req.enabled,
        issuer_url: req.issuer_url.trim_end_matches('/').to_string(),
        client_id: req.client_id,
        client_secret,
        redirect_uri: req.redirect_uri.trim().to_string(),
        default_role: req.default_role,
    };

    save_sso_config_to_db(pool, &config).await?;

    Ok(foims_common::ok_json((), "server.sso.config_updated"))
}

/// 测试已保存的 SSO 配置：执行 OIDC 发现文档获取。
pub async fn test_sso_connection<P: AuthProvider>(
    State(state): State<Arc<P>>,
    _admin: crate::extractor::AdminUser,
) -> Result<Response, AppError> {
    let config = get_sso_config_from_db(&state.pool()?.get_conn())
        .await
        .ok_or_else(|| AppError::NotFound(msg("server.sso.not_configured")))?;

    let issuer = IssuerUrl::new(config.issuer_url.clone())
        .map_err(|e| AppError::Validation(msg("server.sso.issuer_invalid").with("error", e)))?;
    CoreProviderMetadata::discover_async(issuer, sso_http_client()?)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.sso.discovery_failed").with("error", format!("{e:?}")))
        })?;

    Ok(foims_common::ok_json((), "server.sso.test_success"))
}
