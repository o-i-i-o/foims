use crate::config::Config;
use crate::db::DbPool;
use crate::models::{
    ApiResponse, CabinetPosition, CabinetPositionCreate, CabinetPositionPortWithSwitchPort,
    CabinetPositionUpdate, CabinetPositionWithDetails, IpManager, Workstation, WorkstationCreate,
    WorkstationPortWithSwitchPort, WorkstationUpdate, WorkstationWithDetails,
};
use crate::resource::ip::detect_ip_version;
use crate::utils::log_system_operation;
use actix_web::{HttpRequest, HttpResponse, Result, web};
use chrono::Utc;
use sqlx::Row;
use std::str::FromStr;
use uuid::Uuid;
use validator::Validate;

// 验证IP地址是否在网络的CIDR范围内
fn validate_ip_in_cidr(ip_address: &str, network: &crate::models::Network) -> Result<bool, HttpResponse> {
    let ip_addr = match std::net::IpAddr::from_str(ip_address) {
        Ok(ip) => ip,
        Err(_) => {
            return Err(HttpResponse::BadRequest()
                .json(ApiResponse::<()>::error("无效的IP地址格式")));
        }
    };

    let is_ipv4 = matches!(ip_addr, std::net::IpAddr::V4(_));
    let mut is_valid = false;

    let cidr_fields = vec![
        if is_ipv4 {
            network.ipv4_cidr.clone().unwrap_or_default()
        } else {
            "".to_string()
        },
        if !is_ipv4 {
            network.ipv6_cidr.clone().unwrap_or_default()
        } else {
            "".to_string()
        },
    ];

    for cidr_str in cidr_fields {
        if cidr_str.is_empty() {
            continue;
        }

        match ipnetwork::IpNetwork::from_str(&cidr_str) {
            Ok(network_cidr) => {
                if network_cidr.contains(ip_addr) {
                    is_valid = true;
                    break;
                }
            }
            Err(_) => {
                continue;
            }
        }
    }

    Ok(is_valid)
}

// 机位相关路由
// 获取所有机位
pub async fn get_positions(
    pool: web::Data<DbPool>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    // 1. 获取查询参数
    let cabinet_id = query.get("cabinet_id");

    // 2. 根据是否有cabinet_id参数构建不同的SQL查询
    let (positions_basic, position_ids) = if let Some(cabinet_id_str) = cabinet_id {
        // 有cabinet_id参数，解析为UUID类型
        match Uuid::parse_str(cabinet_id_str) {
            Ok(cabinet_id_uuid) => {
                // 查询该机柜的位置
                let rows = match sqlx::query(
                    "SELECT p.id, p.name, p.cabinet_id, 
                            COALESCE((SELECT c.name FROM cabinets c WHERE c.id = p.cabinet_id), '未知机柜') as cabinet_name, 
                            p.start_u, p.end_u, p.network_id, p.description, p.created_at::TIMESTAMPTZ, p.updated_at::TIMESTAMPTZ 
                     FROM positions p 
                     WHERE p.cabinet_id = $1"
                ).bind(cabinet_id_uuid)
                .fetch_all(pool.get_conn()).await {
                    Ok(rows) => rows,
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))))
                    }
                };
                
                let mut positions = Vec::new();
                let mut ids = Vec::new();
                for row in rows {
                    let id: Uuid = row.get("id");
                    ids.push(id);
                    positions.push(row);
                }
                (positions, ids)
            }
            Err(_) => {
                // 无效的UUID格式
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("无效的cabinet_id格式")));
            }
        }
    } else {
        // 没有cabinet_id参数，查询所有机位
        let rows = match sqlx::query(
            "SELECT p.id, p.name, p.cabinet_id, 
                    COALESCE((SELECT c.name FROM cabinets c WHERE c.id = p.cabinet_id), '未知机柜') as cabinet_name, 
                    p.start_u, p.end_u, p.network_id, p.description, p.created_at::TIMESTAMPTZ, p.updated_at::TIMESTAMPTZ 
             FROM positions p"
        ).fetch_all(pool.get_conn()).await {
            Ok(rows) => rows,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))))
            }
        };

        let mut positions = Vec::new();
        let mut ids = Vec::new();
        for row in rows {
            let id: Uuid = row.get("id");
            ids.push(id);
            positions.push(row);
        }
        (positions, ids)
    };

    // 3. 批量获取所有相关机位的端口信息
    let all_ports = if !position_ids.is_empty() {
        match sqlx::query_as::<_, CabinetPositionPortWithSwitchPort>(
            r#"SELECT cp.id, cp.position_id, cp.switch_port_id, sp.switch_id, s.name as switch_name, sp.port_number, sp.port_name, cp.created_at::TIMESTAMPTZ, cp.updated_at::TIMESTAMPTZ 
               FROM position_ports cp 
               JOIN switch_ports sp ON cp.switch_port_id = sp.id 
               JOIN switches s ON sp.switch_id = s.id
               WHERE cp.position_id = ANY($1)"#
        ).bind(&position_ids)
        .fetch_all(pool.get_conn()).await {
            Ok(ports) => ports,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
            }
        }
    } else {
        Vec::new()
    };

    // 4. 将端口按position_id分组
    let mut ports_map: std::collections::HashMap<Uuid, Vec<CabinetPositionPortWithSwitchPort>> = std::collections::HashMap::new();
    for port in all_ports {
        ports_map.entry(port.position_id).or_default().push(port);
    }

    // 5. 组装最终结果
    let mut positions_with_details = Vec::new();

    for row in positions_basic {
        let id: Uuid = row.get("id");
        let name: String = row.get("name");
        let cabinet_id: Uuid = row.get("cabinet_id");
        let cabinet_name: String = row.get("cabinet_name"); // 直接从查询结果获取
        let start_u: i32 = row.get("start_u");
        let end_u: i32 = row.get("end_u");
        let network_id: Option<Uuid> = row.get("network_id");
        let description: Option<String> = row.get("description");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");
        
        // 获取该机位的端口
        let ports = ports_map.get(&id).cloned().unwrap_or_default();
        
        let position_with_details = CabinetPositionWithDetails {
            id,
            name,
            cabinet_id,
            cabinet_name,
            start_u,
            end_u,
            network_id,
            ips: Vec::new(),
            ports,
            description,
            created_at,
            updated_at,
        };

        positions_with_details.push(position_with_details);
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<CabinetPositionWithDetails>>::success(
            positions_with_details,
            "机位获取成功",
        )),
    )
}

