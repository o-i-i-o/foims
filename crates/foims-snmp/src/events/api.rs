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

use crate::core::normalize_mac;

use foims_common::log_info;

/// GET /api/snmp/events：分页查询观测事件（device_id/event_type/severity
/// 精确过滤，start/end 限定 occurred_at 闭区间，固定按 occurred_at 倒序；
/// 时间参数非 ISO-8601 即 400）。
pub async fn list_snmp_events<P: DbProvider>(
    State(state): State<Arc<P>>,
    _user: foims_auth::extractor::AuthUser,
    Query(query): Query<SnmpEventQuery>,
) -> Result<Response, AppError> {
    let pool = state.pool()?.get_conn();
    let pagination = Pagination::new(query.page.unwrap_or(1), query.page_size.unwrap_or(20));

    // 时间参数解析失败用专用 invalid_time 文案，与定位参数错误的 invalid_locator
    // 语义区分，前端据此提示不同字段
    let start: Option<DateTime<Utc>> = match query.start.as_deref() {
        Some(s) => Some(s.parse().map_err(|_| {
            AppError::Validation(msg("server.snmp.invalid_time").with("field", "start"))
        })?),
        None => None,
    };
    let end: Option<DateTime<Utc>> = match query.end.as_deref() {
        Some(s) => Some(s.parse().map_err(|_| {
            AppError::Validation(msg("server.snmp.invalid_time").with("field", "end"))
        })?),
        None => None,
    };

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

/// GET /api/snmp/locate 查询参数。
#[derive(Debug, serde::Deserialize)]
pub struct LocateQuery {
    pub mac: Option<String>,
    pub ip: Option<String>,
}

/// MAC 等值匹配的全部书写形态：把输入展开成入库常见形态后用 `= ANY(...)`
/// 做纯列等值匹配，列侧套函数会让索引失效。
#[must_use]
fn mac_match_forms(mac_norm: &str) -> Vec<String> {
    if mac_norm.is_empty() {
        return Vec::new();
    }
    let mut forms = vec![mac_norm.to_string(), mac_norm.to_lowercase()];
    if mac_norm.len() == 12 && mac_norm.chars().all(|c| c.is_ascii_hexdigit()) {
        let groups: Vec<&str> = mac_norm
            .as_bytes()
            .chunks(2)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect();
        for sep in [':', '-'] {
            let joined = groups.join(&sep.to_string());
            forms.push(joined.clone());
            forms.push(joined.to_lowercase());
        }
    }
    forms.sort();
    forms.dedup();
    forms
}

/// GET /api/snmp/locate?mac=...|ip=...：终端定位（mac 与 ip 至少给一个，
/// 全缺或 ip 非合法 IP 字面量即 400）。
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
    let mac_forms: Option<Vec<String>> = {
        let forms = mac_match_forms(&mac_norm);
        (!forms.is_empty()).then_some(forms)
    };
    let ip_bind: Option<&str> = ip.as_deref();

    // 两分支列宽归一后 UNION ALL 合并，static 分支无 ip/occurred_at 语义，补 NULL；
    // host() 仅作展示列，IP 匹配同样是 INET 纯列等值
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
                   WHERE ii.ip_address = a.ip_address
               ) AS registered,
               'arp' AS source,
               a.updated_at AS occurred_at,
               a.updated_at
          FROM snmp_arp_entries a
          JOIN devices d ON d.id = a.device_id
         WHERE ($1::text[] IS NOT NULL AND a.mac_address = ANY($1))
            OR ($2::text IS NOT NULL AND a.ip_address = CAST($2 AS INET))
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
         WHERE $1::text[] IS NOT NULL
           AND di.mac_address IS NOT NULL
           AND di.mac_address = ANY($1)
         ORDER BY device_name, interface
        "#,
    )
    .bind(&mac_forms)
    .bind(ip_bind)
    .fetch_all(&pool)
    .await?;

    let hit_count = rows.len();
    log_info!(
        "log.snmp.locate_query",
        mac = mac_norm.as_str(),
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
    fn mac_match_forms_covers_common_stored_shapes() {
        // 输入展开为纯/冒号/连字符 × 大小写形态，供列侧等值匹配
        let forms = mac_match_forms("AABBCCDDEEFF");
        assert!(forms.contains(&"AABBCCDDEEFF".to_string()));
        assert!(forms.contains(&"aabbccddeeff".to_string()));
        assert!(forms.contains(&"AA:BB:CC:DD:EE:FF".to_string()));
        assert!(forms.contains(&"aa:bb:cc:dd:ee:ff".to_string()));
        assert!(forms.contains(&"AA-BB-CC-DD-EE-FF".to_string()));
        // 非 12 位十六进制输入不拆分组，仅保留原形态；空串返回空集
        assert_eq!(
            mac_match_forms("XYZ"),
            vec!["XYZ".to_string(), "xyz".to_string()]
        );
        assert!(mac_match_forms("").is_empty());
    }
}
