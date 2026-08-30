//! 物理链路（cable_link）资源管理。
//!
//! 链路连接两个端点（设备端口/网络接口/信息点/配线架端口），端点按
//! `(类型, id)` 规范序存储以保证 A/B 双向查询去重。列表查询使用
//! sqlx `QueryBuilder` 动态拼接过滤条件，全部用户输入经 `push_bind`
//! 参数绑定，排序字段走白名单。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use foims_common::msg;
use sqlx::{PgExecutor, Postgres, QueryBuilder, Row};
use uuid::Uuid;
use validator::Validate;

use foims_auth::meta::{RequestMeta, log_op_best_effort};
use foims_common::AppError;
use foims_common::AppJson;
use foims_common::DbProvider;
use foims_common::pagination::{Pagination, paged_response};
use foims_models::{CableLinkCreate, CableLinkUpdate, CableLinkWithDetails, CablePathNode};

/// 合法的端点资源类型。
const VALID_ENDPOINT_TYPES: [&str; 3] = ["net_outlet", "device_interface", "patch_panel"];
/// 合法的链路类型。
const VALID_LINK_TYPES: [&str; 3] = ["ethernet", "fiber", "console"];
/// `cable_links_with_details` 视图与 `CableLinkWithDetails` 映射对应的列清单。
const CABLE_LINK_COLUMNS: &str = "cl.id, cl.a_endpoint_type, cl.a_endpoint_id, cl.a_endpoint_label, \
     cl.a_room_id, cl.a_cabinet_id, cl.a_device_id, \
     cl.b_endpoint_type, cl.b_endpoint_id, cl.b_endpoint_label, \
     cl.b_room_id, cl.b_cabinet_id, cl.b_device_id, \
     cl.link_type, cl.cable_label, cl.length_m, cl.tested, \
     cl.created_at::TIMESTAMPTZ, cl.updated_at::TIMESTAMPTZ";

fn validate_endpoint_type(endpoint_type: &str) -> Result<(), AppError> {
    if VALID_ENDPOINT_TYPES.contains(&endpoint_type) {
        Ok(())
    } else {
        Err(AppError::Validation(
            msg("server.cable_link.invalid_endpoint_type")
                .with("types", VALID_ENDPOINT_TYPES.join(", ")),
        ))
    }
}

fn validate_link_type(link_type: &str) -> Result<(), AppError> {
    if VALID_LINK_TYPES.contains(&link_type) {
        Ok(())
    } else {
        Err(AppError::Validation(
            msg("server.cable_link.invalid_link_type").with("types", VALID_LINK_TYPES.join(", ")),
        ))
    }
}

/// 将 A/B 端点按 `(类型字符串, id)` 升序排序，保证同一条链路唯一存储。
fn sort_endpoints(
    a_type: &str,
    a_id: Uuid,
    b_type: &str,
    b_id: Uuid,
) -> (String, Uuid, String, Uuid) {
    if a_type < b_type || (a_type == b_type && a_id < b_id) {
        (a_type.to_string(), a_id, b_type.to_string(), b_id)
    } else {
        (b_type.to_string(), b_id, a_type.to_string(), a_id)
    }
}

/// 追加列表过滤条件（端点 A/B 双向匹配 + 链路类型），供 COUNT 与数据查询共用。
fn push_link_filters(
    builder: &mut QueryBuilder<Postgres>,
    endpoint: Option<(&str, Uuid)>,
    link_type: Option<&str>,
) {
    let mut first = true;
    if let Some((endpoint_type, endpoint_id)) = endpoint {
        builder
            .push(" WHERE ((cl.a_endpoint_type = ")
            .push_bind(endpoint_type)
            .push(" AND cl.a_endpoint_id = ")
            .push_bind(endpoint_id)
            .push(") OR (cl.b_endpoint_type = ")
            .push_bind(endpoint_type)
            .push(" AND cl.b_endpoint_id = ")
            .push_bind(endpoint_id)
            .push("))");
        first = false;
    }
    if let Some(link_type) = link_type {
        builder
            .push(if first { " WHERE " } else { " AND " })
            .push("cl.link_type = ")
            .push_bind(link_type);
    }
}

