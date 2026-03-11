use crate::config::Config;
use crate::db::DbPool;
use crate::models::{
    ApiResponse, IpManager, IpManagerCreate, IpManagerUpdate, IpManagerWithNames, Network,
};
use crate::utils::{log_system_operation, DEFAULT_PAGE, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};
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
    let page: i64 = query.get("page").and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_PAGE);
    let page_size: i64 = query.get("page_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(MAX_PAGE_SIZE);
    let offset = (page - 1) * page_size;

    let mut conditions: Vec<String> = Vec::new();
    let mut param_index = 1;

    let search_param = if !search.is_empty() {
        let pattern = format!("%{}%", search);
        conditions.push(format!(
            "(ip_address::TEXT ILIKE ${} OR mac_address ILIKE ${} OR hostname ILIKE ${} OR workstation_name ILIKE ${} OR cabinet_position_name ILIKE ${} OR network_name ILIKE ${})",
            param_index, param_index + 1, param_index + 2, param_index + 3, param_index + 4, param_index + 5
        ));
        param_index += 6;
        Some(pattern)
    } else {
        None
    };

    let device_type_param = if !device_type.is_empty() {
        conditions.push(format!("device_type = ${}", param_index));
        param_index += 1;
        Some(device_type.to_string())
    } else {
        None
    };

    let status_param = if !status.is_empty() {
        conditions.push(format!("status = ${}", param_index));
        param_index += 1;
        Some(status.to_string())
    } else {
        None
    };

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_query = format!("SELECT COUNT(*) FROM ip_managers_with_details {}", where_clause);
    let mut count_sql = sqlx::query_scalar::<_, i64>(&count_query);

    if let Some(ref pattern) = search_param {
        for _ in 0..6 {
            count_sql = count_sql.bind(pattern);
        }
    }
    if let Some(ref dt) = device_type_param {
        count_sql = count_sql.bind(dt);
    }
    if let Some(ref st) = status_param {
        count_sql = count_sql.bind(st);
    }

    let total: i64 = match count_sql.fetch_one(pool.get_conn()).await {
        Ok(count) => count,
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询IP数量失败"));
        }
    };

    let data_query = format!(
        "SELECT id, workstation_id, position_id, switch_id, switch_port_id, device_type, device_name, network_id, workstation_name, cabinet_position_name, switch_name, switch_port_number, network_name, network_region, ip_address::TEXT as ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at FROM ip_managers_with_details {} ORDER BY updated_at DESC LIMIT ${} OFFSET ${}",
        where_clause, param_index, param_index + 1
    );

    let mut data_sql = sqlx::query_as::<_, IpManagerWithNames>(&data_query);

    if let Some(ref pattern) = search_param {
        for _ in 0..6 {
            data_sql = data_sql.bind(pattern);
        }
    }
    if let Some(ref dt) = device_type_param {
        data_sql = data_sql.bind(dt);
    }
    if let Some(ref st) = status_param {
        data_sql = data_sql.bind(st);
    }
    data_sql = data_sql.bind(page_size as i32).bind(offset as i32);

    let mappings = match data_sql.fetch_all(pool.get_conn()).await {
        Ok(mappings) => mappings,
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询IP列表失败"));
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
            return Ok(crate::utils::handle_db_error(err, "查询IP地址失败"));
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

    let ip_version_num = detect_ip_version(&req.ip_address);

    if let Err(err) = sqlx::query(
        "INSERT INTO ip_managers (id, workstation_id, position_id, switch_id, switch_port_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)"
    )
    .bind(id)
    .bind(req.workstation_id)
    .bind(req.position_id)
    .bind(req.switch_id)
    .bind(req.switch_port_id)
    .bind(&req.device_type)
    .bind(req.network_id)
    .bind(&req.ip_address)
    .bind(ip_version_num)
    .bind(&req.mac_address)
    .bind(&req.hostname)
    .bind("active")
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool.get_conn()).await {
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库插入错误: {}", err))));
    }

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
        "SELECT id, workstation_id, position_id, switch_id, switch_port_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen::TIMESTAMPTZ, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM ip_managers WHERE id = $1"
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
            network_id, workstation_name, cabinet_position_name, switch_name, switch_port_number, network_name, network_region, 
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

    // 查询 position_id 或 switch_id 匹配的IP地址
    let ips = sqlx::query_as::<_, IpManagerWithNames>(
        r#"SELECT 
            id, workstation_id, position_id, switch_id, switch_port_id, device_type, device_name, 
            network_id, workstation_name, cabinet_position_name, switch_name, switch_port_number, network_name, network_region, 
            ip_address::TEXT as ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at 
        FROM ip_managers_with_details 
        WHERE position_id = $1 OR switch_id = $1"#
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
            network_id, workstation_name, cabinet_position_name, switch_name, switch_port_number, network_name, network_region, 
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

    let existing_mapping = match sqlx::query_as::<_, IpManager>(
        "SELECT id, workstation_id, position_id, switch_id, switch_port_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen::TIMESTAMPTZ, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM ip_managers WHERE id = $1"
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
        if let Some(ip_version) = &req.ip_version {
            // 直接使用字符串
            *ip_version
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
                && req.switch_id.is_some()))
    {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error("设备类型与设备ID不匹配"))
        );
    }

    if let Err(err) = sqlx::query(
        "UPDATE ip_managers SET 
         workstation_id = $1, 
         position_id = $2,
         switch_id = $3,
         switch_port_id = $4,
         device_type = COALESCE($5, device_type),
         network_id = COALESCE($6, network_id), 
         ip_address = COALESCE(CAST($7 AS INET), ip_address), 
         mac_address = COALESCE($8, mac_address), 
         hostname = COALESCE($9, hostname), 
         status = COALESCE($10, status), 
         ip_version = $11, 
         updated_at = $12 
         WHERE id = $13",
    )
    .bind(req.workstation_id)
    .bind(req.position_id)
    .bind(req.switch_id)
    .bind(req.switch_port_id)
    .bind(&req.device_type)
    .bind(req.network_id)
    .bind(&req.ip_address)
    .bind(&req.mac_address)
    .bind(&req.hostname)
    .bind(&req.status)
    .bind(ip_version_num)
    .bind(now)
    .bind(id)
    .execute(pool.get_conn())
    .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库更新错误: {}", err))));
    }

    let mapping = match sqlx::query_as::<_, IpManager>(
        "SELECT id, workstation_id, position_id, switch_id, switch_port_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen::TIMESTAMPTZ, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM ip_managers WHERE id = $1"
    ).bind(id)
    .fetch_one(pool.get_conn()).await {
        Ok(mapping) => mapping,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
        }
    };

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

