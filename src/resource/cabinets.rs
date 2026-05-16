use crate::config::Config;
use crate::db::DbPool;
use crate::models::{
    ApiResponse, Cabinet, CabinetCreate, CabinetUpdate, CabinetWithNetworks, NetworkInfo,
};
use crate::utils::{DEFAULT_PAGE, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, Result, web};
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

pub async fn get_cabinets(
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
        ("name", "desc") => "ORDER BY c.name DESC",
        ("capacity", "desc") => "ORDER BY c.capacity DESC, c.name ASC",
        ("capacity", _) => "ORDER BY c.capacity ASC, c.name ASC",
        ("created_at", "desc") => "ORDER BY c.created_at DESC",
        ("created_at", _) => "ORDER BY c.created_at ASC",
        _ => "ORDER BY c.name ASC",
    };

    let (total, cabinets) = if search.is_empty() && parsed_room_id.is_none() {
        let total: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM cabinets c")
            .fetch_one(&pool.get_conn())
            .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("数据库查询错误: {err}"))));
            }
        };

        let cabinets = match sqlx::query_as::<_, Cabinet>(
            &format!("SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets c {order_clause} LIMIT $1 OFFSET $2")
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(&pool.get_conn())
        .await
        {
            Ok(c) => c,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {err}"),
                )));
            }
        };

        (total, cabinets)
    } else if parsed_room_id.is_some() && search.is_empty() {
        let total: i64 =
            match sqlx::query_scalar("SELECT COUNT(*) FROM cabinets c WHERE c.room_id = $1")
                .bind(parsed_room_id)
                .fetch_one(&pool.get_conn())
                .await
            {
                Ok(t) => t,
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error(format!("数据库查询错误: {err}"))));
                }
            };

        let cabinets = match sqlx::query_as::<_, Cabinet>(
            &format!("SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets c WHERE c.room_id = $1 {order_clause} LIMIT $2 OFFSET $3")
        )
        .bind(parsed_room_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&pool.get_conn())
        .await
        {
            Ok(c) => c,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {err}"),
                )));
            }
        };

        (total, cabinets)
    } else if parsed_room_id.is_some() {
        let total: i64 = match sqlx::query_scalar(
            "SELECT COUNT(*) FROM cabinets c WHERE c.room_id = $1 AND (c.name ILIKE $2 OR c.description ILIKE $2)"
        )
        .bind(parsed_room_id)
        .bind(&search_pattern)
        .fetch_one(&pool.get_conn())
        .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {err}"),
                )));
            }
        };

        let cabinets = match sqlx::query_as::<_, Cabinet>(
            &format!("SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets c WHERE c.room_id = $1 AND (c.name ILIKE $2 OR c.description ILIKE $2) {order_clause} LIMIT $3 OFFSET $4")
        )
        .bind(parsed_room_id)
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&pool.get_conn())
        .await
        {
            Ok(c) => c,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {err}"),
                )));
            }
        };

        (total, cabinets)
    } else {
        let total: i64 = match sqlx::query_scalar(
            "SELECT COUNT(*) FROM cabinets c WHERE c.name ILIKE $1 OR c.description ILIKE $1",
        )
        .bind(&search_pattern)
        .fetch_one(&pool.get_conn())
        .await
        {
            Ok(t) => t,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("数据库查询错误: {err}"))));
            }
        };

        let cabinets = match sqlx::query_as::<_, Cabinet>(
            &format!("SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets c WHERE c.name ILIKE $1 OR c.description ILIKE $1 {order_clause} LIMIT $2 OFFSET $3")
        )
        .bind(&search_pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(&pool.get_conn())
        .await
        {
            Ok(c) => c,
            Err(err) => {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    format!("数据库查询错误: {err}"),
                )));
            }
        };

        (total, cabinets)
    };

    let mut cabinets_with_networks = Vec::new();

    for cabinet in cabinets {
        let position_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM positions WHERE cabinet_id = $1")
                .bind(cabinet.id)
                .fetch_one(&pool.get_conn())
                .await
                .unwrap_or(0);

        let room_name: Option<String> = sqlx::query_scalar("SELECT name FROM rooms WHERE id = $1")
            .bind(cabinet.room_id)
            .fetch_optional(&pool.get_conn())
            .await
            .unwrap_or(None);

        let cabinet_with_networks = CabinetWithNetworks {
            id: cabinet.id,
            name: cabinet.name,
            room_id: cabinet.room_id,
            room_name,
            capacity: cabinet.capacity,
            position_count,
            description: cabinet.description,
            created_at: cabinet.created_at,
            updated_at: cabinet.updated_at,
        };

        cabinets_with_networks.push(cabinet_with_networks);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({
            "items": cabinets_with_networks,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "机柜获取成功",
    )))
}

