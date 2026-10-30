//! 证书管理接口（转发 foims-x509-management 模块）。
//!
//! 生成证书写入 /etc/ssl/foims-certs/，导入证书写入
//! /etc/ssl/foims-import-certs/，站点根 CA 存于 /etc/ssl/foims-ca/，
//! 导入 CA 池存于 /etc/ssl/foims-import-cas/{id}/。
//!
//! 权限约定：
//! - 管理端点（生成/导入/删除/CA 管理）仅管理员可用；
//! - CA 证书是公开数据，info/download 端点公开（登录页提供下载入口），
//!   下载仅提供 PEM 格式；
//! - 证书仅用于程序运行（HTTPS/nginx），不提供证书下载端点；
//! - 私钥仅保存在服务器文件系统上，任何端点都不得下发私钥。

use std::sync::Arc;

use axum::extract::Multipart;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use foims_common::msg;
use foims_x509_management::{
    CaStatus, CertKind, GenerateCaRequest, GenerateCertRequest, apply_certificate_to_nginx,
    ca_status, delete_certificate, generate_ca, generate_certificate, import_ca,
    import_certificate, list_certificates, read_ca_cert_pem, set_ca_cert_only,
};

use crate::app_state::AppState;
use foims_auth::extractor::SysAdminUser;
use foims_auth::meta::{RequestMeta, log_op_best_effort};
use foims_common::AppError;
use foims_common::AppJson;

/// GET /api/system/certificate/list：证书清单（登录即可读，含生成/导入目录
/// 与站点根 CA 状态）。
pub async fn list(_user: foims_auth::extractor::AuthUser) -> Result<Response, AppError> {
    let inventory = list_certificates().await?;
    Ok(foims_common::ok_json(
        inventory,
        "server.certificate.list_retrieved",
    ))
}

/// POST /api/system/certificate/generate：生成证书（系统管理员；public_url
/// 主机名自动并入 SAN）。
pub async fn generate(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _sysadmin: SysAdminUser,
    AppJson(req): AppJson<GenerateCertRequest>,
) -> Result<Response, AppError> {
    let mut extra_sans = Vec::new();
    let config = state.config_snapshot();
    let public_url = config.server.public_url.trim();
    if !public_url.is_empty() {
        let host_part = public_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or_default();
        // IPv6 字面量以 [] 包裹：取到 ] 之间的地址并校验合法后作为 IP SAN；
        // 裸冒号形式（如 2001:db8::1:443）直接按 split(':') 会截断出非法 SAN
        let host = if host_part.starts_with('[') {
            let close = host_part.find(']').ok_or_else(|| {
                AppError::Validation(msg("server.common.invalid_param").with("param", "public_url"))
            })?;
            let ipv6 = &host_part[1..close];
            ipv6.parse::<std::net::Ipv6Addr>().map_err(|_| {
                AppError::Validation(msg("server.common.invalid_param").with("param", "public_url"))
            })?;
            ipv6
        } else {
            host_part.split(':').next().unwrap_or_default()
        };
        if !host.is_empty() {
            extra_sans.push(host.to_string());
        }
    }

    let common_name = req.common_name.clone();
    let cert = generate_certificate(req, extra_sans).await?;

    let details = serde_json::json!({
        "common_name": common_name,
        "path": cert.cert_path.to_string_lossy(),
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "certificate",
        None,
        &details,
    )
    .await;

    // 有 SAN 条目被丢弃时用独立文案提示用户：请求的主机名可能不在证书中
    let message_key = if cert.dropped_san_count > 0 {
        "server.certificate.generate_succeeded_san_dropped"
    } else {
        "server.certificate.generate_succeeded"
    };
    Ok(foims_common::ok_json((), message_key))
}

/// POST /api/system/certificate/ca/generate：生成站点根 CA（系统管理员；
/// 覆盖既有 CA，由旧 CA 签发的证书将不再被新链信任）。
pub async fn ca_generate(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _sysadmin: SysAdminUser,
    AppJson(req): AppJson<GenerateCaRequest>,
) -> Result<Response, AppError> {
    let ca_path = generate_ca(req).await?;

    let details = serde_json::json!({ "path": ca_path.to_string_lossy() });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "certificate_ca",
        None,
        &details,
    )
    .await;

    Ok(foims_common::ok_json(
        (),
        "server.certificate.ca_generate_succeeded",
    ))
}

/// POST /api/system/certificate/ca/import：导入已有 CA 用于签发本站证书
/// （系统管理员；multipart 字段 cert/key 均必填，缺失或格式非法 400）。
pub async fn ca_import(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _sysadmin: SysAdminUser,
    multipart: Multipart,
) -> Result<Response, AppError> {
    let (cert_data, key_data) = read_multipart_cert_pair(multipart).await?;
    let ca_path = import_ca(cert_data, key_data).await?;

    let details = serde_json::json!({ "path": ca_path.to_string_lossy() });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "certificate_ca",
        None,
        &details,
    )
    .await;

    Ok(foims_common::ok_json(
        (),
        "server.certificate.ca_import_succeeded",
    ))
}

