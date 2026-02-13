use crate::config::Config;
use crate::db::DbPool;
use crate::models::{
    ApiResponse, IpManager, IpManagerCreate, IpManagerUpdate, IpManagerWithNames, Network,
};
use crate::utils::{log_system_operation, send_mac_change_notification};
use actix_web::{HttpRequest, HttpResponse, Result, web};
use chrono::Utc;
use sqlx::Row;
use std::net::IpAddr;
use std::str::FromStr;
use uuid::Uuid;
use validator::Validate;

// 获取所有IP管理（支持搜索和分页）
pub async fn get_ip_managers(
    pool: web::Data<DbPool>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    let search = query.get("search").map(|s| s.as_str()).unwrap_or("");
    let device_type = query.get("device_type").map(|s| s.as_str()).unwrap_or("");
    let status = query.get("status").map(|s| s.as_str()).unwrap_or("");
    let page: i64 = query.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = query.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(100);
    let offset = (page - 1) * page_size;

    let mut conditions = Vec::new();

    if !search.is_empty() {
        conditions.push(format!(
            "(ip_address::TEXT ILIKE '%{}%' OR mac_address ILIKE '%{}%' OR hostname ILIKE '%{}%' OR workstation_name ILIKE '%{}%' OR cabinet_position_name ILIKE '%{}%' OR network_name ILIKE '%{}%')",
            search.replace("'", "''"), search.replace("'", "''"), search.replace("'", "''"), search.replace("'", "''"), search.replace("'", "''"), search.replace("'", "''")
        ));
    }

    if !device_type.is_empty() {
        conditions.push(format!("device_type = '{}'", device_type.replace("'", "''")));
    }

    if !status.is_empty() {
        conditions.push(format!("status = '{}'", status.replace("'", "''")));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_query = format!("SELECT COUNT(*) FROM ip_managers_with_details {}", where_clause);
    let total: i64 = match sqlx::query_scalar(&count_query)
        .fetch_one(pool.get_conn())
        .await
    {
        Ok(count) => count,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "数据库查询错误: {}",
                err
            ))));
        }
    };

    let data_query = format!(
        "SELECT id, workstation_id, position_id, switch_id, switch_port_id, device_type, device_name, network_id, workstation_name, cabinet_position_name, switch_name, network_name, network_region, ip_address::TEXT as ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at FROM ip_managers_with_details {} ORDER BY updated_at DESC LIMIT {} OFFSET {}",
        where_clause, page_size, offset
    );

    let mappings = match sqlx::query_as::<_, IpManagerWithNames>(&data_query)
        .fetch_all(pool.get_conn())
        .await
    {
        Ok(mappings) => mappings,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "数据库查询错误: {}",
                err
            ))));
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "data": mappings,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "IP获取成功",
    )))
}

