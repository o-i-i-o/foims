use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    ApiResponse, Node, NodeCreate, NodeTreeNode, NodeUpdate, NodeWithChildren, Room,
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

/// 校验节点类型层级关系：campus → building → floor
fn validate_node_type_hierarchy(parent_type: &str, child_type: &str) -> Result<(), AppError> {
    let valid = match parent_type {
        "campus" => child_type == "building",
        "building" => child_type == "floor",
        "floor" => false,
        _ => false,
    };
    if !valid {
        let parent_label = match parent_type {
            "campus" => "校区",
            "building" => "建筑",
            "floor" => "楼层",
            _ => parent_type,
        };
        let child_label = match child_type {
            "campus" => "校区",
            "building" => "建筑",
            "floor" => "楼层",
            _ => child_type,
        };
        return Err(AppError::Validation(format!(
            "{parent_label}下不能创建{child_label}类型的节点"
        )));
    }
    Ok(())
}

/// 获取节点列表（支持按 parent_id、node_type 和搜索筛选）
pub async fn get_nodes(
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
    let node_type = query.get("node_type").cloned();
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
    if node_type.is_some() {
        conditions.push(format!("node_type = ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let search_pattern = format!("%{search}%");

    let count_sql = format!("SELECT COUNT(*) FROM nodes {where_clause}");
    let mut count_query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));

    let list_sql = format!(
        "SELECT id, name, node_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM nodes {where_clause}
         ORDER BY created_at ASC LIMIT ${param_idx} OFFSET ${}",
        param_idx + 1
    );
    let mut list_query = sqlx::query_as::<_, Node>(sqlx::AssertSqlSafe(list_sql));

    if !search.is_empty() {
        count_query = count_query.bind(&search_pattern);
        list_query = list_query.bind(&search_pattern);
    }
    if let Some(pid) = parent_id {
        count_query = count_query.bind(pid);
        list_query = list_query.bind(pid);
    }
    if let Some(ref nt) = node_type {
        count_query = count_query.bind(nt);
        list_query = list_query.bind(nt);
    }

    let total: i64 = count_query.fetch_one(&state.pool()?.get_conn()).await?;

    let nodes = list_query
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": nodes,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "节点列表获取成功",
    )))
}

/// 获取节点树形结构
pub async fn get_node_tree(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let all_nodes = sqlx::query_as::<_, Node>(
        "SELECT id, name, node_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM nodes ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let tree = build_tree(&all_nodes);
    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<NodeTreeNode>>::success(
            tree,
            "节点树获取成功",
        )),
    )
}

/// 从扁平列表构建树形结构
fn build_tree(all_nodes: &[Node]) -> Vec<NodeTreeNode> {
    let mut children_map: HashMap<Option<Uuid>, Vec<&Node>> = HashMap::new();
    for node in all_nodes {
        children_map.entry(node.parent_id).or_default().push(node);
    }

    fn build_node(node: &Node, children_map: &HashMap<Option<Uuid>, Vec<&Node>>) -> NodeTreeNode {
        let children: Vec<NodeTreeNode> = children_map
            .get(&Some(node.id))
            .map(|childs| childs.iter().map(|c| build_node(c, children_map)).collect())
            .unwrap_or_default();

        NodeTreeNode {
            id: node.id,
            name: node.name.clone(),
            node_type: node.node_type.clone(),
            parent_id: node.parent_id,
            description: node.description.clone(),
            children,
            created_at: node.created_at,
            updated_at: node.updated_at,
        }
    }

    children_map
        .get(&None)
        .map(|roots| {
            roots
                .iter()
                .map(|node| build_node(node, &children_map))
                .collect()
        })
        .unwrap_or_default()
}

