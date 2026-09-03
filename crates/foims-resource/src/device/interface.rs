//! 统一端口模型（device_interfaces）资源管理。
//!
//! 设备端口与设备网口已合并：接口按 `physical_type`（物理形态：
//! rj45/sfp/.../virtual）与 `interface_role`（角色：management/
//! business/...）两个正交维度描述；网络设备的二层属性（port_type/
//! status/speed）由 SNMP 同步维护。`device_managed` 标记网口是否由
//! 设备编辑模态框托管（设备模态框新建为 true，端口模态框与 SNMP
//! 同步生成的为 false）。未显式指定网卡的端口按名称前缀自动生成
//! 板卡网卡（如 `xg1/0/0/1` → `设备名-xg1`）。
//!
//! 更新时字段缺失表示不修改，可空字段（MAC/描述等）以 `Some(None)`
//! 显式置空。列表过滤使用 sqlx `QueryBuilder` 动态拼接，全部用户
//! 输入经 `push_bind` 参数绑定，搜索关键字先经 `escape_like` 转义。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;
use validator::Validate;

use super::nic::{
    get_or_create_auto_nic, port_group_prefix, validate_interface_role, validate_physical_type,
};
use super::snmp::{DeviceForSnmp, get_device_ports_via_snmp, truncate_to_column_width};
use foims_auth::meta::{RequestMeta, log_op_best_effort};
use foims_common::AppJson;
use foims_common::DbProvider;
use foims_common::pagination::{Pagination, paged_response};
use foims_common::{AppError, msg};
use foims_models::{
    DeviceInterface, DeviceInterfaceCreate, DeviceInterfaceUpdate, DeviceInterfaceWithDevice,
    SnmpPort,
};

/// 接口联表查询列（含所属设备名），列表与单条查询共用。
const INTERFACE_WITH_DEVICE_COLUMNS: &str = "di.id, di.device_id, d.name as device_name,
                di.nic_id, di.name, di.physical_type, di.interface_role, di.mac_address, di.vlan_id,
                di.description, di.sort_order, di.port_type, di.status, di.speed, di.trunk_id,
                di.device_managed, di.created_at, di.updated_at";

/// 校验二层端口类型枚举（与 device_interfaces.port_type CHECK 一致）。
fn validate_port_type(port_type: &str) -> Result<(), AppError> {
    if !matches!(
        port_type,
        "access" | "trunk" | "hybrid" | "uplink" | "stack" | "console"
    ) {
        return Err(AppError::Validation(msg(
            "server.device.interface.port_type_invalid",
        )));
    }
    Ok(())
}

/// 校验端口状态枚举（SNMP 维护，应用层兜底校验）。
fn validate_port_status(status: &str) -> Result<(), AppError> {
    if !matches!(status, "up" | "down" | "admin-down") {
        return Err(AppError::Validation(msg(
            "server.device.interface.status_invalid",
        )));
    }
    Ok(())
}

/// 分页获取指定设备的接口列表。
pub async fn get_device_interfaces<P: DbProvider>(
    State(state): State<Arc<P>>,
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

    Ok(foims_common::ok_json(
        paged_response(data, total, &pagination),
        "server.device.interface.list_retrieved",
    ))
}

/// 分页获取全部设备接口（跨设备视图，支持关键字模糊匹配）。
pub async fn get_all_device_interfaces<P: DbProvider>(
    State(state): State<Arc<P>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let search = query.get("search").cloned().unwrap_or_default();
    let search_pattern = (!search.is_empty()).then(|| foims_common::net::escape_like(&search));

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

    Ok(foims_common::ok_json(
        paged_response(data, total, &pagination),
        "server.device.interface.list_all_retrieved",
    ))
}

/// 解析接口的归属网卡：显式指定时校验归属本设备，否则按名称前缀
/// 自动生成/复用板卡网卡。
async fn resolve_nic_id(
    tx: &mut sqlx::PgConnection,
    device_id: Uuid,
    interface_name: &str,
    explicit_nic_id: Option<Uuid>,
) -> Result<Uuid, AppError> {
    if let Some(nic_id) = explicit_nic_id {
        let owned: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM device_nics WHERE id = $1 AND device_id = $2")
                .bind(nic_id)
                .bind(device_id)
                .fetch_optional(&mut *tx)
                .await?;
        return owned.ok_or_else(|| AppError::NotFound(msg("server.device.nic.not_found")));
    }
    let group = port_group_prefix(interface_name);
    get_or_create_auto_nic(tx, device_id, group).await
}

