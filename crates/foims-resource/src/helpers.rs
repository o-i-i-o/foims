//! 资源域内部辅助：子网查询、房间校验。
//!
//! 仅由本 crate 的各资源模块使用；不对外再导出。
//! 站内通知与 MAC 变更告警已下放 foims-snmp::events::notify。

use uuid::Uuid;

use foims_common::AppError;
use foims_common::msg;

// ==================== 查询参数解析 ====================

/// 解析可选 UUID 查询参数：缺省/空串返回 None，非法值返回 422。
///
/// 过滤参数不静默忽略非法值退化为全量列表（口径与 network.rs 一致）；
/// 各资源列表 handler 统一走此唯一定义，不再各自内联闭包。
pub(crate) fn parse_optional_uuid(
    query: &std::collections::HashMap<String, String>,
    key: &str,
) -> Result<Option<Uuid>, AppError> {
    match query.get(key) {
        Some(v) if !v.is_empty() => Uuid::parse_str(v).map(Some).map_err(|_| {
            AppError::Validation(msg("server.common.invalid_param").with("param", key))
        }),
        _ => Ok(None),
    }
}

// ==================== 名称唯一性预检 ====================

/// SQL 标识符（表名/列名）校验：仅允许字母数字与下划线。
///
/// 调用方只传入 crate 内静态字面量；此处运行时校验字符集作为
/// 防御层，保证标识符无法携带引号/空白等注入载荷后内插进 SQL。
fn validated_identifier(ident: &str) -> Result<&str, AppError> {
    if !ident.is_empty() && ident.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(ident)
    } else {
        Err(AppError::Internal(
            msg("server.error.internal").with("reason", format!("非法 SQL 标识符: {ident}")),
        ))
    }
}

/// 资源名唯一性预检（crate 内各资源 CRUD 与房间同步路径共用的唯一定义）。
///
/// SQL 形态：`SELECT id FROM {table} WHERE name = $1
/// [AND {parent_col} IS NOT DISTINCT FROM $2]
/// AND ($N::uuid IS NULL OR id != $N)`。
/// - `parent_col = None` 表示名称全局唯一（如信息点）；
/// - `IS NOT DISTINCT FROM` 兼容可空父列（如机位的 cabinet_id）与
///   非空父列（工位/机柜的 room_id）两种语义；
/// - `exclude_id = None` 为创建路径，`Some(id)` 为更新路径。
///
/// 表名/列名仅接受 crate 内静态字面量，经字符集校验后内插，
/// 值一律参数绑定，无拼接注入面。
pub(crate) async fn ensure_unique_name<'e, E>(
    executor: E,
    table: &'static str,
    parent_col: Option<&'static str>,
    parent_id: Option<Uuid>,
    name: &str,
    exclude_id: Option<Uuid>,
    conflict_key: &str,
) -> Result<(), AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT id FROM ");
    builder.push(validated_identifier(table)?);
    builder.push(" WHERE name = ");
    builder.push_bind(name);
    if let Some(col) = parent_col {
        builder.push(" AND ");
        builder.push(validated_identifier(col)?);
        builder.push(" IS NOT DISTINCT FROM ");
        builder.push_bind(parent_id);
    }
    builder.push(" AND (");
    builder.push_bind(exclude_id);
    builder.push("::uuid IS NULL OR id != ");
    builder.push_bind(exclude_id);
    builder.push(")");

    let existing: Option<Uuid> = builder
        .build_query_scalar()
        .persistent(false)
        .fetch_optional(executor)
        .await?;
    if existing.is_some() {
        return Err(AppError::Conflict(msg(conflict_key)));
    }
    Ok(())
}

// ==================== 子网业务查询 ====================

