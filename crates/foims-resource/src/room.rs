//! 房间（rooms）资源：房间 CRUD、网段绑定与工位/机柜/信息点子资源的差量
//! 同步，供前端空间管理与设备位置绑定使用。

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use foims_auth::meta::{RequestMeta, log_op_best_effort};
use foims_common::AppJson;
use foims_common::DbProvider;
use foims_common::pagination::{Pagination, paged_response};
use foims_common::{AppError, msg};
use foims_models::{
    CabinetBrief, NetOutletBrief, NetworkInfo, Room, RoomChildrenSync, RoomCreate,
    RoomNetOutletsSync, RoomUpdate, RoomWithNetworks, WorkstationBrief,
};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

/// 房间读模型：`Room` 实体 + 联查的组织名。
///
/// 房间组织为非空外键（RESTRICT），故联表用 INNER JOIN，查询结果恒有组织名。
#[derive(sqlx::FromRow)]
struct RoomRecord {
    id: Uuid,
    name: String,
    room_type: String,
    org_id: Uuid,
    total_power_watts: Option<i32>,
    description: Option<String>,
    org_name: String,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

/// 房间读模型查询（列表与单条共用）：实体列 + 组织名。
const ROOM_RECORD_SELECT: &str = "SELECT r.id, r.name, r.room_type, r.org_id, \
     r.total_power_watts, r.description, \
     r.created_at::TIMESTAMPTZ, r.updated_at::TIMESTAMPTZ, \
     o.name AS org_name \
     FROM rooms r \
     JOIN organizations o ON r.org_id = o.id";

/// 按 id 取单个房间读模型（id 经 QueryBuilder 绑定，不拼接到 SQL 文本）。
fn room_by_id_query(room_id: Uuid) -> sqlx::QueryBuilder<sqlx::Postgres> {
    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(ROOM_RECORD_SELECT);
    builder.push(" WHERE r.id = ");
    builder.push_bind(room_id);
    builder
}

/// GET /api/resources/rooms：分页获取房间列表（关键字、组织子树、逗号分隔房型过滤与白名单排序，
/// 响应含网段/工位数/已分配功率摘要）。
pub async fn get_rooms<P: DbProvider>(
    State(state): State<Arc<P>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());

