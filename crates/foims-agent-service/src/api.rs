//! Agent 配置包下载 Web API。
//!
//! - [`agent_admin_guard`]：角色守卫中间件（admin/secadmin，纵深防御）；
//! - [`download_config_package`]：按所选操作系统现场组「配置包」返回
//!   （deb/rpm/zip），内容统一为 agent.toml + ca.pem + client.pem +
//!   client.key；agent 二进制安装包由 CI 直出（Release 页按需下载），
//!   服务端不再动态组二进制包。
//!
//! 注册令牌的查看/重新生成端点在主程序 src/system/config.rs（需配置写锁
//! 与共享配置槽写权限，AppState 专属能力）。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use foims_common::provider::ConfigProvider;
use foims_common::{ApiResponse, AppError, msg};
use foims_common::{log_info, log_warn};

use crate::packaging::{self, PackageInputs};

/// agent.toml 采集间隔 clamp 范围（与 agent 侧 foims-agent-core 保持一致）
const COLLECT_INTERVAL_MIN_SECS: u64 = 1;
const COLLECT_INTERVAL_MAX_SECS: u64 = 60;
/// agent.toml 上报间隔 clamp 范围（与 agent 侧控制面 clamp 保持一致）
const REPORT_INTERVAL_MIN_SECS: u64 = 10;
const REPORT_INTERVAL_MAX_SECS: u64 = 3600;

/// 配置包下载参数（GET /api/agents/config-package）。
#[derive(Debug, Deserialize)]
pub struct ConfigPackageParams {
    /// 包格式：deb | rpm | zip（Windows 通用）
    format: String,
}

/// Agent 端点角色守卫（admin/secadmin）。
///
/// 必须挂在 auth_middleware 之内（先由其校验令牌并注入 JwtClaims）；
/// 包内含注册令牌属敏感物料（设计 §6.5），此处与提取器形成纵深防御。
pub async fn agent_admin_guard(req: Request, next: Next) -> Response {
    match req.extensions().get::<foims_auth::jwt::JwtClaims>() {
        Some(claims) if claims.role == "admin" || claims.role == "secadmin" => next.run(req).await,
        Some(_) => (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::<()>::error(msg("server.auth.admin_required"))),
        )
            .into_response(),
        None => (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::error(msg("server.auth.auth_failed"))),
        )
            .into_response(),
    }
}

