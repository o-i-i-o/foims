//! 网络区域与网段管理。

use crate::app_state::AppState;
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::{Pagination, paged_response};
use crate::utils::parse_network_from_row;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use ipma_common::AppError;
use ipma_common::{log_error, log_info, msg};
use ipma_models::{
    Network, NetworkCreate, NetworkRegion, NetworkRegionCreate, NetworkRegionUpdate, NetworkUpdate,
};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

/// network_cidrs 唯一约束冲突（并发写入竞态触发 DB 兜底）→ 409，
/// 按约束名区分重名 / IPv4 / IPv6 网段冲突（db-schema-review R2）
fn map_network_unique_violation(e: sqlx::Error, name: &str) -> AppError {
    if let sqlx::Error::Database(db_err) = &e
        && db_err.is_unique_violation()
    {
        let constraint = db_err.constraint().unwrap_or_default();
        return if constraint.contains("ipv4") {
            AppError::Conflict(msg("server.network.ipv4_cidr_exists"))
        } else if constraint.contains("ipv6") {
            AppError::Conflict(msg("server.network.ipv6_cidr_exists"))
        } else {
            AppError::Conflict(msg("server.network.name_exists").with("name", name))
        };
    }
    AppError::from(e)
}

pub async fn get_networks(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();
    let region_id = query
        .get("region_id")
        .and_then(|id| Uuid::parse_str(id).ok());

    let name_filter = query.get("name").cloned().unwrap_or_default();
    let network_region_filter = query.get("network_region").cloned().unwrap_or_default();
    let ipv4_filter = query.get("ipv4_cidr").cloned().unwrap_or_default();
    let ipv6_filter = query.get("ipv6_cidr").cloned().unwrap_or_default();

    let sort_by = query.get("sort_by").cloned().unwrap_or_default();
    let sort_order = query.get("sort_order").cloned().unwrap_or_default();

    // ORDER BY 白名单，未匹配时回落默认序，避免注入
    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY n.name DESC",
        ("name", _) => "ORDER BY n.name ASC",
        ("network_region", "desc") => "ORDER BY nt.name DESC, n.name ASC",
        ("network_region", _) => "ORDER BY nt.name ASC, n.name ASC",
        ("ipv4_cidr", "desc") => "ORDER BY n.ipv4_cidr DESC",
        ("ipv4_cidr", _) => "ORDER BY n.ipv4_cidr ASC",
        ("ipv6_cidr", "desc") => "ORDER BY n.ipv6_cidr DESC",
        ("ipv6_cidr", _) => "ORDER BY n.ipv6_cidr ASC",
        ("created_at", "asc") => "ORDER BY n.created_at ASC",
        _ => "ORDER BY n.created_at DESC",
    };

    let has_filters = !search.is_empty()
        || region_id.is_some()
        || !name_filter.is_empty()
        || !network_region_filter.is_empty()
        || !ipv4_filter.is_empty()
        || !ipv6_filter.is_empty();

    let total: i64 = if has_filters {
        let mut conditions = Vec::new();
        let mut param_count = 1;

        if !search.is_empty() {
            conditions.push(format!("(n.name ILIKE ${param_count} OR n.description ILIKE ${param_count} OR n.ipv4_cidr::TEXT ILIKE ${param_count} OR n.ipv6_cidr::TEXT ILIKE ${param_count})"));
            param_count += 1;
        }

        if let Some(_rid) = region_id {
            conditions.push(format!("n.network_region_id = ${param_count}"));
            param_count += 1;
        }

        if !name_filter.is_empty() {
            conditions.push(format!("n.name ILIKE ${param_count}"));
            param_count += 1;
        }

        if !network_region_filter.is_empty() {
            conditions.push(format!("nt.name ILIKE ${param_count}"));
            param_count += 1;
        }

        if !ipv4_filter.is_empty() {
            conditions.push(format!("n.ipv4_cidr::TEXT ILIKE ${param_count}"));
            param_count += 1;
        }

        if !ipv6_filter.is_empty() {
            conditions.push(format!("n.ipv6_cidr::TEXT ILIKE ${param_count}"));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let count_query = format!(
            "SELECT COUNT(*) FROM network_cidrs n JOIN network_regions nt ON n.network_region_id = nt.id {where_clause}"
        );

        let mut count_sql = sqlx::query_scalar(sqlx::AssertSqlSafe(count_query));

        if !search.is_empty() {
            let pattern = crate::utils::escape_like(&search);
            count_sql = count_sql.bind(pattern);
        }

        if let Some(rid) = region_id {
            count_sql = count_sql.bind(rid);
        }

        if !name_filter.is_empty() {
            let pattern = crate::utils::escape_like(&name_filter);
            count_sql = count_sql.bind(pattern);
        }

        if !network_region_filter.is_empty() {
            let pattern = crate::utils::escape_like(&network_region_filter);
            count_sql = count_sql.bind(pattern);
        }

        if !ipv4_filter.is_empty() {
            let pattern = crate::utils::escape_like(&ipv4_filter);
            count_sql = count_sql.bind(pattern);
        }

        if !ipv6_filter.is_empty() {
            let pattern = crate::utils::escape_like(&ipv6_filter);
            count_sql = count_sql.bind(pattern);
        }

        count_sql.fetch_one(&state.pool()?.get_conn()).await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM network_cidrs")
            .fetch_one(&state.pool()?.get_conn())
            .await?
    };

    let networks: Vec<Network> = if has_filters {
        let mut conditions = Vec::new();
        let mut param_count = 1;

        if !search.is_empty() {
            conditions.push(format!("(n.name ILIKE ${param_count} OR n.description ILIKE ${param_count} OR n.ipv4_cidr::TEXT ILIKE ${param_count} OR n.ipv6_cidr::TEXT ILIKE ${param_count})"));
            param_count += 1;
        }

        if let Some(_rid) = region_id {
            conditions.push(format!("n.network_region_id = ${param_count}"));
            param_count += 1;
        }

        if !name_filter.is_empty() {
            conditions.push(format!("n.name ILIKE ${param_count}"));
            param_count += 1;
        }

        if !network_region_filter.is_empty() {
            conditions.push(format!("nt.name ILIKE ${param_count}"));
            param_count += 1;
        }

        if !ipv4_filter.is_empty() {
            conditions.push(format!("n.ipv4_cidr::TEXT ILIKE ${param_count}"));
            param_count += 1;
        }

        if !ipv6_filter.is_empty() {
            conditions.push(format!("n.ipv6_cidr::TEXT ILIKE ${param_count}"));
            param_count += 1;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let data_query = format!(
            r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, host(n.ipv4_gateway), host(n.ipv6_gateway),
               (SELECT json_agg(host(d)) FROM unnest(n.ipv4_dns) AS d) as ipv4_dns,
               (SELECT json_agg(host(d)) FROM unnest(n.ipv6_dns) AS d) as ipv6_dns,
               n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ
               FROM network_cidrs n
               JOIN network_regions nt ON n.network_region_id = nt.id
               {}
               {order_clause}
               LIMIT ${} OFFSET ${}",
            where_clause,
            param_count,
            param_count + 1
        );

        let mut data_sql = sqlx::query(sqlx::AssertSqlSafe(data_query));

        if !search.is_empty() {
            let pattern = crate::utils::escape_like(&search);
            data_sql = data_sql.bind(pattern);
        }

        if let Some(rid) = region_id {
            data_sql = data_sql.bind(rid);
        }

        if !name_filter.is_empty() {
            let pattern = crate::utils::escape_like(&name_filter);
            data_sql = data_sql.bind(pattern);
        }

        if !network_region_filter.is_empty() {
            let pattern = crate::utils::escape_like(&network_region_filter);
            data_sql = data_sql.bind(pattern);
        }

        if !ipv4_filter.is_empty() {
            let pattern = crate::utils::escape_like(&ipv4_filter);
            data_sql = data_sql.bind(pattern);
        }

        if !ipv6_filter.is_empty() {
            let pattern = crate::utils::escape_like(&ipv6_filter);
            data_sql = data_sql.bind(pattern);
        }

        data_sql = data_sql.bind(page_size).bind(offset);

        data_sql
            .fetch_all(&state.pool()?.get_conn())
            .await?
            .into_iter()
            .map(|row| parse_network_from_row(&row))
            .collect::<Result<_, _>>()?
    } else {
        let data_query = format!(
            r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, host(n.ipv4_gateway), host(n.ipv6_gateway),
               (SELECT json_agg(host(d)) FROM unnest(n.ipv4_dns) AS d) as ipv4_dns,
               (SELECT json_agg(host(d)) FROM unnest(n.ipv6_dns) AS d) as ipv6_dns,
               n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ
               FROM network_cidrs n
               JOIN network_regions nt ON n.network_region_id = nt.id
               {order_clause}
               LIMIT $1 OFFSET $2"
        );

        sqlx::query(sqlx::AssertSqlSafe(data_query))
            .bind(page_size)
            .bind(offset)
            .fetch_all(&state.pool()?.get_conn())
            .await?
            .into_iter()
            .map(|row| parse_network_from_row(&row))
            .collect::<Result<_, _>>()?
    };

    Ok(ipma_common::ok_json(
        paged_response(networks, total, &pagination),
        "server.network.fetched",
    ))
}

pub async fn create_network(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<NetworkCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let network_region = sqlx::query_as::<_, NetworkRegion>(
        "SELECT id, name, description,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv4_cidrs) AS d) as ipv4_cidrs,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv6_cidrs) AS d) as ipv6_cidrs,
                created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions WHERE id = $1"
    ).bind(req.network_region_id)
    .fetch_optional(&state.pool()?.get_conn()).await?
    .ok_or_else(|| AppError::NotFound(msg("server.network.region_not_found")))?;

    let full_network_name = req.name.clone();

    let existing_network = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM network_cidrs WHERE name = $1 AND network_region_id = $2",
    )
    .bind(&full_network_name)
    .bind(req.network_region_id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?;

    if existing_network.is_some() {
        return Err(AppError::Conflict(msg("server.network.name_exists")));
    }

    let mut ipv4_cidr_val: Option<String> = None;
    let mut ipv6_cidr_val: Option<String> = None;
    let mut has_valid_cidr = false;

    if let Some(ipv4_cidr) = &req.ipv4_cidr {
        if crate::utils::validate_cidr(ipv4_cidr)
            && crate::utils::get_cidr_type(ipv4_cidr) == Some("ipv4")
        {
            ipv4_cidr_val = Some(ipv4_cidr.clone());
            has_valid_cidr = true;
        } else {
            return Err(AppError::Validation(msg(
                "server.network.ipv4_cidr_invalid",
            )));
        }
    }

    if let Some(ipv6_cidr) = &req.ipv6_cidr {
        if crate::utils::validate_cidr(ipv6_cidr)
            && crate::utils::get_cidr_type(ipv6_cidr) == Some("ipv6")
        {
            ipv6_cidr_val = Some(ipv6_cidr.clone());
            has_valid_cidr = true;
        } else {
            return Err(AppError::Validation(msg(
                "server.network.ipv6_cidr_invalid",
            )));
        }
    }

    if !has_valid_cidr {
        return Err(AppError::Validation(msg("server.network.cidr_required")));
    }

    // 校验网关格式及其是否落在对应 CIDR 网段内
    crate::utils::validate_gateway_in_cidr(
        req.ipv4_gateway.as_deref(),
        ipv4_cidr_val.as_deref(),
        "ipv4",
    )?;
    crate::utils::validate_gateway_in_cidr(
        req.ipv6_gateway.as_deref(),
        ipv6_cidr_val.as_deref(),
        "ipv6",
    )?;

    // 校验网段 CIDR 是否属于所在区域的 CIDR 范围
    if let Some(ref ipv4) = ipv4_cidr_val
        && !crate::utils::cidr_belongs_to_region(
            ipv4,
            network_region.ipv4_cidrs.as_deref().unwrap_or_default(),
        )
    {
        return Err(AppError::Validation(msg(
            "server.network.ipv4_not_in_region",
        )));
    }

    if let Some(ref ipv6) = ipv6_cidr_val
        && !crate::utils::cidr_belongs_to_region(
            ipv6,
            network_region.ipv6_cidrs.as_deref().unwrap_or_default(),
        )
    {
        return Err(AppError::Validation(msg(
            "server.network.ipv6_not_in_region",
        )));
    }

    if let Some(ref ipv4) = ipv4_cidr_val {
        let existing_ipv4: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM network_cidrs WHERE ipv4_cidr = CAST($1 AS CIDR)")
                .bind(ipv4)
                .fetch_optional(&state.pool()?.get_conn())
                .await
                .map_err(|e| {
                    log_error!("log.network.check_ipv4_duplicate_failed", error = e);
                    AppError::Database(msg("server.network.check_ipv4_duplicate_failed"))
                })?;

        if existing_ipv4.is_some() {
            return Err(AppError::Conflict(msg("server.network.ipv4_cidr_exists")));
        }
    }

    if let Some(ref ipv6) = ipv6_cidr_val {
        let existing_ipv6: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM network_cidrs WHERE ipv6_cidr = CAST($1 AS CIDR)")
                .bind(ipv6)
                .fetch_optional(&state.pool()?.get_conn())
                .await
                .map_err(|e| {
                    log_error!("log.network.check_ipv6_duplicate_failed", error = e);
                    AppError::Database(msg("server.network.check_ipv6_duplicate_failed"))
                })?;

        if existing_ipv6.is_some() {
            return Err(AppError::Conflict(msg("server.network.ipv6_cidr_exists")));
        }
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO network_cidrs (id, name, network_region_id, ipv4_cidr, ipv6_cidr, ipv4_gateway, ipv6_gateway, ipv4_dns, ipv6_dns, description, created_at, updated_at)
         VALUES ($1, $2, $3, CAST($4 AS CIDR), CAST($5 AS CIDR), CAST($6 AS INET), CAST($7 AS INET), $8::INET[], $9::INET[], $10, $11, $12)"
    )
    .bind(id)
    .bind(&full_network_name)
    .bind(req.network_region_id)
    .bind(&ipv4_cidr_val)
    .bind(&ipv6_cidr_val)
    .bind(&req.ipv4_gateway)
    .bind(&req.ipv6_gateway)
    .bind(&req.ipv4_dns)
    .bind(&req.ipv6_dns)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&state.pool()?.get_conn())
    .await
    .map_err(|e| map_network_unique_violation(e, &full_network_name))?;

    let details = serde_json::json!({
        "name": full_network_name.clone(),
        "network_region": network_region.name.clone(),
        "ipv4_cidr": ipv4_cidr_val,
        "ipv6_cidr": ipv6_cidr_val
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "network",
        Some(&id),
        &details,
    )
    .await;
    log_info!("log.network.created", name = full_network_name, id = id);

    // req 在此之后不再使用，字段直接移动
    let network = Network {
        id,
        name: full_network_name,
        network_region_id: req.network_region_id,
        network_region: network_region.name,
        ipv4_cidr: ipv4_cidr_val,
        ipv6_cidr: ipv6_cidr_val,
        ipv4_gateway: req.ipv4_gateway,
        ipv6_gateway: req.ipv6_gateway,
        ipv4_dns: req.ipv4_dns,
        ipv6_dns: req.ipv6_dns,
        description: req.description,
        created_at: now,
        updated_at: now,
    };

    Ok(ipma_common::ok_json(network, "server.network.created"))
}

pub async fn get_network(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let row = sqlx::query(
        r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, 
                  n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, 
                  host(n.ipv4_gateway), host(n.ipv6_gateway), 
                  (SELECT json_agg(host(d)) FROM unnest(n.ipv4_dns) AS d) as ipv4_dns,
                  (SELECT json_agg(host(d)) FROM unnest(n.ipv6_dns) AS d) as ipv6_dns,
                  n.description, 
                  n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
           FROM network_cidrs n 
           JOIN network_regions nt ON n.network_region_id = nt.id 
           WHERE n.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.network.not_found")))?;

    let network = parse_network_from_row(&row)?;

    Ok(ipma_common::ok_json(network, "server.network.fetched"))
}

pub async fn update_network(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<NetworkUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let existing_network =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_cidrs WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool()?.get_conn())
            .await?;

    if existing_network.is_none() {
        return Err(AppError::NotFound(msg("server.network.not_found")));
    }

    if let Some(network_region_id) = &req.network_region_id {
        let network_region = sqlx::query_as::<_, NetworkRegion>(
            "SELECT id, name, description,
                    (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv4_cidrs) AS d) as ipv4_cidrs,
                    (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv6_cidrs) AS d) as ipv6_cidrs,
                    created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions WHERE id = $1"
        )
        .bind(network_region_id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

        if network_region.is_none() {
            return Err(AppError::NotFound(msg("server.network.region_not_found")));
        }
    }

    let now = Utc::now();

    let row = sqlx::query(
        r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, host(n.ipv4_gateway), host(n.ipv6_gateway),
           (SELECT json_agg(host(d)) FROM unnest(n.ipv4_dns) AS d) as ipv4_dns,
           (SELECT json_agg(host(d)) FROM unnest(n.ipv6_dns) AS d) as ipv6_dns,
           n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ
           FROM network_cidrs n
           JOIN network_regions nt ON n.network_region_id = nt.id
           WHERE n.id = $1"
    ).bind(id)
    .fetch_one(&state.pool()?.get_conn()).await?;
    let current_network = parse_network_from_row(&row)?;

    let full_network_name = match &req.name {
        Some(new_name) => new_name.clone(),
        None => current_network.name.clone(),
    };

    let network_region_id = match &req.network_region_id {
        Some(region_id) => region_id,
        None => &current_network.network_region_id,
    };

    // Get the target network region for CIDR validation
    let target_network_region = sqlx::query_as::<_, NetworkRegion>(
        "SELECT id, name, description,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv4_cidrs) AS d) as ipv4_cidrs,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv6_cidrs) AS d) as ipv6_cidrs,
                created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions WHERE id = $1"
    )
    .bind(*network_region_id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let existing_network = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM network_cidrs WHERE name = $1 AND network_region_id = $2 AND id != $3",
    )
    .bind(&full_network_name)
    .bind(network_region_id)
    .bind(id)
    .fetch_optional(&state.pool()?.get_conn())
    .await?;

    if existing_network.is_some() {
        return Err(AppError::Conflict(msg("server.network.name_exists")));
    }

    // 校验 CIDR 格式并检查重复
    if let Some(ref ipv4) = req.ipv4_cidr {
        if !crate::utils::validate_cidr(ipv4) || crate::utils::get_cidr_type(ipv4) != Some("ipv4") {
            return Err(AppError::Validation(msg(
                "server.network.ipv4_cidr_invalid",
            )));
        }

        let existing_ipv4: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM network_cidrs WHERE ipv4_cidr = CAST($1 AS CIDR) AND id != $2",
        )
        .bind(ipv4)
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await
        .map_err(|e| {
            log_error!("log.network.check_ipv4_duplicate_failed", error = e);
            AppError::Database(msg("server.network.check_ipv4_duplicate_failed"))
        })?;

        if existing_ipv4.is_some() {
            return Err(AppError::Conflict(msg("server.network.ipv4_cidr_in_use")));
        }

        // 校验 IPv4 CIDR 是否属于所在区域的 CIDR 范围
        if !crate::utils::cidr_belongs_to_region(
            ipv4,
            target_network_region
                .ipv4_cidrs
                .as_deref()
                .unwrap_or_default(),
        ) {
            return Err(AppError::Validation(msg(
                "server.network.ipv4_not_in_region",
            )));
        }
    }

    if let Some(ref ipv6) = req.ipv6_cidr {
        if !crate::utils::validate_cidr(ipv6) || crate::utils::get_cidr_type(ipv6) != Some("ipv6") {
            return Err(AppError::Validation(msg(
                "server.network.ipv6_cidr_invalid",
            )));
        }

        let existing_ipv6: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM network_cidrs WHERE ipv6_cidr = CAST($1 AS CIDR) AND id != $2",
        )
        .bind(ipv6)
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await
        .map_err(|e| {
            log_error!("log.network.check_ipv6_duplicate_failed", error = e);
            AppError::Database(msg("server.network.check_ipv6_duplicate_failed"))
        })?;

        if existing_ipv6.is_some() {
            return Err(AppError::Conflict(msg("server.network.ipv6_cidr_in_use")));
        }

        // 校验 IPv6 CIDR 是否属于所在区域的 CIDR 范围
        if !crate::utils::cidr_belongs_to_region(
            ipv6,
            target_network_region
                .ipv6_cidrs
                .as_deref()
                .unwrap_or_default(),
        ) {
            return Err(AppError::Validation(msg(
                "server.network.ipv6_not_in_region",
            )));
        }
    }

    // 网关校验：以请求值（缺省回退库中现值）与最终生效的 CIDR 核对，
    // 确保"只改 CIDR 不改网关"等部分更新后的数据仍保持一致
    let effective_ipv4_cidr = req
        .ipv4_cidr
        .as_ref()
        .or(current_network.ipv4_cidr.as_ref())
        .map(String::as_str);
    let effective_ipv4_gateway = req
        .ipv4_gateway
        .as_ref()
        .or(current_network.ipv4_gateway.as_ref())
        .map(String::as_str);
    crate::utils::validate_gateway_in_cidr(effective_ipv4_gateway, effective_ipv4_cidr, "ipv4")?;
    let effective_ipv6_cidr = req
        .ipv6_cidr
        .as_ref()
        .or(current_network.ipv6_cidr.as_ref())
        .map(String::as_str);
    let effective_ipv6_gateway = req
        .ipv6_gateway
        .as_ref()
        .or(current_network.ipv6_gateway.as_ref())
        .map(String::as_str);
    crate::utils::validate_gateway_in_cidr(effective_ipv6_gateway, effective_ipv6_cidr, "ipv6")?;

    sqlx::query(
        "UPDATE network_cidrs SET 
         name = $1, 
         network_region_id = COALESCE($2, network_region_id),
         ipv4_cidr = COALESCE(CAST($3 AS CIDR), ipv4_cidr),
         ipv6_cidr = COALESCE(CAST($4 AS CIDR), ipv6_cidr),
         ipv4_gateway = COALESCE(CAST($5 AS INET), ipv4_gateway),
         ipv6_gateway = COALESCE(CAST($6 AS INET), ipv6_gateway),
         ipv4_dns = COALESCE($7::INET[], ipv4_dns),
         ipv6_dns = COALESCE($8::INET[], ipv6_dns),
         description = COALESCE($9, description), 
         updated_at = $10 
         WHERE id = $11",
    )
    .bind(&full_network_name)
    .bind(req.network_region_id.as_ref())
    .bind(&req.ipv4_cidr)
    .bind(&req.ipv6_cidr)
    .bind(&req.ipv4_gateway)
    .bind(&req.ipv6_gateway)
    .bind(&req.ipv4_dns)
    .bind(&req.ipv6_dns)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&state.pool()?.get_conn())
    .await
    .map_err(|e| map_network_unique_violation(e, &full_network_name))?;

    let row = sqlx::query(
        r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, host(n.ipv4_gateway), host(n.ipv6_gateway), 
           (SELECT json_agg(host(d)) FROM unnest(n.ipv4_dns) AS d) as ipv4_dns,
           (SELECT json_agg(host(d)) FROM unnest(n.ipv6_dns) AS d) as ipv6_dns,
           n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
           FROM network_cidrs n 
           JOIN network_regions nt ON n.network_region_id = nt.id 
           WHERE n.id = $1"
    ).bind(id)
    .fetch_one(&state.pool()?.get_conn()).await?;

    let network = parse_network_from_row(&row)?;

    let details = serde_json::json!({
        "name": network.name,
        "network_region": network.network_region,
        "ipv4_cidr": network.ipv4_cidr,
        "ipv6_cidr": network.ipv6_cidr
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "network",
        Some(&id),
        &details,
    )
    .await;
    log_info!("log.network.updated", name = network.name, id = id);

    Ok(ipma_common::ok_json(network, "server.network.updated"))
}

