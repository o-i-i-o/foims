use crate::config::Config;
use crate::db::DbPool;
use crate::models::{ApiResponse, LayoutSaveRequest};
use crate::utils::log_system_operation;
use actix_web::{HttpRequest, HttpResponse, Result, web};
use serde_json;
use uuid::Uuid;

// 生成区域图
pub async fn generate_region_map(
    _pool: web::Data<DbPool>,
    _id: web::Path<uuid::Uuid>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
        (),
        "Region map generation not implemented yet",
    )))
}

// 生成房间图
pub async fn generate_room_map(
    _pool: web::Data<DbPool>,
    _id: web::Path<uuid::Uuid>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
        (),
        "Room map generation not implemented yet",
    )))
}

// 生成工位图
pub async fn generate_workstation_map(
    _pool: web::Data<DbPool>,
    _id: web::Path<uuid::Uuid>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
        (),
        "Workstation map generation not implemented yet",
    )))
}

// 保存布局
pub async fn save_layout(
    pool: web::Data<DbPool>,
    req: web::Json<LayoutSaveRequest>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    if req.r#type == "workstation" {
        // 检查 room_id 是否存在
        let room_id = match req.room_id {
            Some(room_id) => room_id,
            None => {
                return Ok(
                    HttpResponse::BadRequest().json(ApiResponse::<()>::error("房间ID不能为空"))
                );
            }
        };

        // 开始事务
        let mut tx = pool.get_conn().begin().await.map_err(|e| {
            actix_web::error::InternalError::new(
                format!("获取数据库连接失败: {}", e),
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            )
        })?;

        // 保存新布局 - 使用UPSERT（更新或插入）
        for item in &req.layout {
            // 使用前端发送的元素类型
            let element_type = &item.element_type;

            // 使用ON CONFLICT实现UPSERT
            if let Err(e) = sqlx::query(
                "INSERT INTO svg_layouts (layout_type, room_id, network_region_id, element_id, element_type, x, y, width, height, rotation) 
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT (layout_type, room_id, network_region_id, element_id) 
                 DO UPDATE SET 
                     element_type = EXCLUDED.element_type,
                     x = EXCLUDED.x,
                     y = EXCLUDED.y,
                     width = EXCLUDED.width,
                     height = EXCLUDED.height,
                     rotation = EXCLUDED.rotation,
                     updated_at = NOW()"
            )
            .bind("workstation")
            .bind(room_id)
            .bind::<Option<Uuid>>(None) // 工位布局时network_region_id为NULL
            .bind(item.id)
            .bind(element_type)
            .bind(item.position.x as i32)
            .bind(item.position.y as i32)
            .bind(item.position.width as i32)
            .bind(item.position.height as i32)
            .bind(item.position.rotation as i32)
            .execute(&mut *tx).await {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("保存布局失败: {}", e))));
            }
        }

        // 提交事务
        if let Err(e) = tx.commit().await {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("提交事务失败: {}", e))));
        }

        // 记录操作日志
        let details = serde_json::json!({
            "room_id": room_id,
            "layout_count": req.layout.len(),
            "type": req.r#type
        });
        let _ = log_system_operation(
            pool.get_conn(),
            &http_req,
            config.get_ref(),
            "update",
            "layout",
            &room_id,
            &details,
            true,
        )
        .await;

        Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "工位布局保存成功")))
    } else if req.r#type == "network_region" {
        // 检查 network_region_id 是否存在
        let network_region_id = match req.network_region_id {
            Some(network_region_id) => network_region_id,
            None => {
                return Ok(
                    HttpResponse::BadRequest().json(ApiResponse::<()>::error("网络区域ID不能为空"))
                );
            }
        };

        // 开始事务
        let mut tx = pool.get_conn().begin().await.map_err(|e| {
            actix_web::error::InternalError::new(
                format!("获取数据库连接失败: {}", e),
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            )
        })?;

        // 保存新布局 - 使用UPSERT（更新或插入）
        for item in &req.layout {
            // 使用前端发送的元素类型
            let element_type = &item.element_type;

            // 使用ON CONFLICT实现UPSERT
            if let Err(e) = sqlx::query(
                "INSERT INTO svg_layouts (layout_type, room_id, network_region_id, element_id, element_type, x, y, width, height, rotation) 
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT (layout_type, room_id, network_region_id, element_id) 
                 DO UPDATE SET 
                     element_type = EXCLUDED.element_type,
                     x = EXCLUDED.x,
                     y = EXCLUDED.y,
                     width = EXCLUDED.width,
                     height = EXCLUDED.height,
                     rotation = EXCLUDED.rotation,
                     updated_at = NOW()"
            )
            .bind("network_region")
            .bind::<Option<Uuid>>(None) // 网络区域布局时room_id为NULL
            .bind(network_region_id)
            .bind(item.id)
            .bind(element_type)
            .bind(item.position.x as i32)
            .bind(item.position.y as i32)
            .bind(item.position.width as i32)
            .bind(item.position.height as i32)
            .bind(item.position.rotation as i32)
            .execute(&mut *tx).await {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("保存布局失败: {}", e))));
            }
        }

        // 提交事务
        if let Err(e) = tx.commit().await {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("提交事务失败: {}", e))));
        }

        // 记录操作日志
        let details = serde_json::json!({
            "network_region_id": network_region_id,
            "layout_count": req.layout.len(),
            "type": req.r#type
        });
        let _ = log_system_operation(
            pool.get_conn(),
            &http_req,
            config.get_ref(),
            "update",
            "layout",
            &network_region_id,
            &details,
            true,
        )
        .await;

        Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "网络区域布局保存成功")))
    } else if req.r#type == "cabinet" {
        // 机柜布局保存
        for item in &req.layout {
            if let Err(e) = sqlx::query(
                "UPDATE cabinets SET x = $1, y = $2, width = $3, height = $4, rotation = $5 WHERE id = $6"
            )
            .bind(item.position.x as i32)
            .bind(item.position.y as i32)
            .bind(item.position.width as i32)
            .bind(item.position.height as i32)
            .bind(item.position.rotation as i32)
            .bind(item.id)
            .execute(pool.get_conn()).await {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("布局保存失败: {}", e))));
            }
        }

        // 记录操作日志
        let details = serde_json::json!({
            "layout_count": req.layout.len(),
            "type": req.r#type
        });
        let _ = log_system_operation(
            pool.get_conn(),
            &http_req,
            config.get_ref(),
            "update",
            "layout",
            &Uuid::nil(), // 机柜布局没有特定的room_id，使用nil UUID
            &details,
            true,
        )
        .await;

        Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "机柜布局保存成功")))
    } else {
        Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("不支持的布局类型")))
    }
}

