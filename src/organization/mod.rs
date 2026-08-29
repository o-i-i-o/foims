//! 组织与人员模块（组织架构/员工/组织模板）。
//!
//! 与资源管理（`resource`）平级的业务模块：组织树的 CRUD 直接定义于
//! 本文件，员工挂在组织节点下（`employee`），组织类型模板见
//! `org_template`。

pub mod employee;
pub mod org_template;

pub use employee::*;
pub use org_template::*;

use crate::app_state::AppState;
use crate::models::{
    OrgTemplate, Organization, OrganizationCreate, OrganizationTreeNode, OrganizationUpdate, Room,
};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::{Pagination, paged_response};
use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use ipma_common::AppError;
use ipma_common::msg;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

// ==================== type_path 解析工具 ====================

/// 从模板的 levels JSON 中，根据 type_path 解析出类型名称
///
/// type_path 格式为点分隔的索引路径，如 "0"、"0.1"、"0.0.2"
/// 每个数字表示在该层级的子类型列表中的索引位置
pub fn resolve_type_name(levels: &serde_json::Value, type_path: &str) -> Result<String, AppError> {
    let levels_map = levels
        .as_object()
        .ok_or_else(|| AppError::Internal(msg("server.org_template.levels_format_invalid")))?;

    let indices: Vec<usize> = type_path
        .split('.')
        .map(|s| {
            s.parse::<usize>().map_err(|_| {
                AppError::Validation(
                    msg("server.organization.validation.type_path_format").with("path", type_path),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    if indices.is_empty() {
        return Err(AppError::Validation(msg(
            "server.organization.validation.type_path_required",
        )));
    }

    // 首段是根锚点，必须为 0（防止 "9.1" 之类的畸形路径被静默当作 "0.1" 解析）
    if indices[0] != 0 {
        return Err(AppError::Validation(
            msg("server.organization.validation.type_path_root_anchor").with("path", type_path),
        ));
    }

    // 找到根节点（不被任何其他节点的子级列表引用的节点）
    let root = find_root_in_levels(levels_map)?;

    // type_path 语义：
    //   "0"     → 根节点
    //   "0.x"   → 根节点的第 x 个子级
    //   "0.x.y" → 根节点的第 x 个子级的第 y 个子级
    // 第一个 "0" 是根锚点，后续索引逐级导航子级

    if indices.len() == 1 {
        return Ok(root.to_string());
    }

    // 从根的子级开始，跳过第一个锚点索引
    let mut current_type = root;
    for &idx in &indices[1..] {
        let children = levels_map
            .get(current_type)
            .and_then(|v| v.as_array())
            .ok_or_else(|| AppError::Internal(msg("server.org_template.levels_format_invalid")))?;

        current_type = children.get(idx).and_then(|v| v.as_str()).ok_or_else(|| {
            AppError::Validation(
                msg("server.organization.validation.type_path_out_of_range")
                    .with("path", type_path)
                    .with("index", idx),
            )
        })?;
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
    let last_segment = type_path.rsplit('.').next().ok_or_else(|| {
        AppError::Validation(
            msg("server.organization.validation.type_path_format").with("path", type_path),
        )
    })?;
    last_segment.parse::<usize>().map_err(|_| {
        AppError::Validation(
            msg("server.organization.validation.type_path_format").with("path", type_path),
        )
    })
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
        .ok_or_else(|| AppError::Internal(msg("server.org_template.root_not_found")))
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
        .ok_or_else(|| AppError::Internal(msg("server.organization.template_missing")))?;

    let levels: serde_json::Value =
        sqlx::query_scalar("SELECT levels FROM org_templates WHERE id = $1")
            .bind(template_id)
            .fetch_one(pool)
            .await
            .map_err(|_| AppError::Internal(msg("server.org_template.not_found")))?;

    resolve_type_name(&levels, &org.type_path)
}

/// 批量加载模板 levels（单条查询，避免逐模板 N+1）；加载失败时跳过该模板
async fn load_template_levels_batch(
    pool: &sqlx::PgPool,
    template_ids: &[Uuid],
) -> HashMap<Uuid, serde_json::Value> {
    if template_ids.is_empty() {
        return HashMap::new();
    }
    sqlx::query_as::<_, (Uuid, serde_json::Value)>(
        "SELECT id, levels FROM org_templates WHERE id = ANY($1)",
    )
    .bind(template_ids)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect()
}

// ==================== API 处理函数 ====================

/// 获取组织列表（支持按 parent_id 筛选）
pub async fn get_organizations(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();
    let parent_id_raw = query.get("parent_id").cloned();
    let parent_id = parent_id_raw
        .as_ref()
        .and_then(|id| Uuid::parse_str(id).ok());
    // parent_id 参数给了但解析失败时直接返回校验错误（避免误当全量查询）
    if parent_id_raw.is_some() && parent_id.is_none() {
        return Err(AppError::Validation(msg(
            "server.common.validation.uuid_invalid",
        )));
    }
    let root_only = query
        .get("root_only")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    // 两个筛选条件语义互斥，同时传入时明确报错而非静默忽略 root_only
    if parent_id.is_some() && root_only {
        return Err(AppError::Validation(msg(
            "server.organization.parent_root_only_conflict",
        )));
    }

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
            qb.push_bind(crate::utils::escape_like(search));
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

    Ok(ipma_common::ok_json(
        paged_response(items, total, &pagination),
        "server.organization.list_fetched",
    ))
}

/// 获取组织树形结构
pub async fn get_organization_tree(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    let all_orgs = sqlx::query_as::<_, Organization>(
        "SELECT id, name, type_path, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    // 批量加载所有涉及的模板 levels（单条查询）
    let template_ids: Vec<Uuid> = {
        let mut ids = std::collections::HashSet::new();
        for org in &all_orgs {
            if let Some(tid) = org.template_id {
                ids.insert(tid);
            }
        }
        ids.into_iter().collect()
    };
    let template_levels_map =
        load_template_levels_batch(&state.pool()?.get_conn(), &template_ids).await;

    let tree = build_tree(&all_orgs, &template_levels_map);
    Ok(ipma_common::ok_json(
        tree,
        "server.organization.tree_fetched",
    ))
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

    // 递归构建节点（带深度检查）；节点缺失时跳过（防御悬挂 parent_id 引用）
    fn build_node(
        org_id: Uuid,
        node_map: &HashMap<Uuid, &Organization>,
        children_map: &HashMap<Uuid, Vec<Uuid>>,
        template_levels_map: &HashMap<Uuid, serde_json::Value>,
        depth: usize,
    ) -> Option<OrganizationTreeNode> {
        let org = node_map.get(&org_id)?;

        // 从模板解析类型名称
        let org_type = org
            .template_id
            .and_then(|tid| template_levels_map.get(&tid))
            .and_then(|levels| resolve_type_name(levels, &org.type_path).ok())
            .unwrap_or_else(|| org.type_path.clone());

        // 深度安全检查
        if depth > MAX_ORG_DEPTH {
            return Some(OrganizationTreeNode {
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
            });
        }

        let children: Vec<OrganizationTreeNode> = children_map
            .get(&org_id)
            .map(|child_ids| {
                child_ids
                    .iter()
                    .filter_map(|id| {
                        build_node(*id, node_map, children_map, template_levels_map, depth + 1)
                    })
                    .collect()
            })
            .unwrap_or_default();

        Some(OrganizationTreeNode {
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
        })
    }

    // 构建根节点
    all_orgs
        .iter()
        .filter(|org| org.parent_id.is_none())
        .filter_map(|org| build_node(org.id, &node_map, &children_map, template_levels_map, 0))
        .collect()
}

/// 获取单个组织节点（含子节点）
pub async fn get_organization(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let org = sqlx::query_as::<_, Organization>(
        "SELECT id, name, type_path, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.organization.not_found")))?;

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

    Ok(ipma_common::ok_json(result, "server.organization.fetched"))
}

/// 创建组织节点
pub async fn create_organization(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<OrganizationCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

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
        .ok_or_else(|| AppError::NotFound(msg("server.organization.parent_not_found")))?;

        // 子节点继承父节点的模板
        let parent_template_id = parent.template_id.ok_or_else(|| {
            AppError::Validation(msg("server.organization.parent_template_missing"))
        })?;

        let parent_level = parent.level_index;

        // 获取模板定义；FOR UPDATE 与 update_org_template 互斥，防止本事务
        // 基于旧 levels 完成 type_path 校验后，模板更新并发提交新 levels，
        // 导致新节点在提交后的模板下无法解析（TOCTOU）
        let template: OrgTemplate = sqlx::query_as::<_, OrgTemplate>(
            "SELECT id, name, levels, icons, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
             FROM org_templates WHERE id = $1 FOR UPDATE",
        )
        .bind(parent_template_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::Validation(msg("server.org_template.not_found")))?;

        // 使用 type_path 校验
        let parent_type_name = resolve_type_name(&template.levels, &parent.type_path)?;
        let allowed_children = get_allowed_children(&template.levels, &parent_type_name)?;
        if allowed_children.is_empty() {
            return Err(AppError::Validation(
                msg("server.organization.type_children_forbidden")
                    .with("type", &parent_type_name)
                    .with("template", &template.name),
            ));
        }

        // 校验 type_path 是否在父节点的允许子级中
        let allowed_with_path =
            get_allowed_children_with_path(&template.levels, &parent.type_path)?;
        let is_valid = allowed_with_path
            .iter()
            .any(|(_, path)| path == &req.type_path);
        if !is_valid {
            let expected = allowed_with_path
                .iter()
                .map(|(_, p)| format!("「{p}」"))
                .collect::<Vec<_>>()
                .join("、");
            return Err(AppError::Validation(
                msg("server.organization.type_path_mismatch")
                    .with("template", &template.name)
                    .with("type", &parent_type_name)
                    .with("expected", expected)
                    .with("actual", &req.type_path),
            ));
        }

        let depth = get_depth(&mut tx, parent_id).await?;
        if depth >= MAX_ORG_DEPTH {
            return Err(AppError::Validation(
                msg("server.organization.depth_exceeded").with("max", MAX_ORG_DEPTH),
            ));
        }

        let duplicate: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM organizations WHERE parent_id = $1 AND name = $2")
                .bind(parent_id)
                .bind(&req.name)
                .fetch_optional(&mut *tx)
                .await?;
        if duplicate.is_some() {
            return Err(AppError::Conflict(msg("server.organization.name_exists")));
        }

        (Some(parent_template_id), parent_level + 1)
    } else {
        // 根节点：必须指定 template_id，type_path 必须为 "0"
        let template_id = req.template_id.ok_or_else(|| {
            AppError::Validation(msg("server.organization.root_template_required"))
        })?;

        // 锁定模板行，与模板更新/删除互斥（同子节点路径的并发防护）
        let template_exists: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM org_templates WHERE id = $1 FOR UPDATE")
                .bind(template_id)
                .fetch_optional(&mut *tx)
                .await?;
        if template_exists.is_none() {
            return Err(AppError::NotFound(msg("server.org_template.not_found")));
        }

        // 根节点 type_path 必须是 "0"
        if req.type_path != "0" {
            return Err(AppError::Validation(msg(
                "server.organization.root_type_path_invalid",
            )));
        }

        // 根节点同名查重（parent_id 为 NULL，应用层兜底；数据库侧由
        // UNIQUE NULLS NOT DISTINCT 约束保证并发安全）
        let duplicate: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM organizations WHERE parent_id IS NULL AND name = $1",
        )
        .bind(&req.name)
        .fetch_optional(&mut *tx)
        .await?;
        if duplicate.is_some() {
            return Err(AppError::Conflict(msg("server.organization.name_exists")));
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
    .await
    .map_err(|e| {
        // 并发创建同名节点时由唯一约束兜底，映射为友好冲突错误
        if let sqlx::Error::Database(db_err) = &e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict(msg("server.organization.name_exists"));
        }
        AppError::from(e)
    })?;

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
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "organization",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json(
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
        "server.organization.created",
    ))
}

/// 更新组织节点
///
/// 仅支持更新 name / description。
/// type_path 由模板结构决定，不允许通过此接口修改。
pub async fn update_organization(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<OrganizationUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing = sqlx::query_as::<_, Organization>(
        "SELECT id, name, type_path, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.organization.not_found")))?;

    // type_path 不允许通过此接口修改（由模板结构决定）
    if let Some(ref new_type_path) = req.type_path
        && new_type_path != &existing.type_path
    {
        return Err(AppError::Validation(msg(
            "server.organization.type_path_immutable",
        )));
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
            return Err(AppError::Conflict(msg("server.organization.name_exists")));
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
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "organization",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json(
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
        "server.organization.updated",
    ))
}

/// 删除组织节点
pub async fn delete_organization(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM organizations WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound(msg("server.organization.not_found")));
    }

    let child_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM organizations WHERE parent_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if child_count > 0 {
        return Err(AppError::Validation(
            msg("server.organization.has_children").with("count", child_count),
        ));
    }

    let room_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms WHERE org_id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if room_count > 0 {
        return Err(AppError::Validation(
            msg("server.organization.has_rooms").with("count", room_count),
        ));
    }

    sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "organization_id": id.to_string()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "organization",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json((), "server.organization.deleted"))
}

