use actix_web::{HttpRequest, HttpResponse, Result, web};
use chrono::{DateTime, Utc};
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

use super::snmp::decrypt_snmp_fields;
use crate::config::Config;
use crate::crypto::encrypt_password;
use crate::db::DbPool;
use crate::models::{ApiResponse, Switch, SwitchCreate, SwitchUpdate, SwitchWithParent};
use crate::utils::{DEFAULT_PAGE, log_system_operation};

pub async fn get_switches(
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
    let name_filter = query.get("name").cloned().unwrap_or_default();
    let ip_filter = query.get("ip_address").cloned().unwrap_or_default();
    let model_filter = query.get("model").cloned().unwrap_or_default();
    let offset = (page - 1) * page_size;

    let has_filters = !search.is_empty()
        || !name_filter.is_empty()
        || !ip_filter.is_empty()
        || !model_filter.is_empty();

    let total: i64 = if has_filters {
        let mut conditions = Vec::new();
        let mut param_count = 1;

        if !search.is_empty() {
            conditions.push(format!(
                "(name ILIKE ${param_count} OR location ILIKE ${param_count} OR model ILIKE ${param_count} OR ip_address ILIKE {param_count})"
            ));
            param_count += 1;
        }

        if !name_filter.is_empty() {
            conditions.push(format!("name ILIKE ${param_count}"));
            param_count += 1;
        }

        if !ip_filter.is_empty() {
            conditions.push(format!("ip_address ILIKE ${param_count}"));
            param_count += 1;
        }

        if !model_filter.is_empty() {
            conditions.push(format!("model ILIKE ${param_count}"));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let count_query = format!(
            "SELECT COUNT(*) FROM switches_with_details {where_clause}"
        );

        let mut count_sql = sqlx::query_scalar(&count_query);

        if !search.is_empty() {
            let pattern = format!("%{search}%");
            count_sql = count_sql.bind(pattern);
        }

        if !name_filter.is_empty() {
            let pattern = format!("%{name_filter}%");
            count_sql = count_sql.bind(pattern);
        }

        if !ip_filter.is_empty() {
            let pattern = format!("%{ip_filter}%");
            count_sql = count_sql.bind(pattern);
        }

        if !model_filter.is_empty() {
            let pattern = format!("%{model_filter}%");
            count_sql = count_sql.bind(pattern);
        }

        match count_sql.fetch_one(pool.get_conn()).await {
            Ok(t) => t,
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("数据库查询失败: {e}"))));
            }
        }
    } else {
        match sqlx::query_scalar("SELECT COUNT(*) FROM switches_with_details")
            .fetch_one(pool.get_conn())
            .await
        {
            Ok(t) => t,
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("数据库查询失败: {e}"))));
            }
        }
    };

    let switches_result = if has_filters {
        let mut conditions = Vec::new();
        let mut param_count = 1;

        if !search.is_empty() {
            conditions.push(format!(
                "(name ILIKE ${param_count} OR location ILIKE ${param_count} OR model ILIKE ${param_count} OR ip_address ILIKE {param_count})"
            ));
            param_count += 1;
        }

        if !name_filter.is_empty() {
            conditions.push(format!("name ILIKE ${param_count}"));
            param_count += 1;
        }

        if !ip_filter.is_empty() {
            conditions.push(format!("ip_address ILIKE ${param_count}"));
            param_count += 1;
        }

        if !model_filter.is_empty() {
            conditions.push(format!("model ILIKE ${param_count}"));
            param_count += 1;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let data_query = format!(
            r"SELECT 
                id, name, model, vendor,
                location, snmp_version, 
                snmp_community,
                snmp_username, snmp_auth_protocol, 
                snmp_auth_password,
                snmp_priv_protocol, 
                snmp_priv_password,
                snmp_port,
                parent_switch_id, parent_switch_name,
                parent_port_id, parent_port_number,
                position_id,
                cabinet_id, cabinet_name,
                start_u, end_u,
                position_network_id, network_region_id,
                description,
                device_type,
                ip_address,
                mac_address,
                created_at, updated_at
            FROM switches_with_details
            {}
            ORDER BY created_at DESC
            LIMIT ${} OFFSET ${}",
            where_clause,
            param_count,
            param_count + 1
        );

        let mut data_sql = sqlx::query_as::<_, SwitchWithParent>(&data_query);

        if !search.is_empty() {
            let pattern = format!("%{search}%");
            data_sql = data_sql.bind(pattern);
        }

        if !name_filter.is_empty() {
            let pattern = format!("%{name_filter}%");
            data_sql = data_sql.bind(pattern);
        }

        if !ip_filter.is_empty() {
            let pattern = format!("%{ip_filter}%");
            data_sql = data_sql.bind(pattern);
        }

        if !model_filter.is_empty() {
            let pattern = format!("%{model_filter}%");
            data_sql = data_sql.bind(pattern);
        }

        data_sql = data_sql.bind(page_size).bind(offset);

        data_sql.fetch_all(pool.get_conn()).await
    } else {
        sqlx::query_as::<_, SwitchWithParent>(
            r"SELECT 
                id, name, model, vendor,
                location, snmp_version, 
                snmp_community,
                snmp_username, snmp_auth_protocol, 
                snmp_auth_password,
                snmp_priv_protocol, 
                snmp_priv_password,
                snmp_port,
                parent_switch_id, parent_switch_name,
                parent_port_id, parent_port_number,
                position_id,
                cabinet_id, cabinet_name,
                start_u, end_u,
                position_network_id, network_region_id,
                description,
                device_type,
                ip_address,
                mac_address,
                created_at, updated_at
            FROM switches_with_details
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2",
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
    };

    match switches_result {
        Ok(mut data) => {
            for switch in &mut data {
                let has_community = switch.snmp_community.is_some();
                let has_auth_password = switch.snmp_auth_password.is_some();
                let has_priv_password = switch.snmp_priv_password.is_some();

                decrypt_snmp_fields(switch);

                if has_community {
                    switch.snmp_community = Some("••••••••".to_string());
                }
                if has_auth_password {
                    switch.snmp_auth_password = Some("••••••••".to_string());
                }
                if has_priv_password {
                    switch.snmp_priv_password = Some("••••••••".to_string());
                }
            }
            let total_pages = (total + page_size - 1) / page_size;
            Ok(HttpResponse::Ok().json(ApiResponse::success(
                serde_json::json!({
                    "items": data,
                    "total": total,
                    "page": page,
                    "page_size": page_size,
                    "total_pages": total_pages
                }),
                "获取交换机列表成功",
            )))
        }
        Err(e) => Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "获取交换机列表失败: {e}"
            ))),
        ),
    }
}