    let search_pattern = foims_common::net::escape_like(&search);

    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => " ORDER BY r.name DESC",
        ("created_at", "desc") => " ORDER BY r.created_at DESC",
        ("created_at", _) => " ORDER BY r.created_at ASC",
        ("room_type", "desc") => " ORDER BY r.room_type DESC, r.name ASC",
        ("room_type", _) => " ORDER BY r.room_type ASC, r.name ASC",
        ("org_name", "desc") => " ORDER BY org_name DESC, r.name ASC",
        ("org_name", _) => " ORDER BY org_name ASC, r.name ASC",
        _ => " ORDER BY r.name ASC",
    };

    let org_filter: Option<Uuid> = match query.get("org_id") {
        Some(v) if !v.is_empty() => Some(Uuid::parse_str(v).map_err(|_| {
            AppError::Validation(msg("server.common.invalid_param").with("param", "org_id"))
        })?),
        _ => None,
    };
    let room_types: Vec<String> = query
        .get("room_type")
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_uppercase())
                .collect()
        })
        .unwrap_or_default();

    // 动态拼接过滤条件（count 与列表两侧保持一致）
    let mut count_builder: sqlx::QueryBuilder<sqlx::Postgres> =
        sqlx::QueryBuilder::new("SELECT COUNT(*) FROM rooms r");
    let mut list_builder: sqlx::QueryBuilder<sqlx::Postgres> =
        sqlx::QueryBuilder::new(ROOM_RECORD_SELECT);

    let mut has_where = false;
    if !search.is_empty() {
        // 搜索条件的 OR 组必须用括号包裹（对齐 workstation.rs/ip.rs 的写法），
        // 否则 AND 优先级更高，与 org_id/room_type 组合时命名称/房型的
        // 房间会绕过组织过滤
        // 同一模式绑定 6 次（两个构建器 × 3 列），循环外克隆一次复用
        for builder in [&mut count_builder, &mut list_builder] {
            let pattern = search_pattern.clone();
            builder.push(" WHERE (r.name ILIKE ");
            builder.push_bind(pattern.clone());
            builder.push(" OR r.room_type ILIKE ");
            builder.push_bind(pattern.clone());
            builder.push(" OR r.description ILIKE ");
            builder.push_bind(pattern);
            builder.push(")");
        }
        has_where = true;
    }
    // 各过滤分支独立判断首条件（WHERE）与后续条件（AND），
    // 避免 org_id 与 room_type 组合时拼出双 WHERE；
    // org_id 按组织子树过滤（含全部下级组织的房间）
    if let Some(org_id) = org_filter {
        let conjunction = if has_where { " AND" } else { " WHERE" };
        for builder in [&mut count_builder, &mut list_builder] {
            builder.push(conjunction);
            builder.push(
                " r.org_id IN (WITH RECURSIVE org_tree AS (\
                 SELECT id FROM organizations WHERE id = ",
            );
            builder.push_bind(org_id);
            builder.push(
                " UNION ALL SELECT o.id FROM organizations o \
                 JOIN org_tree t ON o.parent_id = t.id) SELECT id FROM org_tree)",
            );
        }
        has_where = true;
    }
    if !room_types.is_empty() {
        let conjunction = if has_where { " AND" } else { " WHERE" };
        for builder in [&mut count_builder, &mut list_builder] {
            builder.push(conjunction);
            builder.push(" r.room_type = ANY(");
            builder.push_bind(room_types.clone());
            builder.push(")");
        }
    }

    let total: i64 = count_builder
        .build_query_scalar()
        .persistent(false)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    list_builder.push(order_clause);
    list_builder.push(" LIMIT ");
    list_builder.push_bind(page_size);
    list_builder.push(" OFFSET ");
    list_builder.push_bind(offset);

    let rooms = list_builder
        .build_query_as::<RoomRecord>()
        .persistent(false)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    // 批量查询（避免逐房间 N+1）：一次取全部房间的子网、工位数、已分配功率
    let conn = state.pool()?.get_conn();
    let room_ids: Vec<Uuid> = rooms.iter().map(|r| r.id).collect();

    let mut networks_map: HashMap<Uuid, Vec<NetworkInfo>> = HashMap::new();
    let network_rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, Uuid, Option<String>, Option<String>)>(
        r"SELECT rn.room_id, n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text, n.ipv6_cidr::text
           FROM room_networks rn
           JOIN network_cidrs n ON rn.subnet_id = n.id
           JOIN network_regions nr ON n.network_region_id = nr.id
           WHERE rn.room_id = ANY($1)",
    )
    .bind(&room_ids)
    .fetch_all(&conn)
    .await?;
    for (room_id, id, name, network_region, network_region_id, ipv4_cidr, ipv6_cidr) in network_rows
    {
        networks_map.entry(room_id).or_default().push(NetworkInfo {
            id,
            name,
            network_region,
            network_region_id,
            ipv4_cidr,
            ipv6_cidr,
        });
    }

    let ws_count_map: HashMap<Uuid, i64> = sqlx::query_as::<_, (Uuid, i64)>(
        "SELECT room_id, COUNT(*) FROM workstations WHERE room_id = ANY($1) GROUP BY room_id",
    )
    .bind(&room_ids)
    .fetch_all(&conn)
    .await?
    .into_iter()
    .collect();

    // 已分配功率批量查询（房间归属设备的功耗求和），与工位数同口径
    let allocated_power_map: HashMap<Uuid, i64> = sqlx::query_as::<_, (Uuid, i64)>(
        "SELECT room_id, COALESCE(SUM(power_watts), 0) FROM devices \
         WHERE room_id = ANY($1) GROUP BY room_id",
    )
    .bind(&room_ids)
    .fetch_all(&conn)
    .await?
    .into_iter()
    .collect();

    let mut rooms_with_networks = Vec::new();
    for room in rooms {
        let room_networks = networks_map.remove(&room.id).unwrap_or_default();
        let workstation_count = ws_count_map.get(&room.id).copied().unwrap_or(0);

        let room_with_networks = RoomWithNetworks {
            id: room.id,
            name: room.name,
            room_type: room.room_type,
            org_id: room.org_id,
            org_name: room.org_name,
            total_power_watts: room.total_power_watts,
            allocated_power_watts: allocated_power_map.get(&room.id).copied().unwrap_or(0),
            description: room.description,
            networks: room_networks,
            workstation_count,
            workstations: None,
            cabinets: None,
            net_outlets: Vec::new(),
            created_at: room.created_at,
            updated_at: room.updated_at,
        };

        rooms_with_networks.push(room_with_networks);
    }

    Ok(foims_common::ok_json(
        paged_response(rooms_with_networks, total, &pagination),
        "server.room.list_retrieved",
    ))
}

