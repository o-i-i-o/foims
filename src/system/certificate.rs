//! 证书管理接口（转发 ipma-x509-manager 模块）。
//!
//! 生成证书写入 /etc/ssl/ipma-certs/，导入证书写入
//! /etc/ssl/ipma-import-certs/，站点根 CA 存于 /etc/ssl/ipma-ca/，
//! 导入 CA 池存于 /etc/ssl/ipma-import-cas/{id}/。
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
use ipma_common::msg;
use ipma_x509_manager::{
    CaStatus, CertKind, GenerateCaRequest, GenerateCertRequest, ca_status, delete_certificate,
    generate_ca, generate_certificate, import_ca, import_certificate, list_certificates,
    read_ca_cert_pem, set_ca_cert_only,
};

use crate::app_state::AppState;
use crate::auth::extractor::AdminUser;
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use ipma_common::AppError;

pub async fn list(_admin: AdminUser) -> Result<Response, AppError> {
    let inventory = list_certificates().await?;
    Ok(ipma_common::ok_json(
        inventory,
        "server.certificate.list_retrieved",
    ))
}

pub async fn generate(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _admin: AdminUser,
    AppJson(req): AppJson<GenerateCertRequest>,
) -> Result<Response, AppError> {
    // public_url 主机名自动加入 SAN
    let mut extra_sans = Vec::new();
    let public_url = state.config.server.public_url.trim();
    if !public_url.is_empty() {
        let host = public_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or(public_url)
            .split(':')
            .next()
            .unwrap_or(public_url);
        if !host.is_empty() {
            extra_sans.push(host.to_string());
        }
    }

    let common_name = req.common_name.clone();
    let cert_path = generate_certificate(req, extra_sans).await?;

    let details = serde_json::json!({
        "common_name": common_name,
        "path": cert_path.to_string_lossy(),
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

    Ok(ipma_common::ok_json(
        (),
        "server.certificate.generate_succeeded",
    ))
}

/// 生成站点根 CA（覆盖既有 CA；由 CA 签发的旧证书将不再被新链信任）
pub async fn ca_generate(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _admin: AdminUser,
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

    Ok(ipma_common::ok_json(
        (),
        "server.certificate.ca_generate_succeeded",
    ))
}

/// 导入已有 CA（证书 + 私钥，multipart 字段 cert/key），用于签发本站证书
pub async fn ca_import(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _admin: AdminUser,
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

    Ok(ipma_common::ok_json(
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

/// 公开的 CA 状态查询：登录页据此显示/隐藏根证书下载入口
pub async fn ca_info() -> Result<Response, AppError> {
    let status: CaStatus = ca_status().await;
    Ok(ipma_common::ok_json(
        status,
        "server.certificate.list_retrieved",
    ))
}

/// 公开的 CA 证书下载：仅 PEM 格式（站点根 CA）。
///
/// CA 证书是公开数据；私钥不存在任何下载通道。
pub async fn ca_download() -> Result<Response, AppError> {
    let content = read_ca_cert_pem().await?;
    serve_file(content, "ipma-root-ca.crt", "application/x-pem-file")
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

pub async fn import(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _admin: AdminUser,
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

    Ok(ipma_common::ok_json(
        (),
        "server.certificate.import_succeeded",
    ))
}

/// 删除证书对（{stem}.pem 与 {stem}.key）
///
/// 证书仅用于程序运行，不提供证书下载端点；需要取用文件时由部署侧
/// 在服务器文件系统上直接操作。
pub async fn delete(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _admin: AdminUser,
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

    Ok(ipma_common::ok_json(
        (),
        "server.certificate.delete_succeeded",
    ))
}