pub async fn get_switch(pool: web::Data<DbPool>, path: web::Path<Uuid>) -> Result<HttpResponse> {
    let id = path.into_inner();

    let switch = sqlx::query_as::<_, SwitchWithParent>(
        r"SELECT 
            id, name, model, vendor,
            location, snmp_version, 
            snmp_community,
            snmp_username, snmp_auth_protocol, 
            snmp_auth_password,
            snmp_priv_protocol, 
            snmp_priv_password,
            snmp_port,
            parent_switch_id, parent_switch_name,
            parent_port_id, parent_port_number,
            position_id,
            cabinet_id, cabinet_name,
            start_u, end_u,
            position_network_id, network_region_id,
            description,
            device_type,
            ip_address,
            mac_address,
            created_at, updated_at
        FROM switches_with_details
        WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool.get_conn())
    .await;

    match switch {
        Ok(Some(mut data)) => {
            let has_community = data.snmp_community.is_some();
            let has_auth_password = data.snmp_auth_password.is_some();
            let has_priv_password = data.snmp_priv_password.is_some();

            decrypt_snmp_fields(&mut data);

            if has_community {
                data.snmp_community = Some("••••••••".to_string());
            }
            if has_auth_password {
                data.snmp_auth_password = Some("••••••••".to_string());
            }
            if has_priv_password {
                data.snmp_priv_password = Some("••••••••".to_string());
            }

            let ips = sqlx::query(
                r"SELECT 
                    m.id, m.device_type, m.network_id, 
                    host(m.ip_address) as ip_address,
                    m.ip_version, m.mac_address, m.hostname,
                    m.status, m.last_seen, m.created_at, m.updated_at,
                    n.network_region_id, nr.name as network_region
                FROM ips m
                LEFT JOIN network_cidrs n ON m.network_id = n.id
                LEFT JOIN network_regions nr ON n.network_region_id = nr.id
                WHERE m.position_id = (SELECT id FROM positions WHERE device_type = 'switch' AND device_id = $1)
                ORDER BY m.ip_address",
            )
            .bind(id)
            .fetch_all(pool.get_conn())
            .await
            .unwrap_or_default();

            let ips_json: Vec<serde_json::Value> = ips
                .into_iter()
                .map(|row| {
                    serde_json::json!({
                        "id": row.get::<Uuid, _>(0),
                        "device_type": row.get::<Option<String>, _>(1),
                        "network_id": row.get::<Uuid, _>(2),
                        "ip_address": row.get::<String, _>(3),
                        "ip_version": row.get::<i16, _>(4),
                        "mac_address": row.get::<Option<String>, _>(5),
                        "hostname": row.get::<Option<String>, _>(6),
                        "status": row.get::<String, _>(7),
                        "last_seen": row.get::<DateTime<Utc>, _>(8),
                        "created_at": row.get::<DateTime<Utc>, _>(9),
                        "updated_at": row.get::<DateTime<Utc>, _>(10),
                        "network_region_id": row.get::<Option<Uuid>, _>(11),
                        "network_region": row.get::<Option<String>, _>(12)
                    })
                })
                .collect();

            let mut response_data = serde_json::to_value(data).unwrap();
            response_data["ips"] = serde_json::to_value(ips_json).unwrap();

            Ok(HttpResponse::Ok().json(ApiResponse::success(response_data, "获取交换机成功")))
        }
        Ok(None) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("获取交换机失败: {e}")))),
    }
}