pub const NETWORK_QUERY: &str = r"
    SELECT n.id, n.name, n.network_region_id, nt.name as network_region,
           n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT,
           host(n.ipv4_gateway), host(n.ipv6_gateway),
           (SELECT json_agg(host(d)) FROM unnest(n.ipv4_dns) AS d) as ipv4_dns,
           (SELECT json_agg(host(d)) FROM unnest(n.ipv6_dns) AS d) as ipv6_dns,
           n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ
    FROM network_cidrs n
    JOIN network_regions nt ON n.network_region_id = nt.id
    WHERE n.id = $1
";

pub fn parse_network_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<foims_models::Network, AppError> {
    use sqlx::Row;

    Ok(foims_models::Network {
        id: row.get(0),
        name: row.get(1),
        network_region_id: row.get(2),
        network_region: row.get(3),
        ipv4_cidr: row.get(4),
        ipv6_cidr: row.get(5),
        ipv4_gateway: row.get(6),
        ipv6_gateway: row.get(7),
        ipv4_dns: row
            .get::<Option<serde_json::Value>, _>(8)
            .map(|v| {
                serde_json::from_value(v).map_err(|e| {
                    AppError::Internal(msg("server.common.deserialize_failed").with("error", e))
                })
            })
            .transpose()?,
        ipv6_dns: row
            .get::<Option<serde_json::Value>, _>(9)
            .map(|v| {
                serde_json::from_value(v).map_err(|e| {
                    AppError::Internal(msg("server.common.deserialize_failed").with("error", e))
                })
            })
            .transpose()?,
        description: row.get(10),
        created_at: row.get(11),
        updated_at: row.get(12),
    })
}

pub async fn validate_network_in_room<'e, E>(
    executor: E,
    room_id: Uuid,
    subnet_id: Option<Uuid>,
) -> Result<(), AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let Some(nid) = subnet_id else {
        return Ok(());
    };

    let network_in_room: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM room_networks WHERE room_id = $1 AND subnet_id = $2)",
    )
    .bind(room_id)
    .bind(nid)
    .fetch_one(executor)
    .await?;

    if !network_in_room {
        return Err(AppError::Validation(msg("server.network.not_in_room")));
    }

    Ok(())
}

/// IP 与子网一致性校验：ip_address 必须落在子网的同族 CIDR 内
///（PostgreSQL `<<=` 语义，跨族比较为 false），防止显式指定子网时
/// 把 IP 挂到不含它的网段。子网不存在或同族 CIDR 缺失时同样拒绝。
pub async fn validate_ip_in_subnet<'e, E>(
    executor: E,
    subnet_id: Uuid,
    ip_address: &str,
) -> Result<(), AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let contained: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM network_cidrs
            WHERE id = $1
              AND ((ipv4_cidr IS NOT NULL AND CAST($2 AS INET) <<= ipv4_cidr::inet)
                OR (ipv6_cidr IS NOT NULL AND CAST($2 AS INET) <<= ipv6_cidr::inet))
        )",
    )
    .bind(subnet_id)
    .bind(ip_address)
    .fetch_one(executor)
    .await?;

    if !contained {
        return Err(AppError::Validation(msg("server.ip.not_in_subnet")));
    }

    Ok(())
}

// ==================== 功率容量校验 ====================

/// 功率上限作用域：机柜按机位关联设备求和，房间按归属设备求和。
pub(crate) enum PowerScope {
    Room(Uuid),
    Cabinet(Uuid),
}

impl PowerScope {
    /// 当前已分配功耗合计（SUM 忽略 NULL，无设备返回 0）；
    /// `exclude_device_id` 供更新路径排除设备自身。
    pub(crate) async fn allocated_power_excluding(
        self,
        conn: &mut sqlx::PgConnection,
        exclude_device_id: Option<Uuid>,
    ) -> Result<i64, AppError> {
        match self {
            PowerScope::Room(id) => {
                let allocated: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(SUM(power_watts), 0) FROM devices \
                     WHERE room_id = $1 AND ($2::uuid IS NULL OR id != $2)",
                )
                .bind(id)
                .bind(exclude_device_id)
                .fetch_one(conn)
                .await?;
                Ok(allocated)
            }
            PowerScope::Cabinet(id) => {
                let allocated: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(SUM(d.power_watts), 0) \
                     FROM devices d JOIN positions p ON d.position_id = p.id \
                     WHERE p.cabinet_id = $1 AND ($2::uuid IS NULL OR d.id != $2)",
                )
                .bind(id)
                .bind(exclude_device_id)
                .fetch_one(conn)
                .await?;
                Ok(allocated)
            }
        }
    }
}

