//! 设备 CRUD 与跨设备列表查询。

use crate::app_state::AppState;
use crate::crypto::encrypt_password_async;
use crate::error::{AppError, msg};
use crate::models::{Device, DeviceCreate, DeviceUpdate, DeviceWithDetails};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::{Pagination, paged_response};
use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

use super::validate_device_type;

pub async fn get_devices(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();
    let workstation_id = query
        .get("workstation_id")
        .and_then(|id| Uuid::parse_str(id).ok());
    let position_id = query
        .get("position_id")
        .and_then(|id| Uuid::parse_str(id).ok());
    let device_type = query.get("device_type").cloned();
    let room_id = query.get("room_id").and_then(|id| Uuid::parse_str(id).ok());
    let cabinet_id = query
        .get("cabinet_id")
        .and_then(|id| Uuid::parse_str(id).ok());
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());

    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY d.name DESC",
        ("device_type", "desc") => "ORDER BY d.device_type DESC, d.name ASC",
        ("device_type", _) => "ORDER BY d.device_type ASC, d.name ASC",
        ("brand", "desc") => "ORDER BY d.brand DESC NULLS LAST, d.name ASC",
        ("brand", _) => "ORDER BY d.brand ASC NULLS LAST, d.name ASC",
        ("model", "desc") => "ORDER BY d.model DESC NULLS LAST, d.name ASC",
        ("model", _) => "ORDER BY d.model ASC NULLS LAST, d.name ASC",
        ("room_name", "desc") => "ORDER BY d.room_name DESC NULLS LAST, d.name ASC",
        ("room_name", _) => "ORDER BY d.room_name ASC NULLS LAST, d.name ASC",
        ("created_at", "desc") => "ORDER BY d.created_at DESC",
        ("created_at", _) => "ORDER BY d.created_at ASC",
        _ => "ORDER BY d.name ASC",
    };

    // Build dynamic WHERE conditions
    let mut where_parts: Vec<String> = Vec::new();
    let mut param_idx = 1;

    if !search.is_empty() {
        where_parts.push(format!(
            "(d.name ILIKE ${param_idx} OR d.brand ILIKE ${param_idx} OR d.model ILIKE ${param_idx} OR d.serial_number ILIKE ${param_idx} OR d.description ILIKE ${param_idx})"
        ));
        param_idx += 1;
    }

    if workstation_id.is_some() {
        where_parts.push(format!("d.workstation_id = ${param_idx}"));
        param_idx += 1;
    }

    if position_id.is_some() {
        where_parts.push(format!("d.position_id = ${param_idx}"));
        param_idx += 1;
    }

    if device_type.as_ref().is_some() {
        where_parts.push(format!("d.device_type = ${param_idx}"));
        param_idx += 1;
    }

    if room_id.is_some() {
        where_parts.push(format!("d.room_id = ${param_idx}"));
        param_idx += 1;
    }

    if cabinet_id.is_some() {
        where_parts.push(format!("d.cabinet_id = ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_parts.join(" AND "))
    };

    let search_pattern = crate::utils::escape_like(&search);

    let count_sql = sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM devices_with_details d {where_clause}"
    ));
    let data_sql = sqlx::AssertSqlSafe(format!(
        "SELECT d.id, d.name, d.hostname, d.device_type, d.brand, d.model, d.serial_number,
                d.workstation_id, d.position_id, d.room_id,
                d.template_id, d.seller, d.location,
                d.snmp_version, d.snmp_community, d.snmp_username,
                d.snmp_auth_protocol, d.snmp_auth_password,
                d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
                d.description,
                d.workstation_name, d.room_id, d.room_name, d.cabinet_id, d.cabinet_name,
                d.start_u, d.end_u,
                d.template_name,
                d.created_at::TIMESTAMPTZ, d.updated_at::TIMESTAMPTZ
         FROM devices_with_details d
         {where_clause}
         {order_clause}
         LIMIT ${param_idx} OFFSET ${}",
        param_idx + 1
    ));

    // Build and execute count query with bound params
    let mut count_query = sqlx::query_scalar::<_, i64>(count_sql);
    let mut data_query = sqlx::query_as::<_, DeviceWithDetails>(data_sql);

    if !search.is_empty() {
        count_query = count_query.bind(&search_pattern);
        data_query = data_query.bind(&search_pattern);
    }

    if let Some(ws_id) = workstation_id {
        count_query = count_query.bind(ws_id);
        data_query = data_query.bind(ws_id);
    }

    if let Some(pos_id) = position_id {
        count_query = count_query.bind(pos_id);
        data_query = data_query.bind(pos_id);
    }

    if let Some(ref dt) = device_type {
        count_query = count_query.bind(dt);
        data_query = data_query.bind(dt);
    }

    if let Some(r_id) = room_id {
        count_query = count_query.bind(r_id);
        data_query = data_query.bind(r_id);
    }

    if let Some(c_id) = cabinet_id {
        count_query = count_query.bind(c_id);
        data_query = data_query.bind(c_id);
    }

    count_query = count_query.bind(page_size).bind(offset);
    data_query = data_query.bind(page_size).bind(offset);

    let total: i64 = count_query.fetch_one(&state.pool()?.get_conn()).await?;

    let devices = data_query.fetch_all(&state.pool()?.get_conn()).await?;

    let items: Vec<serde_json::Value> = devices
        .into_iter()
        .map(|d| {
            let mut v = serde_json::to_value(&d).map_err(|e| {
                AppError::Internal(msg("server.common.serialize_failed").with("error", e))
            })?;
            v["snmp_community"] = serde_json::Value::Null;
            v["snmp_auth_password"] = serde_json::Value::Null;
            v["snmp_priv_password"] = serde_json::Value::Null;
            Ok(v)
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    Ok(crate::error::ok_json(
        paged_response(items, total, &pagination),
        "server.device.list_retrieved",
    ))
}

pub async fn create_device(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<DeviceCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let device_type = match &req.device_type {
        Some(dt) => {
            validate_device_type(dt)?;
            dt.clone()
        }
        None => "other".to_string(),
    };

    if req.workstation_id.is_some() && req.position_id.is_some() {
        return Err(AppError::Validation(msg(
            "server.device.workstation_position_exclusive",
        )));
    }

    let mut tx = state.pool()?.get_conn().begin().await?;

    if let Some(ws_id) = req.workstation_id {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workstations WHERE id = $1)")
                .bind(ws_id)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(AppError::NotFound(msg("server.workstation.not_found")));
        }
    }

    if let Some(pos_id) = req.position_id {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM positions WHERE id = $1)")
                .bind(pos_id)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(AppError::NotFound(msg("server.position.not_found")));
        }
    }

    let (final_device_type, final_brand, final_model) = if let Some(tmpl_id) = req.template_id {
        let template_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_templates WHERE id = $1)")
                .bind(tmpl_id)
                .fetch_one(&mut *tx)
                .await?;
        if !template_exists {
            return Err(AppError::NotFound(msg("server.device_template.not_found")));
        }

        let tmpl_device_type: Option<String> =
            sqlx::query_scalar("SELECT device_type FROM device_templates WHERE id = $1")
                .bind(tmpl_id)
                .fetch_optional(&mut *tx)
                .await?;
        let tmpl_brand: Option<String> =
            sqlx::query_scalar("SELECT brand FROM device_templates WHERE id = $1")
                .bind(tmpl_id)
                .fetch_optional(&mut *tx)
                .await?;
        let tmpl_model: Option<String> =
            sqlx::query_scalar("SELECT model FROM device_templates WHERE id = $1")
                .bind(tmpl_id)
                .fetch_optional(&mut *tx)
                .await?;

        let dt = req
            .device_type
            .as_deref()
            .or(tmpl_device_type.as_deref())
            .unwrap_or("other")
            .to_string();
        let br = req.brand.as_ref().or(tmpl_brand.as_ref()).cloned();
        let md = req.model.as_ref().or(tmpl_model.as_ref()).cloned();

        (dt, br, md)
    } else {
        (device_type, req.brand.clone(), req.model.clone())
    };

    validate_device_type(&final_device_type)?;

    let id = Uuid::new_v4();
    let now = Utc::now();

    let encrypted_community = match &req.snmp_community {
        Some(c) if !c.is_empty() => Some(encrypt_password_async(c.clone()).await?),
        _ => None,
    };
    let encrypted_auth_password = match &req.snmp_auth_password {
        Some(p) if !p.is_empty() => Some(encrypt_password_async(p.clone()).await?),
        _ => None,
    };
    let encrypted_priv_password = match &req.snmp_priv_password {
        Some(p) if !p.is_empty() => Some(encrypt_password_async(p.clone()).await?),
        _ => None,
    };

    let snmp_version = req
        .snmp_version
        .clone()
        .unwrap_or_else(|| "v2c".to_string());
    let snmp_port = req.snmp_port.unwrap_or(161);

    sqlx::query(
        "INSERT INTO devices (id, name, hostname, device_type, brand, model, serial_number, workstation_id, position_id, room_id, template_id, seller, location, snmp_version, snmp_community, snmp_username, snmp_auth_protocol, snmp_auth_password, snmp_priv_protocol, snmp_priv_password, snmp_port, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.hostname)
    .bind(&final_device_type)
    .bind(&final_brand)
    .bind(&final_model)
    .bind(&req.serial_number)
    .bind(req.workstation_id)
    .bind(req.position_id)
    .bind(req.room_id)
    .bind(req.template_id)
    .bind(&req.seller)
    .bind(&req.location)
    .bind(&snmp_version)
    .bind(&encrypted_community)
    .bind(&req.snmp_username)
    .bind(&req.snmp_auth_protocol)
    .bind(&encrypted_auth_password)
    .bind(&req.snmp_priv_protocol)
    .bind(&encrypted_priv_password)
    .bind(snmp_port)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    // 应用网卡配置（网卡 → 网口 → IP），未提供时自动生成默认可管理网卡+网口
    let cards = req.cards.unwrap_or_default();
    super::nic::apply_network_config(&mut tx, id, req.room_id, &cards, now).await?;
    let ip_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ips WHERE device_interface_id IN (SELECT id FROM device_interfaces WHERE device_id = $1)",
    )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;

    if req.save_as_template == Some(true) {
        let template_name = req.template_name.as_deref().unwrap_or(&req.name);
        let tmpl_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO device_templates (id, name, device_type, brand, model, description, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(tmpl_id)
        .bind(template_name)
        .bind(&final_device_type)
        .bind(&final_brand)
        .bind(&final_model)
        .bind(&req.description)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e
                && db_err.is_unique_violation()
            {
                return AppError::Conflict(msg("server.device_template.name_exists"));
            }
            AppError::from(e)
        })?;

        sqlx::query("UPDATE devices SET template_id = $1 WHERE id = $2")
            .bind(tmpl_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    // req 在此之后不再使用，字段直接移动，避免逐字段克隆
    let device = Device {
        id,
        name: req.name,
        hostname: req.hostname,
        device_type: final_device_type,
        brand: final_brand,
        model: final_model,
        serial_number: req.serial_number,
        workstation_id: req.workstation_id,
        position_id: req.position_id,
        room_id: req.room_id,
        template_id: req.template_id,
        seller: req.seller,
        location: req.location,
        snmp_version: Some(snmp_version),
        snmp_community: encrypted_community,
        snmp_username: req.snmp_username,
        snmp_auth_protocol: req.snmp_auth_protocol,
        snmp_auth_password: encrypted_auth_password,
        snmp_priv_protocol: req.snmp_priv_protocol,
        snmp_priv_password: encrypted_priv_password,
        snmp_port: Some(snmp_port),
        description: req.description,
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": device.name,
        "device_type": device.device_type,
        "brand": device.brand,
        "model": device.model,
        "serial_number": device.serial_number,
        "description": device.description,
        "ip_count": ip_count
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "device",
        Some(&id),
        &details,
    )
    .await;

    // 尽力而为的旁路通知：设备匹配工位且有 IP 时邮件通知管理人
    super::notify::spawn_ip_notification(state.pool()?.get_conn(), id);

    Ok(crate::error::ok_json(device, "server.device.created"))
}