// 创建IP管理
pub async fn create_ip_manager(
    pool: web::Data<DbPool>,
    req: web::Json<IpManagerCreate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 验证创建IP管理请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {:?}", e)))
        );
    }

    // 验证设备类型和对应的设备ID是否匹配
    let device_type = req.device_type.as_deref().unwrap_or("");
    if !((device_type == "workstation"
        && req.workstation_id.is_some()
        && req.position_id.is_none()
        && req.switch_id.is_none())
        || (device_type == "cabinet_position"
            && req.workstation_id.is_none()
            && req.position_id.is_some()
            && req.switch_id.is_none())
        || (device_type == "switch"
            && req.workstation_id.is_none()
            && req.position_id.is_none()
            && req.switch_id.is_some()))
    {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error("设备类型与设备ID不匹配"))
        );
    }

    // 检查IP地址是否已存在于同一网络
    let existing_mapping = match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM ip_managers WHERE ip_address = CAST($1 AS INET) AND network_id = $2",
    )
    .bind(&req.ip_address)
    .bind(req.network_id)
    .fetch_optional(pool.get_conn())
    .await
    {
        Ok(mapping) => mapping,
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "Database query error: {}",
                    err
                ))),
            );
        }
    };

    if existing_mapping.is_some() {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<IpManager>::error("该网络中IP地址已存在")));
    }

    // 检查IP地址是否在所属网络的CIDR范围内
    let network = match sqlx::query(
        r#"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, n.ipv4_dns::TEXT, n.ipv6_dns::TEXT, NULL as gateway, NULL as dns, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
           FROM network_cidrs n 
           JOIN network_regions nt ON n.network_region_id = nt.id 
           WHERE n.id = $1"#
    ).bind(req.network_id)
    .fetch_optional(pool.get_conn()).await {
        Ok(Some(row)) => Network {
            id: row.get(0),
            name: row.get(1),
            network_region_id: row.get(2),
            network_region: row.get(3),
            ipv4_cidr: row.get(4),
            ipv6_cidr: row.get(5),
            ipv4_gateway: row.get(6),
            ipv6_gateway: row.get(7),
            ipv4_dns: row.get(8),
            ipv6_dns: row.get(9),
            description: row.get(12),
            created_at: row.get(13),
            updated_at: row.get(14),
        },
        Ok(None) => {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<IpManager>::error("网络未找到")));
        },
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
        }
    };

    // 验证IP地址是否在网络的CIDR范围内
    let ip_in_cidr = {
        // 首先解析IP地址，检查其有效性
        let ip_addr = match std::net::IpAddr::from_str(&req.ip_address) {
            Ok(ip) => ip,
            Err(_) => {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<IpManager>::error("无效的IP地址格式")));
            }
        };

        // 检查IP地址类型
        let is_ipv4 = matches!(ip_addr, std::net::IpAddr::V4(_));

        // 用于存储验证结果
        let mut is_valid = false;

        // 尝试解析并验证所有可能的CIDR字段
        let cidr_fields = vec![
            // 如果是IPv4，尝试ipv4_cidr
            if is_ipv4 {
                network.ipv4_cidr.clone().unwrap_or_default()
            } else {
                "".to_string()
            },
            // 如果是IPv6，尝试ipv6_cidr
            if !is_ipv4 {
                network.ipv6_cidr.clone().unwrap_or_default()
            } else {
                "".to_string()
            },
        ];

        // 尝试所有CIDR字段，只要有一个验证通过就可以
        for cidr_str in cidr_fields {
            if cidr_str.is_empty() {
                continue; // 跳过空CIDR
            }

            match ipnetwork::IpNetwork::from_str(&cidr_str) {
                Ok(network_cidr) => {
                    // 检查IP地址是否在网段内
                    if network_cidr.contains(ip_addr) {
                        is_valid = true;
                        break;
                    }
                }
                Err(_) => {
                    // CIDR解析失败，尝试下一个
                    continue;
                }
            }
        }

        is_valid
    };

    // 如果IP地址不在任何CIDR范围内，返回错误
    if !ip_in_cidr {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<IpManager>::error("IP地址不在所属网络网段内")));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    // 检测IP地址版本
    let ip_version_num = detect_ip_version(&req.ip_address);

    // 创建IP管理
    if let Err(err) = sqlx::query(
        "INSERT INTO ip_managers (id, workstation_id, position_id, switch_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, CAST($7 AS INET), $8, $9, $10, $11, $12, $13, $14)"
    )
    .bind(id)
    .bind(req.workstation_id)
    .bind(req.position_id)
    .bind(req.switch_id)
    .bind(&req.device_type)
    .bind(req.network_id)
    .bind(&req.ip_address)
    .bind(&ip_version_num)
    .bind(&req.mac_address)
    .bind(&req.hostname)
    .bind("active")
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool.get_conn()).await {
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库插入错误: {}", err))));
    }

    // 返回创建的IP管理
    let mapping = IpManager {
        id,
        workstation_id: req.workstation_id,
        position_id: req.position_id,
        switch_id: req.switch_id,
        switch_port_id: req.switch_port_id,
        device_type: req.device_type.clone(),
        network_id: req.network_id,
        ip_address: req.ip_address.clone(),
        ip_version: ip_version_num,
        mac_address: req.mac_address.clone(),
        hostname: req.hostname.clone(),
        status: "active".to_string(),
        last_seen: now,
        created_at: now,
        updated_at: now,
    };

    // 记录操作日志
    let details = serde_json::json!({
        "ip_address": mapping.ip_address,
        "mac_address": mapping.mac_address,
        "hostname": mapping.hostname,
        "network_id": mapping.network_id,
        "ip_version": mapping.ip_version,
        "workstation_id": mapping.workstation_id,
        "position_id": mapping.position_id
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "create",
        "ip_manager",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<IpManager>::success(mapping, "IP管理创建成功")))
}

// 获取单个IP管理
pub async fn get_ip_manager(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = *id_path;

    let mapping = match sqlx::query_as::<_, IpManager>(
        "SELECT id, workstation_id, position_id, switch_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen::TIMESTAMPTZ, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM ip_managers WHERE id = $1"
    ).bind(id)
    .fetch_optional(pool.get_conn()).await {
        Ok(Some(mapping)) => mapping,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<IpManager>::error("IP管理未找到")));
        },
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::<IpManager>::success(mapping, "IP管理获取成功")))
}

// 获取工位的IP列表
pub async fn get_workstation_ips(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let workstation_id = *id_path;

    let ips = sqlx::query_as::<_, IpManagerWithNames>(
        r#"SELECT 
            id, workstation_id, position_id, switch_id, switch_port_id, device_type, device_name, 
            network_id, workstation_name, cabinet_position_name, switch_name, network_name, network_region, 
            ip_address::TEXT as ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at 
        FROM ip_managers_with_details 
        WHERE workstation_id = $1"#
    )
    .bind(workstation_id)
    .fetch_all(pool.get_conn())
    .await;

    match ips {
        Ok(data) => Ok(
            HttpResponse::Ok().json(ApiResponse::<Vec<IpManagerWithNames>>::success(
                data,
                "获取工位IP列表成功",
            )),
        ),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库查询错误: {}", e)))),
    }
}