/// 获取单个节点（含父级名称、子节点、子节点数、关联房间数）
pub async fn get_node(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let node = sqlx::query_as::<_, Node>(
        "SELECT id, name, node_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM nodes WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("节点未找到".to_string()))?;

    let parent_name: Option<String> = if let Some(pid) = node.parent_id {
        sqlx::query_scalar("SELECT name FROM nodes WHERE id = $1")
            .bind(pid)
            .fetch_optional(&state.pool()?.get_conn())
            .await?
    } else {
        None
    };

    let children = sqlx::query_as::<_, Node>(
        "SELECT id, name, node_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM nodes WHERE parent_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let child_count: i64 = children.len() as i64;

    let room_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms WHERE node_id = $1")
        .bind(id)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let result = NodeWithChildren {
        id: node.id,
        name: node.name,
        node_type: node.node_type,
        parent_id: node.parent_id,
        parent_name,
        description: node.description,
        children,
        child_count,
        room_count,
        created_at: node.created_at,
        updated_at: node.updated_at,
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<NodeWithChildren>::success(
            result,
            "节点获取成功",
        )),
    )
}

/// 创建节点
pub async fn create_node(
    state: web::Data<AppState>,
    req: web::Json<NodeCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    if let Some(parent_id) = req.parent_id {
        // 验证父节点存在
        let parent: Node = sqlx::query_as::<_, Node>(
            "SELECT id, name, node_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
             FROM nodes WHERE id = $1",
        )
        .bind(parent_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("父级节点未找到".to_string()))?;

        // 校验节点类型层级关系：campus → building → floor
        validate_node_type_hierarchy(&parent.node_type, &req.node_type)?;

        // 检查深度限制
        let depth = get_depth(&mut tx, parent_id).await?;
        if depth >= MAX_DEPTH {
            return Err(AppError::Validation(format!(
                "已达到最大层级深度限制({MAX_DEPTH})"
            )));
        }

        // 同级下名称唯一性检查
        let duplicate: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM nodes WHERE parent_id = $1 AND name = $2")
                .bind(parent_id)
                .bind(&req.name)
                .fetch_optional(&mut *tx)
                .await?;
        if duplicate.is_some() {
            return Err(AppError::Conflict("同级下已存在同名节点".to_string()));
        }
    } else {
        // 根节点只能是 campus 类型
        if req.node_type != "campus" {
            return Err(AppError::Validation(
                "根节点只能是校区(campus)类型".to_string(),
            ));
        }

        // 根节点同级名称唯一性检查（parent_id IS NULL）
        let duplicate: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM nodes WHERE parent_id IS NULL AND name = $1")
                .bind(&req.name)
                .fetch_optional(&mut *tx)
                .await?;
        if duplicate.is_some() {
            return Err(AppError::Conflict("同级下已存在同名节点".to_string()));
        }
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO nodes (id, name, node_type, parent_id, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.node_type)
    .bind(req.parent_id)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let node = Node {
        id,
        name: req.name.clone(),
        node_type: req.node_type.clone(),
        parent_id: req.parent_id,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": node.name,
        "node_type": node.node_type,
        "parent_id": node.parent_id,
        "description": node.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
            resource_type: "node",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<Node>::success(node, "节点创建成功")))
}

/// 更新节点
pub async fn update_node(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<NodeUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    (*req).validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing = sqlx::query_as::<_, Node>(
        "SELECT id, name, node_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM nodes WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("节点未找到".to_string()))?;

    // 名称变更校验：同级下不能重名
    if let Some(ref new_name) = req.name
        && new_name != &existing.name
    {
        let duplicate: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM nodes WHERE parent_id IS NOT DISTINCT FROM $1 AND name = $2 AND id != $3",
        )
        .bind(existing.parent_id)
        .bind(new_name)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        if duplicate.is_some() {
            return Err(AppError::Conflict("同级下已存在同名节点".to_string()));
        }
    }

    // node_type 变更校验
    let new_node_type = req.node_type.as_deref().unwrap_or(&existing.node_type);
    if new_node_type != existing.node_type {
        // 检查子节点层级兼容性
        let child_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE parent_id = $1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if child_count > 0 {
            let incompatible_child_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM nodes WHERE parent_id = $1 AND NOT (\
                    ($2 = 'campus' AND node_type = 'building') OR \
                    ($2 = 'building' AND node_type = 'floor')\
                )",
            )
            .bind(id)
            .bind(new_node_type)
            .fetch_one(&mut *tx)
            .await?;
            if incompatible_child_count > 0 {
                return Err(AppError::Validation(
                    "修改节点类型后，现有子节点的层级关系不再合法".to_string(),
                ));
            }
        }
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE nodes SET
         name = COALESCE($1, name),
         node_type = COALESCE($2, node_type),
         description = COALESCE($3, description),
         updated_at = $4
         WHERE id = $5",
    )
    .bind(&req.name)
    .bind(&req.node_type)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let node = sqlx::query_as::<_, Node>(
        "SELECT id, name, node_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM nodes WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "name": node.name,
        "node_type": node.node_type,
        "parent_id": node.parent_id,
        "description": node.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
            resource_type: "node",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<Node>::success(node, "节点更新成功")))
}

