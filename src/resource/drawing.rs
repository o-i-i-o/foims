use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{ApiResponse, LayoutSaveRequest};
use crate::utils::{OperationLogParams, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, web};
use serde_json;
use tracing::warn;
use uuid::Uuid;

pub fn generate_region_map(
    _state: web::Data<AppState>,
    _id: web::Path<uuid::Uuid>,
) -> Result<HttpResponse, AppError> {
    Err(AppError::NotFound("区域地图生成功能尚未实现".to_string()))
}

pub fn generate_room_map(
    _state: web::Data<AppState>,
    _id: web::Path<uuid::Uuid>,
) -> Result<HttpResponse, AppError> {
    Err(AppError::NotFound("房间地图生成功能尚未实现".to_string()))
}

pub fn generate_workstation_map(
    _state: web::Data<AppState>,
    _id: web::Path<uuid::Uuid>,
) -> Result<HttpResponse, AppError> {
    Err(AppError::NotFound("工位地图生成功能尚未实现".to_string()))
}

pub async fn save_layout(
    state: web::Data<AppState>,
    req: web::Json<LayoutSaveRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    if req.r#type == "workstation" {
        let Some(room_id) = req.room_id else {
            return Err(AppError::Validation("房间ID不能为空".to_string()));
        };

        let mut tx = state.pool()?.get_conn().begin().await?;

        let workstation_items: Vec<_> = req
            .layout
            .iter()
            .filter(|item| item.element_type != "door")
            .collect();
        let element_items: Vec<_> = req
            .layout
            .iter()
            .filter(|item| item.element_type == "door")
            .collect();

        for item in &workstation_items {
            sqlx::query(
                "INSERT INTO workstation_layouts (workstation_id, x, y, width, height, rotation) 
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (workstation_id) 
                 DO UPDATE SET 
                     x = EXCLUDED.x,
                     y = EXCLUDED.y,
                     width = EXCLUDED.width,
                     height = EXCLUDED.height,
                     rotation = EXCLUDED.rotation,
                     updated_at = NOW()",
            )
            .bind(item.id)
            .bind(item.position.x_i32())
            .bind(item.position.y_i32())
            .bind(item.position.width_i32())
            .bind(item.position.height_i32())
            .bind(item.position.rotation_i32())
            .execute(&mut *tx)
            .await?;
        }

        for item in &element_items {
            sqlx::query(
                "INSERT INTO element_layouts (room_id, element_type, x, y, width, height, rotation) 
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT DO NOTHING"
            )
            .bind(room_id)
            .bind(&item.element_type)
            .bind(item.position.x_i32())
            .bind(item.position.y_i32())
            .bind(item.position.width_i32())
            .bind(item.position.height_i32())
            .bind(item.position.rotation_i32())
            .execute(&mut *tx).await?;
        }

        tx.commit().await?;

        let details = serde_json::json!({
            "room_id": room_id,
            "layout_count": req.layout.len(),
            "type": req.r#type
        });
        if let Err(e) = log_system_operation(
            &state.pool()?.get_conn(),
            OperationLogParams {
                req: &http_req,
                action: "update",
                resource_type: "layout",
                resource_id: &room_id,
                details: &details,
                result: true,
            },
        )
        .await
        {
            warn!("记录操作日志失败: {}", e);
        }

        Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "工位布局保存成功")))
    } else if req.r#type == "cabinet" {
        let Some(room_id) = req.room_id else {
            return Err(AppError::Validation("房间ID不能为空".to_string()));
        };

        let mut tx = state.pool()?.get_conn().begin().await?;

        let cabinet_ids: Vec<Uuid> = req.layout.iter().map(|item| item.id).collect();
        let existing_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cabinets WHERE id = ANY($1) AND room_id = $2")
                .bind(&cabinet_ids)
                .bind(room_id)
                .fetch_one(&mut *tx)
                .await?;

        if existing_count as usize != cabinet_ids.len() {
            return Err(AppError::Validation(
                "部分机柜ID不存在或不属于该房间".to_string(),
            ));
        }

        for item in &req.layout {
            sqlx::query(
                "INSERT INTO cabinet_layouts (cabinet_id, x, y, width, height, rotation) 
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (cabinet_id) 
                 DO UPDATE SET 
                     x = EXCLUDED.x,
                     y = EXCLUDED.y,
                     width = EXCLUDED.width,
                     height = EXCLUDED.height,
                     rotation = EXCLUDED.rotation,
                     updated_at = NOW()",
            )
            .bind(item.id)
            .bind(item.position.x_i32())
            .bind(item.position.y_i32())
            .bind(item.position.width_i32())
            .bind(item.position.height_i32())
            .bind(item.position.rotation_i32())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        let details = serde_json::json!({
            "room_id": room_id,
            "layout_count": req.layout.len(),
            "type": req.r#type
        });
        if let Err(e) = log_system_operation(
            &state.pool()?.get_conn(),
            OperationLogParams {
                req: &http_req,
                action: "update",
                resource_type: "layout",
                resource_id: &room_id,
                details: &details,
                result: true,
            },
        )
        .await
        {
            warn!("记录操作日志失败: {}", e);
        }

        Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "机柜布局保存成功")))
    } else {
        Err(AppError::Validation("不支持的布局类型".to_string()))
    }
}