/// 按 id 从 `cable_links_with_details` 视图查询链路详情。
async fn fetch_link_by_id(
    executor: impl PgExecutor<'_>,
    id: Uuid,
) -> Result<Option<CableLinkWithDetails>, sqlx::Error> {
    // 列清单来自内部常量，无用户输入，可用 AssertSqlSafe 声明已审计
    sqlx::query_as::<_, CableLinkWithDetails>(sqlx::AssertSqlSafe(format!(
        "SELECT {CABLE_LINK_COLUMNS} FROM cable_links_with_details cl WHERE cl.id = $1"
    )))
    .bind(id)
    .fetch_optional(executor)
    .await
}

/// 分页获取物理链路列表，支持端点（A/B 双向）、链路类型过滤与白名单排序。
pub async fn get_cable_links<P: DbProvider>(
    State(state): State<Arc<P>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let endpoint_type = query.get("endpoint_type").map(String::as_str);
    // 非法 UUID 显式 422（与 network.rs 口径一致），不静默退化为全量列表
    let endpoint_id = match query.get("endpoint_id") {
        Some(v) if !v.is_empty() => Some(Uuid::parse_str(v).map_err(|_| {
            AppError::Validation(msg("server.common.invalid_param").with("param", "endpoint_id"))
        })?),
        _ => None,
    };
    let link_type = query.get("link_type").map(String::as_str);
    let sort_by = query.get("sort_by").map(String::as_str).unwrap_or_default();
    let sort_order = query
        .get("sort_order")
        .map(String::as_str)
        .unwrap_or_default();

    // 端点过滤需要类型与 id 同时提供才生效（与既有行为一致）
    let endpoint_filter = match (endpoint_type, endpoint_id) {
        (Some(endpoint_type), Some(endpoint_id)) => Some((endpoint_type, endpoint_id)),
        _ => None,
    };

    // ORDER BY 白名单，未匹配时回落默认序，避免注入
    let order_clause = match (sort_by, sort_order) {
        ("link_type", "desc") => " ORDER BY cl.link_type DESC, cl.updated_at DESC",
        ("link_type", _) => " ORDER BY cl.link_type ASC, cl.updated_at DESC",
        ("cable_label", "desc") => " ORDER BY cl.cable_label DESC NULLS LAST, cl.updated_at DESC",
        ("cable_label", _) => " ORDER BY cl.cable_label ASC NULLS LAST, cl.updated_at DESC",
        ("length_m", "desc") => " ORDER BY cl.length_m DESC NULLS LAST, cl.updated_at DESC",
        ("length_m", _) => " ORDER BY cl.length_m ASC NULLS LAST, cl.updated_at DESC",
        ("tested", "desc") => " ORDER BY cl.tested DESC, cl.updated_at DESC",
        ("tested", _) => " ORDER BY cl.tested ASC, cl.updated_at DESC",
        ("updated_at", "asc") => " ORDER BY cl.updated_at ASC",
        _ => " ORDER BY cl.updated_at DESC",
    };

    let mut count_builder = QueryBuilder::<Postgres>::new("SELECT COUNT(*) FROM cable_links cl");
    push_link_filters(&mut count_builder, endpoint_filter, link_type);
    let total: i64 = count_builder
        .build_query_scalar()
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let mut data_builder = QueryBuilder::<Postgres>::new(format!(
        "SELECT {CABLE_LINK_COLUMNS} FROM cable_links_with_details cl"
    ));
    push_link_filters(&mut data_builder, endpoint_filter, link_type);
    data_builder
        .push(order_clause)
        .push(" LIMIT ")
        .push_bind(pagination.page_size as i32)
        .push(" OFFSET ")
        // offset 直接绑定 i64：`as i32` 会在超大页码时截断为负值
        .push_bind(pagination.offset);
    let links = data_builder
        .build_query_as::<CableLinkWithDetails>()
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    Ok(foims_common::ok_json(
        paged_response(links, total, &pagination),
        "server.cable_link.fetched",
    ))
}

