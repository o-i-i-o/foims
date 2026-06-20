use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    ApiResponse, OrgType, Organization, OrganizationCreate, OrganizationTreeNode,
    OrganizationUpdate, OrganizationWithChildren,
};
use crate::utils::pagination::DEFAULT_PAGE;
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
    let page: i64 = query
        .get("page")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PAGE);
    let page_size: i64 = query
        .get("page_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let search = query.get("search").cloned().unwrap_or_default();
    let parent_id = query
        .get("parent_id")
        .and_then(|id| Uuid::parse_str(id).ok());
    let org_type = query.get("org_type").cloned();
    let root_only = query
        .get("root_only")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let offset = (page - 1) * page_size;

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
        "SELECT id, name, org_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
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
        "SELECT id, name, org_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
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
        "SELECT id, name, org_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
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
        "SELECT id, name, org_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE parent_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let child_count: i64 = children.len() as i64;

    let result = OrganizationWithChildren {
        id: org.id,
        name: org.name,
        org_type: org.org_type,
        parent_id: org.parent_id,
        parent_name,
        description: org.description,
        children,
        child_count,
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

    let req_org_type = OrgType::from_str_value(&req.org_type)
        .ok_or_else(|| AppError::Validation("无效的组织类型".to_string()))?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    if let Some(parent_id) = req.parent_id {
        let parent: Organization = sqlx::query_as::<_, Organization>(
            "SELECT id, name, org_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
             FROM organizations WHERE id = $1",
        )
        .bind(parent_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("父级组织节点未找到".to_string()))?;

        let parent_type = OrgType::from_str_value(&parent.org_type)
            .ok_or_else(|| AppError::Validation("父级组织类型无效".to_string()))?;

        if !parent_type.can_have_child(&req_org_type) {
            return Err(AppError::Validation(format!(
                "类型「{}」不允许作为类型「{}」的下级，合法下级类型为: {}",
                req.org_type,
                parent.org_type,
                parent_type
                    .allowed_child_types()
                    .iter()
                    .map(|t| t.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
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
    } else {
        if req_org_type != OrgType::Headquarters {
            return Err(AppError::Validation(
                "只有总部(headquarters)类型可以作为顶级节点".to_string(),
            ));
        }
        let root_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM organizations WHERE parent_id IS NULL")
                .fetch_one(&mut *tx)
                .await?;
        if root_count > 0 {
            return Err(AppError::Validation(
                "已存在顶级节点，只能有一个总部".to_string(),
            ));
        }
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO organizations (id, name, org_type, parent_id, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.org_type)
    .bind(req.parent_id)
    .bind(&req.description)
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
        created_at: now,
        updated_at: now,
    };

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
            action: "create",
            resource_type: "organization",
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
        HttpResponse::Ok().json(ApiResponse::<Organization>::success(
            org,
            "组织节点创建成功",
        )),
    )
}

/// 更新组织节点
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
        "SELECT id, name, org_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("组织节点未找到".to_string()))?;

    if let Some(Some(pid)) = req.parent_id {
        if pid == id {
            return Err(AppError::Validation("不能将节点的父级设为自身".to_string()));
        }

        if would_create_cycle(&mut tx, id, pid).await? {
            return Err(AppError::Validation(
                "操作会导致循环引用，不允许将节点移动到其子级下".to_string(),
            ));
        }

        let parent: Organization = sqlx::query_as::<_, Organization>(
            "SELECT id, name, org_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
             FROM organizations WHERE id = $1",
        )
        .bind(pid)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("新的父级组织节点未找到".to_string()))?;

        let effective_type = req.org_type.as_ref().unwrap_or(&existing.org_type);
        let effective_org_type = OrgType::from_str_value(effective_type)
            .ok_or_else(|| AppError::Validation("无效的组织类型".to_string()))?;
        let parent_type = OrgType::from_str_value(&parent.org_type)
            .ok_or_else(|| AppError::Validation("父级组织类型无效".to_string()))?;

        if !parent_type.can_have_child(&effective_org_type) {
            return Err(AppError::Validation(format!(
                "类型「{}」不允许作为类型「{}」的下级",
                effective_type, parent.org_type
            )));
        }

        let depth = get_depth(&mut tx, pid).await?;
        if depth >= MAX_DEPTH {
            return Err(AppError::Validation(format!(
                "已达到最大层级深度限制({MAX_DEPTH})"
            )));
        }
    }

    if let Some(ref new_name) = req.name {
        let effective_parent = match req.parent_id {
            Some(Some(pid)) => Some(pid),
            Some(None) => None,
            None => existing.parent_id,
        };

        let duplicate: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM organizations WHERE parent_id IS NOT DISTINCT FROM $1 AND name = $2 AND id != $3",
        )
        .bind(effective_parent)
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
         parent_id = COALESCE($3, parent_id),
         description = COALESCE($4, description),
         updated_at = $5
         WHERE id = $6",
    )
    .bind(&req.name)
    .bind(&req.org_type)
    .bind(req.parent_id)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let org = sqlx::query_as::<_, Organization>(
        "SELECT id, name, org_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
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
        HttpResponse::Ok().json(ApiResponse::<Organization>::success(
            org,
            "组织节点更新成功",
        )),
    )
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
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "组织节点删除成功")))
}