// 创建机位
pub async fn create_cabinet_position(
    pool: web::Data<DbPool>,
    req: web::Json<CabinetPositionCreate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 验证创建机位请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {:?}", e)))
        );
    }

    // 开始事务
    let mut tx = match pool.pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("开启事务失败: {}", err))));
        }
    };

    // 检查机位名称是否已存在
    let existing_position: Option<Uuid> = match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM positions WHERE name = $1 AND cabinet_id = $2",
    )
    .bind(&req.name)
    .bind(req.cabinet_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(position) => position,
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "Database query error: {}",
                    err
                ))),
            );
        }
    };

    if existing_position.is_some() {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<CabinetPosition>::error("机位名称已存在")));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    // 创建机位
    if let Err(err) = sqlx::query(
        "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
    )
    .bind(id)
    .bind(&req.name)
    .bind(req.cabinet_id)
    .bind(req.start_u)
    .bind(req.end_u)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx).await {
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库插入错误: {}", err))));
    }

    // 创建机位关联的IP地址
    let mut ip_count = 0;
    if let Some(ips) = &req.ips {
        for ip in ips {
            // 验证设备类型和对应的设备ID是否匹配
            let device_type = ip.device_type.as_deref().unwrap_or("");
            if device_type != "cabinet_position" || ip.position_id.is_some() {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("设备类型与设备ID不匹配")));
            }

            // 检查IP地址是否已存在于同一网络
            let existing_mapping: Option<Uuid> =
                match sqlx::query_scalar::<_, Uuid>(
                    "SELECT id FROM ip_managers WHERE ip_address = CAST($1 AS INET) AND network_id = $2",
                )
                .bind(&ip.ip_address)
                .bind(ip.network_id)
                .fetch_optional(&mut *tx)
                .await
                {
                    Ok(mapping) => mapping,
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(
                            ApiResponse::<()>::error(format!("Database query error: {}", err)),
                        ));
                    }
                };

            if existing_mapping.is_some() {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("该网络中IP地址已存在")));
            }

            // 检查IP地址是否在所属网络的CIDR范围内
            let network = match sqlx::query(
                r#"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, n.ipv4_dns::TEXT, n.ipv6_dns::TEXT, NULL as gateway, NULL as dns, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
                   FROM network_cidrs n 
                   JOIN network_regions nt ON n.network_region_id = nt.id 
                   WHERE n.id = $1"#
            ).bind(ip.network_id)
            .fetch_optional(&mut *tx).await {
                Ok(Some(row)) => {
                    use crate::models::Network;
                    Network {
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
                    }
                },
                Ok(None) => {
                    return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("网络未找到")));
                },
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
                }
            };

            // 验证IP地址是否在网络的CIDR范围内
            let ip_in_cidr = match validate_ip_in_cidr(&ip.ip_address, &network) {
                Ok(valid) => valid,
                Err(response) => return Ok(response),
            };

            // 如果IP地址不在任何CIDR范围内，返回错误
            if !ip_in_cidr {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("IP地址不在所属网络网段内")));
            }

            // 检测IP地址版本
            let ip_version = detect_ip_version(&ip.ip_address);

            if let Err(err) = sqlx::query(
                "INSERT INTO ip_managers (id, workstation_id, position_id, switch_id, switch_port_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
                 VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)"
            )
            .bind(Uuid::new_v4())
            .bind(ip.workstation_id)
            .bind(Some(id))
            .bind(ip.switch_id)
            .bind(ip.switch_port_id)
            .bind(&ip.device_type)
            .bind(ip.network_id)
            .bind(&ip.ip_address)
            .bind(ip_version)
            .bind(&ip.mac_address)
            .bind(&ip.hostname)
            .bind("active")
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&mut *tx).await {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库插入错误: {}", err))));
            }

            ip_count += 1;
        }
    }

    if let Err(err) = tx.commit().await {
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("提交事务失败: {}", err))));
    }

    // 返回创建的机位
    let position = CabinetPosition {
        id,
        name: req.name.clone(),
        cabinet_id: req.cabinet_id,
        start_u: req.start_u,
        end_u: req.end_u,
        network_id: req.network_id,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    // 记录操作日志
    let details = serde_json::json!({
        "name": position.name,
        "cabinet_id": position.cabinet_id,
        "start_u": position.start_u,
        "end_u": position.end_u,
        "description": position.description,
        "ip_count": ip_count
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "create",
        "cabinet_position",
        &id,
        &details,
        true,
    )
    .await;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<CabinetPosition>::success(
            position,
            "机位创建成功",
        )),
    )
}