pub async fn get_device(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();
    // 设备行与网卡配置并行拉取；三个 SNMP 凭据解密（AES-GCM）亦并行，
    // 替代原先 1(设备)+N(网卡)+M(网口)+K(IP)+3(解密) 的串行等待
    let (device_opt, cards) = tokio::join!(
        sqlx::query_as::<_, DeviceWithDetails>(
            "SELECT d.id, d.name, d.hostname, d.device_type, d.brand, d.model, d.serial_number,
                    d.workstation_id, d.position_id, d.room_id,
                    d.template_id, d.seller, d.location,
                    d.snmp_version, d.snmp_community, d.snmp_username,
                    d.snmp_auth_protocol, d.snmp_auth_password,
                    d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
                    d.description,
                    d.workstation_name, d.room_id, d.room_name, d.cabinet_id, d.cabinet_name,
                    d.start_u, d.end_u,
                    d.template_name,
                    d.created_at::TIMESTAMPTZ, d.updated_at::TIMESTAMPTZ
             FROM devices_with_details d
             WHERE d.id = $1",
        )
        .bind(id)
        .fetch_optional(&conn),
        super::nic::fetch_device_network_config(&conn, id)
    );

    let device = device_opt?.ok_or_else(|| AppError::NotFound(msg("server.device.not_found")))?;
    let cards = cards?;

    let (decrypted_community, decrypted_auth, decrypted_priv) = tokio::join!(
        crate::crypto::decrypt_credential_async(device.snmp_community.as_deref()),
        crate::crypto::decrypt_credential_async(device.snmp_auth_password.as_deref()),
        crate::crypto::decrypt_credential_async(device.snmp_priv_password.as_deref()),
    );
    let decrypted_community = decrypted_community?;
    let decrypted_auth = decrypted_auth?;
    let decrypted_priv = decrypted_priv?;

    let mut result = serde_json::to_value(&device)
        .map_err(|e| AppError::Internal(msg("server.common.serialize_failed").with("error", e)))?;
    result["cards"] = serde_json::to_value(cards)
        .map_err(|e| AppError::Internal(msg("server.common.serialize_failed").with("error", e)))?;
    result["snmp_community"] = serde_json::to_value(decrypted_community)
        .map_err(|e| AppError::Internal(msg("server.common.serialize_failed").with("error", e)))?;
    result["snmp_auth_password"] = serde_json::to_value(decrypted_auth)
        .map_err(|e| AppError::Internal(msg("server.common.serialize_failed").with("error", e)))?;
    result["snmp_priv_password"] = serde_json::to_value(decrypted_priv)
        .map_err(|e| AppError::Internal(msg("server.common.serialize_failed").with("error", e)))?;

    Ok(crate::error::ok_json(result, "server.device.fetched"))
}

