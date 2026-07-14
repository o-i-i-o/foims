//
// +3../ ;3'
//
//

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    ApiResponse, CableLinkCreate, CableLinkUpdate, CableLinkWithDetails, CablePathNode,
};
use crate::utils::pagination::Pagination;
use crate::utils::{OperationLogParams, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use tracing::warn;
use uuid::Uuid;
use validator::Validate;

const VALID_ENDPOINT_TYPES: [&str; 3] = ["switch_port", "net_outlet", "device_interface"];
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
    state: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let endpoint_type = query.get("endpoint_type").cloned();
    let endpoint_id = query
        .get("endpoint_id")
        .and_then(|s| Uuid::parse_str(s).ok());
    let link_type = query.get("link_type").cloned();

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
         cl.b_endpoint_type, cl.b_endpoint_id, cl.b_endpoint_label, \
         cl.link_type, cl.cable_label, cl.length_m, cl.tested, \
         cl.created_at::TIMESTAMPTZ, cl.updated_at::TIMESTAMPTZ \
         FROM cable_links_with_details cl {where_clause} \
         ORDER BY cl.updated_at DESC LIMIT ${param_idx} OFFSET ${}",
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

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": links,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "物理链路列表获取成功",
    )))
}

pub async fn create_cable_link(
    state: web::Data<AppState>,
    req: web::Json<CableLinkCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

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
         cl.b_endpoint_type, cl.b_endpoint_id, cl.b_endpoint_label, \
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
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
            resource_type: "cable_link",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(link, "物理链路创建成功")))
}

pub async fn get_cable_link(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let link = sqlx::query_as::<_, CableLinkWithDetails>(
        "SELECT cl.id, cl.a_endpoint_type, cl.a_endpoint_id, cl.a_endpoint_label, \
         cl.b_endpoint_type, cl.b_endpoint_id, cl.b_endpoint_label, \
         cl.link_type, cl.cable_label, cl.length_m, cl.tested, \
         cl.created_at::TIMESTAMPTZ, cl.updated_at::TIMESTAMPTZ \
         FROM cable_links_with_details cl WHERE cl.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("物理链路未找到".to_string()))?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(link, "物理链路获取成功")))
}

pub async fn update_cable_link(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<CableLinkUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    (*req).validate()?;

    if let Some(ref lt) = req.link_type {
        validate_link_type(lt)?;
    }

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

    let now = Utc::now();
    query = query.bind(now);
    query = query.bind(id);

    query.execute(&mut *tx).await?;

    tx.commit().await?;

    let link = sqlx::query_as::<_, CableLinkWithDetails>(
        "SELECT cl.id, cl.a_endpoint_type, cl.a_endpoint_id, cl.a_endpoint_label, \
         cl.b_endpoint_type, cl.b_endpoint_id, cl.b_endpoint_label, \
         cl.link_type, cl.cable_label, cl.length_m, cl.tested, \
         cl.created_at::TIMESTAMPTZ, cl.updated_at::TIMESTAMPTZ \
         FROM cable_links_with_details cl WHERE cl.id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({ "cable_link_id": id.to_string() });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
            resource_type: "cable_link",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(link, "物理链路更新成功")))
}

pub async fn delete_cable_link(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

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
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "cable_link",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "物理链路删除成功")))
}

#[derive(Debug, serde::Deserialize)]
pub struct CablePathQuery {
    pub from_type: String,
    pub from_id: Uuid,
    pub to_type: String,
    pub to_id: Uuid,
}

pub async fn get_cable_path(
    state: web::Data<AppState>,
    query: web::Query<CablePathQuery>,
) -> Result<HttpResponse, AppError> {
    let q = query.into_inner();

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

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({ "path": path, "hop_count": path.len() }),
        "链路路径查询成功",
    )))
}