pub async fn delete_network(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let existing_network =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_cidrs WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool()?.get_conn())
            .await?;

    if existing_network.is_none() {
        return Err(AppError::NotFound(msg("server.network.not_found")));
    }

    let ip_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ips WHERE network_id = $1")
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    if ip_count > 0 {
        return Err(AppError::Validation(msg("server.network.in_use_by_ip")));
    }

    let room_network_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM room_networks WHERE network_id = $1")
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    if room_network_count > 0 {
        return Err(AppError::Validation(msg("server.network.in_use_by_room")));
    }

    let cabinet_network_count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM cabinets c JOIN room_networks rn ON c.room_id = rn.room_id WHERE rn.network_id = $1"
    )
        .bind(id)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    if cabinet_network_count > 0 {
        return Err(AppError::Validation(msg(
            "server.network.in_use_by_cabinet",
        )));
    }

    sqlx::query("DELETE FROM network_cidrs WHERE id = $1")
        .bind(id)
        .execute(&state.pool()?.get_conn())
        .await?;

    let details = serde_json::json!({
        "network_id": id.to_string()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "network",
        Some(&id),
        &details,
    )
    .await;
    log_info!("log.network.deleted", id = id);

    Ok(ipma_common::ok_json((), "server.network.deleted"))
}