// 获取单个机位
pub async fn get_cabinet_position(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 获取机位基本信息
    let position = match sqlx::query_as::<_, CabinetPosition>(
        "SELECT id, name, cabinet_id, start_u, end_u, network_id, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM positions WHERE id = $1"
    ).bind(id)
    .fetch_optional(pool.get_conn()).await {
        Ok(Some(position)) => position,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<CabinetPosition>::error("机位未找到")));
        },
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
        }
    };

    // 获取机位关联的IP信息
    let position_ips = match sqlx::query_as::<_, IpManager>(
        r#"SELECT 
            m.id, m.workstation_id, m.position_id, m.switch_id, m.switch_port_id,
            m.device_type, m.network_id, 
            CAST(m.ip_address AS TEXT) as ip_address,
            m.ip_version, m.mac_address, m.hostname,
            m.status, m.last_seen, m.created_at, m.updated_at
        FROM ip_managers m
        WHERE m.position_id = $1
        ORDER BY m.ip_address"#
    ).bind(id)
    .fetch_all(pool.get_conn()).await {
        Ok(ips) => ips,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
        }
    };

    // 获取机位关联的机柜名称
    let cabinet_name =
        match sqlx::query_scalar::<_, String>("SELECT COALESCE((SELECT c.name FROM cabinets c JOIN positions p ON c.id = p.cabinet_id WHERE p.id = $1), '未知机柜')")
            .bind(id)
            .fetch_optional(pool.get_conn())
            .await
        {
            Ok(Some(name)) => name,
            Ok(None) => "未知机柜".to_string(),
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {}",
                        err
                    ))),
                );
            }
        };

    // 创建带详细信息的机位对象
    let position_with_details = CabinetPositionWithDetails {
        id: position.id,
        name: position.name,
        cabinet_id: position.cabinet_id,
        cabinet_name,
        start_u: position.start_u,
        end_u: position.end_u,
        network_id: position.network_id,
        ips: position_ips,
        ports: Vec::new(),
        description: position.description,
        created_at: position.created_at,
        updated_at: position.updated_at,
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<CabinetPositionWithDetails>::success(
            position_with_details,
            "机位获取成功",
        )),
    )
}

