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
    room_id: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let room_id = *room_id;

    let result = sqlx::query(
        "DELETE FROM cabinet_layouts WHERE cabinet_id IN (SELECT id FROM cabinets WHERE room_id = $1)"
    )
    .bind(room_id)
    .execute(pool.get_conn())
    .await;

    match result {
        Ok(_) => {
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

            Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "机柜布局删除成功")))
        }
        Err(err) => Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("删除机柜布局失败: {err}")))),
    }
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
    room_id: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let room_id = *room_id;

    let layouts = sqlx::query_as::<_, (Uuid, Uuid, i32, i32, i32, i32, i32)>(
        r"SELECT cl.id, cl.cabinet_id, cl.x, cl.y, cl.width, cl.height, cl.rotation
          FROM cabinet_layouts cl
          JOIN cabinets c ON cl.cabinet_id = c.id
          WHERE c.room_id = $1",
    )
    .bind(room_id)
    .fetch_all(pool.get_conn())
    .await;

    match layouts {
        Ok(rows) => {
            let items: Vec<serde_json::Value> = rows.iter().map(|(id, cabinet_id, x, y, width, height, rotation)| {
                serde_json::json!({
                    "id": id,
                    "cabinet_id": cabinet_id,
                    "x": x,
                    "y": y,
                    "width": width,
                    "height": height,
                    "rotation": rotation,
                    "element_type": "cabinet"
                })
            }).collect();
            Ok(HttpResponse::Ok().json(ApiResponse::success(items, "获取机柜布局成功")))
        }
        Err(err) => Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("查询机柜布局失败: {err}")))),
    }
}

pub async fn get_room_cabinets_with_positions(
    pool: web::Data<DbPool>,
    room_id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let room_id = *room_id_path;

    let cabinets = sqlx::query_as::<_, (Uuid, String, Uuid, i32, Option<String>)>(
        "SELECT id, name, room_id, capacity, description FROM cabinets WHERE room_id = $1 ORDER BY name",
    )
    .bind(room_id)
    .fetch_all(pool.get_conn())
    .await;

    let cabinets = match cabinets {
        Ok(c) => c,
        Err(err) => return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("查询机柜失败: {err}")))),
    };

    let mut result = Vec::new();
    for (cab_id, cab_name, cab_room_id, capacity, cab_desc) in &cabinets {
        let positions = sqlx::query_as::<_, (Uuid, String, Option<Uuid>, i32, i32, Option<String>, Option<String>, Option<Uuid>)>(
            "SELECT id, name, cabinet_id, start_u, end_u, description, device_type, device_id FROM positions WHERE cabinet_id = $1 ORDER BY start_u",
        )
        .bind(cab_id)
        .fetch_all(pool.get_conn())
        .await
        .unwrap_or_default();

        let pos_items: Vec<serde_json::Value> = positions.iter().map(|(id, name, pos_cab_id, start_u, end_u, desc, dt, did)| {
            serde_json::json!({
                "id": id,
                "name": name,
                "cabinet_id": pos_cab_id,
                "start_u": start_u,
                "end_u": end_u,
                "description": desc,
                "device_type": dt,
                "device_id": did
            })
        }).collect();

        let layout = sqlx::query_as::<_, (i32, i32, i32, i32, i32)>(
            "SELECT x, y, width, height, rotation FROM cabinet_layouts WHERE cabinet_id = $1",
        )
        .bind(cab_id)
        .fetch_optional(pool.get_conn())
        .await
        .ok()
        .flatten();

        let layout_json = layout.map(|(x, y, w, h, r)| serde_json::json!({"x": x, "y": y, "width": w, "height": h, "rotation": r}));

        result.push(serde_json::json!({
            "id": cab_id,
            "name": cab_name,
            "room_id": cab_room_id,
            "capacity": capacity,
            "description": cab_desc,
            "positions": pos_items,
            "layout": layout_json
        }));
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(result, "获取房间机柜数据成功")))
}