pub async fn get_network_regions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();

    let sort_by = query.get("sort_by").cloned().unwrap_or_default();
    let sort_order = query.get("sort_order").cloned().unwrap_or_default();

    // ORDER BY 白名单，未匹配时回落默认序，避免注入。
    // CIDR 列取数组首元素参与排序，空数组（NULL）固定排末尾
    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY name DESC",
        ("name", _) => "ORDER BY name ASC",
        ("ipv4_cidrs", "desc") => "ORDER BY ipv4_cidrs[1] DESC NULLS LAST",
        ("ipv4_cidrs", _) => "ORDER BY ipv4_cidrs[1] ASC NULLS LAST",
        ("ipv6_cidrs", "desc") => "ORDER BY ipv6_cidrs[1] DESC NULLS LAST",
        ("ipv6_cidrs", _) => "ORDER BY ipv6_cidrs[1] ASC NULLS LAST",
        ("created_at", "asc") => "ORDER BY created_at ASC",
        _ => "ORDER BY created_at DESC",
    };

    let total: i64 = if search.is_empty() {
        sqlx::query_scalar("SELECT COUNT(*) FROM network_regions")
            .fetch_one(&state.pool()?.get_conn())
            .await?
    } else {
        let pattern = crate::utils::escape_like(&search);
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM network_regions WHERE name ILIKE $1 OR description ILIKE $1",
        )
        .bind(&pattern)
        .fetch_one(&state.pool()?.get_conn())
        .await?
    };

    let base_select = "SELECT id, name, description,
                    (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv4_cidrs) AS d) as ipv4_cidrs,
                    (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv6_cidrs) AS d) as ipv6_cidrs,
                    created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions";

    let network_regions = if search.is_empty() {
        let sql = format!("{base_select} {order_clause} LIMIT $1 OFFSET $2");
        sqlx::query_as::<_, NetworkRegion>(sqlx::AssertSqlSafe(sql))
            .bind(page_size)
            .bind(offset)
            .fetch_all(&state.pool()?.get_conn())
            .await?
    } else {
        let pattern = crate::utils::escape_like(&search);
        let sql = format!(
            "{base_select} WHERE name ILIKE $1 OR description ILIKE $1 {order_clause} LIMIT $2 OFFSET $3"
        );
        sqlx::query_as::<_, NetworkRegion>(sqlx::AssertSqlSafe(sql))
            .bind(&pattern)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&state.pool()?.get_conn())
            .await?
    };

    Ok(ipma_common::ok_json(
        paged_response(network_regions, total, &pagination),
        "server.network.region_fetched",
    ))
}

