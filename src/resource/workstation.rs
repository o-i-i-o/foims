use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    IpManager, Workstation, WorkstationCreate, WorkstationUpdate, WorkstationWithDetails,
};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::Pagination;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

pub async fn get_workstations(
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
    let parsed_room_id = room_id.as_ref().and_then(|id| Uuid::parse_str(id).ok());

    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY w.name DESC",
        ("room_name", "desc") => "ORDER BY room_name DESC, w.name ASC",
        ("room_name", _) => "ORDER BY room_name ASC, w.name ASC",
        ("manager", "desc") => "ORDER BY w.manager DESC, w.name ASC",
        ("manager", _) => "ORDER BY w.manager ASC, w.name ASC",
        ("created_at", "desc") => "ORDER BY w.created_at DESC",
        ("created_at", _) => "ORDER BY w.created_at ASC",
        _ => "ORDER BY w.name ASC",
    };

    let (total, workstations_basic) = if search.is_empty() && parsed_room_id.is_none() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workstations w")
            .fetch_one(&state.pool()?.get_conn())
            .await?;

        let workstations_basic = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT w.id, w.name, w.room_id,
                    (SELECT r.name FROM rooms r WHERE r.id = w.room_id) as room_name,
                    w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ
             FROM workstations w {order_clause} LIMIT $1 OFFSET $2"
        )))
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, workstations_basic)
    } else if parsed_room_id.is_some() && search.is_empty() {
        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM workstations w WHERE w.room_id = $1")
                .bind(parsed_room_id)
                .fetch_one(&state.pool()?.get_conn())
                .await?;

        let workstations_basic = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT w.id, w.name, w.room_id,
                    (SELECT r.name FROM rooms r WHERE r.id = w.room_id) as room_name,
                    w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ
             FROM workstations w WHERE w.room_id = $1 {order_clause} LIMIT $2 OFFSET $3"
        )))
        .bind(parsed_room_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, workstations_basic)
    } else if parsed_room_id.is_some() {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM workstations w WHERE w.room_id = $1 AND (w.name ILIKE $2 OR w.manager ILIKE $2 OR w.description ILIKE $2)"
        )
        .bind(parsed_room_id)
        .bind(&search_pattern)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

        let workstations_basic = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT w.id, w.name, w.room_id,
                    (SELECT r.name FROM rooms r WHERE r.id = w.room_id) as room_name,
                    w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ
             FROM workstations w WHERE w.room_id = $1 AND (w.name ILIKE $2 OR w.manager ILIKE $2 OR w.description ILIKE $2) {order_clause} LIMIT $3 OFFSET $4"
        )))
        .bind(parsed_room_id)
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, workstations_basic)
    } else {
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM workstations w WHERE w.name ILIKE $1 OR w.manager ILIKE $1 OR w.description ILIKE $1"
        )
        .bind(&search_pattern)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

        let workstations_basic = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT w.id, w.name, w.room_id,
                    (SELECT r.name FROM rooms r WHERE r.id = w.room_id) as room_name,
                    w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ
             FROM workstations w WHERE w.name ILIKE $1 OR w.manager ILIKE $1 OR w.description ILIKE $1 {order_clause} LIMIT $2 OFFSET $3"
        )))
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

        (total, workstations_basic)
    };

    let mut workstations_with_details = Vec::new();

    for row in workstations_basic {
        let id: Uuid = row.get("id");
        let name: String = row.get("name");
        let room_id: Uuid = row.get("room_id");
        let room_name: Option<String> = row.get("room_name");
        let manager: Option<String> = row.get("manager");
        let description: Option<String> = row.get("description");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

        let workstation_with_details = WorkstationWithDetails {
            id,
            name,
            room_id,
            room_name,
            manager,
            ips: Vec::new(),
            description,
            created_at,
            updated_at,
        };

        workstations_with_details.push(workstation_with_details);
    }

    Ok(crate::error::ok_json(
        json!({
            "items": workstations_with_details,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "工位获取成功",
    ))
}

pub async fn create_workstation(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<WorkstationCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_workstation: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM workstations WHERE name = $1 AND room_id = $2",
    )
    .bind(&req.name)
    .bind(req.room_id)
    .fetch_optional(&mut *tx)
    .await?;

    if existing_workstation.is_some() {
        return Err(AppError::Conflict("工位名称已存在".to_string()));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO workstations (id, name, room_id, manager, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(req.room_id)
    .bind(&req.manager)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let workstation = Workstation {
        id,
        name: req.name.clone(),
        room_id: req.room_id,
        room_name: None,
        manager: req.manager.clone(),
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": workstation.name,
        "room_id": workstation.room_id,
        "manager": workstation.manager,
        "description": workstation.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "workstation",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(workstation, "工位创建成功"))
}

pub async fn get_workstation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let workstation = sqlx::query_as::<_, Workstation>(
        "SELECT w.id, w.name, w.room_id, r.name as room_name, w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ FROM workstations w LEFT JOIN rooms r ON w.room_id = r.id WHERE w.id = $1"
    ).bind(id)
    .fetch_optional(&state.pool()?.get_conn()).await?
    .ok_or_else(|| AppError::NotFound("工位未找到".to_string()))?;

    let workstation_ips = sqlx::query_as::<_, IpManager>(
        r"SELECT
            m.id, m.device_interface_id, m.device_id, m.network_id,
            host(m.ip_address) as ip_address,
            m.ip_version, m.mac_address, m.hostname, m.description,
            m.status, m.last_seen, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ, m.last_mac
        FROM ips m
        JOIN devices d ON m.device_id = d.id
        WHERE d.workstation_id = $1
        ORDER BY m.ip_address",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let workstation_with_details = WorkstationWithDetails {
        id: workstation.id,
        name: workstation.name,
        room_id: workstation.room_id,
        room_name: workstation.room_name,
        manager: workstation.manager.clone(),
        ips: workstation_ips,
        description: workstation.description,
        created_at: workstation.created_at,
        updated_at: workstation.updated_at,
    };

    Ok(crate::error::ok_json(
        workstation_with_details,
        "工位获取成功",
    ))
}

pub async fn update_workstation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<WorkstationUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_workstation: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM workstations WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing_workstation.is_none() {
        return Err(AppError::NotFound("工位未找到".to_string()));
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE workstations SET
         name = COALESCE($1, name),
         room_id = COALESCE($2, room_id),
         manager = COALESCE($3, manager),
         description = COALESCE($4, description),
         updated_at = $5
         WHERE id = $6",
    )
    .bind(&req.name)
    .bind(req.room_id)
    .bind(&req.manager)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let row = sqlx::query(
        r"SELECT w.id, w.name, w.room_id, r.name as room_name, w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ
        FROM workstations w
        LEFT JOIN rooms r ON w.room_id = r.id
        WHERE w.id = $1"
    ).bind(id)
    .fetch_one(&state.pool()?.get_conn()).await?;

    let ips: Vec<IpManager> = sqlx::query_as(
        r"SELECT
            m.id, m.device_interface_id, m.device_id, m.network_id,
            host(m.ip_address) as ip_address,
            m.ip_version, m.mac_address, m.hostname, m.description,
            m.status, m.last_seen, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ, m.last_mac
        FROM ips m
        JOIN devices d ON m.device_id = d.id
        WHERE d.workstation_id = $1
        ORDER BY m.ip_address",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let result = WorkstationWithDetails {
        id: row.get("id"),
        name: row.get("name"),
        room_id: row.get("room_id"),
        room_name: row.get::<Option<String>, _>("room_name"),
        manager: row.get("manager"),
        ips,
        description: row.get("description"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };

    let details = serde_json::json!({
        "name": result.name,
        "room_id": result.room_id,
        "manager": result.manager,
        "description": result.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "workstation",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(result, "工位更新成功"))
}

pub async fn delete_workstation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_workstation: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM workstations WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing_workstation.is_none() {
        return Err(AppError::NotFound("工位未找到".to_string()));
    }

    sqlx::query("DELETE FROM workstation_layouts WHERE workstation_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM workstations WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "workstation_id": id.to_string()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "workstation",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json((), "工位删除成功"))
}