/// 校验两个 device_interface 端点不属于同一台设备。
///
/// 任意两台不同设备允许直连；同一设备的两个端口经物理线路互连
/// 会形成自环，应用层先行拦截（数据库触发器为 CSV 导入等旁路兜底）。
async fn ensure_interfaces_on_different_devices(
    tx: &mut sqlx::PgConnection,
    a_id: Uuid,
    b_id: Uuid,
) -> Result<(), AppError> {
    let same_device: bool = sqlx::query_scalar(
        r"SELECT EXISTS(
            SELECT 1 FROM device_interfaces a
            JOIN device_interfaces b ON a.device_id = b.device_id
            WHERE a.id = $1 AND b.id = $2)",
    )
    .bind(a_id)
    .bind(b_id)
    .fetch_one(tx)
    .await?;
    if same_device {
        return Err(AppError::Validation(msg(
            "server.cable_link.same_device_ports_forbidden",
        )));
    }
    Ok(())
}

/// 端点 (类型, id) 存在性预检：端点资源缺失时返回 422，
/// 不再依赖 cable_links 防删触发器的 P0001 自定义异常（→ 500）。
/// 表名来自类型白名单映射（非用户输入直拼），id 经参数绑定。
async fn ensure_endpoint_exists(
    tx: &mut sqlx::PgConnection,
    endpoint_type: &str,
    endpoint_id: Uuid,
) -> Result<(), AppError> {
    let table = match endpoint_type {
        "net_outlet" => "net_outlets",
        "device_interface" => "device_interfaces",
        "patch_panel" => "patch_panels",
        // validate_endpoint_type 已在入口拦截非法类型，此分支仅为白名单完备
        _ => {
            return Err(AppError::Validation(
                msg("server.cable_link.invalid_endpoint_type")
                    .with("types", VALID_ENDPOINT_TYPES.join(", ")),
            ));
        }
    };
    let mut builder = QueryBuilder::new("SELECT EXISTS(SELECT 1 FROM ");
    builder.push(table);
    builder.push(" WHERE id = ");
    builder.push_bind(endpoint_id);
    builder.push(")");
    let exists: bool = builder.build_query_scalar().fetch_one(&mut *tx).await?;
    if !exists {
        return Err(AppError::Validation(
            msg("server.cable_link.endpoint_not_found")
                .with("type", endpoint_type)
                .with("id", endpoint_id),
        ));
    }
    Ok(())
}

/// 端点对重复（同 A/B 规范序端点已存在链路）→ 409 冲突。
fn duplicate_link_error() -> AppError {
    AppError::Conflict(msg("server.db.conflict"))
}

/// cable_links 唯一约束冲突（并发写入竞态触发 DB 兜底）→ 409。
fn map_cable_link_unique_violation(e: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(ref db_err) = e
        && db_err.is_unique_violation()
    {
        return duplicate_link_error();
    }
    AppError::from(e)
}