// 拉取IP管理数据 - 从switch_macs缓存中匹配已存在的IP记录并更新MAC地址
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

    let network_info: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ipv4_cidr::text, ipv6_cidr::text FROM network_cidrs WHERE id = $1"
    )
    .bind(network_id)
    .fetch_optional(pool.get_conn())
    .await
    .map_err(|e| format!("查询网段信息失败: {}", e))
    .ok()
    .flatten();

    if network_info.is_none() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error("未找到网段信息")));
    }

    let switch_macs: Vec<(String, String)> = sqlx::query_as(
        "SELECT ip_address, mac_address FROM switch_macs WHERE switch_id = $1"
    )
    .bind(switch_id)
    .fetch_all(pool.get_conn())
    .await
    .unwrap_or_default();

    if switch_macs.is_empty() {
        return Ok(
            HttpResponse::Ok().json(ApiResponse::<Vec<IpManager>>::success(
                vec![],
                "该交换机暂无MAC数据，请先在交换机管理中同步MAC表",
            )),
        );
    }

    let mut filtered_entries: Vec<(String, String)> = Vec::new();
    for (ip, mac) in switch_macs {
        let is_ipv6 = ip.contains(':');
        
        if let Some((ref ipv4_cidr, ref ipv6_cidr)) = network_info {
            let belongs_to_network = if is_ipv6 {
                ipv6_cidr.as_ref().map(|cidr| ip_belongs_to_cidr(&ip, cidr)).unwrap_or(false)
            } else {
                ipv4_cidr.as_ref().map(|cidr| ip_belongs_to_cidr(&ip, cidr)).unwrap_or(false)
            };
            
            if belongs_to_network {
                filtered_entries.push((ip, mac));
            }
        }
    }

    if filtered_entries.is_empty() {
        return Ok(
            HttpResponse::Ok().json(ApiResponse::<Vec<IpManager>>::success(
                vec![],
                "未发现属于该网段的IP地址",
            )),
        );
    }

    let now = Utc::now();
    let mut updated_count = 0usize;
    let mut skipped_count = 0usize;
    let mut not_found_count = 0usize;

    for (ip, mac) in &filtered_entries {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM ip_managers WHERE ip_address = CAST($1 AS INET))"
        )
        .bind(ip)
        .fetch_one(pool.get_conn())
        .await
        .unwrap_or(false);

        if !exists {
            not_found_count += 1;
            continue;
        }

        let mac_conflict: Option<String> = sqlx::query_scalar(
            "SELECT host(ip_address) FROM ip_managers WHERE mac_address = $1 AND ip_address != CAST($2 AS INET)"
        )
        .bind(mac)
        .bind(ip)
        .fetch_optional(pool.get_conn())
        .await
        .ok()
        .flatten();

        if let Some(conflict_ip) = mac_conflict {
            println!("MAC冲突: {} 已被 IP {} 使用，跳过更新", mac, conflict_ip);
            skipped_count += 1;
            continue;
        }

        let result = sqlx::query(
            r#"UPDATE ip_managers 
               SET mac_address = $1, last_seen = $2, updated_at = $2
               WHERE ip_address = CAST($3 AS INET) AND (mac_address IS NULL OR mac_address = '' OR mac_address != $1)"#
        )
        .bind(mac)
        .bind(now)
        .bind(ip)
        .execute(pool.get_conn())
        .await;

        if let Ok(res) = result {
            if res.rows_affected() > 0 {
                updated_count += 1;
            }
        }
    }

    let results: Vec<IpManager> = match sqlx::query_as::<_, IpManager>(
        "SELECT id, workstation_id, position_id, switch_id, switch_port_id, device_type, network_id, host(ip_address) as ip_address, ip_version, mac_address, hostname, status, last_seen::TIMESTAMPTZ, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM ip_managers WHERE network_id = $1"
    )
    .bind(network_id)
    .fetch_all(pool.get_conn())
    .await
    {
        Ok(results) => results,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                format!("查询结果失败: {}", err),
            )));
        }
    };

    let mut message_parts = Vec::new();
    if updated_count > 0 {
        message_parts.push(format!("更新 {} 条MAC地址", updated_count));
    }
    if not_found_count > 0 {
        message_parts.push(format!("{} 条IP未在管理表中", not_found_count));
    }
    if skipped_count > 0 {
        message_parts.push(format!("{} 条MAC冲突跳过", skipped_count));
    }
    
    let message = if message_parts.is_empty() {
        "MAC地址无变化".to_string()
    } else {
        message_parts.join("，")
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(results, &message)))
}