pub async fn get_cabinets_by_network_region(
    pool: web::Data<DbPool>,
    path: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    let region_id_str = path.into_inner();

    let Ok(region_id) = Uuid::parse_str(&region_id_str) else {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error("无效的网络区域ID"))
        )
    };

    let network_id_filter = query
        .get("network_id")
        .and_then(|s| Uuid::parse_str(s).ok());

    let cabinets = if let Some(network_id) = network_id_filter {
        sqlx::query_as::<_, Cabinet>(
            r"SELECT DISTINCT c.id, c.name, c.room_id, c.capacity, c.description, c.created_at, c.updated_at
               FROM cabinets c
               LEFT JOIN rooms r ON c.room_id = r.id
               LEFT JOIN room_networks rn ON r.id = rn.room_id
               WHERE rn.network_id = $1
               ORDER BY c.name"
        )
        .bind(network_id)
        .fetch_all(&pool.get_conn())
        .await
    } else {
        sqlx::query_as::<_, Cabinet>(
            r"SELECT DISTINCT c.id, c.name, c.room_id, c.capacity, c.description, c.created_at, c.updated_at
               FROM cabinets c
               LEFT JOIN rooms r ON c.room_id = r.id
               LEFT JOIN room_networks rn ON r.id = rn.room_id
               LEFT JOIN network_cidrs nc ON rn.network_id = nc.id
               WHERE nc.network_region_id = $1
               ORDER BY c.name"
        )
        .bind(region_id)
        .fetch_all(&pool.get_conn())
        .await
    };

    let cabinets = match cabinets {
        Ok(c) => c,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("数据库查询错误: {err}"))));
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(cabinets, "机柜获取成功")))
}

