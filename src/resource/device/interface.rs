//! 设备网络接口（device_interfaces）资源管理。
//!
//! 接口按 `physical_type`（物理形态：rj45/sfp/.../virtual）与
//! `interface_role`（角色：management/business/...）两个正交维度描述，
//! 枚举校验复用 `nic` 模块的统一函数。更新时字段缺失表示不修改，
//! 可空字段（MAC/描述等）以 `Some(None)` 显式置空。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::error::{AppError, msg};
use crate::models::{
    DeviceInterface, DeviceInterfaceCreate, DeviceInterfaceUpdate, DeviceInterfaceWithDevice,
};
use crate::resource::device::nic::{validate_interface_role, validate_physical_type};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::{Pagination, paged_response};

/// 接口联表查询列（含所属设备名），列表与单条查询共用。
const INTERFACE_WITH_DEVICE_COLUMNS: &str = "di.id, di.device_id, d.name as device_name,
                di.nic_id, di.name, di.physical_type, di.interface_role, di.mac_address, di.vlan_id,
                di.description, di.sort_order, di.created_at, di.updated_at";

/// 分页获取指定设备的接口列表。
pub async fn get_device_interfaces(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM device_interfaces WHERE device_id = $1")
            .bind(device_id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    let data = sqlx::query_as::<_, DeviceInterface>(
        r"SELECT * FROM device_interfaces WHERE device_id = $1 ORDER BY name LIMIT $2 OFFSET $3",
    )
    .bind(device_id)
    .bind(pagination.page_size)
    .bind(pagination.offset)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(crate::error::ok_json(
        paged_response(data, total, &pagination),
        "server.device.interface.list_retrieved",
    ))
}

/// 分页获取全部设备接口（跨设备视图，支持关键字模糊匹配）。
pub async fn get_all_device_interfaces(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let search = query.get("search").cloned().unwrap_or_default();
    let search_pattern = (!search.is_empty()).then(|| crate::utils::escape_like(&search));

    let mut count_builder = QueryBuilder::<Postgres>::new(
        "SELECT COUNT(*) FROM device_interfaces di JOIN devices d ON di.device_id = d.id",
    );
    let mut data_builder = QueryBuilder::<Postgres>::new(format!(
        "SELECT {INTERFACE_WITH_DEVICE_COLUMNS}
            FROM device_interfaces di
            JOIN devices d ON di.device_id = d.id"
    ));
    if let Some(pattern) = &search_pattern {
        for builder in [&mut count_builder, &mut data_builder] {
            builder
                .push(" WHERE d.name ILIKE ")
                .push_bind(pattern)
                .push(" OR di.name ILIKE ")
                .push_bind(pattern)
                .push(" OR di.mac_address ILIKE ")
                .push_bind(pattern)
                .push(" OR di.description ILIKE ")
                .push_bind(pattern);
        }
    }

    let total: i64 = count_builder
        .build_query_scalar()
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    data_builder
        .push(" ORDER BY d.name, di.name LIMIT ")
        .push_bind(pagination.page_size)
        .push(" OFFSET ")
        .push_bind(pagination.offset);
    let data = data_builder
        .build_query_as::<DeviceInterfaceWithDevice>()
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    Ok(crate::error::ok_json(
        paged_response(data, total, &pagination),
        "server.device.interface.list_all_retrieved",
    ))
}

/// 为设备创建网络接口（同设备接口名唯一，存在性检查与写入在同一事务内）。
pub async fn create_device_interface(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<DeviceInterfaceCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let physical_type = req.physical_type.as_deref().unwrap_or("rj45");
    validate_physical_type(physical_type)?;
    let interface_role = req.interface_role.as_deref().unwrap_or("business");
    validate_interface_role(interface_role)?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let device_exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
            .bind(device_id)
            .fetch_one(&mut *tx)
            .await?;
    if !device_exists {
        return Err(AppError::NotFound(msg("server.device.not_found")));
    }

    let interface_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM device_interfaces WHERE device_id = $1 AND name = $2)",
    )
    .bind(device_id)
    .bind(&req.name)
    .fetch_one(&mut *tx)
    .await?;
    if interface_exists {
        return Err(AppError::Conflict(msg(
            "server.device.interface.name_exists",
        )));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        r"INSERT INTO device_interfaces (
            id, device_id, name, physical_type, interface_role, mac_address, vlan_id, description, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(device_id)
    .bind(&req.name)
    .bind(physical_type)
    .bind(interface_role)
    .bind(&req.mac_address)
    .bind(req.vlan_id)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let data =
        sqlx::query_as::<_, DeviceInterface>("SELECT * FROM device_interfaces WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "device_id": device_id,
        "name": data.name,
        "physical_type": data.physical_type,
        "interface_role": data.interface_role,
        "mac_address": data.mac_address
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "device_interface",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(
        data,
        "server.device.interface.created",
    ))
}

