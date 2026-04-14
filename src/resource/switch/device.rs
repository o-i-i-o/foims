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
    let offset = (page - 1) * page_size;

    let search_pattern = if !search.is_empty() {
        Some(format!("%{}%", search))
    } else {
        None
    };

    let total: i64 = match if let Some(ref pattern) = search_pattern {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM switches_with_details WHERE name ILIKE $1 OR location ILIKE $1 OR model ILIKE $1"
        )
        .bind(pattern)
        .fetch_one(pool.get_conn())
        .await
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM switches_with_details")
            .fetch_one(pool.get_conn())
            .await
    } {
        Ok(t) => t,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("数据库查询失败: {}", e))));
        }
    };

    let switches = if let Some(ref pattern) = search_pattern {
        sqlx::query_as::<_, SwitchWithParent>(
            r#"SELECT 
                id, name, network_region_id, network_id, model, vendor,
                location, snmp_version, 
                snmp_community,
                snmp_username, snmp_auth_protocol, 
                snmp_auth_password,
                snmp_priv_protocol, 
                snmp_priv_password,
                snmp_port,
                parent_switch_id, parent_switch_name,
                parent_port_id, parent_port_number,
                cabinet_id, cabinet_name,
                start_u, end_u,
                description,
                device_type,
                ip_address,
                mac_address,
                created_at, updated_at
            FROM switches_with_details
            WHERE name ILIKE $1 OR location ILIKE $1 OR model ILIKE $1 OR ip_address ILIKE $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3"#,
        )
        .bind(pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
    } else {
        sqlx::query_as::<_, SwitchWithParent>(
            r#"SELECT 
                id, name, network_region_id, network_id, model, vendor,
                location, snmp_version, 
                snmp_community,
                snmp_username, snmp_auth_protocol, 
                snmp_auth_password,
                snmp_priv_protocol, 
                snmp_priv_password,
                snmp_port,
                parent_switch_id, parent_switch_name,
                parent_port_id, parent_port_number,
                cabinet_id, cabinet_name,
                start_u, end_u,
                description,
                device_type,
                ip_address,
                mac_address,
                created_at, updated_at
            FROM switches_with_details
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2"#,
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
    };

    match switches {
        Ok(mut data) => {
            for switch in &mut data {
                decrypt_snmp_fields(switch);
            }
            Ok(HttpResponse::Ok().json(ApiResponse::success(
                serde_json::json!({
                    "items": data,
                    "total": total,
                    "page": page,
                    "page_size": page_size,
                    "total_pages": (total + page_size - 1) / page_size
                }),
                "获取交换机列表成功",
            )))
        }
        Err(e) => Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "获取交换机列表失败: {}",
                e
            ))),
        ),
    }
}