pub async fn pull_ip_managers_internal(
    pool: &sqlx::PgPool,
    switch_id: Uuid,
    network_id: Uuid,
) -> Result<(), String> {
    let network_info: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ipv4_cidr::text, ipv6_cidr::text FROM network_cidrs WHERE id = $1"
    )
    .bind(network_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("查询网段信息失败: {}", e))
    .ok()
    .flatten();

    if network_info.is_none() {
        return Err("未找到网段信息".to_string());
    }

    let switch_macs: Vec<(String, String)> = sqlx::query_as(
        "SELECT ip_address, mac_address FROM switch_macs WHERE switch_id = $1"
    )
    .bind(switch_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    if switch_macs.is_empty() {
        return Err("该交换机暂无MAC数据".to_string());
    }

    let mut filtered_entries: Vec<(String, String)> = Vec::new();
    for (ip, mac) in switch_macs {
        let is_ipv6 = ip.contains(':');
        if let Some((ref ipv4_cidr, ref ipv6_cidr)) = network_info {
            let belongs_to_network = if is_ipv6 {
                ipv6_cidr.as_ref().map(|cidr| ip_belongs_to_cidr(&ip, cidr)).unwrap_or(false)
            } else {
                ipv4_cidr.as_ref().map(|cidr| ip_belongs_to_cidr(&ip, cidr)).unwrap_or(false)
            };
            if belongs_to_network {
                filtered_entries.push((ip, mac));
            }
        }
    }

    if filtered_entries.is_empty() {
        return Err("未发现属于该网段的IP地址".to_string());
    }

    let now = Utc::now();
    for (ip, mac) in &filtered_entries {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM ip_managers WHERE ip_address = CAST($1 AS INET))"
        )
        .bind(ip)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("查询IP存在失败: {}", e))?;

        if !exists {
            continue;
        }

        let mac_conflict: Option<String> = sqlx::query_scalar(
            "SELECT host(ip_address) FROM ip_managers WHERE mac_address = $1 AND ip_address != CAST($2 AS INET)"
        )
        .bind(mac)
        .bind(ip)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("查询MAC冲突失败: {}", e))?
        .flatten();

        if mac_conflict.is_some() {
            continue;
        }

        let old_mac: Option<String> = sqlx::query_scalar(
            "SELECT mac_address FROM ip_managers WHERE ip_address = CAST($1 AS INET)"
        )
        .bind(ip)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("查询旧MAC失败: {}", e))?
        .flatten();

        let result = sqlx::query(
            r#"UPDATE ip_managers 
               SET mac_address = $1, last_seen = $2, updated_at = $2
               WHERE ip_address = CAST($3 AS INET) AND (mac_address IS NULL OR mac_address = '' OR mac_address != $1)"#
        )
        .bind(&mac)
        .bind(now)
        .bind(ip)
        .execute(pool)
        .await
        .map_err(|e| format!("更新MAC地址失败: {}", e))?;

        if result.rows_affected() > 0 {
            if let (Some(old), Some(workstation_id)) = (&old_mac, sqlx::query_scalar::<_, Option<Uuid>>(
                "SELECT workstation_id FROM ip_managers WHERE ip_address = CAST($1 AS INET)"
            )
            .bind(ip)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .flatten()) {
                if *old != *mac && !old.is_empty() {
                    let _ = crate::utils::send_mac_change_notification(
                        pool,
                        &workstation_id,
                        ip,
                        &old,
                        mac,
                    ).await;
                }
            }
        }
    }

    Ok(())
}

