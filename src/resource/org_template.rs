use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{OrgTemplate, OrgTemplateCreate, OrgTemplateSummary, OrgTemplateUpdate};
use crate::routes::static_files::AppJson;
use crate::utils::common::{log_op_best_effort, RequestMeta};
use axum::extract::{Path, State};
use axum::response::Response;
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

/// 获取所有可用的组织类型配置（从所有模板中提取）
pub async fn get_available_org_types(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    // 获取所有模板
    let templates: Vec<OrgTemplate> = sqlx::query_as(
        "SELECT id, name, levels, icons, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM org_templates ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    // 提取所有唯一的组织类型
    let mut all_types: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut all_icons: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    for template in &templates {
        // 从levels中提取所有类型
        if let Some(levels_obj) = template.levels.as_object() {
            // 添加父类型
            for parent_type in levels_obj.keys() {
                all_types.insert(parent_type.clone());
            }
            // 添加子类型
            for children in levels_obj.values() {
                if let Some(children_arr) = children.as_array() {
                    for child in children_arr {
                        if let Some(child_str) = child.as_str() {
                            all_types.insert(child_str.to_string());
                        }
                    }
                }
            }
        }

        // 合并icons配置
        if let Some(icons_obj) = template.icons.as_object() {
            for (type_name, icon) in icons_obj {
                all_icons.insert(type_name.clone(), icon.clone());
            }
        }
    }

    // 构建返回数据
    let types: Vec<String> = all_types.into_iter().collect();
    let icons = serde_json::Value::Object(all_icons);

    Ok(crate::error::ok_json(
        json!({
            "types": types,
            "icons": icons
        }),
        "获取组织类型配置成功",
    ))
}

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

/// 比较两个 levels 的树形拓扑结构是否相同（忽略类型名称）
/// 只比较：层级深度、每个节点的子节点数量
pub fn levels_structure_equals(
    old_levels: &serde_json::Value,
    new_levels: &serde_json::Value,
) -> bool {
    compute_type_name_mapping(old_levels, new_levels).is_some()
}

/// 计算两个 levels 之间的类型名称映射（old_name → new_name）
/// 仅在结构相同时有效；结构不同返回 None
pub fn compute_type_name_mapping(
    old_levels: &serde_json::Value,
    new_levels: &serde_json::Value,
) -> Option<std::collections::HashMap<String, String>> {
    let old_map = old_levels.as_object()?;
    let new_map = new_levels.as_object()?;

    if old_map.len() != new_map.len() {
        return None;
    }

    let old_root = find_root(old_map)?;
    let new_root = find_root(new_map)?;

    let mut mapping = std::collections::HashMap::new();

    fn collect_mapping<'a>(
        old_key: &str,
        old_map: &'a serde_json::Map<String, serde_json::Value>,
        new_key: &str,
        new_map: &'a serde_json::Map<String, serde_json::Value>,
        mapping: &mut std::collections::HashMap<String, String>,
    ) -> bool {
        mapping.insert(old_key.to_string(), new_key.to_string());

        let old_children = match old_map.get(old_key).and_then(|v| v.as_array()) {
            Some(c) => c,
            None => return false,
        };
        let new_children = match new_map.get(new_key).and_then(|v| v.as_array()) {
            Some(c) => c,
            None => return false,
        };

        if old_children.len() != new_children.len() {
            return false;
        }

        for (old_child, new_child) in old_children.iter().zip(new_children.iter()) {
            let old_child_str = match old_child.as_str() {
                Some(s) => s,
                None => return false,
            };
            let new_child_str = match new_child.as_str() {
                Some(s) => s,
                None => return false,
            };
            if !collect_mapping(old_child_str, old_map, new_child_str, new_map, mapping) {
                return false;
            }
        }
        true
    }

    if !collect_mapping(old_root, old_map, new_root, new_map, &mut mapping) {
        return None;
    }
    Some(mapping)
}

fn find_root(map: &serde_json::Map<String, serde_json::Value>) -> Option<&str> {
    let mut child_types: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for value in map.values() {
        if let Some(arr) = value.as_array() {
            for child in arr {
                if let Some(s) = child.as_str() {
                    child_types.insert(s);
                }
            }
        }
    }
    map.keys()
        .find(|key| !child_types.contains(key.as_str()))
        .map(|v| v.as_str())
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
pub async fn get_org_templates(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let templates = sqlx::query_as::<_, OrgTemplateSummary>(
        "SELECT id, name, levels, icons, description FROM org_templates ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(crate::error::ok_json(
        json!({ "items": templates }),
        "模板列表获取成功",
    ))
}

/// 获取单个模板
pub async fn get_org_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let template = sqlx::query_as::<_, OrgTemplate>(
        "SELECT id, name, levels, icons, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM org_templates WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("模板未找到".to_string()))?;

    Ok(crate::error::ok_json(template, "模板获取成功"))
}

/// 创建模板
pub async fn create_org_template(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<OrgTemplateCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

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
    log_op_best_effort(&state.pool()?.get_conn(), &meta, "create", "org_template", Some(&id), &details).await;

    Ok(crate::error::ok_json(template, "模板创建成功"))
}

/// 更新模板
pub async fn update_org_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<OrgTemplateUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

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

    // 如果有节点在使用且 levels 发生了变化，需要校验结构兼容性
    // 允许修改类型名称（如 b1公司 → bb公司），但不允许改变层级结构（增删层级或子节点数量）
    if usage_count > 0
        && let Some(ref new_levels) = req.levels
        && new_levels != &existing.levels
        && !levels_structure_equals(&existing.levels, new_levels)
    {
        return Err(AppError::Validation(format!(
            "有 {usage_count} 个组织节点正在使用此模板，且层级结构发生了变化（增删了层级或子节点），不允许修改。仅允许修改类型名称"
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
    log_op_best_effort(&state.pool()?.get_conn(), &meta, "update", "org_template", Some(&id), &details).await;

    Ok(crate::error::ok_json(template, "模板更新成功"))
}

/// 删除模板
pub async fn delete_org_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
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
    log_op_best_effort(&state.pool()?.get_conn(), &meta, "delete", "org_template", Some(&id), &details).await;

    Ok(crate::error::ok_json((), "模板删除成功"))
}
