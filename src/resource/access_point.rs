use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    AccessPoint, AccessPointCreate, AccessPointLinkPeer, AccessPointUpdate, AccessPointWithDetails,
    ApiResponse,
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

pub async fn get_access_points(
    state: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let page: i64 = query
        .get("page")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PAGE)
        .max(1);
    let page_size: i64 = query
        .get("page_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    let search = query.get("search").cloned().unwrap_or_default();
    let room_id = query.get("room_id").cloned();
    let ap_type = query.get("ap_type").cloned();
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());
    let offset = (page - 1) * page_size;

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
        ("ap_type", "desc") => "ORDER BY ap.ap_type DESC, ap.name ASC",
        ("ap_type", _) => "ORDER BY ap.ap_type ASC, ap.name ASC",
        ("created_at", "desc") => "ORDER BY ap.created_at DESC",
        ("created_at", _) => "ORDER BY ap.created_at ASC",
        _ => "ORDER BY ap.name ASC",
    };

    let has_room_filter = parsed_room_id.is_some();
    let has_type_filter = !ap_type.as_ref().is_none_or(|t| t.is_empty());
    let has_search = !search.is_empty();

    // Build WHERE conditions dynamically
    let mut where_parts: Vec<String> = Vec::new();
    let mut param_idx = 1;

    if has_search {
        where_parts.push(format!(
            "(ap.name ILIKE ${param_idx} OR ap.description ILIKE ${param_idx} OR ap.ap_type ILIKE ${param_idx})"
        ));
        param_idx += 1;
    }
    if has_room_filter {
        where_parts.push(format!("ap.room_id = ${param_idx}"));
        param_idx += 1;
    }
    if has_type_filter {
        where_parts.push(format!("ap.ap_type = ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_parts.join(" AND "))
    };

    let count_sql = sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM access_points_with_details ap {where_clause}"
    ));
    let data_sql = sqlx::AssertSqlSafe(format!(
        "SELECT ap.id, ap.name, ap.ap_type, ap.room_id, ap.room_name, \
         ap.cabinet_id, ap.cabinet_name, ap.peer_access_point_id, ap.peer_access_point_name, \
         ap.switch_port_id, ap.connected_switch_port, ap.connected_switch_name, \
         ap.description, ap.created_at::TIMESTAMPTZ, ap.updated_at::TIMESTAMPTZ \
         FROM access_points_with_details ap {where_clause} {order_clause} LIMIT ${param_idx} OFFSET {}",
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
            q = q.bind(&ap_type);
        }
        q.fetch_one(&state.pool()?.get_conn()).await?
    };

    let access_points = {
        let mut q = sqlx::query_as::<_, AccessPointWithDetails>(data_sql);
        if has_search {
            q = q.bind(&search_pattern);
        }
        if has_room_filter {
            q = q.bind(parsed_room_id);
        }
        if has_type_filter {
            q = q.bind(&ap_type);
        }
        q = q.bind(page_size).bind(offset);
        q.fetch_all(&state.pool()?.get_conn()).await?
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": access_points,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "接入点列表获取成功",
    )))
}

pub async fn create_access_point(
    state: web::Data<AppState>,
    req: web::Json<AccessPointCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    // Verify room_id exists
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE id = $1")
        .bind(req.room_id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .ok_or_else(|| AppError::Validation("房间不存在".to_string()))?;

    // Verify cabinet_id belongs to room_id
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

    let ap_type = req.ap_type.as_deref().unwrap_or("wall_socket");

    // Validate ap_type value
    if !matches!(ap_type, "wall_socket" | "patch_panel" | "wifi_ap" | "other") {
        return Err(AppError::Validation(
            "接入点类型必须是wall_socket、patch_panel、wifi_ap或other".to_string(),
        ));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO access_points (id, name, ap_type, room_id, cabinet_id, peer_access_point_id, switch_port_id, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(ap_type)
    .bind(req.room_id)
    .bind(req.cabinet_id)
    .bind(req.peer_access_point_id)
    .bind(req.switch_port_id)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&state.pool()?.get_conn())
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict("该房间下接入点名称已存在".to_string());
        }
        AppError::from(e)
    })?;

    let access_point = AccessPoint {
        id,
        name: req.name.clone(),
        ap_type: ap_type.to_string(),
        room_id: req.room_id,
        cabinet_id: req.cabinet_id,
        peer_access_point_id: req.peer_access_point_id,
        switch_port_id: req.switch_port_id,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": access_point.name,
        "ap_type": access_point.ap_type,
        "room_id": access_point.room_id,
        "description": access_point.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
            resource_type: "access_point",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<AccessPoint>::success(
        access_point,
        "接入点创建成功",
    )))
}

