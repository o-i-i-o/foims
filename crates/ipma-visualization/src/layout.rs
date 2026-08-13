use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json;
use sqlx::PgPool;
use uuid::Uuid;

// ==================== 错误类型 ====================

#[derive(Debug, thiserror::Error)]
pub enum VisualizationError {
    #[error("数据库错误: {0}")]
    Database(String),

    #[error("资源未找到: {0}")]
    NotFound(String),

    #[error("验证失败: {0}")]
    Validation(String),

    #[error("冲突: {0}")]
    Conflict(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl VisualizationError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            VisualizationError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            VisualizationError::NotFound(_) => StatusCode::NOT_FOUND,
            VisualizationError::Validation(_) => StatusCode::BAD_REQUEST,
            VisualizationError::Conflict(_) => StatusCode::CONFLICT,
            VisualizationError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for VisualizationError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(ApiResponse::<()>::error(self.to_string()));
        (status, body).into_response()
    }
}

impl From<sqlx::Error> for VisualizationError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                Some("23505") => {
                    VisualizationError::Conflict("数据已存在，请检查是否有重复记录".to_string())
                }
                Some("23503") => {
                    VisualizationError::Validation("关联数据不存在或无法删除".to_string())
                }
                Some("23514") => VisualizationError::Validation(db_err.message().to_string()),
                Some("22P02") => VisualizationError::Validation("数据格式无效".to_string()),
                Some("22023") => VisualizationError::Validation("参数值无效".to_string()),
                Some("08006") | Some("08001") | Some("08004") | Some("57P03") => {
                    VisualizationError::Database("数据库连接异常，请稍后重试".to_string())
                }
                Some("57014") => {
                    VisualizationError::Database("数据库操作超时，请稍后重试".to_string())
                }
                _ => {
                    let err_str = err.to_string();
                    if err_str.contains("invalid cidr") {
                        VisualizationError::Validation("不符合CIDR格式".to_string())
                    } else if err_str.contains("invalid inet") {
                        VisualizationError::Validation("不符合IP地址格式".to_string())
                    } else {
                        VisualizationError::Database("数据库操作失败，请稍后重试".to_string())
                    }
                }
            },
            sqlx::Error::RowNotFound => VisualizationError::NotFound("资源不存在".to_string()),
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => {
                VisualizationError::Database("数据库连接异常，请稍后重试".to_string())
            }
            sqlx::Error::Io(_) => {
                VisualizationError::Database("数据库连接异常，请稍后重试".to_string())
            }
            _ => VisualizationError::Database("数据库操作失败，请稍后重试".to_string()),
        }
    }
}

// ==================== API 响应模型 ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T, message: &str) -> Self {
        Self {
            success: true,
            message: message.to_string(),
            data: Some(data),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
        }
    }
}

/// 构造成功 JSON 响应
pub fn ok_json<T: Serialize>(data: T, message: &str) -> Response {
    (StatusCode::OK, Json(ApiResponse::success(data, message))).into_response()
}

// ==================== 布局模型 ====================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
}

impl Position {
    pub fn x_i32(&self) -> i32 {
        self.x.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
    }

    pub fn y_i32(&self) -> i32 {
        self.y.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
    }

    pub fn width_i32(&self) -> i32 {
        self.width.round().clamp(0.0, i32::MAX as f64) as i32
    }

    pub fn height_i32(&self) -> i32 {
        self.height.round().clamp(0.0, i32::MAX as f64) as i32
    }