pub async fn create_cabinet(
    pool: web::Data<DbPool>,
    req: web::Json<CabinetCreate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {e:?}")))
        );
    }

    let existing_cabinet = match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM cabinets WHERE name = $1 AND room_id = $2",
    )
    .bind(&req.name)
    .bind(req.room_id)
    .fetch_optional(&pool.get_conn())
    .await
    {
        Ok(cabinet) => cabinet,
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "Database query error: {err}"
                ))),
            );
        }
    };

    if existing_cabinet.is_some() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<Cabinet>::error("机柜名称已存在")));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

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
    .execute(&pool.get_conn())
    .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库插入错误: {err}"))));
    }

    let cabinet = Cabinet {
        id,
        name: req.name.clone(),
        room_id: req.room_id,
        capacity: req.capacity,
        description: req.description.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": cabinet.name,
        "room_id": cabinet.room_id,
        "capacity": cabinet.capacity,
        "description": cabinet.description
    });
    let _ = log_system_operation(
        &pool.get_conn(),
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

pub async fn get_cabinet(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = *id_path;

    let cabinet = match sqlx::query_as::<_, Cabinet>(
        "SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets WHERE id = $1"
    ).bind(id)
    .fetch_optional(&pool.get_conn()).await {
        Ok(Some(cabinet)) => cabinet,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<Cabinet>::error("机柜未找到")));
        },
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {err}"))));
        }
    };

    let position_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM positions WHERE cabinet_id = $1")
            .bind(cabinet.id)
            .fetch_one(&pool.get_conn())
            .await
            .unwrap_or(0);

    let room_name: Option<String> = sqlx::query_scalar("SELECT name FROM rooms WHERE id = $1")
        .bind(cabinet.room_id)
        .fetch_optional(&pool.get_conn())
        .await
        .unwrap_or(None);

    let cabinet_with_networks = CabinetWithNetworks {
        id: cabinet.id,
        name: cabinet.name,
        room_id: cabinet.room_id,
        room_name,
        capacity: cabinet.capacity,
        position_count,
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

pub async fn update_cabinet(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    req: web::Json<CabinetUpdate>,
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

    let existing_cabinet =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool.get_conn())
            .await
        {
            Ok(cabinet) => cabinet,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {err}"
                    ))),
                );
            }
        };

    if existing_cabinet.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<Cabinet>::error("机柜未找到")));
    }

    let now = Utc::now();

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
    .execute(&pool.get_conn())
    .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库更新错误: {err}"))));
    }

    let cabinet = match sqlx::query_as::<_, Cabinet>(
        "SELECT id, name, room_id, capacity, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM cabinets WHERE id = $1"
    ).bind(id)
    .fetch_one(&pool.get_conn()).await {
        Ok(cabinet) => cabinet,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {err}"))));
        }
    };

    let details = serde_json::json!({
        "name": cabinet.name,
        "room_id": cabinet.room_id,
        "capacity": cabinet.capacity,
        "description": cabinet.description
    });
    let _ = log_system_operation(
        &pool.get_conn(),
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

pub async fn delete_cabinet(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = *id_path;

    let existing_cabinet =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool.get_conn())
            .await
        {
            Ok(cabinet) => cabinet,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {err}"
                    ))),
                );
            }
        };

    if existing_cabinet.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<Cabinet>::error("机柜未找到")));
    }

    let position_count =
        match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM positions WHERE cabinet_id = $1")
            .bind(id)
            .fetch_one(&pool.get_conn())
            .await
        {
            Ok(count) => count,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {err}"
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

    if let Err(err) = sqlx::query("DELETE FROM cabinets WHERE id = $1")
        .bind(id)
        .execute(&pool.get_conn())
        .await
    {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "Database deletion error: {err}"
            ))),
        );
    }

    let details = serde_json::json!({
        "cabinet_id": id.to_string()
    });
    let _ = log_system_operation(
        &pool.get_conn(),
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

pub async fn get_cabinet_networks(
    pool: web::Data<DbPool>,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = *id_path;

    let existing_cabinet =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM cabinets WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool.get_conn())
            .await
        {
            Ok(cabinet) => cabinet,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {err}"
                    ))),
                );
            }
        };

    if existing_cabinet.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("机柜未找到")));
    }

    let cabinet_networks = match sqlx::query_as::<_, NetworkInfo>(
        r"SELECT n.id, n.name, nr.name as network_region, n.network_region_id, n.ipv4_cidr::text as ipv4_cidr, n.ipv6_cidr::text as ipv6_cidr 
           FROM rooms r 
           JOIN room_networks rn ON r.id = rn.room_id
           JOIN network_cidrs n ON rn.network_id = n.id 
           JOIN network_regions nr ON n.network_region_id = nr.id 
           WHERE r.id = (SELECT room_id FROM cabinets WHERE id = $1) 
           AND r.room_type = 'DATA_CENTER'",
    )
    .bind(id)
    .fetch_all(&pool.get_conn())
    .await
    {
        Ok(networks) => networks,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("数据库查询错误: {err}"))));
        }
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<NetworkInfo>>::success(
            cabinet_networks,
            "机柜网段获取成功",
        )),
    )
}