/// 为设备创建网络接口（同设备名称唯一，存在性检查与写入在同一事务内）。
///
/// 端口模态框/SNMP 同步创建的接口默认 `device_managed = false`
/// （不在设备模态框展示）；设备模态框整体同步走 `nic::apply_network_config`。
pub async fn create_device_interface<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(device_id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<DeviceInterfaceCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 显式必填：缺省值会掩盖调用方漏传字段（审计 #12），
    // 前端表单在提交前已回填默认选项
    let Some(physical_type) = req.physical_type.as_deref() else {
        return Err(AppError::Validation(msg(
            "server.device.interface.physical_type_required",
        )));
    };
    validate_physical_type(physical_type)?;
    let Some(interface_role) = req.interface_role.as_deref() else {
        return Err(AppError::Validation(msg(
            "server.device.interface.interface_role_required",
        )));
    };
    validate_interface_role(interface_role)?;
    let port_type = req.port_type.as_deref().unwrap_or("access");
    validate_port_type(port_type)?;
    let status = req.status.as_deref().unwrap_or("up");
    validate_port_status(status)?;

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

    let nic_id = resolve_nic_id(&mut tx, device_id, &req.name, req.nic_id).await?;

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        r"INSERT INTO device_interfaces (
            id, device_id, nic_id, name, physical_type, interface_role, mac_address,
            vlan_id, description, port_type, status, speed, trunk_id, device_managed, sort_order,
            created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, 0, $15, $16)",
    )
    .bind(id)
    .bind(device_id)
    .bind(nic_id)
    .bind(&req.name)
    .bind(physical_type)
    .bind(interface_role)
    .bind(&req.mac_address)
    .bind(req.vlan_id)
    .bind(&req.description)
    .bind(port_type)
    .bind(status)
    .bind(&req.speed)
    .bind(req.trunk_id)
    .bind(req.device_managed.unwrap_or(false))
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
        "mac_address": data.mac_address,
        "device_managed": data.device_managed
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

    Ok(foims_common::ok_json(
        data,
        "server.device.interface.created",
    ))
}

/// 获取单个接口详情（含所属设备名）。
pub async fn get_device_interface<P: DbProvider>(
    State(state): State<Arc<P>>,
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

    Ok(foims_common::ok_json(
        data,
        "server.device.interface.fetched",
    ))
}

