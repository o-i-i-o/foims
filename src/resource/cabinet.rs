use crate::config::Config;
use crate::db::DbPool;
use crate::models::{
    ApiResponse, CabinetPosition, CabinetPositionCreate,
    CabinetPositionUpdate, CabinetPositionWithDetails, IpManager,
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

pub async fn get_positions(
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
    let cabinet_id = query
        .get("cabinet_id")
        .and_then(|id| Uuid::parse_str(id).ok());
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());
    let offset = (page - 1) * page_size;

    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY name DESC",
        ("cabinet_name", "desc") => "ORDER BY cabinet_name DESC, name ASC",
        ("cabinet_name", _) => "ORDER BY cabinet_name ASC, name ASC",
        ("start_u", "desc") => "ORDER BY start_u DESC, name ASC",
        ("start_u", _) => "ORDER BY start_u ASC, name ASC",
        ("created_at", "desc") => "ORDER BY created_at DESC",
        ("created_at", _) => "ORDER BY created_at ASC",
        _ => "ORDER BY name ASC",
    };

    // 查询总数（所有机位都在positions表中）
    let total: i64 = if !search.is_empty() || cabinet_id.is_some() {
        let count_result = if let Some(cid) = cabinet_id {
            if search.is_empty() {
                sqlx::query_scalar("SELECT COUNT(*) FROM positions WHERE cabinet_id = $1")
                    .bind(cid)
                    .fetch_one(pool.get_conn())
                    .await
            } else {
                let pattern = format!("%{search}%");
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM positions WHERE cabinet_id = $1 AND (name ILIKE $2 OR description ILIKE $2)"
                )
                .bind(cid)
                .bind(&pattern)
                .fetch_one(pool.get_conn())
                .await
            }
        } else {
            let pattern = format!("%{search}%");
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM positions WHERE name ILIKE $1 OR description ILIKE $1",
            )
            .bind(&pattern)
            .fetch_one(pool.get_conn())
            .await
        };
        match count_result {
            Ok(t) => t,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询机位数量失败"));
            }
        }
    } else {
        match sqlx::query_scalar("SELECT COUNT(*) FROM positions")
            .fetch_one(pool.get_conn())
            .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询机位数量失败"));
            }
        }
    };

    // 查询机位列表（所有机位都在positions表中）
    let positions_basic = if !search.is_empty() || cabinet_id.is_some() {
        let query_result = if let Some(cid) = cabinet_id {
            if search.is_empty() {
                sqlx::query(
                    &format!(
                        r"SELECT p.id, p.name, p.cabinet_id, 
                                  COALESCE((SELECT c.name FROM cabinets c WHERE c.id = p.cabinet_id), '未知机柜') as cabinet_name, 
                                  p.start_u, p.end_u, p.description, 
                                  p.device_type, p.device_id,
                                  p.created_at::TIMESTAMPTZ as created_at, p.updated_at::TIMESTAMPTZ as updated_at
                           FROM positions p 
                           WHERE p.cabinet_id = $1
                           {order_clause} LIMIT $2 OFFSET $3"
                    )
                )
                .bind(cid)
                .bind(page_size)
                .bind(offset)
                .fetch_all(pool.get_conn())
                .await
            } else {
                let pattern = format!("%{search}%");
                sqlx::query(
                    &format!(
                        r"SELECT p.id, p.name, p.cabinet_id, 
                                  COALESCE((SELECT c.name FROM cabinets c WHERE c.id = p.cabinet_id), '未知机柜') as cabinet_name, 
                                  p.start_u, p.end_u, p.description, 
                                  p.device_type, p.device_id,
                                  p.created_at::TIMESTAMPTZ as created_at, p.updated_at::TIMESTAMPTZ as updated_at
                           FROM positions p 
                           WHERE p.cabinet_id = $1 AND (p.name ILIKE $2 OR p.description ILIKE $2)
                           {order_clause} LIMIT $3 OFFSET $4"
                    )
                )
                .bind(cid)
                .bind(&pattern)
                .bind(page_size)
                .bind(offset)
                .fetch_all(pool.get_conn())
                .await
            }
        } else {
            let pattern = format!("%{search}%");
            sqlx::query(
                &format!(
                    r"SELECT p.id, p.name, p.cabinet_id, 
                              COALESCE((SELECT c.name FROM cabinets c WHERE c.id = p.cabinet_id), '未知机柜') as cabinet_name, 
                              p.start_u, p.end_u, p.description, 
                              p.device_type, p.device_id,
                              p.created_at::TIMESTAMPTZ as created_at, p.updated_at::TIMESTAMPTZ as updated_at
                       FROM positions p 
                       WHERE p.name ILIKE $1 OR p.description ILIKE $1
                       {order_clause} LIMIT $2 OFFSET $3"
                )
            )
            .bind(&pattern)
            .bind(page_size)
            .bind(offset)
            .fetch_all(pool.get_conn())
            .await
        };
        match query_result {
            Ok(rows) => rows,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询机位列表失败"));
            }
        }
    } else {
        match sqlx::query(
            &format!(
                r"SELECT p.id, p.name, p.cabinet_id, 
                          COALESCE((SELECT c.name FROM cabinets c WHERE c.id = p.cabinet_id), '未知机柜') as cabinet_name, 
                          p.start_u, p.end_u, p.description, 
                          p.device_type, p.device_id,
                          p.created_at::TIMESTAMPTZ as created_at, p.updated_at::TIMESTAMPTZ as updated_at
                   FROM positions p 
                   {order_clause} LIMIT $1 OFFSET $2"
            )
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
        {
            Ok(rows) => rows,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询机位列表失败"));
            }
        }
    };

    let mut position_ids = Vec::new();
    for row in &positions_basic {
        let id: Uuid = row.get("id");
        position_ids.push(id);
    }

    let mut positions_with_details = Vec::new();

    for row in positions_basic {
        let id: Uuid = row.get("id");
        let name: String = row.get("name");
        let cabinet_id: Option<Uuid> = row.get("cabinet_id");
        let cabinet_name: Option<String> = row.get("cabinet_name");
        let start_u: i32 = row.get("start_u");
        let end_u: i32 = row.get("end_u");
        let device_type: Option<String> = row.get("device_type");
        let description: Option<String> = row.get("description");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");
        let device_id: Option<Uuid> = row.get("device_id");

        let position_with_details = CabinetPositionWithDetails {
            id,
            name,
            cabinet_id,
            cabinet_name,
            start_u,
            end_u,
            device_type,
            device_id,
            ips: Vec::new(),
            description,
            created_at,
            updated_at,
        };

        positions_with_details.push(position_with_details);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": positions_with_details,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "机位获取成功",
    )))
}

