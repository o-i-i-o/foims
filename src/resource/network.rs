use crate::config::Config;
use crate::db::DbPool;
use crate::models::{
    ApiResponse, Network, NetworkCreate, NetworkRegion, NetworkRegionCreate, NetworkRegionUpdate,
    NetworkUpdate,
};
use crate::utils::{DEFAULT_PAGE, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, Result, web};
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

// 网络相关路由
// 获取所有网络（支持搜索和分页）
pub async fn get_networks(
    pool: web::Data<DbPool>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    let page: i64 = query
        .get("page")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PAGE);
    let page_size: i64 = query
        .get("page_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let search = query.get("search").cloned().unwrap_or_default();
    let region_id = query
        .get("region_id")
        .and_then(|id| Uuid::parse_str(id).ok());

    let name_filter = query.get("name").cloned().unwrap_or_default();
    let network_region_filter = query.get("network_region").cloned().unwrap_or_default();
    let ipv4_filter = query.get("ipv4_cidr").cloned().unwrap_or_default();
    let ipv6_filter = query.get("ipv6_cidr").cloned().unwrap_or_default();

    let offset = (page - 1) * page_size;

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
            conditions.push(format!("(n.name ILIKE ${param_count} OR n.description ILIKE ${param_count} OR n.ipv4_cidr::TEXT ILIKE ${param_count} OR n.ipv6_cidr::TEXT ILIKE {param_count})"));
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

        let mut count_sql = sqlx::query_scalar(&count_query);

        if !search.is_empty() {
            let pattern = format!("%{search}%");
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

        match count_sql.fetch_one(&pool.get_conn()).await {
            Ok(t) => t,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询网络数量失败"));
            }
        }
    } else {
        match sqlx::query_scalar("SELECT COUNT(*) FROM network_cidrs")
            .fetch_one(&pool.get_conn())
            .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询网络数量失败"));
            }
        }
    };

    let networks: Vec<Network> = if has_filters {
        let mut conditions = Vec::new();
        let mut param_count = 1;

        if !search.is_empty() {
            conditions.push(format!("(n.name ILIKE ${param_count} OR n.description ILIKE ${param_count} OR n.ipv4_cidr::TEXT ILIKE ${param_count} OR n.ipv6_cidr::TEXT ILIKE {param_count})"));
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
               NULL as gateway, NULL as dns, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
               FROM network_cidrs n 
               JOIN network_regions nt ON n.network_region_id = nt.id 
               {}
               ORDER BY n.created_at DESC
               LIMIT ${} OFFSET ${}",
            where_clause,
            param_count,
            param_count + 1
        );

        let mut data_sql = sqlx::query(&data_query);

        if !search.is_empty() {
            let pattern = format!("%{search}%");
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

        match data_sql.fetch_all(&pool.get_conn()).await {
            Ok(rows) => rows
                .into_iter()
                .map(|row| Network {
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
                        .and_then(|v| serde_json::from_value(v).ok()),
                    ipv6_dns: row
                        .get::<Option<serde_json::Value>, _>(9)
                        .and_then(|v| serde_json::from_value(v).ok()),
                    description: row.get(12),
                    created_at: row.get(13),
                    updated_at: row.get(14),
                })
                .collect(),
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询网络列表失败"));
            }
        }
    } else {
        match sqlx::query(
            r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, 
               (SELECT json_agg(host(d)) FROM unnest(n.ipv4_dns) AS d) as ipv4_dns,
               (SELECT json_agg(host(d)) FROM unnest(n.ipv6_dns) AS d) as ipv6_dns,
               NULL as gateway, NULL as dns, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
               FROM network_cidrs n 
               JOIN network_regions nt ON n.network_region_id = nt.id 
               ORDER BY n.created_at DESC
               LIMIT $1 OFFSET $2"
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(&pool.get_conn())
        .await
        {
            Ok(rows) => rows
                .into_iter()
                .map(|row| Network {
                    id: row.get(0),
                    name: row.get(1),
                    network_region_id: row.get(2),
                    network_region: row.get(3),
                    ipv4_cidr: row.get(4),
                    ipv6_cidr: row.get(5),
                    ipv4_gateway: row.get(6),
                    ipv6_gateway: row.get(7),
                    ipv4_dns: row.get::<Option<serde_json::Value>, _>(8).and_then(|v| serde_json::from_value(v).ok()),
                    ipv6_dns: row.get::<Option<serde_json::Value>, _>(9).and_then(|v| serde_json::from_value(v).ok()),
                    description: row.get(12),
                    created_at: row.get(13),
                    updated_at: row.get(14),
                })
                .collect(),
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询网络列表失败"));
            }
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": networks,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "网络获取成功",
    )))
}

