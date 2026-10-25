//! SNMP 观测事件查询与终端定位 API（GET /api/snmp/events、/api/snmp/locate）。
//!
//! 依赖 foims-common::DbProvider 泛型状态；鉴权用 `AuthUser`（登录即可，
//! 均为库内只读查询、无外发探测，故不纳入 needs_admin 门槛）。

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::Response;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use foims_common::DbProvider;
use foims_common::pagination::{Pagination, paged_response};
use foims_common::{AppError, msg};
use foims_models::{DeviceLocateResult, SnmpEvent, SnmpEventQuery};

use foims_common::log_info;

/// GET /api/snmp/events：分页查询观测事件（按 occurred_at 倒序）。
pub async fn list_snmp_events<P: DbProvider>(
    State(state): State<Arc<P>>,
    _user: foims_auth::extractor::AuthUser,
    Query(query): Query<SnmpEventQuery>,
) -> Result<Response, AppError> {
    let pool = state.pool()?.get_conn();
    let pagination = Pagination::new(query.page.unwrap_or(1), query.page_size.unwrap_or(20));

    // 时间范围：ISO-8601 解析失败视为非法输入（早于查询返回校验错误）
    let start: Option<DateTime<Utc>> = match query.start.as_deref() {
        Some(s) => Some(s.parse().map_err(|_| {
            AppError::Validation(msg("server.snmp.invalid_locator").with("field", "start"))
        })?),
        None => None,
    };
    let end: Option<DateTime<Utc>> = match query.end.as_deref() {
        Some(s) => Some(s.parse().map_err(|_| {
            AppError::Validation(msg("server.snmp.invalid_locator").with("field", "end"))
        })?),
        None => None,
    };

    // 总数
    let mut count_builder =
        sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT COUNT(*) FROM snmp_events WHERE 1=1");
    push_event_filters(
        &mut count_builder,
        query.device_id,
        query.event_type.as_deref(),
        query.severity.as_deref(),
        start,
        end,
    );
    let total: i64 = count_builder
        .build_query_scalar()
        .persistent(false)
        .fetch_one(&pool)
        .await?;

    // 列表行
    let mut rows_builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        r"SELECT id, device_id, event_type, interface, severity, detail, occurred_at, created_at
           FROM snmp_events WHERE 1=1",
    );
    push_event_filters(
        &mut rows_builder,
        query.device_id,
        query.event_type.as_deref(),
        query.severity.as_deref(),
        start,
        end,
    );
    rows_builder
        .push(" ORDER BY occurred_at DESC LIMIT ")
        .push_bind(pagination.page_size)
        .push(" OFFSET ")
        .push_bind(pagination.offset);
    let items = rows_builder
        .build_query_as::<SnmpEvent>()
        .persistent(false)
        .fetch_all(&pool)
        .await?;

    Ok(foims_common::ok_json(
        paged_response(items, total, &pagination),
        msg("server.snmp.events_retrieved"),
    ))
}

/// 追加事件列表的动态过滤条件（列名来自代码常量，值经 push_bind 参数化）。
fn push_event_filters(
    builder: &mut sqlx::QueryBuilder<sqlx::Postgres>,
    device_id: Option<Uuid>,
    event_type: Option<&str>,
    severity: Option<&str>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) {
    if let Some(device_id) = device_id {
        builder.push(" AND device_id = ").push_bind(device_id);
    }
    if let Some(event_type) = event_type {
        builder.push(" AND event_type = ").push_bind(event_type);
    }
    if let Some(severity) = severity {
        builder.push(" AND severity = ").push_bind(severity);
    }
    if let Some(start) = start {
        builder.push(" AND occurred_at >= ").push_bind(start);
    }
    if let Some(end) = end {
        builder.push(" AND occurred_at <= ").push_bind(end);
    }
}

/// GET /api/snmp/locate 查询参数（mac 与 ip 至少给一个）。
#[derive(Debug, serde::Deserialize)]
pub struct LocateQuery {
    pub mac: Option<String>,
    pub ip: Option<String>,
}

