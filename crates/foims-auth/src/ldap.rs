//! LDAP 外部认证：登录与配置管理。
//!
//! 配置存于 `system_configs`（config_type='ldap'，bind_password 以
//! AES-GCM 加密存储）。登录流程：服务账号（或匿名）绑定 → 按
//! user_filter 搜索用户 DN → 以用户 DN + 密码建立新连接绑定验证；
//! 通过后查找或自动建户（auth_provider='ldap'），签发本系统令牌。
//! 用户名拼入过滤器的占位符前做 RFC 4515 转义，防 LDAP 注入。

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::Response;
use ldap3::{LdapConnAsync, LdapConnSettings, Scope, SearchEntry};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use validator::Validate;

use crate::login::{ExternalUser, build_login_response, find_or_create_external_user, log_login};
use crate::meta::RequestMeta;
use crate::provider::AuthProvider;
use foims_common::AppError;
use foims_common::AppJson;
use foims_common::crypto::{decrypt_password_async, encrypt_password_async};
use foims_common::msg;
use foims_models::LdapLoginRequest;

const LDAP_TIMEOUT: Duration = Duration::from_secs(10);

// ==================== 配置模型 ====================

/// LDAP 配置（`bind_password` 内存中为明文，落库前加密）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdapConfig {
    pub enabled: bool,
    /// LDAP 服务器地址：ldap://host:389 或 ldaps://host:636
    pub url: String,
    /// 服务账号 DN；留空表示匿名绑定检索
    pub bind_dn: String,
    pub bind_password: String,
    pub base_dn: String,
    /// 用户搜索过滤器，支持 %s / {username} 占位符，
    /// 例如 (&(objectClass=person)(sAMAccountName=%s))
    pub user_filter: String,
    /// 首次登录自动建户的默认角色（admin/user）
    pub default_role: String,
}

impl Default for LdapConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            bind_dn: String::new(),
            bind_password: String::new(),
            base_dn: String::new(),
            user_filter: "(&(objectClass=person)(uid=%s))".to_string(),
            default_role: "user".to_string(),
        }
    }
}

/// 读取 LDAP 配置；未配置时返回 None（读取失败记录日志）。
pub async fn get_ldap_config_from_db(pool: &PgPool) -> Option<LdapConfig> {
    let rows = match sqlx::query(
        "SELECT key, value FROM system_configs WHERE config_type = 'ldap' ORDER BY key",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            foims_common::log_error!("log.ldap.query_failed", error = e);
            return None;
        }
    };

    let mut config = LdapConfig::default();
    let mut found = false;
    for row in rows {
        let key: String = row.get("key");
        let value: Option<String> = row.get("value");
        let Some(value) = value else { continue };
        found = true;
        match key.as_str() {
            "enabled" => config.enabled = value == "true",
            "url" => config.url = value,
            "bind_dn" => config.bind_dn = value,
            "bind_password" => {
                // 解密失败视为配置不可用：记日志并整体返回 None，
                // 避免以空密码尝试绑定掩盖故障
                match decrypt_password_async(value).await {
                    Ok(plain) => config.bind_password = plain,
                    Err(e) => {
                        foims_common::log_error!("log.ldap.password_decrypt_failed", error = e);
                        return None;
                    }
                }
            }
            "base_dn" => config.base_dn = value,
            "user_filter" => config.user_filter = value,
            "default_role" => config.default_role = value,
            _ => {}
        }
    }

    if found && !config.url.is_empty() && !config.base_dn.is_empty() {
        Some(config)
    } else {
        None
    }
}

/// 保存 LDAP 配置（bind_password 加密后逐 key 写入）。
pub async fn save_ldap_config_to_db(pool: &PgPool, config: &LdapConfig) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;

    let encrypted = encrypt_password_async(config.bind_password.clone())
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.ldap.password_encrypt_failed").with("error", e))
        })?;

    let entries = [
        ("enabled", config.enabled.to_string()),
        ("url", config.url.clone()),
        ("bind_dn", config.bind_dn.clone()),
        ("bind_password", encrypted),
        ("base_dn", config.base_dn.clone()),
        ("user_filter", config.user_filter.clone()),
        ("default_role", config.default_role.clone()),
    ];

    for (key, value) in entries {
        sqlx::query(
            "INSERT INTO system_configs (config_type, key, value)
                     VALUES ('ldap', $1, $2)
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

// ==================== LDAP 认证 ====================

/// RFC 4515 过滤器值转义：阻止单引号/括号/星号/空字节破坏或 broaden 过滤器。
fn escape_ldap_filter(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\\' => out.push_str("\\5c"),
            '*' => out.push_str("\\2a"),
            '(' => out.push_str("\\28"),
            ')' => out.push_str("\\29"),
            '\0' => out.push_str("\\00"),
            _ => out.push(c),
        }
    }
    out
}

