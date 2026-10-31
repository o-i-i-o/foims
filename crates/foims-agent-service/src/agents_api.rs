//! Agent 监控 Web API（设计 docs/agent-design.md §5.3）：Agent 列表、主机
//! 负载 Top 榜、详情、历史曲线与 label/status 维护。
//!
//! 挂载于 /api/agents（与配置包下载同 nest，经 agent_admin_guard 纵深防御）。

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use foims_auth::extractor::AdminOrSecAdminUser;
use foims_common::net::escape_like;
use foims_common::pagination::{Pagination, paged_response};
use foims_common::provider::ConfigProvider;
use foims_common::{ApiResponse, AppError, msg};

/// 曲线数据抽样上限：超过该点数按均匀步长抽取，避免前端渲染卡顿
const HISTORY_MAX_POINTS: usize = 200;

/// 采集端口名长度上限（与 device_interfaces.name VARCHAR(50) 对齐）
const MAX_INTERFACE_NAME_LEN: usize = 50;

/// GET /api/agents 查询参数。
#[derive(Debug, Deserialize)]
pub struct ListAgentsQuery {
    page: Option<i64>,
    page_size: Option<i64>,
    /// 按状态精确过滤（active/offline/disabled）
    status: Option<String>,
    /// 模糊匹配 hostname / label / ip
    keyword: Option<String>,
}

/// PATCH /api/agents/{id} 请求体。
#[derive(Debug, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct PatchAgentBody {
    /// 备注（trim 后 ≤100 字符）
    label: Option<String>,
    /// 目标状态：仅允许 active | disabled（disabled 拒收该机上报）
    status: Option<String>,
}

/// PATCH 状态白名单（在线/离线由上报自动翻转，仅禁用需人工设置）
const PATCHABLE_STATUSES: &[&str] = &["active", "disabled"];

/// PUT /api/agents/{id}/monitor-port 请求体。
#[derive(Debug, Deserialize)]
pub struct MonitorPortBody {
    /// 实时流量采集端口名；缺省/空串表示不指定（回落设备首端口）
    if_name: Option<String>,
}

/// 采集端口名规整：trim 后空串视为「未指定」（纯函数，单测覆盖）。
fn normalize_monitor_if(name: Option<&str>) -> Option<String> {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// 校验 PATCH 的 status 取值：缺省通过，越界值返回 Err（纯函数，单测覆盖）。
fn normalize_patch_status(status: Option<&str>) -> Result<Option<String>, String> {
    match status.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) if PATCHABLE_STATUSES.contains(&s) => Ok(Some(s.to_string())),
        Some(s) => Err(format!(
            "status 取值非法: {s}（仅允许 {}）",
            PATCHABLE_STATUSES.join("/")
        )),
    }
}

/// 曲线点均匀抽样：超过 max_points 时按步长抽取，首尾点恒保留
/// （纯函数，单测覆盖）。
fn sample_series(points: Vec<Value>, max_points: usize) -> Vec<Value> {
    let len = points.len();
    if len <= max_points || max_points < 2 {
        return points;
    }
    // 步长向上取整：⌈(len-1)/(max-1)⌉，保证抽取点数 ≤ max 且覆盖到末点
    let step = (len - 1).div_ceil(max_points - 1);
    let mut sampled: Vec<Value> = points.iter().step_by(step).cloned().collect();
    let last = points[len - 1].clone();
    if sampled.last().map(|v| v == &last) != Some(true) {
        sampled.push(last);
    }
    sampled
}

/// Timestamp → RFC 3339 字符串（秒精度，Z 后缀）。
pub(crate) fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// 同 IP 多来源去重优先级（ipmi > agent > snmp）：实际场景 agent 与 ipmi 分处
/// 不同 IP（BMC 管理口与 OS），此处仅作同 IP 并存时展示层的确定性取舍。
/// SQL 片段：列表与 Top 榜的 `ROW_NUMBER() OVER (... ORDER BY <本表达式>)` 共用。
const SOURCE_RANK_SQL: &str =
    "(CASE source WHEN 'ipmi' THEN 0 WHEN 'agent' THEN 1 WHEN 'snmp' THEN 2 ELSE 3 END)";

/// 去重窗口函数的分区键：NULL ip 行以自身 id 单独成组（恒保留），非 NULL ip
/// 行按 IP 分组取 rank 最优。
const DEDUP_PARTITION_SQL: &str = "COALESCE(ip, id::text)";