/// 归一 MAC：剔除 `:`/`-`/空格并转大写，返回 12 位十六进制串（比较用，分隔符无关）。
fn normalize_mac(mac: &str) -> String {
    mac.chars()
        .filter(|c| !matches!(c, ':' | '-' | ' ' | '.'))
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// GET /api/snmp/locate?mac=...|ip=...：终端定位。
///
/// ARP/FDB 学习条目（source='arp'）与网口静态 MAC 直查（source='static'）并集，
/// `registered` 标记该 IP 是否已在 interface_ips 登记为资产。
pub async fn locate_device<P: DbProvider>(
    State(state): State<Arc<P>>,
    _user: foims_auth::extractor::AuthUser,
    Query(q): Query<LocateQuery>,
) -> Result<Response, AppError> {
    let pool = state.pool()?.get_conn();

    let mac_norm = q
        .mac
        .as_deref()
        .map(normalize_mac)
        .filter(|m| !m.is_empty());
    let ip =
        q.ip.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

    // 校验 IP 合法性（非法即拒，避免无意义查询）
    if let Some(ip) = &ip
        && ip.parse::<std::net::IpAddr>().is_err()
    {
        return Err(AppError::Validation(
            msg("server.snmp.invalid_locator").with("ip", ip),
        ));
    }

    if mac_norm.is_none() && ip.is_none() {
        return Err(AppError::Validation(msg("server.snmp.invalid_locator")));
    }

    let mac_norm = mac_norm.unwrap_or_default();
    let mac_bind: Option<&str> = if mac_norm.is_empty() {
        None
    } else {
        Some(&mac_norm)
    };
    let ip_bind: Option<&str> = ip.as_deref();

    // ARP 命中 + 网口静态 MAC 直查，UNION ALL 归一列宽
    let rows = sqlx::query_as::<_, DeviceLocateResult>(
        r#"
        SELECT a.device_id,
               d.name AS device_name,
               a.interface,
               a.vlan_id,
               host(a.ip_address) AS ip_address,
               a.mac_address,
               EXISTS (
                   SELECT 1 FROM interface_ips ii
                   WHERE host(ii.ip_address) = host(a.ip_address)
               ) AS registered,
               'arp' AS source,
               a.updated_at AS occurred_at,
               a.updated_at
          FROM snmp_arp_entries a
          JOIN devices d ON d.id = a.device_id
         WHERE ($1::text IS NOT NULL
                AND REPLACE(REPLACE(UPPER(a.mac_address), ':', ''), '-', '') = $1)
            OR ($2::text IS NOT NULL AND host(a.ip_address) = $2)
        UNION ALL
        SELECT di.device_id,
               d.name AS device_name,
               di.name AS interface,
               di.vlan_id,
               NULL::text AS ip_address,
               di.mac_address,
               TRUE AS registered,
               'static' AS source,
               NULL::timestamptz AS occurred_at,
               di.updated_at
          FROM device_interfaces di
          JOIN devices d ON d.id = di.device_id
         WHERE $1::text IS NOT NULL
           AND di.mac_address IS NOT NULL
           AND REPLACE(REPLACE(UPPER(di.mac_address), ':', ''), '-', '') = $1
         ORDER BY device_name, interface
        "#,
    )
    .bind(mac_bind)
    .bind(ip_bind)
    .fetch_all(&pool)
    .await?;

    let hit_count = rows.len();
    log_info!(
        "log.snmp.locate_query",
        mac = mac_bind.unwrap_or(""),
        ip = ip_bind.unwrap_or(""),
        hits = hit_count
    );

    Ok(foims_common::ok_json(
        rows,
        msg("server.snmp.located").with("count", hit_count),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_mac_strips_separators_and_uppercases() {
        assert_eq!(normalize_mac("aa:bb:cc:dd:ee:ff"), "AABBCCDDEEFF");
        assert_eq!(normalize_mac("AA-BB-CC-DD-EE-FF"), "AABBCCDDEEFF");
        assert_eq!(normalize_mac("aa.bb.cc.dd.ee.ff"), "AABBCCDDEEFF");
        assert_eq!(normalize_mac("  aabbccddeeFF "), "AABBCCDDEEFF");
    }
}