/// 更新网络接口。
///
/// 普通字段缺失表示不修改（`COALESCE` 保留旧值）；可空字段
/// （MAC/描述/速率）为 `Option<Option<T>>`，
/// `Some(None)` 显式置空、外层 `None` 不修改；`device_managed`
/// 缺失不修改（SNMP 覆盖同步时保留托管状态）。
pub async fn update_device_interface<P: DbProvider>(
    State(state): State<Arc<P>>,
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
    if let Some(port_type) = req.port_type.as_deref() {
        validate_port_type(port_type)?;
    }
    if let Some(status) = req.status.as_deref() {
        validate_port_status(status)?;
    }

    let mut builder = QueryBuilder::<Postgres>::new("UPDATE device_interfaces SET ");
    {
        // separated 会在非首个 push 前自动插入分隔符，因此列名片段用 push、
        // 绑定值紧随其后用 push_bind_unseparated，避免生成 "col = , $1"。
        // 非空列（name/physical_type/interface_role/port_type/status）仅当
        // 请求携带时才进 SET，缺失绑 NULL 会违反 NOT NULL 约束；
        // 可空列同理按需进 SET，bind 对 Option 直接编码（Some→值，None→NULL）
        let mut sep = builder.separated(", ");
        if let Some(name) = &req.name {
            sep.push("name = ").push_bind_unseparated(name);
        }
        if let Some(physical_type) = &req.physical_type {
            sep.push("physical_type = ")
                .push_bind_unseparated(physical_type);
        }
        if let Some(interface_role) = &req.interface_role {
            sep.push("interface_role = ")
                .push_bind_unseparated(interface_role);
        }
        if let Some(mac_address) = &req.mac_address {
            sep.push("mac_address = ")
                .push_bind_unseparated(mac_address);
        }
        if let Some(vlan_id) = req.vlan_id {
            sep.push("vlan_id = ").push_bind_unseparated(vlan_id);
        }
        if let Some(description) = &req.description {
            sep.push("description = ")
                .push_bind_unseparated(description);
        }
        if let Some(port_type) = &req.port_type {
            sep.push("port_type = ").push_bind_unseparated(port_type);
        }
        if let Some(status) = &req.status {
            sep.push("status = ").push_bind_unseparated(status);
        }
        if let Some(speed) = &req.speed {
            sep.push("speed = ").push_bind_unseparated(speed);
        }
        if let Some(trunk_id) = req.trunk_id {
            sep.push("trunk_id = ").push_bind_unseparated(trunk_id);
        }
        if let Some(device_managed) = req.device_managed {
            sep.push("device_managed = ")
                .push_bind_unseparated(device_managed);
        }
        sep.push("updated_at = ").push_bind_unseparated(Utc::now());
    }
    builder.push(" WHERE id = ").push_bind(interface_id);

    let result = builder
        .build()
        .execute(&state.pool()?.get_conn())
        .await
        .map_err(|e| {
            // 并发重名兜底：device_interfaces UNIQUE(device_id, name) 冲突映射为 409
            if let sqlx::Error::Database(ref db_err) = e
                && db_err.is_unique_violation()
            {
                return AppError::Conflict(msg("server.device.interface.name_exists"));
            }
            AppError::from(e)
        })?;
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
        "mac_address": data.mac_address,
        "device_managed": data.device_managed
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

    Ok(foims_common::ok_json(
        data,
        "server.device.interface.updated",
    ))
}

/// 删除网络接口。
///
/// 接口可能被 IP 地址与物理链路引用，在同一事务内先清理关联数据
/// 再删除接口，避免外键约束与防删触发器（cable_links 侧）报错；
/// 随后清理不再被引用的空网卡（自动板卡随之消失）。
pub async fn delete_device_interface<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(interface_id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let mut tx = state.pool()?.get_conn().begin().await?;

    let interface_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM device_interfaces WHERE id = $1)",
    )
    .bind(interface_id)
    .fetch_one(&mut *tx)
    .await?;
    if !interface_exists {
        return Err(AppError::NotFound(msg("server.device.interface.not_found")));
    }

    let device_id: Uuid =
        sqlx::query_scalar("SELECT device_id FROM device_interfaces WHERE id = $1")
            .bind(interface_id)
            .fetch_one(&mut *tx)
            .await?;

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
    sqlx::query("DELETE FROM device_interfaces WHERE id = $1")
        .bind(interface_id)
        .execute(&mut *tx)
        .await?;

    // 清理不再被引用的网卡（限定本设备：全局清理会顺带影响其他设备
    // 并发操作留下的待清理网卡，作用域应收敛到本次删除的设备）
    sqlx::query(
        "DELETE FROM device_nics WHERE device_id = $1 AND id NOT IN (
            SELECT nic_id FROM device_interfaces WHERE nic_id IS NOT NULL AND device_id = $1)",
    )
    .bind(device_id)
    .execute(&mut *tx)
    .await?;

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

    Ok(foims_common::ok_json((), "server.device.interface.deleted"))
}

/// device_interfaces.name 列宽（VARCHAR(50)，与建表契约一致）
const INTERFACE_NAME_MAX_CHARS: usize = 50;