/// 向 QueryBuilder 追加与列表一致的过滤条件（status 精确 + keyword 模糊）。
fn push_filters(
    builder: &mut sqlx::QueryBuilder<sqlx::Postgres>,
    status: Option<&str>,
    keyword: Option<&str>,
) {
    if let Some(status) = status.map(str::trim).filter(|s| !s.is_empty()) {
        builder.push(" AND status = ").push_bind(status.to_string());
    }
    if let Some(keyword) = keyword.map(str::trim).filter(|s| !s.is_empty()) {
        let pattern = escape_like(keyword);
        builder
            .push(" AND (hostname ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR label ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR ip ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
}

/// GET /api/agents：分页列表（keyword 模糊匹配 hostname/label/ip，status
/// 精确过滤，按 last_seen、created_at 倒序；响应不含 raw_metrics）。
pub async fn list_agents<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
    Query(query): Query<ListAgentsQuery>,
) -> Result<Response, AppError> {
    let pool = state.pool()?.get_conn();
    let pagination = Pagination::new(query.page.unwrap_or(1), query.page_size.unwrap_or(20));

    // 同 IP 去重（ipmi > agent > snmp）：按 IP（NULL 以自身 id）分组取 rank 最优行，
    // 各来源 DB 行保留不变，仅展示层去重。count 与 rows 共用同一 dedup CTE。
    let mut count_builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(format!(
        r"WITH dedup AS (
             SELECT id, ROW_NUMBER() OVER (
               PARTITION BY {DEDUP_PARTITION_SQL}
               ORDER BY {SOURCE_RANK_SQL}, last_seen DESC NULLS LAST
             ) AS rn
             FROM monitored_hosts WHERE 1=1"
    ));
    push_filters(
        &mut count_builder,
        query.status.as_deref(),
        query.keyword.as_deref(),
    );
    count_builder.push(") SELECT COUNT(*) FROM dedup WHERE rn = 1");
    let total: i64 = count_builder
        .build_query_scalar()
        .persistent(false)
        .fetch_one(&pool)
        .await?;

    // NUMERIC 列统一 cast 为 float8 后按 f64 读取
    let mut rows_builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(format!(
        r"WITH dedup AS (
             SELECT id, device_id, status, source, hostname, label, ip, os, arch, agent_version,
                    cpu_usage, mem_usage_pct, disk_usage_pct, max_temp, uptime_secs, last_seen, created_at,
                    ROW_NUMBER() OVER (
                      PARTITION BY {DEDUP_PARTITION_SQL}
                      ORDER BY {SOURCE_RANK_SQL}, last_seen DESC NULLS LAST
                    ) AS rn
             FROM monitored_hosts WHERE 1=1"
    ));
    push_filters(
        &mut rows_builder,
        query.status.as_deref(),
        query.keyword.as_deref(),
    );
    rows_builder
        .push(
            r") SELECT id, device_id, status, source, hostname, label, ip, os, arch, agent_version,
                  cpu_usage::float8 AS cpu_usage,
                  mem_usage_pct::float8 AS mem_usage_pct,
                  disk_usage_pct::float8 AS disk_usage_pct,
                  max_temp::float8 AS max_temp,
                  uptime_secs, last_seen
             FROM dedup WHERE rn = 1
             ORDER BY last_seen DESC NULLS LAST, created_at DESC LIMIT ",
        )
        .push_bind(pagination.page_size)
        .push(" OFFSET ")
        .push_bind(pagination.offset);
    let rows = rows_builder
        .build()
        .persistent(false)
        .fetch_all(&pool)
        .await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Uuid, _>("id"),
                "device_id": row.get::<Option<Uuid>, _>("device_id"),
                "status": row.get::<String, _>("status"),
                "source": row.get::<String, _>("source"),
                "hostname": row.get::<Option<String>, _>("hostname"),
                "label": row.get::<Option<String>, _>("label"),
                "ip": row.get::<Option<String>, _>("ip"),
                "os": row.get::<Option<String>, _>("os"),
                "arch": row.get::<Option<String>, _>("arch"),
                "agent_version": row.get::<Option<String>, _>("agent_version"),
                "cpu_usage": row.get::<Option<f64>, _>("cpu_usage"),
                "mem_usage_pct": row.get::<Option<f64>, _>("mem_usage_pct"),
                "disk_usage_pct": row.get::<Option<f64>, _>("disk_usage_pct"),
                "max_temp": row.get::<Option<f64>, _>("max_temp"),
                "uptime_secs": row.get::<Option<i64>, _>("uptime_secs"),
                "last_seen": row.get::<Option<DateTime<Utc>>, _>("last_seen").map(rfc3339),
            })
        })
        .collect();
    Ok(foims_common::ok_json(
        paged_response(items, total, &pagination),
        msg("server.agent.list_retrieved"),
    ))
}