pub async fn create_switch(
    pool: web::Data<DbPool>,
    req: web::Json<SwitchCreate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    if let Err(e) = req.validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证失败: {e}")))
        );
    }

    let has_ips = req.ips.is_some() && !req.ips.as_ref().unwrap().is_empty();

    if !has_ips {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error("交换机必须至少配置一个IP地址")));
    }

    if req.cabinet_id.is_some() && (req.start_u.is_none() || req.end_u.is_none()) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "选择机柜时必须填写起始U位和结束U位",
        )));
    }

    if let (Some(start_u), Some(end_u)) = (req.start_u, req.end_u)
        && start_u > end_u
    {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error("起始U位不能大于结束U位"))
        );
    }

    let ips = req.ips.as_ref().unwrap();

    let id = Uuid::new_v4();
    let now = Utc::now();

    let encrypted_snmp_community = req
        .snmp_community
        .as_ref()
        .filter(|c| !c.is_empty())
        .and_then(|c| encrypt_password(c));
    let encrypted_snmp_auth_password = req
        .snmp_auth_password
        .as_ref()
        .filter(|p| !p.is_empty())
        .and_then(|p| encrypt_password(p));
    let encrypted_snmp_priv_password = req
        .snmp_priv_password
        .as_ref()
        .filter(|p| !p.is_empty())
        .and_then(|p| encrypt_password(p));

    let position_id = Uuid::new_v4();
    if let Err(e) = sqlx::query(
        "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, device_type, device_id, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, 'switch', $7, $8, $9)"
    )
    .bind(position_id)
    .bind(&req.name)
    .bind(req.cabinet_id)
    .bind(req.start_u.unwrap_or(1))
    .bind(req.end_u.unwrap_or(1))
    .bind(&req.description)
    .bind(id)
    .bind(now)
    .bind(now)
    .execute(pool.get_conn())
    .await
    {
        tracing::error!("创建交换机关联机位记录失败: {}", e);
    }

    let result = sqlx::query(
        r"INSERT INTO switches (
            id, name, model, vendor,
            location, snmp_version, snmp_community, snmp_username,
            snmp_auth_protocol, snmp_auth_password, snmp_priv_protocol,
            snmp_priv_password, snmp_port, parent_switch_id, parent_port_id,
            description, created_at, updated_at, position_id, cabinet_id, start_u, end_u
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22)"
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.model)
    .bind(&req.vendor)
    .bind(&req.location)
    .bind(req.snmp_version.as_deref().unwrap_or("v2c"))
    .bind(&encrypted_snmp_community)
    .bind(&req.snmp_username)
    .bind(&req.snmp_auth_protocol)
    .bind(&encrypted_snmp_auth_password)
    .bind(&req.snmp_priv_protocol)
    .bind(&encrypted_snmp_priv_password)
    .bind(req.snmp_port.unwrap_or(161))
    .bind(req.parent_switch_id)
    .bind(req.parent_port_id)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .bind(position_id)
    .bind(req.cabinet_id)
    .bind(req.start_u)
    .bind(req.end_u)
    .execute(pool.get_conn())
    .await;

    match result {
        Ok(_) => {
            for ip in ips {
                let ip_exists = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM ips WHERE ip_address = CAST($1 AS INET))",
                )
                .bind(&ip.ip_address)
                .fetch_one(pool.get_conn())
                .await
                .unwrap_or(false);

                if ip_exists {
                    return Ok(
                        HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                            "IP地址 {} 已存在",
                            ip.ip_address
                        ))),
                    );
                }

                let ip_version: i16 = if ip.ip_address.contains(':') { 6 } else { 4 };

                let ip_manager_id = Uuid::new_v4();
                if let Err(e) = sqlx::query(
                    "INSERT INTO ips (id, device_type, network_id, ip_address, ip_version, mac_address, hostname, position_id, switch_port_id, status, last_seen, created_at, updated_at) 
                     VALUES ($1, $2, $3, CAST($4 AS INET), $5, $6, $7, $8, $9, $10, $11, $12, $13)"
                )
                .bind(ip_manager_id)
                .bind(ip.device_type.as_deref().unwrap_or("switch"))
                .bind(ip.network_id)
                .bind(&ip.ip_address)
                .bind(ip_version)
                .bind(&ip.mac_address)
                .bind(&ip.hostname)
                .bind(position_id)
                .bind(ip.switch_port_id)
                .bind("active")
                .bind(now)
                .bind(now)
                .bind(now)
                .execute(pool.get_conn())
                .await
                {
                    tracing::error!("创建交换机IP记录失败: {}", e);
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error(format!("创建IP记录失败: {e}"))));
                }
            }

            let switch = sqlx::query_as::<_, Switch>(
                r"SELECT 
                    id, name,
                    model, vendor, 
                    location, snmp_version, 
                    snmp_community, 
                    snmp_username, snmp_auth_protocol, 
                    snmp_auth_password, 
                    snmp_priv_protocol, 
                    snmp_priv_password, 
                    snmp_port, 
                    parent_switch_id, parent_port_id, 
                    description, created_at, updated_at,
                    position_id, cabinet_id, start_u, end_u
                FROM switches WHERE id = $1",
            )
            .bind(id)
            .fetch_one(pool.get_conn())
            .await;

            match switch {
                Ok(data) => {
                    let details = serde_json::json!({
                        "name": data.name,
                        "model": data.model,
                        "vendor": data.vendor,
                        "location": data.location,
                        "cabinet_id": data.cabinet_id,
                        "start_u": data.start_u,
                        "end_u": data.end_u,
                        "ip_count": req.ips.as_ref().unwrap_or(&vec![]).len()
                    });
                    let _ = log_system_operation(
                        pool.get_conn(),
                        &http_req,
                        config.get_ref(),
                        "create",
                        "switch",
                        &id,
                        &details,
                        true,
                    )
                    .await;

                    Ok(HttpResponse::Ok().json(ApiResponse::success(data, "创建交换机成功")))
                }
                Err(e) => Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "创建交换机成功但查询失败: {e}"
                    ))),
                ),
            }
        }
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建交换机失败: {e}")))),
    }
}

