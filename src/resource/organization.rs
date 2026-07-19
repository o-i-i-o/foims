use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    ApiResponse, OrgTemplate, Organization, OrganizationCreate, OrganizationTreeNode,
    OrganizationUpdate, OrganizationWithChildren, Room,
};
use crate::resource::org_template::{get_allowed_children, validate_levels_mapping};
use crate::utils::pagination::Pagination;
use crate::utils::{OperationLogParams, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use tracing::warn;
use uuid::Uuid;
use validator::Validate;

/// 最大层级深度，防止无限递归
const MAX_DEPTH: usize = 10;

/// 获取组织列表（支持按 parent_id 和 org_type 筛选）
pub async fn get_organizations(
    state: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();
    let parent_id = query
        .get("parent_id")
        .and_then(|id| Uuid::parse_str(id).ok());
    let org_type = query.get("org_type").cloned();
    let root_only = query
        .get("root_only")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let mut conditions: Vec<String> = Vec::new();
    let mut param_idx = 1;

    if !search.is_empty() {
        conditions.push(format!("name ILIKE ${param_idx}"));
        param_idx += 1;
    }
    if parent_id.is_some() {
        conditions.push(format!("parent_id = ${param_idx}"));
        param_idx += 1;
    } else if root_only {
        conditions.push("parent_id IS NULL".to_string());
    }
    if org_type.is_some() {
        conditions.push(format!("org_type = ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let search_pattern = format!("%{search}%");

    let count_sql = format!("SELECT COUNT(*) FROM organizations {where_clause}");
    let mut count_query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));

    let list_sql = format!(
        "SELECT id, name, org_type, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations {where_clause}
         ORDER BY created_at ASC LIMIT ${param_idx} OFFSET ${}",
        param_idx + 1
    );
    let mut list_query = sqlx::query_as::<_, Organization>(sqlx::AssertSqlSafe(list_sql));

    if !search.is_empty() {
        count_query = count_query.bind(&search_pattern);
        list_query = list_query.bind(&search_pattern);
    }
    if let Some(pid) = parent_id {
        count_query = count_query.bind(pid);
        list_query = list_query.bind(pid);
    }
    if let Some(ref ot) = org_type {
        count_query = count_query.bind(ot);
        list_query = list_query.bind(ot);
    }

    let total: i64 = count_query.fetch_one(&state.pool()?.get_conn()).await?;

    let organizations = list_query
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": organizations,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "组织列表获取成功",
    )))
}

/// 获取组织树形结构
pub async fn get_organization_tree(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let all_orgs = sqlx::query_as::<_, Organization>(
        "SELECT id, name, org_type, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let tree = build_tree(&all_orgs);
    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<OrganizationTreeNode>>::success(
            tree,
            "组织树获取成功",
        )),
    )
}

/// 从扁平列表构建树形结构
fn build_tree(all_orgs: &[Organization]) -> Vec<OrganizationTreeNode> {
    let mut children_map: HashMap<Option<Uuid>, Vec<&Organization>> = HashMap::new();
    for org in all_orgs {
        children_map.entry(org.parent_id).or_default().push(org);
    }

    fn build_node(
        org: &Organization,
        children_map: &HashMap<Option<Uuid>, Vec<&Organization>>,
    ) -> OrganizationTreeNode {
        let children: Vec<OrganizationTreeNode> = children_map
            .get(&Some(org.id))
            .map(|childs| childs.iter().map(|c| build_node(c, children_map)).collect())
            .unwrap_or_default();

        OrganizationTreeNode {
            id: org.id,
            name: org.name.clone(),
            org_type: org.org_type.clone(),
            parent_id: org.parent_id,
            description: org.description.clone(),
            template_id: org.template_id,
            level_index: org.level_index,
            children,
            created_at: org.created_at,
            updated_at: org.updated_at,
        }
    }

    children_map
        .get(&None)
        .map(|roots| {
            roots
                .iter()
                .map(|org| build_node(org, &children_map))
                .collect()
        })
        .unwrap_or_default()
}

