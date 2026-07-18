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

/// 校验 levels 映射格式并返回根类型
/// levels 格式: { "type_a": ["type_b"], "type_b": ["type_c", "type_d"], ... }
pub fn validate_levels_mapping(levels: &serde_json::Value) -> Result<String, AppError> {
    let levels_map = levels.as_object().ok_or_else(|| {
        AppError::Validation("levels 必须是一个对象（类型→子级映射）".to_string())
    })?;

    if levels_map.is_empty() {
        return Err(AppError::Validation("levels 映射不能为空".to_string()));
    }

    if levels_map.len() > 50 {
        return Err(AppError::Validation(
            "levels 映射的类型数量不能超过50".to_string(),
        ));
    }

    let mut all_child_types: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (key, value) in levels_map {
        if key.trim().is_empty() {
            return Err(AppError::Validation("类型名称不能为空".to_string()));
        }
        if key.len() > 50 {
            return Err(AppError::Validation(format!(
                "类型名称「{key}」长度不能超过50个字符"
            )));
        }

        let children = value
            .as_array()
            .ok_or_else(|| AppError::Validation(format!("类型「{key}」的子级必须是数组")))?;

        let mut seen_in_this_key: std::collections::HashSet<&str> =
            std::collections::HashSet::new();
        for (idx, child) in children.iter().enumerate() {
            let child_str = child.as_str().ok_or_else(|| {
                AppError::Validation(format!("类型「{key}」的子级[{idx}]必须是字符串"))
            })?;
            if child_str.trim().is_empty() {
                return Err(AppError::Validation(format!(
                    "类型「{key}」的子级[{idx}]不能为空"
                )));
            }
            if child_str.len() > 50 {
                return Err(AppError::Validation(format!(
                    "类型「{key}」的子级[{idx}]长度不能超过50个字符"
                )));
            }
            if !seen_in_this_key.insert(child_str) {
                return Err(AppError::Validation(format!(
                    "类型「{key}」的子级中存在重复类型「{child_str}」"
                )));
            }
            all_child_types.insert(child_str.to_string());
        }
    }

    // 所有子类型必须在映射中定义
    for child_type in &all_child_types {
        if !levels_map.contains_key(child_type) {
            return Err(AppError::Validation(format!(
                "子类型「{child_type}」未在映射中定义，请添加该类型作为 key"
            )));
        }
    }

    // 必须有且仅有一个根类型
    let root_types: Vec<&String> = levels_map
        .keys()
        .filter(|k| !all_child_types.contains(*k))
        .collect();
    let root_type = match root_types.len() {
        1 => root_types[0].clone(),
        0 => {
            return Err(AppError::Validation(
                "未找到根类型（所有类型都作为子级出现，存在循环引用）".to_string(),
            ));
        }
        _ => {
            return Err(AppError::Validation(format!(
                "存在多个根类型: {:?}，请确保只有一个根类型",
                root_types
            )));
        }
    };

    // 检测非根节点间的循环引用（DFS 染色法）
    let mut visited: std::collections::HashMap<&String, u8> = std::collections::HashMap::new();
    fn has_cycle<'a>(
        node: &'a String,
        levels_map: &'a serde_json::Map<String, serde_json::Value>,
        visited: &mut std::collections::HashMap<&'a String, u8>,
    ) -> bool {
        match visited.get(node) {
            Some(&2) => return false,
            Some(&1) => return true,
            _ => {}
        }
        visited.insert(node, 1);
        if let Some(children) = levels_map.get(node).and_then(|v| v.as_array()) {
            for child in children {
                if let Some(child_str) = child.as_str() {
                    let child_key = levels_map.keys().find(|k| k.as_str() == child_str);
                    if let Some(child_key) = child_key
                        && has_cycle(child_key, levels_map, visited)
                    {
                        return true;
                    }
                }
            }
        }
        visited.insert(node, 2);
        false
    }
    if has_cycle(&root_type, levels_map, &mut visited) {
        return Err(AppError::Validation(
            "模板层级存在循环引用，请检查类型间的父子关系".to_string(),
        ));
    }

    Ok(root_type)
}

/// 校验 icons 映射格式
/// icons 格式: { "type_name": "🏢", ... }
/// key 必须存在于 levels 中，值必须是字符串
pub fn validate_icons_mapping(
    icons: &serde_json::Value,
    levels: &serde_json::Value,
) -> Result<(), AppError> {
    let icons_map = icons
        .as_object()
        .ok_or_else(|| AppError::Validation("icons 必须是一个对象（类型→图标映射）".to_string()))?;

    let levels_map = levels
        .as_object()
        .ok_or_else(|| AppError::Internal("levels 格式错误".to_string()))?;

    for (key, value) in icons_map {
        if !levels_map.contains_key(key) {
            return Err(AppError::Validation(format!(
                "图标映射中的类型「{key}」未在 levels 中定义"
            )));
        }
        if !value.is_string() {
            return Err(AppError::Validation(format!(
                "类型「{key}」的图标必须是字符串"
            )));
        }
    }

    Ok(())
}

/// 从 levels 映射中获取指定类型的允许子级类型
pub fn get_allowed_children(
    levels: &serde_json::Value,
    type_str: &str,
) -> Result<Vec<String>, AppError> {
    let levels_map = levels
        .as_object()
        .ok_or_else(|| AppError::Internal("模板 levels 格式错误".to_string()))?;

    let children = levels_map
        .get(type_str)
        .ok_or_else(|| AppError::Validation(format!("类型「{type_str}」未在模板中定义")))?
        .as_array()
        .ok_or_else(|| AppError::Internal("模板 levels 格式错误".to_string()))?;

    children
        .iter()
        .map(|c| {
            c.as_str()
                .map(String::from)
                .ok_or_else(|| AppError::Internal("模板 levels 格式错误".to_string()))
        })
        .collect()
}

/// 获取所有模板
pub async fn get_org_templates(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let templates = sqlx::query_as::<_, OrgTemplateSummary>(
        "SELECT id, name, levels, icons, description FROM org_templates ORDER BY created_at ASC",
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
        "SELECT id, name, levels, icons, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
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

    // 校验 levels 映射格式
    let _root_type = validate_levels_mapping(&req.levels)?;

    // 校验 icons 格式
    let icons = req.icons.clone().unwrap_or_else(|| serde_json::json!({}));
    validate_icons_mapping(&icons, &req.levels)?;

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO org_templates (id, name, levels, icons, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.levels)
    .bind(&icons)
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
        icons,
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
            resource_id: Some(&id),
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
        validate_levels_mapping(levels_val)?;
    }

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing = sqlx::query_as::<_, OrgTemplate>(
        "SELECT id, name, levels, icons, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
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

    // 校验 icons 格式
    let effective_levels = req.levels.as_ref().unwrap_or(&existing.levels);
    if let Some(ref icons_val) = req.icons {
        validate_icons_mapping(icons_val, effective_levels)?;
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE org_templates SET
         name = COALESCE($1, name),
         levels = COALESCE($2, levels),
         icons = COALESCE($3, icons),
         description = COALESCE($4, description),
         updated_at = $5
         WHERE id = $6",
    )
    .bind(&req.name)
    .bind(&req.levels)
    .bind(&req.icons)
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
        "SELECT id, name, levels, icons, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
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
            resource_id: Some(&id),
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
            resource_id: Some(&id),
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