/// GET /api/agents/config-package?format=deb|rpm|zip：按所选操作系统现场
/// 组配置包返回：格式非法 400，站点 CA/客户端证书/注册令牌缺失 409，
/// 组包失败 500。
///
/// 不写库：注册由 agent 首报自动完成（machine_id upsert），根治下载即建
/// pending 行的脏数据问题。
pub async fn download_config_package<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: foims_auth::extractor::AdminOrSecAdminUser,
    Query(params): Query<ConfigPackageParams>,
    headers: axum::http::HeaderMap,
    uri: axum::http::Uri,
) -> Result<Response, AppError> {
    // deb/rpm 由包管理器安装配置文件，zip 为双平台通用格式
    let format = params.format.as_str();
    if !matches!(format, "deb" | "rpm" | "zip") {
        return Err(AppError::Validation(
            msg("server.agent.unknown_format").with("format", &params.format),
        ));
    }

    // 未生成站点 CA 时拒绝下载，提示先在证书管理页生成
    let ca_pem = tokio::fs::read(crate::SITE_CA_PATH)
        .await
        .map_err(|_| AppError::Conflict(msg("server.agent.ca_missing")))?;

    // 客户端证书是 mTLS 上报身份；未生成时提示先启用 Agent 采集
    let certs_dir = std::path::Path::new(crate::cert::AGENT_CERT_DIR);
    let client_cert_pem = tokio::fs::read_to_string(certs_dir.join(crate::cert::CLIENT_CERT))
        .await
        .map_err(|_| AppError::Conflict(msg("server.agent.cert_missing")))?;
    let client_key_pem = tokio::fs::read_to_string(certs_dir.join(crate::cert::CLIENT_KEY))
        .await
        .map_err(|_| AppError::Conflict(msg("server.agent.cert_missing")))?;

    let config = state.config();
    let enrollment_token = config.agent.enrollment_token.trim().to_string();
    if !foims_common::net::valid_enrollment_token(&enrollment_token) {
        return Err(AppError::Conflict(msg(
            "server.agent.enrollment_token_missing",
        )));
    }

    let agent = config.agent.clone();
    let public_url = config.server.public_url.clone();
    let server_addr = resolve_report_addr(
        agent.use_public_url,
        &agent.report_url_override,
        &public_url,
        &agent.bind_addr,
        agent.port,
        &headers,
        &uri,
    )?;

    let collect_interval = agent
        .collect_interval_secs
        .clamp(COLLECT_INTERVAL_MIN_SECS, COLLECT_INTERVAL_MAX_SECS);
    let report_interval = agent
        .report_interval_secs
        .clamp(REPORT_INTERVAL_MIN_SECS, REPORT_INTERVAL_MAX_SECS);
    let agent_toml = format!(
        "# FOIMS Agent 配置（由 FOIMS 服务端下载时自动生成）\n\
         server_addr = \"{server_addr}\"\n\
         token = \"{enrollment_token}\"\n\
         report_interval_secs = {report_interval}\n\
         collect_interval_secs = {collect_interval}\n"
    );

    // 配置包无架构维度：deb=all / rpm=noarch / zip 平铺
    let inputs = PackageInputs {
        agent_toml: &agent_toml,
        ca_pem: &ca_pem,
        client_cert_pem: &client_cert_pem,
        client_key_pem: &client_key_pem,
    };
    let (content_type, package) = match format {
        "zip" => ("application/zip", packaging::build_zip(inputs)),
        "deb" => (
            "application/vnd.debian.binary-package",
            packaging::build_deb(env!("CARGO_PKG_VERSION"), &inputs),
        ),
        "rpm" => (
            "application/x-rpm",
            packaging::build_rpm(env!("CARGO_PKG_VERSION"), &inputs),
        ),
        // 格式已在前面校验拒绝，此分支仅为 match 穷尽兜底
        _ => {
            return Err(AppError::Validation(
                msg("server.agent.unknown_format").with("format", &params.format),
            ));
        }
    };
    let package = match package {
        Ok(package) => package,
        Err(e) => {
            log_warn!("log.agent.package_failed", detail = e);
            return Err(AppError::Internal(
                msg("server.agent.package_failed").with("format", &params.format),
            ));
        }
    };

    log_info!(
        "log.agent.config_package_downloaded",
        format = params.format,
    );
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"foims-agent-config-{}.{format}\"",
                    env!("CARGO_PKG_VERSION")
                ),
            ),
        ],
        package,
    )
        .into_response())
}

/// 校验上报地址合法形式 host[:port]（白名单字符，防止注入配置文本）。
/// 尾冒号（如 "host:"）会在补端口时产出非法 "host::9100"，直接拒绝；
/// 方括号 IPv6 以 "]" 结尾不受影响。
fn valid_server_addr(addr: &str) -> bool {
    !addr.is_empty()
        && !addr.ends_with(':')
        && addr
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '[' | ']' | '-' | '_'))
}

/// 端口段校验：非空纯数字（0 与超长段交由 agent 端配置校验拒绝）。
fn is_port_segment(segment: &str) -> bool {
    !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit())
}

/// 拆分手动覆盖地址为 (主机, 显式端口段)。仅处理已通过 `valid_override_addr`
/// 校验的 host[:port] 形式；裸 IPv6 已被校验拒绝，不会走到这里。
fn split_override_addr(addr: &str) -> (&str, Option<&str>) {
    if let Some(idx) = addr.rfind(']') {
        // [v6]:port 或 [v6]
        let (host, rest) = addr.split_at(idx + 1);
        return (host, rest.strip_prefix(':'));
    }
    match addr.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (addr, None),
    }
}

