//! 设备物理端口（device_ports）资源管理。
//!
//! 提供端口的 CRUD、跨设备分页检索与 SNMP 端口同步。列表过滤使用
//! sqlx `QueryBuilder` 动态拼接，全部用户输入经 `push_bind` 参数绑定；
//! 搜索关键字先经 `escape_like` 转义，避免 ILIKE 通配符误匹配。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;
use validator::Validate;

use super::snmp::{DeviceForSnmp, get_device_ports_via_snmp};
use crate::app_state::AppState;
use crate::error::{AppError, msg};
use crate::models::{DevicePort, DevicePortCreate, DevicePortUpdate, DevicePortWithDevice};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::{Pagination, paged_response};

/// 端口联表查询片段：设备信息 + 设备首个 IP 的横向视图（LATERAL），
/// 列表与单条查询共用。
const PORT_WITH_DEVICE_FROM: &str = "FROM device_ports sp
            JOIN devices d ON sp.device_id = d.id
            LEFT JOIN LATERAL (
                SELECT host(im.ip_address) AS ip, nc.name AS network_name, nr.name AS network_region
                FROM ips im
                JOIN device_interfaces dim ON im.device_interface_id = dim.id
                LEFT JOIN network_cidrs nc ON im.network_id = nc.id
                LEFT JOIN network_regions nr ON nc.network_region_id = nr.id
                WHERE dim.device_id = d.id
                LIMIT 1
            ) dip ON true";

/// 分页获取指定设备的端口列表。
pub async fn get_device_ports(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_ports WHERE device_id = $1")
        .bind(device_id)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let data = sqlx::query_as::<_, DevicePort>(
        "SELECT * FROM device_ports WHERE device_id = $1 ORDER BY port_number LIMIT $2 OFFSET $3",
    )
    .bind(device_id)
    .bind(pagination.page_size)
    .bind(pagination.offset)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(crate::error::ok_json(
        paged_response(data, total, &pagination),
        "server.device.port.list_retrieved",
    ))
}

/// 追加跨设备端口列表过滤条件（关键字模糊匹配 + 机房），供 COUNT 与数据查询共用。
fn push_port_filters(
    builder: &mut QueryBuilder<Postgres>,
    search_pattern: Option<&str>,
    room_id: Option<Uuid>,
) {
    let mut first = true;
    if let Some(pattern) = search_pattern {
        builder
            .push(" WHERE (d.name ILIKE ")
            .push_bind(pattern)
            .push(" OR sp.port_number::TEXT ILIKE ")
            .push_bind(pattern)
            .push(" OR sp.port_name ILIKE ")
            .push_bind(pattern)
            .push(" OR sp.description ILIKE ")
            .push_bind(pattern)
            .push(")");
        first = false;
    }
    if let Some(room_id) = room_id {
        builder
            .push(if first { " WHERE " } else { " AND " })
            .push("d.room_id = ")
            .push_bind(room_id);
    }
}

/// 分页获取全部设备端口（跨设备视图，支持关键字与机房过滤）。
pub async fn get_all_device_ports(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let search = query.get("search").cloned().unwrap_or_default();
    let room_id = query.get("room_id").cloned();

    let search_pattern = (!search.is_empty()).then(|| crate::utils::escape_like(&search));

    let parsed_room_id = room_id
        .as_ref()
        .map(|id| {
            Uuid::parse_str(id).map_err(|_| {
                AppError::Validation(msg("server.common.invalid_param").with("param", "room_id"))
            })
        })
        .transpose()?;

    let mut count_builder = QueryBuilder::<Postgres>::new(
        "SELECT COUNT(*) FROM device_ports sp JOIN devices d ON sp.device_id = d.id",
    );
    push_port_filters(
        &mut count_builder,
        search_pattern.as_deref(),
        parsed_room_id,
    );
    let total: i64 = count_builder
        .build_query_scalar()
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let mut data_builder = QueryBuilder::<Postgres>::new(format!(
        "SELECT sp.id, sp.device_id, d.name as device_name,
                COALESCE(dip.ip, '') as device_ip,
                dip.network_name as device_network_name,
                dip.network_region as device_network_region,
                sp.port_number, sp.port_name, sp.port_type, sp.vlan_id,
                sp.status, sp.speed, sp.description, sp.created_at, sp.updated_at
            {PORT_WITH_DEVICE_FROM}"
    ));
    push_port_filters(&mut data_builder, search_pattern.as_deref(), parsed_room_id);
    data_builder
        .push(" ORDER BY d.name, sp.port_number LIMIT ")
        .push_bind(pagination.page_size)
        .push(" OFFSET ")
        .push_bind(pagination.offset);
    let data = data_builder
        .build_query_as::<DevicePortWithDevice>()
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    Ok(crate::error::ok_json(
        paged_response(data, total, &pagination),
        "server.device.port.list_all_retrieved",
    ))
}

