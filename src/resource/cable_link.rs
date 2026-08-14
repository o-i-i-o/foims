use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{CableLinkCreate, CableLinkUpdate, CableLinkWithDetails, CablePathNode};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::Pagination;
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

const VALID_ENDPOINT_TYPES: [&str; 4] = [
    "device_port",
    "net_outlet",
    "device_interface",
    "patch_panel",
];
const VALID_LINK_TYPES: [&str; 3] = ["ethernet", "fiber", "console"];

fn validate_endpoint_type(endpoint_type: &str) -> Result<(), AppError> {
    if VALID_ENDPOINT_TYPES.contains(&endpoint_type) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "端点类型必须是以下之一: {}",
            VALID_ENDPOINT_TYPES.join(", ")
        )))
    }
}

fn validate_link_type(link_type: &str) -> Result<(), AppError> {
    if VALID_LINK_TYPES.contains(&link_type) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "链路类型必须是以下之一: {}",
            VALID_LINK_TYPES.join(", ")
        )))
    }
}

fn sort_endpoints(
    a_type: &str,
    a_id: Uuid,
    b_type: &str,
    b_id: Uuid,
) -> (String, Uuid, String, Uuid) {
    if a_type < b_type || (a_type == b_type && a_id < b_id) {
        (a_type.to_string(), a_id, b_type.to_string(), b_id)
    } else {
        (b_type.to_string(), b_id, a_type.to_string(), a_id)
    }
}

