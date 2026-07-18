use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    ApiResponse, NetOutlet, NetOutletCreate, NetOutletUpdate, NetOutletWithDetails,
};
use crate::utils::pagination::Pagination;
use crate::utils::{OperationLogParams, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use tracing::warn;
use uuid::Uuid;
use validator::Validate;

pub async fn get_net_outlets(
    state: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();
    let room_id = query.get("room_id").cloned();
    let outlet_type = query.get("outlet_type").cloned();
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());

    let escaped = search
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let search_pattern = format!("%{escaped}%");
    let parsed_room_id = room_id
        .as_ref()
        .map(|id| {
            Uuid::parse_str(id).map_err(|_| AppError::Validation("无效的room_id参数".to_string()))
        })
        .transpose()?;

    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY ap.name DESC",
        ("outlet_type", "desc") => "ORDER BY ap.outlet_type DESC, ap.name ASC",
        ("outlet_type", _) => "ORDER BY ap.outlet_type ASC, ap.name ASC",
        ("created_at", "desc") => "ORDER BY ap.created_at DESC",
        ("created_at", _) => "ORDER BY ap.created_at ASC",
        _ => "ORDER BY ap.name ASC",
    };

    let has_room_filter = parsed_room_id.is_some();
    let has_type_filter = !outlet_type.as_ref().is_none_or(|t| t.is_empty());
    let has_search = !search.is_empty();

    let mut where_parts: Vec<String> = Vec::new();
    let mut param_idx = 1;

    if has_search {
        where_parts.push(format!(
            "(ap.name ILIKE ${param_idx} OR ap.description ILIKE ${param_idx} OR ap.outlet_type ILIKE ${param_idx})"
        ));
        param_idx += 1;
    }
    if has_room_filter {
        where_parts.push(format!("ap.room_id = ${param_idx}"));
        param_idx += 1;
    }
    if has_type_filter {
        where_parts.push(format!("ap.outlet_type = ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_parts.join(" AND "))
    };

    let count_sql = sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM net_outlets_with_details ap {where_clause}"
    ));
    let data_sql = sqlx::AssertSqlSafe(format!(
        "SELECT ap.id, ap.name, ap.outlet_type, ap.room_id, ap.room_name, \
         ap.cabinet_id, ap.cabinet_name, \
         ap.description, \
         ap.peer_type, ap.peer_room_id, ap.peer_outlet_id, ap.peer_switch_port_id, \
         ap.peer_room_name, ap.peer_outlet_name, ap.peer_switch_port_label, \
         ap.created_at::TIMESTAMPTZ, ap.updated_at::TIMESTAMPTZ \
         FROM net_outlets_with_details ap {where_clause} {order_clause} LIMIT ${param_idx} OFFSET ${}",
        param_idx + 1
    ));

    let total: i64 = {
        let mut q = sqlx::query_scalar::<_, i64>(count_sql);
        if has_search {
            q = q.bind(&search_pattern);
        }
        if has_room_filter {
            q = q.bind(parsed_room_id);
        }
        if has_type_filter {
            q = q.bind(&outlet_type);
        }
        q.fetch_one(&state.pool()?.get_conn()).await?
    };

    let net_outlets = {
        let mut q = sqlx::query_as::<_, NetOutletWithDetails>(data_sql);
        if has_search {
            q = q.bind(&search_pattern);
        }
        if has_room_filter {
            q = q.bind(parsed_room_id);
        }
        if has_type_filter {
            q = q.bind(&outlet_type);
        }
        q = q.bind(page_size).bind(offset);
        q.fetch_all(&state.pool()?.get_conn()).await?
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": net_outlets,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "信息点列表获取成功",
    )))
}

