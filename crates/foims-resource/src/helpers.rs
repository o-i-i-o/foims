//! 资源域内部辅助：子网查询、房间校验、站内通知与 MAC 变更告警。
//!
//! 仅由本 crate 的各资源模块使用；不对外再导出。

use uuid::Uuid;

use foims_common::AppError;
use foims_common::msg;
use foims_common::{log_error, log_info, log_warn};

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

// ==================== 站内通知与 MAC 变更告警 ====================

/// 组装站内通知内容：以 JSON 形式存储「消息 key + 动态参数」，
/// 前端展示时解析并按用户语言翻译；历史遗留的纯文本内容原样展示。
fn encode_notification_content(key: &str, params: &[(&str, &str)]) -> String {
    let params: std::collections::HashMap<String, String> = params
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    serde_json::json!({ "key": key, "params": params }).to_string()
}

/// 写入一条站内通知（user_id 为 None 时为广播占位历史行）。
pub async fn create_notification(
    pool: &sqlx::PgPool,
    title: &str,
    content: &str,
    notification_type: &str,
    user_id: Option<&Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(r"INSERT INTO notifications (id, user_id, title, content, notification_type, read, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7)")
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(title)
        .bind(content)
        .bind(notification_type)
        .bind(false)
        .bind(chrono::Utc::now())
        .execute(pool)
        .await?;
    Ok(())
}

/// 工作站 MAC 变更告警：站内通知全部管理员并发送邮件（SMTP 未配置时降级）。
pub async fn send_mac_change_notification(
    pool: &sqlx::PgPool,
    workstation_id: &Uuid,
    ip_address: &str,
    old_mac: &str,
    new_mac: &str,
) -> Result<(), sqlx::Error> {
    let workstation_name =
        match sqlx::query_scalar::<_, String>("SELECT name FROM workstations WHERE id = $1")
            .bind(workstation_id)
            .fetch_optional(pool)
            .await
        {
            Ok(Some(name)) => name,
            Ok(None) => {
                log_warn!("log.workstation.not_found", id = workstation_id);
                workstation_id.to_string()
            }
            Err(e) => {
                log_warn!("log.workstation.query_name_failed", error = e);
                workstation_id.to_string()
            }
        };

    let content = encode_notification_content(
        "server.notification.mac_change.body",
        &[
            ("workstation", &workstation_name),
            ("ip", ip_address),
            ("old_mac", old_mac),
            ("new_mac", new_mac),
        ],
    );
    // 通知目标：所有启用状态的管理员（admin/secadmin）按人各发一条，
    // 已读状态随用户独立；此前统一写 user_id = NULL 的行不匹配任何人的
    // 查询条件（user_id = $1），通知列表对所有用户恒为空
    let admin_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM users WHERE status = TRUE AND role IN ('admin', 'secadmin')",
    )
    .fetch_all(pool)
    .await?;

    for admin_id in &admin_ids {
        create_notification(
            pool,
            "server.notification.mac_change.title",
            &content,
            "mac_change",
            Some(admin_id),
        )
        .await?;
    }
    log_info!(
        "log.mac_change.notification_created",
        workstation = workstation_name,
        ip = ip_address,
        recipients = admin_ids.len()
    );

    match foims_auth::smtp::send_mac_change_email(
        pool,
        &workstation_name,
        ip_address,
        old_mac,
        new_mac,
    )
    .await
    {
        Ok(()) => {
            log_info!("log.mac_change.email_sent", workstation = workstation_name)
        }
        // SMTP 未配置实际返回 Validation 错误（走下方兜底记录）；
        // 收件人缺失已在 send_mac_change_email 内降级为 Ok(())，此分支
        // 当前不可达，保留作兜底
        Err(AppError::NotFound(m)) => {
            log_warn!(
                "log.mac_change.email_skipped",
                workstation = workstation_name,
                reason = m.key()
            );
        }
        Err(e) => {
            log_error!(
                "log.mac_change.email_send_failed",
                workstation = workstation_name,
                error = e
            );
        }
    }

    Ok(())
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_notification_content() {
        let content = encode_notification_content(
            "server.notification.mac_change.body",
            &[("ip", "10.0.0.1"), ("old_mac", "aa:aa")],
        );
        let value: serde_json::Value =
            serde_json::from_str(&content).unwrap_or_else(|e| panic!("应为合法 JSON: {e}"));
        assert_eq!(value["key"], "server.notification.mac_change.body");
        assert_eq!(value["params"]["ip"], "10.0.0.1");
        assert_eq!(value["params"]["old_mac"], "aa:aa");
    }

    #[test]
    fn test_encode_notification_content_empty_params() {
        // 无参数时 params 为空对象
        let content = encode_notification_content("some.key", &[]);
        let value: serde_json::Value =
            serde_json::from_str(&content).unwrap_or_else(|e| panic!("应为合法 JSON: {e}"));
        assert_eq!(value["key"], "some.key");
        assert_eq!(
            value["params"].as_object().map(serde_json::Map::len),
            Some(0)
        );
    }
}