/// 获取单个接口详情（含所属设备名）。
pub async fn get_device_interface(
    State(state): State<Arc<AppState>>,
    Path(interface_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let data = sqlx::query_as::<_, DeviceInterfaceWithDevice>(sqlx::AssertSqlSafe(format!(
        "SELECT {INTERFACE_WITH_DEVICE_COLUMNS}
            FROM device_interfaces di
            JOIN devices d ON di.device_id = d.id
            WHERE di.id = $1"
    )))
    .bind(interface_id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.device.interface.not_found")))?;

    Ok(crate::error::ok_json(
        data,
        "server.device.interface.fetched",
    ))
}

/// 更新网络接口。
///
/// 普通字段缺失表示不修改（`COALESCE` 保留旧值）；可空字段
/// （MAC/描述）为 `Option<Option<T>>`，
/// `Some(None)` 显式置空、外层 `None` 不修改。
pub async fn update_device_interface(
    State(state): State<Arc<AppState>>,
    Path(interface_id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<DeviceInterfaceUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    if let Some(physical_type) = req.physical_type.as_deref() {
        validate_physical_type(physical_type)?;
    }
    if let Some(interface_role) = req.interface_role.as_deref() {
        validate_interface_role(interface_role)?;
    }

    let mut builder = QueryBuilder::<Postgres>::new("UPDATE device_interfaces SET ");
    {
        // separated 会在非首个 push 前自动插入分隔符，因此列名片段用 push、
        // 绑定值紧随其后用 push_bind_unseparated，避免生成 "col = , $1"
        let mut sep = builder.separated(", ");
        sep.push("name = ").push_bind_unseparated(&req.name);
        sep.push("physical_type = ")
            .push_bind_unseparated(&req.physical_type);
        sep.push("interface_role = ")
            .push_bind_unseparated(&req.interface_role);
        // 可空字段：仅当请求中出现该字段时才加入 SET，bind 对 Option
        // 直接编码（Some→值，None→NULL），无需 CASE WHEN 区分
        if let Some(mac_address) = &req.mac_address {
            sep.push("mac_address = ")
                .push_bind_unseparated(mac_address);
        }
        sep.push("vlan_id = ").push_bind_unseparated(req.vlan_id);
        if let Some(description) = &req.description {
            sep.push("description = ")
                .push_bind_unseparated(description);
        }
        sep.push("updated_at = ").push_bind_unseparated(Utc::now());
    }
    builder.push(" WHERE id = ").push_bind(interface_id);

    let result = builder.build().execute(&state.pool()?.get_conn()).await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(msg("server.device.interface.not_found")));
    }

    let data =
        sqlx::query_as::<_, DeviceInterface>("SELECT * FROM device_interfaces WHERE id = $1")
            .bind(interface_id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    let details = serde_json::json!({
        "device_id": data.device_id,
        "name": data.name,
        "physical_type": data.physical_type,
        "interface_role": data.interface_role,
        "mac_address": data.mac_address
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "device_interface",
        Some(&interface_id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(
        data,
        "server.device.interface.updated",
    ))
}

/// 删除网络接口。
///
/// 接口可能被 IP 地址与物理链路引用，在同一事务内先清理关联数据
/// 再删除接口，避免外键约束与防删触发器（cable_links 侧）报错。
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
        return Err(AppError::NotFound(msg("server.device.interface.not_found")));
    }

    tx.commit().await?;

    let details = serde_json::json!({
        "interface_id": interface_id
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "device_interface",
        Some(&interface_id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json((), "server.device.interface.deleted"))
}