pub async fn create_cabinet_position(
    pool: web::Data<DbPool>,
    req: web::Json<CabinetPositionCreate>,
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
            return Ok(crate::utils::handle_db_error(err, "开启事务失败"));
        }
    };

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
            return Ok(crate::utils::handle_db_error(err, "查询机位失败"));
        }
    };

    if existing_position.is_some() {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<CabinetPosition>::error("机位名称已存在")));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    if let Err(err) = sqlx::query(
        "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, device_type, device_id, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, 'cabinet_position', NULL, $7, $8)"
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
        return Ok(crate::utils::handle_db_error(err, "创建机位失败"));
    }

    let mut ip_count = 0;
    if let Some(ips) = &req.ips {
        for ip in ips {
            let device_type = ip.device_type.as_deref().unwrap_or("");
            if device_type != "cabinet_position" || ip.position_id.is_some() {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("设备类型与设备ID不匹配")));
            }

            let existing_mapping: Option<Uuid> =
                match sqlx::query_scalar::<_, Uuid>(
                    "SELECT id FROM ips WHERE ip_address = CAST($1 AS INET) AND network_id = $2",
                )
                .bind(&ip.ip_address)
                .bind(ip.network_id)
                .fetch_optional(&mut *tx)
                .await
                {
                    Ok(mapping) => mapping,
                    Err(err) => {
                        return Ok(crate::utils::handle_db_error(err, "查询IP地址失败"));
                    }
                };

            if existing_mapping.is_some() {
                return Ok(HttpResponse::BadRequest()
                    .json(ApiResponse::<()>::error("该网络中IP地址已存在")));
            }

            let network = match sqlx::query(crate::utils::NETWORK_QUERY)
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
                    return Ok(crate::utils::handle_db_error(err, "查询网络失败"));
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
                "INSERT INTO ips (id, workstation_id, position_id, switch_port_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
                 VALUES ($1, $2, $3, $4, $5, $6, CAST($7 AS INET), $8, $9, $10, $11, $12, $13, $14)"
            )
            .bind(Uuid::new_v4())
            .bind(ip.workstation_id)
            .bind(Some(id))
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
                return Ok(crate::utils::handle_db_error(err, "创建IP记录失败"));
            }

            ip_count += 1;
        }
    }

    if let Err(err) = tx.commit().await {
        return Ok(crate::utils::handle_db_error(err, "提交事务失败"));
    }

    let position = CabinetPosition {
        id,
        name: req.name.clone(),
        cabinet_id: req.cabinet_id,
        start_u: req.start_u,
        end_u: req.end_u,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
        device_type: Some("cabinet_position".to_string()),
        device_id: None,
    };

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