/// 校验端口类型枚举（与 device_ports.port_type CHECK 一致）。
/// 此前裸传非法值直写 DB 报 500 而非 422（db-schema-review 第六节）。
pub fn validate_port_type(port_type: &str) -> Result<(), AppError> {
    if !matches!(
        port_type,
        "access" | "trunk" | "uplink" | "stack" | "console"
    ) {
        return Err(AppError::Validation(msg("server.device.port.type_invalid")));
    }
    Ok(())
}

/// 为设备创建端口（同设备端口号唯一，存在性检查与写入在同一事务内）。
pub async fn create_device_port(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<DevicePortCreate>,
) -> Result<Response, AppError> {
    req.validate()?;
    if let Some(port_type) = req.port_type.as_deref() {
        validate_port_type(port_type)?;
    }

    let mut tx = state.pool()?.get_conn().begin().await?;

    let device_exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
            .bind(device_id)
            .fetch_one(&mut *tx)
            .await?;
    if !device_exists {
        return Err(AppError::NotFound(msg("server.device.not_found")));
    }

    let port_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM device_ports WHERE device_id = $1 AND port_number = $2)",
    )
    .bind(device_id)
    .bind(&req.port_number)
    .fetch_one(&mut *tx)
    .await?;
    if port_exists {
        return Err(AppError::Conflict(msg("server.device.port.number_exists")));
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
    .execute(&mut *tx)
    .await?;

    let data = sqlx::query_as::<_, DevicePort>("SELECT * FROM device_ports WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "device_id": device_id,
        "port_number": data.port_number,
        "port_name": data.port_name,
        "port_type": data.port_type,
        "vlan_id": data.vlan_id
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "device_port",
        Some(&id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(data, "server.device.port.created"))
}

/// 获取单个端口详情（含所属设备与设备首个 IP）。
pub async fn get_device_port(
    State(state): State<Arc<AppState>>,
    Path(port_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let sql = format!(
        "SELECT sp.id, sp.device_id, d.name as device_name,
                COALESCE(dip.ip, '') as device_ip,
                dip.network_name as device_network_name,
                dip.network_region as device_network_region,
                sp.port_number, sp.port_name, sp.port_type, sp.vlan_id,
                sp.status, sp.speed, sp.description, sp.created_at, sp.updated_at
            {PORT_WITH_DEVICE_FROM}
            WHERE sp.id = $1"
    );
    let data = sqlx::query_as::<_, DevicePortWithDevice>(sqlx::AssertSqlSafe(sql))
        .bind(port_id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .ok_or_else(|| AppError::NotFound(msg("server.device.port.not_found")))?;

    Ok(crate::error::ok_json(data, "server.device.port.fetched"))
}

/// 更新端口（字段缺失表示不修改，`Option` 绑定经 COALESCE 保留旧值）。
pub async fn update_device_port(
    State(state): State<Arc<AppState>>,
    Path(port_id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<DevicePortUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;
    if let Some(port_type) = req.port_type.as_deref() {
        validate_port_type(port_type)?;
    }

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
    .bind(Utc::now())
    .bind(port_id)
    .execute(&state.pool()?.get_conn())
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(msg("server.device.port.not_found")));
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
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "device_port",
        Some(&port_id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json(data, "server.device.port.updated"))
}

/// 删除端口（被物理链路引用时由外键约束拦截并转为友好提示）。
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
                return AppError::Validation(msg("server.device.port.in_use"));
            }
            AppError::from(e)
        })?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(msg("server.device.port.not_found")));
    }

    let details = serde_json::json!({
        "port_id": port_id
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "device_port",
        Some(&port_id),
        &details,
    )
    .await;

    Ok(crate::error::ok_json((), "server.device.port.deleted"))
}

