//! 证书管理接口（转发 ipma-x509-manager 模块）。
//!
//! 生成证书写入 /etc/ssl/ipma-certs/，导入证书写入
//! /etc/ssl/ipma-import-certs/；全部端点仅管理员可用。

use std::sync::Arc;

use axum::extract::Multipart;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use ipma_common::msg;
use ipma_x509_manager::{
    CertKind, GenerateCertRequest, delete_certificate, generate_self_signed, import_certificate,
    list_certificates, read_certificate,
};

use crate::app_state::AppState;
use crate::auth::extractor::AdminUser;
use crate::error::AppError;
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};

pub async fn list(_admin: AdminUser) -> Result<Response, AppError> {
    let inventory = list_certificates().await?;
    Ok(crate::error::ok_json(
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
    let cert_path = generate_self_signed(req, extra_sans).await?;

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

    Ok(crate::error::ok_json(
        (),
        "server.certificate.generate_succeeded",
    ))
}

pub async fn import(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    _admin: AdminUser,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
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

    let cert_data = cert_data
        .ok_or_else(|| AppError::Validation(msg("server.certificate.cert_file_missing")))?;
    let key_data = key_data
        .ok_or_else(|| AppError::Validation(msg("server.certificate.cert_file_missing")))?;

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

    Ok(crate::error::ok_json(
        (),
        "server.certificate.import_succeeded",
    ))
}

/// 下载证书或私钥：kind ∈ generated/imported，filename 为目录内文件名
pub async fn download(
    _admin: AdminUser,
    Path((kind, filename)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let kind = CertKind::parse(&kind)
        .ok_or_else(|| AppError::Validation(msg("server.certificate.kind_invalid")))?;

    let (filename, content) = read_certificate(kind, &filename).await?;

    Ok((
        StatusCode::OK,
        [
            (
                axum::http::header::CONTENT_TYPE,
                "application/x-pem-file".to_string(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        content,
    )
        .into_response())
}

/// 删除证书对（{stem}.pem 与 {stem}.key）
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

    Ok(crate::error::ok_json(
        (),
        "server.certificate.delete_succeeded",
    ))
}
