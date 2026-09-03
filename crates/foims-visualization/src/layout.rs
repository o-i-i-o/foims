//! 可视化布局管理：错误类型与布局业务函数。
//!
//! 布局模型（Position/LayoutItem/LayoutSaveRequest）唯一副本位于
//! `foims-models`，本 crate 直接复用；SVG 渲染所需的 i32 取整访问器
//! 亦随类型定义在 models 中。

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use foims_common::{AppMessage, DbErrorKind, msg};
use foims_models::LayoutSaveRequest;
use serde_json;
use sqlx::PgPool;
use uuid::Uuid;

// ==================== 错误类型 ====================

/// 可视化模块错误类型。
#[derive(Debug, thiserror::Error)]
pub enum VisualizationError {
    #[error("数据库错误: {0}")]
    Database(AppMessage),

    #[error("资源未找到: {0}")]
    NotFound(AppMessage),

    #[error("验证失败: {0}")]
    Validation(AppMessage),

    #[error("冲突: {0}")]
    Conflict(AppMessage),

    #[error("内部错误: {0}")]
    Internal(AppMessage),
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
        let body = match self {
            VisualizationError::Database(m) => {
                foims_common::log_error!("log.error.database_detail", detail = m.log_string());
                Json(foims_common::ApiResponse::<()>::error(msg(
                    "server.error.database",
                )))
            }
            VisualizationError::Internal(m) => {
                foims_common::log_error!("log.error.internal_detail", detail = m.log_string());
                Json(foims_common::ApiResponse::<()>::error(msg(
                    "server.error.internal",
                )))
            }
            VisualizationError::NotFound(m)
            | VisualizationError::Validation(m)
            | VisualizationError::Conflict(m) => Json(foims_common::ApiResponse::<()>::error(m)),
        };
        (status, body).into_response()
    }
}

impl From<sqlx::Error> for VisualizationError {
    fn from(err: sqlx::Error) -> Self {
        match foims_common::classify_db_error(&err) {
            DbErrorKind::Conflict(m) => VisualizationError::Conflict(m),
            DbErrorKind::Validation(m) => VisualizationError::Validation(m),
            DbErrorKind::NotFound => VisualizationError::NotFound(msg("server.common.not_found")),
            DbErrorKind::Database(m) => VisualizationError::Database(m),
        }
    }
}

// ==================== API 响应模型 ====================

/// 统一 API 响应结构与成功响应构造（由 foims-common 提供，保持原有路径兼容）。
pub use foims_common::{ApiResponse, ok_json};

// ==================== 布局管理函数 ====================

pub async fn save_layout(
    pool: &PgPool,
    req: LayoutSaveRequest,
) -> Result<Response, VisualizationError> {
    if req.r#type == "workstation" {
        // room_id 与 layout 非空已由 LayoutSaveRequest 校验保证
        let room_id = req.room_id;
        let mut tx = pool.begin().await?;

        // element_type 白名单：仅 door（房间元素）与 workstation（工位），
        // 未知类型一律拒绝而非静默按工位处理
        if req
            .layout
            .iter()
            .any(|item| item.element_type != "door" && item.element_type != "workstation")
        {
            return Err(VisualizationError::Validation(msg(
                "server.visualization.type_unsupported",
            )));
        }

        let workstation_items: Vec<_> = req
            .layout
            .iter()
            .filter(|item| item.element_type == "workstation")
            .collect();
        let element_items: Vec<_> = req
            .layout
            .iter()
            .filter(|item| item.element_type == "door")
            .collect();

        // 归属校验（与 cabinet 分支同型）：全部工位必须属于该房间，
        // 防止跨房间工位被 UPSERT 进当前房间布局（重复 id 去重后再比对）
        let workstation_ids: std::collections::HashSet<Uuid> =
            workstation_items.iter().map(|item| item.id).collect();
        let workstation_ids: Vec<Uuid> = workstation_ids.into_iter().collect();
        if !workstation_ids.is_empty() {
            let existing_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM workstations WHERE id = ANY($1) AND room_id = $2",
            )
            .bind(&workstation_ids)
            .bind(room_id)
            .fetch_one(&mut *tx)
            .await?;

            if existing_count as usize != workstation_ids.len() {
                return Err(VisualizationError::Validation(msg(
                    "server.visualization.workstation_ids_invalid",
                )));
            }
        }

        for item in &workstation_items {
            sqlx::query(
                "INSERT INTO workstation_layouts (workstation_id, room_id, x, y, width, height, rotation)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (workstation_id)
                 DO UPDATE SET
                     room_id = EXCLUDED.room_id,
                     x = EXCLUDED.x,
                     y = EXCLUDED.y,
                     width = EXCLUDED.width,
                     height = EXCLUDED.height,
                     rotation = EXCLUDED.rotation,
                     updated_at = NOW()",
            )
            .bind(item.id)
            .bind(room_id)
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
                 ON CONFLICT (room_id, element_type)
                 DO UPDATE SET
                     x = EXCLUDED.x,
                     y = EXCLUDED.y,
                     width = EXCLUDED.width,
                     height = EXCLUDED.height,
                     rotation = EXCLUDED.rotation,
                     updated_at = NOW()",
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

        Ok(ok_json((), "server.visualization.workstation_layout_saved"))
    } else if req.r#type == "cabinet" {
        let room_id = req.room_id;
        let mut tx = pool.begin().await?;

        // id 先去重：重复条目会使 COUNT != len 误判为"归属非法"
        //（workstation 分支同口径）
        let mut cabinet_ids: Vec<Uuid> = Vec::with_capacity(req.layout.len());
        for item in &req.layout {
            if !cabinet_ids.contains(&item.id) {
                cabinet_ids.push(item.id);
            }
        }
        let existing_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cabinets WHERE id = ANY($1) AND room_id = $2")
                .bind(&cabinet_ids)
                .bind(room_id)
                .fetch_one(&mut *tx)
                .await?;

        if existing_count as usize != cabinet_ids.len() {
            return Err(VisualizationError::Validation(msg(
                "server.visualization.cabinet_ids_invalid",
            )));
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

        Ok(ok_json((), "server.visualization.cabinet_layout_saved"))
    } else {
        Err(VisualizationError::Validation(msg(
            "server.visualization.type_unsupported",
        )))
    }
}

