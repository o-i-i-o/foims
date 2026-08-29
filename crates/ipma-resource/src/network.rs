//! 网络区域与网段管理。

use crate::helpers::parse_network_from_row;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use ipma_auth::meta::{RequestMeta, log_op_best_effort};
use ipma_common::AppError;
use ipma_common::AppJson;
use ipma_common::DbProvider;
use ipma_common::pagination::{Pagination, paged_response};
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

/// 校验新 CIDR 与既有同行网段不重叠（包含/被包含均视为冲突），
/// IPv4/IPv6 分别处理；`exclude_id` 用于更新时排除自身。
async fn ensure_cidr_not_overlapping(
    conn: &mut sqlx::PgConnection,
    ipv4: Option<&str>,
    ipv6: Option<&str>,
    exclude_id: Option<Uuid>,
) -> Result<(), AppError> {
    if let Some(cidr) = ipv4 {
        let overlap: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM network_cidrs \
             WHERE ipv4_cidr && CAST($1 AS CIDR) \
             AND ($2::uuid IS NULL OR id != $2) LIMIT 1",
        )
        .bind(cidr)
        .bind(exclude_id)
        .fetch_optional(&mut *conn)
        .await?;
        if overlap.is_some() {
            return Err(AppError::Validation(msg("server.network.ipv4_cidr_exists")));
        }
    }
    if let Some(cidr) = ipv6 {
        let overlap: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM network_cidrs \
             WHERE ipv6_cidr && CAST($1 AS CIDR) \
             AND ($2::uuid IS NULL OR id != $2) LIMIT 1",
        )
        .bind(cidr)
        .bind(exclude_id)
        .fetch_optional(&mut *conn)
        .await?;
        if overlap.is_some() {
            return Err(AppError::Validation(msg("server.network.ipv6_cidr_exists")));
        }
    }
    Ok(())
}

