use crate::config::Config;
use crate::db::DbPool;
use crate::models::{
    ApiResponse, Cabinet, CabinetCreate, CabinetUpdate, CabinetWithNetworks, NetworkInfo, Room,
    RoomCreate, RoomUpdate, RoomWithNetworks,
};
use crate::utils::log_system_operation;
use actix_web::{HttpRequest, HttpResponse, Result, web};
use chrono::Utc;

use uuid::Uuid;
use validator::Validate;

// 房间相关路由
// 获取所有房间
pub async fn get_rooms(
    pool: web::Data<DbPool>,
    _query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    // 查询所有房间
    let rooms = match sqlx::query_as::<_, Room>(
        "SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms"
    ).fetch_all(pool.get_conn()).await {
        Ok(rooms) => rooms,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
        }
    };

    // 为每个房间获取关联的网络信息
    let mut rooms_with_networks = Vec::new();

    for room in rooms {
        // 获取房间关联的网络
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

        // 创建带网络信息的房间对象
        let room_with_networks = RoomWithNetworks {
            id: room.id,
            name: room.name,
            room_type: room.room_type,
            description: room.description,
            networks: room_networks,
            created_at: room.created_at,
            updated_at: room.updated_at,
        };

        rooms_with_networks.push(room_with_networks);
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<RoomWithNetworks>>::success(
            rooms_with_networks,
            "房间获取成功",
        )),
    )
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
    let room_with_networks = RoomWithNetworks {
        id: room.id,
        name: room.name,
        room_type: room.room_type,
        description: room.description,
        networks: room_networks,
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

// 机柜相关路由
// 获取所有机柜
pub async fn get_cabinets(
    pool: web::Data<DbPool>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    // 1. 获取查询参数
    let room_id = query.get("room_id");

    // 2. 根据是否有room_id参数构建不同的SQL查询
    let cabinets = if let Some(room_id_str) = room_id {
        // 有room_id参数，解析为UUID类型
        match Uuid::parse_str(room_id_str) {
            Ok(room_id_uuid) => {
                // 查询该房间的机柜
                match sqlx::query_as::<_, Cabinet>(
                    "SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets WHERE room_id = $1"
                ).bind(room_id_uuid)
                .fetch_all(pool.get_conn()).await {
                    Ok(cabinets) => cabinets,
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                }
            }
            Err(_) => {
                // 无效的UUID格式
                return Ok(
                    HttpResponse::BadRequest().json(ApiResponse::<()>::error("无效的room_id格式"))
                );
            }
        }
    } else {
        // 没有room_id参数，查询所有机柜
        match sqlx::query_as::<_, Cabinet>(
            "SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets"
        ).fetch_all(pool.get_conn()).await {
            Ok(cabinets) => cabinets,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
            }
        }
    };

    // 为每个机柜获取关联的网络信息
    let mut cabinets_with_networks = Vec::new();

    for cabinet in cabinets {
        // 获取机柜关联的网络
        let cabinet_networks = match sqlx::query_as::<_, NetworkInfo>(
            r#"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr 
               FROM cabinet_networks cn 
               JOIN network_cidrs n ON cn.network_id = n.id 
               JOIN network_regions nr ON n.network_region_id = nr.id 
               WHERE cn.cabinet_id = $1"#,
        )
        .bind(cabinet.id)
        .fetch_all(pool.get_conn())
        .await
        {
            Ok(networks) => networks,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
            }
        };

        // 创建带网络信息的机柜对象
        let cabinet_with_networks = CabinetWithNetworks {
            id: cabinet.id,
            name: cabinet.name,
            room_id: cabinet.room_id,
            capacity: cabinet.capacity,
            networks: cabinet_networks,
            description: cabinet.description,
            created_at: cabinet.created_at,
            updated_at: cabinet.updated_at,
        };

        cabinets_with_networks.push(cabinet_with_networks);
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<CabinetWithNetworks>>::success(
            cabinets_with_networks,
            "机柜获取成功",
        )),
    )
}