// 创建网络
pub async fn create_network(
    pool: web::Data<DbPool>,
    req: web::Json<NetworkCreate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 验证创建网络请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {e:?}")))
        );
    }

    // 验证网络区域是否存在
    let network_region = match sqlx::query_as::<_, NetworkRegion>(
        "SELECT id, name, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions WHERE id = $1"
    ).bind(req.network_region_id)
    .fetch_optional(&pool.get_conn()).await {
        Ok(Some(network_region)) => network_region,
        Ok(None) => {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<Network>::error("网络区域不存在")));
        },
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "数据库查询错误"));
        }
    };

    // 直接使用用户输入的网络名称，不再与网络区域名称组合
    let full_network_name = req.name.clone();

    // 检查同一网络区域内网络名称是否已存在
    let existing_network = match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM network_cidrs WHERE name = $1 AND network_region_id = $2",
    )
    .bind(&full_network_name)
    .bind(req.network_region_id)
    .fetch_optional(&pool.get_conn())
    .await
    {
        Ok(network) => network,
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "数据库查询错误"));
        }
    };

    if existing_network.is_some() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<Network>::error(
                "同一网络区域内网络名称已存在",
            )),
        );
    }

    // CIDR验证逻辑：允许IPv4或IPv6其中一个校验通过即可
    let mut ipv4_cidr_val: Option<String> = None;
    let mut ipv6_cidr_val: Option<String> = None;
    let mut has_valid_cidr = false;

    // 处理ipv4_cidr字段
    if let Some(ipv4_cidr) = &req.ipv4_cidr {
        if crate::utils::validate_cidr(ipv4_cidr)
            && crate::utils::get_cidr_type(ipv4_cidr) == Some("ipv4")
        {
            ipv4_cidr_val = Some(ipv4_cidr.clone());
            has_valid_cidr = true;
        } else {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                "请输入有效的IPv4 CIDR格式，例如：192.168.1.0/24",
            )));
        }
    }

    // 处理ipv6_cidr字段
    if let Some(ipv6_cidr) = &req.ipv6_cidr {
        if crate::utils::validate_cidr(ipv6_cidr)
            && crate::utils::get_cidr_type(ipv6_cidr) == Some("ipv6")
        {
            ipv6_cidr_val = Some(ipv6_cidr.clone());
            has_valid_cidr = true;
        } else {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                "请输入有效的IPv6 CIDR格式，例如：2001:db8::/32",
            )));
        }
    }

    // 至少需要一个有效的CIDR
    if !has_valid_cidr {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "至少需要提供一个有效的IPv4或IPv6 CIDR",
        )));
    }

    // 检查网段唯一性：ipv4或ipv6任一重复即视为重复
    if let Some(ref ipv4) = ipv4_cidr_val {
        let existing_ipv4: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM network_cidrs WHERE ipv4_cidr = CAST($1 AS CIDR)")
                .bind(ipv4)
                .fetch_optional(&pool.get_conn())
                .await
                .unwrap_or(None);

        if existing_ipv4.is_some() {
            return Ok(HttpResponse::BadRequest()
                .json(ApiResponse::<()>::error("IPv4网段已存在，网段不能重复")));
        }
    }

    if let Some(ref ipv6) = ipv6_cidr_val {
        let existing_ipv6: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM network_cidrs WHERE ipv6_cidr = CAST($1 AS CIDR)")
                .bind(ipv6)
                .fetch_optional(&pool.get_conn())
                .await
                .unwrap_or(None);

        if existing_ipv6.is_some() {
            return Ok(HttpResponse::BadRequest()
                .json(ApiResponse::<()>::error("IPv6网段已存在，网段不能重复")));
        }
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    // 创建网络
    let ipv4_dns_array: Option<Vec<String>> = req.ipv4_dns.clone();
    let ipv6_dns_array: Option<Vec<String>> = req.ipv6_dns.clone();

    if let Err(err) = sqlx::query(
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
    .execute(&pool.get_conn()).await {
        return Ok(crate::utils::handle_db_error(err, "数据库插入错误"));
    }

    // 记录操作日志（在创建network结构体之前，避免值移动问题）
    let details = serde_json::json!({
        "name": full_network_name.clone(),
        "network_region": network_region.name.clone(),
        "ipv4_cidr": ipv4_cidr_val,
        "ipv6_cidr": ipv6_cidr_val
    });
    let _ = log_system_operation(
        &pool.get_conn(),
        &http_req,
        config.get_ref(),
        "create",
        "network",
        &id,
        &details,
        true,
    )
    .await;

    // 返回创建的网络
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

    Ok(HttpResponse::Ok().json(ApiResponse::<Network>::success(network, "网络创建成功")))
}

// 获取单个网络
pub async fn get_network(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = *id_path;

    let network = match sqlx::query(
        r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, 
                  n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, 
                  n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, 
                  (SELECT json_agg(host(d)) FROM unnest(n.ipv4_dns) AS d) as ipv4_dns,
                  (SELECT json_agg(host(d)) FROM unnest(n.ipv6_dns) AS d) as ipv6_dns,
                  NULL as gateway, NULL as dns, 
                  n.description, 
                  n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
           FROM network_cidrs n 
           JOIN network_regions nt ON n.network_region_id = nt.id 
           WHERE n.id = $1",
    )
    .bind(id)
    .fetch_optional(&pool.get_conn())
    .await
    {
        Ok(Some(row)) => Network {
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
                .and_then(|v| serde_json::from_value(v).ok()),
            ipv6_dns: row
                .get::<Option<serde_json::Value>, _>(9)
                .and_then(|v| serde_json::from_value(v).ok()),
            description: row.get(12),
            created_at: row.get(13),
            updated_at: row.get(14),
        },
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<Network>::error("网络未找到")));
        }
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询网络失败"));
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::<Network>::success(network, "网络获取成功")))
}