/// LDAP 认证结果：用户 DN 与可选属性。
struct LdapVerifiedUser {
    email: Option<String>,
}

/// 执行 LDAP 绑定验证：搜索用户 DN 后，以该 DN + 密码建立新连接绑定。
async fn ldap_authenticate(
    config: &LdapConfig,
    username: &str,
    password: &str,
) -> Result<LdapVerifiedUser, AppError> {
    let settings = LdapConnSettings::new().set_conn_timeout(LDAP_TIMEOUT);

    // 1) 建立检索连接（服务账号或匿名绑定）
    let (conn, mut ldap) = LdapConnAsync::with_settings(settings.clone(), &config.url)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.ldap.connect_failed").with("error", e.to_string()))
        })?;
    ldap3::drive!(conn);

    if !config.bind_dn.is_empty() {
        let bind_ok = ldap
            .simple_bind(&config.bind_dn, &config.bind_password)
            .await
            .map(|result| result.success().is_ok())
            .unwrap_or(false);
        if !bind_ok {
            return Err(AppError::Internal(msg("server.ldap.bind_failed")));
        }
    }

    // 2) 按过滤器搜索用户条目
    let escaped = escape_ldap_filter(username);
    let filter = config
        .user_filter
        .replace("%s", &escaped)
        .replace("{username}", &escaped);

    let search_result = ldap
        .search(
            &config.base_dn,
            Scope::Subtree,
            &filter,
            &["mail", "email", "userPrincipalName"],
        )
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.ldap.search_failed").with("error", e.to_string()))
        })?;
    let (entries, _ldap_result) = search_result
        .success()
        .map_err(|e| AppError::Internal(msg("server.ldap.search_failed").with("error", e)))?;

    if entries.len() != 1 {
        // 0 条（用户不存在）或多条（过滤器不唯一）均按认证失败处理，不区分提示
        let _ = ldap.unbind().await;
        return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
    }

    let mut result_entries = entries;
    let entry = match result_entries.pop() {
        Some(entry) => SearchEntry::construct(entry),
        None => return Err(AppError::Unauthorized(msg("server.auth.login_failed"))),
    };
    let _ = ldap.unbind().await;

    // 3) 用户身份验证：以用户 DN + 密码新建连接绑定
    let (user_conn, mut user_ldap) = LdapConnAsync::with_settings(settings, &config.url)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.ldap.connect_failed").with("error", e.to_string()))
        })?;
    ldap3::drive!(user_conn);

    let verified = user_ldap
        .simple_bind(&entry.dn, password)
        .await
        .map(|result| result.success().is_ok())
        .unwrap_or(false);
    let _ = user_ldap.unbind().await;

    if !verified {
        return Err(AppError::Unauthorized(msg("server.auth.login_failed")));
    }

    let email = entry
        .attrs
        .get("mail")
        .or_else(|| entry.attrs.get("email"))
        .or_else(|| entry.attrs.get("userPrincipalName"))
        .and_then(|values| values.first())
        .filter(|v| v.contains('@'))
        .cloned();

    Ok(LdapVerifiedUser { email })
}

/// LDAP 登录请求：在 foims-models 基础模型上扩展动态码与图形验证码字段
///（基础模型跨 crate 共享，字段经 flatten 组合）。
///
/// `code` 为外部账户启用 TOTP 时的动态码：未携带时若账户启用了 2FA，
/// 返回 `requires_two_factor` 响应（字段形态与本地登录 2FA 分支一致）。
#[derive(Debug, Deserialize)]
pub struct LdapLoginRequestExt {
    #[serde(flatten)]
    pub base: LdapLoginRequest,
    /// 启用 2FA 的外部账户登录时携带的 TOTP 动态码
    pub code: Option<String>,
}