/// GET /api/agents/top-metrics 查询参数。
#[derive(Debug, Deserialize)]
pub struct TopMetricsQuery {
    /// 每类榜单条数（1..=10，默认 5）
    limit: Option<i64>,
}

/// GET /api/agents/top-metrics 支持的指标列白名单（列名来自代码常量，非用户输入）。
const TOP_METRIC_COLUMNS: &[(&str, &str)] = &[
    ("cpu", "cpu_usage"),
    ("memory", "mem_usage_pct"),
    ("temperature", "max_temp"),
];

/// GET /api/agents/top-metrics：主机负载 Top 榜（CPU/内存/温度，各取快照值
/// 最高的前 N 台，仅统计 active 且指标非空的行）。供仪表盘「主机负载」
/// 卡片消费，一次返回三类榜单避免三次往返。
pub async fn get_top_metrics<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
    Query(query): Query<TopMetricsQuery>,
) -> Result<Response, AppError> {
    let pool = state.pool()?.get_conn();
    let limit = query.limit.unwrap_or(5).clamp(1, 10);

    let mut body = serde_json::Map::new();
    for (key, column) in TOP_METRIC_COLUMNS {
        // 与列表同口径的 IP 去重（ipmi > agent > snmp）：先在 active 行内按 IP
        // 取 rank 最优行，再按指标非空排序取 Top N（IPMI 行为 rank 最优但
        // cpu/mem 无读数时自然不参与该类榜单）。
        let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(format!(
            r"WITH dedup AS (
                 SELECT id, hostname, label, ip, {column} AS metric, last_seen,
                        ROW_NUMBER() OVER (
                          PARTITION BY {DEDUP_PARTITION_SQL}
                          ORDER BY {SOURCE_RANK_SQL}, last_seen DESC NULLS LAST
                        ) AS rn
                 FROM monitored_hosts
                WHERE status = 'active'
               )
               SELECT id, hostname, label, ip, metric::float8 AS value, last_seen
                 FROM dedup
                WHERE rn = 1 AND metric IS NOT NULL
                ORDER BY metric DESC
                LIMIT "
        ));
        builder.push_bind(limit);
        let rows = builder.build().fetch_all(&pool).await?;

        let items: Vec<Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "hostname": row.get::<Option<String>, _>("hostname"),
                    "label": row.get::<Option<String>, _>("label"),
                    "ip": row.get::<Option<String>, _>("ip"),
                    "value": row.get::<Option<f64>, _>("value"),
                    "last_seen": row.get::<Option<DateTime<Utc>>, _>("last_seen").map(rfc3339),
                })
            })
            .collect();
        body.insert((*key).to_string(), Value::Array(items));
    }

    Ok(foims_common::ok_json(
        Value::Object(body),
        msg("server.agent.top_metrics_retrieved"),
    ))
}