// 更新机位
pub async fn update_cabinet_position(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    req: web::Json<CabinetPositionUpdate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 验证更新机位请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "Validation error: {:?}",
                e
            ))),
        );
    }

    // 开启事务
    let mut tx = match pool.pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("开启事务失败: {}", err))));
        }
    };

    // 检查机位是否存在
    let existing_position: Option<Uuid> =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM positions WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
        {
            Ok(position) => position,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {}",
                        err
                    ))),
                );
            }
        };

    if existing_position.is_none() {
        return Ok(
            HttpResponse::NotFound().json(ApiResponse::<CabinetPosition>::error("机位未找到"))
        );
    }

    let now = Utc::now();

    // 更新机位基本信息
    if let Err(err) = sqlx::query(
        "UPDATE positions SET 
         name = COALESCE($1, name), 
         start_u = COALESCE($2, start_u),
         end_u = COALESCE($3, end_u),
         description = COALESCE($4, description), 
         updated_at = $5 
         WHERE id = $6",
    )
    .bind(&req.name)
    .bind(req.start_u)
    .bind(req.end_u)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库更新错误: {}", err))));
    }

    // 如果提供了IP列表，则更新IP管理记录
    if let Some(ips) = &req.ips {
        // 删除现有IP记录
        if let Err(err) = sqlx::query("DELETE FROM ip_managers WHERE position_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
        {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("删除IP记录失败: {}", err))));
        }

        // 创建新的IP记录
        for ip in ips {
            let ip_version = if ip.ip_address.contains(":") { 6i16 } else { 4i16 };
            
            if let Err(err) = sqlx::query(
                "INSERT INTO ip_managers (id, position_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, switch_id, switch_port_id, status, last_seen, created_at, updated_at) 
                 VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, $8, $9, $10, $11, $12, $13, $14)"
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(ip.device_type.as_deref().unwrap_or("cabinet_position"))
            .bind(ip.network_id)
            .bind(&ip.ip_address)
            .bind(ip_version)
            .bind(&ip.mac_address)
            .bind(&ip.hostname)
            .bind(ip.switch_id)
            .bind(ip.switch_port_id)
            .bind("active")
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&mut *tx).await {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("插入IP记录失败: {}", err))));
            }
        }
    }

    // 提交事务
    if let Err(err) = tx.commit().await {
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("提交事务失败: {}", err))));
    }

    // 查询更新后的完整机位信息（包含IP和端口）
    let row = match sqlx::query(
        "SELECT p.id, p.name, p.cabinet_id, c.name as cabinet_name, p.start_u, p.end_u, p.network_id, p.description, p.created_at::TIMESTAMPTZ, p.updated_at::TIMESTAMPTZ 
        FROM positions p 
        LEFT JOIN cabinets c ON p.cabinet_id = c.id 
        WHERE p.id = $1"
    ).bind(id)
    .fetch_one(pool.get_conn()).await {
        Ok(r) => r,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("查询机位失败: {}", err))));
        }
    };

    let position_with_details = CabinetPositionWithDetails {
        id: row.get("id"),
        name: row.get("name"),
        cabinet_id: row.get("cabinet_id"),
        cabinet_name: row.get::<Option<String>, _>("cabinet_name").unwrap_or_default(),
        start_u: row.get("start_u"),
        end_u: row.get("end_u"),
        network_id: row.get("network_id"),
        description: row.get("description"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        ips: vec![],
        ports: vec![],
    };

    // 查询IP信息
    let ips = match sqlx::query_as::<_, IpManager>(
        r#"SELECT id, workstation_id, position_id, switch_id, switch_port_id, device_type, network_id, 
           CAST(ip_address AS TEXT) as ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at
           FROM ip_managers WHERE position_id = $1"#
    ).bind(id)
    .fetch_all(pool.get_conn()).await {
        Ok(i) => i,
        Err(_) => vec![],
    };

    let result = CabinetPositionWithDetails {
        id: position_with_details.id,
        name: position_with_details.name,
        cabinet_id: position_with_details.cabinet_id,
        cabinet_name: position_with_details.cabinet_name,
        start_u: position_with_details.start_u,
        end_u: position_with_details.end_u,
        network_id: position_with_details.network_id,
        ips,
        ports: vec![],
        description: position_with_details.description,
        created_at: position_with_details.created_at,
        updated_at: position_with_details.updated_at,
    };

    // 记录操作日志
    let details = serde_json::json!({
        "name": result.name,
        "cabinet_id": result.cabinet_id,
        "start_u": result.start_u,
        "end_u": result.end_u,
        "description": result.description
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "update",
        "cabinet_position",
        &id,
        &details,
        true,
    )
    .await;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<CabinetPositionWithDetails>::success(
            result,
            "机位更新成功",
        )),
    )
}

