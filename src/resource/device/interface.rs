use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    DeviceInterface, DeviceInterfaceCreate, DeviceInterfaceUpdate, DeviceInterfaceWithDevice,
};
use crate::routes::static_files::AppJson;
use crate::utils::common::RequestMeta;
use crate::utils::pagination::Pagination;
use crate::utils::{OperationLogParams, log_system_operation};
use tracing::warn;

pub async fn get_device_interfaces(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM device_interfaces WHERE device_id = $1")
            .bind(device_id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    let data = sqlx::query_as::<_, DeviceInterface>(
        r"SELECT * FROM device_interfaces WHERE device_id = $1 ORDER BY name LIMIT $2 OFFSET $3",
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
        "获取接口列表成功",
    ))
}

pub async fn get_all_device_interfaces(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page = pagination.page;
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();

    let search_pattern = if search.is_empty() {
        None
    } else {
        Some(format!("%{search}%"))
    };

    let total: i64 = if let Some(ref pattern) = search_pattern {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM device_interfaces di JOIN devices d ON di.device_id = d.id WHERE d.name ILIKE $1 OR di.name ILIKE $1 OR di.mac_address ILIKE $1 OR di.description ILIKE $1"
        )
        .bind(pattern)
        .fetch_one(&state.pool()?.get_conn())
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM device_interfaces di JOIN devices d ON di.device_id = d.id",
        )
        .fetch_one(&state.pool()?.get_conn())
        .await?
    };

    let data = if let Some(ref pattern) = search_pattern {
        sqlx::query_as::<_, DeviceInterfaceWithDevice>(
            r"SELECT
                di.id, di.device_id, d.name as device_name,
                di.nic_id, di.name, di.interface_type, di.mac_address, di.vlan_id,
                di.description, di.switch_id, di.uplink_interface_id,
                di.sort_order, di.created_at, di.updated_at
            FROM device_interfaces di
            JOIN devices d ON di.device_id = d.id
            WHERE d.name ILIKE $1 OR di.name ILIKE $1 OR di.mac_address ILIKE $1 OR di.description ILIKE $1
            ORDER BY d.name, di.name
            LIMIT $2 OFFSET $3"
        )
        .bind(pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?
    } else {
        sqlx::query_as::<_, DeviceInterfaceWithDevice>(
            r"SELECT
                di.id, di.device_id, d.name as device_name,
                di.nic_id, di.name, di.interface_type, di.mac_address, di.vlan_id,
                di.description, di.switch_id, di.uplink_interface_id,
                di.sort_order, di.created_at, di.updated_at
            FROM device_interfaces di
            JOIN devices d ON di.device_id = d.id
            ORDER BY d.name, di.name
            LIMIT $1 OFFSET $2",
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(&state.pool()?.get_conn())
        .await?
    };

    Ok(crate::error::ok_json(
        serde_json::json!({
            "items": data,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "获取所有接口列表成功",
    ))
}


pub async fn create_device_interface(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<DeviceInterfaceCreate>,
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

    let interface_type = req.interface_type.as_deref().unwrap_or("physical");

    if !matches!(
        interface_type,
        "physical" | "svi" | "management" | "loopback" | "wifi"
    ) {
        return Err(AppError::Validation(
            "接口类型必须是physical、svi、management、loopback或wifi".to_string(),
        ));
    }

    let interface_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM device_interfaces WHERE device_id = $1 AND name = $2)",
    )
    .bind(device_id)
    .bind(&req.name)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    if interface_exists {
        return Err(AppError::Conflict("该接口名已存在".to_string()));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        r"INSERT INTO device_interfaces (
            id, device_id, name, interface_type, mac_address, vlan_id, description, switch_id, uplink_interface_id, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(id)
    .bind(device_id)
    .bind(&req.name)
    .bind(interface_type)
    .bind(&req.mac_address)
    .bind(req.vlan_id)
    .bind(&req.description)
    .bind(req.switch_id)
    .bind(req.uplink_interface_id)
    .bind(now)
    .bind(now)
    .execute(&state.pool()?.get_conn())
    .await?;

    let data =
        sqlx::query_as::<_, DeviceInterface>("SELECT * FROM device_interfaces WHERE id = $1")
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    let details = serde_json::json!({
        "device_id": device_id,
        "name": data.name,
        "interface_type": data.interface_type,
        "mac_address": data.mac_address
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            ip_address: &meta.ip_address,
            user_id: meta.user_id(),
            action: "create",
            resource_type: "device_interface",
            resource_id: Some(&id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(crate::error::ok_json(data, "创建接口成功"))
}

pub async fn get_device_interface(
    State(state): State<Arc<AppState>>,
    Path(interface_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let data = sqlx::query_as::<_, DeviceInterfaceWithDevice>(
        r"SELECT
            di.id, di.device_id, d.name as device_name,
            di.nic_id, di.name, di.interface_type, di.mac_address, di.vlan_id,
            di.description, di.switch_id, di.uplink_interface_id,
            di.sort_order, di.created_at, di.updated_at
        FROM device_interfaces di
        JOIN devices d ON di.device_id = d.id
        WHERE di.id = $1",
    )
    .bind(interface_id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound("接口不存在".to_string()))?;

    Ok(crate::error::ok_json(data, "获取接口成功"))
}

pub async fn update_device_interface(
    State(state): State<Arc<AppState>>,
    Path(interface_id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<DeviceInterfaceUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    if let Some(ref interface_type) = req.interface_type
        && !matches!(
            interface_type.as_str(),
            "physical" | "svi" | "management" | "loopback" | "wifi"
        )
    {
        return Err(AppError::Validation(
            "接口类型必须是physical、svi、management、loopback或wifi".to_string(),
        ));
    }

    let now = Utc::now();

    let mut set_clauses: Vec<String> = Vec::new();
    let mut param_index = 1;

    set_clauses.push(format!("name = COALESCE(${param_index}, name)"));
    param_index += 1;

    set_clauses.push(format!(
        "interface_type = COALESCE(${param_index}, interface_type)"
    ));
    param_index += 1;

    let mac_update = req.mac_address.is_some();
    if mac_update {
        set_clauses.push(format!(
            "mac_address = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE mac_address END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    set_clauses.push(format!("vlan_id = COALESCE(${param_index}, vlan_id)"));
    param_index += 1;

    let desc_update = req.description.is_some();
    if desc_update {
        set_clauses.push(format!(
            "description = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE description END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    let switch_id_update = req.switch_id.is_some();
    if switch_id_update {
        set_clauses.push(format!(
            "switch_id = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE switch_id END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    let uplink_interface_id_update = req.uplink_interface_id.is_some();
    if uplink_interface_id_update {
        set_clauses.push(format!(
            "uplink_interface_id = CASE WHEN ${param_index}::boolean IS TRUE THEN ${param_idx_val} ELSE uplink_interface_id END",
            param_index = param_index,
            param_idx_val = param_index + 1
        ));
        param_index += 2;
    }

    set_clauses.push(format!("updated_at = ${param_index}"));
    param_index += 1;

    let where_param = param_index;

    let sql = format!(
        "UPDATE device_interfaces SET {} WHERE id = ${}",
        set_clauses.join(", "),
        where_param
    );

    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
    query = query.bind(&req.name);
    query = query.bind(&req.interface_type);

    if mac_update {
        match &req.mac_address {
            Some(Some(m)) => {
                query = query.bind(true);
                query = query.bind(m);
            }
            Some(None) => {
                query = query.bind(true);
                query = query.bind(Option::<String>::None);
            }
            None => unreachable!(),
        }
    }

    query = query.bind(req.vlan_id);

    if desc_update {
        match &req.description {
            Some(Some(d)) => {
                query = query.bind(true);
                query = query.bind(d);
            }
            Some(None) => {
                query = query.bind(true);
                query = query.bind(Option::<String>::None);
            }
            None => unreachable!(),
        }
    }

    if switch_id_update {
        match &req.switch_id {
            Some(Some(sid)) => {
                query = query.bind(true);
                query = query.bind(sid);
            }
            Some(None) => {
                query = query.bind(true);
                query = query.bind(Option::<Uuid>::None);
            }
            None => unreachable!(),
        }
    }

    if uplink_interface_id_update {
        match &req.uplink_interface_id {
            Some(Some(uifid)) => {
                query = query.bind(true);
                query = query.bind(uifid);
            }
            Some(None) => {
                query = query.bind(true);
                query = query.bind(Option::<Uuid>::None);
            }
            None => unreachable!(),
        }
    }

    query = query.bind(now);
    query = query.bind(interface_id);

    let result = query.execute(&state.pool()?.get_conn()).await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("接口不存在".to_string()));
    }

    let data =
        sqlx::query_as::<_, DeviceInterface>("SELECT * FROM device_interfaces WHERE id = $1")
            .bind(interface_id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    let details = serde_json::json!({
        "device_id": data.device_id,
        "name": data.name,
        "interface_type": data.interface_type,
        "mac_address": data.mac_address
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            ip_address: &meta.ip_address,
            user_id: meta.user_id(),
            action: "update",
            resource_type: "device_interface",
            resource_id: Some(&interface_id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(crate::error::ok_json(data, "更新接口成功"))
}

pub async fn delete_device_interface(
    State(state): State<Arc<AppState>>,
    Path(interface_id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let mut tx = state.pool()?.get_conn().begin().await?;

    // 先删除相关的 IP 地址
    sqlx::query("DELETE FROM ips WHERE device_interface_id = $1")
        .bind(interface_id)
        .execute(&mut *tx)
        .await?;

    // 删除相关的电缆链接
    sqlx::query(
        r"DELETE FROM cable_links
         WHERE (a_endpoint_type = 'device_interface' AND a_endpoint_id = $1)
            OR (b_endpoint_type = 'device_interface' AND b_endpoint_id = $1)",
    )
    .bind(interface_id)
    .execute(&mut *tx)
    .await?;

    // 删除接口
    let result = sqlx::query("DELETE FROM device_interfaces WHERE id = $1")
        .bind(interface_id)
        .execute(&mut *tx)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("接口不存在".to_string()));
    }

    tx.commit().await?;

    let details = serde_json::json!({
        "interface_id": interface_id
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            ip_address: &meta.ip_address,
            user_id: meta.user_id(),
            action: "delete",
            resource_type: "device_interface",
            resource_id: Some(&interface_id),
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(crate::error::ok_json((), "删除接口成功"))
}