/// GET /api/agents/{id}：详情（含 raw_metrics），不存在返回 404。
pub async fn get_agent_detail<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let pool = state.pool()?.get_conn();
    let row = sqlx::query(
        r#"SELECT id, machine_id, label, status, source, device_id, hostname, ip, os, kernel, arch,
                  agent_version,
                  cpu_usage::float8 AS cpu_usage,
                  mem_usage_pct::float8 AS mem_usage_pct,
                  disk_usage_pct::float8 AS disk_usage_pct,
                  max_temp::float8 AS max_temp,
                  uptime_secs, raw_metrics, first_seen, last_seen, created_at
             FROM monitored_hosts WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?;
    let Some(row) = row else {
        return Err(AppError::NotFound(msg("server.agent.not_found")));
    };

    let body = json!({
        "id": row.get::<Uuid, _>("id"),
        "machine_id": row.get::<Option<String>, _>("machine_id"),
        "label": row.get::<Option<String>, _>("label"),
        "status": row.get::<String, _>("status"),
        "source": row.get::<String, _>("source"),
        "device_id": row.get::<Option<Uuid>, _>("device_id"),
        "hostname": row.get::<Option<String>, _>("hostname"),
        "ip": row.get::<Option<String>, _>("ip"),
        "os": row.get::<Option<String>, _>("os"),
        "kernel": row.get::<Option<String>, _>("kernel"),
        "arch": row.get::<Option<String>, _>("arch"),
        "agent_version": row.get::<Option<String>, _>("agent_version"),
        "cpu_usage": row.get::<Option<f64>, _>("cpu_usage"),
        "mem_usage_pct": row.get::<Option<f64>, _>("mem_usage_pct"),
        "disk_usage_pct": row.get::<Option<f64>, _>("disk_usage_pct"),
        "max_temp": row.get::<Option<f64>, _>("max_temp"),
        "uptime_secs": row.get::<Option<i64>, _>("uptime_secs"),
        "raw_metrics": row.get::<Option<Value>, _>("raw_metrics"),
        "first_seen": row.get::<Option<DateTime<Utc>>, _>("first_seen").map(rfc3339),
        "last_seen": row.get::<Option<DateTime<Utc>>, _>("last_seen").map(rfc3339),
        "created_at": rfc3339(row.get::<DateTime<Utc>, _>("created_at")),
    });
    Ok(foims_common::ok_json(
        body,
        msg("server.agent.detail_retrieved"),
    ))
}

/// 取 `source='snmp'` 主机行对应的设备 id；Agent 主机或未关联设备返回 None。
async fn load_host_device_id(pool: &sqlx::PgPool, host_id: Uuid) -> Result<Option<Uuid>, AppError> {
    let row =
        sqlx::query("SELECT device_id FROM monitored_hosts WHERE id = $1 AND source = 'snmp'")
            .bind(host_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|row| row.get::<Option<Uuid>, _>("device_id")))
}

/// 设备端口选项与当前采集端口（`monitored` 行）。
async fn load_monitor_ports(
    pool: &sqlx::PgPool,
    device_id: Uuid,
) -> Result<(Option<String>, Vec<Value>), AppError> {
    let rows = sqlx::query(
        r#"SELECT name, status, speed, monitored
             FROM device_interfaces
            WHERE device_id = $1
            ORDER BY sort_order, name"#,
    )
    .bind(device_id)
    .fetch_all(pool)
    .await?;
    let mut current = None;
    let mut ports = Vec::with_capacity(rows.len());
    for row in rows {
        let name = row.get::<String, _>("name");
        let monitored = row.get::<bool, _>("monitored");
        if monitored {
            current = Some(name.clone());
        }
        ports.push(json!({
            "name": name,
            "status": row.get::<String, _>("status"),
            "speed": row.get::<Option<String>, _>("speed"),
            "monitored": monitored,
        }));
    }
    Ok((current, ports))
}

/// GET /api/agents/{id}/monitor-port：读取实时流量采集端口与可选端口清单。
pub async fn get_monitor_port<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let pool = state.pool()?.get_conn();
    let Some(device_id) = load_host_device_id(&pool, id).await? else {
        return Err(AppError::Validation(msg(
            "server.agent.monitor_port_agent_source",
        )));
    };
    let (if_name, ports) = load_monitor_ports(&pool, device_id).await?;
    Ok(foims_common::ok_json(
        json!({ "device_id": device_id, "if_name": if_name, "ports": ports }),
        msg("server.agent.monitor_port_retrieved"),
    ))
}

