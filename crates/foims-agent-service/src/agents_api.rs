//! Agent 监控 Web API（设计 docs/agent-design.md §5.3）。
//!
//! 挂载于 /api/agents（与分发下载同 nest，经 agent_admin_guard 纵深防御）：
//! - `GET  /api/agents`：分页列表（keyword 匹配 hostname/label/ip，不含 raw_metrics）；
//! - `GET  /api/agents/{id}`：详情（含 raw_metrics）；
//! - `GET  /api/agents/{id}/history`：曲线数据（hours 1..=168，超 200 点均匀抽样）；
//! - `PATCH /api/agents/{id}`：改 label/status（status 白名单 active|disabled|revoked）；
//! - `DELETE /api/agents/{id}`：删除（history 经 FK CASCADE 级联清理）。

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

/// GET /api/agents 查询参数。
#[derive(Debug, Deserialize)]
pub struct ListAgentsQuery {
    page: Option<i64>,
    page_size: Option<i64>,
    /// 按状态精确过滤（pending/active/offline/disabled/revoked）
    status: Option<String>,
    /// 模糊匹配 hostname / label / ip
    keyword: Option<String>,
}

/// PATCH /api/agents/{id} 请求体。
#[derive(Debug, Deserialize)]
pub struct PatchAgentBody {
    /// 备注（trim 后 ≤100 字符）
    label: Option<String>,
    /// 目标状态：仅允许 active | disabled | revoked（pending 不可手工设置）
    status: Option<String>,
}

/// PATCH 状态白名单（pending 不可手工设置，离线/激活由上报自动翻转）
const PATCHABLE_STATUSES: &[&str] = &["active", "disabled", "revoked"];

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
fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

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
        // ILIKE 模糊匹配 hostname/label/ip，转义 %_\ 防通配符注入
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

/// GET /api/agents：分页列表（不含 raw_metrics，pending 空值行同样返回）。
pub async fn list_agents<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
    Query(query): Query<ListAgentsQuery>,
) -> Result<Response, AppError> {
    let pool = state.pool()?.get_conn();
    let pagination = Pagination::new(query.page.unwrap_or(1), query.page_size.unwrap_or(20));

    // 总数
    let mut count_builder =
        sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT COUNT(*) FROM agents WHERE 1=1");
    push_filters(
        &mut count_builder,
        query.status.as_deref(),
        query.keyword.as_deref(),
    );
    let total: i64 = count_builder.build_query_scalar().fetch_one(&pool).await?;

    // 列表行（NUMERIC 统一 cast float8 读取；不取 raw_metrics）
    let mut rows_builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        r#"SELECT id, status, hostname, label, ip, os, arch, agent_version,
                  cpu_usage::float8 AS cpu_usage,
                  mem_usage_pct::float8 AS mem_usage_pct,
                  disk_usage_pct::float8 AS disk_usage_pct,
                  max_temp::float8 AS max_temp,
                  uptime_secs, last_seen
             FROM agents WHERE 1=1"#,
    );
    push_filters(
        &mut rows_builder,
        query.status.as_deref(),
        query.keyword.as_deref(),
    );
    rows_builder
        .push(" ORDER BY last_seen DESC NULLS LAST, created_at DESC LIMIT ")
        .push_bind(pagination.page_size)
        .push(" OFFSET ")
        .push_bind(pagination.offset);
    let rows = rows_builder.build().fetch_all(&pool).await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Uuid, _>("id"),
                "status": row.get::<String, _>("status"),
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

/// GET /api/agents/{id}：详情（含 raw_metrics），不存在返回 404。
pub async fn get_agent_detail<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let pool = state.pool()?.get_conn();
    let row = sqlx::query(
        r#"SELECT id, machine_id, label, status, hostname, ip, os, kernel, arch,
                  agent_version,
                  cpu_usage::float8 AS cpu_usage,
                  mem_usage_pct::float8 AS mem_usage_pct,
                  disk_usage_pct::float8 AS disk_usage_pct,
                  max_temp::float8 AS max_temp,
                  uptime_secs, raw_metrics, first_seen, last_seen, created_at
             FROM agents WHERE id = $1"#,
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

/// GET /api/agents/{id}/history?hours=24：曲线数据，hours 合法区间 1..=168，
/// 返回 {items:[{t,cpu,mem,disk,temp}]}（非分页，超出上限均匀抽样）。
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
    //（rn/n 为 bigint，ceil(...)::int 为 int4，仅外层取 collected_at/metrics；
    // 内插值仅为 HISTORY_MAX_POINTS 编译期常量，无外部输入，id/时间走 bind）
    let sql = sqlx::AssertSqlSafe(format!(
        r#"SELECT collected_at, metrics FROM (
               SELECT collected_at, metrics,
                      row_number() OVER (ORDER BY collected_at) AS rn,
                      count(*) OVER () AS n
                 FROM agent_metrics_history
                WHERE agent_id = $1 AND collected_at >= $2
           ) sampled
         WHERE n <= {HISTORY_MAX_POINTS}
            OR rn % ceil(n::float / {HISTORY_MAX_POINTS})::int = 1
         ORDER BY collected_at"#,
    ));
    let rows = sqlx::query_as::<_, (DateTime<Utc>, Value)>(sql)
        .bind(id)
        .bind(since)
        .fetch_all(&pool)
        .await?;

    let points: Vec<Value> = rows
        .into_iter()
        .map(|(t, metrics)| {
            // 曲线精简快照键：cpu/mem/disk/temp（缺键时补 null）
            let pick = |key: &str| metrics.get(key).cloned().unwrap_or(Value::Null);
            json!({
                "t": rfc3339(t),
                "cpu": pick("cpu"),
                "mem": pick("mem"),
                "disk": pick("disk"),
                "temp": pick("temp"),
            })
        })
        .collect();
    Ok(foims_common::ok_json(
        json!({ "items": sample_series(points, HISTORY_MAX_POINTS) }),
        msg("server.agent.history_retrieved"),
    ))
}

/// PATCH /api/agents/{id}：更新备注与状态（status 白名单，
/// pending 不可手工设置），行不存在返回 404。
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
    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE agents SET ");
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
    let updated: Option<(Uuid, Option<String>, String)> =
        builder.build_query_as().fetch_optional(&pool).await?;
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
    let result = sqlx::query("DELETE FROM agents WHERE id = $1")
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
    fn patch状态_白名单校验() {
        assert_eq!(normalize_patch_status(None), Ok(None), "缺省通过");
        assert_eq!(normalize_patch_status(Some("")), Ok(None), "空串视为缺省");
        assert_eq!(
            normalize_patch_status(Some(" active ")),
            Ok(Some("active".to_string())),
            "容忍首尾空白"
        );
        for ok in ["active", "disabled", "revoked"] {
            assert_eq!(
                normalize_patch_status(Some(ok)),
                Ok(Some(ok.to_string())),
                "{ok} 应在白名单内"
            );
        }
        // pending 不可手工设置，未知值一律拒绝
        for bad in ["pending", "offline", "unknown", "Active"] {
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