/// 创建物理链路（端点校验后按规范序写入）。
pub async fn create_cable_link<P: DbProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<CableLinkCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    validate_endpoint_type(&req.a_endpoint_type)?;
    validate_endpoint_type(&req.b_endpoint_type)?;

    if req.a_endpoint_type == req.b_endpoint_type && req.a_endpoint_id == req.b_endpoint_id {
        return Err(AppError::Validation(msg(
            "server.cable_link.self_connection_forbidden",
        )));
    }

    // 线缆长度非负校验（DB 列无 CHECK，handler 兜底）
    if let Some(length) = req.length_m
        && length < 0.0
    {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "length_m"),
        ));
    }

    let link_type = req.link_type.as_deref().unwrap_or("ethernet");
    validate_link_type(link_type)?;

    let (a_type, a_id, b_type, b_id) = sort_endpoints(
        &req.a_endpoint_type,
        req.a_endpoint_id,
        &req.b_endpoint_type,
        req.b_endpoint_id,
    );

    let id = Uuid::new_v4();
    let now = Utc::now();
    let tested = req.tested.unwrap_or(false);

    let mut tx = state.pool()?.get_conn().begin().await?;

    // 任意两台不同设备允许直连；仅禁止同一设备的两个端口互连（环路风险）
    if a_type == "device_interface" && b_type == "device_interface" {
        ensure_interfaces_on_different_devices(&mut tx, a_id, b_id).await?;
    }

    // 端点资源存在性预检：缺失时 422，而非触发器 P0001 的 500
    ensure_endpoint_exists(&mut tx, &a_type, a_id).await?;
    ensure_endpoint_exists(&mut tx, &b_type, b_id).await?;

    // 重复端点对预检（uq_cable_links_endpoint_pair），并发兜底见 INSERT 的
    // 唯一冲突映射：正常业务冲突返回 409 而非 500
    let duplicate: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM cable_links \
         WHERE a_endpoint_type = $1 AND a_endpoint_id = $2 \
           AND b_endpoint_type = $3 AND b_endpoint_id = $4)",
    )
    .bind(&a_type)
    .bind(a_id)
    .bind(&b_type)
    .bind(b_id)
    .fetch_one(&mut *tx)
    .await?;
    if duplicate {
        return Err(duplicate_link_error());
    }

    sqlx::query(
        "INSERT INTO cable_links (id, a_endpoint_type, a_endpoint_id, b_endpoint_type, b_endpoint_id, link_type, cable_label, length_m, tested, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(id)
    .bind(&a_type)
    .bind(a_id)
    .bind(&b_type)
    .bind(b_id)
    .bind(link_type)
    .bind(&req.cable_label)
    .bind(req.length_m)
    .bind(tested)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(map_cable_link_unique_violation)?;

    let link = fetch_link_by_id(&mut *tx, id)
        .await?
        .ok_or_else(|| AppError::Internal(msg("server.cable_link.fetch_after_create_failed")))?;

    tx.commit().await?;

    let details = serde_json::json!({
        "a_endpoint_type": a_type,
        "a_endpoint_id": a_id,
        "b_endpoint_type": b_type,
        "b_endpoint_id": b_id,
        "link_type": link_type
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "cable_link",
        Some(&id),
        &details,
    )
    .await;

    Ok(foims_common::ok_json(link, "server.cable_link.created"))
}

/// 获取单条物理链路详情。
pub async fn get_cable_link<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let link = fetch_link_by_id(&state.pool()?.get_conn(), id)
        .await?
        .ok_or_else(|| AppError::NotFound(msg("server.cable_link.not_found")))?;

    Ok(foims_common::ok_json(link, "server.cable_link.fetched"))
}