pub async fn create_network_region(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<NetworkRegionCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_regions WHERE name = $1")
            .bind(&req.name)
            .fetch_optional(&state.pool()?.get_conn())
            .await?;

    if existing.is_some() {
        return Err(AppError::Conflict(msg("server.network.region_name_exists")));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO network_regions (id, name, description, ipv4_cidrs, ipv6_cidrs, created_at, updated_at) 
         VALUES ($1, $2, $3, $4::CIDR[], $5::CIDR[], $6, $7)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.ipv4_cidrs)
    .bind(&req.ipv6_cidrs)
    .bind(now)
    .bind(now)
    .execute(&state.pool()?.get_conn())
    .await?;

    let network_region = NetworkRegion {
        id,
        name: req.name.clone(),
        description: req.description.clone(),
        ipv4_cidrs: req.ipv4_cidrs.clone(),
        ipv6_cidrs: req.ipv6_cidrs.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": network_region.name,
        "description": network_region.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "network_region",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json(
        network_region,
        "server.network.region_created",
    ))
}

pub async fn get_network_region(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let network_region = sqlx::query_as::<_, NetworkRegion>(
        "SELECT id, name, description,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv4_cidrs) AS d) as ipv4_cidrs,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv6_cidrs) AS d) as ipv6_cidrs,
                created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions WHERE id = $1"
    ).bind(id)
    .fetch_optional(&state.pool()?.get_conn()).await?
    .ok_or_else(|| AppError::NotFound(msg("server.network.region_not_found")))?;

    Ok(ipma_common::ok_json(
        network_region,
        "server.network.region_fetched",
    ))
}