// 更新网络
pub async fn update_network(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    req: web::Json<NetworkUpdate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 验证更新网络请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "Validation error: {e:?}"
            ))),
        );
    }

    // 检查网络是否存在
    let existing_network =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_cidrs WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool.get_conn())
            .await
        {
            Ok(network) => network,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "数据库查询错误"));
            }
        };

    if existing_network.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<Network>::error("网络未找到")));
    }

    // 如果提供了网络区域ID，验证它是否存在
    if let Some(network_region_id) = &req.network_region_id {
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_regions WHERE id = $1")
            .bind(network_region_id)
            .fetch_optional(&pool.get_conn())
            .await
        {
            Ok(Some(_)) => (),
            Ok(None) => {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<Network>::error("网络区域不存在")));
            }
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "数据库查询错误"));
            }
        }
    }

    let now = Utc::now();

    // 获取当前网络区域信息，用于构建完整网络名称
    let current_network = match sqlx::query(
        r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, 
           (SELECT json_agg(host(d)) FROM unnest(n.ipv4_dns) AS d) as ipv4_dns,
           (SELECT json_agg(host(d)) FROM unnest(n.ipv6_dns) AS d) as ipv6_dns,
           n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
           FROM network_cidrs n 
           JOIN network_regions nt ON n.network_region_id = nt.id 
           WHERE n.id = $1"
    ).bind(id)
    .fetch_one(&pool.get_conn()).await {
        Ok(row) => Network {
            id: row.get(0),
            name: row.get(1),
            network_region_id: row.get(2),
            network_region: row.get(3),
            ipv4_cidr: row.get(4),
            ipv6_cidr: row.get(5),
            ipv4_gateway: row.get(6),
            ipv6_gateway: row.get(7),
            ipv4_dns: row.get::<Option<serde_json::Value>, _>(8).and_then(|v| serde_json::from_value(v).ok()),
            ipv6_dns: row.get::<Option<serde_json::Value>, _>(9).and_then(|v| serde_json::from_value(v).ok()),
            description: row.get(10),
            created_at: row.get(11),
            updated_at: row.get(12),
        },
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "数据库查询错误"));
        }
    };

    // 编辑时直接使用请求中的名称，不再重新组合
    let full_network_name = match &req.name {
        Some(new_name) => new_name.clone(),
        None => current_network.name.clone(),
    };

    // 确定要使用的网络区域ID（来自请求或当前网络）
    let network_region_id = match &req.network_region_id {
        Some(region_id) => region_id,
        None => &current_network.network_region_id,
    };

    // 检查同一网络区域内网络名称是否已被其他网络使用
    let existing_network = match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM network_cidrs WHERE name = $1 AND network_region_id = $2 AND id != $3",
    )
    .bind(&full_network_name)
    .bind(network_region_id)
    .bind(id)
    .fetch_optional(&pool.get_conn())
    .await
    {
        Ok(network) => network,
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "数据库查询错误"));
        }
    };

    if existing_network.is_some() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<Network>::error(
                "同一网络区域内网络名称已存在",
            )),
        );
    }

    // 检查网段唯一性：ipv4或ipv6任一重复即视为重复（排除当前网段）
    if let Some(ref ipv4) = req.ipv4_cidr {
        let existing_ipv4: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM network_cidrs WHERE ipv4_cidr = CAST($1 AS CIDR) AND id != $2",
        )
        .bind(ipv4)
        .bind(id)
        .fetch_optional(&pool.get_conn())
        .await
        .unwrap_or(None);

        if existing_ipv4.is_some() {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                "IPv4网段已被其他网段使用，网段不能重复",
            )));
        }
    }

    if let Some(ref ipv6) = req.ipv6_cidr {
        let existing_ipv6: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM network_cidrs WHERE ipv6_cidr = CAST($1 AS CIDR) AND id != $2",
        )
        .bind(ipv6)
        .bind(id)
        .fetch_optional(&pool.get_conn())
        .await
        .unwrap_or(None);

        if existing_ipv6.is_some() {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                "IPv6网段已被其他网段使用，网段不能重复",
            )));
        }
    }

    let ipv4_dns_array: Option<Vec<String>> = req.ipv4_dns.clone();
    let ipv6_dns_array: Option<Vec<String>> = req.ipv6_dns.clone();

    if let Err(err) = sqlx::query(
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
    .execute(&pool.get_conn())
    .await
    {
        return Ok(crate::utils::handle_db_error(err, "数据库更新错误"));
    }

    // 返回更新后的网络
    let network = match sqlx::query(
        r"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, 
           (SELECT json_agg(host(d)) FROM unnest(n.ipv4_dns) AS d) as ipv4_dns,
           (SELECT json_agg(host(d)) FROM unnest(n.ipv6_dns) AS d) as ipv6_dns,
           NULL as gateway, NULL as dns, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
           FROM network_cidrs n 
           JOIN network_regions nt ON n.network_region_id = nt.id 
           WHERE n.id = $1"
    ).bind(id)
    .fetch_one(&pool.get_conn()).await {
        Ok(row) => Network {
            id: row.get(0),
            name: row.get(1),
            network_region_id: row.get(2),
            network_region: row.get(3),
            ipv4_cidr: row.get(4),
            ipv6_cidr: row.get(5),
            ipv4_gateway: row.get(6),
            ipv6_gateway: row.get(7),
            ipv4_dns: row.get::<Option<serde_json::Value>, _>(8).and_then(|v| serde_json::from_value(v).ok()),
            ipv6_dns: row.get::<Option<serde_json::Value>, _>(9).and_then(|v| serde_json::from_value(v).ok()),
            description: row.get(12),
            created_at: row.get(13),
            updated_at: row.get(14),
        },
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询更新后的网络失败"));
        }
    };

    // 记录操作日志
    let details = serde_json::json!({
        "name": network.name,
        "network_region": network.network_region,
        "ipv4_cidr": network.ipv4_cidr,
        "ipv6_cidr": network.ipv6_cidr
    });
    let _ = log_system_operation(
        &pool.get_conn(),
        &http_req,
        config.get_ref(),
        "update",
        "network",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<Network>::success(network, "网络更新成功")))
}

