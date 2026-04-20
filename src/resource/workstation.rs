use crate::config::Config;
use crate::db::DbPool;
use crate::models::{
    ApiResponse, IpManager, Workstation, WorkstationCreate, WorkstationPortWithSwitchPort,
    WorkstationUpdate, WorkstationWithDetails,
};
use crate::resource::ip::detect_ip_version;
use crate::utils::{DEFAULT_PAGE, log_system_operation, validate_ip_in_cidr};
use actix_web::{HttpRequest, HttpResponse, Result, web};
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

pub async fn get_workstations(
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
    let room_id = query.get("room_id").cloned();
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());
    let offset = (page - 1) * page_size;

    let search_pattern = format!("%{search}%");
    let parsed_room_id = room_id.as_ref().and_then(|id| Uuid::parse_str(id).ok());

    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY w.name DESC",
        ("room_name", "desc") => "ORDER BY room_name DESC, w.name ASC",
        ("room_name", _) => "ORDER BY room_name ASC, w.name ASC",
        ("manager", "desc") => "ORDER BY w.manager DESC, w.name ASC",
        ("manager", _) => "ORDER BY w.manager ASC, w.name ASC",
        ("created_at", "desc") => "ORDER BY w.created_at DESC",
        ("created_at", _) => "ORDER BY w.created_at ASC",
        _ => "ORDER BY w.name ASC",
    };

    let (total, workstations_basic) = if search.is_empty() && parsed_room_id.is_none() {
        let total: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM workstations w")
            .fetch_one(pool.get_conn())
            .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("数据库查询错误: {err}"))));
            }
        };

        let workstations_basic = match sqlx::query(&format!(
            "SELECT w.id, w.name, w.room_id, 
                    COALESCE((SELECT r.name FROM rooms r WHERE r.id = w.room_id), '未知房间') as room_name, 
                    w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ 
             FROM workstations w {order_clause} LIMIT $1 OFFSET $2"
        ))
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
        {
            Ok(w) => w,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {err}"),
                )));
            }
        };

        (total, workstations_basic)
    } else if parsed_room_id.is_some() && search.is_empty() {
        let total: i64 =
            match sqlx::query_scalar("SELECT COUNT(*) FROM workstations w WHERE w.room_id = $1")
                .bind(parsed_room_id)
                .fetch_one(pool.get_conn())
                .await
            {
                Ok(t) => t,
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error(format!("数据库查询错误: {err}"))));
                }
            };

        let workstations_basic = match sqlx::query(&format!(
            "SELECT w.id, w.name, w.room_id, 
                    COALESCE((SELECT r.name FROM rooms r WHERE r.id = w.room_id), '未知房间') as room_name, 
                    w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ 
             FROM workstations w WHERE w.room_id = $1 {order_clause} LIMIT $2 OFFSET $3"
        ))
        .bind(parsed_room_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
        {
            Ok(w) => w,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {err}"),
                )));
            }
        };

        (total, workstations_basic)
    } else if parsed_room_id.is_some() {
        let total: i64 = match sqlx::query_scalar(
            "SELECT COUNT(*) FROM workstations w WHERE w.room_id = $1 AND (w.name ILIKE $2 OR w.manager ILIKE $2 OR w.description ILIKE $2)"
        )
        .bind(parsed_room_id)
        .bind(&search_pattern)
        .fetch_one(pool.get_conn())
        .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {err}"),
                )));
            }
        };

        let workstations_basic = match sqlx::query(&format!(
            "SELECT w.id, w.name, w.room_id, 
                    COALESCE((SELECT r.name FROM rooms r WHERE r.id = w.room_id), '未知房间') as room_name, 
                    w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ 
             FROM workstations w WHERE w.room_id = $1 AND (w.name ILIKE $2 OR w.manager ILIKE $2 OR w.description ILIKE $2) {order_clause} LIMIT $3 OFFSET $4"
        ))
        .bind(parsed_room_id)
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
        {
            Ok(w) => w,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {err}"),
                )));
            }
        };

        (total, workstations_basic)
    } else {
        let total: i64 = match sqlx::query_scalar(
            "SELECT COUNT(*) FROM workstations w WHERE w.name ILIKE $1 OR w.manager ILIKE $1 OR w.description ILIKE $1"
        )
        .bind(&search_pattern)
        .fetch_one(pool.get_conn())
        .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {err}"),
                )));
            }
        };

        let workstations_basic = match sqlx::query(&format!(
            "SELECT w.id, w.name, w.room_id, 
                    COALESCE((SELECT r.name FROM rooms r WHERE r.id = w.room_id), '未知房间') as room_name, 
                    w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ 
             FROM workstations w WHERE w.name ILIKE $1 OR w.manager ILIKE $1 OR w.description ILIKE $1 {order_clause} LIMIT $2 OFFSET $3"
        ))
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
        {
            Ok(w) => w,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {err}"),
                )));
            }
        };

        (total, workstations_basic)
    };

    let mut workstation_ids = Vec::new();
    for row in &workstations_basic {
        let id: Uuid = row.get("id");
        workstation_ids.push(id);
    }

    let all_ports = if workstation_ids.is_empty() {
        Vec::new()
    } else {
        match sqlx::query_as::<_, WorkstationPortWithSwitchPort>(
            r"SELECT wp.id, wp.workstation_id, wp.switch_port_id, sp.switch_id, COALESCE(s.name, '未知交换机') as switch_name, COALESCE(sp.port_number, '') as port_number, sp.port_name, wp.created_at::TIMESTAMPTZ, wp.updated_at::TIMESTAMPTZ 
               FROM workstation_ports wp 
               LEFT JOIN switch_ports sp ON wp.switch_port_id = sp.id 
               LEFT JOIN switches s ON sp.switch_id = s.id
               WHERE wp.workstation_id = ANY($1)"
        ).bind(&workstation_ids)
        .fetch_all(pool.get_conn()).await {
            Ok(ports) => ports,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {err}"))));
            }
        }
    };

    let mut ports_map: std::collections::HashMap<Uuid, Vec<WorkstationPortWithSwitchPort>> =
        std::collections::HashMap::new();
    for port in all_ports {
        ports_map.entry(port.workstation_id).or_default().push(port);
    }

    let mut workstations_with_details = Vec::new();

    for row in workstations_basic {
        let id: Uuid = row.get("id");
        let name: String = row.get("name");
        let room_id: Uuid = row
            .get::<Option<Uuid>, _>("room_id")
            .unwrap_or_else(Uuid::nil);
        let room_name: String = row
            .get::<Option<String>, _>("room_name")
            .unwrap_or_else(|| "未知房间".to_string());
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

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": workstations_with_details,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "工位获取成功",
    )))
}