pub async fn get_switch(pool: web::Data<DbPool>, path: web::Path<Uuid>) -> Result<HttpResponse> {
    let id = path.into_inner();

    let switch = sqlx::query_as::<_, SwitchWithParent>(
        r#"SELECT 
            id, name, network_region_id, network_id, model, vendor,
            location, snmp_version, 
            snmp_community,
            snmp_username, snmp_auth_protocol, 
            snmp_auth_password,
            snmp_priv_protocol, 
            snmp_priv_password,
            snmp_port,
            parent_switch_id, parent_switch_name,
            parent_port_id, parent_port_number,
            cabinet_id, cabinet_name,
            start_u, end_u,
            description,
            device_type,
            ip_address,
            mac_address,
            created_at, updated_at
        FROM switches_with_details
        WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool.get_conn())
    .await;

    match switch {
        Ok(Some(mut data)) => {
            decrypt_snmp_fields(&mut data);

            let ips = sqlx::query(
                r#"SELECT 
                    m.id, m.switch_id, m.device_type, m.network_id, 
                    host(m.ip_address) as ip_address,
                    m.ip_version, m.mac_address, m.hostname,
                    m.status, m.last_seen, m.created_at, m.updated_at,
                    n.network_region_id, nr.name as network_region
                FROM ip_managers m
                LEFT JOIN network_cidrs n ON m.network_id = n.id
                LEFT JOIN network_regions nr ON n.network_region_id = nr.id
                WHERE m.switch_id = $1 AND m.device_type = 'switch'
                ORDER BY m.ip_address"#,
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
                        "switch_id": row.get::<Option<Uuid>, _>(1),
                        "device_type": row.get::<Option<String>, _>(2),
                        "network_id": row.get::<Uuid, _>(3),
                        "ip_address": row.get::<String, _>(4),
                        "ip_version": row.get::<i16, _>(5),
                        "mac_address": row.get::<Option<String>, _>(6),
                        "hostname": row.get::<Option<String>, _>(7),
                        "status": row.get::<String, _>(8),
                        "last_seen": row.get::<DateTime<Utc>, _>(9),
                        "created_at": row.get::<DateTime<Utc>, _>(10),
                        "updated_at": row.get::<DateTime<Utc>, _>(11),
                        "network_region_id": row.get::<Option<Uuid>, _>(12),
                        "network_region": row.get::<Option<String>, _>(13)
                    })
                })
                .collect();

            let mut response_data = serde_json::to_value(data).unwrap();
            response_data["ips"] = serde_json::to_value(ips_json).unwrap();

            Ok(HttpResponse::Ok().json(ApiResponse::success(response_data, "获取交换机成功")))
        }
        Ok(None) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("获取交换机失败: {}", e)))),
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
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证失败: {}", e)))
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
    let first_ip = &ips[0];

    let network_region_id = if let Some(nrid) = req.network_region_id {
        Some(nrid)
    } else if let Some(nrid) = first_ip.network_region_id {
        Some(nrid)
    } else {
        match sqlx::query_scalar::<_, Uuid>(
            "SELECT network_region_id FROM network_cidrs WHERE id = $1",
        )
        .bind(first_ip.network_id)
        .fetch_optional(pool.get_conn())
        .await
        {
            Ok(Some(id)) => Some(id),
            _ => {
                return Ok(
                    HttpResponse::BadRequest().json(ApiResponse::<()>::error("无法获取网络区域ID"))
                );
            }
        }
    };

    let network_id = first_ip.network_id;

    let id = Uuid::new_v4();
    let now = Utc::now();

    let encrypted_snmp_community = req.snmp_community.as_ref().map(|c| encrypt_password(c));
    let encrypted_snmp_auth_password = req.snmp_auth_password.as_ref().map(|p| encrypt_password(p));
    let encrypted_snmp_priv_password = req.snmp_priv_password.as_ref().map(|p| encrypt_password(p));

    let result = sqlx::query(
        r#"INSERT INTO switches (
            id, name, network_region_id, network_id, model, vendor,
            location, snmp_version, snmp_community, snmp_username,
            snmp_auth_protocol, snmp_auth_password, snmp_priv_protocol,
            snmp_priv_password, snmp_port, parent_switch_id, parent_port_id,
            cabinet_id, start_u, end_u, description, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23)"#
    )
    .bind(id)
    .bind(&req.name)
    .bind(network_region_id)
    .bind(network_id)
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
    .bind(req.cabinet_id)
    .bind(req.start_u)
    .bind(req.end_u)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(pool.get_conn())
    .await;

    match result {
        Ok(_) => {
            let position_id = if req.cabinet_id.is_some() {
                let pos_id = Uuid::new_v4();
                if let Err(e) = sqlx::query(
                    "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
                )
                .bind(pos_id)
                .bind(&req.name)
                .bind(req.cabinet_id)
                .bind(req.start_u.unwrap_or(1))
                .bind(req.end_u.unwrap_or(1))
                .bind(&req.description)
                .bind(now)
                .bind(now)
                .execute(pool.get_conn())
                .await
                {
                    tracing::error!("创建交换机关联机位记录失败: {}", e);
                    None
                } else {
                    Some(pos_id)
                }
            } else {
                None
            };

            for ip in ips {
                let ip_exists = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM ip_managers WHERE ip_address = CAST($1 AS INET))",
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

                let ip_version: i16 = if ip.ip_address.contains(":") { 6 } else { 4 };

                let ip_manager_id = Uuid::new_v4();
                if let Err(e) = sqlx::query(
                    "INSERT INTO ip_managers (id, switch_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, position_id, status, last_seen, created_at, updated_at) 
                     VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, $8, $9, $10, $11, $12, $13, $14)"
                )
                .bind(ip_manager_id)
                .bind(id)
                .bind(ip.device_type.as_ref().unwrap_or(&"switch".to_string()))
                .bind(ip.network_id)
                .bind(&ip.ip_address)
                .bind(ip_version)
                .bind(&ip.mac_address)
                .bind(&ip.hostname)
                .bind(position_id)
                .bind("active")
                .bind(now)
                .bind(now)
                .bind(now)
                .execute(pool.get_conn())
                .await
                {
                    tracing::error!("创建交换机IP记录失败: {}", e);
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error(format!("创建IP记录失败: {}", e))));
                }
            }

            let switch = sqlx::query_as::<_, Switch>(
                r#"SELECT 
                    id, name, network_region_id, network_id,
                    model, vendor, 
                    location, snmp_version, 
                    snmp_community, 
                    snmp_username, snmp_auth_protocol, 
                    snmp_auth_password, 
                    snmp_priv_protocol, 
                    snmp_priv_password, 
                    snmp_port, 
                    parent_switch_id, parent_port_id, 
                    cabinet_id, start_u, end_u, 
                    description, created_at, updated_at 
                FROM switches WHERE id = $1"#,
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
                        "创建交换机成功但查询失败: {}",
                        e
                    ))),
                ),
            }
        }
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建交换机失败: {}", e)))),
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
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证失败: {}", e)))
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

    let encrypted_snmp_community = req.snmp_community.as_ref().map(|c| encrypt_password(c));
    let encrypted_snmp_auth_password = req.snmp_auth_password.as_ref().map(|p| encrypt_password(p));
    let encrypted_snmp_priv_password = req.snmp_priv_password.as_ref().map(|p| encrypt_password(p));

    let result = sqlx::query(
        r#"UPDATE switches SET
            name = COALESCE($1, name),
            network_region_id = COALESCE($2, network_region_id),
            network_id = COALESCE($3, network_id),
            model = COALESCE($4, model),
            vendor = COALESCE($5, vendor),
            location = COALESCE($6, location),
            snmp_version = COALESCE($7, snmp_version),
            snmp_community = COALESCE($8, snmp_community),
            snmp_username = COALESCE($9, snmp_username),
            snmp_auth_protocol = COALESCE($10, snmp_auth_protocol),
            snmp_auth_password = COALESCE($11, snmp_auth_password),
            snmp_priv_protocol = COALESCE($12, snmp_priv_protocol),
            snmp_priv_password = COALESCE($13, snmp_priv_password),
            snmp_port = COALESCE($14, snmp_port),
            parent_switch_id = $15,
            parent_port_id = $16,
            cabinet_id = $17,
            start_u = $18,
            end_u = $19,
            description = COALESCE($20, description),
            updated_at = $21
        WHERE id = $22"#,
    )
    .bind(&req.name)
    .bind(req.network_region_id)
    .bind(req.network_id)
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
    .bind(req.cabinet_id)
    .bind(req.start_u)
    .bind(req.end_u)
    .bind(&req.description)
    .bind(now)
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
                    "SELECT position_id FROM ip_managers WHERE switch_id = $1 AND position_id IS NOT NULL LIMIT 1"
                )
                .bind(id)
                .fetch_optional(pool.get_conn())
                .await
                .unwrap_or(None);

                if let Some(pos_id) = existing_position_id {
                    let mut updates = Vec::new();
                    let mut param_idx = 2u32;

                    if req.name.is_some() {
                        updates.push(format!("name = ${}", param_idx));
                        param_idx += 1;
                    }
                    if req.cabinet_id.is_some() {
                        updates.push(format!("cabinet_id = ${}", param_idx));
                        param_idx += 1;
                    }
                    if req.start_u.is_some() {
                        updates.push(format!("start_u = ${}", param_idx));
                        param_idx += 1;
                    }
                    if req.end_u.is_some() {
                        updates.push(format!("end_u = ${}", param_idx));
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
                        "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
                    )
                    .bind(pos_id)
                    .bind(req.name.as_ref().unwrap_or(&String::new()))
                    .bind(req.cabinet_id)
                    .bind(req.start_u.unwrap_or(1))
                    .bind(req.end_u.unwrap_or(1))
                    .bind(&req.description)
                    .bind(now)
                    .bind(now)
                    .execute(pool.get_conn())
                    .await
                    {
                        tracing::error!("创建交换机关联机位记录失败: {}", e);
                    } else if let Err(e) = sqlx::query(
                        "UPDATE ip_managers SET position_id = $1 WHERE switch_id = $2"
                    )
                    .bind(pos_id)
                    .bind(id)
                    .execute(pool.get_conn())
                    .await
                    {
                        tracing::error!("关联交换机IP到机位失败: {}", e);
                    }
                }
            }

            if let Some(ips) = &req.ips {
                let position_id: Option<Uuid> = sqlx::query_scalar(
                    "SELECT position_id FROM ip_managers WHERE switch_id = $1 AND position_id IS NOT NULL LIMIT 1"
                )
                .bind(id)
                .fetch_optional(pool.get_conn())
                .await
                .unwrap_or(None);

                if let Err(e) = sqlx::query("DELETE FROM ip_managers WHERE switch_id = $1")
                    .bind(id)
                    .execute(pool.get_conn())
                    .await
                {
                    tracing::error!("删除交换机旧IP记录失败: {}", e);
                }

                for ip in ips {
                    let ip_exists = sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS(SELECT 1 FROM ip_managers WHERE ip_address = CAST($1 AS INET) AND switch_id != $2)",
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

                    let ip_version: i16 = if ip.ip_address.contains(":") { 6 } else { 4 };

                    let ip_manager_id = Uuid::new_v4();
                    if let Err(e) = sqlx::query(
                        "INSERT INTO ip_managers (id, switch_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, position_id, status, last_seen, created_at, updated_at) 
                         VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, $8, $9, $10, $11, $12, $13, $14)"
                    )
                    .bind(ip_manager_id)
                    .bind(id)
                    .bind(ip.device_type.as_ref().unwrap_or(&"switch".to_string()))
                    .bind(ip.network_id)
                    .bind(&ip.ip_address)
                    .bind(ip_version)
                    .bind(&ip.mac_address)
                    .bind(&ip.hostname)
                    .bind(position_id)
                    .bind("active")
                    .bind(now)
                    .bind(now)
                    .bind(now)
                    .execute(pool.get_conn())
                    .await
                    {
                        tracing::error!("更新交换机IP记录失败: {}", e);
                        return Ok(HttpResponse::InternalServerError()
                            .json(ApiResponse::<()>::error(format!("创建IP记录失败: {}", e))));
                    }
                }
            }

            let switch = sqlx::query_as::<_, Switch>(
                r#"SELECT 
                    id, name, network_region_id, network_id,
                    model, vendor, 
                    location, snmp_version, 
                    snmp_community, 
                    snmp_username, snmp_auth_protocol, 
                    snmp_auth_password, 
                    snmp_priv_protocol, 
                    snmp_priv_password, 
                    snmp_port, 
                    parent_switch_id, parent_port_id, 
                    cabinet_id, start_u, end_u, 
                    description, created_at, updated_at 
                FROM switches WHERE id = $1"#,
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
                        "更新交换机成功但查询失败: {}",
                        e
                    ))),
                ),
            }
        }
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("更新交换机失败: {}", e)))),
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

    if let Err(e) = sqlx::query("DELETE FROM ip_managers WHERE switch_id = $1")
        .bind(id)
        .execute(pool.get_conn())
        .await
    {
        tracing::error!("删除交换机IP记录失败: {}", e);
    }

    let linked_position_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT p.id FROM positions p JOIN ip_managers im ON im.position_id = p.id WHERE im.switch_id = $1"
    )
    .bind(id)
    .fetch_all(pool.get_conn())
    .await
    .unwrap_or_default();

    for pos_id in &linked_position_ids {
        if let Err(e) = sqlx::query("DELETE FROM positions WHERE id = $1")
            .bind(pos_id)
            .execute(pool.get_conn())
            .await
        {
            tracing::error!("删除交换机关联机位失败: {}", e);
        }
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
            .json(ApiResponse::<()>::error(format!("删除交换机失败: {}", e)))),
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
                Ok(None) => break,
                Err(_) => break,
            };

        match next_parent {
            Some(id) => current = id,
            None => break,
        }
    }

    Ok(false)
}