pub async fn get_cable_links(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let endpoint_type = query.get("endpoint_type").cloned();
    let endpoint_id = query
        .get("endpoint_id")
        .and_then(|s| Uuid::parse_str(s).ok());
    let link_type = query.get("link_type").cloned();
    let sort_by = query.get("sort_by").cloned().unwrap_or_default();
    let sort_order = query.get("sort_order").cloned().unwrap_or_default();

    // ORDER BY 白名单，未匹配时回落默认序，避免注入
    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("link_type", "desc") => "ORDER BY cl.link_type DESC, cl.updated_at DESC",
        ("link_type", _) => "ORDER BY cl.link_type ASC, cl.updated_at DESC",
        ("cable_label", "desc") => "ORDER BY cl.cable_label DESC NULLS LAST, cl.updated_at DESC",
        ("cable_label", _) => "ORDER BY cl.cable_label ASC NULLS LAST, cl.updated_at DESC",
        ("length_m", "desc") => "ORDER BY cl.length_m DESC NULLS LAST, cl.updated_at DESC",
        ("length_m", _) => "ORDER BY cl.length_m ASC NULLS LAST, cl.updated_at DESC",
        ("tested", "desc") => "ORDER BY cl.tested DESC, cl.updated_at DESC",
        ("tested", _) => "ORDER BY cl.tested ASC, cl.updated_at DESC",
        ("updated_at", "asc") => "ORDER BY cl.updated_at ASC",
        _ => "ORDER BY cl.updated_at DESC",
    };

    let mut conditions: Vec<String> = Vec::new();
    let mut param_idx = 1;

    let has_endpoint_filter = endpoint_type.is_some() && endpoint_id.is_some();

    if has_endpoint_filter {
        let p1 = param_idx;
        let p2 = param_idx + 1;
        conditions.push(format!(
            "((cl.a_endpoint_type = ${p1} AND cl.a_endpoint_id = ${p2}) OR (cl.b_endpoint_type = ${p1} AND cl.b_endpoint_id = ${p2}))"
        ));
        param_idx += 2;
    }

    if link_type.is_some() {
        conditions.push(format!("cl.link_type = ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_sql = sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM cable_links cl {where_clause}"
    ));
    let data_sql = sqlx::AssertSqlSafe(format!(
        "SELECT cl.id, cl.a_endpoint_type, cl.a_endpoint_id, cl.a_endpoint_label, \
         cl.a_room_id, cl.a_cabinet_id, cl.a_device_id, \
         cl.b_endpoint_type, cl.b_endpoint_id, cl.b_endpoint_label, \
         cl.b_room_id, cl.b_cabinet_id, cl.b_device_id, \
         cl.link_type, cl.cable_label, cl.length_m, cl.tested, \
         cl.created_at::TIMESTAMPTZ, cl.updated_at::TIMESTAMPTZ \
         FROM cable_links_with_details cl {where_clause} \
         {order_clause} LIMIT ${param_idx} OFFSET ${}",
        param_idx + 1
    ));

    let mut count_query = sqlx::query_scalar::<_, i64>(count_sql);
    let mut data_query = sqlx::query_as::<_, CableLinkWithDetails>(data_sql);

    if let Some(ref et) = endpoint_type
        && let Some(eid) = endpoint_id
    {
        count_query = count_query.bind(et).bind(eid).bind(et).bind(eid);
        data_query = data_query.bind(et).bind(eid).bind(et).bind(eid);
    }

    if let Some(ref lt) = link_type {
        count_query = count_query.bind(lt);
        data_query = data_query.bind(lt);
    }

    let total: i64 = count_query.fetch_one(&state.pool()?.get_conn()).await?;
    data_query = data_query.bind(page_size as i32).bind(offset as i32);
    let links = data_query.fetch_all(&state.pool()?.get_conn()).await?;

    Ok(crate::error::ok_json(
        json!({
            "items": links,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "物理链路列表获取成功",
    ))
}

pub async fn create_cable_link(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<CableLinkCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    validate_endpoint_type(&req.a_endpoint_type)?;
    validate_endpoint_type(&req.b_endpoint_type)?;

    if req.a_endpoint_type == req.b_endpoint_type && req.a_endpoint_id == req.b_endpoint_id {
        return Err(AppError::Validation("不允许自连接链路".to_string()));
    }

    if req.a_endpoint_type == "device_interface" && req.b_endpoint_type == "device_interface" {
        return Err(AppError::Validation(
            "不允许两台设备直连，必须经过交换机或信息点".to_string(),
        ));
    }

    let link_type = req.link_type.as_deref().unwrap_or("ethernet");
    validate_link_type(link_type)?;

    let (a_type, a_id, b_type, b_id) = sort_endpoints(
        &req.a_endpoint_type,
        req.a_endpoint_id,
        &req.b_endpoint_type,
        req.b_endpoint_id,
    );

    let id = Uuid::new_v4();
    let now = Utc::now();
    let tested = req.tested.unwrap_or(false);

    sqlx::query(
        "INSERT INTO cable_links (id, a_endpoint_type, a_endpoint_id, b_endpoint_type, b_endpoint_id, link_type, cable_label, length_m, tested, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(id)
    .bind(&a_type)
    .bind(a_id)
    .bind(&b_type)
    .bind(b_id)
    .bind(link_type)
    .bind(&req.cable_label)
    .bind(req.length_m)
    .bind(tested)
    .bind(now)
    .bind(now)
    .execute(&state.pool()?.get_conn())
    .await?;

    let link = sqlx::query_as::<_, CableLinkWithDetails>(
        "SELECT cl.id, cl.a_endpoint_type, cl.a_endpoint_id, cl.a_endpoint_label, \
         cl.a_room_id, cl.a_cabinet_id, cl.a_device_id, \
         cl.b_endpoint_type, cl.b_endpoint_id, cl.b_endpoint_label, \
         cl.b_room_id, cl.b_cabinet_id, cl.b_device_id, \
         cl.link_type, cl.cable_label, cl.length_m, cl.tested, \
         cl.created_at::TIMESTAMPTZ, cl.updated_at::TIMESTAMPTZ \
         FROM cable_links_with_details cl WHERE cl.id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "a_endpoint_type": a_type,
        "a_endpoint_id": a_id,
        "b_endpoint_type": b_type,
        "b_endpoint_id": b_id,
        "link_type": link_type
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "cable_link",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(link, "物理链路创建成功"))
}

pub async fn get_cable_link(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let link = sqlx::query_as::<_, CableLinkWithDetails>(
        "SELECT cl.id, cl.a_endpoint_type, cl.a_endpoint_id, cl.a_endpoint_label, \
         cl.a_room_id, cl.a_cabinet_id, cl.a_device_id, \
         cl.b_endpoint_type, cl.b_endpoint_id, cl.b_endpoint_label, \
         cl.b_room_id, cl.b_cabinet_id, cl.b_device_id, \
         cl.link_type, cl.cable_label, cl.length_m, cl.tested, \
         cl.created_at::TIMESTAMPTZ, cl.updated_at::TIMESTAMPTZ \
         FROM cable_links_with_details cl WHERE cl.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("物理链路未找到".to_string()))?;

    Ok(crate::error::ok_json(link, "物理链路获取成功"))
}

pub async fn update_cable_link(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<CableLinkUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    if let Some(ref lt) = req.link_type {
        validate_link_type(lt)?;
    }

    // 端点更新：四字段必须同时提供，参照 create 校验并按规范序排序
    let endpoint_changed = req.a_endpoint_type.is_some()
        && req.a_endpoint_id.is_some()
        && req.b_endpoint_type.is_some()
        && req.b_endpoint_id.is_some();

    let (a_type, a_id, b_type, b_id) = if endpoint_changed {
        let a_t = req.a_endpoint_type.as_deref().unwrap();
        let b_t = req.b_endpoint_type.as_deref().unwrap();
        validate_endpoint_type(a_t)?;
        validate_endpoint_type(b_t)?;
        let a_i = req.a_endpoint_id.unwrap();
        let b_i = req.b_endpoint_id.unwrap();
        if a_t == b_t && a_i == b_i {
            return Err(AppError::Validation("不允许自连接链路".to_string()));
        }
        if a_t == "device_interface" && b_t == "device_interface" {
            return Err(AppError::Validation(
                "不允许两台设备直连，必须经过交换机或信息点".to_string(),
            ));
        }
        sort_endpoints(a_t, a_i, b_t, b_i)
    } else if req.a_endpoint_type.is_some()
        || req.a_endpoint_id.is_some()
        || req.b_endpoint_type.is_some()
        || req.b_endpoint_id.is_some()
    {
        return Err(AppError::Validation(
            "更新端点时 A/B 两端的类型与 id 必须同时提供".to_string(),
        ));
    } else {
        (String::new(), Uuid::nil(), String::new(), Uuid::nil())
    };

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM cable_links WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("物理链路未找到".to_string()));
    }

    let mut set_clauses: Vec<String> = Vec::new();
    let mut param_index = 1;

    if req.link_type.is_some() {
        set_clauses.push(format!("link_type = ${param_index}"));
        param_index += 1;
    }

    if req.cable_label.is_some() {
        set_clauses.push(format!(
            "cable_label = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE cable_label END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    if req.length_m.is_some() {
        set_clauses.push(format!(
            "length_m = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE length_m END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    if req.tested.is_some() {
        set_clauses.push(format!("tested = ${param_index}"));
        param_index += 1;
    }

    if endpoint_changed {
        set_clauses.push(format!("a_endpoint_type = ${param_index}"));
        param_index += 1;
        set_clauses.push(format!("a_endpoint_id = ${param_index}"));
        param_index += 1;
        set_clauses.push(format!("b_endpoint_type = ${param_index}"));
        param_index += 1;
        set_clauses.push(format!("b_endpoint_id = ${param_index}"));
        param_index += 1;
    }

    if set_clauses.is_empty() {
        return Err(AppError::Validation("没有需要更新的字段".to_string()));
    }

    set_clauses.push(format!("updated_at = ${param_index}"));
    param_index += 1;
    let where_param = param_index;

    let sql = format!(
        "UPDATE cable_links SET {} WHERE id = ${}",
        set_clauses.join(", "),
        where_param
    );

    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));

    if let Some(ref lt) = req.link_type {
        query = query.bind(lt);
    }

    if req.cable_label.is_some() {
        match &req.cable_label {
            Some(Some(label)) => {
                query = query.bind(true);
                query = query.bind(label);
            }
            Some(None) => {
                query = query.bind(true);
                query = query.bind(Option::<String>::None);
            }
            None => unreachable!(),
        }
    }

    if req.length_m.is_some() {
        match req.length_m {
            Some(Some(len)) => {
                query = query.bind(true);
                query = query.bind(Some(len));
            }
            Some(None) => {
                query = query.bind(true);
                query = query.bind(Option::<f64>::None);
            }
            None => unreachable!(),
        }
    }

    if let Some(tested) = req.tested {
        query = query.bind(tested);
    }

    if endpoint_changed {
        query = query.bind(&a_type);
        query = query.bind(a_id);
        query = query.bind(&b_type);
        query = query.bind(b_id);
    }

    let now = Utc::now();
    query = query.bind(now);
    query = query.bind(id);

    query.execute(&mut *tx).await?;

    tx.commit().await?;

    let link = sqlx::query_as::<_, CableLinkWithDetails>(
        "SELECT cl.id, cl.a_endpoint_type, cl.a_endpoint_id, cl.a_endpoint_label, \
         cl.a_room_id, cl.a_cabinet_id, cl.a_device_id, \
         cl.b_endpoint_type, cl.b_endpoint_id, cl.b_endpoint_label, \
         cl.b_room_id, cl.b_cabinet_id, cl.b_device_id, \
         cl.link_type, cl.cable_label, cl.length_m, cl.tested, \
         cl.created_at::TIMESTAMPTZ, cl.updated_at::TIMESTAMPTZ \
         FROM cable_links_with_details cl WHERE cl.id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({ "cable_link_id": id.to_string() });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "cable_link",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(link, "物理链路更新成功"))
}

pub async fn delete_cable_link(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM cable_links WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("物理链路未找到".to_string()));
    }

    sqlx::query("DELETE FROM cable_links WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({ "cable_link_id": id.to_string() });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "cable_link",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json((), "物理链路删除成功"))
}

#[derive(Debug, serde::Deserialize)]
pub struct CablePathQuery {
    pub from_type: String,
    pub from_id: Uuid,
    pub to_type: String,
    pub to_id: Uuid,
}

pub async fn get_cable_path(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CablePathQuery>,
) -> Result<Response, AppError> {
    validate_endpoint_type(&q.from_type)?;
    validate_endpoint_type(&q.to_type)?;

    let rows = sqlx::query(
        "SELECT hop_idx, node_type, node_id, node_label, cable_id, cable_label, hop_type \
         FROM find_cable_path($1, $2, $3, $4) ORDER BY hop_idx",
    )
    .bind(&q.from_type)
    .bind(q.from_id)
    .bind(&q.to_type)
    .bind(q.to_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let path: Vec<CablePathNode> = rows
        .into_iter()
        .map(|r| CablePathNode {
            hop_idx: r.get::<i32, _>("hop_idx"),
            node_type: r.get::<String, _>("node_type"),
            node_id: r.get::<Uuid, _>("node_id"),
            node_label: r.get::<Option<String>, _>("node_label"),
            cable_id: r.get::<Option<Uuid>, _>("cable_id"),
            cable_label: r.get::<Option<String>, _>("cable_label"),
            hop_type: r.get::<String, _>("hop_type"),
        })
        .collect();

    if path.is_empty() {
        return Err(AppError::NotFound("未找到连接路径".to_string()));
    }

    Ok(crate::error::ok_json(
        json!({ "path": path, "hop_count": path.len() }),
        "链路路径查询成功",
    ))
}
