use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{ApiResponse, DeviceTemplate, DeviceTemplateSummary};
use crate::utils::{OperationLogParams, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, web};
use serde_json::json;
use tracing::warn;
use uuid::Uuid;

/// 获取所有设备模板
pub async fn get_device_templates(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let templates = sqlx::query_as::<_, DeviceTemplateSummary>(
        "SELECT id, name, device_type, brand, model FROM device_templates ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({ "items": templates }),
        "设备模板列表获取成功",
    )))
}

/// 获取单个设备模板
pub async fn get_device_template(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let template = sqlx::query_as::<_, DeviceTemplate>(
        "SELECT id, name, device_type, brand, model, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM device_templates WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("设备模板未找到".to_string()))?;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<DeviceTemplate>::success(
            template,
            "设备模板获取成功",
        )),
    )
}

/// 删除设备模板
pub async fn delete_device_template(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

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
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "device_template",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "设备模板删除成功")))
}
