use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    ApiResponse, OrgTemplate, OrgTemplateCreate, OrgTemplateSummary, OrgTemplateUpdate,
};
use crate::utils::{OperationLogParams, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use serde_json::json;
use tracing::warn;
use uuid::Uuid;
use validator::Validate;

/// 获取所有模板
pub async fn get_org_templates(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let templates = sqlx::query_as::<_, OrgTemplateSummary>(
        "SELECT id, name, levels, description FROM org_templates ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({ "items": templates }),
        "模板列表获取成功",
    )))
}

/// 获取单个模板
pub async fn get_org_template(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let template = sqlx::query_as::<_, OrgTemplate>(
        "SELECT id, name, levels, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM org_templates WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("模板未找到".to_string()))?;

    Ok(HttpResponse::Ok().json(ApiResponse::<OrgTemplate>::success(
        template,
        "模板获取成功",
    )))
}

/// 创建模板
pub async fn create_org_template(
    state: web::Data<AppState>,
    req: web::Json<OrgTemplateCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    // 校验 levels 是一个非空字符串数组，且每个值都是合法的 OrgType
    let levels = req
        .levels
        .as_array()
        .ok_or_else(|| AppError::Validation("levels 必须是一个数组".to_string()))?;

    if levels.is_empty() {
        return Err(AppError::Validation("levels 数组不能为空".to_string()));
    }

    if levels.len() > 10 {
        return Err(AppError::Validation(
            "levels 数组长度不能超过10".to_string(),
        ));
    }

    for (idx, level) in levels.iter().enumerate() {
        let type_str = level
            .as_str()
            .ok_or_else(|| AppError::Validation(format!("levels[{idx}] 必须是字符串")))?;
        if type_str.trim().is_empty() {
            return Err(AppError::Validation(format!("levels[{idx}] 的值不能为空")));
        }
        if type_str.len() > 50 {
            return Err(AppError::Validation(format!(
                "levels[{idx}] 的值长度不能超过50个字符"
            )));
        }
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO org_templates (id, name, levels, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.levels)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&state.pool()?.get_conn())
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict("模板名称已存在".to_string());
        }
        AppError::from(e)
    })?;

    let template = OrgTemplate {
        id,
        name: req.name.clone(),
        levels: req.levels.clone(),
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": template.name,
        "levels": template.levels,
        "description": template.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
            resource_type: "org_template",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<OrgTemplate>::success(
        template,
        "模板创建成功",
    )))
}

/// 更新模板
pub async fn update_org_template(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<OrgTemplateUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    (*req).validate()?;

    // 如果更新了 levels，需要校验
    if let Some(ref levels_val) = req.levels {
        let levels = levels_val
            .as_array()
            .ok_or_else(|| AppError::Validation("levels 必须是一个数组".to_string()))?;

        if levels.is_empty() {
            return Err(AppError::Validation("levels 数组不能为空".to_string()));
        }

        if levels.len() > 10 {
            return Err(AppError::Validation(
                "levels 数组长度不能超过10".to_string(),
            ));
        }

        for (idx, level) in levels.iter().enumerate() {
            let type_str = level
                .as_str()
                .ok_or_else(|| AppError::Validation(format!("levels[{idx}] 必须是字符串")))?;
            if type_str.trim().is_empty() {
                return Err(AppError::Validation(format!("levels[{idx}] 的值不能为空")));
            }
            if type_str.len() > 50 {
                return Err(AppError::Validation(format!(
                    "levels[{idx}] 的值长度不能超过50个字符"
                )));
            }
        }
    }

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing = sqlx::query_as::<_, OrgTemplate>(
        "SELECT id, name, levels, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM org_templates WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("模板未找到".to_string()))?;

    // 检查是否有关联的组织节点正在使用此模板
    let usage_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM organizations WHERE template_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    // 如果有节点在使用且 levels 发生了变化，需要校验兼容性
    if usage_count > 0
        && let Some(ref new_levels) = req.levels
        && new_levels != &existing.levels
    {
        return Err(AppError::Validation(format!(
            "有 {usage_count} 个组织节点正在使用此模板，且 levels 发生了变化，不允许修改。请先删除或迁移相关节点"
        )));
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE org_templates SET
         name = COALESCE($1, name),
         levels = COALESCE($2, levels),
         description = COALESCE($3, description),
         updated_at = $4
         WHERE id = $5",
    )
    .bind(&req.name)
    .bind(&req.levels)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict("模板名称已存在".to_string());
        }
        AppError::from(e)
    })?;

    tx.commit().await?;

    let template = sqlx::query_as::<_, OrgTemplate>(
        "SELECT id, name, levels, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM org_templates WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "name": template.name,
        "levels": template.levels,
        "description": template.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
            resource_type: "org_template",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<OrgTemplate>::success(
        template,
        "模板更新成功",
    )))
}

/// 删除模板
pub async fn delete_org_template(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM org_templates WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("模板未找到".to_string()));
    }

    let usage_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM organizations WHERE template_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if usage_count > 0 {
        return Err(AppError::Validation(format!(
            "有 {usage_count} 个组织节点正在使用此模板，不允许删除。请先删除或迁移相关节点"
        )));
    }

    sqlx::query("DELETE FROM org_templates WHERE id = $1")
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
            resource_type: "org_template",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "模板删除成功")))
}