/// POST /api/resources/rooms：创建房间（事务内校验全局重名与组织/网段引用：重名 409，引用非法 400，
/// subnet_ids 随房间一并绑定）。
pub async fn create_room<P: DbProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<RoomCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let id = Uuid::new_v4();
    let now = Utc::now();

    // 预检与写入同事务（与 update_room 同口径），避免预检-写入窗口
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_room = sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE name = $1")
        .bind(&req.name)
        .fetch_optional(&mut *tx)
        .await?;

    if existing_room.is_some() {
        return Err(AppError::Conflict(msg("server.room.name_exists")));
    }

    // 引用存在性校验：organizations / network_cidrs 非法引用返回校验错误，
    // 而非依赖 FK 约束的 500 兜底
    let org_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organizations WHERE id = $1)")
            .bind(req.org_id)
            .fetch_one(&mut *tx)
            .await?;
    if !org_exists {
        return Err(AppError::Validation(msg("server.organization.not_found")));
    }
    for subnet_id in &req.subnet_ids {
        let network_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM network_cidrs WHERE id = $1)")
                .bind(subnet_id)
                .fetch_one(&mut *tx)
                .await?;
        if !network_exists {
            return Err(AppError::Validation(msg("server.network.not_found")));
        }
    }

    // 入库值与返回值统一为大写房型（库内 CHECK/查询口径均为大写）
    let room_type_upper = req.room_type.to_uppercase();

    sqlx::query(
        "INSERT INTO rooms (id, name, room_type, org_id, total_power_watts, description, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&room_type_upper)
    .bind(req.org_id)
    .bind(req.total_power_watts)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // 并发写入竞态兜底：uq_rooms_name 冲突映射为 409
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict(msg("server.room.name_exists"));
        }
        AppError::from(e)
    })?;

    for subnet_id in &req.subnet_ids {
        sqlx::query(
            "INSERT INTO room_networks (id, room_id, subnet_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(id)
        .bind(subnet_id)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let room = Room {
        id,
        name: req.name.clone(),
        room_type: room_type_upper,
        org_id: req.org_id,
        total_power_watts: req.total_power_watts,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": room.name,
        "room_type": room.room_type,
        "org_id": room.org_id.to_string(),
        "total_power_watts": room.total_power_watts,
        "description": room.description,
        "network_count": req.subnet_ids.len()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "room",
        Some(&id),
        &details,
    )
    .await;

    Ok(foims_common::ok_json(room, "server.room.created"))
}

/// GET /api/resources/rooms/{id}/brief：房间编辑回显专用轻量端点，房间不存在 404。
///
/// 完整详情接口的组装视图查询较多，而编辑弹窗需要
/// name/room_type/org_id/description/networks 以及工位/机柜/信息点子项，
/// 此处在一次并行查询内取齐，避免编辑保存时空列表同步误删既有子项。
pub async fn get_room_brief<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();
    // QueryAs 借用 QueryBuilder 的内部 SQL 缓冲，builder 必须活到并行任务结束
    let mut room_builder = room_by_id_query(id);
    let room_query = room_builder
        .build_query_as::<RoomRecord>()
        .persistent(false);
    let (room, room_networks, ws_rows, cab_rows, no_rows) = tokio::join!(
        room_query.fetch_optional(&conn),
        sqlx::query_as::<_, NetworkInfo>(
            r"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr
               FROM room_networks rn
               JOIN network_cidrs n ON rn.subnet_id = n.id
               JOIN network_regions nr ON n.network_region_id = nr.id
               WHERE rn.room_id = $1",
        )
        .bind(id)
        .fetch_all(&conn),
        // 工位/机柜/信息点按房型由前端过滤展示，这里统一取全量保证"其他"房型两类子项齐全
        sqlx::query(
            "SELECT w.id, w.name, e.name AS manager_name, w.manager_employee_id FROM workstations w \
             LEFT JOIN employees e ON w.manager_employee_id = e.id \
             WHERE w.room_id = $1 ORDER BY w.name",
        )
        .bind(id)
        .fetch_all(&conn),
        sqlx::query("SELECT id, name, capacity FROM cabinets WHERE room_id = $1 ORDER BY name")
            .bind(id)
            .fetch_all(&conn),
        sqlx::query("SELECT id, name FROM net_outlets WHERE room_id = $1 ORDER BY name")
            .bind(id)
            .fetch_all(&conn)
    );

    let room = room?.ok_or_else(|| AppError::NotFound(msg("server.room.not_found")))?;
    let room_networks = room_networks?;

    let workstations: Vec<WorkstationBrief> = ws_rows?
        .iter()
        .map(|r| WorkstationBrief {
            id: r.get("id"),
            name: r.get("name"),
            manager_name: r.get("manager_name"),
            manager_employee_id: r.get("manager_employee_id"),
        })
        .collect();
    let cabinets: Vec<CabinetBrief> = cab_rows?
        .iter()
        .map(|r| CabinetBrief {
            id: r.get("id"),
            name: r.get("name"),
            capacity: r.get("capacity"),
        })
        .collect();
    let net_outlets: Vec<NetOutletBrief> = no_rows?
        .iter()
        .map(|r| NetOutletBrief {
            id: r.get("id"),
            name: r.get("name"),
        })
        .collect();

    Ok(foims_common::ok_json(
        serde_json::json!({
            "id": room.id,
            "name": room.name,
            "room_type": room.room_type,
            "org_id": room.org_id,
            "org_name": room.org_name,
            "total_power_watts": room.total_power_watts,
            "description": room.description,
            "networks": room_networks,
            "workstations": workstations,
            "cabinets": cabinets,
            "net_outlets": net_outlets,
        }),
        "server.room.fetched",
    ))
}