pub async fn get_networks<P: DbProvider>(
    State(state): State<Arc<P>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();
    // 过滤参数非法 UUID 显式 422（与 options.rs/patch_panel.rs 口径一致），
    // 不再静默忽略退化为全量列表；空串视为未提供
    let region_id = match query.get("region_id") {
        Some(v) if !v.is_empty() => Some(Uuid::parse_str(v).map_err(|_| {
            AppError::Validation(msg("server.common.invalid_param").with("param", "region_id"))
        })?),
        _ => None,
    };

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
            let pattern = ipma_common::net::escape_like(&search);
            count_sql = count_sql.bind(pattern);
        }

        if let Some(rid) = region_id {
            count_sql = count_sql.bind(rid);
        }

        if !name_filter.is_empty() {
            let pattern = ipma_common::net::escape_like(&name_filter);
            count_sql = count_sql.bind(pattern);
        }

        if !network_region_filter.is_empty() {
            let pattern = ipma_common::net::escape_like(&network_region_filter);
            count_sql = count_sql.bind(pattern);
        }

        if !ipv4_filter.is_empty() {
            let pattern = ipma_common::net::escape_like(&ipv4_filter);
            count_sql = count_sql.bind(pattern);
        }

        if !ipv6_filter.is_empty() {
            let pattern = ipma_common::net::escape_like(&ipv6_filter);
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
            let pattern = ipma_common::net::escape_like(&search);
            data_sql = data_sql.bind(pattern);
        }

        if let Some(rid) = region_id {
            data_sql = data_sql.bind(rid);
        }

        if !name_filter.is_empty() {
            let pattern = ipma_common::net::escape_like(&name_filter);
            data_sql = data_sql.bind(pattern);
        }

        if !network_region_filter.is_empty() {
            let pattern = ipma_common::net::escape_like(&network_region_filter);
            data_sql = data_sql.bind(pattern);
        }

        if !ipv4_filter.is_empty() {
            let pattern = ipma_common::net::escape_like(&ipv4_filter);
            data_sql = data_sql.bind(pattern);
        }

        if !ipv6_filter.is_empty() {
            let pattern = ipma_common::net::escape_like(&ipv6_filter);
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

pub async fn create_network<P: DbProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<NetworkCreate>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 区域加载、重名/重复 CIDR/重叠校验与写入包进同一事务：
    // 校验与 INSERT 原子生效，缩小并发创建重叠网段的窗口
    let mut tx = state.pool()?.get_conn().begin().await?;

    let network_region = sqlx::query_as::<_, NetworkRegion>(
        "SELECT id, name, description,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv4_cidrs) AS d) as ipv4_cidrs,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv6_cidrs) AS d) as ipv6_cidrs,
                created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions WHERE id = $1"
    ).bind(req.network_region_id)
    .fetch_optional(&mut *tx).await?
    .ok_or_else(|| AppError::NotFound(msg("server.network.region_not_found")))?;

    let full_network_name = req.name.clone();

    let existing_network = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM network_cidrs WHERE name = $1 AND network_region_id = $2",
    )
    .bind(&full_network_name)
    .bind(req.network_region_id)
    .fetch_optional(&mut *tx)
    .await?;

    if existing_network.is_some() {
        return Err(AppError::Conflict(msg("server.network.name_exists")));
    }

    let mut ipv4_cidr_val: Option<String> = None;
    let mut ipv6_cidr_val: Option<String> = None;
    let mut has_valid_cidr = false;

    if let Some(ipv4_cidr) = &req.ipv4_cidr {
        if ipma_common::net::validate_cidr(ipv4_cidr)
            && ipma_common::net::get_cidr_type(ipv4_cidr) == Some("ipv4")
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
        if ipma_common::net::validate_cidr(ipv6_cidr)
            && ipma_common::net::get_cidr_type(ipv6_cidr) == Some("ipv6")
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
    ipma_common::net::validate_gateway_in_cidr(
        req.ipv4_gateway.as_deref(),
        ipv4_cidr_val.as_deref(),
        "ipv4",
    )?;
    ipma_common::net::validate_gateway_in_cidr(
        req.ipv6_gateway.as_deref(),
        ipv6_cidr_val.as_deref(),
        "ipv6",
    )?;

    // 校验网段 CIDR 是否属于所在区域的 CIDR 范围
    if let Some(ref ipv4) = ipv4_cidr_val
        && !ipma_common::net::cidr_belongs_to_region(
            ipv4,
            network_region.ipv4_cidrs.as_deref().unwrap_or_default(),
        )
    {
        return Err(AppError::Validation(msg(
            "server.network.ipv4_not_in_region",
        )));
    }

    if let Some(ref ipv6) = ipv6_cidr_val
        && !ipma_common::net::cidr_belongs_to_region(
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
                .fetch_optional(&mut *tx)
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
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| {
                    log_error!("log.network.check_ipv6_duplicate_failed", error = e);
                    AppError::Database(msg("server.network.check_ipv6_duplicate_failed"))
                })?;

        if existing_ipv6.is_some() {
            return Err(AppError::Conflict(msg("server.network.ipv6_cidr_exists")));
        }
    }

    // 重叠网段校验：与既有网段存在包含/被包含关系时拒绝（/25 vs /24 等）
    ensure_cidr_not_overlapping(
        &mut tx,
        ipv4_cidr_val.as_deref(),
        ipv6_cidr_val.as_deref(),
        None,
    )
    .await?;

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
    .execute(&mut *tx)
    .await
    .map_err(|e| map_network_unique_violation(e, &full_network_name))?;

    tx.commit().await?;

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