/// 从 multipart 中读取 cert/key（CA 导入用，两字段均必填）
async fn read_multipart_cert_pair(
    mut multipart: Multipart,
) -> Result<(Vec<u8>, Vec<u8>), AppError> {
    let mut cert_data: Option<Vec<u8>> = None;
    let mut key_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(msg("server.certificate.read_failed").with("error", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        let data = field.bytes().await.map_err(|e| {
            AppError::Validation(msg("server.certificate.read_failed").with("error", e))
        })?;
        match name.as_str() {
            "cert" => cert_data = Some(data.to_vec()),
            "key" => key_data = Some(data.to_vec()),
            _ => {}
        }
    }

    let cert_data =
        cert_data.ok_or_else(|| AppError::Validation(msg("server.certificate.ca_key_required")))?;
    let key_data =
        key_data.ok_or_else(|| AppError::Validation(msg("server.certificate.ca_key_required")))?;
    Ok((cert_data, key_data))
}

/// GET /api/certificate/ca/info：公开的 CA 状态查询（登录页据此显示/隐藏
/// 根证书下载入口）。
pub async fn ca_info() -> Result<Response, AppError> {
    let status: CaStatus = ca_status().await;
    Ok(foims_common::ok_json(
        status,
        "server.certificate.list_retrieved",
    ))
}

/// GET /api/certificate/ca/download：下载站点根 CA 证书（公开端点，仅 PEM）。
pub async fn ca_download() -> Result<Response, AppError> {
    let content = read_ca_cert_pem().await?;
    serve_file(content, "foims-root-ca.crt", "application/x-pem-file")
}

fn serve_file(content: Vec<u8>, filename: &str, content_type: &str) -> Result<Response, AppError> {
    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, content_type.to_string()),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        content,
    )
        .into_response())
}

/// POST /api/system/certificate/import：导入证书对（系统管理员；multipart
/// 字段 cert/key 必填、ca 可选且仅供导出终端根证书使用；缺失或格式非法 400）。
pub async fn import(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _sysadmin: SysAdminUser,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let mut cert_data: Option<Vec<u8>> = None;
    let mut key_data: Option<Vec<u8>> = None;
    let mut ca_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(msg("server.certificate.read_failed").with("error", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        let data = field.bytes().await.map_err(|e| {
            AppError::Validation(msg("server.certificate.read_failed").with("error", e))
        })?;
        match name.as_str() {
            "cert" => cert_data = Some(data.to_vec()),
            "key" => key_data = Some(data.to_vec()),
            "ca" => ca_data = Some(data.to_vec()),
            _ => {}
        }
    }

    let cert_data = cert_data
        .ok_or_else(|| AppError::Validation(msg("server.certificate.cert_file_missing")))?;
    let key_data = key_data
        .ok_or_else(|| AppError::Validation(msg("server.certificate.cert_file_missing")))?;

    // 附带的 CA（可选）仅供导出终端根证书使用，不参与证书/私钥校验
    if let Some(ca) = ca_data {
        set_ca_cert_only(ca).await?;
    }

    let cert_path = import_certificate(cert_data, key_data).await?;

    let details = serde_json::json!({ "path": cert_path.to_string_lossy() });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "certificate",
        None,
        &details,
    )
    .await;

    Ok(foims_common::ok_json(
        (),
        "server.certificate.import_succeeded",
    ))
}

/// DELETE /api/system/certificate/{kind}/{file_stem}：删除证书对（系统管理员；
/// kind 非法 400，文件不存在 404）。
pub async fn delete(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _sysadmin: SysAdminUser,
    Path((kind, file_stem)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let cert_kind = CertKind::parse(&kind)
        .ok_or_else(|| AppError::Validation(msg("server.certificate.kind_invalid")))?;

    delete_certificate(cert_kind, &file_stem).await?;

    let details = serde_json::json!({ "kind": kind, "file_stem": file_stem });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "certificate",
        None,
        &details,
    )
    .await;

    Ok(foims_common::ok_json(
        (),
        "server.certificate.delete_succeeded",
    ))
}

/// POST /api/system/certificate/{kind}/{file_stem}/apply：把该证书的证书/私钥
/// 路径整体替换进 nginx 的 foims.conf（系统管理员；kind 非法 400，文件不存在
/// 404；不自动重启服务，由前端提示用户重启生效）。
pub async fn apply(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _sysadmin: SysAdminUser,
    Path((kind, file_stem)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let cert_kind = CertKind::parse(&kind)
        .ok_or_else(|| AppError::Validation(msg("server.certificate.kind_invalid")))?;

    let result = apply_certificate_to_nginx(cert_kind, &file_stem).await?;

    let updated: Vec<String> = result
        .updated
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let details = serde_json::json!({
        "kind": kind,
        "file_stem": file_stem,
        "updated": updated,
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "certificate",
        None,
        &details,
    )
    .await;

    Ok(foims_common::ok_json(
        (),
        "server.certificate.apply_succeeded",
    ))
}
