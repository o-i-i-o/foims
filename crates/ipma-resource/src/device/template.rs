//! 设备模板管理。

use axum::extract::{Path, State};
use axum::response::Response;
use ipma_auth::meta::{RequestMeta, log_op_best_effort};
use ipma_common::AppJson;
use ipma_common::DbProvider;
use ipma_common::{AppError, msg};
use ipma_models::{DeviceTemplate, DeviceTemplateSummary, UpdateDeviceTemplateRequest};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

/// 获取所有设备模板
pub async fn get_device_templates<P: DbProvider>(
    State(state): State<Arc<P>>,
) -> Result<Response, AppError> {
    let templates = sqlx::query_as::<_, DeviceTemplateSummary>(
        "SELECT id, name, device_type, brand, model FROM device_templates ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(ipma_common::ok_json(
        json!({ "items": templates }),
        "server.device_template.list_retrieved",
    ))
}

/// 获取单个设备模板
pub async fn get_device_template<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let template = sqlx::query_as::<_, DeviceTemplate>(
        "SELECT id, name, device_type, brand, model, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM device_templates WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.device_template.not_found")))?;

    Ok(ipma_common::ok_json(
        template,
        "server.device_template.fetched",
    ))
}

/// 删除设备模板
pub async fn delete_device_template<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM device_templates WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if existing.is_none() {
        return Err(AppError::NotFound(msg("server.device_template.not_found")));
    }

    let usage_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE template_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if usage_count > 0 {
        return Err(AppError::Validation(
            msg("server.device_template.in_use").with("count", usage_count),
        ));
    }

    sqlx::query("DELETE FROM device_templates WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({ "template_id": id.to_string() });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "device_template",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json((), "server.device_template.deleted"))
}

/// 更新设备模板
pub async fn update_device_template<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<UpdateDeviceTemplateRequest>,
) -> Result<Response, AppError> {
    req.validate()?;

    // device_type 与 create 路径一致：走白名单校验（模型仅有长度校验）
    super::validate_device_type(&req.device_type)?;

    // brand/model/description 对齐 DB 列宽（50/100/TEXT 不限），超长拒绝
    if let Some(brand) = &req.brand
        && brand.chars().count() > 50
    {
        return Err(AppError::Validation(msg(
            "server.device.validation.brand_length",
        )));
    }
    if let Some(model) = &req.model
        && model.chars().count() > 100
    {
        return Err(AppError::Validation(msg(
            "server.device.validation.model_length",
        )));
    }
    if let Some(description) = &req.description
        && description.chars().count() > 255
    {
        return Err(AppError::Validation(msg(
            "server.common.validation.description_length",
        )));
    }

    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM device_templates WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool()?.get_conn())
            .await?;
    if existing.is_none() {
        return Err(AppError::NotFound(msg("server.device_template.not_found")));
    }

    let name_conflict: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM device_templates WHERE name = $1 AND id != $2)",
    )
    .bind(&req.name)
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;
    if name_conflict {
        return Err(AppError::Conflict(msg(
            "server.device_template.name_exists",
        )));
    }

    sqlx::query(
        "UPDATE device_templates SET name = $1, device_type = $2, brand = $3, model = $4, description = $5, updated_at = NOW() WHERE id = $6",
    )
    .bind(&req.name)
    .bind(&req.device_type)
    .bind(&req.brand)
    .bind(&req.model)
    .bind(&req.description)
    .bind(id)
    .execute(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({ "template_id": id.to_string(), "name": req.name });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "device_template",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json((), "server.device_template.updated"))
}