pub async fn update_switch(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    req: web::Json<SwitchUpdate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = path.into_inner();

    if let Err(e) = req.validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证失败: {e}")))
        );
    }

    let exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM switches WHERE id = $1)")
            .bind(id)
            .fetch_one(pool.get_conn())
            .await
            .unwrap_or(false);

    if !exists {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在")));
    }

    if let Some(parent_switch_id) = req.parent_switch_id {
        if parent_switch_id == id {
            return Ok(HttpResponse::BadRequest()
                .json(ApiResponse::<()>::error("不能将自己设置为上级交换机")));
        }

        if check_switch_cycle(pool.get_conn(), id, parent_switch_id).await? {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                "检测到交换机层级循环引用，无法设置此上级交换机",
            )));
        }
    }

    if req.cabinet_id.is_some() && (req.start_u.is_none() || req.end_u.is_none()) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "选择机柜时必须填写起始U位和结束U位",
        )));
    }

    if let (Some(start_u), Some(end_u)) = (req.start_u, req.end_u)
        && start_u > end_u
    {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error("起始U位不能大于结束U位"))
        );
    }

    let now = Utc::now();

    let encrypted_snmp_community = req
        .snmp_community
        .as_ref()
        .filter(|c| !c.is_empty())
        .and_then(|c| encrypt_password(c));
    let encrypted_snmp_auth_password = req
        .snmp_auth_password
        .as_ref()
        .filter(|p| !p.is_empty())
        .and_then(|p| encrypt_password(p));
    let encrypted_snmp_priv_password = req
        .snmp_priv_password
        .as_ref()
        .filter(|p| !p.is_empty())
        .and_then(|p| encrypt_password(p));

    let result = sqlx::query(
        r"UPDATE switches SET
            name = COALESCE($1, name),
            model = COALESCE($2, model),
            vendor = COALESCE($3, vendor),
            location = COALESCE($4, location),
            snmp_version = COALESCE($5, snmp_version),
            snmp_community = COALESCE($6, snmp_community),
            snmp_username = COALESCE($7, snmp_username),
            snmp_auth_protocol = COALESCE($8, snmp_auth_protocol),
            snmp_auth_password = COALESCE($9, snmp_auth_password),
            snmp_priv_protocol = COALESCE($10, snmp_priv_protocol),
            snmp_priv_password = COALESCE($11, snmp_priv_password),
            snmp_port = COALESCE($12, snmp_port),
            parent_switch_id = $13,
            parent_port_id = $14,
            description = COALESCE($15, description),
            updated_at = $16,
            position_id = COALESCE($17, position_id),
            cabinet_id = $18,
            start_u = $19,
            end_u = $20
        WHERE id = $21",
    )
    .bind(&req.name)
    .bind(&req.model)
    .bind(&req.vendor)
    .bind(&req.location)
    .bind(&req.snmp_version)
    .bind(&encrypted_snmp_community)
    .bind(&req.snmp_username)
    .bind(&req.snmp_auth_protocol)
    .bind(&encrypted_snmp_auth_password)
    .bind(&req.snmp_priv_protocol)
    .bind(&encrypted_snmp_priv_password)
    .bind(req.snmp_port)
    .bind(req.parent_switch_id)
    .bind(req.parent_port_id)
    .bind(&req.description)
    .bind(now)
    .bind(req.position_id)
    .bind(req.cabinet_id)
    .bind(req.start_u)
    .bind(req.end_u)
    .bind(id)
    .execute(pool.get_conn())
    .await;

    match result {
        Ok(_) => {
            if req.cabinet_id.is_some()
                || req.start_u.is_some()
                || req.end_u.is_some()
                || req.name.is_some()
            {
                let existing_position_id: Option<Uuid> = sqlx::query_scalar(
                    "SELECT id FROM positions WHERE device_type = 'switch' AND device_id = $1",
                )
                .bind(id)
                .fetch_optional(pool.get_conn())
                .await
                .unwrap_or(None);

                if let Some(pos_id) = existing_position_id {
                    let mut updates = Vec::new();
                    let mut param_idx = 2u32;

                    if req.name.is_some() {
                        updates.push(format!("name = ${param_idx}"));
                        param_idx += 1;
                    }
                    if req.cabinet_id.is_some() {
                        updates.push(format!("cabinet_id = ${param_idx}"));
                        param_idx += 1;
                    }
                    if req.start_u.is_some() {
                        updates.push(format!("start_u = ${param_idx}"));
                        param_idx += 1;
                    }
                    if req.end_u.is_some() {
                        updates.push(format!("end_u = ${param_idx}"));
                        param_idx += 1;
                    }
                    updates.push("updated_at = $1".to_string());

                    if !updates.is_empty() {
                        let query_str = format!(
                            "UPDATE positions SET {} WHERE id = ${}",
                            updates.join(", "),
                            param_idx
                        );

                        let mut query = sqlx::query(&query_str).bind(now);
                        if let Some(ref name) = req.name {
                            query = query.bind(name);
                        }
                        if let Some(cabinet_id) = req.cabinet_id {
                            query = query.bind(cabinet_id);
                        }
                        if let Some(start_u) = req.start_u {
                            query = query.bind(start_u);
                        }
                        if let Some(end_u) = req.end_u {
                            query = query.bind(end_u);
                        }
                        query = query.bind(pos_id);

                        if let Err(e) = query.execute(pool.get_conn()).await {
                            tracing::error!("同步更新交换机关联机位失败: {}", e);
                        }
                    }
                } else if req.cabinet_id.is_some() {
                    let pos_id = Uuid::new_v4();
                    if let Err(e) = sqlx::query(
                        "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, device_type, device_id, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, 'switch', $7, $8, $9)"
                    )
                    .bind(pos_id)
                    .bind(req.name.as_ref().unwrap_or(&String::new()))
                    .bind(req.cabinet_id)
                    .bind(req.start_u.unwrap_or(1))
                    .bind(req.end_u.unwrap_or(1))
                    .bind(&req.description)
                    .bind(id)
                    .bind(now)
                    .bind(now)
                    .execute(pool.get_conn())
                    .await
                    {
                        tracing::error!("创建交换机关联机位记录失败: {}", e);
                    }
                }
            }

            if let Some(ips) = &req.ips {
                let position_id: Option<Uuid> = sqlx::query_scalar(
                    "SELECT id FROM positions WHERE device_type = 'switch' AND device_id = $1",
                )
                .bind(id)
                .fetch_optional(pool.get_conn())
                .await
                .unwrap_or(None);

                if let Err(e) = sqlx::query("DELETE FROM ips WHERE position_id = (SELECT id FROM positions WHERE device_type = 'switch' AND device_id = $1)")
                    .bind(id)
                    .execute(pool.get_conn())
                    .await
                {
                    tracing::error!("删除交换机旧IP记录失败: {}", e);
                }

                for ip in ips {
                    let ip_exists = sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS(SELECT 1 FROM ips WHERE ip_address = CAST($1 AS INET) AND position_id != (SELECT id FROM positions WHERE device_type = 'switch' AND device_id = $2))",
                    )
                    .bind(&ip.ip_address)
                    .bind(id)
                    .fetch_one(pool.get_conn())
                    .await
                    .unwrap_or(false);

                    if ip_exists {
                        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                            format!("IP地址 {} 已被其他设备使用", ip.ip_address),
                        )));
                    }

                    let ip_version: i16 = if ip.ip_address.contains(':') { 6 } else { 4 };

                    let ip_manager_id = Uuid::new_v4();
                    if let Err(e) = sqlx::query(
                        "INSERT INTO ips (id, device_type, network_id, ip_address, ip_version, mac_address, hostname, position_id, switch_port_id, status, last_seen, created_at, updated_at) 
                         VALUES ($1, $2, $3, CAST($4 AS INET), $5, $6, $7, $8, $9, $10, $11, $12, $13)"
                    )
                    .bind(ip_manager_id)
                    .bind(ip.device_type.as_deref().unwrap_or("switch"))
                    .bind(ip.network_id)
                    .bind(&ip.ip_address)
                    .bind(ip_version)
                    .bind(&ip.mac_address)
                    .bind(&ip.hostname)
                    .bind(position_id)
                    .bind(ip.switch_port_id)
                    .bind("active")
                    .bind(now)
                    .bind(now)
                    .bind(now)
                    .execute(pool.get_conn())
                    .await
                    {
                        tracing::error!("更新交换机IP记录失败: {}", e);
                        return Ok(HttpResponse::InternalServerError()
                            .json(ApiResponse::<()>::error(format!("创建IP记录失败: {e}"))));
                    }
                }
            }

            let switch = sqlx::query_as::<_, Switch>(
                r"SELECT 
                    id, name,
                    model, vendor, 
                    location, snmp_version, 
                    snmp_community, 
                    snmp_username, snmp_auth_protocol, 
                    snmp_auth_password, 
                    snmp_priv_protocol, 
                    snmp_priv_password, 
                    snmp_port, 
                    parent_switch_id, parent_port_id, 
                    description, created_at, updated_at,
                    position_id, cabinet_id, start_u, end_u
                FROM switches WHERE id = $1",
            )
            .bind(id)
            .fetch_one(pool.get_conn())
            .await;

            match switch {
                Ok(data) => {
                    let details = serde_json::json!({
                        "name": data.name,
                        "model": data.model,
                        "vendor": data.vendor,
                        "location": data.location,
                        "cabinet_id": data.cabinet_id,
                        "start_u": data.start_u,
                        "end_u": data.end_u,
                        "ip_count": req.ips.as_ref().unwrap_or(&vec![]).len()
                    });
                    let _ = log_system_operation(
                        pool.get_conn(),
                        &http_req,
                        config.get_ref(),
                        "update",
                        "switch",
                        &id,
                        &details,
                        true,
                    )
                    .await;

                    Ok(HttpResponse::Ok().json(ApiResponse::success(data, "更新交换机成功")))
                }
                Err(e) => Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "更新交换机成功但查询失败: {e}"
                    ))),
                ),
            }
        }
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("更新交换机失败: {e}")))),
    }
}