/// LDAP 登录入口（公开路由）。
pub async fn login_with_ldap<P: AuthProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<LdapLoginRequestExt>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    req.base.validate()?;

    // 连续失败达到阈值后要求图形验证码（与本地登录共用触发计数）
    if let Err(key) = crate::captcha::enforce(
        &meta.ip_address,
        req.base.username.trim(),
        &req.base.captcha_id,
        &req.base.captcha_text,
    ) {
        return Err(AppError::Validation(msg(key)));
    }

    let client_ip = meta.ip_address.clone();

    if crate::app_fail2ban::is_ip_banned(&client_ip) {
        let remaining = crate::app_fail2ban::get_ban_remaining(&client_ip);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }

    let username = req.base.username.trim().to_string();

    // 用户名维度封禁检查（此前只查 IP，失败却写入用户名键——记录了但从未执行）
    if crate::app_fail2ban::is_user_banned(&username) {
        let remaining = crate::app_fail2ban::get_user_ban_remaining(&username);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }

    let config = get_ldap_config_from_db(&conn)
        .await
        .ok_or_else(|| AppError::NotFound(msg("server.ldap.not_configured")))?;
    if !config.enabled {
        return Err(AppError::Forbidden(msg("server.ldap.disabled")));
    }

    // LDAP 认证失败与本地登录失败保持一致的对外提示与 fail2ban 记录。
    // 仅认证语义失败（用户不存在/口令错误）计入失败封禁；连接、检索、
    // 服务账号绑定等内部故障不计入，避免 LDAP 故障期间误封正常用户
    let ldap_user = match ldap_authenticate(&config, &username, &req.base.password).await {
        Ok(user) => user,
        Err(error) => {
            if matches!(error, AppError::Unauthorized(_)) {
                crate::app_fail2ban::record_login_failure(
                    &client_ip,
                    &username,
                    "server.login_log.ldap_auth_failed",
                );
                if let Err(e) = log_login(
                    &conn,
                    &username,
                    &meta.ip_address,
                    &meta.user_agent,
                    false,
                    Some("server.login_log.ldap_auth_failed"),
                )
                .await
                {
                    foims_common::log_warn!("log.login.record_failed", error = e);
                }
            } else {
                // 内部故障：仅记录日志，不计入失败封禁
                foims_common::log_error!(
                    "log.ldap.internal_error",
                    username = username,
                    error = error.message().log_string()
                );
            }
            return Err(error);
        }
    };

    let external = find_or_create_external_user(
        &conn,
        "ldap",
        &username,
        ldap_user.email.as_deref(),
        &config.default_role,
    )
    .await?;

    // 防「换标识」绕过：命中 DB 记录后对 DB 用户名键复查封禁
    if crate::app_fail2ban::is_user_banned(&external.username) {
        let remaining = crate::app_fail2ban::get_user_ban_remaining(&external.username);
        return Err(AppError::Forbidden(
            msg("server.auth.ip_banned").with("seconds", remaining),
        ));
    }

    // 2FA：外部账户启用 TOTP 时与本地登录保持一致——
    //   未携带动态码 → 返回 requires_two_factor 响应（字段形态与本地一致，
    //   前端弹出动态码输入后携带 code 重试本端点）；
    //   携带动态码 → 校验 TOTP（复用本地 claim/verify 逻辑，含已用码占用）通过后才签发
    let mut totp_verified = false;
    if external.two_factor_enabled {
        match req.code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
            None => {
                return Ok(crate::login::requires_two_factor_response(
                    &external.username,
                ));
            }
            Some(code) => {
                let verified = crate::login::verify_external_totp(&conn, &external, code).await?;
                if !verified {
                    crate::app_fail2ban::record_login_failure(
                        &client_ip,
                        &username,
                        "server.login_log.invalid_2fa_code",
                    );
                    if let Err(e) = log_login(
                        &conn,
                        &username,
                        &meta.ip_address,
                        &meta.user_agent,
                        false,
                        Some("server.login_log.invalid_2fa_code"),
                    )
                    .await
                    {
                        foims_common::log_warn!("log.login.record_failed", error = e);
                    }
                    return Err(AppError::Unauthorized(msg("server.auth.code_invalid")));
                }
                totp_verified = true;
            }
        }
    }

    complete_external_login(
        state,
        meta,
        external,
        req.base.remember_me.unwrap_or(false),
        totp_verified,
    )
    .await
}

/// 外部认证（LDAP/SSO）通过后的通用收尾：签发令牌并构造 JSON 登录响应。
pub(crate) async fn complete_external_login<P: AuthProvider>(
    state: Arc<P>,
    meta: RequestMeta,
    external: ExternalUser,
    remember_me: bool,
    totp_verified: bool,
) -> Result<Response, AppError> {
    let login_tokens = crate::login::issue_external_login_tokens(
        &state,
        &meta,
        &external,
        remember_me,
        totp_verified,
    )
    .await?;

    let user = foims_models::User {
        id: external.id,
        username: external.username,
        email: external.email,
        role: external.role,
        status: external.status,
        two_factor_enabled: external.two_factor_enabled,
        two_factor_verified: true,
        created_at: external.created_at,
        updated_at: external.updated_at,
    };

    build_login_response(user, login_tokens, meta.is_secure)
}