// 删除机位
pub async fn delete_cabinet_position(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 开启事务
    let mut tx = match pool.pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("开启事务失败: {}", err))));
        }
    };

    // 检查机位是否存在
    let existing_position: Option<Uuid> =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM positions WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
        {
            Ok(position) => position,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {}",
                        err
                    ))),
                );
            }
        };

    if existing_position.is_none() {
        return Ok(
            HttpResponse::NotFound().json(ApiResponse::<CabinetPosition>::error("机位未找到"))
        );
    }

    // 检查是否有IP管理关联到该机位
    // 用户要求级联删除，因此直接删除关联的IP管理记录
    if let Err(err) = sqlx::query("DELETE FROM ip_managers WHERE position_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "删除IP管理记录失败: {}",
                err
            ))),
        );
    }

    // 删除机位
    if let Err(err) = sqlx::query("DELETE FROM positions WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "Database deletion error: {}",
                err
            ))),
        );
    }

    // 提交事务
    if let Err(err) = tx.commit().await {
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("提交事务失败: {}", err))));
    }

    // 记录操作日志
    let details = serde_json::json!({
        "position_id": id.to_string()
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "delete",
        "cabinet_position",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "机位删除成功")))
}

// 工位相关路由
// 获取所有工位
pub async fn get_workstations(
    pool: web::Data<DbPool>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    // 1. 获取查询参数
    let room_id = query.get("room_id");

    // 2. 根据是否有room_id参数构建不同的SQL查询
    let (workstations_basic, workstation_ids) = if let Some(room_id_str) = room_id {
        // 有room_id参数，解析为UUID类型
        match Uuid::parse_str(room_id_str) {
            Ok(room_id_uuid) => {
                // 查询该房间的工位
                let rows = match sqlx::query(
                    "SELECT w.id, w.name, w.room_id, 
                            COALESCE((SELECT r.name FROM rooms r WHERE r.id = w.room_id), '未知房间') as room_name, 
                            w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ 
                     FROM workstations w 
                     WHERE w.room_id = $1"
                ).bind(room_id_uuid)
                .fetch_all(pool.get_conn()).await {
                    Ok(rows) => rows,
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                };

                let mut workstations = Vec::new();
                let mut ids = Vec::new();
                for row in rows {
                    let id: Uuid = row.get("id");
                    ids.push(id);
                    workstations.push(row);
                }
                (workstations, ids)
            }
            Err(_) => {
                // 无效的UUID格式
                return Ok(
                    HttpResponse::BadRequest().json(ApiResponse::<()>::error("无效的room_id格式"))
                );
            }
        }
    } else {
        // 没有room_id参数，查询所有工位
        let rows = match sqlx::query(
            "SELECT w.id, w.name, w.room_id, 
                    COALESCE((SELECT r.name FROM rooms r WHERE r.id = w.room_id), '未知房间') as room_name, 
                    w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ 
             FROM workstations w"
        ).fetch_all(pool.get_conn()).await {
            Ok(rows) => rows,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
            }
        };

        let mut workstations = Vec::new();
        let mut ids = Vec::new();
        for row in rows {
            let id: Uuid = row.get("id");
            ids.push(id);
            workstations.push(row);
        }
        (workstations, ids)
    };

    // 3. 批量获取所有相关工位的端口信息
    let all_ports = if !workstation_ids.is_empty() {
        match sqlx::query_as::<_, WorkstationPortWithSwitchPort>(
            r#"SELECT wp.id, wp.workstation_id, wp.switch_port_id, sp.switch_id, COALESCE(s.name, '未知交换机') as switch_name, COALESCE(sp.port_number, '') as port_number, sp.port_name, wp.created_at::TIMESTAMPTZ, wp.updated_at::TIMESTAMPTZ 
               FROM workstation_ports wp 
               LEFT JOIN switch_ports sp ON wp.switch_port_id = sp.id 
               LEFT JOIN switches s ON sp.switch_id = s.id
               WHERE wp.workstation_id = ANY($1)"#
        ).bind(&workstation_ids)
        .fetch_all(pool.get_conn()).await {
            Ok(ports) => ports,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
            }
        }
    } else {
        Vec::new()
    };

    // 4. 将端口按workstation_id分组
    let mut ports_map: std::collections::HashMap<Uuid, Vec<WorkstationPortWithSwitchPort>> = std::collections::HashMap::new();
    for port in all_ports {
        ports_map.entry(port.workstation_id).or_default().push(port);
    }

    // 5. 组装最终结果
    let mut workstations_with_details = Vec::new();

    for row in workstations_basic {
        let id: Uuid = row.get("id");
        let name: String = row.get("name");
        let room_id: Uuid = row.get::<Option<Uuid>, _>("room_id").unwrap_or_else(Uuid::nil);
        let room_name: String = row.get::<Option<String>, _>("room_name").unwrap_or_else(|| "未知房间".to_string());
        let manager: Option<String> = row.get("manager");
        let description: Option<String> = row.get("description");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

        let workstation_with_details = WorkstationWithDetails {
            id,
            name,
            room_id,
            room_name,
            manager,
            ips: Vec::new(),
            description,
            created_at,
            updated_at,
        };

        workstations_with_details.push(workstation_with_details);
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<WorkstationWithDetails>>::success(
            workstations_with_details,
            "工位获取成功",
        )),
    )
}