pub async fn create_net_outlet(
    state: web::Data<AppState>,
    req: web::Json<NetOutletCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE id = $1")
        .bind(req.room_id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .ok_or_else(|| AppError::Validation("房间不存在".to_string()))?;

    if let Some(cabinet_id) = req.cabinet_id {
        let cabinet_room_id: Option<Uuid> =
            sqlx::query_scalar("SELECT room_id FROM cabinets WHERE id = $1")
                .bind(cabinet_id)
                .fetch_optional(&state.pool()?.get_conn())
                .await?;
        let cabinet_room_id =
            cabinet_room_id.ok_or_else(|| AppError::NotFound("机柜未找到".to_string()))?;
        if cabinet_room_id != req.room_id {
            return Err(AppError::Validation("机柜不属于所选房间".to_string()));
        }
    }

    let outlet_type = req.outlet_type.as_deref().unwrap_or("wall_socket");

    if !matches!(
        outlet_type,
        "wall_socket" | "patch_panel" | "wifi_ap" | "other"
    ) {
        return Err(AppError::Validation(
            "信息点类型必须是wall_socket、patch_panel、wifi_ap或other".to_string(),
        ));
    }

    if let Some(peer_type) = req.peer_type.as_deref()
        && !matches!(peer_type, "outlet" | "switch_port")
    {
        return Err(AppError::Validation(
            "对端类型必须是outlet或switch_port".to_string(),
        ));
    }

    if let Some(peer_type) = req.peer_type.as_deref() {
        match peer_type {
            "outlet" if req.peer_outlet_id.is_none() => {
                return Err(AppError::Validation(
                    "对端类型为信息点时必须指定对端信息点".to_string(),
                ));
            }
            "switch_port" if req.peer_switch_port_id.is_none() => {
                return Err(AppError::Validation(
                    "对端类型为交换机接口时必须指定对端交换机接口".to_string(),
                ));
            }
            _ => {}
        }
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO net_outlets (id, name, outlet_type, room_id, cabinet_id, description, peer_type, peer_room_id, peer_outlet_id, peer_switch_port_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(outlet_type)
    .bind(req.room_id)
    .bind(req.cabinet_id)
    .bind(&req.description)
    .bind(&req.peer_type)
    .bind(req.peer_room_id)
    .bind(req.peer_outlet_id)
    .bind(req.peer_switch_port_id)
    .bind(now)
    .bind(now)
    .execute(&state.pool()?.get_conn())
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict("该房间下信息点名称已存在".to_string());
        }
        AppError::from(e)
    })?;

    let net_outlet = NetOutlet {
        id,
        name: req.name.clone(),
        outlet_type: outlet_type.to_string(),
        room_id: req.room_id,
        cabinet_id: req.cabinet_id,
        description: req.description.clone(),
        peer_type: req.peer_type.clone(),
        peer_room_id: req.peer_room_id,
        peer_outlet_id: req.peer_outlet_id,
        peer_switch_port_id: req.peer_switch_port_id,
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": net_outlet.name,
        "outlet_type": net_outlet.outlet_type,
        "room_id": net_outlet.room_id,
        "description": net_outlet.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
            resource_type: "net_outlet",
            resource_id: Some(&id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<NetOutlet>::success(
        net_outlet,
        "信息点创建成功",
    )))
}

pub async fn get_net_outlet(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let net_outlet = sqlx::query_as::<_, NetOutletWithDetails>(
        "SELECT id, name, outlet_type, room_id, room_name, \
         cabinet_id, cabinet_name, \
         description, \
         peer_type, peer_room_id, peer_outlet_id, peer_switch_port_id, \
         peer_room_name, peer_outlet_name, peer_switch_port_label, \
         created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ \
         FROM net_outlets_with_details WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("信息点未找到".to_string()))?;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<NetOutletWithDetails>::success(
            net_outlet,
            "信息点获取成功",
        )),
    )
}