/// 获取单个组织节点（含子节点）
pub async fn get_organization(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let org = sqlx::query_as::<_, Organization>(
        "SELECT id, name, org_type, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("组织节点未找到".to_string()))?;

    let parent_name: Option<String> = if let Some(pid) = org.parent_id {
        sqlx::query_scalar("SELECT name FROM organizations WHERE id = $1")
            .bind(pid)
            .fetch_optional(&state.pool()?.get_conn())
            .await?
    } else {
        None
    };

    let children = sqlx::query_as::<_, Organization>(
        "SELECT id, name, org_type, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE parent_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let child_count: i64 = children.len() as i64;

    let room_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms WHERE org_id = $1")
        .bind(id)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let result = OrganizationWithChildren {
        id: org.id,
        name: org.name,
        org_type: org.org_type,
        parent_id: org.parent_id,
        parent_name,
        description: org.description,
        template_id: org.template_id,
        level_index: org.level_index,
        children,
        child_count,
        room_count,
        created_at: org.created_at,
        updated_at: org.updated_at,
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<OrganizationWithChildren>::success(
            result,
            "组织节点获取成功",
        )),
    )
}

/// 创建组织节点
pub async fn create_organization(
    state: web::Data<AppState>,
    req: web::Json<OrganizationCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let (template_id, level_index) = if let Some(parent_id) = req.parent_id {
        let parent: Organization = sqlx::query_as::<_, Organization>(
            "SELECT id, name, org_type, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
             FROM organizations WHERE id = $1",
        )
        .bind(parent_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("父级组织节点未找到".to_string()))?;

        // 子节点继承父节点的模板
        let parent_template_id = parent.template_id.ok_or_else(|| {
            AppError::Validation("父级节点未关联模板，无法添加子节点".to_string())
        })?;

        let parent_level = parent.level_index;

        // 获取模板定义
        let template: OrgTemplate = sqlx::query_as::<_, OrgTemplate>(
            "SELECT id, name, levels, icons, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
             FROM org_templates WHERE id = $1",
        )
        .bind(parent_template_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::Validation("关联的模板不存在".to_string()))?;

        let allowed_children = get_allowed_children(&template.levels, &parent.org_type)?;
        if allowed_children.is_empty() {
            return Err(AppError::Validation(format!(
                "类型「{}」不允许添加下级节点（模板「{}」）",
                &parent.org_type, template.name
            )));
        }
        if !allowed_children.contains(&req.org_type) {
            return Err(AppError::Validation(format!(
                "根据模板「{}」，类型「{}」的下级应为 {}，实际为「{}」",
                template.name,
                &parent.org_type,
                allowed_children
                    .iter()
                    .map(|s| format!("「{}」", s))
                    .collect::<Vec<_>>()
                    .join("、"),
                req.org_type
            )));
        }

        let depth = get_depth(&mut tx, parent_id).await?;
        if depth >= MAX_DEPTH {
            return Err(AppError::Validation(format!(
                "已达到最大层级深度限制({MAX_DEPTH})"
            )));
        }

        let duplicate: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM organizations WHERE parent_id = $1 AND name = $2")
                .bind(parent_id)
                .bind(&req.name)
                .fetch_optional(&mut *tx)
                .await?;
        if duplicate.is_some() {
            return Err(AppError::Conflict("同级下已存在同名组织节点".to_string()));
        }

        (Some(parent_template_id), parent_level + 1)
    } else {
        // 根节点：必须指定 template_id
        let template_id = req
            .template_id
            .ok_or_else(|| AppError::Validation("创建根节点时必须指定模板".to_string()))?;

        let template: OrgTemplate = sqlx::query_as::<_, OrgTemplate>(
            "SELECT id, name, levels, icons, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
             FROM org_templates WHERE id = $1",
        )
        .bind(template_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("指定的模板不存在".to_string()))?;

        let root_type = validate_levels_mapping(&template.levels)?;

        if req.org_type != root_type {
            return Err(AppError::Validation(format!(
                "根据模板「{}」，根节点应为类型「{}」，实际为「{}」",
                template.name, &root_type, req.org_type
            )));
        }

        (Some(template_id), 0i32)
    };

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO organizations (id, name, org_type, parent_id, description, template_id, level_index, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.org_type)
    .bind(req.parent_id)
    .bind(&req.description)
    .bind(template_id)
    .bind(level_index)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let org = Organization {
        id,
        name: req.name.clone(),
        org_type: req.org_type.clone(),
        parent_id: req.parent_id,
        description: req.description.clone(),
        template_id,
        level_index,
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": org.name,
        "org_type": org.org_type,
        "parent_id": org.parent_id,
        "template_id": org.template_id,
        "level_index": org.level_index,
        "description": org.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
            resource_type: "organization",
            resource_id: Some(&id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Organization>::success(
            org,
            "组织节点创建成功",
        )),
    )
}

/// 更新组织节点
///
/// 仅支持更新 name / org_type / description。
/// 不支持通过此接口修改 parent_id、template_id、level_index 等结构性字段
/// （这些字段涉及模板层级关系和深度计算，应通过专门的移动接口处理）。
pub async fn update_organization(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<OrganizationUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    (*req).validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing = sqlx::query_as::<_, Organization>(
        "SELECT id, name, org_type, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("组织节点未找到".to_string()))?;

    // 类型变更校验：必须符合当前节点位置（根节点或父节点的子级类型）的模板约束
    if let Some(ref new_org_type) = req.org_type
        && new_org_type != &existing.org_type
    {
        validate_org_type_change(&mut tx, &existing, new_org_type).await?;
    }

    // 名称变更校验：同级下不能重名
    if let Some(ref new_name) = req.name
        && new_name != &existing.name
    {
        let duplicate: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM organizations WHERE parent_id IS NOT DISTINCT FROM $1 AND name = $2 AND id != $3",
        )
        .bind(existing.parent_id)
        .bind(new_name)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        if duplicate.is_some() {
            return Err(AppError::Conflict("同级下已存在同名组织节点".to_string()));
        }
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE organizations SET
         name = COALESCE($1, name),
         org_type = COALESCE($2, org_type),
         description = COALESCE($3, description),
         updated_at = $4
         WHERE id = $5",
    )
    .bind(&req.name)
    .bind(&req.org_type)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let org = sqlx::query_as::<_, Organization>(
        "SELECT id, name, org_type, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "name": org.name,
        "org_type": org.org_type,
        "parent_id": org.parent_id,
        "description": org.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
            resource_type: "organization",
            resource_id: Some(&id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Organization>::success(
            org,
            "组织节点更新成功",
        )),
    )
}

/// 校验节点类型变更是否符合模板约束
///
/// 规则：
/// - 根节点：新类型必须是模板的根类型
/// - 子节点：新类型必须在父节点类型的允许子级列表中
/// - 如果有子节点：所有子节点的现有类型必须在新类型的允许子级列表中
async fn validate_org_type_change(
    conn: &mut sqlx::PgConnection,
    existing: &Organization,
    new_org_type: &str,
) -> Result<(), AppError> {
    let template_id = existing
        .template_id
        .ok_or_else(|| AppError::Validation("该节点未关联模板，无法校验类型".to_string()))?;

    let template: OrgTemplate = sqlx::query_as::<_, OrgTemplate>(
        "SELECT id, name, levels, icons, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM org_templates WHERE id = $1",
    )
    .bind(template_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| AppError::Validation("关联的模板不存在".to_string()))?;

    // 根据节点位置（根/子节点）确定允许的类型列表
    let (allowed_types, position_desc) = if let Some(parent_id) = existing.parent_id {
        let parent: Organization = sqlx::query_as::<_, Organization>(
            "SELECT id, name, org_type, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
             FROM organizations WHERE id = $1",
        )
        .bind(parent_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| AppError::Validation("父级节点不存在".to_string()))?;

        let children = get_allowed_children(&template.levels, &parent.org_type)?;
        (children, format!("父级「{}」的子级", parent.org_type))
    } else {
        let root_type = validate_levels_mapping(&template.levels)?;
        (vec![root_type], "根节点".to_string())
    };

    if !allowed_types.iter().any(|t| t == new_org_type) {
        return Err(AppError::Validation(format!(
            "根据模板「{}」，{}允许的类型为 {}，实际为「{}」",
            template.name,
            position_desc,
            allowed_types
                .iter()
                .map(|s| format!("「{}」", s))
                .collect::<Vec<_>>()
                .join("、"),
            new_org_type
        )));
    }

    // 如果该节点有子节点，需要保证子节点的现有类型在新类型的允许子级列表中
    let children: Vec<String> =
        sqlx::query_scalar("SELECT org_type FROM organizations WHERE parent_id = $1")
            .bind(existing.id)
            .fetch_all(&mut *conn)
            .await?;

    if !children.is_empty() {
        let new_allowed = get_allowed_children(&template.levels, new_org_type)?;
        for child_type in &children {
            if !new_allowed.iter().any(|t| t == child_type) {
                return Err(AppError::Validation(format!(
                    "类型变更后子节点类型「{}」不在新类型「{}」的允许子级列表中，无法变更类型",
                    child_type, new_org_type
                )));
            }
        }
    }

    Ok(())
}

/// 删除组织节点
pub async fn delete_organization(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM organizations WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("组织节点未找到".to_string()));
    }

    let child_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM organizations WHERE parent_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if child_count > 0 {
        return Err(AppError::Validation(format!(
            "该节点下还有 {child_count} 个子节点，请先删除所有子节点"
        )));
    }

    let room_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms WHERE org_id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if room_count > 0 {
        return Err(AppError::Validation(format!(
            "该节点下还有 {room_count} 个机房，请先解除关联后再删除"
        )));
    }

    sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "organization_id": id.to_string()
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "organization",
            resource_id: Some(&id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "组织节点删除成功")))
}

