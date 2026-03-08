use crate::config::Config;
use crate::db::DbPool;
use crate::models::{
    ApiResponse, NetworkInfo, Room, RoomCreate, RoomUpdate, RoomWithNetworks,
};
use crate::utils::{log_system_operation, DEFAULT_PAGE};
use actix_web::{HttpRequest, HttpResponse, Result, web};
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

// 房间相关路由
// 获取所有房间（支持搜索和分页）
pub async fn get_rooms(
    pool: web::Data<DbPool>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    let page: i64 = query.get("page").and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_PAGE);
    let page_size: i64 = query.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(20);
    let search = query.get("search").cloned().unwrap_or_default();
    let sort_by = query.get("sort_by").cloned().unwrap_or_else(|| "name".to_string());
    let sort_order = query.get("sort_order").cloned().unwrap_or_else(|| "asc".to_string());
    let offset = (page - 1) * page_size;

    let search_pattern = format!("%{}%", search);

    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY name DESC",
        ("name", _) => "ORDER BY name ASC",
        ("created_at", "desc") => "ORDER BY created_at DESC",
        ("created_at", _) => "ORDER BY created_at ASC",
        ("room_type", "desc") => "ORDER BY room_type DESC, name ASC",
        ("room_type", _) => "ORDER BY room_type ASC, name ASC",
        _ => "ORDER BY name ASC",
    };

    let (total, rooms) = if search.is_empty() {
        let total: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM rooms")
            .fetch_one(pool.get_conn())
            .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {}", err),
                )));
            }
        };

        let rooms = match sqlx::query_as::<_, Room>(
            &format!("SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms {} LIMIT $1 OFFSET $2", order_clause)
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
        {
            Ok(r) => r,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {}", err),
                )));
            }
        };

        (total, rooms)
    } else {
        let total: i64 = match sqlx::query_scalar(
            "SELECT COUNT(*) FROM rooms WHERE name ILIKE $1 OR room_type ILIKE $1 OR description ILIKE $1"
        )
        .bind(&search_pattern)
        .fetch_one(pool.get_conn())
        .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {}", err),
                )));
            }
        };

        let rooms = match sqlx::query_as::<_, Room>(
            &format!("SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms WHERE name ILIKE $1 OR room_type ILIKE $1 OR description ILIKE $1 {} LIMIT $2 OFFSET $3", order_clause)
        )
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
        {
            Ok(r) => r,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {}", err),
                )));
            }
        };

        (total, rooms)
    };

    let mut rooms_with_networks = Vec::new();

    for room in rooms {
        let room_networks = match sqlx::query_as::<_, NetworkInfo>(
            r#"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr 
               FROM room_networks rn 
               JOIN network_cidrs n ON rn.network_id = n.id 
               JOIN network_regions nr ON n.network_region_id = nr.id 
               WHERE rn.room_id = $1"#,
        )
        .bind(room.id)
        .fetch_all(pool.get_conn())
        .await
        {
            Ok(networks) => networks,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
            }
        };

        let workstation_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM workstations WHERE room_id = $1"
        )
        .bind(room.id)
        .fetch_one(pool.get_conn())
        .await
        .unwrap_or(0);

        let room_with_networks = RoomWithNetworks {
            id: room.id,
            name: room.name,
            room_type: room.room_type,
            description: room.description,
            networks: room_networks,
            workstation_count,
            created_at: room.created_at,
            updated_at: room.updated_at,
        };

        rooms_with_networks.push(room_with_networks);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": rooms_with_networks,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "房间获取成功",
    )))
}

// 创建房间
pub async fn create_room(
    pool: web::Data<DbPool>,
    req: web::Json<RoomCreate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 验证创建房间请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {:?}", e)))
        );
    }

    // 检查房间名称是否已存在
    let existing_room = match sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE name = $1")
        .bind(&req.name)
        .fetch_optional(pool.get_conn())
        .await
    {
        Ok(room) => room,
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "Database query error: {}",
                    err
                ))),
            );
        }
    };

    if existing_room.is_some() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<Room>::error("房间名称已存在")));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    // 创建房间
    if let Err(err) = sqlx::query(
        "INSERT INTO rooms (id, name, room_type, description, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.room_type)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(pool.get_conn())
    .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库插入错误: {}", err))));
    }

    // 关联网络
    for network_id in &req.network_ids {
        if let Err(err) = sqlx::query(
            "INSERT INTO room_networks (id, room_id, network_id, created_at, updated_at) 
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(id)
        .bind(network_id)
        .bind(now)
        .bind(now)
        .execute(pool.get_conn())
        .await
        {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("数据库插入错误: {}", err))));
        }
    }

    // 返回创建的房间
    let room = Room {
        id,
        name: req.name.clone(),
        room_type: req.room_type.clone(),
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    // 记录操作日志
    let details = serde_json::json!({
        "name": room.name,
        "room_type": room.room_type,
        "description": room.description,
        "network_count": req.network_ids.len()
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "create",
        "room",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<Room>::success(room, "房间创建成功")))
}