// 获取机位的IP列表
pub async fn get_cabinet_position_ips(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let position_id = *id_path;

    let ips = sqlx::query_as::<_, IpManagerWithNames>(
        r#"SELECT 
            id, workstation_id, position_id, switch_id, switch_port_id, device_type, device_name, 
            network_id, workstation_name, cabinet_position_name, switch_name, network_name, network_region, 
            ip_address::TEXT as ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at 
        FROM ip_managers_with_details 
        WHERE position_id = $1"#
    )
    .bind(position_id)
    .fetch_all(pool.get_conn())
    .await;

    match ips {
        Ok(data) => Ok(
            HttpResponse::Ok().json(ApiResponse::<Vec<IpManagerWithNames>>::success(
                data,
                "获取机位IP列表成功",
            )),
        ),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库查询错误: {}", e)))),
    }
}

// 获取交换机的IP列表
pub async fn get_switch_ips(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let switch_id = *id_path;

    let ips = sqlx::query_as::<_, IpManagerWithNames>(
        r#"SELECT 
            id, workstation_id, position_id, switch_id, switch_port_id, device_type, device_name, 
            network_id, workstation_name, cabinet_position_name, switch_name, network_name, network_region, 
            ip_address::TEXT as ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at 
        FROM ip_managers_with_details 
        WHERE switch_id = $1"#
    )
    .bind(switch_id)
    .fetch_all(pool.get_conn())
    .await;

    match ips {
        Ok(data) => Ok(
            HttpResponse::Ok().json(ApiResponse::<Vec<IpManagerWithNames>>::success(
                data,
                "获取交换机IP列表成功",
            )),
        ),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库查询错误: {}", e)))),
    }
}