/// 获取指定节点的下级类型信息（基于模板）
pub async fn get_allowed_child_types(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let org = sqlx::query_as::<_, Organization>(
        "SELECT id, name, org_type, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("组织节点未找到".to_string()))?;

    let template_id = org
        .template_id
        .ok_or_else(|| AppError::Validation("该节点未关联模板".to_string()))?;

    let template: OrgTemplate = sqlx::query_as::<_, OrgTemplate>(
        "SELECT id, name, levels, icons, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM org_templates WHERE id = $1",
    )
    .bind(template_id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::Validation("关联的模板不存在".to_string()))?;

    let allowed_children = get_allowed_children(&template.levels, &org.org_type)?;
    let next_level = org.level_index + 1;
    let allowed: Vec<serde_json::Value> = allowed_children
        .iter()
        .map(|child_type| {
            json!({
                "type": child_type,
                "label": child_type,
                "level_index": next_level
            })
        })
        .collect();

    let levels_map = template.levels.as_object();
    let type_count = levels_map.as_ref().map(|m| m.len()).unwrap_or(0);

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "parent_id": id,
            "parent_name": org.name,
            "parent_type": org.org_type,
            "template_id": template_id,
            "template_name": template.name,
            "current_level_index": org.level_index,
            "type_count": type_count,
            "allowed_child_types": allowed
        }),
        "允许的下级类型获取成功",
    )))
}