// 创建机柜
pub async fn create_cabinet(
    pool: web::Data<DbPool>,
    req: web::Json<CabinetCreate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 验证创建机柜请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {:?}", e)))
        );
    }

    // 检查机柜名称是否已存在
    let existing_cabinet = match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM cabinets WHERE name = $1 AND room_id = $2",
    )
    .bind(&req.name)
    .bind(req.room_id)
    .fetch_optional(pool.get_conn())
    .await
    {
        Ok(cabinet) => cabinet,
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "Database query error: {}",
                    err
                ))),
            );
        }
    };

    if existing_cabinet.is_some() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<Cabinet>::error("机柜名称已存在")));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    // 创建机柜
    if let Err(err) = sqlx::query(
        "INSERT INTO cabinets (id, name, room_id, capacity, description, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(req.room_id)
    .bind(req.capacity)
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
            "INSERT INTO cabinet_networks (id, cabinet_id, network_id, created_at, updated_at) 
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

    // 返回创建的机柜
    let cabinet = Cabinet {
        id,
        name: req.name.clone(),
        room_id: req.room_id,
        capacity: req.capacity,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    // 记录操作日志
    let details = serde_json::json!({
        "name": cabinet.name,
        "room_id": cabinet.room_id,
        "capacity": cabinet.capacity,
        "description": cabinet.description,
        "network_count": req.network_ids.len()
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "create",
        "cabinet",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<Cabinet>::success(cabinet, "机柜创建成功")))
}

// 获取单个机柜
pub async fn get_cabinet(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 获取机柜基本信息
    let cabinet = match sqlx::query_as::<_, Cabinet>(
        "SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets WHERE id = $1"
    ).bind(id)
    .fetch_optional(pool.get_conn()).await {
        Ok(Some(cabinet)) => cabinet,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<Cabinet>::error("机柜未找到")));
        },
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
        }
    };

    // 获取机柜关联的网络
    let cabinet_networks = match sqlx::query_as::<_, NetworkInfo>(
        r#"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr 
           FROM cabinet_networks cn 
           JOIN network_cidrs n ON cn.network_id = n.id 
           JOIN network_regions nr ON n.network_region_id = nr.id 
           WHERE cn.cabinet_id = $1"#,
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

    // 创建带网络信息的机柜对象
    let cabinet_with_networks = CabinetWithNetworks {
        id: cabinet.id,
        name: cabinet.name,
        room_id: cabinet.room_id,
        capacity: cabinet.capacity,
        networks: cabinet_networks,
        description: cabinet.description,
        created_at: cabinet.created_at,
        updated_at: cabinet.updated_at,
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<CabinetWithNetworks>::success(
            cabinet_with_networks,
            "机柜获取成功",
        )),
    )
}

// 更新机柜
pub async fn update_cabinet(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    req: web::Json<CabinetUpdate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 验证更新机柜请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "Validation error: {:?}",
                e
            ))),
        );
    }

    // 检查机柜是否存在
    let existing_cabinet =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
            .bind(id)
            .fetch_optional(pool.get_conn())
            .await
        {
            Ok(cabinet) => cabinet,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {}",
                        err
                    ))),
                );
            }
        };

    if existing_cabinet.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<Cabinet>::error("机柜未找到")));
    }

    let now = Utc::now();

    // 更新机柜基本信息
    if let Err(err) = sqlx::query(
        "UPDATE cabinets SET 
         name = COALESCE($1, name), 
         room_id = COALESCE($2, room_id), 
         capacity = COALESCE($3, capacity), 
         description = COALESCE($4, description), 
         updated_at = $5 
         WHERE id = $6",
    )
    .bind(&req.name)
    .bind(req.room_id)
    .bind(req.capacity)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(pool.get_conn())
    .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库更新错误: {}", err))));
    }

    // 如果提供了网络ID列表，则更新机柜-网络关联
    if let Some(network_ids) = &req.network_ids {
        // 删除现有网络关联
        if let Err(err) = sqlx::query("DELETE FROM cabinet_networks WHERE cabinet_id = $1")
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
                "INSERT INTO cabinet_networks (id, cabinet_id, network_id, created_at, updated_at) 
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

    // 返回更新后的机柜
    let cabinet = match sqlx::query_as::<_, Cabinet>(
        "SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets WHERE id = $1"
    ).bind(id)
    .fetch_one(pool.get_conn()).await {
        Ok(cabinet) => cabinet,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
        }
    };

    // 记录操作日志
    let details = serde_json::json!({
        "name": cabinet.name,
        "room_id": cabinet.room_id,
        "capacity": cabinet.capacity,
        "description": cabinet.description
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "update",
        "cabinet",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<Cabinet>::success(cabinet, "机柜更新成功")))
}

// 删除机柜
pub async fn delete_cabinet(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 检查机柜是否存在
    let existing_cabinet =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
            .bind(id)
            .fetch_optional(pool.get_conn())
            .await
        {
            Ok(cabinet) => cabinet,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {}",
                        err
                    ))),
                );
            }
        };

    if existing_cabinet.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<Cabinet>::error("机柜未找到")));
    }

    // 检查是否有机位关联到该机柜
    let position_count =
        match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM positions WHERE cabinet_id = $1")
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

    if position_count > 0 {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<Cabinet>::error(
                "该机柜已被机位关联，无法删除",
            )),
        );
    }

    // 删除机柜
    if let Err(err) = sqlx::query("DELETE FROM cabinets WHERE id = $1")
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
        "cabinet_id": id.to_string()
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "delete",
        "cabinet",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "机柜删除成功")))
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

// 获取机柜关联的网段
pub async fn get_cabinet_networks(pool: web::Data<DbPool>, id_path: web::Path<Uuid>) -> Result<HttpResponse> {
    let id = *id_path;

    // 检查机柜是否存在
    let existing_cabinet = match sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
        .bind(id)
        .fetch_optional(pool.get_conn())
        .await
    {
        Ok(cabinet) => cabinet,
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "Database query error: {}",
                    err
                ))),
            );
        }
    };

    if existing_cabinet.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("机柜未找到")));
    }

    // 获取机柜关联的网络
    let cabinet_networks = match sqlx::query_as::<_, NetworkInfo>(
        r#"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr 
           FROM cabinet_networks cn 
           JOIN network_cidrs n ON cn.network_id = n.id 
           JOIN network_regions nr ON n.network_region_id = nr.id 
           WHERE cn.cabinet_id = $1"#,
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
            cabinet_networks,
            "机柜网段获取成功",
        )),
    )
}