pub async fn update_network_region(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<NetworkRegionUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_regions WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool()?.get_conn())
            .await?;

    if existing.is_none() {
        return Err(AppError::NotFound(msg("server.network.region_not_found")));
    }

    if let Some(name) = &req.name {
        let duplicate: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM network_regions WHERE name = $1 AND id != $2",
        )
        .bind(name)
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

        if duplicate.is_some() {
            return Err(AppError::Conflict(msg("server.network.region_name_exists")));
        }
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE network_regions SET
         name = COALESCE($1, name),
         description = COALESCE($2, description),
         ipv4_cidrs = COALESCE($3::CIDR[], ipv4_cidrs),
         ipv6_cidrs = COALESCE($4::CIDR[], ipv6_cidrs),
         updated_at = $5
         WHERE id = $6",
    )
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.ipv4_cidrs)
    .bind(&req.ipv6_cidrs)
    .bind(now)
    .bind(id)
    .execute(&state.pool()?.get_conn())
    .await?;

    let network_region = sqlx::query_as::<_, NetworkRegion>(
        "SELECT id, name, description,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv4_cidrs) AS d) as ipv4_cidrs,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv6_cidrs) AS d) as ipv6_cidrs,
                created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions WHERE id = $1"
    ).bind(id)
    .fetch_one(&state.pool()?.get_conn()).await?;

    let details = serde_json::json!({
        "name": network_region.name,
        "description": network_region.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "network_region",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json(
        network_region,
        "server.network.region_updated",
    ))
}