pub async fn delete_layout(pool: &PgPool, room_id: Uuid) -> Result<Response, VisualizationError> {
    // 两表删除包进同一事务：部分删除不可回滚（与 save_layout 的事务口径一致）
    let mut tx = pool.begin().await?;

    sqlx::query(
        r"DELETE FROM workstation_layouts
         WHERE workstation_id IN (SELECT id FROM workstations WHERE room_id = $1)",
    )
    .bind(room_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM element_layouts WHERE room_id = $1")
        .bind(room_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(ok_json((), "server.visualization.layout_deleted"))
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

    Ok(ok_json((), "server.visualization.cabinet_layout_deleted"))
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

    let element_layouts = sqlx::query_as::<_, (Uuid, String, serde_json::Value)>(
        r"SELECT id,
                  element_type,
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

    layout_data.extend(
        element_layouts
            .into_iter()
            .map(|(id, element_type, position)| {
                serde_json::json!({
                    "id": id,
                    "element_type": element_type,
                    "position": position
                })
            }),
    );

    Ok(ok_json(
        layout_data,
        "server.visualization.layout_retrieved",
    ))
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

    Ok(ok_json(
        items,
        "server.visualization.cabinet_layout_retrieved",
    ))
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

    Ok(ok_json(
        result,
        "server.visualization.room_cabinets_retrieved",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use foims_models::{LayoutItem, Position};

    /// 构造指定字段的坐标
    fn pos(x: f64, y: f64, width: f64, height: f64, rotation: f64) -> Position {
        Position {
            x,
            y,
            width,
            height,
            rotation,
        }
    }

    #[test]
    fn 坐标取整_四舍五入() {
        let p = pos(1.4, 2.6, 100.5, 0.5, 10.0);
        assert_eq!(p.x_i32(), 1, "1.4 舍去");
        assert_eq!(p.y_i32(), 3, "2.6 进位");
        assert_eq!(p.width_i32(), 101, "100.5 进位（远离零取整）");
        assert_eq!(p.height_i32(), 1, "0.5 进位");
        assert_eq!(p.rotation_i32(), 10);
    }

    #[test]
    fn 坐标取整_负数远离零() {
        let p = pos(-1.5, -2.4, 3.0, 4.0, 0.0);
        assert_eq!(p.x_i32(), -2, "-1.5 远离零取整为 -2");
        assert_eq!(p.y_i32(), -2, "-2.4 取整为 -2");
    }

    #[test]
    fn 坐标钳制_超出i32范围饱和() {
        let p = pos(1e12, -1e12, 9e18, 9e18, 0.0);
        assert_eq!(p.x_i32(), i32::MAX, "超大 x 饱和为 i32::MAX");
        assert_eq!(p.y_i32(), i32::MIN, "超小 y 饱和为 i32::MIN");
        assert_eq!(p.width_i32(), i32::MAX);
        assert_eq!(p.height_i32(), i32::MAX);
    }

    #[test]
    fn 尺寸规范化_负值钳为零() {
        let p = pos(0.0, 0.0, -10.6, -0.4, 0.0);
        assert_eq!(p.width_i32(), 0, "负宽度钳为 0");
        assert_eq!(p.height_i32(), 0, "负高度钳为 0");
    }

    #[test]
    fn 旋转规范化_钳到0至360() {
        assert_eq!(pos(0.0, 0.0, 1.0, 1.0, 359.6).rotation_i32(), 360);
        assert_eq!(
            pos(0.0, 0.0, 1.0, 1.0, 400.0).rotation_i32(),
            360,
            "超上限钳为 360"
        );
        assert_eq!(
            pos(0.0, 0.0, 1.0, 1.0, -5.0).rotation_i32(),
            0,
            "负旋转钳为 0"
        );
        assert_eq!(pos(0.0, 0.0, 1.0, 1.0, 0.4).rotation_i32(), 0);
    }

    #[test]
    fn 坐标取整_nan饱和为零() {
        // f64 饱和转换：NaN as i32 == 0
        assert_eq!(pos(f64::NAN, 0.0, 1.0, 1.0, 0.0).x_i32(), 0);
    }

    #[test]
    fn position序列化_键名与往返() {
        let p = pos(1.5, 2.5, 30.0, 40.0, 90.0);
        let json = serde_json::to_string(&p).unwrap_or_else(|e| panic!("序列化失败: {e}"));
        assert_eq!(
            json,
            r#"{"x":1.5,"y":2.5,"width":30.0,"height":40.0,"rotation":90.0}"#
        );
        let back: Position =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(
            (back.x, back.y, back.width, back.height, back.rotation),
            (1.5, 2.5, 30.0, 40.0, 90.0)
        );
    }

    #[test]
    fn 布局保存请求_反序列化与字段判别() {
        let json = r#"{
            "type": "workstation",
            "room_id": "550e8400-e29b-41d4-a716-446655440000",
            "layout": []
        }"#;
        let req: LayoutSaveRequest =
            serde_json::from_str(json).unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(req.r#type, "workstation");
        assert_eq!(
            req.room_id.to_string(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert!(req.layout.is_empty());
    }

    #[test]
    fn 布局条目_反序列化() {
        let json = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440001",
            "position": {"x": 1, "y": 2, "width": 3, "height": 4, "rotation": 5},
            "element_type": "door"
        }"#;
        let item: LayoutItem =
            serde_json::from_str(json).unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(item.element_type, "door");
        assert_eq!(item.position.x_i32(), 1);
        assert_eq!(item.position.height_i32(), 4);
        assert_eq!(item.position.rotation_i32(), 5);
    }

    /// 各错误变体到 HTTP 状态码的映射
    #[test]
    fn 错误状态码_各变体映射() {
        let m = msg("server.x");
        let cases: Vec<(VisualizationError, StatusCode)> = vec![
            (
                VisualizationError::Database(m.clone()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (
                VisualizationError::NotFound(m.clone()),
                StatusCode::NOT_FOUND,
            ),
            (
                VisualizationError::Validation(m.clone()),
                StatusCode::BAD_REQUEST,
            ),
            (
                VisualizationError::Conflict(m.clone()),
                StatusCode::CONFLICT,
            ),
            (
                VisualizationError::Internal(m),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];
        for (err, expected) in cases {
            assert_eq!(err.status_code(), expected, "变体 {err}");
        }
    }

    #[test]
    fn from_sqlx_行不存在映射为not_found() {
        let err = VisualizationError::from(sqlx::Error::RowNotFound);
        match &err {
            VisualizationError::NotFound(m) => assert_eq!(m.key(), "server.common.not_found"),
            other => panic!("应映射为 NotFound，实际 {other}"),
        }
    }

    #[test]
    fn from_sqlx_连接池关闭映射为数据库错误() {
        let err = VisualizationError::from(sqlx::Error::PoolClosed);
        assert!(matches!(err, VisualizationError::Database(_)));
    }

    /// 极简 block_on：响应体为内存数据，忙轮询即可完成
    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        let mut fut = std::pin::pin!(fut);
        loop {
            if let std::task::Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
                return out;
            }
        }
    }

    /// 提取响应体 JSON 文本
    fn body_string(resp: Response) -> String {
        let Ok(bytes) = block_on(axum::body::to_bytes(resp.into_body(), usize::MAX)) else {
            panic!("读取响应体失败");
        };
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[test]
    fn into_response_校验错误透传key() {
        let resp = VisualizationError::Validation(msg("server.visualization.room_id_required"))
            .into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_string(resp);
        assert!(body.contains(r#""success":false"#), "响应体: {body}");
        assert!(
            body.contains(r#""message":"server.visualization.room_id_required""#),
            "响应体: {body}"
        );
    }

    #[test]
    fn into_response_数据库错误返回通用key() {
        let resp = VisualizationError::Database(msg("server.detail.sensitive")).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = body_string(resp);
        assert!(
            body.contains(r#""message":"server.error.database""#),
            "响应体: {body}"
        );
        assert!(!body.contains("sensitive"), "不应透出内部详情: {body}");
    }

    #[test]
    fn into_response_内部错误返回通用key() {
        let resp = VisualizationError::Internal(msg("server.detail.internal")).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = body_string(resp);
        assert!(
            body.contains(r#""message":"server.error.internal""#),
            "响应体: {body}"
        );
    }
}

impl From<VisualizationError> for foims_common::AppError {
    fn from(err: VisualizationError) -> Self {
        match err {
            VisualizationError::Database(m) => foims_common::AppError::Database(m),
            VisualizationError::NotFound(m) => foims_common::AppError::NotFound(m),
            VisualizationError::Validation(m) => foims_common::AppError::Validation(m),
            VisualizationError::Conflict(m) => foims_common::AppError::Conflict(m),
            VisualizationError::Internal(m) => foims_common::AppError::Internal(m),
        }
    }
}
