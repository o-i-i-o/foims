//! Agent 分发下载 Web API（设计 docs/agent-design.md §5.4 / §6.3）。
//!
//! - [`agent_admin_guard`]：角色守卫中间件（admin/secadmin，纵深防御）；
//! - [`get_agent_dist`]：产物清单摘要（版本/门控状态/平台列表），驱动下载面板；
//! - [`download_agent_package`]：创建 pending agent 并动态组包返回
//!   （zip/deb/rpm），token 哈希入库，明文仅写入包内配置。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use foims_common::provider::ConfigProvider;
use foims_common::{ApiResponse, AppError, msg};
use foims_common::{log_error, log_info, log_warn};

use crate::manifest::{self, AgentManifest};
use crate::packaging::{self, PackageInputs};

/// 站点 CA 公钥路径取 crate 根常量（cert 模块与下载流程共用，见 lib.rs）。
/// 默认上报端口兜底：监听地址缺合法端口段时按 9100 补齐。
const DEFAULT_AGENT_PORT: &str = "9100";

/// 下载参数（GET /api/agents/download）。
#[derive(Debug, Deserialize)]
pub struct DownloadParams {
    /// rust target triple
    target: String,
    /// 包格式：zip | deb | rpm
    format: String,
    /// 下载备注（写入 agents.label）
    label: Option<String>,
}

/// Agent 分发端点角色守卫（admin/secadmin）。
///
/// 必须挂在 auth_middleware 之内（先由其校验令牌并注入 JwtClaims）；
/// 包内含 token 属敏感物料（设计 §6.5），此处与提取器形成纵深防御。
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

/// GET /api/agents/dist：产物清单摘要。
///
/// 产物目录不可用时仍返回 200（available=false），由前端置灰下载面板并提示。
pub async fn get_agent_dist<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: foims_auth::extractor::AdminOrSecAdminUser,
) -> Result<Response, AppError> {
    let dist_dir = state.config().agent.dist_dir.clone();
    let server_version = state.server_version();

    // 清单读取为同步 IO，移入阻塞线程避免卡住异步 worker
    let manifest_path = std::path::Path::new(&dist_dir).to_path_buf();
    let loaded = tokio::task::spawn_blocking(move || manifest::load(&manifest_path)).await;
    let manifest = match loaded {
        Ok(Ok(manifest)) => manifest,
        // 产物目录不可用时仍返回 200（available=false），由前端置灰下载面板并提示
        Ok(Err(_)) | Err(_) => {
            return Ok((
                StatusCode::OK,
                Json(serde_json::json!({
                    "available": false,
                    "message": msg("server.agent.dist_unavailable").key(),
                })),
            )
                .into_response());
        }
    };

    // 版本门控（设计 §6.2）：manifest.version >= 主程序版本才允许下载
    let gate_ok = manifest::gate_check(&manifest.version, server_version).is_ok();

    // 站点 CA 是否已生成（证书管理页生成后即可下载）
    let ca_ok = tokio::fs::metadata(crate::SITE_CA_PATH).await.is_ok();

    // 平台列表：zip 恒有；deb/rpm 按架构映射成功与否；按架构标签排序
    let mut targets: Vec<serde_json::Value> = manifest
        .targets
        .iter()
        .map(|(target, entry)| {
            let arch = packaging::arch_label(target);
            let mut formats = vec!["zip"];
            if arch.is_some_and(|a| packaging::deb_arch(a).is_some()) {
                formats.push("deb");
            }
            if arch.is_some_and(|a| packaging::rpm_arch(a).is_some()) {
                formats.push("rpm");
            }
            serde_json::json!({
                "target": target,
                "arch": arch,
                "size": entry.size,
                "formats": formats,
            })
        })
        .collect();
    targets.sort_by_key(|v| {
        v.get("arch")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("\u{10FFFF}")
            .to_string()
    });

    let mut body = serde_json::json!({
        "available": true,
        "agent_version": manifest.version,
        "server_version": server_version,
        "gate_ok": gate_ok,
        "ca_ok": ca_ok,
        "targets": targets,
    });
    if !gate_ok {
        body["message"] = serde_json::json!(
            msg("server.agent.version_gate")
                .with("agent_version", &manifest.version)
                .with("server_version", server_version)
                .key()
        );
    }

    Ok((StatusCode::OK, Json(body)).into_response())
}