/// 获取指定节点的下级类型信息（基于模板）
pub async fn get_allowed_child_types(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let org = sqlx::query_as::<_, Organization>(
        "SELECT id, name, type_path, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.organization.not_found")))?;

    let template_id = org
        .template_id
        .ok_or_else(|| AppError::Validation(msg("server.organization.template_missing")))?;

    let template: OrgTemplate = sqlx::query_as::<_, OrgTemplate>(
        "SELECT id, name, levels, icons, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM org_templates WHERE id = $1",
    )
    .bind(template_id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::Validation(msg("server.org_template.not_found")))?;

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

    Ok(ipma_common::ok_json(
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
        "server.organization.allowed_child_types_fetched",
    ))
}

/// 获取指定父节点的子节点列表
pub async fn get_children(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM organizations WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound(msg("server.organization.not_found")));
    }

    let children = sqlx::query_as::<_, Organization>(
        "SELECT id, name, type_path, parent_id, description, template_id, level_index, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE parent_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let items = resolve_org_list_types(&state, &children).await?;

    Ok(ipma_common::ok_json(
        items,
        "server.organization.children_fetched",
    ))
}

// ==================== 内部辅助函数 ====================

/// 批量解析组织列表的 org_type
async fn resolve_org_list_types(
    state: &Arc<AppState>,
    orgs: &[Organization],
) -> Result<Vec<serde_json::Value>, AppError> {
    let template_ids: Vec<Uuid> = {
        let mut ids = std::collections::HashSet::new();
        for org in orgs {
            if let Some(tid) = org.template_id {
                ids.insert(tid);
            }
        }
        ids.into_iter().collect()
    };
    let template_levels_map =
        load_template_levels_batch(&state.pool()?.get_conn(), &template_ids).await;

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

    for _ in 0..=MAX_ORG_DEPTH {
        let parent_id: Option<Uuid> =
            sqlx::query_scalar("SELECT parent_id FROM organizations WHERE id = $1")
                .bind(current_id)
                .fetch_optional(&mut *conn)
                .await?
                .flatten();

        match parent_id {
            Some(pid) => {
                if !visited.insert(pid) {
                    return Err(AppError::Internal(
                        msg("server.organization.cycle_detected").with("id", pid),
                    ));
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
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM organizations WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound(msg("server.organization.not_found")));
    }
    let rooms = sqlx::query_as::<_, Room>(
        "SELECT id, name, room_type, org_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM rooms WHERE org_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;
    Ok(ipma_common::ok_json(
        rooms,
        "server.organization.rooms_fetched",
    ))
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
            assert!(MAX_ORG_DEPTH >= 5);
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
        // 首段锚点非 0 必须拒绝，不允许被静默当作根路径解析
        assert!(resolve_type_name(&levels, "1.0").is_err());
        assert!(resolve_type_name(&levels, "9").is_err());
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