// 删除网络
pub async fn delete_network(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 检查网络是否存在
    let existing_network =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_cidrs WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool.get_conn())
            .await
        {
            Ok(network) => network,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询网络失败"));
            }
        };

    if existing_network.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<Network>::error("网络未找到")));
    }

    // 检查是否有IP管理关联
    let ip_count = match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM ips WHERE network_id = $1",
    )
    .bind(id)
    .fetch_one(&pool.get_conn())
    .await
    {
        Ok(count) => count,
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询IP关联失败"));
        }
    };

    if ip_count > 0 {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "无法删除网络：该网络已被IP管理关联，请先解除关联关系",
        )));
    }

    // 检查是否有房间网络关联
    let room_network_count = match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM room_networks WHERE network_id = $1",
    )
    .bind(id)
    .fetch_one(&pool.get_conn())
    .await
    {
        Ok(count) => count,
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询房间关联失败"));
        }
    };

    if room_network_count > 0 {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "无法删除网络：该网络已被房间关联，请先解除关联关系",
        )));
    }

    // 检查是否有机柜通过房间间接关联到该网络
    let cabinet_network_count = match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM cabinets c JOIN room_networks rn ON c.room_id = rn.room_id WHERE rn.network_id = $1"
    )
        .bind(id)
        .fetch_one(&pool.get_conn())
        .await
    {
        Ok(count) => count,
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询机柜关联失败"));
        }
    };

    if cabinet_network_count > 0 {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "无法删除网络：该网络已被机柜通过房间间接关联，请先解除关联关系",
        )));
    }

    // 删除网络
    if let Err(err) = sqlx::query("DELETE FROM network_cidrs WHERE id = $1")
        .bind(id)
        .execute(&pool.get_conn())
        .await
    {
        return Ok(crate::utils::handle_db_error(err, "删除网络失败"));
    }

    // 记录操作日志
    let details = serde_json::json!({
        "network_id": id.to_string()
    });
    let _ = log_system_operation(
        &pool.get_conn(),
        &http_req,
        config.get_ref(),
        "delete",
        "network",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "网络删除成功")))
}