pub async fn delete_switch(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = path.into_inner();

    let has_children = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM switches WHERE parent_switch_id = $1)",
    )
    .bind(id)
    .fetch_one(pool.get_conn())
    .await
    .unwrap_or(false);

    if has_children {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error("该交换机存在下级交换机，无法删除")));
    }

    if let Err(e) = sqlx::query("DELETE FROM ips WHERE position_id = (SELECT id FROM positions WHERE device_type = 'switch' AND device_id = $1)")
        .bind(id)
        .execute(pool.get_conn())
        .await
    {
        tracing::error!("删除交换机IP记录失败: {}", e);
    }

    if let Err(e) =
        sqlx::query("DELETE FROM positions WHERE device_type = 'switch' AND device_id = $1")
            .bind(id)
            .execute(pool.get_conn())
            .await
    {
        tracing::error!("删除交换机关联机位失败: {}", e);
    }

    let result = sqlx::query("DELETE FROM switches WHERE id = $1")
        .bind(id)
        .execute(pool.get_conn())
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            let details = serde_json::json!({
                "switch_id": id.to_string()
            });
            let _ = log_system_operation(
                pool.get_conn(),
                &http_req,
                config.get_ref(),
                "delete",
                "switch",
                &id,
                &details,
                true,
            )
            .await;

            Ok(HttpResponse::Ok().json(ApiResponse::success((), "删除交换机成功")))
        }
        Ok(_) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("删除交换机失败: {e}")))),
    }
}

async fn check_switch_cycle(pool: &sqlx::PgPool, switch_id: Uuid, parent_id: Uuid) -> Result<bool> {
    let mut current = parent_id;
    let mut visited = std::collections::HashSet::new();

    while !visited.contains(&current) {
        if current == switch_id {
            return Ok(true);
        }
        visited.insert(current);

        let next_parent: Option<Uuid> =
            match sqlx::query_scalar("SELECT parent_switch_id FROM switches WHERE id = $1")
                .bind(current)
                .fetch_optional(pool)
                .await
            {
                Ok(Some(id)) => id,
                Ok(None) | Err(_) => break,
            };

        match next_parent {
            Some(id) => current = id,
            None => break,
        }
    }

    Ok(false)
}