// ==================== 配置管理接口 ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct LdapConfigResponse {
    pub enabled: bool,
    pub url: String,
    pub bind_dn: String,
    pub base_dn: String,
    pub user_filter: String,
    pub default_role: String,
    pub has_password: bool,
}

pub async fn get_ldap_config<P: AuthProvider>(
    State(state): State<Arc<P>>,
    _admin: crate::extractor::AdminUser,
) -> Result<Response, AppError> {
    let resp = match get_ldap_config_from_db(&state.pool()?.get_conn()).await {
        Some(config) => LdapConfigResponse {
            enabled: config.enabled,
            url: config.url,
            bind_dn: config.bind_dn,
            base_dn: config.base_dn,
            user_filter: config.user_filter,
            default_role: config.default_role,
            has_password: !config.bind_password.is_empty(),
        },
        None => LdapConfigResponse {
            enabled: false,
            url: String::new(),
            bind_dn: String::new(),
            base_dn: String::new(),
            user_filter: "(&(objectClass=person)(uid=%s))".to_string(),
            default_role: "user".to_string(),
            has_password: false,
        },
    };

    Ok(foims_common::ok_json(resp, "server.ldap.config_retrieved"))
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateLdapConfigRequest {
    pub enabled: bool,
    #[validate(url(message = "server.ldap.validation.url_format"))]
    pub url: String,
    #[validate(length(max = 255, message = "server.ldap.validation.bind_dn_length"))]
    pub bind_dn: String,
    #[validate(length(max = 200, message = "server.ldap.validation.password_length"))]
    pub bind_password: String,
    #[validate(length(min = 1, max = 255, message = "server.ldap.validation.base_dn_length"))]
    pub base_dn: String,
    #[validate(length(min = 3, max = 255, message = "server.ldap.validation.filter_length"))]
    pub user_filter: String,
    #[validate(custom(
        function = "foims_models::validate_role",
        message = "server.user.validation.role_invalid"
    ))]
    pub default_role: String,
}

pub async fn update_ldap_config<P: AuthProvider>(
    State(state): State<Arc<P>>,
    _admin: crate::extractor::AdminUser,
    AppJson(req): AppJson<UpdateLdapConfigRequest>,
) -> Result<Response, AppError> {
    req.validate()?;

    if !req.user_filter.contains("%s") && !req.user_filter.contains("{username}") {
        return Err(AppError::Validation(msg(
            "server.ldap.validation.filter_placeholder",
        )));
    }

    let pool = &state.pool()?.get_conn();
    let bind_password = if req.bind_password.is_empty() {
        get_ldap_config_from_db(pool)
            .await
            .ok_or_else(|| AppError::NotFound(msg("server.ldap.not_configured_password")))?
            .bind_password
    } else {
        req.bind_password.clone()
    };

    let config = LdapConfig {
        enabled: req.enabled,
        url: req.url.trim_end_matches('/').to_string(),
        bind_dn: req.bind_dn,
        bind_password,
        base_dn: req.base_dn,
        user_filter: req.user_filter,
        default_role: req.default_role,
    };

    save_ldap_config_to_db(pool, &config).await?;

    Ok(foims_common::ok_json((), "server.ldap.config_updated"))
}