// 删除布局
pub async fn delete_layout(
    pool: web::Data<DbPool>,
    room_id: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let room_id = *room_id;

    // 从数据库删除布局数据
    if let Err(e) =
        sqlx::query("DELETE FROM svg_layouts WHERE layout_type = 'workstation' AND room_id = $1")
            .bind(room_id)
            .execute(pool.get_conn())
            .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("删除布局失败: {}", e))));
    }

    // 记录操作日志
    let details = serde_json::json!({
        "room_id": room_id
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "delete",
        "layout",
        &room_id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "布局删除成功")))
}

// 删除网络区域布局
pub async fn delete_positions_layout(
    pool: web::Data<DbPool>,
    network_region_id: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let network_region_id = *network_region_id;

    // 从数据库删除布局数据
    if let Err(e) = sqlx::query(
        "DELETE FROM svg_layouts WHERE layout_type = 'network_region' AND network_region_id = $1",
    )
    .bind(network_region_id)
    .execute(pool.get_conn())
    .await
    {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "删除网络区域布局失败: {}",
                e
            ))),
        );
    }

    // 记录操作日志
    let details = serde_json::json!({
        "network_region_id": network_region_id
    });
    let _ = log_system_operation(
        pool.get_conn(),
        &http_req,
        config.get_ref(),
        "delete",
        "layout",
        &network_region_id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "网络区域布局删除成功")))
}

// 获取布局
pub async fn get_layout(pool: web::Data<DbPool>, room_id: web::Path<Uuid>) -> Result<HttpResponse> {
    let room_id = *room_id;

    // 从数据库获取布局数据
    let layouts = sqlx::query_as::<_, (Uuid, String, serde_json::Value)>(
        r#"SELECT element_id, element_type, 
                  json_build_object(
                      'x', x, 
                      'y', y, 
                      'width', width, 
                      'height', height, 
                      'rotation', rotation
                  ) as position
           FROM svg_layouts 
           WHERE layout_type = 'workstation' AND room_id = $1"#,
    )
    .bind(room_id)
    .fetch_all(pool.get_conn())
    .await
    .map_err(|e| {
        actix_web::error::InternalError::new(
            format!("获取布局失败: {}", e),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        )
    })?;

    // 转换为前端需要的格式
    let layout_data: Vec<serde_json::Value> = layouts
        .into_iter()
        .map(|(id, element_type, position)| {
            let mut obj = serde_json::Map::new();
            obj.insert("id".to_string(), serde_json::Value::String(id.to_string()));
            obj.insert(
                "element_type".to_string(),
                serde_json::Value::String(element_type),
            );
            obj.insert("position".to_string(), position);
            serde_json::Value::Object(obj)
        })
        .collect();

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<serde_json::Value>>::success(
            layout_data,
            "布局获取成功",
        )),
    )
}

// 获取网络区域布局
pub async fn get_positions_layout(
    pool: web::Data<DbPool>,
    network_region_id: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let network_region_id = *network_region_id;

    // 从数据库获取布局数据
    let layouts = sqlx::query_as::<_, (Uuid, String, serde_json::Value)>(
        r#"SELECT element_id, element_type, 
                  json_build_object(
                      'x', x, 
                      'y', y, 
                      'width', width, 
                      'height', height, 
                      'rotation', rotation
                  ) as position
           FROM svg_layouts 
           WHERE layout_type = 'position' AND network_region_id = $1"#,
    )
    .bind(network_region_id)
    .fetch_all(pool.get_conn())
    .await
    .map_err(|e| {
        actix_web::error::InternalError::new(
            format!("获取布局失败: {}", e),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        )
    })?;

    // 转换为前端需要的格式
    let layout_data: Vec<serde_json::Value> = layouts
        .into_iter()
        .map(|(id, element_type, position)| {
            let mut obj = serde_json::Map::new();
            obj.insert("id".to_string(), serde_json::Value::String(id.to_string()));
            obj.insert(
                "element_type".to_string(),
                serde_json::Value::String(element_type),
            );
            obj.insert("position".to_string(), position);
            serde_json::Value::Object(obj)
        })
        .collect();

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<serde_json::Value>>::success(
            layout_data,
            "布局获取成功",
        )),
    )
}