fn ip_belongs_to_cidr(ip: &str, cidr: &str) -> bool {
    let cidr_parts: Vec<&str> = cidr.split('/').collect();
    if cidr_parts.len() != 2 {
        return false;
    }
    
    let network_addr = cidr_parts[0];
    let prefix_len: u32 = match cidr_parts[1].parse() {
        Ok(v) => v,
        Err(_) => return false,
    };

    if ip.contains(':') {
        let ip_parsed = match std::net::Ipv6Addr::from_str(ip) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let network_parsed = match std::net::Ipv6Addr::from_str(network_addr) {
            Ok(v) => v,
            Err(_) => return false,
        };
        
        let ip_bytes = ip_parsed.octets();
        let network_bytes = network_parsed.octets();
        
        let full_bits = prefix_len as usize;
        let byte_idx = full_bits / 8;
        let bit_offset = full_bits % 8;
        
        for i in 0..byte_idx {
            if ip_bytes[i] != network_bytes[i] {
                return false;
            }
        }
        
        if byte_idx < 16 && bit_offset > 0 {
            let mask = 0xFF_u8 << (8 - bit_offset);
            if (ip_bytes[byte_idx] & mask) != (network_bytes[byte_idx] & mask) {
                return false;
            }
        }
        
        true
    } else {
        let ip_parsed = match std::net::Ipv4Addr::from_str(ip) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let network_parsed = match std::net::Ipv4Addr::from_str(network_addr) {
            Ok(v) => v,
            Err(_) => return false,
        };
        
        let ip_u32 = u32::from(ip_parsed);
        let network_u32 = u32::from(network_parsed);
        let mask = if prefix_len == 0 { 0 } else { !0u32 << (32 - prefix_len) };
        
        (ip_u32 & mask) == (network_u32 & mask)
    }
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

    if let Some(ipv4_cidr) = &network.ipv4_cidr
        && let Ok(network_cidr) = ipnetwork::IpNetwork::from_str(ipv4_cidr) {
            let used_ips: Vec<String> = sqlx::query_scalar(
                "SELECT ip_address::TEXT FROM ip_managers WHERE network_id = $1"
            )
            .bind(network_id)
            .fetch_all(pool.get_conn())
            .await
            .unwrap_or_default();

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
    let switch_port_id = req.get("switch_port_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok());
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
                let used_ips: Vec<String> = sqlx::query_scalar(
                    "SELECT ip_address::TEXT FROM ip_managers WHERE network_id = $1"
                )
                .bind(network_id)
                .fetch_all(pool.get_conn())
                .await
                .unwrap_or_default();

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
        "INSERT INTO ip_managers (id, workstation_id, position_id, switch_id, switch_port_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)"
    )
    .bind(id)
    .bind(workstation_id)
    .bind(position_id)
    .bind(switch_id)
    .bind(switch_port_id)
    .bind(device_type)
    .bind(network_id)
    .bind(&assigned_ip)
    .bind(ip_version_num)
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
        switch_port_id,
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
    let now = Utc::now();
    let mut valid_requests: Vec<(usize, &IpManagerCreate, Uuid, i16)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for (index, ip_req) in req.iter().enumerate() {
        if let Err(e) = ip_req.validate() {
            errors.push(format!("第{}条记录验证失败: {:?}", index + 1, e));
            continue;
        }

        let device_type = ip_req.device_type.as_deref().unwrap_or("");
        let device_valid = (device_type == "workstation" && ip_req.workstation_id.is_some() && ip_req.position_id.is_none() && ip_req.switch_id.is_none())
            || (device_type == "cabinet_position" && ip_req.workstation_id.is_none() && ip_req.position_id.is_some() && ip_req.switch_id.is_none())
            || (device_type == "switch" && ip_req.workstation_id.is_none() && ip_req.switch_id.is_some());

        if !device_valid {
            errors.push(format!("第{}条记录: 设备类型与设备ID不匹配", index + 1));
            continue;
        }

        let id = Uuid::new_v4();
        let ip_version_num = detect_ip_version(&ip_req.ip_address);
        valid_requests.push((index, ip_req, id, ip_version_num));
    }

    if valid_requests.is_empty() {
        let error_msg = if errors.is_empty() { 
            "没有有效的记录".to_string() 
        } else { 
            errors.join("; ") 
        };
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(&error_msg)));
    }

    let mut tx = match pool.get_conn().begin().await {
        Ok(tx) => tx,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                format!("事务启动失败: {}", err),
            )));
        }
    };

    let mut created_ips = Vec::new();
    let mut duplicate_errors = Vec::new();

    for (index, ip_req, id, ip_version_num) in &valid_requests {
        let existing: Option<Uuid> = match sqlx::query_scalar(
            "SELECT id FROM ip_managers WHERE ip_address = CAST($1 AS INET) AND network_id = $2"
        )
        .bind(&ip_req.ip_address)
        .bind(ip_req.network_id)
        .fetch_optional(tx.as_mut())
        .await
        {
            Ok(opt) => opt,
            Err(err) => {
                duplicate_errors.push(format!("第{}条记录: 数据库查询错误 - {}", index + 1, err));
                continue;
            }
        };

        if existing.is_some() {
            duplicate_errors.push(format!("第{}条记录: IP地址 {} 已存在", index + 1, ip_req.ip_address));
            continue;
        }

        if let Err(err) = sqlx::query(
            "INSERT INTO ip_managers (id, workstation_id, position_id, switch_id, switch_port_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
             VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)"
        )
        .bind(*id)
        .bind(ip_req.workstation_id)
        .bind(ip_req.position_id)
        .bind(ip_req.switch_id)
        .bind(ip_req.switch_port_id)
        .bind(&ip_req.device_type)
        .bind(ip_req.network_id)
        .bind(&ip_req.ip_address)
        .bind(*ip_version_num)
        .bind(&ip_req.mac_address)
        .bind(&ip_req.hostname)
        .bind("active")
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(tx.as_mut())
        .await
        {
            duplicate_errors.push(format!("第{}条记录: 插入失败 - {}", index + 1, err));
            continue;
        }

        created_ips.push(IpManager {
            id: *id,
            workstation_id: ip_req.workstation_id,
            position_id: ip_req.position_id,
            switch_id: ip_req.switch_id,
            switch_port_id: ip_req.switch_port_id,
            device_type: ip_req.device_type.clone(),
            network_id: ip_req.network_id,
            ip_address: ip_req.ip_address.clone(),
            ip_version: *ip_version_num,
            mac_address: ip_req.mac_address.clone(),
            hostname: ip_req.hostname.clone(),
            status: "active".to_string(),
            last_seen: now,
            created_at: now,
            updated_at: now,
        });
    }

    if let Err(err) = tx.commit().await {
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
            format!("事务提交失败: {}", err),
        )));
    }

    errors.extend(duplicate_errors);

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