/// 删除节点
pub async fn delete_node(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM nodes WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("节点未找到".to_string()));
    }

    let child_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE parent_id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if child_count > 0 {
        return Err(AppError::Validation(format!(
            "该节点下还有 {child_count} 个子节点，请先删除所有子节点"
        )));
    }

    let room_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms WHERE node_id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if room_count > 0 {
        return Err(AppError::Validation(format!(
            "该节点下还有 {room_count} 个关联房间，请先移除或变更所有关联房间"
        )));
    }

    sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "node_id": id.to_string()
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "node",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "节点删除成功")))
}

/// 获取指定节点的子节点列表
pub async fn get_node_children(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM nodes WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("节点未找到".to_string()));
    }

    let children = sqlx::query_as::<_, Node>(
        "SELECT id, name, node_type, parent_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM nodes WHERE parent_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::<Vec<Node>>::success(
        children,
        "子节点列表获取成功",
    )))
}

/// 获取节点下的房间列表
pub async fn get_node_rooms(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM nodes WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("节点未找到".to_string()));
    }

    let rooms = sqlx::query_as::<_, Room>(
        "SELECT id, name, room_type, node_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ
         FROM rooms WHERE node_id = $1 ORDER BY created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::<Vec<Room>>::success(
        rooms,
        "节点房间列表获取成功",
    )))
}

// ==================== 内部辅助函数 ====================

/// 获取节点深度（从根到该节点的层数）
async fn get_depth(conn: &mut sqlx::PgConnection, node_id: Uuid) -> Result<usize, AppError> {
    let mut depth = 0usize;
    let mut current_id = node_id;

    for _ in 0..=MAX_DEPTH {
        let parent_id: Option<Uuid> =
            sqlx::query_scalar("SELECT parent_id FROM nodes WHERE id = $1")
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
        let root = Node {
            id: Uuid::new_v4(),
            name: "主校区".to_string(),
            node_type: "campus".to_string(),
            parent_id: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let tree = build_tree(&[root.clone()]);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "主校区");
        assert!(tree[0].children.is_empty());
    }

    #[test]
    fn test_build_tree_with_children() {
        let root_id = Uuid::new_v4();
        let root = Node {
            id: root_id,
            name: "主校区".to_string(),
            node_type: "campus".to_string(),
            parent_id: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let child1 = Node {
            id: Uuid::new_v4(),
            name: "一号楼".to_string(),
            node_type: "building".to_string(),
            parent_id: Some(root_id),
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let child2 = Node {
            id: Uuid::new_v4(),
            name: "二号楼".to_string(),
            node_type: "building".to_string(),
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
    fn test_hierarchy_depth_limit() {
        assert!(MAX_DEPTH >= 5);
    }
}