// 更新IP管理
pub async fn update_ip_manager(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    req: web::Json<IpManagerUpdate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 验证更新IP管理请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {:?}", e)))
        );
    }

    // 检查IP管理是否存在
    let existing_mapping = match sqlx::query_as::<_, IpManager>(
        "SELECT id, workstation_id, position_id, switch_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen::TIMESTAMPTZ, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM ip_managers WHERE id = $1"
    ).bind(id)
    .fetch_optional(pool.get_conn()).await {
        Ok(Some(mapping)) => mapping,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<IpManager>::error("IP管理未找到")));
        },
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
        }
    };

    // 确定要使用的网络ID和IP地址
    let network_id = req.network_id.unwrap_or(existing_mapping.network_id);
    let ip_address = req
        .ip_address
        .clone()
        .unwrap_or(existing_mapping.ip_address.clone());

    // 只有当IP地址被更新或网络ID被更新时，才需要验证IP地址
    if req.ip_address.is_some() || req.network_id.is_some() {
        // 验证IP地址是否已被同一网络中的其他IP管理使用
        let existing_ip = match sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM ip_managers WHERE ip_address = CAST($1 AS INET) AND network_id = $2 AND id != $3",
        )
        .bind(&ip_address)
        .bind(network_id)
        .bind(id)
        .fetch_optional(pool.get_conn())
        .await
        {
            Ok(existing) => existing,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {}",
                        err
                    ))),
                );
            }
        };

        if existing_ip.is_some() {
            return Ok(HttpResponse::BadRequest()
                .json(ApiResponse::<IpManager>::error("该网络中IP地址已存在")));
        }

        // 检查IP地址是否在所属网络的CIDR范围内
        let network = match sqlx::query(
            r#"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, n.ipv4_dns::TEXT, n.ipv6_dns::TEXT, NULL as gateway, NULL as dns, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
               FROM network_cidrs n 
               JOIN network_regions nt ON n.network_region_id = nt.id 
               WHERE n.id = $1"#
        ).bind(network_id)
        .fetch_optional(pool.get_conn()).await {
            Ok(Some(row)) => Network {
                id: row.get(0),
                name: row.get(1),
                network_region_id: row.get(2),
                network_region: row.get(3),
                ipv4_cidr: row.get(4),
                ipv6_cidr: row.get(5),
                ipv4_gateway: row.get(6),
                ipv6_gateway: row.get(7),
                ipv4_dns: row.get(8),
                ipv6_dns: row.get(9),
                description: row.get(12),
                created_at: row.get(13),
                updated_at: row.get(14),
            },
            Ok(None) => {
                return Ok(HttpResponse::BadRequest().json(ApiResponse::<IpManager>::error("网络未找到")));
            },
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
            }
        };

        // 验证IP地址是否在网络的CIDR范围内
        let ip_in_cidr = {
            // 首先解析IP地址，检查其有效性
            let ip_addr = match std::net::IpAddr::from_str(&ip_address) {
                Ok(ip) => ip,
                Err(_) => {
                    return Ok(HttpResponse::BadRequest()
                        .json(ApiResponse::<IpManager>::error("无效的IP地址格式")));
                }
            };

            // 检查IP地址类型
            let is_ipv4 = matches!(ip_addr, std::net::IpAddr::V4(_));

            // 用于存储验证结果
            let mut is_valid = false;

            // 尝试解析并验证所有可能的CIDR字段
            let cidr_fields = vec![
                // 如果是IPv4，尝试ipv4_cidr
                if is_ipv4 {
                    network.ipv4_cidr.clone().unwrap_or_default()
                } else {
                    "".to_string()
                },
                // 如果是IPv6，尝试ipv6_cidr
                if !is_ipv4 {
                    network.ipv6_cidr.clone().unwrap_or_default()
                } else {
                    "".to_string()
                },
            ];

            // 尝试所有CIDR字段，只要有一个验证通过就可以
            for cidr_str in cidr_fields {
                if cidr_str.is_empty() {
                    continue; // 跳过空CIDR
                }

                match ipnetwork::IpNetwork::from_str(&cidr_str) {
                    Ok(network_cidr) => {
                        // 检查IP地址是否在网段内
                        if network_cidr.contains(ip_addr) {
                            is_valid = true;
                            break;
                        }
                    }
                    Err(_) => {
                        // CIDR解析失败，尝试下一个
                        continue;
                    }
                }
            }

            is_valid
        };

        // 如果IP地址不在任何CIDR范围内，返回错误
        if !ip_in_cidr {
            return Ok(HttpResponse::BadRequest()
                .json(ApiResponse::<IpManager>::error("IP地址不在所属网络网段内")));
        }
    }

    let now = Utc::now();

    // 自动检测并更新IP版本
    let ip_version_num = if let Some(ip_address) = &req.ip_address {
        // 根据IP地址自动检测版本
        detect_ip_version(ip_address)
    } else {
        // 如果没有更新IP地址，使用请求中的ip_version或保持不变
        if let Some(ip_version_num) = &req.ip_version {
            // 直接使用数字
            *ip_version_num
        } else {
            // 如果请求中没有ip_version，使用现有映射的ip_version
            detect_ip_version(&existing_mapping.ip_address)
        }
    };

    // 验证设备类型和对应的设备ID是否匹配
    if let Some(device_type) = &req.device_type
        && !((device_type == "workstation"
            && req.workstation_id.is_some()
            && req.position_id.is_none()
            && req.switch_id.is_none())
            || (device_type == "cabinet_position"
                && req.workstation_id.is_none()
                && req.position_id.is_some()
                && req.switch_id.is_none())
            || (device_type == "switch"
                && req.workstation_id.is_none()
                && req.position_id.is_none()
                && req.switch_id.is_some()))
    {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error("设备类型与设备ID不匹配"))
        );
    }

    // 更新IP管理
    if let Err(err) = sqlx::query(
        "UPDATE ip_managers SET 
         workstation_id = $1, 
         position_id = $2,
         switch_id = $3,
         device_type = COALESCE($4, device_type),
         network_id = COALESCE($5, network_id), 
         ip_address = COALESCE(CAST($6 AS INET), ip_address), 
         mac_address = COALESCE($7, mac_address), 
         hostname = COALESCE($8, hostname), 
         status = COALESCE($9, status), 
         ip_version = $10, 
         updated_at = $11 
         WHERE id = $12",
    )
    .bind(req.workstation_id)
    .bind(req.position_id)
    .bind(req.switch_id)
    .bind(&req.device_type)
    .bind(req.network_id)
    .bind(&req.ip_address)
    .bind(&req.mac_address)
    .bind(&req.hostname)
    .bind(&req.status)
    .bind(&ip_version_num)
    .bind(now)
    .bind(id)
    .execute(pool.get_conn())
    .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库更新错误: {}", err))));
    }

    // 返回更新后的IP管理
    let mapping = match sqlx::query_as::<_, IpManager>(
        "SELECT id, workstation_id, position_id, switch_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen::TIMESTAMPTZ, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM ip_managers WHERE id = $1"
    ).bind(id)
    .fetch_one(pool.get_conn()).await {
        Ok(mapping) => mapping,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
        }
    };

    // 记录操作日志
    let details = serde_json::json!({
        "ip_address": mapping.ip_address,
        "mac_address": mapping.mac_address,
        "hostname": mapping.hostname,
        "network_id": mapping.network_id,
        "ip_version": mapping.ip_version,
        "status": mapping.status,
        "workstation_id": mapping.workstation_id,
        "position_id": mapping.position_id
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "update",
        "ip_manager",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<IpManager>::success(mapping, "IP管理更新成功")))
}