/// GET /api/resources/rooms/{id}：获取房间详情（组装视图：网段、工位/机柜/信息点子项与功率摘要并行取回；
/// 房间不存在 404）。
pub async fn get_room<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let room = room_by_id_query(id)
        .build_query_as::<RoomRecord>()
        .persistent(false)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .ok_or_else(|| AppError::NotFound(msg("server.room.not_found")))?;

    // 以下查询相互独立，并行执行避免串行等待
    let conn = state.pool()?.get_conn();
    // 办公类（含"其他"）携带工位、机房类（含"其他"）携带机柜，
    // 与 helpers::room_allows_workstation / room_allows_cabinet 同一判定
    let is_office = crate::helpers::room_allows_workstation(&room.room_type);
    let is_dc = crate::helpers::room_allows_cabinet(&room.room_type);

    let (room_networks, workstation_count, ws_rows, cab_rows, no_rows, allocated_power) =
        tokio::join!(
            sqlx::query_as::<_, NetworkInfo>(
                r"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr
                   FROM room_networks rn
                   JOIN network_cidrs n ON rn.subnet_id = n.id
                   JOIN network_regions nr ON n.network_region_id = nr.id
                   WHERE rn.room_id = $1",
            )
            .bind(room.id)
            .fetch_all(&conn),
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workstations WHERE room_id = $1")
                .bind(room.id)
                .fetch_one(&conn),
            async {
                if is_office {
                    sqlx::query(
                        "SELECT w.id, w.name, e.name AS manager_name, w.manager_employee_id FROM workstations w LEFT JOIN employees e ON w.manager_employee_id = e.id WHERE w.room_id = $1 ORDER BY w.name",
                    )
                    .bind(room.id)
                    .fetch_all(&conn)
                    .await
                    .map_err(AppError::from)
                } else {
                    Ok(Vec::new())
                }
            },
            async {
                if is_dc {
                    sqlx::query(
                        "SELECT id, name, capacity FROM cabinets WHERE room_id = $1 ORDER BY name",
                    )
                    .bind(room.id)
                    .fetch_all(&conn)
                    .await
                    .map_err(AppError::from)
                } else {
                    Ok(Vec::new())
                }
            },
            sqlx::query("SELECT id, name FROM net_outlets WHERE room_id = $1 ORDER BY name")
                .bind(room.id)
                .fetch_all(&conn),
            // 已分配功率：房间归属设备的功耗求和（SUM 忽略 NULL，无设备返回 0）
            sqlx::query_scalar::<_, i64>(
                "SELECT COALESCE(SUM(power_watts), 0) FROM devices WHERE room_id = $1",
            )
            .bind(room.id)
            .fetch_one(&conn)
        );

    let workstation_count = workstation_count?;
    let allocated_power = allocated_power?;

    let workstations = if is_office {
        Some(
            ws_rows?
                .iter()
                .map(|r| WorkstationBrief {
                    id: r.get("id"),
                    name: r.get("name"),
                    manager_name: r.get("manager_name"),
                    manager_employee_id: r.get("manager_employee_id"),
                })
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    let cabinets = if is_dc {
        Some(
            cab_rows?
                .iter()
                .map(|r| CabinetBrief {
                    id: r.get("id"),
                    name: r.get("name"),
                    capacity: r.get("capacity"),
                })
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    let net_outlets: Vec<NetOutletBrief> = no_rows?
        .iter()
        .map(|r| NetOutletBrief {
            id: r.get("id"),
            name: r.get("name"),
        })
        .collect();

    let room_with_networks = RoomWithNetworks {
        id: room.id,
        name: room.name,
        room_type: room.room_type,
        org_id: room.org_id,
        org_name: room.org_name,
        total_power_watts: room.total_power_watts,
        allocated_power_watts: allocated_power,
        description: room.description,
        networks: room_networks?,
        workstation_count,
        workstations,
        cabinets,
        net_outlets,
        created_at: room.created_at,
        updated_at: room.updated_at,
    };

    Ok(foims_common::ok_json(
        room_with_networks,
        "server.room.fetched",
    ))
}

/// PUT /api/resources/rooms/{id}：更新房间（缺省字段保留旧值、null 清空总功率
/// 上限；行锁内复核重名与组织/房型变更的存量子项约束，subnet_ids 整体替换：
/// 子项冲突或新上限低于房内功耗 400、重名 409、房间不存在 404）。
pub async fn update_room<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<RoomUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 预检（存在性/重名/引用）与写入放同一事务，避免 TOCTOU；
    // 存在性检查即 FOR UPDATE 锁行，为后续功率上限校验关闭并发写入窗口
    let mut tx = state.pool()?.get_conn().begin().await?;

    // 随存在性检查取回当前组织与房型：换组织/换房型需据此校验房内既有子项
    let current: Option<(Uuid, String)> =
        sqlx::query_as("SELECT org_id, room_type FROM rooms WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((current_org_id, current_room_type)) = current else {
        return Err(AppError::NotFound(msg("server.room.not_found")));
    };

    // 重名预检：房间名全局唯一（uq_rooms_name）
    if let Some(name) = &req.name {
        let duplicate: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM rooms WHERE name = $1 AND id != $2")
                .bind(name)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        if duplicate.is_some() {
            return Err(AppError::Conflict(msg("server.room.name_exists")));
        }
    }

    if let Some(Some(limit)) = req.total_power_watts {
        crate::helpers::ensure_power_limit_covers_allocated(
            &mut tx,
            crate::helpers::PowerScope::Room(id),
            limit,
            "server.room.power_exceeded",
        )
        .await?;
    }

    // 换组织：目标组织必须存在，且房内工位管理人必须已属于新组织
    //（触发器只守写入侧，换组织会让既有工位瞬间变成跨组织引用）
    if let Some(org_id) = req.org_id
        && org_id != current_org_id
    {
        let org_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organizations WHERE id = $1)")
                .bind(org_id)
                .fetch_one(&mut *tx)
                .await?;
        if !org_exists {
            return Err(AppError::Validation(msg("server.organization.not_found")));
        }
        let cross_org_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM workstations w \
             JOIN employees e ON e.id = w.manager_employee_id \
             WHERE w.room_id = $1 AND e.org_id != $2",
        )
        .bind(id)
        .bind(org_id)
        .fetch_one(&mut *tx)
        .await?;
        if cross_org_count > 0 {
            return Err(AppError::Validation(
                msg("server.room.cross_org_workstations").with("count", cross_org_count),
            ));
        }
    }

    // 换房型：新类型必须容纳房内存量子项（办公类只能有工位、机房类只能有机柜），
    // 与 helpers::room_allows_workstation / room_allows_cabinet 同一判定
    if let Some(new_type) = req.room_type.as_ref().map(|t| t.to_uppercase())
        && new_type != current_room_type
    {
        let (ws_count, cab_count): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM workstations WHERE room_id = $1), \
             (SELECT COUNT(*) FROM cabinets WHERE room_id = $1)",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if ws_count > 0 && !crate::helpers::room_allows_workstation(&new_type) {
            return Err(AppError::Validation(
                msg("server.room.type_change_has_workstations").with("count", ws_count),
            ));
        }
        if cab_count > 0 && !crate::helpers::room_allows_cabinet(&new_type) {
            return Err(AppError::Validation(
                msg("server.room.type_change_has_cabinets").with("count", cab_count),
            ));
        }
    }

    if let Some(subnet_ids) = &req.subnet_ids {
        for subnet_id in subnet_ids {
            let network_exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM network_cidrs WHERE id = $1)")
                    .bind(subnet_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if !network_exists {
                return Err(AppError::Validation(msg("server.network.not_found")));
            }
        }
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE rooms SET
         name = COALESCE($1, name),
         room_type = COALESCE($2, room_type),
         org_id = COALESCE($3, org_id),
         total_power_watts = CASE WHEN $4::boolean THEN $5 ELSE total_power_watts END,
         description = COALESCE($6, description),
         updated_at = $7
         WHERE id = $8",
    )
    .bind(&req.name)
    .bind(req.room_type.as_ref().map(|t| t.to_uppercase()))
    .bind(req.org_id)
    .bind(req.total_power_watts.is_some())
    .bind(req.total_power_watts.flatten())
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // 并发写入竞态兜底：uq_rooms_name 冲突映射为 409
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict(msg("server.room.name_exists"));
        }
        AppError::from(e)
    })?;

    if let Some(subnet_ids) = &req.subnet_ids {
        // 解绑子网前校验：本房间存量设备的 IP 若仍落在将被移除的子网上，
        // 拒绝变更（不变量：设备 IP 必须归属其所在房间绑定的子网）
        let stale_ip_count: i64 = sqlx::query_scalar(
            r"SELECT COUNT(*)
               FROM interface_ips i
               JOIN device_interfaces di ON di.id = i.device_interface_id
               JOIN devices d ON d.id = di.device_id
               WHERE d.room_id = $1
                 AND i.subnet_id != ANY($2)",
        )
        .bind(id)
        .bind(subnet_ids)
        .fetch_one(&mut *tx)
        .await?;
        if stale_ip_count > 0 {
            return Err(AppError::Validation(
                msg("server.room.subnet_in_use_by_device_ip").with("count", stale_ip_count),
            ));
        }

        sqlx::query("DELETE FROM room_networks WHERE room_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        for subnet_id in subnet_ids {
            sqlx::query(
                "INSERT INTO room_networks (id, room_id, subnet_id, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(subnet_id)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    let room = sqlx::query_as::<_, Room>(
        "SELECT id, name, room_type, org_id, total_power_watts, \
         description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "name": room.name,
        "room_type": room.room_type,
        "org_id": room.org_id.to_string(),
        "total_power_watts": room.total_power_watts,
        "description": room.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "room",
        Some(&id),
        &details,
    )
    .await;

    Ok(foims_common::ok_json(room, "server.room.updated"))
}

/// DELETE /api/resources/rooms/{id}：删除房间（存在机柜、工位、信息点或直接挂载设备时 400，
/// 计数检查与删除同事务；不存在 404）。
pub async fn delete_room<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    // 计数检查与 DELETE 放同一事务，避免检查后被并发写入绕过触发级联
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_room = sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

    if existing_room.is_none() {
        return Err(AppError::NotFound(msg("server.room.not_found")));
    }

    let cabinet_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cabinets WHERE room_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    if cabinet_count > 0 {
        return Err(AppError::Validation(msg("server.room.has_cabinets")));
    }

    let workstation_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workstations WHERE room_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    if workstation_count > 0 {
        return Err(AppError::Validation(msg("server.room.has_workstations")));
    }

    let net_outlet_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM net_outlets WHERE room_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    if net_outlet_count > 0 {
        return Err(AppError::Validation(msg("server.room.has_net_outlets")));
    }

    // 设备可直接挂在房间下（不经工位/机位），同样拦截：
    // devices.room_id 为 ON DELETE RESTRICT，漏检会以 FK 500 收场
    let device_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM devices WHERE room_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    if device_count > 0 {
        return Err(AppError::Validation(msg("server.room.has_devices")));
    }

    sqlx::query("DELETE FROM rooms WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "room_id": id.to_string()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "room",
        Some(&id),
        &details,
    )
    .await;

    Ok(foims_common::ok_json((), "server.room.deleted"))
}