pub async fn create_workstation(
    pool: web::Data<DbPool>,
    req: web::Json<WorkstationCreate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {e:?}")))
        );
    }

    let mut tx = match pool.pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("开启事务失败: {err}"))));
        }
    };

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
                    "Database query error: {err}"
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
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库插入错误: {err}"))));
    }

    let mut ip_count = 0;
    if let Some(ips) = &req.ips {
        for ip in ips {
            let device_type = ip.device_type.as_deref().unwrap_or("");
            if device_type != "workstation" || ip.workstation_id.is_some() {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("设备类型与设备ID不匹配")));
            }

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
                            ApiResponse::<()>::error(format!("Database query error: {err}")),
                        ));
                    }
                };

            if existing_mapping.is_some() {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("该网络中IP地址已存在")));
            }

            let network =
                match sqlx::query(crate::utils::NETWORK_QUERY)
                    .bind(ip.network_id)
                    .fetch_optional(&mut *tx)
                    .await
                {
                    Ok(Some(row)) => crate::utils::parse_network_from_row(&row),
                    Ok(None) => {
                        return Ok(
                            HttpResponse::BadRequest().json(ApiResponse::<()>::error("网络未找到"))
                        );
                    }
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(
                            ApiResponse::<()>::error(format!("Database query error: {err}")),
                        ));
                    }
                };

            let ip_in_cidr = match validate_ip_in_cidr(&ip.ip_address, &network) {
                Ok(valid) => valid,
                Err(response) => return Ok(response),
            };

            if !ip_in_cidr {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("IP地址不在所属网络网段内")));
            }

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
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库插入错误: {err}"))));
            }

            ip_count += 1;
        }
    }

    if let Err(err) = tx.commit().await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("提交事务失败: {err}"))));
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