pub async fn update_net_outlet(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<NetOutletUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    (*req).validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM net_outlets WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("信息点未找到".to_string()));
    }

    let (current_room_id, current_cabinet_id): (Uuid, Option<Uuid>) =
        sqlx::query_as("SELECT room_id, cabinet_id FROM net_outlets WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    let effective_room_id = req.room_id.unwrap_or(current_room_id);
    let effective_cabinet_id = match &req.cabinet_id {
        Some(Some(cid)) => Some(*cid),
        Some(None) => None,
        None => current_cabinet_id,
    };

    let room_changed = req.room_id.is_some() && req.room_id != Some(current_room_id);
    let cabinet_explicitly_set = req.cabinet_id.is_some();

    let effective_cabinet_id = if room_changed && !cabinet_explicitly_set {
        None
    } else {
        effective_cabinet_id
    };

    if let Some(cab_id) = effective_cabinet_id {
        let cabinet_room_id: Option<Uuid> =
            sqlx::query_scalar("SELECT room_id FROM cabinets WHERE id = $1")
                .bind(cab_id)
                .fetch_optional(&mut *tx)
                .await?;
        let cabinet_room_id =
            cabinet_room_id.ok_or_else(|| AppError::NotFound("机柜未找到".to_string()))?;
        if cabinet_room_id != effective_room_id {
            return Err(AppError::Validation("机柜不属于所选房间".to_string()));
        }
    }

    let force_cabinet_null = room_changed && !cabinet_explicitly_set;

    if let Some(ref outlet_type) = req.outlet_type
        && !matches!(
            outlet_type.as_str(),
            "wall_socket" | "patch_panel" | "wifi_ap" | "other"
        )
    {
        return Err(AppError::Validation(
            "信息点类型必须是wall_socket、patch_panel、wifi_ap或other".to_string(),
        ));
    }

    if let Some(Some(ref peer_type)) = req.peer_type
        && !matches!(peer_type.as_str(), "outlet" | "switch_port")
    {
        return Err(AppError::Validation(
            "对端类型必须是outlet或switch_port".to_string(),
        ));
    }

    let now = Utc::now();

    let mut set_clauses: Vec<String> = Vec::new();
    let mut param_index = 1;

    set_clauses.push(format!("name = COALESCE(${param_index}, name)"));
    param_index += 1;

    set_clauses.push(format!(
        "outlet_type = COALESCE(${param_index}, outlet_type)"
    ));
    param_index += 1;

    set_clauses.push(format!("room_id = COALESCE(${param_index}, room_id)"));
    param_index += 1;

    let cabinet_id_update = req.cabinet_id.is_some() || force_cabinet_null;
    if cabinet_id_update {
        set_clauses.push(format!(
            "cabinet_id = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE cabinet_id END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    set_clauses.push(format!(
        "description = COALESCE(${param_index}, description)"
    ));
    param_index += 1;

    let peer_type_update = req.peer_type.is_some();
    if peer_type_update {
        set_clauses.push(format!(
            "peer_type = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE peer_type END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    let peer_room_id_update = req.peer_room_id.is_some();
    if peer_room_id_update {
        set_clauses.push(format!(
            "peer_room_id = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE peer_room_id END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    let peer_outlet_id_update = req.peer_outlet_id.is_some();
    if peer_outlet_id_update {
        set_clauses.push(format!(
            "peer_outlet_id = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE peer_outlet_id END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    let peer_switch_port_id_update = req.peer_switch_port_id.is_some();
    if peer_switch_port_id_update {
        set_clauses.push(format!(
            "peer_switch_port_id = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE peer_switch_port_id END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    set_clauses.push(format!("updated_at = ${param_index}"));
    param_index += 1;

    let where_param = param_index;

    let sql = format!(
        "UPDATE net_outlets SET {} WHERE id = ${}",
        set_clauses.join(", "),
        where_param
    );

    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));

    query = query.bind(&req.name);
    query = query.bind(&req.outlet_type);
    query = query.bind(req.room_id);

    if cabinet_id_update {
        if force_cabinet_null {
            query = query.bind(true);
            query = query.bind(Option::<Uuid>::None);
        } else {
            match req.cabinet_id {
                Some(Some(cid)) => {
                    query = query.bind(true);
                    query = query.bind(cid);
                }
                Some(None) => {
                    query = query.bind(true);
                    query = query.bind(Option::<Uuid>::None);
                }
                None => unreachable!(),
            }
        }
    }

    query = query.bind(&req.description);

    if peer_type_update {
        match &req.peer_type {
            Some(Some(pt)) => {
                query = query.bind(true);
                query = query.bind(pt);
            }
            Some(None) => {
                query = query.bind(true);
                query = query.bind(Option::<String>::None);
            }
            None => unreachable!(),
        }
    }

    if peer_room_id_update {
        match &req.peer_room_id {
            Some(Some(rid)) => {
                query = query.bind(true);
                query = query.bind(rid);
            }
            Some(None) => {
                query = query.bind(true);
                query = query.bind(Option::<Uuid>::None);
            }
            None => unreachable!(),
        }
    }

    if peer_outlet_id_update {
        match &req.peer_outlet_id {
            Some(Some(oid)) => {
                query = query.bind(true);
                query = query.bind(oid);
            }
            Some(None) => {
                query = query.bind(true);
                query = query.bind(Option::<Uuid>::None);
            }
            None => unreachable!(),
        }
    }

    if peer_switch_port_id_update {
        match &req.peer_switch_port_id {
            Some(Some(spid)) => {
                query = query.bind(true);
                query = query.bind(spid);
            }
            Some(None) => {
                query = query.bind(true);
                query = query.bind(Option::<Uuid>::None);
            }
            None => unreachable!(),
        }
    }

    query = query.bind(now);
    query = query.bind(id);

    query.execute(&mut *tx).await.map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict("该房间下信息点名称已存在".to_string());
        }
        AppError::from(e)
    })?;

    tx.commit().await?;

    let net_outlet = sqlx::query_as::<_, NetOutletWithDetails>(
        "SELECT id, name, outlet_type, room_id, room_name, \
         cabinet_id, cabinet_name, \
         description, \
         peer_type, peer_room_id, peer_outlet_id, peer_switch_port_id, \
         peer_room_name, peer_outlet_name, peer_switch_port_label, \
         created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ \
         FROM net_outlets_with_details WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "name": net_outlet.name,
        "outlet_type": net_outlet.outlet_type,
        "room_id": net_outlet.room_id,
        "description": net_outlet.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
            resource_type: "net_outlet",
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
        HttpResponse::Ok().json(ApiResponse::<NetOutletWithDetails>::success(
            net_outlet,
            "信息点更新成功",
        )),
    )
}

pub async fn delete_net_outlet(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM net_outlets WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("信息点未找到".to_string()));
    }

    // net_outlet_ids 数组无外键约束，需手动清理 device_interfaces 中的悬空引用
    sqlx::query(
        "UPDATE device_interfaces
         SET net_outlet_ids = array_remove(net_outlet_ids, $1)
         WHERE $1 = ANY(net_outlet_ids)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM net_outlets WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({ "net_outlet_id": id.to_string() });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "net_outlet",
            resource_id: Some(&id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "信息点删除成功")))
}