pub async fn delete_layout(
    state: web::Data<AppState>,
    room_id: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let room_id = *room_id;

    sqlx::query(
        r"DELETE FROM workstation_layouts 
         WHERE workstation_id IN (SELECT id FROM workstations WHERE room_id = $1)",
    )
    .bind(room_id)
    .execute(&state.pool()?.get_conn())
    .await?;

    sqlx::query("DELETE FROM element_layouts WHERE room_id = $1")
        .bind(room_id)
        .execute(&state.pool()?.get_conn())
        .await?;

    let details = serde_json::json!({
        "room_id": room_id
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "layout",
            resource_id: &room_id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "布局删除成功")))
}

pub async fn delete_positions_layout(
    state: web::Data<AppState>,
    room_id: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let room_id = *room_id;

    sqlx::query(
        "DELETE FROM cabinet_layouts WHERE cabinet_id IN (SELECT id FROM cabinets WHERE room_id = $1)"
    )
    .bind(room_id)
    .execute(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "room_id": room_id
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "layout",
            resource_id: &room_id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "机柜布局删除成功")))
}

pub async fn get_layout(
    state: web::Data<AppState>,
    room_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let room_id = *room_id;

    let workstation_layouts = sqlx::query_as::<_, (Uuid, serde_json::Value)>(
        r"SELECT workstation_id,
                  json_build_object(
                      'x', x, 
                      'y', y, 
                      'width', width, 
                      'height', height, 
                      'rotation', rotation
                  ) as position
           FROM workstation_layouts
           WHERE room_id = $1",
    )
    .bind(room_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let element_layouts = sqlx::query_as::<_, (String, serde_json::Value)>(
        r"SELECT element_type,
                  json_build_object(
                      'x', x, 
                      'y', y, 
                      'width', width, 
                      'height', height, 
                      'rotation', rotation
                  ) as position
           FROM element_layouts
           WHERE room_id = $1",
    )
    .bind(room_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let mut layout_data: Vec<serde_json::Value> = workstation_layouts
        .into_iter()
        .map(|(id, position)| {
            serde_json::json!({
                "id": id,
                "element_type": "workstation",
                "position": position
            })
        })
        .collect();

    layout_data.extend(element_layouts.into_iter().map(|(element_type, position)| {
        serde_json::json!({
            "id": Uuid::nil(),
            "element_type": element_type,
            "position": position
        })
    }));

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<serde_json::Value>>::success(
            layout_data,
            "布局获取成功",
        )),
    )
}

pub async fn get_positions_layout(
    state: web::Data<AppState>,
    room_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let room_id = *room_id;

    let rows = sqlx::query_as::<_, (Uuid, i32, i32, i32, i32, i32)>(
        r"SELECT cl.cabinet_id, cl.x, cl.y, cl.width, cl.height, cl.rotation
          FROM cabinet_layouts cl
          JOIN cabinets c ON cl.cabinet_id = c.id
          WHERE c.room_id = $1",
    )
    .bind(room_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|(cabinet_id, x, y, width, height, rotation)| {
            serde_json::json!({
                "id": cabinet_id,
                "position": {
                    "x": x,
                    "y": y,
                    "width": width,
                    "height": height,
                    "rotation": rotation
                },
                "element_type": "cabinet"
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(ApiResponse::success(items, "获取机柜布局成功")))
}

pub async fn get_room_cabinets_with_positions(
    state: web::Data<AppState>,
    room_id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let room_id = *room_id_path;

    let cabinets = sqlx::query_as::<_, (Uuid, String, Uuid, i32, Option<String>)>(
        "SELECT id, name, room_id, capacity, description FROM cabinets WHERE room_id = $1 ORDER BY name",
    )
    .bind(room_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let mut result = Vec::new();
    for (cab_id, cab_name, cab_room_id, capacity, cab_desc) in &cabinets {
        let positions = sqlx::query_as::<_, (Uuid, String, Option<Uuid>, i32, i32, Option<String>, Option<String>)>(
            "SELECT id, name, cabinet_id, start_u, end_u, description, device_type FROM positions WHERE cabinet_id = $1 ORDER BY start_u",
        )
        .bind(cab_id)
        .fetch_all(&state.pool()?.get_conn())
    .await
    ?;

        let pos_items: Vec<serde_json::Value> = positions
            .iter()
            .map(|(id, name, pos_cab_id, start_u, end_u, desc, dt)| {
                serde_json::json!({
                    "id": id,
                    "name": name,
                    "cabinet_id": pos_cab_id,
                    "start_u": start_u,
                    "end_u": end_u,
                    "description": desc,
                    "device_type": dt
                })
            })
            .collect();

        let layout = sqlx::query_as::<_, (i32, i32, i32, i32, i32)>(
            "SELECT x, y, width, height, rotation FROM cabinet_layouts WHERE cabinet_id = $1",
        )
        .bind(cab_id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?;

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
