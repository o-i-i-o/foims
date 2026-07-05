use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    ApiResponse, DeviceTemplate, DeviceTemplateCreate, DeviceTemplateSummary, DeviceTemplateUpdate,
};
use crate::utils::{OperationLogParams, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use serde_json::json;
use tracing::warn;
use uuid::Uuid;
use validator::Validate;

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

/// 创建设备模板
pub async fn create_device_template(
    state: web::Data<AppState>,
    req: web::Json<DeviceTemplateCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    // 校验设备类型，默认为 'other'
    let device_type = req.device_type.as_deref().unwrap_or("other").to_string();

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO device_templates (id, name, device_type, brand, model, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&device_type)
    .bind(&req.brand)
    .bind(&req.model)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&state.pool()?.get_conn())
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict("设备模板名称已存在".to_string());
        }
        AppError::from(e)
    })?;

    let template = DeviceTemplate {
        id,
        name: req.name.clone(),
        device_type,
        brand: req.brand.clone(),
        model: req.model.clone(),
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": template.name,
        "device_type": template.device_type,
        "brand": template.brand,
        "model": template.model,
        "description": template.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
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

    Ok(
        HttpResponse::Ok().json(ApiResponse::<DeviceTemplate>::success(
            template,
            "设备模板创建成功",
        )),
    )
}

/// 更新设备模板
pub async fn update_device_template(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<DeviceTemplateUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    (*req).validate()?;

    // device_type 验证已由 #[validate(custom)] 注解处理
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing = sqlx::query_as::<_, DeviceTemplate>(
        "SELECT id, name, device_type, brand, model, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM device_templates WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("设备模板未找到".to_string()))?;

    // 如果名称发生了变化，检查是否有设备正在使用此模板
    if let Some(ref new_name) = req.name
        && new_name != &existing.name
    {
        let usage_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE template_id = $1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if usage_count > 0 {
            return Err(AppError::Validation(format!(
                "有 {usage_count} 个设备正在使用此模板，不允许修改名称。请先修改相关设备的模板"
            )));
        }
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE device_templates SET
         name = COALESCE($1, name),
         device_type = COALESCE($2, device_type),
         brand = COALESCE($3, brand),
         model = COALESCE($4, model),
         description = COALESCE($5, description),
         updated_at = $6
         WHERE id = $7",
    )
    .bind(&req.name)
    .bind(&req.device_type)
    .bind(&req.brand)
    .bind(&req.model)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict("设备模板名称已存在".to_string());
        }
        AppError::from(e)
    })?;

    tx.commit().await?;

    let template = sqlx::query_as::<_, DeviceTemplate>(
        "SELECT id, name, device_type, brand, model, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM device_templates WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "name": template.name,
        "device_type": template.device_type,
        "brand": template.brand,
        "model": template.model,
        "description": template.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
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

    Ok(
        HttpResponse::Ok().json(ApiResponse::<DeviceTemplate>::success(
            template,
            "设备模板更新成功",
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