// 删除IP管理
pub async fn delete_ip_manager(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 检查IP管理是否存在
    let existing_mapping =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM ip_managers WHERE id = $1")
            .bind(id)
            .fetch_optional(pool.get_conn())
            .await
        {
            Ok(mapping) => mapping,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {}",
                        err
                    ))),
                );
            }
        };

    if existing_mapping.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<IpManager>::error("IP管理未找到")));
    }

    // 删除IP管理
    if let Err(err) = sqlx::query("DELETE FROM ip_managers WHERE id = $1")
        .bind(id)
        .execute(pool.get_conn())
        .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库删除错误: {}", err))));
    }

    // 记录操作日志
    let details = serde_json::json!({
        "ip_manager_id": id.to_string()
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "delete",
        "ip_manager",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "IP管理删除成功")))
}

// 拉取IP管理数据
pub async fn pull_ip_managers(
    pool: web::Data<DbPool>,
    req: web::Json<serde_json::Value>,
) -> Result<HttpResponse> {
    let switch_id = match req.get("switch_id") {
        Some(switch_id_value) => match switch_id_value.as_str() {
            Some(switch_id_str) => match Uuid::parse_str(switch_id_str) {
                Ok(switch_id) => switch_id,
                Err(_) => {
                    return Ok(HttpResponse::BadRequest()
                        .json(ApiResponse::<()>::error("无效的switch_id格式")));
                }
            },
            None => {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("switch_id必须是字符串")));
            }
        },
        None => {
            return Ok(
                HttpResponse::BadRequest().json(ApiResponse::<()>::error("缺少switch_id字段"))
            );
        }
    };

    let network_id = match req.get("network_id") {
        Some(network_id_value) => match network_id_value.as_str() {
            Some(network_id_str) => match Uuid::parse_str(network_id_str) {
                Ok(network_id) => network_id,
                Err(_) => {
                    return Ok(HttpResponse::BadRequest()
                        .json(ApiResponse::<()>::error("无效的network_id格式")));
                }
            },
            None => {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("network_id必须是字符串")));
            }
        },
        None => {
            return Ok(
                HttpResponse::BadRequest().json(ApiResponse::<()>::error("缺少network_id字段"))
            );
        }
    };

    // 获取指定网络的IP管理
    let existing_mappings = match sqlx::query_as::<_, IpManager>(
        "SELECT id, workstation_id, position_id, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen::TIMESTAMPTZ, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM ip_managers WHERE network_id = $1"
    ).bind(network_id)
    .fetch_all(pool.get_conn()).await {
        Ok(mappings) => mappings,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
        }
    };

    if existing_mappings.is_empty() {
        return Ok(
            HttpResponse::Ok().json(ApiResponse::<Vec<IpManager>>::success(
                vec![],
                "暂无IP管理数据",
            )),
        );
    }

    // 收集所有IP地址
    let ips: Vec<String> = existing_mappings
        .iter()
        .map(|m| m.ip_address.clone())
        .collect();

    // 从指定交换机通过SNMP获取MAC地址
    let mac_results =
        match crate::resource::switch::get_mac_from_switch(pool.get_conn(), &switch_id, &ips).await
        {
            Ok(results) => results,
            Err(e) => {
                return Ok(
                    HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                        "从交换机获取MAC失败: {}",
                        e
                    ))),
                );
            }
        };

    let mut updated_mappings = Vec::new();
    let mut changes_log = Vec::new();
    let now = Utc::now();

    // 处理每个映射
    for mut mapping in existing_mappings {
        let new_mac_address = mac_results.get(&mapping.ip_address).cloned().flatten();

        // 检查MAC地址是否发生变化
        if mapping.mac_address != new_mac_address {
            // 记录变化
            let old_mac = mapping.mac_address.clone().unwrap_or("无".to_string());
            let new_mac = new_mac_address.clone().unwrap_or("无".to_string());
            changes_log.push(format!(
                "IP地址 {} 的MAC地址从 {} 变为 {}",
                mapping.ip_address, old_mac, new_mac
            ));

            // 更新映射
            let old_mac_for_notify = mapping.mac_address.clone();
            mapping.mac_address = new_mac_address.clone();
            mapping.updated_at = now;

            // 如果获取到了新的MAC地址，更新last_seen
            if new_mac_address.is_some() {
                mapping.last_seen = now;
                mapping.status = "active".to_string();
            }

            // 更新数据库
            if let Err(err) = sqlx::query(
                "UPDATE ip_managers SET 
                 mac_address = $1, 
                 status = $2,
                 last_seen = $3,
                 updated_at = $4 
                 WHERE id = $5",
            )
            .bind(&mapping.mac_address)
            .bind(&mapping.status)
            .bind(mapping.last_seen)
            .bind(now)
            .bind(mapping.id)
            .execute(pool.get_conn())
            .await
            {
                println!("数据库更新错误: {}", err);
                continue;
            }

            // 发送通知
            if let Some(workstation_id) = &mapping.workstation_id {
                let old_mac_str = old_mac_for_notify.unwrap_or("无".to_string());
                let new_mac_str = mapping.mac_address.clone().unwrap_or("无".to_string());
                if let Err(err) = send_mac_change_notification(
                    pool.get_ref().get_conn(),
                    workstation_id,
                    &mapping.ip_address,
                    &old_mac_str,
                    &new_mac_str,
                )
                .await
                {
                    println!("发送MAC变更通知失败: {}", err);
                }
            }

            updated_mappings.push(mapping);
        }
    }

    // 记录操作日志
    if !changes_log.is_empty() {
        for log in &changes_log {
            println!("[MAC变化日志] {}", log);
        }
    }

    let message = if updated_mappings.is_empty() {
        "MAC地址无变化".to_string()
    } else {
        format!("成功更新 {} 条MAC地址", updated_mappings.len())
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<IpManager>>::success(
            updated_mappings,
            &message,
        )),
    )
}

