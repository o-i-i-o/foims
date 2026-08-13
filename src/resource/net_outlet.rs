use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{NetOutlet, NetOutletCreate, NetOutletUpdate, NetOutletWithDetails};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::Pagination;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

pub async fn get_net_outlets(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();
    let room_id = query.get("room_id").cloned();
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());

    let search_pattern = crate::utils::escape_like(&search);
    let parsed_room_id = room_id
        .as_ref()
        .map(|id| {
            Uuid::parse_str(id).map_err(|_| AppError::Validation("无效的room_id参数".to_string()))
        })
        .transpose()?;

    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY ap.name DESC",
        ("created_at", "desc") => "ORDER BY ap.created_at DESC",
        ("created_at", _) => "ORDER BY ap.created_at ASC",
        _ => "ORDER BY ap.name ASC",
    };

    let has_room_filter = parsed_room_id.is_some();
    let has_search = !search.is_empty();

    let mut where_parts: Vec<String> = Vec::new();
    let mut param_idx = 1;

    if has_search {
        where_parts.push(format!("ap.name ILIKE ${param_idx}"));
        param_idx += 1;
    }
    if has_room_filter {
        where_parts.push(format!("ap.room_id = ${param_idx}"));
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
        "SELECT ap.id, ap.name, ap.room_id, ap.room_name, \
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
        q = q.bind(page_size).bind(offset);
        q.fetch_all(&state.pool()?.get_conn()).await?
    };

    Ok(crate::error::ok_json(
        json!({
            "items": net_outlets,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "信息点列表获取成功",
    ))
}

pub async fn create_net_outlet(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<NetOutletCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE id = $1")
        .bind(req.room_id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .ok_or_else(|| AppError::Validation("房间不存在".to_string()))?;

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO net_outlets (id, name, room_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(req.room_id)
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
        room_id: req.room_id,
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": net_outlet.name,
        "room_id": net_outlet.room_id
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "net_outlet",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(net_outlet, "信息点创建成功"))
}

pub async fn get_net_outlet(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let net_outlet = sqlx::query_as::<_, NetOutletWithDetails>(
        "SELECT id, name, room_id, room_name, \
         created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ \
         FROM net_outlets_with_details WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("信息点未找到".to_string()))?;

    Ok(crate::error::ok_json(net_outlet, "信息点获取成功"))
}

pub async fn update_net_outlet(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<NetOutletUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM net_outlets WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("信息点未找到".to_string()));
    }

    if let Some(room_id) = req.room_id {
        let room_exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM rooms WHERE id = $1")
            .bind(room_id)
            .fetch_optional(&mut *tx)
            .await?;
        if room_exists.is_none() {
            return Err(AppError::Validation("房间不存在".to_string()));
        }
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE net_outlets \
         SET name = COALESCE($1, name), \
             room_id = COALESCE($2, room_id), \
             updated_at = $3 \
         WHERE id = $4",
    )
    .bind(&req.name)
    .bind(req.room_id)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict("该房间下信息点名称已存在".to_string());
        }
        AppError::from(e)
    })?;

    tx.commit().await?;

    let net_outlet = sqlx::query_as::<_, NetOutletWithDetails>(
        "SELECT id, name, room_id, room_name, \
         created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ \
         FROM net_outlets_with_details WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "name": net_outlet.name,
        "room_id": net_outlet.room_id
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "net_outlet",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(net_outlet, "信息点更新成功"))
}

pub async fn delete_net_outlet(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM net_outlets WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if existing.is_none() {
        return Err(AppError::NotFound("信息点未找到".to_string()));
    }

    sqlx::query("DELETE FROM net_outlets WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e {
                let msg = db_err.message();
                if msg.contains("cable_links") {
                    return AppError::Validation("信息点已被线路引用，无法删除".to_string());
                }
            }
            AppError::from(e)
        })?;

    tx.commit().await?;

    let details = serde_json::json!({ "net_outlet_id": id.to_string() });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "net_outlet",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json((), "信息点删除成功"))
}
