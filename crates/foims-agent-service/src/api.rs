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
use foims_common::{log_info, log_warn};

use crate::manifest::{self, AgentManifest};
use crate::packaging::{self, PackageInputs};

/// 站点 CA 公钥路径（公开物料，作 agent 信任锚，设计 §3.1）。
const SITE_CA_PATH: &str = "/etc/ssl/foims-ca/ca.pem";
/// agent 默认上报端口（设计 §5.4：server_addr 未指定端口时取请求 Host + 9100）。
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
    /// 显式覆盖上报地址（host[:port]）
    server_addr: Option<String>,
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

    let Ok(manifest) = manifest::load(std::path::Path::new(&dist_dir)) else {
        return Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "available": false,
                "message": msg("server.agent.dist_unavailable").key(),
            })),
        )
            .into_response());
    };

    // 版本门控（设计 §6.2）：manifest.version >= 主程序版本才允许下载
    let gate_ok = manifest::gate_check(&manifest.version, server_version).is_ok();

    // 站点 CA 是否已生成（证书管理页生成后即可下载）
    let ca_ok = tokio::fs::metadata(SITE_CA_PATH).await.is_ok();

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

    // 1. 清单加载
    let manifest: AgentManifest = manifest::load(std::path::Path::new(&dist_dir))
        .map_err(|_| AppError::NotFound(msg("server.agent.dist_unavailable")))?;

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
    let ca_pem = tokio::fs::read(SITE_CA_PATH)
        .await
        .map_err(|_| AppError::Conflict(msg("server.agent.ca_missing")))?;

    // 5. 二进制校验（sha256 + size 与清单一致）
    let binary =
        manifest::verify_binary(std::path::Path::new(&dist_dir), &manifest, &params.target)
            .map_err(|_| {
                AppError::Validation(
                    msg("server.agent.binary_missing").with("target", &params.target),
                )
            })?;

    // 6. 上报地址：显式参数优先，否则取请求 Host/URI authority（h2c 无 Host
    //    头）+ 默认端口；解析失败须在入库前返回，避免泄漏 pending 记录
    let server_addr = resolve_server_addr(&params, &headers, &uri)?;

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
        install_sh: &install_sh,
    };
    let (content_type, package) = match format {
        "zip" => ("application/zip", packaging::build_zip(inputs)),
        "deb" => (
            "application/vnd.debian.binary-package",
            packaging::build_deb(inputs),
        ),
        "rpm" => ("application/x-rpm", packaging::build_rpm(inputs)),
        _ => {
            return Err(AppError::Validation(
                msg("server.agent.unknown_format").with("format", &params.format),
            ));
        }
    };
    let package = package.map_err(|e| {
        foims_common::log_error!("log.agent.package_failed", detail = e);
        AppError::Internal(
            msg("server.agent.package_failed")
                .with("target", &params.target)
                .with("format", &params.format),
        )
    })?;

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
fn valid_server_addr(addr: &str) -> bool {
    !addr.is_empty()
        && addr
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '[' | ']' | '-' | '_'))
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

/// 解析上报地址：显式 server_addr 优先（trim 非空 + 白名单校验），
/// 否则取请求 Host 拼 9100。h2c（HTTP/2 cleartext）请求没有 Host 头
/// （仅 :authority 伪头，不进 HeaderMap），故再以请求 URI authority 兜底；
/// 两者皆缺时要求显式传入。
fn resolve_server_addr(
    params: &DownloadParams,
    headers: &axum::http::HeaderMap,
    uri: &axum::http::Uri,
) -> Result<String, AppError> {
    if let Some(explicit) = params
        .server_addr
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if !valid_server_addr(explicit) {
            return Err(AppError::Validation(
                msg("server.common.invalid_param").with("param", "server_addr"),
            ));
        }
        return Ok(explicit.to_string());
    }
    let host = host_from_headers(headers)
        .or_else(|| uri.host().map(str::to_string))
        .ok_or_else(|| {
            AppError::Validation(msg("server.common.invalid_param").with("param", "server_addr"))
        })?;
    let addr = format!("{host}:{DEFAULT_AGENT_PORT}");
    if !valid_server_addr(&addr) {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "server_addr"),
        ));
    }
    Ok(addr)
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
    systemctl enable --now foims-agent.service
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
