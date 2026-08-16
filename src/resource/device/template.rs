//! 设备模板管理。

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{DeviceTemplate, DeviceTemplateSummary, UpdateDeviceTemplateRequest};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use axum::extract::{Path, State};
use axum::response::Response;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

/// 获取所有设备模板
pub async fn get_device_templates(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    let templates = sqlx::query_as::<_, DeviceTemplateSummary>(
        "SELECT id, name, device_type, brand, model FROM device_templates ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(crate::error::ok_json(
        json!({ "items": templates }),
        "设备模板列表获取成功",
    ))
}

/// 获取单个设备模板
pub async fn get_device_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let template = sqlx::query_as::<_, DeviceTemplate>(
        "SELECT id, name, device_type, brand, model, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM device_templates WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("设备模板未找到".to_string()))?;

    Ok(crate::error::ok_json(template, "设备模板获取成功"))
}

/// 删除设备模板
pub async fn delete_device_template(
    State(state): State<Arc<AppState>>,
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
        return Err(AppError::NotFound("设备模板未找到".to_string()));
    }

    let usage_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE template_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if usage_count > 0 {
        return Err(AppError::Validation(format!(
            "有 {usage_count} 个设备正在使用此模板，不允许删除。请先修改相关设备的模板"
        )));
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

    Ok(crate::error::ok_json((), "设备模板删除成功"))
}

/// 更新设备模板
pub async fn update_device_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<UpdateDeviceTemplateRequest>,
) -> Result<Response, AppError> {
    req.validate()?;

    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM device_templates WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool()?.get_conn())
            .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("设备模板未找到".to_string()));
    }

    let name_conflict: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM device_templates WHERE name = $1 AND id != $2)",
    )
    .bind(&req.name)
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;
    if name_conflict {
        return Err(AppError::Conflict("模板名称已存在".to_string()));
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

    Ok(crate::error::ok_json((), "设备模板更新成功"))
}