/// 获取指定父节点的子节点列表
pub async fn get_children(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM organizations WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("组织节点未找到".to_string()));
    }

    let children = sqlx::query_as::<_, Organization>(
        "SELECT id, name, org_type, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE parent_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<Organization>>::success(
            children,
            "子节点列表获取成功",
        )),
    )
}

// ==================== 内部辅助函数 ====================

/// 获取节点深度（从根到该节点的层数）
///
/// 同时检测 parent_id 链中的循环引用，若发现环则返回错误而非静默返回。
async fn get_depth(conn: &mut sqlx::PgConnection, node_id: Uuid) -> Result<usize, AppError> {
    let mut depth = 0usize;
    let mut current_id = node_id;
    let mut visited = std::collections::HashSet::new();
    visited.insert(current_id);

    for _ in 0..=MAX_DEPTH {
        let parent_id: Option<Uuid> =
            sqlx::query_scalar("SELECT parent_id FROM organizations WHERE id = $1")
                .bind(current_id)
                .fetch_optional(&mut *conn)
                .await?
                .flatten();

        match parent_id {
            Some(pid) => {
                if !visited.insert(pid) {
                    return Err(AppError::Internal(format!(
                        "组织节点存在循环引用（检测到节点 {pid} 被重复访问）"
                    )));
                }
                depth += 1;
                current_id = pid;
            }
            None => break,
        }
    }

    Ok(depth)
}

/// 获取组织节点关联的房间列表
pub async fn get_org_rooms(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM organizations WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("组织节点未找到".to_string()));
    }
    let rooms = sqlx::query_as::<_, Room>(
        "SELECT id, name, room_type, org_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM rooms WHERE org_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<Vec<Room>>::success(
        rooms,
        "组织节点房间列表获取成功",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_tree_empty() {
        let tree = build_tree(&[]);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_build_tree_single_root() {
        let root = Organization {
            id: Uuid::new_v4(),
            name: "总部".to_string(),
            org_type: "headquarters".to_string(),
            parent_id: None,
            description: None,
            template_id: None,
            level_index: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let tree = build_tree(std::slice::from_ref(&root));
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "总部");
        assert!(tree[0].children.is_empty());
    }

    #[test]
    fn test_build_tree_with_children() {
        let root_id = Uuid::new_v4();
        let root = Organization {
            id: root_id,
            name: "总部".to_string(),
            org_type: "headquarters".to_string(),
            parent_id: None,
            description: None,
            template_id: None,
            level_index: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let child1 = Organization {
            id: Uuid::new_v4(),
            name: "一号楼".to_string(),
            org_type: "building".to_string(),
            parent_id: Some(root_id),
            description: None,
            template_id: None,
            level_index: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let child2 = Organization {
            id: Uuid::new_v4(),
            name: "二号楼".to_string(),
            org_type: "building".to_string(),
            parent_id: Some(root_id),
            description: None,
            template_id: None,
            level_index: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let tree = build_tree(&[root, child1, child2]);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].children.len(), 2);
    }

    #[test]
    fn test_hierarchy_depth_limit() {
        const {
            assert!(MAX_DEPTH >= 5);
        }
    }
}