/// 设备功耗写入前的容量校验（房间/机柜总功率上限）。
///
/// 所有调用方必须在事务内且已完成房间/机位存在性与一致性校验；
/// 本函数按 房间行（FOR UPDATE）→ 机柜行 的固定锁序加锁，各调用方
/// 一致，无死锁环。校验口径：
/// - `power_watts` 为 None 或 <= 0 不设限；
/// - 房间上限：total_power_watts 非 NULL 时，该房间全部设备功耗之和
///   （含本次写入、排除 `exclude_device_id`）不得超过上限；
/// - 机柜上限：设备挂机位时按机位所属机柜的关联设备求和，上限为
///   NULL 不限制。
///
/// 超限返回 400（AppError::Validation 映射 BAD_REQUEST），携带
/// allocated（现有合计）/power（设备功耗）/total（上限值）占位。
pub(crate) async fn ensure_device_power_capacity(
    conn: &mut sqlx::PgConnection,
    room_id: Uuid,
    position_id: Option<Uuid>,
    exclude_device_id: Option<Uuid>,
    power_watts: Option<i32>,
) -> Result<(), AppError> {
    let Some(w) = power_watts else {
        return Ok(());
    };
    if w <= 0 {
        return Ok(());
    }

    // 房间上限（锁序 1：房间行）
    let room_limit: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT total_power_watts FROM rooms WHERE id = $1 FOR UPDATE",
    )
    .bind(room_id)
    .fetch_optional(&mut *conn)
    .await?
    .flatten();
    if let Some(total) = room_limit {
        let allocated = PowerScope::Room(room_id)
            .allocated_power_excluding(conn, exclude_device_id)
            .await?;
        if allocated + i64::from(w) > i64::from(total) {
            return Err(AppError::Validation(
                msg("server.device.room_power_exceeded")
                    .with("allocated", allocated)
                    .with("power", w)
                    .with("total", total),
            ));
        }
    }

    // 机柜上限（锁序 2：机柜行）；未挂机位的设备仅校验房间
    let Some(pos_id) = position_id else {
        return Ok(());
    };
    let cabinet_id: Option<Uuid> =
        sqlx::query_scalar::<_, Option<Uuid>>("SELECT cabinet_id FROM positions WHERE id = $1")
            .bind(pos_id)
            .fetch_optional(&mut *conn)
            .await?
            .flatten();
    let Some(cab_id) = cabinet_id else {
        return Ok(());
    };
    let cabinet_limit: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT total_power_watts FROM cabinets WHERE id = $1 FOR UPDATE",
    )
    .bind(cab_id)
    .fetch_optional(&mut *conn)
    .await?
    .flatten();
    if let Some(total) = cabinet_limit {
        let allocated = PowerScope::Cabinet(cab_id)
            .allocated_power_excluding(conn, exclude_device_id)
            .await?;
        if allocated + i64::from(w) > i64::from(total) {
            return Err(AppError::Validation(
                msg("server.device.cabinet_power_exceeded")
                    .with("allocated", allocated)
                    .with("power", w)
                    .with("total", total),
            ));
        }
    }

    Ok(())
}