pub async fn get_workstation(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = *id_path;

    let workstation = match sqlx::query_as::<_, Workstation>(
        "SELECT w.id, w.name, w.room_id, r.name as room_name, w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ FROM workstations w LEFT JOIN rooms r ON w.room_id = r.id WHERE w.id = $1"
    ).bind(id)
    .fetch_optional(pool.get_conn()).await {
        Ok(Some(workstation)) => workstation,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<Workstation>::error("工位未找到")));
        },
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {err}"))));
        }
    };

    let workstation_ips = match sqlx::query_as::<_, IpManager>(
        r"SELECT 
            m.id, m.workstation_id, m.position_id, m.switch_id, m.switch_port_id,
            m.device_type, m.network_id, 
            host(m.ip_address) as ip_address,
            m.ip_version, m.mac_address, m.hostname,
            m.status, m.last_seen, m.created_at, m.updated_at
        FROM ip_managers m
        WHERE m.workstation_id = $1
        ORDER BY m.ip_address",
    )
    .bind(id)
    .fetch_all(pool.get_conn())
    .await
    {
        Ok(ips) => ips,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("数据库查询错误: {err}"))));
        }
    };

    let room_name = match sqlx::query_scalar::<_, String>("SELECT COALESCE((SELECT r.name FROM rooms r JOIN workstations w ON r.id = w.room_id WHERE w.id = $1), '未知房间')")
        .bind(id)
        .fetch_optional(pool.get_conn())
        .await
    {
        Ok(Some(name)) => name,
        Ok(None) => "未知房间".to_string(),
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {err}"))),
            );
        }
    };

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

pub async fn update_workstation(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    req: web::Json<WorkstationUpdate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "Validation error: {e:?}"
            ))),
        );
    }

    let mut tx = match pool.pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("开启事务失败: {err}"))));
        }
    };

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
                        "Database query error: {err}"
                    ))),
                );
            }
        };

    if existing_workstation.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<Workstation>::error("工位未找到")));
    }

    let now = Utc::now();

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
            .json(ApiResponse::<()>::error(format!("数据库更新错误: {err}"))));
    }

    if let Some(ips) = &req.ips {
        if let Err(err) = sqlx::query("DELETE FROM ip_managers WHERE workstation_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
        {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("删除IP记录失败: {err}"))));
        }

        for ip in ips {
            let ip_version = if ip.ip_address.contains(':') {
                6i16
            } else {
                4i16
            };

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
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("插入IP记录失败: {err}"))));
            }
        }
    }

    if let Err(err) = tx.commit().await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("提交事务失败: {err}"))));
    }

    let row = match sqlx::query(
        r"SELECT w.id, w.name, w.room_id, r.name as room_name, w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ 
        FROM workstations w 
        LEFT JOIN rooms r ON w.room_id = r.id 
        WHERE w.id = $1"
    ).bind(id)
    .fetch_one(pool.get_conn()).await {
        Ok(r) => r,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("查询工位失败: {err}"))));
        }
    };

    let ips: Vec<IpManager> = sqlx::query_as(
        r"SELECT id, workstation_id, position_id, switch_id, switch_port_id, device_type, network_id, 
           host(ip_address) as ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at
           FROM ip_managers WHERE workstation_id = $1"
    ).bind(id)
    .fetch_all(pool.get_conn()).await.unwrap_or_default();

    let result = WorkstationWithDetails {
        id: row.get("id"),
        name: row.get("name"),
        room_id: row.get("room_id"),
        room_name: row
            .get::<Option<String>, _>("room_name")
            .unwrap_or_default(),
        manager: row.get("manager"),
        ips,
        description: row.get("description"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };

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

    Ok(
        HttpResponse::Ok().json(ApiResponse::<WorkstationWithDetails>::success(
            result,
            "工位更新成功",
        )),
    )
}

pub async fn delete_workstation(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    let mut tx = match pool.pool.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("开启事务失败: {err}"))));
        }
    };

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
                        "Database query error: {err}"
                    ))),
                );
            }
        };

    if existing_workstation.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<Workstation>::error("工位未找到")));
    }

    if let Err(err) = sqlx::query("DELETE FROM ip_managers WHERE workstation_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "删除IP管理记录失败: {err}"
            ))),
        );
    }

    if let Err(err) = sqlx::query("DELETE FROM svg_layouts WHERE element_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "删除布局数据失败: {err}"
            ))),
        );
    }

    if let Err(err) = sqlx::query("DELETE FROM workstations WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "Database deletion error: {err}"
            ))),
        );
    }

    if let Err(err) = tx.commit().await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("提交事务失败: {err}"))));
    }

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