/// 更新物理链路。
///
/// 端点四字段（A/B 类型与 id）必须同时提供才会更新；可空字段
/// （标签/长度）以 `Some(None)` 表示清除、字段缺失表示不修改。
pub async fn update_cable_link<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<CableLinkUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    if let Some(link_type) = req.link_type.as_deref() {
        validate_link_type(link_type)?;
    }

    // 线缆长度非负校验（双层 Option：Some(Some(v)) 为设置新值）
    if let Some(Some(length)) = req.length_m
        && length < 0.0
    {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "length_m"),
        ));
    }

    // 端点更新：四字段必须同时提供，参照 create 校验并按规范序排序
    let new_endpoints = match (
        req.a_endpoint_type.as_deref(),
        req.a_endpoint_id,
        req.b_endpoint_type.as_deref(),
        req.b_endpoint_id,
    ) {
        (Some(a_type), Some(a_id), Some(b_type), Some(b_id)) => {
            validate_endpoint_type(a_type)?;
            validate_endpoint_type(b_type)?;
            if a_type == b_type && a_id == b_id {
                return Err(AppError::Validation(msg(
                    "server.cable_link.self_connection_forbidden",
                )));
            }
            Some(sort_endpoints(a_type, a_id, b_type, b_id))
        }
        (None, None, None, None) => None,
        _ => {
            return Err(AppError::Validation(msg(
                "server.cable_link.endpoints_partial_update",
            )));
        }
    };

    let mut tx = state.pool()?.get_conn().begin().await?;

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cable_links WHERE id = $1)")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Err(AppError::NotFound(msg("server.cable_link.not_found")));
    }

    // 端点变更后的端点对重复预检（排除自身，与 create 同一口径）
    if let Some((a_type, a_id, b_type, b_id)) = &new_endpoints {
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM cable_links \
             WHERE a_endpoint_type = $1 AND a_endpoint_id = $2 \
               AND b_endpoint_type = $3 AND b_endpoint_id = $4 AND id != $5)",
        )
        .bind(a_type)
        .bind(a_id)
        .bind(b_type)
        .bind(b_id)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if duplicate {
            return Err(duplicate_link_error());
        }

        // 端点变更时校验不属于同一设备（与 create 同一口径）
        if a_type == "device_interface" && b_type == "device_interface" {
            ensure_interfaces_on_different_devices(&mut tx, *a_id, *b_id).await?;
        }

        // 端点资源存在性预检（与 create 同一口径）
        ensure_endpoint_exists(&mut tx, a_type, *a_id).await?;
        ensure_endpoint_exists(&mut tx, b_type, *b_id).await?;
    }

    let has_field_update = req.link_type.is_some()
        || req.cable_label.is_some()
        || req.length_m.is_some()
        || req.tested.is_some()
        || new_endpoints.is_some();
    if !has_field_update {
        return Err(AppError::Validation(msg(
            "server.cable_link.no_fields_to_update",
        )));
    }

    let mut builder = QueryBuilder::<Postgres>::new("UPDATE cable_links SET ");
    {
        // separated 会在非首个 push 前自动插入分隔符，因此列名片段用 push、
        // 绑定值紧随其后用 push_bind_unseparated，避免生成 "col = , $1"
        let mut sep = builder.separated(", ");
        if let Some(link_type) = &req.link_type {
            sep.push("link_type = ").push_bind_unseparated(link_type);
        }
        // Option<Option<T>>：Some(Some(v)) 设新值；Some(None) 置空
        //（bind 对 Option 直接编码为 NULL，无需 CASE WHEN 区分）
        if let Some(label) = &req.cable_label {
            sep.push("cable_label = ").push_bind_unseparated(label);
        }
        if let Some(length) = req.length_m {
            sep.push("length_m = ").push_bind_unseparated(length);
        }
        if let Some(tested) = req.tested {
            sep.push("tested = ").push_bind_unseparated(tested);
        }
        if let Some((a_type, a_id, b_type, b_id)) = &new_endpoints {
            sep.push("a_endpoint_type = ").push_bind_unseparated(a_type);
            sep.push("a_endpoint_id = ").push_bind_unseparated(*a_id);
            sep.push("b_endpoint_type = ").push_bind_unseparated(b_type);
            sep.push("b_endpoint_id = ").push_bind_unseparated(*b_id);
        }
        sep.push("updated_at = ").push_bind_unseparated(Utc::now());
    }
    builder.push(" WHERE id = ").push_bind(id);
    builder
        .build()
        .execute(&mut *tx)
        .await
        .map_err(map_cable_link_unique_violation)?;

    let link = fetch_link_by_id(&mut *tx, id)
        .await?
        .ok_or_else(|| AppError::Internal(msg("server.cable_link.fetch_after_update_failed")))?;

    tx.commit().await?;

    let details = serde_json::json!({ "cable_link_id": id.to_string() });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "cable_link",
        Some(&id),
        &details,
    )
    .await;

    Ok(foims_common::ok_json(link, "server.cable_link.updated"))
}