/// 机柜/房间设置（调低）总功率上限时的容量校验：已分配功耗不得超过
/// 新上限；清空上限（NULL）不受此函数约束（调用方跳过）。调用方须在
/// 同一事务内先对目标行 FOR UPDATE 加锁，保证校验-写入窗口无并发写入。
pub(crate) async fn ensure_power_limit_covers_allocated(
    conn: &mut sqlx::PgConnection,
    scope: PowerScope,
    new_limit: i32,
    error_key: &'static str,
) -> Result<(), AppError> {
    let allocated = scope.allocated_power_excluding(conn, None).await?;
    if allocated > i64::from(new_limit) {
        return Err(AppError::Validation(
            msg(error_key)
                .with("allocated", allocated)
                .with("total", new_limit),
        ));
    }
    Ok(())
}

// ==================== 空间归属与责任人校验 ====================

/// 房间空间属性：组织归属与房型。
///
/// 「组织 → 房间 → 工位/机柜/设备」链上的所有子资源校验都以此为准，
/// 避免各路径各自查一遍 room_type/org_id 后口径不一致。
pub(crate) struct RoomSpace {
    pub(crate) org_id: Uuid,
    pub(crate) room_type: String,
}

/// 读取房间空间属性；房间不存在返回校验错误（房间由弹窗同步创建，
/// 引用不存在的房间属于前端构造错误，不是 404 语义）。
pub(crate) async fn load_room_space<'e, E>(
    executor: E,
    room_id: Uuid,
) -> Result<RoomSpace, AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT org_id, room_type FROM rooms WHERE id = $1")
            .bind(room_id)
            .fetch_optional(executor)
            .await?;
    let Some((org_id, room_type)) = row else {
        return Err(AppError::Validation(msg("server.room.not_found")));
    };
    Ok(RoomSpace { org_id, room_type })
}

/// 该房型是否允许工位（办公类 + 其他）。
///
/// 与房间弹窗的子项分区（`showWorkstations`）保持同一判定：
/// 办公类空间才有人坐，机房/弱电井只放机柜。
pub(crate) fn room_allows_workstation(room_type: &str) -> bool {
    matches!(room_type, "OFFICE" | "LOBBY" | "RECEPTION" | "OTHER")
}

/// 该房型是否允许机柜（机房类 + 其他）。
pub(crate) fn room_allows_cabinet(room_type: &str) -> bool {
    matches!(room_type, "DATA_CENTER" | "TELECOM_CLOSET" | "OTHER")
}

/// 责任人同组织校验：管理人员工必须属于指定组织。
///
/// 与触发器 `trg_validate_room_manager_org` / `trg_validate_workstation_manager_org`
/// 同口径（应用层预检把非法引用从 500 降为 400）。`not_found_key` /
/// `mismatch_key` 按调用方资源域传入。
pub(crate) async fn ensure_employee_in_org<'e, E>(
    executor: E,
    employee_id: Uuid,
    org_id: Uuid,
    not_found_key: &'static str,
    mismatch_key: &'static str,
) -> Result<(), AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let employee_org: Option<Uuid> =
        sqlx::query_scalar("SELECT org_id FROM employees WHERE id = $1")
            .bind(employee_id)
            .fetch_optional(executor)
            .await?;
    match employee_org {
        None => Err(AppError::Validation(msg(not_found_key))),
        Some(actual) if actual != org_id => Err(AppError::Validation(msg(mismatch_key))),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 合法标识符原样返回() {
        assert_eq!(validated_identifier("users").ok(), Some("users"));
        assert_eq!(validated_identifier("net_region").ok(), Some("net_region"));
        assert_eq!(validated_identifier("Tbl_2").ok(), Some("Tbl_2"));
    }

    #[test]
    fn 空串与非法字符拒绝防注入() {
        assert!(validated_identifier("").is_err());
        // 引号/空白/分号/连字符等注入载荷均拒绝
        assert!(validated_identifier("users; DROP").is_err());
        assert!(validated_identifier("a b").is_err());
        assert!(validated_identifier("tbl\"").is_err());
        assert!(validated_identifier("col-1").is_err());
    }
}