/// 获取指定类型的允许下级类型
pub async fn get_allowed_child_types(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let org = sqlx::query_as::<_, Organization>(
        "SELECT id, name, org_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM organizations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("组织节点未找到".to_string()))?;

    let org_type = OrgType::from_str_value(&org.org_type)
        .ok_or_else(|| AppError::Validation("无效的组织类型".to_string()))?;

    let allowed: Vec<serde_json::Value> = org_type
        .allowed_child_types()
        .iter()
        .map(|t| {
            json!({
                "type": t.as_str(),
                "label": org_type_label(t.as_str())
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "parent_id": id,
            "parent_name": org.name,
            "parent_type": org.org_type,
            "allowed_child_types": allowed
        }),
        "允许的下级类型获取成功",
    )))
}

/// 获取所有组织类型的允许下级类型映射（用于前端预定义）
pub async fn get_org_type_schema() -> HttpResponse {
    let all_types = [
        OrgType::Headquarters,
        OrgType::Building,
        OrgType::Floor,
        OrgType::Hall,
        OrgType::Office,
        OrgType::DataCenter,
        OrgType::Workstation,
        OrgType::Cabinet,
        OrgType::CabinetPosition,
    ];

    let schema: Vec<serde_json::Value> = all_types
        .iter()
        .map(|t| {
            json!({
                "type": t.as_str(),
                "label": org_type_label(t.as_str()),
                "allowed_children": t.allowed_child_types().iter().map(|c| {
                    json!({
                        "type": c.as_str(),
                        "label": org_type_label(c.as_str())
                    })
                }).collect::<Vec<_>>()
            })
        })
        .collect();

    HttpResponse::Ok().json(ApiResponse::success(
        json!({ "schema": schema }),
        "组织类型结构获取成功",
    ))
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
        "SELECT id, name, org_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
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
async fn get_depth(conn: &mut sqlx::PgConnection, node_id: Uuid) -> Result<usize, AppError> {
    let mut depth = 0usize;
    let mut current_id = node_id;

    for _ in 0..=MAX_DEPTH {
        let parent_id: Option<Uuid> =
            sqlx::query_scalar("SELECT parent_id FROM organizations WHERE id = $1")
                .bind(current_id)
                .fetch_optional(&mut *conn)
                .await?
                .flatten();

        match parent_id {
            Some(pid) => {
                depth += 1;
                current_id = pid;
            }
            None => break,
        }
    }

    Ok(depth)
}

/// 检查将 node 移动到 new_parent 下是否会形成循环引用
async fn would_create_cycle(
    conn: &mut sqlx::PgConnection,
    node_id: Uuid,
    new_parent_id: Uuid,
) -> Result<bool, AppError> {
    let mut current = new_parent_id;
    for _ in 0..=MAX_DEPTH {
        if current == node_id {
            return Ok(true);
        }
        let parent: Option<Uuid> =
            sqlx::query_scalar("SELECT parent_id FROM organizations WHERE id = $1")
                .bind(current)
                .fetch_optional(&mut *conn)
                .await?
                .flatten();
        match parent {
            Some(pid) => current = pid,
            None => break,
        }
    }
    Ok(false)
}

/// 组织类型中文标签
fn org_type_label(type_str: &str) -> &'static str {
    match type_str {
        "headquarters" => "总部",
        "building" => "楼号",
        "floor" => "楼层",
        "hall" => "大厅",
        "office" => "办公室",
        "data_center" => "机房",
        "workstation" => "工位",
        "cabinet" => "机柜",
        "cabinet_position" => "机位",
        _ => "未知",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_org_type_allowed_children() {
        assert_eq!(
            OrgType::Headquarters.allowed_child_types(),
            vec![OrgType::Building]
        );
        assert_eq!(
            OrgType::Floor.allowed_child_types(),
            vec![OrgType::Hall, OrgType::Office, OrgType::DataCenter]
        );
        assert!(OrgType::Workstation.allowed_child_types().is_empty());
        assert!(OrgType::CabinetPosition.allowed_child_types().is_empty());
    }

    #[test]
    fn test_can_have_child() {
        assert!(OrgType::Headquarters.can_have_child(&OrgType::Building));
        assert!(!OrgType::Headquarters.can_have_child(&OrgType::Floor));
        assert!(OrgType::Floor.can_have_child(&OrgType::Hall));
        assert!(OrgType::Floor.can_have_child(&OrgType::Office));
        assert!(OrgType::Floor.can_have_child(&OrgType::DataCenter));
        assert!(!OrgType::Floor.can_have_child(&OrgType::Cabinet));
        assert!(OrgType::DataCenter.can_have_child(&OrgType::Cabinet));
        assert!(OrgType::Cabinet.can_have_child(&OrgType::CabinetPosition));
        assert!(!OrgType::Cabinet.can_have_child(&OrgType::Workstation));
    }

    #[test]
    fn test_org_type_from_str() {
        assert_eq!(
            OrgType::from_str_value("headquarters"),
            Some(OrgType::Headquarters)
        );
        assert_eq!(
            OrgType::from_str_value("data_center"),
            Some(OrgType::DataCenter)
        );
        assert_eq!(OrgType::from_str_value("invalid"), None);
    }

    #[test]
    fn test_org_type_as_str() {
        assert_eq!(OrgType::Headquarters.as_str(), "headquarters");
        assert_eq!(OrgType::CabinetPosition.as_str(), "cabinet_position");
    }

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
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let tree = build_tree(&[root.clone()]);
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let child1 = Organization {
            id: Uuid::new_v4(),
            name: "一号楼".to_string(),
            org_type: "building".to_string(),
            parent_id: Some(root_id),
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let child2 = Organization {
            id: Uuid::new_v4(),
            name: "二号楼".to_string(),
            org_type: "building".to_string(),
            parent_id: Some(root_id),
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let tree = build_tree(&[root, child1, child2]);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].children.len(), 2);
    }

    #[test]
    fn test_org_type_label() {
        assert_eq!(org_type_label("headquarters"), "总部");
        assert_eq!(org_type_label("building"), "楼号");
        assert_eq!(org_type_label("floor"), "楼层");
        assert_eq!(org_type_label("data_center"), "机房");
        assert_eq!(org_type_label("unknown_type"), "未知");
    }

    #[test]
    fn test_hierarchy_depth_limit() {
        assert!(MAX_DEPTH >= 5);
    }
}