pub async fn get_network<P: DbProvider>(
    State(state): State<Arc<P>>,
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

pub async fn update_network<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<NetworkUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;

    // 存在性/区域/重名/重复 CIDR/重叠/越界校验与写入包进同一事务：
    // 各项校验与 UPDATE 原子生效，缩小并发创建重叠网段的窗口
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_network =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_cidrs WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing_network.is_none() {
        return Err(AppError::NotFound(msg("server.network.not_found")));
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
    .fetch_one(&mut *tx).await?;
    let current_network = parse_network_from_row(&row)?;

    let full_network_name = match &req.name {
        Some(new_name) => new_name.clone(),
        None => current_network.name.clone(),
    };

    // 最终生效的区域：请求指定时为目标区域，否则沿用现值
    let network_region_id: Uuid = req
        .network_region_id
        .unwrap_or(current_network.network_region_id);

    // 目标区域存在性与取值合并为一次查询（同时覆盖"仅改区域"的存在性校验）
    let target_network_region = sqlx::query_as::<_, NetworkRegion>(
        "SELECT id, name, description,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv4_cidrs) AS d) as ipv4_cidrs,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv6_cidrs) AS d) as ipv6_cidrs,
                created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions WHERE id = $1"
    )
    .bind(network_region_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.network.region_not_found")))?;

    let existing_network = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM network_cidrs WHERE name = $1 AND network_region_id = $2 AND id != $3",
    )
    .bind(&full_network_name)
    .bind(network_region_id)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;

    if existing_network.is_some() {
        return Err(AppError::Conflict(msg("server.network.name_exists")));
    }

    // 双层 Option 解引用：Some(Some(v)) 提交新值；Some(None) 表示清空；
    // 外层 None 表示不修改（ipv4/ipv6 同口径）
    let req_ipv4_cidr: Option<&str> = req.ipv4_cidr.as_ref().and_then(|o| o.as_deref());
    let req_ipv6_cidr: Option<&str> = req.ipv6_cidr.as_ref().and_then(|o| o.as_deref());

    // 提交新 IPv4 CIDR 时校验格式、重复与区域归属
    if let Some(ipv4) = req_ipv4_cidr {
        if !ipma_common::net::validate_cidr(ipv4)
            || ipma_common::net::get_cidr_type(ipv4) != Some("ipv4")
        {
            return Err(AppError::Validation(msg(
                "server.network.ipv4_cidr_invalid",
            )));
        }

        let existing_ipv4: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM network_cidrs WHERE ipv4_cidr = CAST($1 AS CIDR) AND id != $2",
        )
        .bind(ipv4)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            log_error!("log.network.check_ipv4_duplicate_failed", error = e);
            AppError::Database(msg("server.network.check_ipv4_duplicate_failed"))
        })?;

        if existing_ipv4.is_some() {
            return Err(AppError::Conflict(msg("server.network.ipv4_cidr_in_use")));
        }

        // 校验 IPv4 CIDR 是否属于所在区域的 CIDR 范围
        if !ipma_common::net::cidr_belongs_to_region(
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

    if let Some(ref ipv6) = req_ipv6_cidr {
        if !ipma_common::net::validate_cidr(ipv6)
            || ipma_common::net::get_cidr_type(ipv6) != Some("ipv6")
        {
            return Err(AppError::Validation(msg(
                "server.network.ipv6_cidr_invalid",
            )));
        }

        let existing_ipv6: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM network_cidrs WHERE ipv6_cidr = CAST($1 AS CIDR) AND id != $2",
        )
        .bind(ipv6)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            log_error!("log.network.check_ipv6_duplicate_failed", error = e);
            AppError::Database(msg("server.network.check_ipv6_duplicate_failed"))
        })?;

        if existing_ipv6.is_some() {
            return Err(AppError::Conflict(msg("server.network.ipv6_cidr_in_use")));
        }

        // 校验 IPv6 CIDR 是否属于所在区域的 CIDR 范围
        if !ipma_common::net::cidr_belongs_to_region(
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

    // 仅修改区域而未同时提交新 CIDR 时，存量 CIDR 也必须属于目标区域
    //（v4/v6 分别校验），避免换区域后下辖网段悬空
    if req.network_region_id.is_some() {
        if req_ipv4_cidr.is_none()
            && let Some(cidr) = current_network.ipv4_cidr.as_deref()
            && !ipma_common::net::cidr_belongs_to_region(
                cidr,
                target_network_region
                    .ipv4_cidrs
                    .as_deref()
                    .unwrap_or_default(),
            )
        {
            return Err(AppError::Validation(msg(
                "server.network.ipv4_not_in_region",
            )));
        }
        if req_ipv6_cidr.is_none()
            && let Some(cidr) = current_network.ipv6_cidr.as_deref()
            && !ipma_common::net::cidr_belongs_to_region(
                cidr,
                target_network_region
                    .ipv6_cidrs
                    .as_deref()
                    .unwrap_or_default(),
            )
        {
            return Err(AppError::Validation(msg(
                "server.network.ipv6_not_in_region",
            )));
        }
    }

    // 网关校验：以最终生效值（字段被提交时取提交值——含 Some(None) 清空，
    // 否则取库中现值）与最终生效的 CIDR 核对，
    // 确保"只改 CIDR 不改网关"等部分更新后的数据仍保持一致
    let effective_ipv4_cidr = match &req.ipv4_cidr {
        Some(inner) => inner.as_deref(),
        None => current_network.ipv4_cidr.as_deref(),
    };
    let effective_ipv4_gateway = match &req.ipv4_gateway {
        Some(inner) => inner.as_deref(),
        None => current_network.ipv4_gateway.as_deref(),
    };
    ipma_common::net::validate_gateway_in_cidr(
        effective_ipv4_gateway,
        effective_ipv4_cidr,
        "ipv4",
    )?;
    // ipv6 同口径：Some(inner) 取提交值（含 Some(None) 清空），None 取库中现值
    let effective_ipv6_cidr = match &req.ipv6_cidr {
        Some(inner) => inner.as_deref(),
        None => current_network.ipv6_cidr.as_deref(),
    };
    let effective_ipv6_gateway = match &req.ipv6_gateway {
        Some(inner) => inner.as_deref(),
        None => current_network.ipv6_gateway.as_deref(),
    };
    ipma_common::net::validate_gateway_in_cidr(
        effective_ipv6_gateway,
        effective_ipv6_cidr,
        "ipv6",
    )?;

    // 重叠网段校验：新 CIDR 与既有同行网段存在包含/被包含关系时拒绝（排除自身）；
    // 清空（Some(None)）与不修改（None）无需重叠复核
    ensure_cidr_not_overlapping(&mut tx, req_ipv4_cidr, req_ipv6_cidr, Some(id)).await?;

    // 修改 CIDR 后校验存量 IP 仍在新网段内：有越界 IP 时拒绝（422）并列出数量
    if req.ipv4_cidr.is_some() || req.ipv6_cidr.is_some() {
        let outside_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ips \
             WHERE network_id = $1 \
             AND NOT ( \
                 (CAST($2 AS CIDR) IS NOT NULL AND ip_address <<= CAST($2 AS CIDR)) \
                 OR (CAST($3 AS CIDR) IS NOT NULL AND ip_address <<= CAST($3 AS CIDR)) \
             )",
        )
        .bind(id)
        .bind(effective_ipv4_cidr)
        .bind(effective_ipv6_cidr)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            // 日志键区分于 IPv4 查重失败（复制粘贴遗留的正确键见 i18n 契约）
            log_error!("log.network.check_ips_outside_cidr_failed", error = e);
            AppError::Database(msg("server.network.check_ipv4_duplicate_failed"))
        })?;
        if outside_count > 0 {
            tracing::warn!(
                "网段 {} 的 CIDR 收缩后仍有 {} 个存量 IP 越界，拒绝更新",
                id,
                outside_count
            );
            return Err(AppError::Validation(
                msg("server.common.invalid_param").with("param", "ipv4_cidr"),
            ));
        }
    }

    // 三态写入：ipv4_cidr/ipv4_gateway 为双层 Option——外层 Some 进 SET
    //（Some(None) 置 NULL，Some(Some(v)) 设新值），None 保留旧值
    sqlx::query(
        "UPDATE network_cidrs SET
         name = $1,
         network_region_id = COALESCE($2, network_region_id),
         ipv4_cidr = CASE WHEN $3::boolean THEN CAST($4 AS CIDR) ELSE ipv4_cidr END,
         ipv6_cidr = CASE WHEN $5::boolean THEN CAST($6 AS CIDR) ELSE ipv6_cidr END,
         ipv4_gateway = CASE WHEN $7::boolean THEN CAST($8 AS INET) ELSE ipv4_gateway END,
         ipv6_gateway = CASE WHEN $9::boolean THEN CAST($10 AS INET) ELSE ipv6_gateway END,
         ipv4_dns = COALESCE($11::INET[], ipv4_dns),
         ipv6_dns = COALESCE($12::INET[], ipv6_dns),
         description = COALESCE($13, description),
         updated_at = $14
         WHERE id = $15",
    )
    .bind(&full_network_name)
    .bind(req.network_region_id)
    .bind(req.ipv4_cidr.is_some())
    .bind(req.ipv4_cidr.clone().flatten())
    .bind(req.ipv6_cidr.is_some())
    .bind(req.ipv6_cidr.clone().flatten())
    .bind(req.ipv4_gateway.is_some())
    .bind(req.ipv4_gateway.clone().flatten())
    .bind(req.ipv6_gateway.is_some())
    .bind(req.ipv6_gateway.clone().flatten())
    .bind(&req.ipv4_dns)
    .bind(&req.ipv6_dns)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
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
    .fetch_one(&mut *tx).await?;

    let network = parse_network_from_row(&row)?;

    tx.commit().await?;

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