pub async fn get_cabinet_position(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = *id_path;

    let position_data = match sqlx::query(
        r"SELECT id, name, cabinet_id, start_u, end_u, description, 
                  device_type, device_id,
                  created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ 
           FROM positions WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool.get_conn())
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            return Ok(HttpResponse::NotFound()
                .json(ApiResponse::<serde_json::Value>::error("机位未找到")));
        }
        Err(err) => {
            return Ok(crate::utils::handle_db_error(err, "查询机位失败"));
        }
    };

    let device_type: Option<String> = position_data.get("device_type");
    let device_id: Option<Uuid> = position_data.get("device_id");
    let is_switch = device_type.as_deref() == Some("switch");

    let position_ips = if is_switch {
        match sqlx::query(
            r"SELECT 
                m.id, m.workstation_id, m.position_id, m.switch_port_id,
                m.device_type, m.network_id, 
                host(m.ip_address) as ip_address,
                m.ip_version, m.mac_address, m.hostname,
                m.status, m.last_seen, m.created_at, m.updated_at,
                n.network_region_id, nr.name as network_region
            FROM ips m
            LEFT JOIN network_cidrs n ON m.network_id = n.id
            LEFT JOIN network_regions nr ON n.network_region_id = nr.id
            WHERE m.position_id = (SELECT p.id FROM positions p WHERE p.device_type = 'switch' AND p.device_id = $1)
            ORDER BY m.ip_address",
        )
        .bind(device_id)
        .fetch_all(pool.get_conn())
        .await
        {
            Ok(ips) => ips,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询交换机IP信息失败"));
            }
        }
    } else {
        match sqlx::query(
            r"SELECT 
                m.id, m.workstation_id, m.position_id, m.switch_port_id,
                m.device_type, m.network_id, 
                host(m.ip_address) as ip_address,
                m.ip_version, m.mac_address, m.hostname,
                m.status, m.last_seen, m.created_at, m.updated_at,
                n.network_region_id, nr.name as network_region
            FROM ips m
            LEFT JOIN network_cidrs n ON m.network_id = n.id
            LEFT JOIN network_regions nr ON n.network_region_id = nr.id
            WHERE m.position_id = $1
            ORDER BY m.ip_address",
        )
        .bind(id)
        .fetch_all(pool.get_conn())
        .await
        {
            Ok(ips) => ips,
            Err(err) => {
                return Ok(crate::utils::handle_db_error(err, "查询机位IP信息失败"));
            }
        }
    };

    let ips_with_region: Vec<serde_json::Value> = position_ips
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.get::<Uuid, _>(0),
                "workstation_id": row.get::<Option<Uuid>, _>(1),
                "position_id": row.get::<Option<Uuid>, _>(2),
                "switch_port_id": row.get::<Option<Uuid>, _>(3),
                "device_type": row.get::<Option<String>, _>(4),
                "network_id": row.get::<Uuid, _>(5),
                "ip_address": row.get::<String, _>(6),
                "ip_version": row.get::<i16, _>(7),
                "mac_address": row.get::<Option<String>, _>(8),
                "hostname": row.get::<Option<String>, _>(9),
                "status": row.get::<String, _>(10),
                "last_seen": row.get::<chrono::DateTime<chrono::Utc>, _>(11),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>(12),
                "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>(13),
                "network_region_id": row.get::<Option<Uuid>, _>(14),
                "network_region": row.get::<Option<String>, _>(15)
            })
        })
        .collect();

    let cabinet_name: String = if position_data.get::<Option<Uuid>, _>("cabinet_id").is_some() {
        let cabinet_id: Uuid = position_data.get("cabinet_id");
        match sqlx::query_scalar::<_, String>("SELECT name FROM cabinets WHERE id = $1")
            .bind(cabinet_id)
            .fetch_optional(pool.get_conn())
            .await
        {
            Ok(Some(name)) => name,
            _ => "未知机柜".to_string(),
        }
    } else {
        "未知机柜".to_string()
    };

    let position_with_details = serde_json::json!({
        "id": position_data.get::<Uuid, _>("id"),
        "name": position_data.get::<String, _>("name"),
        "cabinet_id": position_data.get::<Option<Uuid>, _>("cabinet_id"),
        "cabinet_name": cabinet_name,
        "start_u": position_data.get::<i32, _>("start_u"),
        "end_u": position_data.get::<i32, _>("end_u"),
        "device_type": device_type,
        "device_id": device_id,
        "ips": ips_with_region,
        "description": position_data.get::<Option<String>, _>("description"),
        "created_at": position_data.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updated_at": position_data.get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
    });

    Ok(
        HttpResponse::Ok().json(ApiResponse::<serde_json::Value>::success(
            position_with_details,
            "机位获取成功",
        )),
    )
}