/// 从 SNMP 同步设备端口。
///
/// 以 `(device_id, port_number)` 唯一约束做批量 `INSERT ... ON CONFLICT
/// DO NOTHING`：已存在的端口跳过，未知的端口入库，单次往返完成
/// （替代旧的逐条 EXISTS + INSERT，避免 N+1 查询）。
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
    .ok_or_else(|| AppError::NotFound(msg("server.device.not_found")))?;

    let ip_address: Option<String> = sqlx::query_scalar(
        r"SELECT host(i.ip_address) FROM ips i
           JOIN device_interfaces di ON i.device_interface_id = di.id
           WHERE di.device_id = $1
           ORDER BY i.created_at LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?;

    let ip_address = match ip_address {
        Some(ref ip) if !ip.is_empty() => ip,
        _ => {
            return Err(AppError::Validation(msg("server.device.no_ip_configured")));
        }
    };

    let snmp_params = switch_data.to_snmp_params_async(ip_address).await?;

    let ports = get_device_ports_via_snmp(&snmp_params).await.map_err(|e| {
        AppError::Snmp(msg("server.device.snmp.ports_fetch_failed").with("error", e))
    })?;

    let now = Utc::now();
    let mut saved_count = 0usize;
    if !ports.is_empty() {
        let mut builder = QueryBuilder::<Postgres>::new(
            r"INSERT INTO device_ports (
                id, device_id, port_number, port_name, port_type, vlan_id,
                status, speed, description, created_at, updated_at
            )",
        );
        builder.push_values(ports.iter(), |mut row, port| {
            row.push_bind(Uuid::new_v4())
                .push_bind(device_id)
                .push_bind(&port.port_number)
                .push_bind(&port.port_name)
                .push_bind(port.port_type.as_deref().unwrap_or("access"))
                .push_bind(port.vlan_id)
                .push_bind(port.status.as_deref().unwrap_or("up"))
                .push_bind(&port.speed)
                .push_bind(&port.description)
                .push_bind(now)
                .push_bind(now);
        });
        builder.push(" ON CONFLICT (device_id, port_number) DO NOTHING");

        saved_count = builder
            .build()
            .execute(&state.pool()?.get_conn())
            .await?
            .rows_affected() as usize;
    }
    let skipped_count = ports.len() - saved_count;

    let saved_ports = sqlx::query_as::<_, DevicePort>(
        "SELECT * FROM device_ports WHERE device_id = $1 ORDER BY port_number",
    )
    .bind(device_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    // 按同步结果构造消息：区分无数据 / 部分保存 / 全部保存 / 全部已存在
    let message = if ports.is_empty() {
        msg("server.device.port.snmp_no_ports")
    } else if saved_count > 0 && skipped_count > 0 {
        msg("server.device.port.sync_partial")
            .with("saved", saved_count)
            .with("skipped", skipped_count)
    } else if saved_count > 0 {
        msg("server.device.port.sync_saved").with("saved", saved_count)
    } else {
        msg("server.device.port.sync_all_skipped").with("skipped", skipped_count)
    };

    Ok(crate::error::ok_json(saved_ports, message))
}