/// PUT /api/agents/{id}/monitor-port：设置实时流量采集端口（每设备最多一个）。
///
/// 只改 `device_interfaces.monitored`，不落指标库；实时采集 worker 在下一轮
/// 重载（≤60s）比对到端口变化后自行重建，无需重启服务。
pub async fn put_monitor_port<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<MonitorPortBody>,
) -> Result<Response, AppError> {
    let pool = state.pool()?.get_conn();
    let Some(device_id) = load_host_device_id(&pool, id).await? else {
        return Err(AppError::Validation(msg(
            "server.agent.monitor_port_agent_source",
        )));
    };
    let wanted = normalize_monitor_if(body.if_name.as_deref());
    if wanted
        .as_deref()
        .is_some_and(|name| name.chars().count() > MAX_INTERFACE_NAME_LEN)
    {
        return Err(AppError::Validation(msg(
            "server.agent.monitor_port_invalid",
        )));
    }
    if let Some(name) = wanted.as_deref() {
        let exists =
            sqlx::query("SELECT 1 FROM device_interfaces WHERE device_id = $1 AND name = $2")
                .bind(device_id)
                .bind(name)
                .fetch_optional(&pool)
                .await?
                .is_some();
        if !exists {
            return Err(AppError::Validation(msg(
                "server.agent.monitor_port_invalid",
            )));
        }
    }

    // 部分唯一索引 (device_id) WHERE monitored 不允许一条 UPDATE 同时置两行，
    // 故在同一事务内先清空旧采集端口再设置新端口
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE device_interfaces SET monitored = FALSE WHERE device_id = $1 AND monitored",
    )
    .bind(device_id)
    .execute(&mut *tx)
    .await?;
    if let Some(name) = wanted.as_deref() {
        sqlx::query(
            "UPDATE device_interfaces SET monitored = TRUE WHERE device_id = $1 AND name = $2",
        )
        .bind(device_id)
        .bind(name)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    let (if_name, ports) = load_monitor_ports(&pool, device_id).await?;
    Ok(foims_common::ok_json(
        json!({ "device_id": device_id, "if_name": if_name, "ports": ports }),
        msg("server.agent.monitor_port_saved"),
    ))
}

/// GET /api/agents/{id}/history?hours=24：曲线数据，hours 合法区间 1..=168，
/// 返回 {items:[{t,cpu,mem,disk,temp,rx_bps,tx_bps}]}（非分页，超出上限
/// 均匀抽样）。
pub async fn get_agent_history<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
    Path(id): Path<Uuid>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Response, AppError> {
    let hours = query
        .get("hours")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(24);
    if !(1..=168).contains(&hours) {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "hours (1-168)"),
        ));
    }
    let pool = state.pool()?.get_conn();
    let since = Utc::now() - chrono::Duration::hours(hours);
    // 抽样下推：窗口函数计算行号 rn 与总行数 n，超上限时按 rn % ⌈n/上限⌉ = 1
    // 在 SQL 内均匀抽取，避免 168h 最坏约 6 万行全量拉取后内存抽样
    //（rn/n 为 bigint；内插值仅为 HISTORY_MAX_POINTS 编译期常量，
    // 经 QueryBuilder push 拼接；id/时间走 push_bind）
    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        r#"SELECT collected_at, metrics FROM (
               SELECT collected_at, metrics,
                      row_number() OVER (ORDER BY collected_at) AS rn,
                      count(*) OVER () AS n
                 FROM host_metrics_history
                WHERE agent_id = "#,
    );
    qb.push_bind(id)
        .push(" AND collected_at >= ")
        .push_bind(since)
        .push(") sampled WHERE n <= ")
        .push(HISTORY_MAX_POINTS)
        .push(" OR rn % ceil(n::float / ")
        .push(HISTORY_MAX_POINTS)
        .push(")::int = 1 ORDER BY collected_at");
    let rows = qb
        .build_query_as::<(DateTime<Utc>, Value)>()
        .fetch_all(&pool)
        .await?;

    let points: Vec<Value> = rows
        .into_iter()
        .map(|(t, metrics)| {
            let pick = |key: &str| metrics.get(key).cloned().unwrap_or(Value::Null);
            json!({
                "t": rfc3339(t),
                "cpu": pick("cpu"),
                "mem": pick("mem"),
                "disk": pick("disk"),
                "temp": pick("temp"),
                "rx_bps": pick("rx_bps"),
                "tx_bps": pick("tx_bps"),
            })
        })
        .collect();
    Ok(foims_common::ok_json(
        json!({ "items": sample_series(points, HISTORY_MAX_POINTS) }),
        msg("server.agent.history_retrieved"),
    ))
}

