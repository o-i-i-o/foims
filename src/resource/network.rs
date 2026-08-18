//! 网络区域与网段管理。

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    Network, NetworkCreate, NetworkRegion, NetworkRegionCreate, NetworkRegionUpdate, NetworkUpdate,
};
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};
use crate::utils::pagination::{Pagination, paged_response};
use crate::utils::parse_network_from_row;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use ipma_common::{log_error, log_info, msg};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

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
            let pattern = format!("%{name_filter}%");
            count_sql = count_sql.bind(pattern);
        }

        if !network_region_filter.is_empty() {
            let pattern = format!("%{network_region_filter}%");
            count_sql = count_sql.bind(pattern);
        }

        if !ipv4_filter.is_empty() {
            let pattern = format!("%{ipv4_filter}%");
            count_sql = count_sql.bind(pattern);
        }

        if !ipv6_filter.is_empty() {
            let pattern = format!("%{ipv6_filter}%");
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
            r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT,
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
            let pattern = format!("%{name_filter}%");
            data_sql = data_sql.bind(pattern);
        }

        if !network_region_filter.is_empty() {
            let pattern = format!("%{network_region_filter}%");
            data_sql = data_sql.bind(pattern);
        }

        if !ipv4_filter.is_empty() {
            let pattern = format!("%{ipv4_filter}%");
            data_sql = data_sql.bind(pattern);
        }

        if !ipv6_filter.is_empty() {
            let pattern = format!("%{ipv6_filter}%");
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
            r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT,
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

    Ok(crate::error::ok_json(
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

    // 校验网段 CIDR 是否属于所在区域的 CIDR 范围
    if let Some(ref ipv4) = ipv4_cidr_val
        && !crate::utils::cidr_belongs_to_region(
            ipv4,
            &network_region.ipv4_cidrs.clone().unwrap_or_default(),
        )
    {
        return Err(AppError::Validation(msg(
            "server.network.ipv4_not_in_region",
        )));
    }

    if let Some(ref ipv6) = ipv6_cidr_val
        && !crate::utils::cidr_belongs_to_region(
            ipv6,
            &network_region.ipv6_cidrs.clone().unwrap_or_default(),
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

    let ipv4_dns_array: Option<Vec<String>> = req.ipv4_dns.clone();
    let ipv6_dns_array: Option<Vec<String>> = req.ipv6_dns.clone();

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
    .bind(&ipv4_dns_array)
    .bind(&ipv6_dns_array)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&state.pool()?.get_conn()).await?;

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

    let network = Network {
        id,
        name: full_network_name,
        network_region_id: req.network_region_id,
        network_region: network_region.name,
        ipv4_cidr: ipv4_cidr_val,
        ipv6_cidr: ipv6_cidr_val,
        ipv4_gateway: req.ipv4_gateway.clone(),
        ipv6_gateway: req.ipv6_gateway.clone(),
        ipv4_dns: req.ipv4_dns.clone(),
        ipv6_dns: req.ipv6_dns.clone(),
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    Ok(crate::error::ok_json(network, "server.network.created"))
}

pub async fn get_network(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let row = sqlx::query(
        r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, 
                  n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, 
                  n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, 
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

    Ok(crate::error::ok_json(network, "server.network.fetched"))
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
        r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT,
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
            &target_network_region.ipv4_cidrs.clone().unwrap_or_default(),
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
            &target_network_region.ipv6_cidrs.clone().unwrap_or_default(),
        ) {
            return Err(AppError::Validation(msg(
                "server.network.ipv6_not_in_region",
            )));
        }
    }

    let ipv4_dns_array: Option<Vec<String>> = req.ipv4_dns.clone();
    let ipv6_dns_array: Option<Vec<String>> = req.ipv6_dns.clone();

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
    .bind(&ipv4_dns_array)
    .bind(&ipv6_dns_array)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&state.pool()?.get_conn())
    .await?;

    let row = sqlx::query(
        r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, 
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

    Ok(crate::error::ok_json(network, "server.network.updated"))
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

    Ok(crate::error::ok_json((), "server.network.deleted"))
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
        let pattern = format!("%{search}%");
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
        let pattern = format!("%{search}%");
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

    Ok(crate::error::ok_json(
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

    Ok(crate::error::ok_json(
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

    Ok(crate::error::ok_json(
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

    Ok(crate::error::ok_json(
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

    Ok(crate::error::ok_json((), "server.network.region_deleted"))
}