pub async fn update_cabinet_position(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    req: web::Json<CabinetPositionUpdate>,
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

    let position_info: Option<(Uuid, String, Option<Uuid>)> =
        match sqlx::query_as::<_, (Uuid, String, Option<Uuid>)>(
            "SELECT id, device_type, device_id FROM positions WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        {
            Ok(pos) => pos,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {err}"
                    ))),
                );
            }
        };

    let Some((_position_id, device_type, _device_id)) = position_info else {
        return Ok(
            HttpResponse::NotFound().json(ApiResponse::<CabinetPosition>::error("机位未找到"))
        )
    };

    let is_switch = device_type == "switch";

    if is_switch {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "交换机机位请通过交换机管理页面编辑",
        )));
    }

    let now = Utc::now();

    if let Err(err) = sqlx::query(
        "UPDATE positions SET 
         name = COALESCE($1, name), 
         cabinet_id = COALESCE($2, cabinet_id),
         start_u = COALESCE($3, start_u),
         end_u = COALESCE($4, end_u),
         description = COALESCE($5, description), 
         updated_at = $6 
         WHERE id = $7",
    )
    .bind(&req.name)
    .bind(req.cabinet_id)
    .bind(req.start_u)
    .bind(req.end_u)
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
        if let Err(err) = sqlx::query("DELETE FROM ips WHERE position_id = $1")
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
                "INSERT INTO ips (id, position_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, switch_port_id, status, last_seen, created_at, updated_at) 
                 VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, $8, $9, $10, $11, $12, $13)"
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(ip.device_type.as_deref().unwrap_or("cabinet_position"))
            .bind(ip.network_id)
            .bind(&ip.ip_address)
            .bind(ip_version)
            .bind(&ip.mac_address)
            .bind(&ip.hostname)
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
        "SELECT p.id, p.name, p.cabinet_id, c.name as cabinet_name, p.start_u, p.end_u, p.description, p.device_type, p.device_id, p.created_at::TIMESTAMPTZ, p.updated_at::TIMESTAMPTZ 
        FROM positions p 
        LEFT JOIN cabinets c ON p.cabinet_id = c.id 
        WHERE p.id = $1"
    ).bind(id)
    .fetch_one(pool.get_conn()).await {
        Ok(r) => r,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("查询机位失败: {err}"))));
        }
    };

    let ips: Vec<IpManager> = sqlx::query_as(
        r"SELECT id, workstation_id, position_id, switch_port_id, device_type, network_id, 
           host(ip_address) as ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at, last_mac
           FROM ips WHERE position_id = $1"
    ).bind(id)
    .fetch_all(pool.get_conn()).await.unwrap_or_default();

    let result = CabinetPositionWithDetails {
        id: row.get("id"),
        name: row.get("name"),
        cabinet_id: row.get("cabinet_id"),
        cabinet_name: row
            .get::<Option<String>, _>("cabinet_name"),
        start_u: row.get("start_u"),
        end_u: row.get("end_u"),
        device_type: row.get("device_type"),
        device_id: row.get("device_id"),
        ips,
        description: row.get("description"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };

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

pub async fn delete_cabinet_position(
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
                        "Database query error: {err}"
                    ))),
                );
            }
        };

    if existing_position.is_none() {
        let existing_switch: Option<Uuid> =
            match sqlx::query_scalar::<_, Uuid>("SELECT id FROM switches WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
            {
                Ok(switch) => switch,
                Err(err) => {
                    return Ok(
                        HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                            format!("Database query error: {err}"),
                        )),
                    );
                }
            };

        if existing_switch.is_some() {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                "该机位是交换机占用的位置，请通过交换机管理页面删除对应的交换机",
            )));
        }

        return Ok(
            HttpResponse::NotFound().json(ApiResponse::<CabinetPosition>::error("机位未找到"))
        );
    }

    let has_switch: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM positions p WHERE p.id = $1 AND p.device_type = 'switch')"
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(false);

    if has_switch {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "该机位已关联交换机，请通过删除交换机来删除机位",
        )));
    }

    let device_type: Option<String> = sqlx::query_scalar("SELECT device_type FROM positions WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .ok()
        .flatten();

    if device_type.as_deref() == Some("switch") {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "该机位是交换机占用的位置，请通过交换机管理页面删除对应的交换机",
        )));
    }

    if let Err(err) = sqlx::query("DELETE FROM ips WHERE position_id = $1")
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

    if let Err(err) = sqlx::query("DELETE FROM positions WHERE id = $1")
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