/// 测试已保存的 LDAP 配置：连通性 + 服务账号绑定 + 基础检索。
pub async fn test_ldap_connection<P: AuthProvider>(
    State(state): State<Arc<P>>,
    _admin: crate::extractor::AdminUser,
) -> Result<Response, AppError> {
    let config = get_ldap_config_from_db(&state.pool()?.get_conn())
        .await
        .ok_or_else(|| AppError::NotFound(msg("server.ldap.not_configured")))?;

    let settings = LdapConnSettings::new().set_conn_timeout(LDAP_TIMEOUT);
    let (conn, mut ldap) = LdapConnAsync::with_settings(settings.clone(), &config.url)
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.ldap.connect_failed").with("error", e.to_string()))
        })?;
    ldap3::drive!(conn);

    if !config.bind_dn.is_empty() {
        let bind_ok = ldap
            .simple_bind(&config.bind_dn, &config.bind_password)
            .await
            .map(|result| result.success().is_ok())
            .unwrap_or(false);
        if !bind_ok {
            return Err(AppError::Internal(msg("server.ldap.bind_failed")));
        }
    }

    // 基础检索验证 base_dn 可访问（读取 base 条目本身，不取属性）
    let search_result = ldap
        .search(&config.base_dn, Scope::Base, "(objectclass=*)", &["1.1"])
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.ldap.search_failed").with("error", e.to_string()))
        })?;
    search_result
        .success()
        .map_err(|e| AppError::Internal(msg("server.ldap.search_failed").with("error", e)))?;
    let _ = ldap.unbind().await;

    Ok(foims_common::ok_json((), "server.ldap.test_success"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== RFC 4515 过滤器转义 ====================

    #[test]
    fn test_escape_ldap_filter_special_chars() {
        // 五类特殊字符必须转义为 RFC 4515 十六进制形式
        assert_eq!(escape_ldap_filter(r#"\"#), r#"\5c"#);
        assert_eq!(escape_ldap_filter("*"), r#"\2a"#);
        assert_eq!(escape_ldap_filter("("), r#"\28"#);
        assert_eq!(escape_ldap_filter(")"), r#"\29"#);
        assert_eq!(escape_ldap_filter("\0"), r#"\00"#);
        // 组合输入逐字符全部转义
        assert_eq!(escape_ldap_filter("()*"), r#"\28\29\2a"#);
    }

    #[test]
    fn test_escape_ldap_filter_keeps_ordinary_chars() {
        // 普通字母数字与常见标点保持原样
        assert_eq!(escape_ldap_filter("alice"), "alice");
        assert_eq!(
            escape_ldap_filter("user_01@example.com"),
            "user_01@example.com"
        );
        // 中文字符原样保留
        assert_eq!(escape_ldap_filter("张三"), "张三");
        // 空串保持为空
        assert_eq!(escape_ldap_filter(""), "");
    }

    #[test]
    fn test_escape_ldap_filter_blocks_injection() {
        // 典型注入载荷：闭合括号扩大过滤器匹配范围，转义后不再含裸元字符
        let payload = "admin*)(objectClass=*)";
        let escaped = escape_ldap_filter(payload);
        assert!(!escaped.contains('*'), "转义后不应残留裸星号: {escaped}");
        assert!(!escaped.contains('('), "转义后不应残留裸左括号: {escaped}");
        assert!(!escaped.contains(')'), "转义后不应残留裸右括号: {escaped}");
        assert_eq!(escaped, r"admin\2a\29\28objectClass=\2a\29");

        // 空字节注入同样被转义
        let null_payload = "user\0";
        assert_eq!(escape_ldap_filter(null_payload), r"user\00");
    }

    #[test]
    fn test_ldap_config_default_values() {
        let config = LdapConfig::default();
        assert!(!config.enabled, "默认未启用");
        assert!(config.url.is_empty(), "默认 URL 为空");
        assert!(config.bind_dn.is_empty(), "默认服务账号 DN 为空");
        assert!(config.bind_password.is_empty(), "默认密码为空");
        assert!(config.base_dn.is_empty(), "默认 Base DN 为空");
        assert_eq!(config.user_filter, "(&(objectClass=person)(uid=%s))");
        assert_eq!(config.default_role, "user", "默认建户角色为 user");
        // 默认过滤器应包含用户名占位符
        assert!(config.user_filter.contains("%s"));
    }

    #[test]
    fn test_ldap_config_serde_roundtrip() {
        // 配置结构体 JSON 序列化往返应保持字段不变
        let config = LdapConfig {
            enabled: true,
            url: "ldap://ldap.example.com:389".to_string(),
            bind_dn: "cn=admin,dc=example,dc=com".to_string(),
            bind_password: "secret".to_string(),
            base_dn: "dc=example,dc=com".to_string(),
            user_filter: "(&(objectClass=person)(sAMAccountName={username}))".to_string(),
            default_role: "admin".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap_or_else(|e| panic!("序列化失败: {e}"));
        let parsed: LdapConfig =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(parsed.url, config.url);
        assert_eq!(parsed.bind_dn, config.bind_dn);
        assert_eq!(parsed.bind_password, config.bind_password);
        assert_eq!(parsed.base_dn, config.base_dn);
        assert_eq!(parsed.user_filter, config.user_filter);
        assert_eq!(parsed.default_role, config.default_role);
        assert!(parsed.enabled);
    }
}
