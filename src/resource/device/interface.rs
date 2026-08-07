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
                di.description, di.switch_id, di.uplink_interface_id, di.net_outlet_ids,
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
                di.description, di.switch_id, di.uplink_interface_id, di.net_outlet_ids,
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

/// 验证信息点链完整性并推导上级端口。
/// 规则：多个信息点时，除最后一个外必须有对端（peer_type 不为空）；
/// 最后一个信息点的对端为 switch_port 时，自动推导 switch_id/uplink_interface_id。
pub async fn validate_and_resolve_outlet_chain(
    executor: &mut sqlx::PgConnection,
    net_outlet_ids: &[Uuid],
    switch_id: &mut Option<Uuid>,
    uplink_interface_id: &mut Option<Uuid>,
) -> Result<(), AppError> {
    if net_outlet_ids.is_empty() {
        return Ok(());
    }

    // 批量查询信息点的 peer 信息
    let outlets: Vec<(Uuid, Option<String>, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, peer_type, peer_switch_port_id FROM net_outlets WHERE id = ANY($1)",
    )
    .bind(net_outlet_ids)
    .fetch_all(&mut *executor)
    .await?;

    let outlet_map: std::collections::HashMap<Uuid, (Option<String>, Option<Uuid>)> = outlets
        .into_iter()
        .map(|(id, pt, psp)| (id, (pt, psp)))
        .collect();

    // 验证：非最后信息点必须有对端
    if net_outlet_ids.len() > 1 {
        for outlet_id in &net_outlet_ids[..net_outlet_ids.len() - 1] {
            if let Some((peer_type, _)) = outlet_map.get(outlet_id)
                && (peer_type.is_none() || peer_type.as_ref().is_some_and(|pt| pt.is_empty()))
            {
                return Err(AppError::Validation(
                    "链路中除最后一个信息点外，其他信息点必须配置对端".to_string(),
                ));
            }
        }
    }

    // 自动推导：最后一个信息点的对端为 switch_port 时
    if let Some(last_id) = net_outlet_ids.last()
        && let Some((peer_type, peer_switch_port_id)) = outlet_map.get(last_id)
        && peer_type.as_deref() == Some("switch_port")
        && let Some(sp_id) = peer_switch_port_id
    {
        // 从 switch_ports 查 device_id，同时查找对应的 device_interface
        let resolved: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
            r"SELECT sp.device_id,
                     (SELECT di.id FROM device_interfaces di
                      WHERE di.device_id = sp.device_id
                        AND di.name IN (sp.port_name, sp.port_number)
                      LIMIT 1) AS iface_id
                  FROM switch_ports sp WHERE sp.id = $1",
        )
        .bind(sp_id)
        .fetch_optional(&mut *executor)
        .await?;

        if let Some((dev_id, iface_id)) = resolved {
            *switch_id = Some(dev_id);
            *uplink_interface_id = iface_id;
        }
    }

    Ok(())
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

    // 验证信息点链并推导上级端口
    let mut resolved_switch_id = req.switch_id;
    let mut resolved_uplink_interface_id = req.uplink_interface_id;
    let mut conn = state.pool()?.get_conn().acquire().await?;
    validate_and_resolve_outlet_chain(
        &mut conn,
        &req.net_outlet_ids,
        &mut resolved_switch_id,
        &mut resolved_uplink_interface_id,
    )
    .await?;

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        r"INSERT INTO device_interfaces (
            id, device_id, name, interface_type, mac_address, vlan_id, description, switch_id, uplink_interface_id, net_outlet_ids, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(id)
    .bind(device_id)
    .bind(&req.name)
    .bind(interface_type)
    .bind(&req.mac_address)
    .bind(req.vlan_id)
    .bind(&req.description)
    .bind(resolved_switch_id)
    .bind(resolved_uplink_interface_id)
    .bind(&req.net_outlet_ids)
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
            di.description, di.switch_id, di.uplink_interface_id, di.net_outlet_ids,
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
    AppJson(mut req): AppJson<DeviceInterfaceUpdate>,
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

    // 如果 net_outlet_ids 有更新，验证信息点链并推导上级端口
    if let Some(ref outlet_ids) = req.net_outlet_ids {
        // 获取当前或请求中的 switch_id/uplink_interface_id
        let current: Option<(Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
            "SELECT switch_id, uplink_interface_id FROM device_interfaces WHERE id = $1",
        )
        .bind(interface_id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

        let mut sid = match &req.switch_id {
            Some(Some(id)) => Some(*id),
            Some(None) => None,
            None => current.as_ref().and_then(|c| c.0),
        };
        let mut uifid = match &req.uplink_interface_id {
            Some(Some(id)) => Some(*id),
            Some(None) => None,
            None => current.as_ref().and_then(|c| c.1),
        };

        let mut conn = state.pool()?.get_conn().acquire().await?;
        validate_and_resolve_outlet_chain(&mut conn, outlet_ids, &mut sid, &mut uifid).await?;

        // 仅在推导产生值时覆盖请求值，避免把未传字段强制置 NULL
        if sid.is_some() {
            req.switch_id = Some(sid);
            req.uplink_interface_id = Some(uifid);
        }
    }

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

    let net_outlet_ids_update = req.net_outlet_ids.is_some();
    if net_outlet_ids_update {
        set_clauses.push(format!("net_outlet_ids = ${param_index}"));
        param_index += 1;
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

    if net_outlet_ids_update {
        query = query.bind(&req.net_outlet_ids);
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