pub async fn get_access_point(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let access_point = sqlx::query_as::<_, AccessPointWithDetails>(
        "SELECT id, name, ap_type, room_id, room_name, \
         cabinet_id, cabinet_name, peer_access_point_id, peer_access_point_name, \
         switch_port_id, connected_switch_port, connected_switch_name, \
         description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ \
         FROM access_points_with_details WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("接入点未找到".to_string()))?;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<AccessPointWithDetails>::success(
            access_point,
            "接入点获取成功",
        )),
    )
}

pub async fn update_access_point(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<AccessPointUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;
    (*req).validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM access_points WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("接入点未找到".to_string()));
    }

    // Fetch current room_id and cabinet_id for validation
    let (current_room_id, current_cabinet_id): (Uuid, Option<Uuid>) =
        sqlx::query_as("SELECT room_id, cabinet_id FROM access_points WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    // Determine the effective room_id and cabinet_id after update
    let effective_room_id = req.room_id.unwrap_or(current_room_id);
    // If cabinet_id is being explicitly set (outer Some), use that; otherwise keep current
    let effective_cabinet_id = match &req.cabinet_id {
        Some(Some(cid)) => Some(*cid),
        Some(None) => None,
        None => current_cabinet_id,
    };

    // If room_id is changing and cabinet_id is not also being explicitly changed, clear cabinet_id
    let room_changed = req.room_id.is_some() && req.room_id != Some(current_room_id);
    let cabinet_explicitly_set = req.cabinet_id.is_some();

    let effective_cabinet_id = if room_changed && !cabinet_explicitly_set {
        // Room changed but cabinet not explicitly set -> auto-clear cabinet
        None
    } else {
        effective_cabinet_id
    };

    // Verify cabinet_id belongs to the effective room_id
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

    // If room changed and cabinet not explicitly set, force cabinet_id to NULL
    let force_cabinet_null = room_changed && !cabinet_explicitly_set;

    // Validate ap_type if provided
    if let Some(ref ap_type) = req.ap_type
        && !matches!(
            ap_type.as_str(),
            "wall_socket" | "patch_panel" | "wifi_ap" | "other"
        )
    {
        return Err(AppError::Validation(
            "接入点类型必须是wall_socket、patch_panel、wifi_ap或other".to_string(),
        ));
    }

    let now = Utc::now();

    // Handle Option<Option<Uuid>> fields:
    // None -> don't change, Some(Some(id)) -> set to id, Some(None) -> set to NULL
    // We use a dynamic SET clause approach
    let mut set_clauses: Vec<String> = Vec::new();
    let mut param_index = 1;

    // name: Option<String> -> COALESCE pattern
    set_clauses.push(format!("name = COALESCE(${param_index}, name)"));
    param_index += 1;

    // ap_type: Option<String> -> COALESCE pattern
    set_clauses.push(format!("ap_type = COALESCE(${param_index}, ap_type)"));
    param_index += 1;

    // room_id: Option<Uuid> -> COALESCE pattern
    set_clauses.push(format!("room_id = COALESCE(${param_index}, room_id)"));
    param_index += 1;

    // cabinet_id: Option<Option<Uuid>>
    // Use CASE: when outer is None -> keep current, when Some(Some(id)) -> set id, when Some(None) -> set NULL
    // Also, if room changed without cabinet being explicitly set, force to NULL
    let cabinet_id_update = req.cabinet_id.is_some() || force_cabinet_null;
    if cabinet_id_update {
        set_clauses.push(format!(
            "cabinet_id = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE cabinet_id END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    // peer_access_point_id: Option<Option<Uuid>>
    let peer_id_update = req.peer_access_point_id.is_some();
    if peer_id_update {
        set_clauses.push(format!(
            "peer_access_point_id = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE peer_access_point_id END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    // switch_port_id: Option<Option<Uuid>>
    let switch_port_id_update = req.switch_port_id.is_some();
    if switch_port_id_update {
        set_clauses.push(format!(
            "switch_port_id = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE switch_port_id END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    // description: Option<String> -> COALESCE pattern
    set_clauses.push(format!(
        "description = COALESCE(${param_index}, description)"
    ));
    param_index += 1;

    // updated_at
    set_clauses.push(format!("updated_at = ${param_index}"));
    param_index += 1;

    // WHERE id = $N
    let where_param = param_index;

    let sql = format!(
        "UPDATE access_points SET {} WHERE id = ${}",
        set_clauses.join(", "),
        where_param
    );

    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));

    // Bind name
    query = query.bind(&req.name);
    // Bind ap_type
    query = query.bind(&req.ap_type);
    // Bind room_id
    query = query.bind(req.room_id);

    // Bind cabinet_id: Some(Some(id)) -> (true, id), Some(None) -> (true, NULL)
    // If force_cabinet_null, always bind (true, NULL)
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

    // Bind peer_access_point_id
    if peer_id_update {
        match req.peer_access_point_id {
            Some(Some(pid)) => {
                query = query.bind(true);
                query = query.bind(pid);
            }
            Some(None) => {
                query = query.bind(true);
                query = query.bind(Option::<Uuid>::None);
            }
            None => unreachable!(),
        }
    }

    // Bind switch_port_id
    if switch_port_id_update {
        match req.switch_port_id {
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

    // Bind description
    query = query.bind(&req.description);
    // Bind updated_at
    query = query.bind(now);
    // Bind id
    query = query.bind(id);

    query.execute(&mut *tx).await.map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict("该房间下接入点名称已存在".to_string());
        }
        AppError::from(e)
    })?;

    tx.commit().await?;

    let access_point = sqlx::query_as::<_, AccessPointWithDetails>(
        "SELECT id, name, ap_type, room_id, room_name, \
         cabinet_id, cabinet_name, peer_access_point_id, peer_access_point_name, \
         switch_port_id, connected_switch_port, connected_switch_name, \
         description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ \
         FROM access_points_with_details WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "name": access_point.name,
        "ap_type": access_point.ap_type,
        "room_id": access_point.room_id,
        "description": access_point.description
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
            resource_type: "access_point",
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
        HttpResponse::Ok().json(ApiResponse::<AccessPointWithDetails>::success(
            access_point,
            "接入点更新成功",
        )),
    )
}

pub async fn delete_access_point(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM access_points WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("接入点未找到".to_string()));
    }

    // Check if any devices reference this access point
    let device_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE access_point_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if device_count > 0 {
        return Err(AppError::Validation(format!(
            "该接入点已被 {device_count} 个设备关联，无法删除"
        )));
    }

    // Clear peer_access_point_id on any AP that points to this one
    sqlx::query(
        "UPDATE access_points SET peer_access_point_id = NULL WHERE peer_access_point_id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM access_points WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({ "access_point_id": id.to_string() });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "access_point",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "接入点删除成功")))
}

pub async fn link_peer(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<AccessPointLinkPeer>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    (*req).validate()?;

    let id = *id_path;
    let peer_id = req.peer_access_point_id;

    if id == peer_id {
        return Err(AppError::Validation(
            "接入点不能与自身建立对端连接".to_string(),
        ));
    }

    let mut tx = state.pool()?.get_conn().begin().await?;

    // Verify current AP exists
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM access_points WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("接入点未找到".to_string()))?;

    // Verify peer AP exists
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM access_points WHERE id = $1")
        .bind(peer_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::Validation("对端接入点不存在".to_string()))?;

    // Check for circular reference: the peer shouldn't already have a peer that creates a chain > 2
    // If the peer already has a peer_access_point_id set (and it's not our current id), that would create a chain
    let peer_of_peer: Option<Uuid> =
        sqlx::query_scalar("SELECT peer_access_point_id FROM access_points WHERE id = $1")
            .bind(peer_id)
            .fetch_one(&mut *tx)
            .await?;

    if let Some(other_peer_id) = peer_of_peer
        && other_peer_id != id
    {
        return Err(AppError::Validation(
            "对端接入点已有其他对端连接，无法建立链式连接".to_string(),
        ));
    }

    let now = Utc::now();

    // Check if current AP already has a different peer; if so, clear the old peer's reverse link
    let current_peer_id: Option<Uuid> =
        sqlx::query_scalar("SELECT peer_access_point_id FROM access_points WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if let Some(old_peer_id) = current_peer_id
        && old_peer_id != peer_id
    {
        sqlx::query(
            "UPDATE access_points SET peer_access_point_id = NULL, updated_at = $1 WHERE id = $2 AND peer_access_point_id = $3",
        )
        .bind(now)
        .bind(old_peer_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "UPDATE access_points SET peer_access_point_id = $1, updated_at = $2 WHERE id = $3",
    )
    .bind(peer_id)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    // Also set the reverse link on the peer
    sqlx::query(
        "UPDATE access_points SET peer_access_point_id = $1, updated_at = $2 WHERE id = $3",
    )
    .bind(id)
    .bind(now)
    .bind(peer_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let access_point = sqlx::query_as::<_, AccessPointWithDetails>(
        "SELECT id, name, ap_type, room_id, room_name, \
         cabinet_id, cabinet_name, peer_access_point_id, peer_access_point_name, \
         switch_port_id, connected_switch_port, connected_switch_name, \
         description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ \
         FROM access_points_with_details WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "access_point_id": id.to_string(),
        "peer_access_point_id": peer_id.to_string()
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "link_peer",
            resource_type: "access_point",
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
        HttpResponse::Ok().json(ApiResponse::<AccessPointWithDetails>::success(
            access_point,
            "对端接入点关联成功",
        )),
    )
}

pub async fn unlink_peer(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing = sqlx::query_as::<_, AccessPoint>(
        "SELECT id, name, ap_type, room_id, cabinet_id, peer_access_point_id, switch_port_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ \
         FROM access_points WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("接入点未找到".to_string()))?;

    let peer_id = existing
        .peer_access_point_id
        .ok_or_else(|| AppError::Validation("该接入点没有对端连接".to_string()))?;

    let now = Utc::now();

    // Clear peer on current AP
    sqlx::query(
        "UPDATE access_points SET peer_access_point_id = NULL, updated_at = $1 WHERE id = $2",
    )
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    // Clear reverse link on the peer
    sqlx::query(
        "UPDATE access_points SET peer_access_point_id = NULL, updated_at = $1 WHERE id = $2 AND peer_access_point_id = $3",
    )
    .bind(now)
    .bind(peer_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let access_point = sqlx::query_as::<_, AccessPointWithDetails>(
        "SELECT id, name, ap_type, room_id, room_name, \
         cabinet_id, cabinet_name, peer_access_point_id, peer_access_point_name, \
         switch_port_id, connected_switch_port, connected_switch_name, \
         description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ \
         FROM access_points_with_details WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "access_point_id": id.to_string(),
        "unlinked_peer_id": peer_id.to_string()
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "unlink_peer",
            resource_type: "access_point",
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
        HttpResponse::Ok().json(ApiResponse::<AccessPointWithDetails>::success(
            access_point,
            "对端接入点取消关联成功",
        )),
    )
}