/// GET /api/resources/rooms/{id}/networks：获取房间绑定的网段列表（不分页；房间不存在返回 404）。
pub async fn get_room_networks<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let existing_room = sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

    if existing_room.is_none() {
        return Err(AppError::NotFound(msg("server.room.not_found")));
    }

    let room_networks = sqlx::query_as::<_, NetworkInfo>(
        r"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr
           FROM room_networks rn
           JOIN network_cidrs n ON rn.subnet_id = n.id
           JOIN network_regions nr ON n.network_region_id = nr.id
           WHERE rn.room_id = $1",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    Ok(foims_common::ok_json(
        room_networks,
        "server.room.networks_retrieved",
    ))
}

/// PUT /api/resources/rooms/{id}/children：差量同步房间子资源（按房型性质取工位或机柜集合，
/// 缺对应集合时 400；请求缺失项删除、被引用时 400，逐项重名与责任人预检后新增或更新，整批同事务）。
pub async fn sync_room_children<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<RoomChildrenSync>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 房间存在性与空间属性读取放进同步事务内，避免事务外的
    // 快照读与后续写入之间出现 TOCTOU
    let mut tx = state.pool()?.get_conn().begin().await?;

    let (room_type, room_org_id): (String, Uuid) =
        sqlx::query_as("SELECT room_type, org_id FROM rooms WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::NotFound(msg("server.room.not_found")))?;

    let now = Utc::now();

    // 按房型性质同步子项：办公类（办公室/大厅/前台）同步工位；
    // 机房类（机房/弱电井）同步机柜；"其他"无固定性质，两类均同步。
    // 判定统一走 helpers，与独立 CRUD 的房型允许性校验同源
    let sync_workstations = crate::helpers::room_allows_workstation(&room_type);
    let sync_cabinets = crate::helpers::room_allows_cabinet(&room_type);

    if sync_workstations {
        let items = req
            .workstations
            .as_ref()
            .ok_or_else(|| AppError::Validation(msg("server.room.office_requires_workstations")))?;
        sync_workstation_children(&mut tx, id, room_org_id, items, now).await?;
    }

    if sync_cabinets {
        let items = req
            .cabinets
            .as_ref()
            .ok_or_else(|| AppError::Validation(msg("server.room.datacenter_requires_cabinets")))?;
        sync_cabinet_children(&mut tx, id, items, now).await?;
    }

    tx.commit().await?;

    let details = serde_json::json!({
        "room_id": id.to_string(),
        "room_type": room_type,
        "workstation_count": req.workstations.as_ref().map(|v| v.len()).unwrap_or(0),
        "cabinet_count": req.cabinets.as_ref().map(|v| v.len()).unwrap_or(0)
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "sync_children",
        "room",
        Some(&id),
        &details,
    )
    .await;

    Ok(foims_common::ok_json((), "server.room.children_synced"))
}

/// PUT /api/resources/rooms/{id}/net-outlets：差量同步房间信息点（请求缺失项删除，被线路引用时 400；
/// 逐项名称唯一预检后新增或更新，整批同事务）。
pub async fn sync_room_net_outlets<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<RoomNetOutletsSync>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 房间存在性检查放进同步事务内，避免事务外的快照读与后续写入
    // 之间出现 TOCTOU（与 sync_room_children 同口径）
    let mut tx = state.pool()?.get_conn().begin().await?;

    let room_exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM rooms WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if room_exists.is_none() {
        return Err(AppError::NotFound(msg("server.room.not_found")));
    }

    let items = &req.net_outlets;
    let now = Utc::now();

    let existing_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM net_outlets WHERE room_id = $1")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;

    let request_ids: Vec<Uuid> = items.iter().filter_map(|i| i.id).collect();

    // 删除请求中不存在的信息点（cable_links 的删除保护触发器会阻止被引用的删除）
    for existing_id in &existing_ids {
        if !request_ids.contains(existing_id) {
            sqlx::query("DELETE FROM net_outlets WHERE id = $1")
                .bind(existing_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    if let sqlx::Error::Database(db_err) = &e {
                        let msg_text = db_err.message();
                        if msg_text.contains("ERR_CABLE_LINK_REFERENCE") {
                            return AppError::Validation(msg("server.net_outlet.in_use"));
                        }
                    }
                    AppError::from(e)
                })?;
        }
    }

    for item in items {
        validate_net_outlet_name(&mut tx, &item.name, item.id).await?;
        if let Some(item_id) = item.id {
            // 归属校验：仅允许更新当前房间下的信息点，
            // 携带其他房间的 id 时按未找到处理，避免跨父资源静默搬移
            let updated = sqlx::query(
                "UPDATE net_outlets SET name = $1, room_id = $2, updated_at = $3 WHERE id = $4 AND room_id = $5",
            )
            .bind(&item.name)
            .bind(id)
            .bind(now)
            .bind(item_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() == 0 {
                return Err(AppError::NotFound(msg("server.net_outlet.not_found")));
            }
        } else {
            let new_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO net_outlets (id, name, room_id, created_at, updated_at) VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(new_id)
            .bind(&item.name)
            .bind(id)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    let details = serde_json::json!({
        "room_id": id.to_string(),
        "net_outlet_count": items.len()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "sync_net_outlets",
        "room",
        Some(&id),
        &details,
    )
    .await;

    Ok(foims_common::ok_json((), "server.room.net_outlets_synced"))
}

/// 同步房间工位：删除请求中缺失的（被设备引用的拒绝），再新增/更新。
/// 管理人必须属于房间所属组织（与独立工位 CRUD 与触发器同口径）。
async fn sync_workstation_children(
    tx: &mut sqlx::PgConnection,
    room_id: Uuid,
    room_org_id: Uuid,
    items: &[foims_models::WorkstationSyncItem],
    now: chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    let existing_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM workstations WHERE room_id = $1")
            .bind(room_id)
            .fetch_all(&mut *tx)
            .await?;

    let request_ids: Vec<Uuid> = items.iter().filter_map(|i| i.id).collect();

    for existing_id in &existing_ids {
        if !request_ids.contains(existing_id) {
            let device_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE workstation_id = $1")
                    .bind(existing_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if device_count > 0 {
                return Err(AppError::Validation(msg(
                    "server.workstation.in_use_by_device",
                )));
            }
            sqlx::query("DELETE FROM workstation_layouts WHERE workstation_id = $1")
                .bind(existing_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM workstations WHERE id = $1")
                .bind(existing_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    for item in items {
        validate_workstation_name(tx, &item.name, room_id, item.id).await?;
        // 管理人（可空：未指定的不校验）必须属于房间组织：非法引用/跨组织
        // 引用以 400 返回，而不是等触发器抛出自定义异常映射成 500
        crate::helpers::ensure_employee_in_org(
            &mut *tx,
            item.manager_employee_id,
            room_org_id,
            "server.workstation.manager_employee_not_found",
            "server.workstation.manager_org_mismatch",
        )
        .await?;
        if let Some(item_id) = item.id {
            // 归属校验：仅允许更新当前房间下的工位，
            // 携带其他房间的 id 时按未找到处理，避免跨父资源静默搬移
            let updated = sqlx::query(
                "UPDATE workstations SET name = $1,
                    manager_employee_id = $2,
                    room_id = $3, updated_at = $4
                 WHERE id = $5 AND room_id = $6",
            )
            .bind(&item.name)
            .bind(item.manager_employee_id)
            .bind(room_id)
            .bind(now)
            .bind(item_id)
            .bind(room_id)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() == 0 {
                return Err(AppError::NotFound(msg("server.workstation.not_found")));
            }
        } else {
            let new_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO workstations (id, name, room_id, manager_employee_id, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(new_id)
            .bind(&item.name)
            .bind(room_id)
            .bind(item.manager_employee_id)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }
    Ok(())
}

/// 同步房间机柜：删除请求中缺失的（含机位的拒绝），再新增/更新。
async fn sync_cabinet_children(
    tx: &mut sqlx::PgConnection,
    room_id: Uuid,
    items: &[foims_models::CabinetSyncItem],
    now: chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    let existing_ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM cabinets WHERE room_id = $1")
        .bind(room_id)
        .fetch_all(&mut *tx)
        .await?;

    let request_ids: Vec<Uuid> = items.iter().filter_map(|i| i.id).collect();

    for existing_id in &existing_ids {
        if !request_ids.contains(existing_id) {
            let position_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM positions WHERE cabinet_id = $1")
                    .bind(existing_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if position_count > 0 {
                return Err(AppError::Validation(msg(
                    "server.cabinet.in_use_by_positions",
                )));
            }
            // 机柜下配线架被线路引用时禁止删除：否则 DELETE 级联删除配线架
            // 会触发 cable_links 防删触发器（自定义异常 → 500），
            // 与独立删除路径（delete_cabinet）的预检口径一致
            let linked_pp_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM patch_panels pp \
                 WHERE pp.cabinet_id = $1 \
                 AND EXISTS ( \
                     SELECT 1 FROM cable_links cl \
                     WHERE (cl.a_endpoint_type = 'patch_panel' AND cl.a_endpoint_id = pp.id) \
                        OR (cl.b_endpoint_type = 'patch_panel' AND cl.b_endpoint_id = pp.id) \
                 )",
            )
            .bind(existing_id)
            .fetch_one(&mut *tx)
            .await?;
            if linked_pp_count > 0 {
                return Err(AppError::Validation(msg(
                    "server.cabinet.patch_panel_linked",
                )));
            }
            sqlx::query("DELETE FROM cabinet_layouts WHERE cabinet_id = $1")
                .bind(existing_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM cabinets WHERE id = $1")
                .bind(existing_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    for item in items {
        validate_cabinet_name(tx, &item.name, room_id, item.id).await?;
        if let Some(item_id) = item.id {
            // 归属校验：仅允许更新当前房间下的机柜，
            // 携带其他房间的 id 时按未找到处理，避免跨父资源静默搬移
            let updated = sqlx::query(
                "UPDATE cabinets SET name = $1, capacity = $2, room_id = $3, updated_at = $4 WHERE id = $5 AND room_id = $6",
            )
            .bind(&item.name)
            .bind(item.capacity)
            .bind(room_id)
            .bind(now)
            .bind(item_id)
            .bind(room_id)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() == 0 {
                return Err(AppError::NotFound(msg("server.cabinet.not_found")));
            }
        } else {
            let new_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO cabinets (id, name, room_id, capacity, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(new_id)
            .bind(&item.name)
            .bind(room_id)
            .bind(item.capacity)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }
    Ok(())
}

/// 信息点名称唯一性预检（全局范围、不限房间；冲突返回 409）。
async fn validate_net_outlet_name(
    conn: &mut sqlx::PgConnection,
    name: &str,
    exclude_id: Option<Uuid>,
) -> Result<(), AppError> {
    crate::helpers::ensure_unique_name(
        conn,
        "net_outlets",
        None,
        None,
        name,
        exclude_id,
        "server.net_outlet.name_exists",
    )
    .await
}

/// 工位名称唯一性预检（同房间内唯一；冲突返回 409）。
async fn validate_workstation_name(
    conn: &mut sqlx::PgConnection,
    name: &str,
    room_id: Uuid,
    exclude_id: Option<Uuid>,
) -> Result<(), AppError> {
    crate::helpers::ensure_unique_name(
        conn,
        "workstations",
        Some("room_id"),
        Some(room_id),
        name,
        exclude_id,
        "server.workstation.name_exists",
    )
    .await
}

/// 机柜名称唯一性预检（同房间内唯一，与独立创建路径的 UNIQUE(room_id, name) 同口径；冲突返回 409）。
async fn validate_cabinet_name(
    conn: &mut sqlx::PgConnection,
    name: &str,
    room_id: Uuid,
    exclude_id: Option<Uuid>,
) -> Result<(), AppError> {
    crate::helpers::ensure_unique_name(
        conn,
        "cabinets",
        Some("room_id"),
        Some(room_id),
        name,
        exclude_id,
        "server.cabinet.name_exists",
    )
    .await
}