pub async fn delete_network<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    // 计数检查与 DELETE 放同一事务，避免检查后被并发写入绕过
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing_network =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_cidrs WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing_network.is_none() {
        return Err(AppError::NotFound(msg("server.network.not_found")));
    }

    let ip_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ips WHERE network_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    if ip_count > 0 {
        return Err(AppError::Validation(msg("server.network.in_use_by_ip")));
    }

    let room_network_count: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM room_networks WHERE network_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    if room_network_count > 0 {
        return Err(AppError::Validation(msg("server.network.in_use_by_room")));
    }

    // cabinet_network_count 检查已删除：cabinets 与 room_networks 仅经
    // rooms.room_id 间接关联，无直接外键，删除网段不会级联影响机柜，
    // 该分支为不可达死逻辑（room_network_count 已覆盖删除保护）

    sqlx::query("DELETE FROM network_cidrs WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

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

pub async fn get_network_regions<P: DbProvider>(
    State(state): State<Arc<P>>,
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
        let pattern = ipma_common::net::escape_like(&search);
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
        let pattern = ipma_common::net::escape_like(&search);
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

/// 区域 CIDR 数组入库前校验：逐条 [`ipma_common::net::validate_cidr`]
/// 格式检查（非法直接 422，而非绑定后撞 PG 22P02 报 500），
/// 并拒绝数组内重复条目（重复项对包含语义无增益，只会污染展示）。
fn validate_region_cidr_array(
    cidrs: Option<&Vec<String>>,
    param: &'static str,
) -> Result<(), AppError> {
    let Some(list) = cidrs else {
        return Ok(());
    };
    let mut seen = std::collections::HashSet::with_capacity(list.len());
    for cidr in list {
        if cidr.trim().is_empty() || !ipma_common::net::validate_cidr(cidr) {
            return Err(AppError::Validation(
                msg("server.common.invalid_param").with("param", param),
            ));
        }
        if !seen.insert(cidr.trim()) {
            return Err(AppError::Validation(
                msg("server.common.invalid_param").with("param", param),
            ));
        }
    }
    Ok(())
}

pub async fn create_network_region<P: DbProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<NetworkRegionCreate>,
) -> Result<Response, AppError> {
    req.validate()?;
    // CIDR 数组前置校验：非法格式/数组内重复在绑定 CIDR[] 前拦截
    validate_region_cidr_array(req.ipv4_cidrs.as_ref(), "ipv4_cidrs")?;
    validate_region_cidr_array(req.ipv6_cidrs.as_ref(), "ipv6_cidrs")?;

    // 重名预检与写入放同一事务，避免 TOCTOU
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_regions WHERE name = $1")
            .bind(&req.name)
            .fetch_optional(&mut *tx)
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
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // 并发写入竞态兜底：network_regions.name 唯一冲突映射为 409
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict(msg("server.network.region_name_exists"));
        }
        AppError::from(e)
    })?;

    tx.commit().await?;

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