pub async fn delete_network_region(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_regions WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool()?.get_conn())
            .await?;

    if existing.is_none() {
        return Err(AppError::NotFound(msg("server.network.region_not_found")));
    }

    let network_count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM network_cidrs WHERE network_region_id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    if network_count > 0 {
        return Err(AppError::Validation(msg("server.network.region_in_use")));
    }

    sqlx::query("DELETE FROM network_regions WHERE id = $1")
        .bind(id)
        .execute(&state.pool()?.get_conn())
        .await?;

    let details = serde_json::json!({
        "network_region_id": id.to_string()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "network_region",
        Some(&id),
        &details,
    )
    .await;

    Ok(ipma_common::ok_json((), "server.network.region_deleted"))
}

#[cfg(test)]
mod tests {
    //! 本模块覆盖 network.rs 处理器所依赖的纯校验链
    //!（CIDR 格式/类型、网关归属、区域包含），不触及数据库。

    use crate::utils::{
        cidr_belongs_to_region, get_cidr_type, validate_cidr, validate_gateway_in_cidr,
    };
    use ipma_common::AppError;

    // ==================== CIDR 格式校验（IPv4） ====================

    #[test]
    fn test_validate_cidr_ipv4_valid() {
        // 主机位全 0 的合法网段（含前缀边界 0 与 32）
        for cidr in [
            "10.0.0.0/8",
            "192.168.1.0/24",
            "172.16.0.0/12",
            "0.0.0.0/0",
            "10.1.2.3/32",
            // 无掩码的裸地址按 /32 处理（与 PostgreSQL CIDR 语义一致）
            "10.2.3.4",
        ] {
            assert!(validate_cidr(cidr), "合法 IPv4 CIDR {cidr} 应通过");
        }
    }