// 创建工位
pub async fn create_workstation(
    pool: web::Data<DbPool>,
    req: web::Json<WorkstationCreate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 验证创建工位请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {:?}", e)))
        );
    }

    // 开启事务
    let mut tx = match pool.pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("开启事务失败: {}", err))));
        }
    };

    // 检查工位名称是否已存在
    let existing_workstation: Option<Uuid> = match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM workstations WHERE name = $1 AND room_id = $2",
    )
    .bind(&req.name)
    .bind(req.room_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(workstation) => workstation,
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "Database query error: {}",
                    err
                ))),
            );
        }
    };

    if existing_workstation.is_some() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<Workstation>::error("工位名称已存在"))
        );
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    // 创建工位
    if let Err(err) = sqlx::query(
        "INSERT INTO workstations (id, name, room_id, manager, description, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(id)
    .bind(&req.name)
    .bind(req.room_id)
    .bind(&req.manager)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(&mut *tx).await {
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库插入错误: {}", err))));
    }

    // 创建工位关联的IP地址
    let mut ip_count = 0;
    if let Some(ips) = &req.ips {
        for ip in ips {
            // 验证设备类型和对应的设备ID是否匹配
            let device_type = ip.device_type.as_deref().unwrap_or("");
            if device_type != "workstation" || ip.workstation_id.is_some() {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("设备类型与设备ID不匹配")));
            }

            // 检查IP地址是否已存在于同一网络
            let existing_mapping: Option<Uuid> =
                match sqlx::query_scalar::<_, Uuid>(
                    "SELECT id FROM ip_managers WHERE ip_address = CAST($1 AS INET) AND network_id = $2",
                )
                .bind(&ip.ip_address)
                .bind(ip.network_id)
                .fetch_optional(&mut *tx)
                .await
                {
                    Ok(mapping) => mapping,
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(
                            ApiResponse::<()>::error(format!("Database query error: {}", err)),
                        ));
                    }
                };

            if existing_mapping.is_some() {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("该网络中IP地址已存在")));
            }

            // 检查IP地址是否在所属网络的CIDR范围内
            let network = match sqlx::query(
                r#"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, n.ipv4_dns::TEXT, n.ipv6_dns::TEXT, NULL as gateway, NULL as dns, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
                   FROM network_cidrs n 
                   JOIN network_regions nt ON n.network_region_id = nt.id 
                   WHERE n.id = $1"#
            ).bind(ip.network_id)
            .fetch_optional(&mut *tx).await {
                Ok(Some(row)) => {
                    use crate::models::Network;
                    Network {
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
                    }
                },
                Ok(None) => {
                    return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("网络未找到")));
                },
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
                }
            };

            // 验证IP地址是否在网络的CIDR范围内
            let ip_in_cidr = match validate_ip_in_cidr(&ip.ip_address, &network) {
                Ok(valid) => valid,
                Err(response) => return Ok(response),
            };

            // 如果IP地址不在任何CIDR范围内，返回错误
            if !ip_in_cidr {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("IP地址不在所属网络网段内")));
            }

            // 检测IP地址版本
            let ip_version = detect_ip_version(&ip.ip_address);

            if let Err(err) = sqlx::query(
                "INSERT INTO ip_managers (id, workstation_id, position_id, switch_id, switch_port_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
                 VALUES ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15)"
            )
            .bind(Uuid::new_v4())
            .bind(Some(id))
            .bind(ip.position_id)
            .bind(ip.switch_id)
            .bind(ip.switch_port_id)
            .bind(&ip.device_type)
            .bind(ip.network_id)
            .bind(&ip.ip_address)
            .bind(ip_version)
            .bind(&ip.mac_address)
            .bind(&ip.hostname)
            .bind("active")
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&mut *tx).await {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库插入错误: {}", err))));
            }

            ip_count += 1;
        }
    }

    if let Err(err) = tx.commit().await {
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("提交事务失败: {}", err))));
    }

    let workstation = Workstation {
        id,
        name: req.name.clone(),
        room_id: req.room_id,
        room_name: None,
        manager: req.manager.clone(),
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    // 记录操作日志
    let details = serde_json::json!({
        "name": workstation.name,
        "room_id": workstation.room_id,
        "manager": workstation.manager,
        "description": workstation.description,
        "ip_count": ip_count
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "create",
        "workstation",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<Workstation>::success(
        workstation,
        "工位创建成功",
    )))
}