pub async fn get_network_region<P: DbProvider>(
    State(state): State<Arc<P>>,
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

pub async fn update_network_region<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<NetworkRegionUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;
    // CIDR 数组前置校验（含数组内重复检查）：收缩区域等更新场景
    // 在绑定 CIDR[] 前拦截非法值
    validate_region_cidr_array(req.ipv4_cidrs.as_ref(), "ipv4_cidrs")?;
    validate_region_cidr_array(req.ipv6_cidrs.as_ref(), "ipv6_cidrs")?;

    // 预检、写入与更新后复查放同一事务，任一失败整体回滚
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_regions WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
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
        .fetch_optional(&mut *tx)
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
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // 并发写入竞态兜底：network_regions.name 唯一冲突映射为 409
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict(msg("server.network.region_name_exists"));
        }
        AppError::from(e)
    })?;

    // 区域 CIDR 更新后复查：其下所有网段的 CIDR 仍须被新区域范围包含，
    // 越界则整体回滚（422），避免收缩区域后下辖网段悬空
    if req.ipv4_cidrs.is_some() || req.ipv6_cidrs.is_some() {
        let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT ipv4_cidr::TEXT, ipv6_cidr::TEXT FROM network_cidrs WHERE network_region_id = $1",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        for (ipv4, ipv6) in &rows {
            if let Some(cidr) = ipv4
                && !ipma_common::net::cidr_belongs_to_region(
                    cidr,
                    req.ipv4_cidrs.as_deref().unwrap_or_default(),
                )
            {
                tx.rollback().await.ok();
                return Err(AppError::Validation(
                    msg("server.common.invalid_param").with("param", "ipv4_cidrs"),
                ));
            }
            if let Some(cidr) = ipv6
                && !ipma_common::net::cidr_belongs_to_region(
                    cidr,
                    req.ipv6_cidrs.as_deref().unwrap_or_default(),
                )
            {
                tx.rollback().await.ok();
                return Err(AppError::Validation(
                    msg("server.common.invalid_param").with("param", "ipv6_cidrs"),
                ));
            }
        }
    }

    tx.commit().await?;

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

