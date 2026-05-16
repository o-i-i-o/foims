use crate::config::Config;
use crate::db::DbPool;
use crate::models::{ApiResponse, LayoutSaveRequest};
use crate::utils::log_system_operation;
use actix_web::{HttpRequest, HttpResponse, Result, web};
use serde_json;
use uuid::Uuid;

pub fn generate_region_map(
    _pool: web::Data<DbPool>,
    _id: web::Path<uuid::Uuid>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
        (),
        "Region map generation not implemented yet",
    )))
}

pub fn generate_room_map(
    _pool: web::Data<DbPool>,
    _id: web::Path<uuid::Uuid>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
        (),
        "Room map generation not implemented yet",
    )))
}

pub fn generate_workstation_map(
    _pool: web::Data<DbPool>,
    _id: web::Path<uuid::Uuid>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
        (),
        "Workstation map generation not implemented yet",
    )))
}

pub async fn save_layout(
    pool: web::Data<DbPool>,
    req: web::Json<LayoutSaveRequest>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    if req.r#type == "workstation" {
        let Some(room_id) = req.room_id else {
            return Ok(
                HttpResponse::BadRequest().json(ApiResponse::<()>::error("房间ID不能为空"))
            )
        };

        let mut tx = pool.get_conn().begin().await.map_err(|e| {
            actix_web::error::InternalError::new(
                format!("获取数据库连接失败: {e}"),
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            )
        })?;

        for item in &req.layout {
            if let Err(e) = sqlx::query(
                "INSERT INTO workstation_layouts (workstation_id, x, y, width, height, rotation) 
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (workstation_id) 
                 DO UPDATE SET 
                     x = EXCLUDED.x,
                     y = EXCLUDED.y,
                     width = EXCLUDED.width,
                     height = EXCLUDED.height,
                     rotation = EXCLUDED.rotation,
                     updated_at = NOW()"
            )
            .bind(item.id)
            .bind(item.position.x_i32())
            .bind(item.position.y_i32())
            .bind(item.position.width_i32())
            .bind(item.position.height_i32())
            .bind(item.position.rotation_i32())
            .execute(&mut *tx).await {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("保存布局失败: {e}"))));
            }
        }

        if let Err(e) = tx.commit().await {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("提交事务失败: {e}"))));
        }

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
    } else if req.r#type == "cabinet" {
        let mut tx = pool.get_conn().begin().await.map_err(|e| {
            actix_web::error::InternalError::new(
                format!("获取数据库连接失败: {e}"),
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            )
        })?;

        for item in &req.layout {
            if let Err(e) = sqlx::query(
                "INSERT INTO cabinet_layouts (cabinet_id, x, y, width, height, rotation) 
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (cabinet_id) 
                 DO UPDATE SET 
                     x = EXCLUDED.x,
                     y = EXCLUDED.y,
                     width = EXCLUDED.width,
                     height = EXCLUDED.height,
                     rotation = EXCLUDED.rotation,
                     updated_at = NOW()"
            )
            .bind(item.id)
            .bind(item.position.x_i32())
            .bind(item.position.y_i32())
            .bind(item.position.width_i32())
            .bind(item.position.height_i32())
            .bind(item.position.rotation_i32())
            .execute(&mut *tx).await {
                return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("保存布局失败: {e}"))));
            }
        }

        if let Err(e) = tx.commit().await {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("提交事务失败: {e}"))));
        }

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
            &Uuid::nil(),
            &details,
            true,
        )
        .await;

        Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "机柜布局保存成功")))
    } else {
        Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("不支持的布局类型")))
    }
}

pub async fn delete_layout(
    pool: web::Data<DbPool>,
    room_id: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let room_id = *room_id;

    if let Err(e) =
        sqlx::query("DELETE FROM workstation_layouts WHERE workstation_id IN (SELECT id FROM workstations WHERE room_id = $1)")
            .bind(room_id)
            .execute(pool.get_conn())
            .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("删除布局失败: {e}"))));
    }

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

pub async fn delete_positions_layout(
    pool: web::Data<DbPool>,
    network_region_id: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let network_region_id = *network_region_id;

    let _ = pool;
    let _ = network_region_id;

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

pub async fn get_layout(pool: web::Data<DbPool>, room_id: web::Path<Uuid>) -> Result<HttpResponse> {
    let room_id = *room_id;

    let layouts = sqlx::query_as::<_, (Uuid, serde_json::Value)>(
        r"SELECT wl.workstation_id, 
                  json_build_object(
                      'x', wl.x, 
                      'y', wl.y, 
                      'width', wl.width, 
                      'height', wl.height, 
                      'rotation', wl.rotation
                  ) as position
           FROM workstation_layouts wl
           JOIN workstations w ON wl.workstation_id = w.id
           WHERE w.room_id = $1",
    )
    .bind(room_id)
    .fetch_all(pool.get_conn())
    .await
    .map_err(|e| {
        actix_web::error::InternalError::new(
            format!("获取布局失败: {e}"),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        )
    })?;

    let layout_data: Vec<serde_json::Value> = layouts
        .into_iter()
        .map(|(id, position)| {
            let mut obj = serde_json::Map::new();
            obj.insert("id".to_string(), serde_json::Value::String(id.to_string()));
            obj.insert(
                "element_type".to_string(),
                serde_json::Value::String("workstation".to_string()),
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

pub async fn get_positions_layout(
    pool: web::Data<DbPool>,
    network_region_id: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let network_region_id = *network_region_id;

    let _ = pool;
    let _ = network_region_id;

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<serde_json::Value>>::success(
            vec![],
            "布局获取成功",
        )),
    )
}