// 获取单个工位
pub async fn get_workstation(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 获取工位基本信息
    let workstation = match sqlx::query_as::<_, Workstation>(
        "SELECT w.id, w.name, w.room_id, r.name as room_name, w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ FROM workstations w LEFT JOIN rooms r ON w.room_id = r.id WHERE w.id = $1"
    ).bind(id)
    .fetch_optional(pool.get_conn()).await {
        Ok(Some(workstation)) => workstation,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<Workstation>::error("工位未找到")));
        },
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
        }
    };

    // 获取工位关联的IP信息
    let workstation_ips = match sqlx::query_as::<_, IpManager>(
        r#"SELECT 
            m.id, m.workstation_id, m.position_id, m.switch_id, m.switch_port_id,
            m.device_type, m.network_id, 
            CAST(m.ip_address AS TEXT) as ip_address,
            m.ip_version, m.mac_address, m.hostname,
            m.status, m.last_seen, m.created_at, m.updated_at
        FROM ip_managers m
        WHERE m.workstation_id = $1
        ORDER BY m.ip_address"#
    ).bind(id)
    .fetch_all(pool.get_conn()).await {
        Ok(ips) => ips,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
        }
    };

    // 获取工位关联的房间名称
    let room_name = match sqlx::query_scalar::<_, String>("SELECT COALESCE((SELECT r.name FROM rooms r JOIN workstations w ON r.id = w.room_id WHERE w.id = $1), '未知房间')")
        .bind(id)
        .fetch_optional(pool.get_conn())
        .await
    {
        Ok(Some(name)) => name,
        Ok(None) => "未知房间".to_string(),
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))),
            );
        }
    };

    // 创建带详细信息的工位对象
    let workstation_with_details = WorkstationWithDetails {
        id: workstation.id,
        name: workstation.name,
        room_id: workstation.room_id,
        room_name,
        manager: workstation.manager.clone(),
        ips: workstation_ips,
        description: workstation.description,
        created_at: workstation.created_at,
        updated_at: workstation.updated_at,
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<WorkstationWithDetails>::success(
            workstation_with_details,
            "工位获取成功",
        )),
    )
}