// 网络区域相关路由
// 获取所有网络区域（支持搜索和分页）
pub async fn get_network_regions(
    pool: web::Data<DbPool>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    let page: i64 = query.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = query
        .get("page_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let search = query.get("search").cloned().unwrap_or_default();
    let offset = (page - 1) * page_size;

    let total: i64 = if search.is_empty() {
        match sqlx::query_scalar("SELECT COUNT(*) FROM network_regions")
            .fetch_one(&pool.get_conn())
            .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询网络区域数量失败"));
            }
        }
    } else {
        let pattern = format!("%{search}%");
        match sqlx::query_scalar(
            "SELECT COUNT(*) FROM network_regions WHERE name ILIKE $1 OR description ILIKE $1",
        )
        .bind(&pattern)
        .fetch_one(&pool.get_conn())
        .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询网络区域数量失败"));
            }
        }
    };

    let network_regions = if search.is_empty() {
        match sqlx::query_as::<_, NetworkRegion>(
            "SELECT id, name, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(&pool.get_conn())
        .await
        {
            Ok(network_regions) => network_regions,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询网络区域列表失败"));
            }
        }
    } else {
        let pattern = format!("%{search}%");
        match sqlx::query_as::<_, NetworkRegion>(
            "SELECT id, name, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions WHERE name ILIKE $1 OR description ILIKE $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(&pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&pool.get_conn())
        .await
        {
            Ok(network_regions) => network_regions,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询网络区域列表失败"));
            }
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": network_regions,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "网络区域获取成功",
    )))
}

// 创建网络区域
pub async fn create_network_region(
    pool: web::Data<DbPool>,
    req: web::Json<NetworkRegionCreate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 验证创建网络区域请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {e:?}")))
        );
    }

    // 检查网络区域名称是否已存在
    match sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_regions WHERE name = $1")
        .bind(&req.name)
        .fetch_optional(&pool.get_conn())
        .await
    {
        Ok(Some(_)) => {
            return Ok(HttpResponse::BadRequest()
                .json(ApiResponse::<NetworkRegion>::error("网络区域名称已存在")));
        }
        Ok(None) => (),
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询网络区域失败"));
        }
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    // 创建网络区域
    if let Err(err) = sqlx::query(
        "INSERT INTO network_regions (id, name, description, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&pool.get_conn())
    .await
    {
        return Ok(crate::utils::handle_db_error(err, "创建网络区域失败"));
    }

    // 返回创建的网络区域
    let network_region = NetworkRegion {
        id,
        name: req.name.clone(),
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    // 记录操作日志
    let details = serde_json::json!({
        "name": network_region.name,
        "description": network_region.description
    });
    let _ = log_system_operation(
        &pool.get_conn(),
        &http_req,
        config.get_ref(),
        "create",
        "network_region",
        &id,
        &details,
        true,
    )
    .await;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<NetworkRegion>::success(
            network_region,
            "网络区域创建成功",
        )),
    )
}