/// GET /api/agents/download：创建 pending agent 并动态组包返回。
///
/// 流程（设计 §6.3）：清单 → 版本门控 → 目标/格式校验 → CA → 二进制校验
/// → 解析上报地址 → 生成 token（库存哈希）→ 建记录 → 生成 agent.toml
/// → 组包 → 流式返回。
pub async fn download_agent_package<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: foims_auth::extractor::AdminOrSecAdminUser,
    Query(params): Query<DownloadParams>,
    headers: axum::http::HeaderMap,
    uri: axum::http::Uri,
) -> Result<Response, AppError> {
    let dist_dir = state.config().agent.dist_dir.clone();
    let server_version = state.server_version();

    // 1. 清单加载（同步 IO 移入阻塞线程）
    let manifest_path = std::path::Path::new(&dist_dir).to_path_buf();
    let loaded = tokio::task::spawn_blocking(move || manifest::load(&manifest_path)).await;
    let manifest: AgentManifest = match loaded {
        Ok(Ok(manifest)) => manifest,
        Ok(Err(_)) | Err(_) => {
            return Err(AppError::NotFound(msg("server.agent.dist_unavailable")));
        }
    };

    // 2. 版本门控：agent 版本不得低于主程序版本
    if let Err(e) = manifest::gate_check(&manifest.version, server_version) {
        log_warn!("log.agent.version_gate_blocked", detail = e);
        return Err(AppError::Conflict(
            msg("server.agent.version_gate")
                .with("agent_version", &manifest.version)
                .with("server_version", server_version),
        ));
    }

    // 3. 目标平台与格式校验
    if !manifest.targets.contains_key(&params.target) {
        return Err(AppError::NotFound(
            msg("server.agent.unknown_target").with("target", &params.target),
        ));
    }
    let format = params.format.as_str();
    if !matches!(format, "zip" | "deb" | "rpm") {
        return Err(AppError::Validation(
            msg("server.agent.unknown_format").with("format", &params.format),
        ));
    }
    let arch = packaging::arch_label(&params.target).ok_or_else(|| {
        AppError::Validation(msg("server.agent.arch_unsupported").with("target", &params.target))
    })?;
    if format == "deb" && packaging::deb_arch(arch).is_none() {
        return Err(AppError::Validation(
            msg("server.agent.arch_unsupported").with("target", &params.target),
        ));
    }
    if format == "rpm" && packaging::rpm_arch(arch).is_none() {
        return Err(AppError::Validation(
            msg("server.agent.arch_unsupported").with("target", &params.target),
        ));
    }

    // 4. 站点 CA（未生成 CA 时拒绝下载，提示先在证书管理页生成）
    let ca_pem = tokio::fs::read(crate::SITE_CA_PATH)
        .await
        .map_err(|_| AppError::Conflict(msg("server.agent.ca_missing")))?;

    // 4b. Agent 客户端证书（mTLS 上报身份；未生成时提示先启用 Agent 采集）
    let certs_dir = std::path::Path::new(crate::cert::AGENT_CERT_DIR);
    let client_cert_pem = tokio::fs::read_to_string(certs_dir.join(crate::cert::CLIENT_CERT))
        .await
        .map_err(|_| AppError::Conflict(msg("server.agent.cert_missing")))?;
    let client_key_pem = tokio::fs::read_to_string(certs_dir.join(crate::cert::CLIENT_KEY))
        .await
        .map_err(|_| AppError::Conflict(msg("server.agent.cert_missing")))?;

    // 5. 二进制校验（sha256 + size 与清单一致；逐块哈希为 CPU/IO 密集同步
    //    操作，移入阻塞线程避免卡住异步 worker）
    let verify_manifest = manifest.clone();
    let verify_dir = std::path::Path::new(&dist_dir).to_path_buf();
    let verify_target = params.target.clone();
    let verified = tokio::task::spawn_blocking(move || {
        manifest::verify_binary(&verify_dir, &verify_manifest, &verify_target)
    })
    .await;
    let binary = match verified {
        Ok(Ok(binary)) => binary,
        Ok(Err(_)) | Err(_) => {
            return Err(AppError::Validation(
                msg("server.agent.binary_missing").with("target", &params.target),
            ));
        }
    };

    // 6. 上报地址：统一以监听地址推导（通配主机时按请求 Host 兜底）；
    //    解析失败须在入库前返回，避免泄漏 pending 记录
    let bind_addr = state.config().agent.bind_addr.clone();
    let server_addr = resolve_server_addr(&bind_addr, &headers, &uri)?;

    // 7. 生成 32 字节随机 token（明文仅写入包内配置，库内存 SHA-256 哈希）
    let token = generate_token();
    let token_hash = foims_common::net::generate_token_hash(&token);

    // 8. 创建 pending agent 记录（machine_id 首报激活时回填）
    let label = normalize_label(&params.label);
    let pool = state.pool()?.get_conn();
    let agent_id: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO agents (token_hash, label) VALUES ($1, $2) RETURNING id")
            .bind(&token_hash)
            .bind(&label)
            .fetch_one(&pool)
            .await?;

    // agent.toml（由服务端下载时自动生成）
    let agent_toml = format!(
        "# FOIMS Agent 配置（由 FOIMS 服务端下载时自动生成）\n\
         server_addr = \"{server_addr}\"\n\
         token = \"{token}\"\n\
         report_interval_secs = 60\n"
    );

    // install.sh：优先 dist_dir 下的部署版本，缺失时回退内置精简脚本
    let install_sh = load_install_sh(&dist_dir).await;

    // 9. 组包
    let inputs = PackageInputs {
        binary: &binary,
        target: &params.target,
        agent_version: &manifest.version,
        agent_toml: &agent_toml,
        ca_pem: &ca_pem,
        client_cert_pem: &client_cert_pem,
        client_key_pem: &client_key_pem,
        install_sh: &install_sh,
    };
    let (content_type, package) = match format {
        "zip" => ("application/zip", packaging::build_zip(inputs)),
        "deb" => (
            "application/vnd.debian.binary-package",
            packaging::build_deb(inputs),
        ),
        "rpm" => ("application/x-rpm", packaging::build_rpm(inputs)),
        // 格式已在步骤 3 校验，此分支防御性兜底（同样需回删 pending 行）
        _ => {
            rollback_pending_agent(&pool, agent_id).await;
            return Err(AppError::Validation(
                msg("server.agent.unknown_format").with("format", &params.format),
            ));
        }
    };
    let package = match package {
        Ok(package) => package,
        Err(e) => {
            log_error!("log.agent.package_failed", detail = e);
            // 组包失败：回删本次请求创建的 pending 行，避免残留脏记录
            rollback_pending_agent(&pool, agent_id).await;
            return Err(AppError::Internal(
                msg("server.agent.package_failed")
                    .with("target", &params.target)
                    .with("format", &params.format),
            ));
        }
    };

    // 10. 流式返回（attachment 文件名 foims-agent-<版本>-<架构>.<格式>）
    log_info!(
        "log.agent.package_downloaded",
        agent_id = agent_id,
        target = params.target,
        format = params.format,
        label = label.unwrap_or_else(|| "-".to_string()),
    );
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"foims-agent-{}-{}.{format}\"",
                    manifest.version, arch
                ),
            ),
        ],
        package,
    )
        .into_response())
}