/// 删除物理链路。
pub async fn delete_cable_link<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let mut tx = state.pool()?.get_conn().begin().await?;

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cable_links WHERE id = $1)")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Err(AppError::NotFound(msg("server.cable_link.not_found")));
    }

    sqlx::query("DELETE FROM cable_links WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({ "cable_link_id": id.to_string() });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "cable_link",
        Some(&id),
        &details,
    )
    .await;

    Ok(foims_common::ok_json((), "server.cable_link.deleted"))
}

#[derive(Debug, serde::Deserialize)]
pub struct CablePathQuery {
    pub from_type: String,
    pub from_id: Uuid,
    pub to_type: String,
    pub to_id: Uuid,
}

/// 查询两端点之间的线缆路径（调用数据库 `find_cable_path` 函数逐跳返回）。
pub async fn get_cable_path<P: DbProvider>(
    State(state): State<Arc<P>>,
    Query(q): Query<CablePathQuery>,
) -> Result<Response, AppError> {
    validate_endpoint_type(&q.from_type)?;
    validate_endpoint_type(&q.to_type)?;

    let rows = sqlx::query(
        "SELECT hop_idx, node_type, node_id, node_label, cable_id, cable_label, hop_type \
         FROM find_cable_path($1, $2, $3, $4) ORDER BY hop_idx",
    )
    .bind(&q.from_type)
    .bind(q.from_id)
    .bind(&q.to_type)
    .bind(q.to_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let path: Vec<CablePathNode> = rows
        .into_iter()
        .map(|r| CablePathNode {
            hop_idx: r.get::<i32, _>("hop_idx"),
            node_type: r.get::<String, _>("node_type"),
            node_id: r.get::<Uuid, _>("node_id"),
            node_label: r.get::<Option<String>, _>("node_label"),
            cable_id: r.get::<Option<Uuid>, _>("cable_id"),
            cable_label: r.get::<Option<String>, _>("cable_label"),
            hop_type: r.get::<String, _>("hop_type"),
        })
        .collect();

    if path.is_empty() {
        return Err(AppError::NotFound(msg("server.cable_link.path_not_found")));
    }

    Ok(foims_common::ok_json(
        serde_json::json!({ "path": path, "hop_count": path.len() }),
        "server.cable_link.path_fetched",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 端点/链路类型校验 ====================

    #[test]
    fn test_validate_endpoint_type_accepts_all_valid_types() {
        for endpoint_type in VALID_ENDPOINT_TYPES {
            assert!(
                validate_endpoint_type(endpoint_type).is_ok(),
                "合法端点类型 {endpoint_type} 应通过校验"
            );
        }
    }

    #[test]
    fn test_validate_endpoint_type_rejects_invalid() {
        // 未收录类型、空串、大小写变体均应拒绝为 Validation 错误
        for invalid in [
            "",
            "device",
            "port",
            "Device_Port",
            "net_outlets",
            "patch-panel",
        ] {
            let result = validate_endpoint_type(invalid);
            let err = result
                .err()
                .unwrap_or_else(|| panic!("非法类型 {invalid} 应被拒绝"));
            assert!(
                matches!(err, AppError::Validation(_)),
                "应返回 Validation 错误，实际: {err}"
            );
        }
    }

    #[test]
    fn test_validate_link_type_accepts_all_valid_types() {
        for link_type in VALID_LINK_TYPES {
            assert!(
                validate_link_type(link_type).is_ok(),
                "合法链路类型 {link_type} 应通过校验"
            );
        }
    }

    #[test]
    fn test_validate_link_type_rejects_invalid() {
        for invalid in ["", "Ethernet", "optic", "cons0le", "ethernet "] {
            let result = validate_link_type(invalid);
            let err = result
                .err()
                .unwrap_or_else(|| panic!("非法类型 {invalid} 应被拒绝"));
            assert!(
                matches!(err, AppError::Validation(_)),
                "应返回 Validation 错误，实际: {err}"
            );
        }
    }

    // ==================== 端点规范化序 ====================

    #[test]
    fn test_sort_endpoints_orders_by_type_string() {
        // 类型不同时按类型字符串升序：device_interface < net_outlet
        let id_x = Uuid::new_v4();
        let id_y = Uuid::new_v4();
        let (a_type, a_id, b_type, b_id) =
            sort_endpoints("net_outlet", id_x, "device_interface", id_y);
        assert_eq!(a_type, "device_interface");
        assert_eq!(a_id, id_y);
        assert_eq!(b_type, "net_outlet");
        assert_eq!(b_id, id_x);
    }

    #[test]
    fn test_sort_endpoints_same_type_orders_by_uuid() {
        // 类型相同时按 id 升序：确保 id_1 > id_2 的构造场景
        let (id_big, id_small) = loop {
            let u1 = Uuid::new_v4();
            let u2 = Uuid::new_v4();
            if u1 > u2 {
                break (u1, u2);
            }
        };
        let (a_type, a_id, b_type, b_id) =
            sort_endpoints("patch_panel", id_big, "patch_panel", id_small);
        assert_eq!(a_type, "patch_panel");
        assert_eq!(b_type, "patch_panel");
        assert_eq!(a_id, id_small, "同类型时较小的 id 应排到 A 端");
        assert_eq!(b_id, id_big);
    }

    #[test]
    fn test_sort_endpoints_already_ordered_unchanged() {
        // 已满足规范序时保持原顺序（类型不同且 a_type < b_type）
        let id_a = Uuid::new_v4();
        let id_b = Uuid::new_v4();
        let (a_type, a_id, b_type, b_id) =
            sort_endpoints("device_interface", id_a, "net_outlet", id_b);
        assert_eq!(a_type, "device_interface");
        assert_eq!(a_id, id_a);
        assert_eq!(b_type, "net_outlet");
        assert_eq!(b_id, id_b);
    }

    #[test]
    fn test_sort_endpoints_commutative() {
        // 交换输入顺序应得到完全相同的规范化结果（A/B 双向存储去重依据）
        let type_a = "device_interface";
        let id_a = Uuid::new_v4();
        let type_b = "patch_panel";
        let id_b = Uuid::new_v4();

        let forward = sort_endpoints(type_a, id_a, type_b, id_b);
        let backward = sort_endpoints(type_b, id_b, type_a, id_a);
        assert_eq!(forward, backward);

        // 结果必须满足规范序定义：(a_type < b_type) 或 (同类型且 a_id <= b_id)
        let (a_type, a_id, b_type, b_id) = forward;
        assert!(
            a_type < b_type || (a_type == b_type && a_id <= b_id),
            "规范化结果必须满足字典规范序"
        );
    }

    #[test]
    fn test_sort_endpoints_idempotent() {
        // 对已排序结果再次排序应保持不变（幂等性）
        let type_a = "net_outlet";
        let id_a = Uuid::new_v4();
        let type_b = "net_outlet";
        let id_b = Uuid::new_v4();
        let (a_type, a_id, b_type, b_id) = sort_endpoints(type_a, id_a, type_b, id_b);
        // 再排一次
        let (a2_type, a2_id, b2_type, b2_id) = sort_endpoints(&a_type, a_id, &b_type, b_id);
        assert_eq!((a2_type, a2_id), (a_type, a_id));
        assert_eq!((b2_type, b2_id), (b_type, b_id));
    }
}