// 获取单个网络区域
pub async fn get_network_region(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = *id_path;

    let network_region = match sqlx::query_as::<_, NetworkRegion>(
        "SELECT id, name, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions WHERE id = $1"
    ).bind(id)
    .fetch_optional(&pool.get_conn()).await {
        Ok(Some(network_type)) => network_type,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<NetworkRegion>::error("网络区域未找到")));
        },
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询网络区域失败"));
        }
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<NetworkRegion>::success(
            network_region,
            "网络区域获取成功",
        )),
    )
}

// 更新网络区域
pub async fn update_network_region(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    req: web::Json<NetworkRegionUpdate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 验证更新网络区域请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "Validation error: {e:?}"
            ))),
        );
    }

    // 检查网络区域是否存在
    match sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_regions WHERE id = $1")
        .bind(id)
        .fetch_optional(&pool.get_conn())
        .await
    {
        Ok(Some(_)) => (),
        Ok(None) => {
            return Ok(HttpResponse::NotFound()
                .json(ApiResponse::<NetworkRegion>::error("网络区域未找到")));
        }
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询网络区域失败"));
        }
    }

    // 检查新名称是否已被其他网络区域使用
    if let Some(name) = &req.name {
        match sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM network_regions WHERE name = $1 AND id != $2",
        )
        .bind(name)
        .bind(id)
        .fetch_optional(&pool.get_conn())
        .await
        {
            Ok(Some(_)) => {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<NetworkRegion>::error("网络区域名称已存在")));
            }
            Ok(None) => (),
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询网络区域失败"));
            }
        }
    }

    let now = Utc::now();

    // 更新网络区域信息
    if let Err(err) = sqlx::query(
        "UPDATE network_regions SET 
         name = COALESCE($1, name), 
         description = COALESCE($2, description), 
         updated_at = $3 
         WHERE id = $4",
    )
    .bind(&req.name)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&pool.get_conn())
    .await
    {
        return Ok(crate::utils::handle_db_error(err, "更新网络区域失败"));
    }

    // 返回更新后的网络区域
    let network_region = match sqlx::query_as::<_, NetworkRegion>(
        "SELECT id, name, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions WHERE id = $1"
    ).bind(id)
    .fetch_one(&pool.get_conn()).await {
        Ok(network_region) => network_region,
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询更新后的网络区域失败"));
        }
    };

    // 记录操作日志
    let details = serde_json::json!({
        "name": network_region.name,
        "description": network_region.description
    });
    let _ = log_system_operation(
        &pool.get_conn(),
        &http_req,
        config.get_ref(),
        "update",
        "network_region",
        &id,
        &details,
        true,
    )
    .await;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<NetworkRegion>::success(
            network_region,
            "网络区域更新成功",
        )),
    )
}

// 删除网络区域
pub async fn delete_network_region(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 检查网络区域是否存在
    match sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_regions WHERE id = $1")
        .bind(id)
        .fetch_optional(&pool.get_conn())
        .await
    {
        Ok(Some(_)) => (),
        Ok(None) => {
            return Ok(HttpResponse::NotFound()
                .json(ApiResponse::<NetworkRegion>::error("网络区域未找到")));
        }
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询网络区域失败"));
        }
    }

    // 检查是否有网络关联到该区域
    let network_count = match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM network_cidrs WHERE network_region_id = $1",
    )
    .bind(id)
    .fetch_one(&pool.get_conn())
    .await
    {
        Ok(count) => count,
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询网络关联失败"));
        }
    };

    if network_count > 0 {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "无法删除网络区域：该区域下存在网络，请先删除相关网络",
        )));
    }

    // 删除网络区域
    if let Err(err) = sqlx::query("DELETE FROM network_regions WHERE id = $1")
        .bind(id)
        .execute(&pool.get_conn())
        .await
    {
        return Ok(crate::utils::handle_db_error(err, "删除网络区域失败"));
    }

    // 记录操作日志
    let details = serde_json::json!({
        "network_region_id": id.to_string()
    });
    let _ = log_system_operation(
        &pool.get_conn(),
        &http_req,
        config.get_ref(),
        "delete",
        "network_region",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "网络区域删除成功")))
}