// 检测IP地址版本
pub fn detect_ip_version(ip: &str) -> i16 {
    match IpAddr::from_str(ip) {
        Ok(IpAddr::V4(_)) => 4,
        Ok(IpAddr::V6(_)) => 6,
        Err(_) => 4,
    }
}

// 获取网络中可用的IP地址列表
pub async fn get_available_ips(
    pool: web::Data<DbPool>,
    network_id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let network_id = *network_id_path;

    let network = match sqlx::query(
        r#"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, n.ipv4_dns::TEXT, n.ipv6_dns::TEXT, NULL as gateway, NULL as dns, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
           FROM network_cidrs n 
           JOIN network_regions nt ON n.network_region_id = nt.id 
           WHERE n.id = $1"#
    ).bind(network_id)
    .fetch_optional(pool.get_conn()).await {
        Ok(Some(row)) => Network {
            id: row.get(0),
            name: row.get(1),
            network_region_id: row.get(2),
            network_region: row.get(3),
            ipv4_cidr: row.get(4),
            ipv6_cidr: row.get(5),
            ipv4_gateway: row.get(6),
            ipv6_gateway: row.get(7),
            ipv4_dns: row.get(8),
            ipv6_dns: row.get(9),
            description: row.get(12),
            created_at: row.get(13),
            updated_at: row.get(14),
        },
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("网络未找到")));
        },
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
        }
    };

    let mut available_ips = Vec::new();

    if let Some(ipv4_cidr) = &network.ipv4_cidr {
        if let Ok(network_cidr) = ipnetwork::IpNetwork::from_str(ipv4_cidr) {
            let used_ips: Vec<String> = match sqlx::query_scalar(
                "SELECT ip_address::TEXT FROM ip_managers WHERE network_id = $1"
            )
            .bind(network_id)
            .fetch_all(pool.get_conn())
            .await
            {
                Ok(ips) => ips,
                Err(_) => vec![],
            };

            let used_set: std::collections::HashSet<String> = used_ips.into_iter().collect();

            let gateway_ip = network.ipv4_gateway.clone();
            let network_addr = network_cidr.network();
            let broadcast_addr = match network_cidr {
                ipnetwork::IpNetwork::V4(v4_network) => {
                    Some(v4_network.broadcast().to_string())
                }
                _ => None,
            };

            for ip in network_cidr.iter() {
                let ip_str = ip.to_string();
                if used_set.contains(&ip_str) {
                    continue;
                }
                if Some(&ip_str) == gateway_ip.as_ref() {
                    continue;
                }
                if ip.to_string() == network_addr.to_string() {
                    continue;
                }
                if broadcast_addr.as_ref() == Some(&ip_str) {
                    continue;
                }
                available_ips.push(ip_str);
            }
        }
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "network_id": network_id,
            "network_name": network.name,
            "available_count": available_ips.len(),
            "available_ips": available_ips
        }),
        "获取可用IP列表成功",
    )))
}