pub async fn delete_network_region<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    // 存在性/占用检查与 DELETE 包进同一事务，避免检查后被并发创建
    // 的网段绕过（并发下以 FK 500 收场的 TOCTOU）
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_regions WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_none() {
        return Err(AppError::NotFound(msg("server.network.region_not_found")));
    }

    let network_count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM network_cidrs WHERE network_region_id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;

    if network_count > 0 {
        return Err(AppError::Validation(msg("server.network.region_in_use")));
    }

    sqlx::query("DELETE FROM network_regions WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

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

    use ipma_common::AppError;
    use ipma_common::net::{
        cidr_belongs_to_region, get_cidr_type, validate_cidr, validate_gateway_in_cidr,
    };

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

    // ==================== 区域 CIDR 数组前置校验 ====================

    #[test]
    fn test_validate_region_cidr_array_valid() {
        // None（字段缺省）与合法数组均通过
        assert!(super::validate_region_cidr_array(None, "ipv4_cidrs").is_ok());
        let list = vec!["10.0.0.0/8".to_string(), "192.168.0.0/16".to_string()];
        assert!(super::validate_region_cidr_array(Some(&list), "ipv4_cidrs").is_ok());
        let v6 = vec!["2001:db8::/32".to_string()];
        assert!(super::validate_region_cidr_array(Some(&v6), "ipv6_cidrs").is_ok());
        // 空数组合法（清空区域范围）
        assert!(super::validate_region_cidr_array(Some(&Vec::new()), "ipv4_cidrs").is_ok());
    }

    #[test]
    fn test_validate_region_cidr_array_invalid_format() {
        // 非法 CIDR（主机位非 0 / 前缀越界 / 非法文本 / 空串）在绑定前拦截为 422
        for bad in ["192.168.1.1/24", "10.0.0.0/33", "abc", ""] {
            let list = vec!["10.0.0.0/8".to_string(), bad.to_string()];
            let err = super::validate_region_cidr_array(Some(&list), "ipv4_cidrs")
                .err()
                .unwrap_or_else(|| panic!("非法 CIDR {bad:?} 应被拒绝"));
            assert!(
                matches!(err, AppError::Validation(_)),
                "应为 422 校验错误，实际 {err}"
            );
        }
    }

    #[test]
    fn test_validate_region_cidr_array_duplicate_rejected() {
        // 数组内重复条目拒绝（trim 后等价视为同一条）
        let dup = vec![
            "10.0.0.0/8".to_string(),
            "172.16.0.0/12".to_string(),
            "10.0.0.0/8".to_string(),
        ];
        assert!(super::validate_region_cidr_array(Some(&dup), "ipv4_cidrs").is_err());
        // 带首尾空白的条目同样被拒绝（非法格式或 trim 后重复）
        let spaced = vec!["2001:db8::/32".to_string(), " 2001:db8::/32 ".to_string()];
        assert!(super::validate_region_cidr_array(Some(&spaced), "ipv6_cidrs").is_err());
    }
}