/// PATCH /api/agents/{id}：更新备注（trim 后 ≤100 字符）与状态（白名单
/// active|disabled）；取值越界或两者皆缺返回 400，行不存在返回 404。
pub async fn patch_agent<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchAgentBody>,
) -> Result<Response, AppError> {
    let status = normalize_patch_status(body.status.as_deref())
        .map_err(|e| AppError::Validation(msg("server.common.invalid_param").with("param", e)))?;
    let label = body
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if let Some(label) = &label
        && label.chars().count() > 100
    {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "label (<=100)"),
        ));
    }
    if label.is_none() && status.is_none() {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "label/status"),
        ));
    }

    let pool = state.pool()?.get_conn();
    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE monitored_hosts SET ");
    {
        let mut separated = builder.separated(", ");
        if let Some(label) = &label {
            separated.push("label = ");
            separated.push_bind_unseparated(label.clone());
        }
        if let Some(status) = &status {
            separated.push("status = ");
            separated.push_bind_unseparated(status.clone());
        }
    }
    builder.push(" WHERE id = ").push_bind(id);
    builder.push(" RETURNING id, label, status");
    let updated: Option<(Uuid, Option<String>, String)> = builder
        .build_query_as()
        .persistent(false)
        .fetch_optional(&pool)
        .await?;
    let Some((id, label, status)) = updated else {
        return Err(AppError::NotFound(msg("server.agent.not_found")));
    };

    Ok((
        StatusCode::OK,
        Json(ApiResponse::success(
            json!({ "id": id, "label": label, "status": status }),
            msg("server.agent.updated"),
        )),
    )
        .into_response())
}

/// DELETE /api/agents/{id}：删除 Agent（history 经 FK CASCADE 级联删除），
/// 行不存在返回 404。
pub async fn delete_agent<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let pool = state.pool()?.get_conn();
    let result = sqlx::query("DELETE FROM monitored_hosts WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(msg("server.agent.not_found")));
    }
    Ok((
        StatusCode::OK,
        Json(ApiResponse::<()>::success((), msg("server.agent.deleted"))),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 采集端口名_空白视为未指定() {
        assert_eq!(normalize_monitor_if(None), None);
        assert_eq!(normalize_monitor_if(Some("   ")), None);
        assert_eq!(
            normalize_monitor_if(Some(" Gi0/1 ")),
            Some("Gi0/1".to_string())
        );
    }

    #[test]
    fn patch状态_白名单校验() {
        assert_eq!(normalize_patch_status(None), Ok(None), "缺省通过");
        assert_eq!(normalize_patch_status(Some("")), Ok(None), "空串视为缺省");
        assert_eq!(
            normalize_patch_status(Some(" active ")),
            Ok(Some("active".to_string())),
            "容忍首尾空白"
        );
        for ok in ["active", "disabled"] {
            assert_eq!(
                normalize_patch_status(Some(ok)),
                Ok(Some(ok.to_string())),
                "{ok} 应在白名单内"
            );
        }
        // offline 由上报自动翻转，未知值一律拒绝
        for bad in ["offline", "revoked", "pending", "unknown", "Active"] {
            assert!(normalize_patch_status(Some(bad)).is_err(), "{bad} 应被拒绝");
        }
    }

    #[test]
    fn 历史抽样_不超上限原样返回() {
        let mk = |i: usize| json!({ "t": i, "cpu": i });
        let points: Vec<Value> = (0..HISTORY_MAX_POINTS).map(mk).collect();
        let sampled = sample_series(points.clone(), HISTORY_MAX_POINTS);
        assert_eq!(sampled, points, "不超上限应原样返回");

        assert_eq!(
            sample_series(Vec::new(), HISTORY_MAX_POINTS),
            Vec::<Value>::new()
        );
        assert_eq!(sample_series(vec![mk(0)], HISTORY_MAX_POINTS).len(), 1);
    }

    #[test]
    fn 历史抽样_超上限均匀抽取且首尾保留() {
        let points: Vec<Value> = (0..1000).map(|i| json!({ "t": i, "cpu": i })).collect();
        let sampled = sample_series(points, HISTORY_MAX_POINTS);
        assert!(sampled.len() <= HISTORY_MAX_POINTS, "抽样点数不得超过上限");
        assert_eq!(sampled[0]["t"], 0, "首点恒保留");
        assert_eq!(sampled[sampled.len() - 1]["t"], 999, "末点恒保留");
        // 均匀性：除补齐的末点外，相邻抽样点原始索引差恒为步长
        let step = 999usize.div_ceil(HISTORY_MAX_POINTS - 1);
        let regular = &sampled[..sampled.len() - 1];
        for pair in regular.windows(2) {
            let a = pair[0]["t"].as_u64().unwrap_or(0) as usize;
            let b = pair[1]["t"].as_u64().unwrap_or(0) as usize;
            assert_eq!(b - a, step, "相邻点应按步长均匀分布");
        }
        // 补齐的末点与前一采样点间距小于步长（因末点恒保留）
        let tail_gap = 999 - regular[regular.len() - 1]["t"].as_u64().unwrap_or(0) as usize;
        assert!(tail_gap < step, "末点补齐间距应小于步长: {tail_gap}");
    }
}