/// 将 SNMP 拉取的端口映射为统一接口写入参数（未指定网卡前缀）。
fn snmp_port_to_create(port: &SnmpPort) -> DeviceInterfaceCreate {
    let description = port
        .description
        .as_deref()
        .filter(|d| !d.is_empty())
        .map(str::to_string);
    // name 列宽 VARCHAR(50)：ifName/ifDescr 可能超长（如含描述性文本），
    // 整批 INSERT 会因单行超宽整批失败，入库前按字符截断（与 mac.rs
    // 的 interface 列处理同口径）；网卡前缀分组使用截断后的名称
    let name = truncate_to_column_width(&port.name, INTERFACE_NAME_MAX_CHARS);
    DeviceInterfaceCreate {
        name,
        nic_id: None,
        physical_type: Some("other".to_string()),
        interface_role: Some("business".to_string()),
        mac_address: None,
        vlan_id: port.vlan_id,
        description,
        port_type: port.port_type.clone(),
        status: port.status.clone(),
        speed: port.speed.clone(),
        // SNMP 同步来源不涉及 hybrid 端口的 Native VLAN 配置
        trunk_id: None,
        device_managed: Some(false),
    }
}

/// 从 SNMP 同步设备端口（批量入库，未指定类型时回退默认值）。
///
/// 以 `(device_id, name)` 唯一约束做批量 `INSERT ... ON CONFLICT
/// DO NOTHING`：已存在的接口跳过，未知的入库，单次往返完成。
/// 网卡按端口名前缀自动生成板卡；同步来源接口一律不托管
/// （`device_managed = false`，不进设备模态框）。
pub async fn sync_ports_from_snmp<P: DbProvider>(
    State(state): State<Arc<P>>,
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
        let mut tx = state.pool()?.get_conn().begin().await?;

        // 按分组前缀预先创建板卡网卡，避免逐端口重复查询
        let mut nic_cache: HashMap<String, Uuid> = HashMap::new();
        let mut port_rows = Vec::with_capacity(ports.len());
        for port in &ports {
            let req = snmp_port_to_create(port);
            let group = port_group_prefix(&req.name).to_string();
            let nic_id = if let Some(id) = nic_cache.get(&group) {
                *id
            } else {
                let id = get_or_create_auto_nic(&mut tx, device_id, &group).await?;
                nic_cache.insert(group, id);
                id
            };
            port_rows.push((req, nic_id));
        }

        let mut builder = QueryBuilder::<Postgres>::new(
            r"INSERT INTO device_interfaces (
                id, device_id, nic_id, name, physical_type, interface_role,
                mac_address, vlan_id, description, port_type, status, speed,
                device_managed, sort_order, created_at, updated_at
            )",
        );
        builder.push_values(port_rows.iter(), |mut row, (req, nic_id)| {
            row.push_bind(Uuid::new_v4())
                .push_bind(device_id)
                .push_bind(*nic_id)
                .push_bind(&req.name)
                .push_bind(req.physical_type.as_deref().unwrap_or("other"))
                .push_bind(req.interface_role.as_deref().unwrap_or("business"))
                .push_bind(&req.mac_address)
                .push_bind(req.vlan_id)
                .push_bind(&req.description)
                .push_bind(req.port_type.as_deref().unwrap_or("access"))
                .push_bind(req.status.as_deref().unwrap_or("up"))
                .push_bind(&req.speed)
                .push_bind(false)
                .push_bind(0)
                .push_bind(now)
                .push_bind(now);
        });
        builder.push(" ON CONFLICT (device_id, name) DO NOTHING");

        saved_count = builder.build().execute(&mut *tx).await?.rows_affected() as usize;
        tx.commit().await?;
    }
    let skipped_count = ports.len() - saved_count;

    let saved_interfaces = sqlx::query_as::<_, DeviceInterface>(
        "SELECT * FROM device_interfaces WHERE device_id = $1 ORDER BY name",
    )
    .bind(device_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    // 按同步结果构造消息：区分无数据 / 部分保存 / 全部保存 / 全部已存在
    let message = if ports.is_empty() {
        msg("server.device.interface.snmp_no_ports")
    } else if saved_count > 0 && skipped_count > 0 {
        msg("server.device.interface.sync_partial")
            .with("saved", saved_count)
            .with("skipped", skipped_count)
    } else if saved_count > 0 {
        msg("server.device.interface.sync_saved").with("saved", saved_count)
    } else {
        msg("server.device.interface.sync_all_skipped").with("skipped", skipped_count)
    };

    Ok(foims_common::ok_json(saved_interfaces, message))
}