// 自动分配IP地址
pub async fn auto_assign_ip(
    pool: web::Data<DbPool>,
    req: web::Json<serde_json::Value>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let network_id = match req.get("network_id") {
        Some(v) => match v.as_str() {
            Some(s) => match Uuid::parse_str(s) {
                Ok(id) => id,
                Err(_) => return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("无效的network_id格式"))),
            },
            None => return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("network_id必须是字符串"))),
        },
        None => return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("缺少network_id字段"))),
    };

    let workstation_id = req.get("workstation_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok());
    let position_id = req.get("position_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok());
    let switch_id = req.get("switch_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok());
    let mac_address = req.get("mac_address").and_then(|v| v.as_str()).map(|s| s.to_string());
    let hostname = req.get("hostname").and_then(|v| v.as_str()).map(|s| s.to_string());

    let device_type = match (workstation_id, position_id, switch_id) {
        (Some(_), None, None) => "workstation",
        (None, Some(_), None) => "cabinet_position",
        (None, None, Some(_)) => "switch",
        _ => return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("必须指定一个设备ID（workstation_id、position_id或switch_id）"))),
    };

    let network = match sqlx::query(
        r#"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, n.ipv4_dns::TEXT, n.ipv6_dns::TEXT, NULL as gateway, NULL as dns, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
           FROM network_cidrs n 
           JOIN network_regions nt ON n.network_region_id = nt.id 
           WHERE n.id = $1"#
    ).bind(network_id)
    .fetch_optional(pool.get_conn()).await {
        Ok(Some(row)) => Network {
            id: row.get(0),
            name: row.get(1),
            network_region_id: row.get(2),
            network_region: row.get(3),
            ipv4_cidr: row.get(4),
            ipv6_cidr: row.get(5),
            ipv4_gateway: row.get(6),
            ipv6_gateway: row.get(7),
            ipv4_dns: row.get(8),
            ipv6_dns: row.get(9),
            description: row.get(12),
            created_at: row.get(13),
            updated_at: row.get(14),
        },
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("网络未找到")));
        },
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
        }
    };

    let assigned_ip = match &network.ipv4_cidr {
        Some(ipv4_cidr) => {
            if let Ok(network_cidr) = ipnetwork::IpNetwork::from_str(ipv4_cidr) {
                let used_ips: Vec<String> = match sqlx::query_scalar(
                    "SELECT ip_address::TEXT FROM ip_managers WHERE network_id = $1"
                )
                .bind(network_id)
                .fetch_all(pool.get_conn())
                .await
                {
                    Ok(ips) => ips,
                    Err(_) => vec![],
                };

                let used_set: std::collections::HashSet<String> = used_ips.into_iter().collect();
                let gateway_ip = network.ipv4_gateway.clone();
                let network_addr = network_cidr.network();
                let broadcast_addr = match network_cidr {
                    ipnetwork::IpNetwork::V4(v4_network) => Some(v4_network.broadcast().to_string()),
                    _ => None,
                };

                let mut found_ip = None;
                for ip in network_cidr.iter() {
                    let ip_str = ip.to_string();
                    if used_set.contains(&ip_str) {
                        continue;
                    }
                    if Some(&ip_str) == gateway_ip.as_ref() {
                        continue;
                    }
                    if ip.to_string() == network_addr.to_string() {
                        continue;
                    }
                    if broadcast_addr.as_ref() == Some(&ip_str) {
                        continue;
                    }
                    found_ip = Some(ip_str);
                    break;
                }
                found_ip
            } else {
                None
            }
        }
        None => None,
    };

    let assigned_ip = match assigned_ip {
        Some(ip) => ip,
        None => return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("该网络没有可用的IP地址"))),
    };

    let id = Uuid::new_v4();
    let now = Utc::now();
    let ip_version_num = detect_ip_version(&assigned_ip);

    if let Err(err) = sqlx::query(
        "INSERT INTO ip_managers (id, workstation_id, position_id, switch_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, CAST($7 AS INET), $8, $9, $10, $11, $12, $13, $14)"
    )
    .bind(id)
    .bind(workstation_id)
    .bind(position_id)
    .bind(switch_id)
    .bind(device_type)
    .bind(network_id)
    .bind(&assigned_ip)
    .bind(&ip_version_num)
    .bind(&mac_address)
    .bind(&hostname)
    .bind("active")
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool.get_conn()).await {
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库插入错误: {}", err))));
    }

    let mapping = IpManager {
        id,
        workstation_id,
        position_id,
        switch_id,
        switch_port_id: None,
        device_type: Some(device_type.to_string()),
        network_id,
        ip_address: assigned_ip.clone(),
        ip_version: ip_version_num,
        mac_address,
        hostname,
        status: "active".to_string(),
        last_seen: now,
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "ip_address": mapping.ip_address,
        "mac_address": mapping.mac_address,
        "hostname": mapping.hostname,
        "network_id": mapping.network_id,
        "auto_assigned": true
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "auto_assign_ip",
        "ip_manager",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::success(mapping, "IP地址自动分配成功")))
}

