//! 组织类型模板管理。

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{OrgTemplate, OrgTemplateCreate, OrgTemplateSummary, OrgTemplateUpdate};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use axum::extract::{Path, State};
use axum::response::Response;
use chrono::Utc;
use ipma_common::msg;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

/// 组织层级最大深度（根为第 1 层），模板校验与组织节点创建共用
pub const MAX_ORG_DEPTH: usize = 10;

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
        "server.org_template.types_fetched",
    ))
}

/// 类型 key 允许的字符：Unicode 字母/数字（含中文）、下划线、连字符。
/// 拒绝 HTML 元字符，防止类型名注入前端 innerHTML 渲染。
fn is_valid_type_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// 校验 levels 映射格式并返回根类型
/// levels 格式: { "type_a": ["type_b"], "type_b": ["type_c", "type_d"], ... }
pub fn validate_levels_mapping(levels: &serde_json::Value) -> Result<String, AppError> {
    let levels_map = levels.as_object().ok_or_else(|| {
        AppError::Validation(msg("server.org_template.validation.levels_must_be_object"))
    })?;

    if levels_map.is_empty() {
        return Err(AppError::Validation(msg(
            "server.org_template.validation.levels_empty",
        )));
    }

    if levels_map.len() > 50 {
        return Err(AppError::Validation(msg(
            "server.org_template.validation.levels_too_many_types",
        )));
    }

    let mut all_child_types: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (key, value) in levels_map {
        if key.trim().is_empty() {
            return Err(AppError::Validation(msg(
                "server.org_template.validation.type_name_required",
            )));
        }
        if key.len() > 50 {
            return Err(AppError::Validation(
                msg("server.org_template.validation.type_name_too_long").with("name", key),
            ));
        }
        if let Some(invalid) = key.chars().find(|c| !is_valid_type_char(*c)) {
            return Err(AppError::Validation(
                msg("server.org_template.validation.type_name_invalid")
                    .with("name", key)
                    .with("char", invalid),
            ));
        }

        let children = value.as_array().ok_or_else(|| {
            AppError::Validation(
                msg("server.org_template.validation.children_not_array").with("name", key),
            )
        })?;

        let mut seen_in_this_key: std::collections::HashSet<&str> =
            std::collections::HashSet::new();
        for (idx, child) in children.iter().enumerate() {
            let child_str = child.as_str().ok_or_else(|| {
                AppError::Validation(
                    msg("server.org_template.validation.child_not_string")
                        .with("name", key)
                        .with("index", idx),
                )
            })?;
            if child_str.trim().is_empty() {
                return Err(AppError::Validation(
                    msg("server.org_template.validation.child_empty")
                        .with("name", key)
                        .with("index", idx),
                ));
            }
            if child_str.len() > 50 {
                return Err(AppError::Validation(
                    msg("server.org_template.validation.child_too_long")
                        .with("name", key)
                        .with("index", idx),
                ));
            }
            if !seen_in_this_key.insert(child_str) {
                return Err(AppError::Validation(
                    msg("server.org_template.validation.child_duplicate")
                        .with("name", key)
                        .with("child", child_str),
                ));
            }
            all_child_types.insert(child_str.to_string());
        }
    }

    // 所有子类型必须在映射中定义
    for child_type in &all_child_types {
        if !levels_map.contains_key(child_type) {
            return Err(AppError::Validation(
                msg("server.org_template.validation.child_type_undefined").with("type", child_type),
            ));
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
            return Err(AppError::Validation(msg(
                "server.org_template.validation.root_type_missing",
            )));
        }
        _ => {
            let types = root_types
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("、");
            return Err(AppError::Validation(
                msg("server.org_template.validation.root_type_multiple").with("types", types),
            ));
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
        return Err(AppError::Validation(msg(
            "server.org_template.validation.levels_cycle",
        )));
    }

    // 层级深度不得超过组织创建上限（根为第 1 层），否则模板能建、组织节点建不全
    let mut max_depth = 0usize;
    fn walk_depth(
        node: &str,
        levels_map: &serde_json::Map<String, serde_json::Value>,
        depth: usize,
        max_depth: &mut usize,
    ) {
        *max_depth = (*max_depth).max(depth);
        let Some(children) = levels_map.get(node).and_then(|v| v.as_array()) else {
            return;
        };
        for child in children {
            if let Some(child_str) = child.as_str() {
                walk_depth(child_str, levels_map, depth + 1, max_depth);
            }
        }
    }
    walk_depth(&root_type, levels_map, 1, &mut max_depth);
    if max_depth > MAX_ORG_DEPTH {
        return Err(AppError::Validation(
            msg("server.org_template.validation.levels_depth_exceeded")
                .with("depth", max_depth)
                .with("max", MAX_ORG_DEPTH),
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
    let icons_map = icons.as_object().ok_or_else(|| {
        AppError::Validation(msg("server.org_template.validation.icons_must_be_object"))
    })?;

    let levels_map = levels
        .as_object()
        .ok_or_else(|| AppError::Internal(msg("server.org_template.levels_format_invalid")))?;

    for (key, value) in icons_map {
        if !levels_map.contains_key(key) {
            return Err(AppError::Validation(
                msg("server.org_template.validation.icon_type_undefined").with("type", key),
            ));
        }
        let Some(icon_str) = value.as_str() else {
            return Err(AppError::Validation(
                msg("server.org_template.validation.icon_not_string").with("type", key),
            ));
        };
        // 图标值渲染进前端 innerHTML，仅允许图标 key / emoji 等纯文本，禁止 HTML 元字符
        let invalid = icon_str.len() > 20
            || icon_str
                .chars()
                .any(|c| matches!(c, '<' | '>' | '&' | '"' | '\'' | '`' | '=') || c.is_control());
        if invalid {
            return Err(AppError::Validation(
                msg("server.org_template.validation.icon_value_invalid").with("type", key),
            ));
        }
    }

    Ok(())
}

/// 按类型重命名映射重排 icons 键（重命名传播到图标），并清理新 levels 中不存在的悬空键
fn remap_icons(
    icons: &serde_json::Value,
    mapping: Option<&std::collections::HashMap<String, String>>,
    new_levels: &serde_json::Value,
) -> serde_json::Value {
    let Some(levels_map) = new_levels.as_object() else {
        return serde_json::json!({});
    };

    let mut result = serde_json::Map::new();
    if let Some(icons_map) = icons.as_object() {
        for (type_name, icon) in icons_map {
            let new_name = mapping
                .and_then(|m| m.get(type_name))
                .map(String::as_str)
                .unwrap_or(type_name);
            if levels_map.contains_key(new_name) {
                result.insert(new_name.to_string(), icon.clone());
            }
        }
    }
    serde_json::Value::Object(result)
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
        .ok_or_else(|| AppError::Internal(msg("server.org_template.levels_format_invalid")))?;

    let children = levels_map
        .get(type_str)
        .ok_or_else(|| {
            AppError::Validation(
                msg("server.org_template.validation.type_not_defined").with("type", type_str),
            )
        })?
        .as_array()
        .ok_or_else(|| AppError::Internal(msg("server.org_template.levels_format_invalid")))?;

    children
        .iter()
        .map(|c| {
            c.as_str()
                .map(String::from)
                .ok_or_else(|| AppError::Internal(msg("server.org_template.levels_format_invalid")))
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
        "server.org_template.list_fetched",
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
    .ok_or_else(|| AppError::NotFound(msg("server.org_template.not_found")))?;

    Ok(crate::error::ok_json(
        template,
        "server.org_template.fetched",
    ))
}

/// 创建模板
pub async fn create_org_template(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<OrgTemplateCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 校验 levels 映射格式（返回值仅用于校验，无需保留）
    validate_levels_mapping(&req.levels)?;

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
            return AppError::Conflict(msg("server.org_template.already_exists"));
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
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "org_template",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(
        template,
        "server.org_template.created",
    ))
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
    .ok_or_else(|| AppError::NotFound(msg("server.org_template.not_found")))?;

    // 检查是否有关联的组织节点正在使用此模板
    let usage_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM organizations WHERE template_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    // 使用中模板的 levels 变更校验：
    // 允许任意增加层级/子类型（同层或下层）与删除未被引用的类型，
    // 但存量组织节点的类型语义不得改变，具体规则：
    // 1) 结构一致时（compute_type_name_mapping 返回 Some）：仅允许"真重命名"，
    //    发生变更的旧类型名必须在新层级中彻底消失；若旧名仍存在，说明是调换
    //    子级顺序或交换名称，索引型 type_path 会把存量节点静默改成其他类型
    // 2) 结构变化时：逐个校验存量节点的 type_path 在新层级中解析出的类型名
    //    与旧层级一致（等价于：被引用的类型不可删除、不可重命名，且同层
    //    新类型的插入位置不得位于被引用索引之前）
    let mut rename_mapping: Option<std::collections::HashMap<String, String>> = None;
    if usage_count > 0
        && let Some(ref new_levels) = req.levels
        && new_levels != &existing.levels
    {
        if let Some(mapping) = compute_type_name_mapping(&existing.levels, new_levels) {
            let new_names: std::collections::HashSet<&str> = new_levels
                .as_object()
                .map(|m| m.keys().map(String::as_str).collect())
                .unwrap_or_default();
            for (old_name, new_name) in &mapping {
                if old_name != new_name && new_names.contains(old_name.as_str()) {
                    return Err(AppError::Validation(
                        msg("server.org_template.in_use_swap_forbidden")
                            .with("count", usage_count)
                            .with("name", old_name),
                    ));
                }
            }
            rename_mapping = Some(mapping);
        } else {
            let node_paths: Vec<String> =
                sqlx::query_scalar("SELECT type_path FROM organizations WHERE template_id = $1")
                    .bind(id)
                    .fetch_all(&mut *tx)
                    .await?;
            for path in &node_paths {
                let old_name = super::resolve_type_name(&existing.levels, path)?;
                let new_name = super::resolve_type_name(new_levels, path);
                let broken = match &new_name {
                    Ok(name) => name != &old_name,
                    Err(_) => true,
                };
                if broken {
                    return Err(AppError::Validation(
                        msg("server.org_template.in_use_path_broken")
                            .with("count", usage_count)
                            .with("name", old_name),
                    ));
                }
            }
        }
    }

    // 计算更新后的 icons：请求值优先；未传时按重命名映射重排现有键（重命名传播到图标），
    // 并清理新 levels 中不存在的悬空键（避免仅改 levels 时旧图标键无限累积）
    let effective_levels = req
        .levels
        .clone()
        .unwrap_or_else(|| existing.levels.clone());
    let effective_icons = match &req.icons {
        Some(icons_val) => icons_val.clone(),
        None => remap_icons(&existing.icons, rename_mapping.as_ref(), &effective_levels),
    };
    validate_icons_mapping(&effective_icons, &effective_levels)?;

    let now = Utc::now();

    sqlx::query(
        "UPDATE org_templates SET
         name = COALESCE($1, name),
         levels = COALESCE($2, levels),
         icons = $3,
         description = COALESCE($4, description),
         updated_at = $5
         WHERE id = $6",
    )
    .bind(&req.name)
    .bind(&req.levels)
    .bind(&effective_icons)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict(msg("server.org_template.already_exists"));
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
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "org_template",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(
        template,
        "server.org_template.updated",
    ))
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
        return Err(AppError::NotFound(msg("server.org_template.not_found")));
    }

    let usage_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM organizations WHERE template_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if usage_count > 0 {
        return Err(AppError::Validation(
            msg("server.org_template.in_use_delete_forbidden").with("count", usage_count),
        ));
    }

    sqlx::query("DELETE FROM org_templates WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({ "template_id": id.to_string() });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "org_template",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json((), "server.org_template.deleted"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一条深度为 depth 的线性链 levels
    fn chain_levels(depth: usize) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for i in 0..depth {
            let key = format!("t{i}");
            let children = if i + 1 < depth {
                vec![serde_json::json!(format!("t{}", i + 1))]
            } else {
                vec![]
            };
            map.insert(key, serde_json::Value::Array(children));
        }
        serde_json::Value::Object(map)
    }

    #[test]
    fn test_validate_levels_depth_within_limit() {
        let levels = chain_levels(MAX_ORG_DEPTH);
        assert!(validate_levels_mapping(&levels).is_ok());
    }

    #[test]
    fn test_validate_levels_depth_exceeds_limit() {
        let levels = chain_levels(MAX_ORG_DEPTH + 1);
        let err = validate_levels_mapping(&levels).unwrap_err();
        // 错误消息已 key 化，AppError Display 仅展示 key
        assert!(err.to_string().contains("levels_depth_exceeded"));
    }

    #[test]
    fn test_remap_icons_renames_and_prunes() {
        let old_levels = serde_json::json!({ "公司": ["楼号"], "楼号": [] });
        let new_levels = serde_json::json!({ "园区": ["楼号"], "楼号": [] });
        let icons = serde_json::json!({ "公司": "🏠", "楼号": "🏫", "已删除类型": "📦" });

        let mapping = compute_type_name_mapping(&old_levels, &new_levels);
        assert!(mapping.is_some());
        let remapped = remap_icons(&icons, mapping.as_ref(), &new_levels);

        // 公司 → 园区（重命名传播），楼号保留，已删除类型被清理
        assert_eq!(remapped, serde_json::json!({ "园区": "🏠", "楼号": "🏫" }));
    }

    #[test]
    fn test_remap_icons_without_mapping_prunes_only() {
        let levels = serde_json::json!({ "公司": ["楼号"], "楼号": [] });
        let icons = serde_json::json!({ "公司": "🏠", "楼号": "🏫", "悬空": "📦" });

        let result = remap_icons(&icons, None, &levels);
        assert_eq!(result, serde_json::json!({ "公司": "🏠", "楼号": "🏫" }));
    }

    #[test]
    fn test_compute_mapping_detects_structure_change() {
        let old_levels = serde_json::json!({ "a": ["b", "c"], "b": [], "c": [] });
        let added = serde_json::json!({ "a": ["b", "c", "d"], "b": [], "c": [], "d": [] });
        assert!(compute_type_name_mapping(&old_levels, &added).is_none());
    }
}