/// 组包失败回滚：删除本次下载请求刚创建的 pending agent 行（按本次返回的
/// id 精确删除，不影响其他请求），删除失败仅记日志（残留行可人工清理）。
async fn rollback_pending_agent(pool: &sqlx::PgPool, agent_id: uuid::Uuid) {
    let deleted = sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(agent_id)
        .execute(pool)
        .await;
    match deleted {
        Ok(_) => log_error!("log.agent.package_rollback", agent_id = agent_id),
        Err(e) => log_error!("log.agent.package_rollback", agent_id = agent_id, error = e),
    }
}

/// 生成 32 字节随机 token 的十六进制字符串。
fn generate_token() -> String {
    use rand::Rng;
    let mut bytes = vec![0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 备注规范化：去首尾空白，空串归一为 None（入库 NULL）。
fn normalize_label(label: &Option<String>) -> Option<String> {
    label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
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

/// 拆分监听地址为 (主机, 端口)。端口段非法（裸 [::]、缺端口等）时取默认
/// 端口 9100；裸 IPv6 的末段无法与端口区分，原样视为主机段（agent 端
/// 配置校验会拒绝非法形式）。
fn split_bind_addr(bind_addr: &str) -> (&str, &str) {
    if let Some(idx) = bind_addr.rfind(']') {
        // [v6]:port 或 [v6]
        let (host, rest) = bind_addr.split_at(idx + 1);
        let port = rest.strip_prefix(':').filter(|p| is_port_segment(p));
        return (host, port.unwrap_or(DEFAULT_AGENT_PORT));
    }
    if bind_addr.contains('[') {
        // 方括号不闭合等畸形：整体视为主机段，交白名单校验拒绝
        return (bind_addr, DEFAULT_AGENT_PORT);
    }
    match bind_addr.rsplit_once(':') {
        Some((host, port)) if is_port_segment(port) => (host, port),
        _ => (bind_addr, DEFAULT_AGENT_PORT),
    }
}

/// 解析上报地址：统一以监听地址（agent.bind_addr）为准——主机部分为通配
/// 地址（[::]/::/0.0.0.0/空）时按请求 Host 推导（h2c 无 Host 头，仅
/// :authority 伪头，故再以 URI authority 兜底），端口恒取监听端口；
/// 最终结果经白名单校验，非法时返回参数错误。
fn resolve_server_addr(
    bind_addr: &str,
    headers: &axum::http::HeaderMap,
    uri: &axum::http::Uri,
) -> Result<String, AppError> {
    let (bind_host, port) = split_bind_addr(bind_addr);
    let mut host = if matches!(bind_host, "" | "0.0.0.0" | "::" | "[::]") {
        host_from_headers(headers)
            .or_else(|| uri.host().map(str::to_string))
            .ok_or_else(|| {
                AppError::Validation(
                    msg("server.common.invalid_param").with("param", "agent.bind_addr"),
                )
            })?
    } else {
        bind_host.to_string()
    };
    // 请求 Host 为 IPv6 字面量时补回方括号（host_from_headers/Uri::host 已剥去），
    // 裸 IPv6 直接拼端口会产生歧义地址
    if host.contains(':') && !host.starts_with('[') {
        host = format!("[{host}]");
    }
    let addr = format!("{host}:{port}");
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

/// 内置精简 install.sh：dist_dir 部署版本缺失时的兜底（zip 为按 target 定向组包，
/// 包内二进制即当前平台版本，无需架构探测）。unit 内联写入——zip 内并无
/// foims-agent.service 文件，引用包内路径会导致 set -e 安装失败。
const INSTALL_SH_FALLBACK: &str = r#"#!/bin/sh
# FOIMS Agent 安装脚本（服务端内置兜底版本）
set -e
if [ "$(id -u)" -ne 0 ]; then
    echo "请使用 sudo 执行本脚本" >&2
    exit 1
fi
install -m 0755 foims-agent /usr/local/bin/foims-agent
install -d -m 0755 /etc/foims-agent
install -m 0600 agent.toml /etc/foims-agent/agent.toml
install -m 0644 ca.pem /etc/foims-agent/ca.pem
install -m 0644 client.pem /etc/foims-agent/client.pem
install -m 0600 client.key /etc/foims-agent/client.key
if command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
    cat > /etc/systemd/system/foims-agent.service << 'UNIT_EOF'
[Unit]
Description=FOIMS Agent host metrics collector
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/foims-agent --config /etc/foims-agent/agent.toml
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
UNIT_EOF
    systemctl daemon-reload
    systemctl enable foims-agent.service
    systemctl restart foims-agent.service
else
    echo "未检测到 systemd，请自行配置 foims-agent 的开机自启" >&2
fi
echo "FOIMS Agent 安装完成"
"#;

/// 读取 dist_dir 下的 install.sh；不可用时回退内置精简脚本并告警。
async fn load_install_sh(dist_dir: &str) -> String {
    match tokio::fs::read_to_string(std::path::Path::new(dist_dir).join("install.sh")).await {
        Ok(text) if !text.is_empty() => text,
        Ok(_) | Err(_) => {
            log_warn!("log.agent.install_sh_fallback");
            INSTALL_SH_FALLBACK.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_server_addr, split_bind_addr, valid_server_addr};
    use axum::http::{HeaderMap, Uri};

    #[test]
    fn test_split_bind_addr() {
        // 常规 host:port 与 [v6]:port：原样拆分
        assert_eq!(split_bind_addr("127.0.0.1:9100"), ("127.0.0.1", "9100"));
        assert_eq!(split_bind_addr("[::]:9100"), ("[::]", "9100"));
        assert_eq!(split_bind_addr("[::1]:9443"), ("[::1]", "9443"));
        // 缺合法端口段：端口回退默认 9100
        assert_eq!(split_bind_addr("127.0.0.1"), ("127.0.0.1", "9100"));
        assert_eq!(split_bind_addr("[::]"), ("[::]", "9100"));
        assert_eq!(
            split_bind_addr("agent.example-com_cn"),
            ("agent.example-com_cn", "9100")
        );
    }

    #[test]
    fn test_resolve_server_addr() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::HOST,
            "foims.example.com:8443".parse().unwrap(),
        );
        let uri: Uri = "https://foims.example.com/api/agents/download"
            .parse()
            .unwrap();

        // 监听主机为通配地址：主机取请求 Host（忽略其端口），端口取监听端口
        assert_eq!(
            resolve_server_addr("[::]:9100", &headers, &uri).unwrap(),
            "foims.example.com:9100"
        );
        assert_eq!(
            resolve_server_addr("0.0.0.0:9100", &headers, &uri).unwrap(),
            "foims.example.com:9100"
        );
        // 监听主机可路由：原样作为上报地址
        assert_eq!(
            resolve_server_addr("agent.example-com_cn:9100", &headers, &uri).unwrap(),
            "agent.example-com_cn:9100"
        );
        assert_eq!(
            resolve_server_addr("[2001:db8::1]:9100", &headers, &uri).unwrap(),
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
        let uri: Uri = "https://[2001:db8::1]/api/agents/download".parse().unwrap();
        assert_eq!(
            resolve_server_addr("[::]:9100", &headers, &uri).unwrap(),
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
}