    pub fn rotation_i32(&self) -> i32 {
        self.rotation.round().clamp(0.0, 360.0) as i32
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LayoutSaveRequest {
    pub r#type: String,
    pub room_id: Option<Uuid>,
    pub network_region_id: Option<Uuid>,
    pub cabinet_id: Option<Uuid>,
    pub layout: Vec<LayoutItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LayoutItem {
    pub id: Uuid,
    pub position: Position,
    pub element_type: String,
}

// ==================== 布局管理函数 ====================

pub async fn save_layout(
    pool: &PgPool,
    req: LayoutSaveRequest,
) -> Result<Response, VisualizationError> {
    if req.r#type == "workstation" {
        let Some(room_id) = req.room_id else {
            return Err(VisualizationError::Validation("房间ID不能为空".to_string()));
        };

        let mut tx = pool.begin().await?;

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
                 ON CONFLICT DO NOTHING",
            )
            .bind(room_id)
            .bind(&item.element_type)
            .bind(item.position.x_i32())
            .bind(item.position.y_i32())
            .bind(item.position.width_i32())
            .bind(item.position.height_i32())
            .bind(item.position.rotation_i32())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(ok_json((), "工位布局保存成功"))
    } else if req.r#type == "cabinet" {
        let Some(room_id) = req.room_id else {
            return Err(VisualizationError::Validation("房间ID不能为空".to_string()));
        };

        let mut tx = pool.begin().await?;

        let cabinet_ids: Vec<Uuid> = req.layout.iter().map(|item| item.id).collect();
        let existing_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cabinets WHERE id = ANY($1) AND room_id = $2")
                .bind(&cabinet_ids)
                .bind(room_id)
                .fetch_one(&mut *tx)
                .await?;

        if existing_count as usize != cabinet_ids.len() {
            return Err(VisualizationError::Validation(
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

        Ok(ok_json((), "机柜布局保存成功"))
    } else {
        Err(VisualizationError::Validation(
            "不支持的布局类型".to_string(),
        ))
    }
}

pub async fn delete_layout(pool: &PgPool, room_id: Uuid) -> Result<Response, VisualizationError> {
    sqlx::query(
        r"DELETE FROM workstation_layouts
         WHERE workstation_id IN (SELECT id FROM workstations WHERE room_id = $1)",
    )
    .bind(room_id)
    .execute(pool)
    .await?;

    sqlx::query("DELETE FROM element_layouts WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await?;

    Ok(ok_json((), "布局删除成功"))
}

pub async fn delete_positions_layout(
    pool: &PgPool,
    room_id: Uuid,
) -> Result<Response, VisualizationError> {
    sqlx::query(
        "DELETE FROM cabinet_layouts WHERE cabinet_id IN (SELECT id FROM cabinets WHERE room_id = $1)"
    )
    .bind(room_id)
    .execute(pool)
    .await?;

    Ok(ok_json((), "机柜布局删除成功"))
}

pub async fn get_layout(pool: &PgPool, room_id: Uuid) -> Result<Response, VisualizationError> {
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
    .fetch_all(pool)
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
    .fetch_all(pool)
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

    Ok(ok_json(layout_data, "布局获取成功"))
}

pub async fn get_positions_layout(
    pool: &PgPool,
    room_id: Uuid,
) -> Result<Response, VisualizationError> {
    let rows = sqlx::query_as::<_, (Uuid, i32, i32, i32, i32, i32)>(
        r"SELECT cl.cabinet_id, cl.x, cl.y, cl.width, cl.height, cl.rotation
          FROM cabinet_layouts cl
          JOIN cabinets c ON cl.cabinet_id = c.id
          WHERE c.room_id = $1",
    )
    .bind(room_id)
    .fetch_all(pool)
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

    Ok(ok_json(items, "获取机柜布局成功"))
}

pub async fn get_room_cabinets_with_positions(
    pool: &PgPool,
    room_id: Uuid,
) -> Result<Response, VisualizationError> {
    let cabinets = sqlx::query_as::<_, (Uuid, String, Uuid, i32, Option<String>)>(
        "SELECT id, name, room_id, capacity, description FROM cabinets WHERE room_id = $1 ORDER BY name",
    )
    .bind(room_id)
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();
    for (cab_id, cab_name, cab_room_id, capacity, cab_desc) in &cabinets {
        let positions = sqlx::query_as::<_, (Uuid, String, Option<Uuid>, i32, i32, Option<String>)>(
            "SELECT id, name, cabinet_id, start_u, end_u, description FROM positions WHERE cabinet_id = $1 ORDER BY start_u",
        )
        .bind(cab_id)
        .fetch_all(pool)
    .await
    ?;

        let pos_items: Vec<serde_json::Value> = positions
            .iter()
            .map(|(id, name, pos_cab_id, start_u, end_u, desc)| {
                serde_json::json!({
                    "id": id,
                    "name": name,
                    "cabinet_id": pos_cab_id,
                    "start_u": start_u,
                    "end_u": end_u,
                    "description": desc
                })
            })
            .collect();

        let layout = sqlx::query_as::<_, (i32, i32, i32, i32, i32)>(
            "SELECT x, y, width, height, rotation FROM cabinet_layouts WHERE cabinet_id = $1",
        )
        .bind(cab_id)
        .fetch_optional(pool)
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

    Ok(ok_json(result, "获取房间机柜数据成功"))
}