    #[test]
    fn test_validate_cidr_ipv4_invalid() {
        // 主机位非 0、前缀越界、格式错误均应拒绝
        for cidr in [
            "192.168.1.1/24", // 主机位非 0
            "10.0.0.0/33",    // 前缀超过 32
            "10.0.0.0/-1",    // 负前缀
            "10.0.0.0/",      // 掩码为空
            "300.0.0.0/8",    // 非法地址段
            "",               // 空串
            "abc/24",         // 非数字地址
        ] {
            assert!(!validate_cidr(cidr), "非法 IPv4 CIDR {cidr:?} 应被拒绝");
        }
    }

    #[test]
    fn test_validate_cidr_ipv6_valid() {
        for cidr in [
            "2001:db8::/32",
            "2001:db8:1::/48",
            "fe80::/10",
            "::/0",
            "2001:db8::1/128",
            // 无掩码的裸地址按 /128 处理
            "2001:db8::2",
        ] {
            assert!(validate_cidr(cidr), "合法 IPv6 CIDR {cidr} 应通过");
        }
    }

    #[test]
    fn test_validate_cidr_ipv6_invalid() {
        for cidr in [
            "2001:db8::1/64", // 主机位非 0
            "::1/0",          // 全地址下主机位非 0
            "::/129",         // 前缀超过 128
            "gg::/16",        // 非法十六进制
        ] {
            assert!(!validate_cidr(cidr), "非法 IPv6 CIDR {cidr:?} 应被拒绝");
        }
    }

    // ==================== CIDR 类型识别 ====================

    #[test]
    fn test_get_cidr_type_v4_v6_and_none() {
        assert_eq!(get_cidr_type("10.0.0.0/8"), Some("ipv4"));
        assert_eq!(get_cidr_type("192.168.1.0/30"), Some("ipv4"));
        assert_eq!(get_cidr_type("2001:db8::/32"), Some("ipv6"));
        assert_eq!(get_cidr_type("fe80::/64"), Some("ipv6"));
        // 非法输入（即便主机位非 0）类型解析只看地址族
        assert_eq!(get_cidr_type("192.168.1.1/24"), Some("ipv4"));
        // 完全非法输入返回 None
        assert_eq!(get_cidr_type("invalid"), None);
        assert_eq!(get_cidr_type(""), None);
        assert_eq!(get_cidr_type("10.0.0.0/33"), None);
    }