/// 解析配置包内上报地址，优先级：
/// 1. 公共URL开关开启：主机名取系统配置 server.public_url（未配置则返回
///    参数错误），端口取监听端口——监听地址/端口可能经代理、NAT 映射，
///    并非 agent 可达的真实上报地址；
/// 2. 开关关闭且填写覆盖地址：直接使用（缺端口补监听端口，裸 IPv6 拒绝）；
/// 3. 其余：按监听地址推导（通配主机按请求 Host 兜底）。
fn resolve_report_addr(
    use_public_url: bool,
    report_url_override: &str,
    public_url: &str,
    bind_addr: &str,
    bind_port: u16,
    headers: &axum::http::HeaderMap,
    uri: &axum::http::Uri,
) -> Result<String, AppError> {
    if use_public_url {
        let host = host_from_public_url(public_url)
            .ok_or_else(|| AppError::Validation(msg("server.agent.public_url_missing")))?;
        let addr = format!("{host}:{bind_port}");
        if !valid_server_addr(&addr) {
            return Err(AppError::Validation(
                msg("server.common.invalid_param").with("param", "server.public_url"),
            ));
        }
        return Ok(addr);
    }
    let override_addr = report_url_override.trim();
    if !override_addr.is_empty() {
        if !valid_override_addr(override_addr) {
            return Err(AppError::Validation(msg(
                "server.agent.invalid_report_override",
            )));
        }
        // 无显式端口段（host 或 [v6]）时补监听端口
        let (host, port) = split_override_addr(override_addr);
        let addr = match port {
            Some(_) => override_addr.to_string(),
            None => format!("{host}:{bind_port}"),
        };
        return Ok(addr);
    }
    resolve_server_addr(bind_addr, bind_port, headers, uri)
}

/// 校验手动覆盖地址为严格 host[:port] 形式：主机名/IPv4 或方括号 IPv6，
/// 端口段须为纯数字；裸 IPv6（多冒号无方括号）因与端口歧义直接拒绝。
fn valid_override_addr(addr: &str) -> bool {
    if addr.starts_with('[') {
        let Some(end) = addr.find(']') else {
            return false;
        };
        let rest = &addr[end + 1..];
        return rest.is_empty() || (rest.starts_with(':') && is_port_segment(&rest[1..]));
    }
    if addr.contains('[') {
        // 方括号不闭合等畸形：整体视为主机段，交白名单校验拒绝
        return valid_server_addr(addr);
    }
    match addr.split_once(':') {
        None => valid_server_addr(addr),
        Some((host, port)) => !host.is_empty() && is_port_segment(port) && valid_server_addr(addr),
    }
}

/// 从系统配置 server.public_url 提取主机名段：剥 scheme 与路径/查询，
/// 丢弃 URL 自带端口（端口统一取监听端口）；方括号 IPv6 保留方括号。
fn host_from_public_url(public_url: &str) -> Option<String> {
    let raw = public_url.trim();
    let rest = raw.split_once("://").map_or(raw, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next()?;
    if host.starts_with('[') {
        let end = host.find(']')? + 1;
        let v6 = &host[..end];
        return (!v6.is_empty()).then(|| v6.to_string());
    }
    let host = host.split_once(':').map_or(host, |(h, _)| h);
    (!host.is_empty()).then(|| host.to_string())
}

/// 监听地址推导分支（优先级 3）：统一以监听配置（agent.bind_addr 主机 +
/// agent.port 端口）为准——主机部分为通配地址（[::]/::/0.0.0.0/空）时按
/// 请求 Host 推导（h2c 无 Host 头，仅 :authority 伪头，故再以 URI
/// authority 兜底），端口恒取监听端口；最终结果经白名单校验，非法时返回
/// 参数错误。
fn resolve_server_addr(
    bind_addr: &str,
    bind_port: u16,
    headers: &axum::http::HeaderMap,
    uri: &axum::http::Uri,
) -> Result<String, AppError> {
    let mut host = if matches!(bind_addr, "" | "0.0.0.0" | "::" | "[::]") {
        host_from_headers(headers)
            .or_else(|| uri.host().map(str::to_string))
            .ok_or_else(|| {
                AppError::Validation(
                    msg("server.common.invalid_param").with("param", "agent.bind_addr"),
                )
            })?
    } else {
        bind_addr.to_string()
    };
    // 请求 Host 为 IPv6 字面量时补回方括号（host_from_headers/Uri::host 已剥去），
    // 裸 IPv6 直接拼端口会产生歧义地址
    if host.contains(':') && !host.starts_with('[') {
        host = format!("[{host}]");
    }
    let addr = format!("{host}:{bind_port}");
    if !valid_server_addr(&addr) {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "agent.bind_addr"),
        ));
    }
    Ok(addr)
}

