use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    ApiResponse, OrgTemplate, Organization, OrganizationCreate, OrganizationTreeNode,
    OrganizationUpdate, Room,
};
use crate::resource::org_template::get_allowed_children;
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

// ==================== type_path 解析工具 ====================

/// 从模板的 levels JSON 中，根据 type_path 解析出类型名称
///
/// type_path 格式为点分隔的索引路径，如 "0"、"0.1"、"0.0.2"
/// 每个数字表示在该层级的子类型列表中的索引位置
pub fn resolve_type_name(levels: &serde_json::Value, type_path: &str) -> Result<String, AppError> {
    let levels_map = levels
        .as_object()
        .ok_or_else(|| AppError::Internal("模板 levels 格式错误".to_string()))?;

    let indices: Vec<usize> = type_path
        .split('.')
        .map(|s| {
            s.parse::<usize>()
                .map_err(|_| AppError::Validation(format!("类型路径「{type_path}」格式错误")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if indices.is_empty() {
        return Err(AppError::Validation("类型路径不能为空".to_string()));
    }

    // 第一个索引：在根层级中的位置
    // 找到根节点（不被任何其他节点的子级列表引用的节点）
    let root = find_root_in_levels(levels_map)?;

    // 如果只有一个索引且为0，说明是根节点本身
    if indices.len() == 1 && indices[0] == 0 {
        // 检查是否是根节点（type_path="0"）
        // 根节点名称就是根的 key
        return Ok(root.to_string());
    }

    // 从根的子级开始逐级查找
    let mut current_type = root;
    for (depth, &idx) in indices.iter().enumerate() {
        if depth == 0 {
            // 第一个索引指向根的子级列表
            let children = levels_map
                .get(current_type)
                .and_then(|v| v.as_array())
                .ok_or_else(|| AppError::Internal("模板 levels 格式错误".to_string()))?;

            current_type = children.get(idx).and_then(|v| v.as_str()).ok_or_else(|| {
                AppError::Validation(format!(
                    "类型路径「{type_path}」在模板中不存在（索引 {idx} 超出范围）"
                ))
            })?;
        } else {
            let children = levels_map
                .get(current_type)
                .and_then(|v| v.as_array())
                .ok_or_else(|| AppError::Internal("模板 levels 格式错误".to_string()))?;

            current_type = children.get(idx).and_then(|v| v.as_str()).ok_or_else(|| {
                AppError::Validation(format!(
                    "类型路径「{type_path}」在模板中不存在（索引 {idx} 超出范围）"
                ))
            })?;
        }
    }

    Ok(current_type.to_string())
}

/// 从 type_path 计算子节点的 type_path
///
/// 例如父节点 type_path="0.1"，子节点索引为2，则子节点 type_path="0.1.2"
pub fn child_type_path(parent_type_path: &str, child_index: usize) -> String {
    format!("{parent_type_path}.{child_index}")
}

/// 从 type_path 获取父 type_path
///
/// 例如 "0.1.2" → "0.1"，"0" → None（根节点无父路径）
pub fn parent_type_path(type_path: &str) -> Option<String> {
    let last_dot = type_path.rfind('.')?;
    Some(type_path[..last_dot].to_string())
}

/// 从 type_path 获取子节点在父级子列表中的索引
///
/// 例如 "0.1" → 1，"0.1.2" → 2
pub fn type_path_index(type_path: &str) -> Result<usize, AppError> {
    let last_segment = type_path
        .rsplit('.')
        .next()
        .ok_or_else(|| AppError::Validation(format!("类型路径「{type_path}」格式错误")))?;
    last_segment
        .parse::<usize>()
        .map_err(|_| AppError::Validation(format!("类型路径「{type_path}」格式错误")))
}

/// 从模板的 levels 中找根节点 key
fn find_root_in_levels(
    levels_map: &serde_json::Map<String, serde_json::Value>,
) -> Result<&str, AppError> {
    let mut child_types: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for value in levels_map.values() {
        if let Some(arr) = value.as_array() {
            for child in arr {
                if let Some(s) = child.as_str() {
                    child_types.insert(s);
                }
            }
        }
    }
    levels_map
        .keys()
        .find(|key| !child_types.contains(key.as_str()))
        .map(|v| v.as_str())
        .ok_or_else(|| AppError::Internal("模板 levels 中未找到根节点".to_string()))
}

/// 根据 type_path 获取允许的子类型列表（带 type_path）
pub fn get_allowed_children_with_path(
    levels: &serde_json::Value,
    type_path: &str,
) -> Result<Vec<(String, String)>, AppError> {
    let type_name = resolve_type_name(levels, type_path)?;
    let children_names = get_allowed_children(levels, &type_name)?;
    let result: Vec<(String, String)> = children_names
        .into_iter()
        .enumerate()
        .map(|(idx, name)| (name, child_type_path(type_path, idx)))
        .collect();
    Ok(result)
}

/// 将 Organization 的 type_path 解析为 org_type（类型名称），
/// 需要查询模板的 levels
async fn resolve_org_type(pool: &sqlx::PgPool, org: &Organization) -> Result<String, AppError> {
    let template_id = org
        .template_id
        .ok_or_else(|| AppError::Internal("组织节点未关联模板".to_string()))?;

    let levels: serde_json::Value =
        sqlx::query_scalar("SELECT levels FROM org_templates WHERE id = $1")
            .bind(template_id)
            .fetch_one(pool)
            .await
            .map_err(|_| AppError::Internal("关联的模板不存在".to_string()))?;

    resolve_type_name(&levels, &org.type_path)
}

/// 批量加载模板 levels，用于解析 org_type
async fn load_template_levels(
    pool: &sqlx::PgPool,
    template_id: Uuid,
) -> Result<serde_json::Value, AppError> {
    sqlx::query_scalar("SELECT levels FROM org_templates WHERE id = $1")
        .bind(template_id)
        .fetch_one(pool)
        .await
        .map_err(|_| AppError::Internal("关联的模板不存在".to_string()))
}

// ==================== API 处理函数 ====================

/// 获取组织列表（支持按 parent_id 筛选）
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
    let root_only = query
        .get("root_only")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    // 辅助函数：构建WHERE子句
    fn build_where_clause(
        qb: &mut sqlx::QueryBuilder<sqlx::Postgres>,
        search: &str,
        parent_id: Option<Uuid>,
        root_only: bool,
    ) {
        let mut conditions = Vec::new();

        if !search.is_empty() {
            qb.push(" WHERE name ILIKE ");
            qb.push_bind(format!("%{}%", search));
            conditions.push("search");
        }

        if let Some(pid) = parent_id {
            if !conditions.is_empty() {
                qb.push(" AND parent_id = ");
            } else {
                qb.push(" WHERE parent_id = ");
            }
            qb.push_bind(pid);
            conditions.push("parent_id");
        } else if root_only {
            if !conditions.is_empty() {
                qb.push(" AND parent_id IS NULL");
            } else {
                qb.push(" WHERE parent_id IS NULL");
            }
        }
    }

    // 构建计数查询
    let mut count_qb = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM organizations");
    build_where_clause(&mut count_qb, &search, parent_id, root_only);

    let total: i64 = count_qb
        .build_query_scalar()
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    // 构建列表查询
    let mut list_qb = sqlx::QueryBuilder::new(
        "SELECT id, name, type_path, parent_id, description, template_id, level_index, \
         created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM organizations",
    );
    build_where_clause(&mut list_qb, &search, parent_id, root_only);

    list_qb.push(" ORDER BY created_at ASC LIMIT ");
    list_qb.push_bind(page_size);
    list_qb.push(" OFFSET ");
    list_qb.push_bind(offset);

    let organizations = list_qb
        .build_query_as::<Organization>()
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    // 解析 org_type
    let items = resolve_org_list_types(&state, &organizations).await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": items,
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
        "SELECT id, name, type_path, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    // 批量加载所有需要的模板 levels
    let mut template_levels_map: HashMap<Uuid, serde_json::Value> = HashMap::new();
    for org in &all_orgs {
        if let Some(tid) = org.template_id
            && let std::collections::hash_map::Entry::Vacant(e) = template_levels_map.entry(tid)
            && let Ok(levels) = load_template_levels(&state.pool()?.get_conn(), tid).await
        {
            e.insert(levels);
        }
    }

    let tree = build_tree(&all_orgs, &template_levels_map);
    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<OrganizationTreeNode>>::success(
            tree,
            "组织树获取成功",
        )),
    )
}

/// 从扁平列表构建树形结构（优化版本）
fn build_tree(
    all_orgs: &[Organization],
    template_levels_map: &HashMap<Uuid, serde_json::Value>,
) -> Vec<OrganizationTreeNode> {
    // 先按 parent_id 分组
    let mut children_map: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    let mut node_map: HashMap<Uuid, &Organization> = HashMap::new();

    for org in all_orgs {
        node_map.insert(org.id, org);
        if let Some(parent_id) = org.parent_id {
            children_map.entry(parent_id).or_default().push(org.id);
        }
    }

    // 递归构建节点（带深度检查）
    fn build_node(
        org_id: Uuid,
        node_map: &HashMap<Uuid, &Organization>,
        children_map: &HashMap<Uuid, Vec<Uuid>>,
        template_levels_map: &HashMap<Uuid, serde_json::Value>,
        depth: usize,
    ) -> OrganizationTreeNode {
        let org = node_map.get(&org_id).expect("Organization must exist");

        // 从模板解析类型名称
        let org_type = org
            .template_id
            .and_then(|tid| template_levels_map.get(&tid))
            .and_then(|levels| resolve_type_name(levels, &org.type_path).ok())
            .unwrap_or_else(|| org.type_path.clone());

        // 深度安全检查
        if depth > MAX_DEPTH {
            return OrganizationTreeNode {
                id: org.id,
                name: org.name.clone(),
                org_type,
                parent_id: org.parent_id,
                description: org.description.clone(),
                template_id: org.template_id,
                level_index: org.level_index,
                children: vec![],
                created_at: org.created_at,
                updated_at: org.updated_at,
            };
        }

        let children: Vec<OrganizationTreeNode> = children_map
            .get(&org_id)
            .map(|child_ids| {
                child_ids
                    .iter()
                    .map(|id| {
                        build_node(*id, node_map, children_map, template_levels_map, depth + 1)
                    })
                    .collect()
            })
            .unwrap_or_default();

        OrganizationTreeNode {
            id: org.id,
            name: org.name.clone(),
            org_type,
            parent_id: org.parent_id,
            description: org.description.clone(),
            template_id: org.template_id,
            level_index: org.level_index,
            children,
            created_at: org.created_at,
            updated_at: org.updated_at,
        }
    }

    // 构建根节点
    all_orgs
        .iter()
        .filter(|org| org.parent_id.is_none())
        .map(|org| build_node(org.id, &node_map, &children_map, template_levels_map, 0))
        .collect()
}

/// 获取单个组织节点（含子节点）
pub async fn get_organization(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let org = sqlx::query_as::<_, Organization>(
        "SELECT id, name, type_path, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("组织节点未找到".to_string()))?;

    let org_type = resolve_org_type(&state.pool()?.get_conn(), &org).await?;

    let parent_name: Option<String> = if let Some(pid) = org.parent_id {
        sqlx::query_scalar("SELECT name FROM organizations WHERE id = $1")
            .bind(pid)
            .fetch_optional(&state.pool()?.get_conn())
            .await?
    } else {
        None
    };

    let children = sqlx::query_as::<_, Organization>(
        "SELECT id, name, type_path, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
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

    let child_items = resolve_org_list_types(&state, &children).await?;

    let result = json!({
        "id": org.id,
        "name": org.name,
        "type_path": org.type_path,
        "org_type": org_type,
        "parent_id": org.parent_id,
        "parent_name": parent_name,
        "description": org.description,
        "template_id": org.template_id,
        "level_index": org.level_index,
        "children": child_items,
        "child_count": child_count,
        "room_count": room_count,
        "created_at": org.created_at,
        "updated_at": org.updated_at,
    });

    Ok(HttpResponse::Ok().json(ApiResponse::success(result, "组织节点获取成功")))
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
        // 使用FOR UPDATE锁定父节点，防止并发修改
        let parent: Organization = sqlx::query_as::<_, Organization>(
            "SELECT id, name, type_path, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
             FROM organizations WHERE id = $1 FOR UPDATE",
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

        // 使用 type_path 校验
        let parent_type_name = resolve_type_name(&template.levels, &parent.type_path)?;
        let allowed_children = get_allowed_children(&template.levels, &parent_type_name)?;
        if allowed_children.is_empty() {
            return Err(AppError::Validation(format!(
                "类型「{}」不允许添加下级节点（模板「{}」）",
                parent_type_name, template.name
            )));
        }

        // 校验 type_path 是否在父节点的允许子级中
        let allowed_with_path =
            get_allowed_children_with_path(&template.levels, &parent.type_path)?;
        let is_valid = allowed_with_path
            .iter()
            .any(|(_, path)| path == &req.type_path);
        if !is_valid {
            return Err(AppError::Validation(format!(
                "根据模板「{}」，类型「{}」的下级路径应为 {}，实际为「{}」",
                template.name,
                parent_type_name,
                allowed_with_path
                    .iter()
                    .map(|(_, p)| format!("「{p}」"))
                    .collect::<Vec<_>>()
                    .join("、"),
                req.type_path
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
        // 根节点：必须指定 template_id，type_path 必须为 "0"
        let template_id = req
            .template_id
            .ok_or_else(|| AppError::Validation("创建根节点时必须指定模板".to_string()))?;

        let template_exists: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM org_templates WHERE id = $1")
                .bind(template_id)
                .fetch_optional(&mut *tx)
                .await?;
        if template_exists.is_none() {
            return Err(AppError::NotFound("指定的模板不存在".to_string()));
        }

        // 根节点 type_path 必须是 "0"
        if req.type_path != "0" {
            return Err(AppError::Validation(
                "根节点的类型路径必须为「0」".to_string(),
            ));
        }

        (Some(template_id), 0i32)
    };

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO organizations (id, name, type_path, parent_id, description, template_id, level_index, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.type_path)
    .bind(req.parent_id)
    .bind(&req.description)
    .bind(template_id)
    .bind(level_index)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let org_type = resolve_org_type(
        &state.pool()?.get_conn(),
        &Organization {
            id,
            name: req.name.clone(),
            type_path: req.type_path.clone(),
            parent_id: req.parent_id,
            description: req.description.clone(),
            template_id,
            level_index,
            created_at: now,
            updated_at: now,
        },
    )
    .await
    .unwrap_or_else(|_| req.type_path.clone());

    let details = serde_json::json!({
        "name": req.name,
        "type_path": req.type_path,
        "org_type": org_type,
        "parent_id": req.parent_id,
        "template_id": template_id,
        "level_index": level_index,
        "description": req.description
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

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "id": id,
            "name": req.name,
            "type_path": req.type_path,
            "org_type": org_type,
            "parent_id": req.parent_id,
            "description": req.description,
            "template_id": template_id,
            "level_index": level_index,
            "created_at": now,
            "updated_at": now,
        }),
        "组织节点创建成功",
    )))
}

/// 更新组织节点
///
/// 仅支持更新 name / description。
/// type_path 由模板结构决定，不允许通过此接口修改。
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
        "SELECT id, name, type_path, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("组织节点未找到".to_string()))?;

    // type_path 不允许通过此接口修改（由模板结构决定）
    if let Some(ref new_type_path) = req.type_path
        && new_type_path != &existing.type_path
    {
        return Err(AppError::Validation(
            "类型路径不允许修改（由模板结构决定）".to_string(),
        ));
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
         description = COALESCE($2, description),
         updated_at = $3
         WHERE id = $4",
    )
    .bind(&req.name)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let org = sqlx::query_as::<_, Organization>(
        "SELECT id, name, type_path, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let org_type = resolve_org_type(&state.pool()?.get_conn(), &org).await?;

    let details = serde_json::json!({
        "name": org.name,
        "type_path": org.type_path,
        "org_type": org_type,
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

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "id": org.id,
            "name": org.name,
            "type_path": org.type_path,
            "org_type": org_type,
            "parent_id": org.parent_id,
            "description": org.description,
            "template_id": org.template_id,
            "level_index": org.level_index,
            "created_at": org.created_at,
            "updated_at": org.updated_at,
        }),
        "组织节点更新成功",
    )))
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
        "SELECT id, name, type_path, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
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

    let org_type = resolve_type_name(&template.levels, &org.type_path)?;
    let allowed_with_path = get_allowed_children_with_path(&template.levels, &org.type_path)?;
    let next_level = org.level_index + 1;
    let allowed: Vec<serde_json::Value> = allowed_with_path
        .iter()
        .map(|(type_name, path)| {
            json!({
                "type": type_name,
                "type_path": path,
                "label": type_name,
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
            "parent_type": org_type,
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
        "SELECT id, name, type_path, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE parent_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let items = resolve_org_list_types(&state, &children).await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(items, "子节点列表获取成功")))
}

// ==================== 内部辅助函数 ====================

/// 批量解析组织列表的 org_type
async fn resolve_org_list_types(
    state: &web::Data<AppState>,
    orgs: &[Organization],
) -> Result<Vec<serde_json::Value>, AppError> {
    let mut template_levels_map: HashMap<Uuid, serde_json::Value> = HashMap::new();
    for org in orgs {
        if let Some(tid) = org.template_id
            && let std::collections::hash_map::Entry::Vacant(e) = template_levels_map.entry(tid)
            && let Ok(levels) = load_template_levels(&state.pool()?.get_conn(), tid).await
        {
            e.insert(levels);
        }
    }

    let items: Vec<serde_json::Value> = orgs
        .iter()
        .map(|org| {
            let org_type = org
                .template_id
                .and_then(|tid| template_levels_map.get(&tid))
                .and_then(|levels| resolve_type_name(levels, &org.type_path).ok())
                .unwrap_or_else(|| org.type_path.clone());

            json!({
                "id": org.id,
                "name": org.name,
                "type_path": org.type_path,
                "org_type": org_type,
                "parent_id": org.parent_id,
                "description": org.description,
                "template_id": org.template_id,
                "level_index": org.level_index,
                "created_at": org.created_at,
                "updated_at": org.updated_at,
            })
        })
        .collect();

    Ok(items)
}

/// 获取节点深度（从根到该节点的层数）
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
        let tree = build_tree(&[], &HashMap::new());
        assert!(tree.is_empty());
    }

    #[test]
    fn test_build_tree_single_root() {
        let root = Organization {
            id: Uuid::new_v4(),
            name: "总部".to_string(),
            type_path: "0".to_string(),
            parent_id: None,
            description: None,
            template_id: None,
            level_index: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let tree = build_tree(std::slice::from_ref(&root), &HashMap::new());
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
            type_path: "0".to_string(),
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
            type_path: "0.0".to_string(),
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
            type_path: "0.1".to_string(),
            parent_id: Some(root_id),
            description: None,
            template_id: None,
            level_index: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let tree = build_tree(&[root, child1, child2], &HashMap::new());
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].children.len(), 2);
    }

    #[test]
    fn test_hierarchy_depth_limit() {
        const {
            assert!(MAX_DEPTH >= 5);
        }
    }

    #[test]
    fn test_resolve_type_name() {
        let levels = serde_json::json!({
            "总部": ["a1公司", "b1公司"],
            "a1公司": [],
            "b1公司": []
        });

        // 根节点
        assert_eq!(resolve_type_name(&levels, "0").unwrap(), "总部");
        // 子节点
        assert_eq!(resolve_type_name(&levels, "0.0").unwrap(), "a1公司");
        assert_eq!(resolve_type_name(&levels, "0.1").unwrap(), "b1公司");
    }

    #[test]
    fn test_child_type_path() {
        assert_eq!(child_type_path("0", 0), "0.0");
        assert_eq!(child_type_path("0", 1), "0.1");
        assert_eq!(child_type_path("0.1", 2), "0.1.2");
    }

    #[test]
    fn test_parent_type_path() {
        assert_eq!(parent_type_path("0.1"), Some("0".to_string()));
        assert_eq!(parent_type_path("0.1.2"), Some("0.1".to_string()));
        assert_eq!(parent_type_path("0"), None);
    }

    #[test]
    fn test_type_path_index() {
        assert_eq!(type_path_index("0.1").unwrap(), 1);
        assert_eq!(type_path_index("0.1.2").unwrap(), 2);
        assert_eq!(type_path_index("0").unwrap(), 0);
    }
}
