use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use uuid::Uuid;
use validator::Validate;

use super::snmp::{DeviceForSnmp, get_device_ports_via_snmp};
use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{DevicePort, DevicePortCreate, DevicePortUpdate, DevicePortWithDevice};
use crate::routes::static_files::AppJson;
use crate::utils::common::RequestMeta;
use crate::utils::pagination::Pagination;
use crate::utils::{OperationLogParams, log_system_operation};
use tracing::warn;

pub async fn get_device_ports(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_ports WHERE device_id = $1")
        .bind(device_id)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let data = sqlx::query_as::<_, DevicePort>(
        r"SELECT * FROM device_ports WHERE device_id = $1 ORDER BY port_number LIMIT $2 OFFSET $3",
    )
    .bind(device_id)
    .bind(page_size)
    .bind(offset)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(crate::error::ok_json(
        serde_json::json!({
            "items": data,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "获取端口列表成功",
    ))
}

pub async fn get_all_device_ports(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();
    let room_id = query.get("room_id").cloned();

    let search_pattern = if search.is_empty() {
        None
    } else {
        Some(format!("%{search}%"))
    };

    let parsed_room_id = room_id
        .as_ref()
        .map(|id| {
            Uuid::parse_str(id).map_err(|_| AppError::Validation("无效的room_id参数".to_string()))
        })
        .transpose()?;

    let has_room_filter = parsed_room_id.is_some();
    let has_search = search_pattern.is_some();

    let mut where_parts: Vec<String> = Vec::new();
    let mut param_idx = 1;

    if has_search {
        where_parts.push(format!(
            "(d.name ILIKE ${param_idx} OR sp.port_number::TEXT ILIKE ${param_idx} OR sp.port_name ILIKE ${param_idx} OR sp.description ILIKE ${param_idx})"
        ));
        param_idx += 1;
    }
    if has_room_filter {
        where_parts.push(format!("d.room_id = ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_parts.join(" AND "))
    };

    let total: i64 = {
        let sql = format!(
            "SELECT COUNT(*) FROM device_ports sp JOIN devices d ON sp.device_id = d.id {where_clause}"
        );
        let mut q = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql));
        if has_search {
            q = q.bind(search_pattern.as_ref().unwrap());
        }
        if has_room_filter {
            q = q.bind(parsed_room_id);
        }
        q.fetch_one(&state.pool()?.get_conn()).await?
    };

    let data = {
        let sql = format!(
            r"SELECT
                sp.id, sp.device_id, d.name as device_name,
                COALESCE(
                    (SELECT host(im.ip_address) FROM ips im WHERE im.device_id = d.id LIMIT 1),
                    ''
                ) as device_ip,
                sp.port_number, sp.port_name, sp.port_type, sp.vlan_id,
                sp.status, sp.speed, sp.description, sp.created_at, sp.updated_at
            FROM device_ports sp
            JOIN devices d ON sp.device_id = d.id
            {where_clause}
            ORDER BY d.name, sp.port_number
            LIMIT ${param_idx} OFFSET ${}",
            param_idx + 1
        );
        let mut q = sqlx::query_as::<_, DevicePortWithDevice>(sqlx::AssertSqlSafe(sql));
        if has_search {
            q = q.bind(search_pattern.as_ref().unwrap());
        }
        if has_room_filter {
            q = q.bind(parsed_room_id);
        }
        q = q.bind(page_size).bind(offset);
        q.fetch_all(&state.pool()?.get_conn()).await?
    };

    Ok(crate::error::ok_json(
        serde_json::json!({
            "items": data,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "获取所有端口列表成功",
    ))
}

pub async fn create_device_port(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<DevicePortCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let device_exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
            .bind(device_id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    if !device_exists {
        return Err(AppError::NotFound("设备不存在".to_string()));
    }

    let port_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM device_ports WHERE device_id = $1 AND port_number = $2)",
    )
    .bind(device_id)
    .bind(&req.port_number)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    if port_exists {
        return Err(AppError::Conflict("该端口号已存在".to_string()));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        r"INSERT INTO device_ports (
            id, device_id, port_number, port_name, port_type, vlan_id,
            status, speed, description, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(id)
    .bind(device_id)
    .bind(&req.port_number)
    .bind(&req.port_name)
    .bind(req.port_type.as_deref().unwrap_or("access"))
    .bind(req.vlan_id)
    .bind(req.status.as_deref().unwrap_or("up"))
    .bind(&req.speed)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&state.pool()?.get_conn())
    .await?;

    let data = sqlx::query_as::<_, DevicePort>("SELECT * FROM device_ports WHERE id = $1")
        .bind(id)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let details = serde_json::json!({
        "device_id": device_id,
        "port_number": data.port_number,
        "port_name": data.port_name,
        "port_type": data.port_type,
        "vlan_id": data.vlan_id
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            ip_address: &meta.ip_address,
            user_id: meta.user_id(),
            action: "create",
            resource_type: "device_port",
            resource_id: Some(&id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(crate::error::ok_json(data, "创建端口成功"))
}

pub async fn get_device_port(
    State(state): State<Arc<AppState>>,
    Path(port_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let data = sqlx::query_as::<_, DevicePortWithDevice>(
        r"SELECT
            sp.id, sp.device_id, d.name as device_name,
            COALESCE(
                (SELECT host(im.ip_address) FROM ips im WHERE im.device_id = d.id LIMIT 1),
                ''
            ) as device_ip,
            sp.port_number, sp.port_name, sp.port_type, sp.vlan_id,
            sp.status, sp.speed, sp.description, sp.created_at, sp.updated_at
        FROM device_ports sp
        JOIN devices d ON sp.device_id = d.id
        WHERE sp.id = $1",
    )
    .bind(port_id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("端口不存在".to_string()))?;

    Ok(crate::error::ok_json(data, "获取端口成功"))
}

pub async fn update_device_port(
    State(state): State<Arc<AppState>>,
    Path(port_id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<DevicePortUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let now = Utc::now();

    let result = sqlx::query(
        r"UPDATE device_ports SET
            port_number = COALESCE($1, port_number),
            port_name = COALESCE($2, port_name),
            port_type = COALESCE($3, port_type),
            vlan_id = COALESCE($4, vlan_id),
            status = COALESCE($5, status),
            speed = COALESCE($6, speed),
            description = COALESCE($7, description),
            updated_at = $8
        WHERE id = $9",
    )
    .bind(&req.port_number)
    .bind(&req.port_name)
    .bind(&req.port_type)
    .bind(req.vlan_id)
    .bind(&req.status)
    .bind(&req.speed)
    .bind(&req.description)
    .bind(now)
    .bind(port_id)
    .execute(&state.pool()?.get_conn())
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("端口不存在".to_string()));
    }

    let data = sqlx::query_as::<_, DevicePort>("SELECT * FROM device_ports WHERE id = $1")
        .bind(port_id)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let details = serde_json::json!({
        "device_id": data.device_id,
        "port_number": data.port_number,
        "port_name": data.port_name,
        "port_type": data.port_type,
        "vlan_id": data.vlan_id
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            ip_address: &meta.ip_address,
            user_id: meta.user_id(),
            action: "update",
            resource_type: "device_port",
            resource_id: Some(&port_id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(crate::error::ok_json(data, "更新端口成功"))
}

pub async fn delete_device_port(
    State(state): State<Arc<AppState>>,
    Path(port_id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let result = sqlx::query("DELETE FROM device_ports WHERE id = $1")
        .bind(port_id)
        .execute(&state.pool()?.get_conn())
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e
                && db_err.is_foreign_key_violation()
            {
                return AppError::Validation("该端口已被 cable_links 引用，无法删除".to_string());
            }
            AppError::from(e)
        })?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("端口不存在".to_string()));
    }

    let details = serde_json::json!({
        "port_id": port_id
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            ip_address: &meta.ip_address,
            user_id: meta.user_id(),
            action: "delete",
            resource_type: "device_port",
            resource_id: Some(&port_id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(crate::error::ok_json((), "删除端口成功"))
}

pub async fn sync_ports_from_snmp(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let switch_data = sqlx::query_as::<_, DeviceForSnmp>(
        r"SELECT
            id, name, snmp_version, snmp_community,
            snmp_username, snmp_auth_protocol,
            snmp_auth_password, snmp_priv_protocol,
            snmp_priv_password, snmp_port
        FROM devices WHERE id = $1",
    )
    .bind(device_id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("设备不存在".to_string()))?;

    let ip_address: Option<String> = sqlx::query_scalar(
        r"SELECT host(ip_address) FROM ips
           WHERE device_id = $1
           ORDER BY created_at LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?;

    let ip_address = match ip_address {
        Some(ref ip) if !ip.is_empty() => ip,
        _ => {
            return Err(AppError::Validation("设备没有配置IP地址".to_string()));
        }
    };

    let snmp_params = switch_data.to_snmp_params_async(ip_address).await?;

    let ports = get_device_ports_via_snmp(&snmp_params)
        .await
        .map_err(|e| AppError::Snmp(format!("获取设备端口信息失败: {e}")))?;

    let mut saved_count = 0;
    let mut skipped_count = 0;
    let mut error_count = 0;

    for port in &ports {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM device_ports WHERE device_id = $1 AND port_number = $2)",
        )
        .bind(device_id)
        .bind(&port.port_number)
        .fetch_one(&state.pool()?.get_conn())
        .await
        .map_err(|e| {
            tracing::error!("检查端口是否存在时数据库查询失败: {}", e);
            AppError::Database(format!("检查端口是否存在失败: {e}"))
        })?;

        if exists {
            skipped_count += 1;
            continue;
        }

        let id = Uuid::new_v4();
        let now = Utc::now();

        let result = sqlx::query(
            r"INSERT INTO device_ports (
                id, device_id, port_number, port_name, port_type, vlan_id,
                status, speed, description, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(id)
        .bind(device_id)
        .bind(&port.port_number)
        .bind(&port.port_name)
        .bind(port.port_type.as_deref().unwrap_or("access"))
        .bind(port.vlan_id)
        .bind(port.status.as_deref().unwrap_or("up"))
        .bind(&port.speed)
        .bind(&port.description)
        .bind(now)
        .bind(now)
        .execute(&state.pool()?.get_conn())
        .await;

        if let Err(e) = result {
            tracing::error!("插入端口 {} 失败: {}", port.port_number, e);
            error_count += 1;
        } else {
            saved_count += 1;
        }
    }

    let saved_ports = sqlx::query_as::<_, DevicePort>(
        "SELECT * FROM device_ports WHERE device_id = $1 ORDER BY port_number",
    )
    .bind(device_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let message = if error_count > 0 {
        format!("保存 {saved_count} 个端口，跳过 {skipped_count} 个，失败 {error_count} 个")
    } else if saved_count > 0 && skipped_count > 0 {
        format!("成功保存 {saved_count} 个端口，跳过 {skipped_count} 个已存在的端口")
    } else if saved_count > 0 {
        format!("成功保存 {saved_count} 个端口到数据库")
    } else if skipped_count > 0 {
        format!("所有 {skipped_count} 个端口已存在，跳过保存")
    } else {
        "未获取到端口信息".to_string()
    };

    Ok(crate::error::ok_json(saved_ports, &message))
}