/// 从 Host 头提取主机部分（去端口、去方括号 IPv6）。
fn host_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    let raw = headers.get(header::HOST)?.to_str().ok()?;
    let host = if let Some(rest) = raw.strip_prefix('[') {
        // IPv6 字面量 [::1]:8080 → ::1
        let end = rest.find(']')?;
        &rest[..end]
    } else if let Some((h, _)) = raw.split_once(':') {
        // 多冒号为裸 IPv6（无端口），单冒号去端口
        if h.contains(':') { raw } else { h }
    } else {
        raw
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        host_from_public_url, resolve_report_addr, resolve_server_addr, split_override_addr,
        valid_override_addr, valid_server_addr,
    };
    use axum::http::{HeaderMap, Uri};

    #[test]
    fn test_split_override_addr() {
        // host:port 与 [v6]:port：拆分出主机与显式端口段
        assert_eq!(
            split_override_addr("127.0.0.1:9100"),
            ("127.0.0.1", Some("9100"))
        );
        assert_eq!(split_override_addr("[::1]:9443"), ("[::1]", Some("9443")));
        // 缺端口段（host 或 [v6]）：端口为 None，由调用方补监听端口
        assert_eq!(split_override_addr("127.0.0.1"), ("127.0.0.1", None));
        assert_eq!(split_override_addr("[::]"), ("[::]", None));
        assert_eq!(
            split_override_addr("agent.example-com_cn"),
            ("agent.example-com_cn", None)
        );
    }

    #[test]
    fn test_resolve_server_addr() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::HOST,
            "foims.example.com:8443".parse().unwrap(),
        );
        let uri: Uri = "https://foims.example.com/api/agents/config-package"
            .parse()
            .unwrap();

        // 监听主机为通配地址：主机取请求 Host（忽略其端口），端口取监听端口
        assert_eq!(
            resolve_server_addr("[::]", 9100, &headers, &uri).unwrap(),
            "foims.example.com:9100"
        );
        assert_eq!(
            resolve_server_addr("0.0.0.0", 9100, &headers, &uri).unwrap(),
            "foims.example.com:9100"
        );
        // 监听主机可路由：原样作为上报地址
        assert_eq!(
            resolve_server_addr("agent.example-com_cn", 9100, &headers, &uri).unwrap(),
            "agent.example-com_cn:9100"
        );
        assert_eq!(
            resolve_server_addr("[2001:db8::1]", 9100, &headers, &uri).unwrap(),
            "[2001:db8::1]:9100"
        );
    }

    #[test]
    fn test_resolve_server_addr_ipv6_host_rebracket() {
        // 请求 Host 为 IPv6 字面量：剥去的方括号应补回，避免裸 IPv6 歧义
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::HOST,
            "[2001:db8::1]:8443".parse().unwrap(),
        );
        let uri: Uri = "https://[2001:db8::1]/api/agents/config-package"
            .parse()
            .unwrap();
        assert_eq!(
            resolve_server_addr("[::]", 9100, &headers, &uri).unwrap(),
            "[2001:db8::1]:9100"
        );
    }

    #[test]
    fn test_valid_server_addr() {
        // 合法：域名 / IPv4 / 方括号 IPv6（缺端口与带端口形式均合法）
        assert!(valid_server_addr("foims.example.com"));
        assert!(valid_server_addr("127.0.0.1:9100"));
        assert!(valid_server_addr("[::1]"));
        assert!(valid_server_addr("[::1]:9100"));
        assert!(valid_server_addr("agent.example-com_cn"));
        // 尾冒号：补端口会产出 "host::9100" 非法形式，必须拒绝
        //（方括号 IPv6 以 "]" 结尾不受影响，见上方合法用例）
        assert!(!valid_server_addr("host:"), "尾冒号应拒绝");
        assert!(!valid_server_addr("127.0.0.1:"), "尾冒号应拒绝");
        assert!(!valid_server_addr("[::1]:"), "尾冒号应拒绝");
        assert!(!valid_server_addr("::"), "裸 IPv6 尾冒号应拒绝");
        // 空串与非白名单字符拒绝
        assert!(!valid_server_addr(""));
        assert!(!valid_server_addr("host\n.example.com"));
        assert!(!valid_server_addr("host\"; rm -rf /"));
    }

    #[test]
    fn test_host_from_public_url() {
        // scheme / 路径 / 端口 / 尾斜杠均剥离，仅留主机名；端口统一取监听端口
        assert_eq!(
            host_from_public_url("https://foims.example.com"),
            Some("foims.example.com".into())
        );
        assert_eq!(
            host_from_public_url("https://foims.example.com:8443/console/"),
            Some("foims.example.com".into())
        );
        assert_eq!(host_from_public_url("localhost/"), Some("localhost".into()));
        assert_eq!(
            host_from_public_url("http://127.0.0.1"),
            Some("127.0.0.1".into())
        );
        // 方括号 IPv6 保留方括号；空值/纯 scheme 返回 None
        assert_eq!(
            host_from_public_url("https://[2001:db8::1]/x"),
            Some("[2001:db8::1]".into())
        );
        assert_eq!(host_from_public_url(""), None);
        assert_eq!(host_from_public_url("  "), None);
        assert_eq!(host_from_public_url("https://"), None);
    }

    #[test]
    fn test_valid_override_addr() {
        // 合法：host / host:port / [v6] / [v6]:port
        assert!(valid_override_addr("agent.example.com"));
        assert!(valid_override_addr("agent.example.com:9100"));
        assert!(valid_override_addr("127.0.0.1:9100"));
        assert!(valid_override_addr("[2001:db8::1]"));
        assert!(valid_override_addr("[2001:db8::1]:9100"));
        // 非法：裸 IPv6 歧义 / 尾冒号 / 畸形端口 / 空串
        assert!(!valid_override_addr("2001:db8::1"));
        assert!(!valid_override_addr("host:"));
        assert!(!valid_override_addr("host:abc"));
        assert!(!valid_override_addr("[2001:db8::1"));
        assert!(!valid_override_addr(""));
        assert!(!valid_override_addr("host\"; rm -rf /"));
    }

    #[test]
    fn test_resolve_report_addr() {
        let empty_headers = HeaderMap::new();
        let uri: Uri = "https://foims.example.com/api/agents/config-package"
            .parse()
            .unwrap();

        // 优先级 1：公共URL开关 → 公共URL主机 + 监听端口
        assert_eq!(
            resolve_report_addr(
                true,
                "",
                "https://foims.example.com:8443/",
                "[::]",
                9100,
                &empty_headers,
                &uri
            )
            .unwrap(),
            "foims.example.com:9100"
        );
        assert_eq!(
            resolve_report_addr(true, "", "", "[::]", 9100, &empty_headers, &uri)
                .unwrap_err()
                .status_code(),
            axum::http::StatusCode::BAD_REQUEST
        );

        // 优先级 2：手动覆盖地址；缺端口补监听端口，带端口原样
        assert_eq!(
            resolve_report_addr(
                false,
                "relay.example.com",
                "",
                "[::]",
                9100,
                &empty_headers,
                &uri
            )
            .unwrap(),
            "relay.example.com:9100"
        );
        assert_eq!(
            resolve_report_addr(
                false,
                "relay.example.com:9443",
                "",
                "[::]",
                9100,
                &empty_headers,
                &uri
            )
            .unwrap(),
            "relay.example.com:9443"
        );
        assert_eq!(
            resolve_report_addr(
                false,
                "[2001:db8::1]",
                "",
                "[::]",
                9100,
                &empty_headers,
                &uri
            )
            .unwrap(),
            "[2001:db8::1]:9100"
        );
        // 覆盖地址非法：400
        assert_eq!(
            resolve_report_addr(false, "2001:db8::1", "", "[::]", 9100, &empty_headers, &uri)
                .unwrap_err()
                .status_code(),
            axum::http::StatusCode::BAD_REQUEST
        );

        // 优先级 3：覆盖为空 → 监听地址推导（可路由主机原样采用）
        assert_eq!(
            resolve_report_addr(
                false,
                "",
                "",
                "agent.example-com_cn",
                9100,
                &empty_headers,
                &uri
            )
            .unwrap(),
            "agent.example-com_cn:9100"
        );
    }
}