// 获取单个房间
pub async fn get_room(pool: web::Data<DbPool>, id_path: web::Path<Uuid>) -> Result<HttpResponse> {
    let id = *id_path;

    // 获取房间基本信息
    let room = match sqlx::query_as::<_, Room>(
        "SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms WHERE id = $1"
    ).bind(id)
    .fetch_optional(pool.get_conn()).await {
        Ok(Some(room)) => room,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<Room>::error("房间未找到")));
        },
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
        }
    };

    // 获取房间关联的网络
    let room_networks = match sqlx::query_as::<_, NetworkInfo>(
        r#"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr 
           FROM room_networks rn 
           JOIN network_cidrs n ON rn.network_id = n.id 
           JOIN network_regions nr ON n.network_region_id = nr.id 
           WHERE rn.room_id = $1"#,
    )
    .bind(id)
    .fetch_all(pool.get_conn())
    .await
    {
        Ok(networks) => networks,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
        }
    };

    // 创建带网络信息的房间对象
    let workstation_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM workstations WHERE room_id = $1"
    )
    .bind(room.id)
    .fetch_one(pool.get_conn())
    .await
    .unwrap_or(0);

    let room_with_networks = RoomWithNetworks {
        id: room.id,
        name: room.name,
        room_type: room.room_type,
        description: room.description,
        networks: room_networks,
        workstation_count,
        created_at: room.created_at,
        updated_at: room.updated_at,
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<RoomWithNetworks>::success(
            room_with_networks,
            "房间获取成功",
        )),
    )
}

// 更新房间
pub async fn update_room(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    req: web::Json<RoomUpdate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 验证更新房间请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "Validation error: {:?}",
                e
            ))),
        );
    }

    // 检查房间是否存在
    let existing_room = match sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE id = $1")
        .bind(id)
        .fetch_optional(pool.get_conn())
        .await
    {
        Ok(room) => room,
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "Database query error: {}",
                    err
                ))),
            );
        }
    };

    if existing_room.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<Room>::error("房间未找到")));
    }

    let now = Utc::now();

    // 更新房间基本信息
    if let Err(err) = sqlx::query(
        "UPDATE rooms SET 
         name = COALESCE($1, name), 
         room_type = COALESCE($2, room_type), 
         description = COALESCE($3, description), 
         updated_at = $4 
         WHERE id = $5",
    )
    .bind(&req.name)
    .bind(&req.room_type)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(pool.get_conn())
    .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库更新错误: {}", err))));
    }

    // 如果提供了网络ID列表，则更新房间-网络关联
    if let Some(network_ids) = &req.network_ids {
        // 删除现有网络关联
        if let Err(err) = sqlx::query("DELETE FROM room_networks WHERE room_id = $1")
            .bind(id)
            .execute(pool.get_conn())
            .await
        {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("数据库删除错误: {}", err))));
        }

        // 创建新的网络关联
        for network_id in network_ids {
            if let Err(err) = sqlx::query(
                "INSERT INTO room_networks (id, room_id, network_id, created_at, updated_at) 
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(network_id)
            .bind(now)
            .bind(now)
            .execute(pool.get_conn())
            .await
            {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("数据库插入错误: {}", err))));
            }
        }
    }

    // 返回更新后的房间
    let room = match sqlx::query_as::<_, Room>(
        "SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms WHERE id = $1"
    ).bind(id)
    .fetch_one(pool.get_conn()).await {
        Ok(room) => room,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
        }
    };

    // 记录操作日志
    let details = serde_json::json!({
        "name": room.name,
        "room_type": room.room_type,
        "description": room.description
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "update",
        "room",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<Room>::success(room, "房间更新成功")))
}

// 删除房间
pub async fn delete_room(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 检查房间是否存在
    let existing_room = match sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE id = $1")
        .bind(id)
        .fetch_optional(pool.get_conn())
        .await
    {
        Ok(room) => room,
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "Database query error: {}",
                    err
                ))),
            );
        }
    };

    if existing_room.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<Room>::error("房间未找到")));
    }

    // 检查是否有机柜关联到该房间
    let cabinet_count =
        match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cabinets WHERE room_id = $1")
            .bind(id)
            .fetch_one(pool.get_conn())
            .await
        {
            Ok(count) => count,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {}",
                        err
                    ))),
                );
            }
        };

    if cabinet_count > 0 {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<Room>::error("该房间已被机柜关联，无法删除")));
    }

    // 检查是否有工位关联到该房间
    let workstation_count =
        match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workstations WHERE room_id = $1")
            .bind(id)
            .fetch_one(pool.get_conn())
            .await
        {
            Ok(count) => count,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {}",
                        err
                    ))),
                );
            }
        };

    if workstation_count > 0 {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<Room>::error("该房间已被工位关联，无法删除")));
    }

    // 删除房间
    if let Err(err) = sqlx::query("DELETE FROM rooms WHERE id = $1")
        .bind(id)
        .execute(pool.get_conn())
        .await
    {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "Database deletion error: {}",
                err
            ))),
        );
    }

    // 记录操作日志
    let details = serde_json::json!({
        "room_id": id.to_string()
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "delete",
        "room",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "房间删除成功")))
}

// 获取房间关联的网段
pub async fn get_room_networks(pool: web::Data<DbPool>, id_path: web::Path<Uuid>) -> Result<HttpResponse> {
    let id = *id_path;

    // 检查房间是否存在
    let existing_room = match sqlx::query_scalar::<_, Uuid>("SELECT id FROM rooms WHERE id = $1")
        .bind(id)
        .fetch_optional(pool.get_conn())
        .await
    {
        Ok(room) => room,
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "Database query error: {}",
                    err
                ))),
            );
        }
    };

    if existing_room.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("房间未找到")));
    }

    // 获取房间关联的网络
    let room_networks = match sqlx::query_as::<_, NetworkInfo>(
        r#"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr 
           FROM room_networks rn 
           JOIN network_cidrs n ON rn.network_id = n.id 
           JOIN network_regions nr ON n.network_region_id = nr.id 
           WHERE rn.room_id = $1"#,
    )
    .bind(id)
    .fetch_all(pool.get_conn())
    .await
    {
        Ok(networks) => networks,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
        }
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<NetworkInfo>>::success(
            room_networks,
            "房间网段获取成功",
        )),
    )
}