// 更新工位
pub async fn update_workstation(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    req: web::Json<WorkstationUpdate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 验证更新工位请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "Validation error: {:?}",
                e
            ))),
        );
    }

    // 开启事务
    let mut tx = match pool.pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("开启事务失败: {}", err))));
        }
    };

    // 检查工位是否存在
    let existing_workstation: Option<Uuid> =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM workstations WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
        {
            Ok(workstation) => workstation,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {}",
                        err
                    ))),
                );
            }
        };

    if existing_workstation.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<Workstation>::error("工位未找到")));
    }

    let now = Utc::now();

    // 更新工位基本信息
    if let Err(err) = sqlx::query(
        "UPDATE workstations SET 
         name = COALESCE($1, name), 
         room_id = COALESCE($2, room_id), 
         manager = COALESCE($3, manager), 
         description = COALESCE($4, description), 
         updated_at = $5 
         WHERE id = $6",
    )
    .bind(&req.name)
    .bind(req.room_id)
    .bind(&req.manager)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库更新错误: {}", err))));
    }

    // 如果提供了IP列表，则更新IP管理记录
    if let Some(ips) = &req.ips {
        // 删除现有IP记录
        if let Err(err) = sqlx::query("DELETE FROM ip_managers WHERE workstation_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
        {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("删除IP记录失败: {}", err))));
        }

        // 创建新的IP记录
        for ip in ips {
            let ip_version = if ip.ip_address.contains(":") { 6i16 } else { 4i16 };
            
            if let Err(err) = sqlx::query(
                "INSERT INTO ip_managers (id, workstation_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, switch_id, switch_port_id, status, last_seen, created_at, updated_at) 
                 VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, $8, $9, $10, $11, $12, $13, $14)"
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(ip.device_type.as_deref().unwrap_or("workstation"))
            .bind(ip.network_id)
            .bind(&ip.ip_address)
            .bind(ip_version)
            .bind(&ip.mac_address)
            .bind(&ip.hostname)
            .bind(ip.switch_id)
            .bind(ip.switch_port_id)
            .bind("active")
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&mut *tx).await {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("插入IP记录失败: {}", err))));
            }
        }
    }

    // 提交事务
    if let Err(err) = tx.commit().await {
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("提交事务失败: {}", err))));
    }

    // 查询更新后的完整工位信息（包含IP和端口）
    let row = match sqlx::query(
        r#"SELECT w.id, w.name, w.room_id, r.name as room_name, w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ 
        FROM workstations w 
        LEFT JOIN rooms r ON w.room_id = r.id 
        WHERE w.id = $1"#
    ).bind(id)
    .fetch_one(pool.get_conn()).await {
        Ok(r) => r,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("查询工位失败: {}", err))));
        }
    };

    let workstation_with_details = WorkstationWithDetails {
        id: row.get("id"),
        name: row.get("name"),
        room_id: row.get("room_id"),
        room_name: row.get::<Option<String>, _>("room_name").unwrap_or_default(),
        manager: row.get("manager"),
        description: row.get("description"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        ips: vec![],
    };

    // 查询IP信息
    let ips = match sqlx::query_as::<_, IpManager>(
        r#"SELECT id, workstation_id, position_id, switch_id, switch_port_id, device_type, network_id, 
           CAST(ip_address AS TEXT) as ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at
           FROM ip_managers WHERE workstation_id = $1"#
    ).bind(id)
    .fetch_all(pool.get_conn()).await {
        Ok(i) => i,
        Err(_) => vec![],
    };

    let result = WorkstationWithDetails {
        id: workstation_with_details.id,
        name: workstation_with_details.name,
        room_id: workstation_with_details.room_id,
        room_name: workstation_with_details.room_name,
        manager: workstation_with_details.manager,
        ips,
        description: workstation_with_details.description,
        created_at: workstation_with_details.created_at,
        updated_at: workstation_with_details.updated_at,
    };

    // 记录操作日志
    let details = serde_json::json!({
        "name": result.name,
        "room_id": result.room_id,
        "manager": result.manager,
        "description": result.description
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "update",
        "workstation",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<WorkstationWithDetails>::success(
        result,
        "工位更新成功",
    )))
}

// 删除工位
pub async fn delete_workstation(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 开启事务
    let mut tx = match pool.pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("开启事务失败: {}", err))));
        }
    };

    // 检查工位是否存在
    let existing_workstation: Option<Uuid> =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM workstations WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
        {
            Ok(workstation) => workstation,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {}",
                        err
                    ))),
                );
            }
        };

    if existing_workstation.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<Workstation>::error("工位未找到")));
    }

    // 先删除关联的IP管理记录
    if let Err(err) = sqlx::query("DELETE FROM ip_managers WHERE workstation_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "删除IP管理记录失败: {}",
                err
            ))),
        );
    }

    // 删除对应的布局数据
    if let Err(err) = sqlx::query("DELETE FROM svg_layouts WHERE element_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "删除布局数据失败: {}",
                err
            ))),
        );
    }

    // 删除工位
    if let Err(err) = sqlx::query("DELETE FROM workstations WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "Database deletion error: {}",
                err
            ))),
        );
    }

    // 提交事务
    if let Err(err) = tx.commit().await {
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("提交事务失败: {}", err))));
    }

    // 记录操作日志
    let details = serde_json::json!({
        "workstation_id": id.to_string()
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "delete",
        "workstation",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "工位删除成功")))
}