pub async fn update_device(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<DeviceUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    // Validate device_type if provided
    if let Some(ref dt) = req.device_type {
        validate_device_type(dt)?;
    }

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM devices WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_none() {
        return Err(AppError::NotFound(msg("server.device.not_found")));
    }

    // Fetch current device data for business validations
    let current_row =
        sqlx::query("SELECT workstation_id, position_id, room_id FROM devices WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    let current_ws_id: Option<Uuid> = current_row.get("workstation_id");
    let current_pos_id: Option<Uuid> = current_row.get("position_id");
    let current_room_id: Uuid = current_row.get("room_id");

    // Resolve the final values for Option<Option<Uuid>> fields
    let resolved_workstation_id = match &req.workstation_id {
        None => current_ws_id,
        Some(Some(ws_id)) => {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workstations WHERE id = $1)")
                    .bind(ws_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if !exists {
                return Err(AppError::NotFound(msg("server.workstation.not_found")));
            }
            Some(*ws_id)
        }
        Some(None) => None,
    };

    let resolved_position_id = match &req.position_id {
        None => current_pos_id,
        Some(Some(pos_id)) => {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM positions WHERE id = $1)")
                    .bind(pos_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if !exists {
                return Err(AppError::NotFound(msg("server.position.not_found")));
            }
            Some(*pos_id)
        }
        Some(None) => None,
    };

    if resolved_workstation_id.is_some() && resolved_position_id.is_some() {
        return Err(AppError::Validation(msg(
            "server.device.workstation_position_exclusive",
        )));
    }

    let now = Utc::now();

    let encrypted_community = match &req.snmp_community {
        Some(c) if !c.is_empty() => Some(encrypt_password_async(c.clone()).await?),
        _ => None,
    };
    let encrypted_auth_password = match &req.snmp_auth_password {
        Some(p) if !p.is_empty() => Some(encrypt_password_async(p.clone()).await?),
        _ => None,
    };
    let encrypted_priv_password = match &req.snmp_priv_password {
        Some(p) if !p.is_empty() => Some(encrypt_password_async(p.clone()).await?),
        _ => None,
    };

    sqlx::query(
        "UPDATE devices SET
         name = COALESCE($1, name),
         hostname = COALESCE($27, hostname),
         device_type = COALESCE($2, device_type),
         brand = COALESCE($3, brand),
         model = COALESCE($4, model),
         serial_number = COALESCE($5, serial_number),
         workstation_id = CASE WHEN $6::boolean THEN $7 ELSE workstation_id END,
         position_id = CASE WHEN $8::boolean THEN $9 ELSE position_id END,
         room_id = COALESCE($10, room_id),
         seller = COALESCE($11, seller),
         location = COALESCE($12, location),
         snmp_version = COALESCE($13, snmp_version),
         snmp_community = CASE WHEN $14::boolean THEN $15 ELSE snmp_community END,
         snmp_username = COALESCE($16, snmp_username),
         snmp_auth_protocol = COALESCE($17, snmp_auth_protocol),
         snmp_auth_password = CASE WHEN $18::boolean THEN $19 ELSE snmp_auth_password END,
         snmp_priv_protocol = COALESCE($20, snmp_priv_protocol),
         snmp_priv_password = CASE WHEN $21::boolean THEN $22 ELSE snmp_priv_password END,
         snmp_port = COALESCE($23, snmp_port),
         description = COALESCE($24, description),
         updated_at = $25
         WHERE id = $26",
    )
    .bind(&req.name)
    .bind(&req.device_type)
    .bind(&req.brand)
    .bind(&req.model)
    .bind(&req.serial_number)
    .bind(req.workstation_id.is_some())
    .bind(resolved_workstation_id)
    .bind(req.position_id.is_some())
    .bind(resolved_position_id)
    .bind(req.room_id)
    .bind(&req.seller)
    .bind(&req.location)
    .bind(&req.snmp_version)
    .bind(req.snmp_community.is_some())
    .bind(&encrypted_community)
    .bind(&req.snmp_username)
    .bind(&req.snmp_auth_protocol)
    .bind(req.snmp_auth_password.is_some())
    .bind(&encrypted_auth_password)
    .bind(&req.snmp_priv_protocol)
    .bind(req.snmp_priv_password.is_some())
    .bind(&encrypted_priv_password)
    .bind(req.snmp_port)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .bind(&req.hostname)
    .execute(&mut *tx)
    .await?;

    // Handle network config replacement if cards are provided
    if let Some(cards) = &req.cards {
        let room_id = req.room_id.unwrap_or(current_room_id);
        super::nic::apply_network_config(&mut tx, id, room_id, cards, now).await?;
    }

    if req.save_as_template == Some(true) {
        let template_name = req
            .template_name
            .as_deref()
            .unwrap_or(req.name.as_deref().unwrap_or(""));
        let tmpl_id = Uuid::new_v4();

        let current_device_type: String =
            sqlx::query_scalar("SELECT device_type FROM devices WHERE id = $1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        let current_brand: Option<String> =
            sqlx::query_scalar("SELECT brand FROM devices WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
        let current_model: Option<String> =
            sqlx::query_scalar("SELECT model FROM devices WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
        let current_description: Option<String> =
            sqlx::query_scalar("SELECT description FROM devices WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();

        sqlx::query(
            "INSERT INTO device_templates (id, name, device_type, brand, model, description, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(tmpl_id)
        .bind(template_name)
        .bind(&current_device_type)
        .bind(&current_brand)
        .bind(&current_model)
        .bind(&current_description)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e
                && db_err.is_unique_violation()
            {
                return AppError::Conflict(msg("server.device_template.name_exists"));
            }
            AppError::from(e)
        })?;

        sqlx::query("UPDATE devices SET template_id = $1 WHERE id = $2")
            .bind(tmpl_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    // Fetch updated device with details
    let updated_device = sqlx::query_as::<_, DeviceWithDetails>(
        "SELECT d.id, d.name, d.hostname, d.device_type, d.brand, d.model, d.serial_number,
                d.workstation_id, d.position_id, d.room_id,
                d.template_id, d.seller, d.location,
                d.snmp_version, d.snmp_community, d.snmp_username,
                d.snmp_auth_protocol, d.snmp_auth_password,
                d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
                d.description,
                d.workstation_name, d.room_id, d.room_name, d.cabinet_id, d.cabinet_name,
                d.start_u, d.end_u,
                d.template_name,
                d.created_at::TIMESTAMPTZ, d.updated_at::TIMESTAMPTZ
         FROM devices_with_details d
         WHERE d.id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    // Fetch updated network cards (with nested ports and IPs)
    let cards = super::nic::fetch_device_network_config(&state.pool()?.get_conn(), id).await?;

    let mut result = serde_json::to_value(&updated_device)
        .map_err(|e| AppError::Internal(msg("server.common.serialize_failed").with("error", e)))?;
    result["cards"] = serde_json::to_value(cards)
        .map_err(|e| AppError::Internal(msg("server.common.serialize_failed").with("error", e)))?;

    let details = serde_json::json!({
        "name": updated_device.name,
        "device_type": updated_device.device_type,
        "brand": updated_device.brand,
        "model": updated_device.model,
        "description": updated_device.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "device",
        Some(&id),
        &details,
    )
    .await;

    // 尽力而为的旁路通知：设备编辑完成且匹配工位时邮件通知管理人 IP 信息
    super::notify::spawn_ip_notification(state.pool()?.get_conn(), id);

    Ok(crate::error::ok_json(result, "server.device.updated"))
}

pub async fn delete_device(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM devices WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_none() {
        return Err(AppError::NotFound(msg("server.device.not_found")));
    }

    // 预清理引用本设备接口的线路：devices 级联删除 device_interfaces
    // 时会触发 cable_links 的防删触发器，直接 DELETE 会报 500 且设备
    // 永远无法删除（db-schema-review R1，已实测确认）。
    // 与 delete_device_interface / apply_network_config 的清理口径一致。
    sqlx::query(
        r"DELETE FROM cable_links
         WHERE (a_endpoint_type = 'device_interface' AND a_endpoint_id IN (SELECT id FROM device_interfaces WHERE device_id = $1))
            OR (b_endpoint_type = 'device_interface' AND b_endpoint_id IN (SELECT id FROM device_interfaces WHERE device_id = $1))",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    // 拓扑逻辑连线成员引用本设备端口：随设备删除一并清理
    sqlx::query(
        "DELETE FROM topology_connection_members WHERE device_id = $1 OR device_port_id IN (SELECT id FROM device_interfaces WHERE device_id = $1)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM topology_nodes WHERE device_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM devices WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "device_id": id.to_string()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "device",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json((), "server.device.deleted"))
}