    #[test]
    fn test_get_cidr_type_family_mismatch_guard() {
        // create_network 的校验链要求「格式合法 + 地址族匹配」同时成立：
        // IPv4 CIDR 提交到 ipv6 槽位 / 反之均应视为不匹配
        assert_ne!(get_cidr_type("10.0.0.0/8"), Some("ipv6"));
        assert_ne!(get_cidr_type("2001:db8::/32"), Some("ipv4"));
    }

    // ==================== 网关与网段归属校验 ====================

    #[test]
    fn test_gateway_in_cidr_ipv4_ok() {
        // 网关在网段内且地址族一致应通过
        assert!(
            validate_gateway_in_cidr(Some("192.168.1.1"), Some("192.168.1.0/24"), "ipv4").is_ok()
        );
        // 网段边界上的首/末地址同样合法
        assert!(validate_gateway_in_cidr(Some("10.0.0.255"), Some("10.0.0.0/24"), "ipv4").is_ok());
        // 容忍 PostgreSQL INET 文本自带的掩码后缀
        assert!(
            validate_gateway_in_cidr(Some("192.168.1.1/32"), Some("192.168.1.0/24"), "ipv4")
                .is_ok()
        );
        // 空白网关视为未提供，直接通过
        assert!(validate_gateway_in_cidr(Some("  "), Some("192.168.1.0/24"), "ipv4").is_ok());
        assert!(validate_gateway_in_cidr(None, None, "ipv4").is_ok());
    }

    #[test]
    fn test_gateway_in_cidr_ipv4_rejected() {
        // 网段外
        let err = validate_gateway_in_cidr(Some("192.168.2.1"), Some("192.168.1.0/24"), "ipv4")
            .err()
            .unwrap_or_else(|| panic!("网段外网关应被拒绝"));
        assert!(matches!(err, AppError::Validation(_)));
        // 格式非法
        assert!(
            validate_gateway_in_cidr(Some("not-an-ip"), Some("192.168.1.0/24"), "ipv4").is_err()
        );
        // 地址族不匹配：IPv6 网关填入 ipv4 槽位
        assert!(
            validate_gateway_in_cidr(Some("2001:db8::1"), Some("192.168.1.0/24"), "ipv4").is_err(),
            "地址族不匹配应被拒绝"
        );
        // 提供网关但缺少同族 CIDR
        assert!(validate_gateway_in_cidr(Some("192.168.1.1"), None, "ipv4").is_err());
    }

    #[test]
    fn test_gateway_in_cidr_ipv6() {
        assert!(
            validate_gateway_in_cidr(Some("2001:db8::1"), Some("2001:db8::/64"), "ipv6").is_ok()
        );
        // 网段外
        assert!(
            validate_gateway_in_cidr(Some("2001:db9::1"), Some("2001:db8::/64"), "ipv6").is_err()
        );
        // IPv4 网关填入 ipv6 槽位
        assert!(
            validate_gateway_in_cidr(Some("192.168.1.1"), Some("2001:db8::/64"), "ipv6").is_err(),
            "地址族不匹配应被拒绝"
        );
        // /128 边界：网关恰好为唯一地址
        assert!(
            validate_gateway_in_cidr(Some("2001:db8::1"), Some("2001:db8::1/128"), "ipv6").is_ok()
        );
    }

    // ==================== 网段与区域 CIDR 包含关系 ====================

    #[test]
    fn test_cidr_belongs_to_region_empty_region_allows_all() {
        // 区域未定义 CIDR 时不做限制
        assert!(cidr_belongs_to_region("192.168.1.0/24", &[]));
        assert!(cidr_belongs_to_region("2001:db8:1::/48", &[]));
    }

    #[test]
    fn test_cidr_belongs_to_region_membership() {
        let region = vec!["10.0.0.0/8".to_string(), "2001:db8::/32".to_string()];
        // 子网（前缀更大）属于任一区域 CIDR 即通过
        assert!(cidr_belongs_to_region("10.1.0.0/16", &region));
        assert!(cidr_belongs_to_region("10.1.2.0/24", &region));
        assert!(cidr_belongs_to_region("2001:db8:1::/48", &region));
        // 与区域 CIDR 完全相同（前缀相等）不算子网
        assert!(
            !cidr_belongs_to_region("10.0.0.0/8", &region),
            "前缀相等不满足 << 语义"
        );
        // 区域外的网段
        assert!(!cidr_belongs_to_region("192.168.1.0/24", &region));
        assert!(!cidr_belongs_to_region("2001:db9::/32", &region));
    }

    #[test]
    fn test_cidr_belongs_to_region_cross_family_rejected() {
        // IPv4 与 IPv6 不能互相包含
        let region_v4 = vec!["10.0.0.0/8".to_string()];
        assert!(!cidr_belongs_to_region("2001:db8:1::/48", &region_v4));
        let region_v6 = vec!["2001:db8::/32".to_string()];
        assert!(!cidr_belongs_to_region("10.1.0.0/16", &region_v6));
        // 超网（前缀更小）同样不属于区域
        assert!(!cidr_belongs_to_region("10.0.0.0/7", &region_v4));
    }
}