// 批量创建IP管理记录
pub async fn batch_create_ip_managers(
    pool: web::Data<DbPool>,
    req: web::Json<Vec<IpManagerCreate>>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let mut created_ips = Vec::new();
    let mut errors = Vec::new();
    let now = Utc::now();

    for (index, ip_req) in req.iter().enumerate() {
        if let Err(e) = ip_req.validate() {
            errors.push(format!("第{}条记录验证失败: {:?}", index + 1, e));
            continue;
        }

        let device_type = ip_req.device_type.as_deref().unwrap_or("");
        let device_valid = (device_type == "workstation" && ip_req.workstation_id.is_some() && ip_req.position_id.is_none() && ip_req.switch_id.is_none())
            || (device_type == "cabinet_position" && ip_req.workstation_id.is_none() && ip_req.position_id.is_some() && ip_req.switch_id.is_none())
            || (device_type == "switch" && ip_req.workstation_id.is_none() && ip_req.position_id.is_none() && ip_req.switch_id.is_some());

        if !device_valid {
            errors.push(format!("第{}条记录: 设备类型与设备ID不匹配", index + 1));
            continue;
        }

        let existing = match sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM ip_managers WHERE ip_address = CAST($1 AS INET) AND network_id = $2",
        )
        .bind(&ip_req.ip_address)
        .bind(ip_req.network_id)
        .fetch_optional(pool.get_conn())
        .await
        {
            Ok(opt) => opt,
            Err(err) => {
                errors.push(format!("第{}条记录: 数据库查询错误 - {}", index + 1, err));
                continue;
            }
        };

        if existing.is_some() {
            errors.push(format!("第{}条记录: IP地址 {} 已存在", index + 1, ip_req.ip_address));
            continue;
        }

        let id = Uuid::new_v4();
        let ip_version_num = detect_ip_version(&ip_req.ip_address);

        if let Err(err) = sqlx::query(
            "INSERT INTO ip_managers (id, workstation_id, position_id, switch_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
             VALUES ($1, $2, $3, $4, $5, $6, CAST($7 AS INET), $8, $9, $10, $11, $12, $13, $14)"
        )
        .bind(id)
        .bind(ip_req.workstation_id)
        .bind(ip_req.position_id)
        .bind(ip_req.switch_id)
        .bind(&ip_req.device_type)
        .bind(ip_req.network_id)
        .bind(&ip_req.ip_address)
        .bind(&ip_version_num)
        .bind(&ip_req.mac_address)
        .bind(&ip_req.hostname)
        .bind("active")
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(pool.get_conn()).await {
            errors.push(format!("第{}条记录: 插入失败 - {}", index + 1, err));
            continue;
        }

        created_ips.push(IpManager {
            id,
            workstation_id: ip_req.workstation_id,
            position_id: ip_req.position_id,
            switch_id: ip_req.switch_id,
            switch_port_id: ip_req.switch_port_id,
            device_type: ip_req.device_type.clone(),
            network_id: ip_req.network_id,
            ip_address: ip_req.ip_address.clone(),
            ip_version: ip_version_num,
            mac_address: ip_req.mac_address.clone(),
            hostname: ip_req.hostname.clone(),
            status: "active".to_string(),
            last_seen: now,
            created_at: now,
            updated_at: now,
        });
    }

    let details = serde_json::json!({
        "created_count": created_ips.len(),
        "error_count": errors.len(),
        "errors": errors
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "batch_create",
        "ip_manager",
        &Uuid::nil(),
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "created": created_ips,
            "created_count": created_ips.len(),
            "errors": errors,
            "error_count": errors.len()
        }),
        &format!("批量创建完成，成功 {} 条，失败 {} 条", created_ips.len(), errors.len()),
    )))
}

// 批量删除IP管理记录
pub async fn batch_delete_ip_managers(
    pool: web::Data<DbPool>,
    req: web::Json<serde_json::Value>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let ids: Vec<Uuid> = match req.get("ids") {
        Some(v) => match v.as_array() {
            Some(arr) => {
                let mut parsed_ids = Vec::new();
                for id_val in arr {
                    if let Some(id_str) = id_val.as_str() {
                        if let Ok(id) = Uuid::parse_str(id_str) {
                            parsed_ids.push(id);
                        }
                    }
                }
                parsed_ids
            }
            None => return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("ids必须是数组"))),
        },
        None => return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("缺少ids字段"))),
    };

    if ids.is_empty() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("ids数组不能为空")));
    }

    let mut deleted_count = 0;
    let mut errors = Vec::new();

    for id in &ids {
        match sqlx::query("DELETE FROM ip_managers WHERE id = $1")
            .bind(id)
            .execute(pool.get_conn())
            .await
        {
            Ok(result) => {
                if result.rows_affected() > 0 {
                    deleted_count += 1;
                } else {
                    errors.push(format!("ID {} 不存在", id));
                }
            }
            Err(err) => {
                errors.push(format!("ID {} 删除失败: {}", id, err));
            }
        }
    }

    let details = serde_json::json!({
        "deleted_count": deleted_count,
        "error_count": errors.len(),
        "ids": ids
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "batch_delete",
        "ip_manager",
        &Uuid::nil(),
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "deleted_count": deleted_count,
            "errors": errors
        }),
        &format!("批量删除完成，成功 {} 条", deleted_count),
    )))
}
