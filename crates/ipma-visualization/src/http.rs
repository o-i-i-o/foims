//! 可视化 HTTP handler 层（机房布局图纸 + 拓扑可视化）。
//!
//! handler 面向 `P: DbProvider` 泛型编写，由主程序 `AppState` 实现，
//! 与 resource/organization 的 crate 结构一致。业务与 SQL 位于
//! `layout` / `topology` 模块；此处负责 axum 提取器解析、审计日志
//! 与错误转换（`VisualizationError` → `AppError`）。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;

use ipma_auth::meta::{RequestMeta, log_op_best_effort};
use ipma_common::{AppError, AppJson, DbProvider};
use ipma_models::LayoutSaveRequest;
use uuid::Uuid;
use validator::Validate;

use crate::topology::{TopologyConnectionRequest, TopologyNodesRequest};

pub async fn save_layout<P: DbProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<LayoutSaveRequest>,
) -> Result<Response, AppError> {
    req.validate()?;

    let result = crate::layout::save_layout(&state.pool()?.get_conn(), req.clone())
        .await
        .map_err(AppError::from)?;

    let room_id = req.room_id;
    let details = serde_json::json!({
        "room_id": room_id,
        "layout_count": req.layout.len(),
        "type": req.r#type
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "layout",
        Some(&room_id),
        &details,
    )
    .await;

    Ok(result)
}

pub async fn delete_layout<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(room_id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let result = crate::layout::delete_layout(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)?;

    let details = serde_json::json!({
        "room_id": room_id
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "layout",
        Some(&room_id),
        &details,
    )
    .await;

    Ok(result)
}

pub async fn delete_positions_layout<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(room_id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let result = crate::layout::delete_positions_layout(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)?;

    let details = serde_json::json!({
        "room_id": room_id
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "layout",
        Some(&room_id),
        &details,
    )
    .await;

    Ok(result)
}

pub async fn get_layout<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(room_id): Path<Uuid>,
) -> Result<Response, AppError> {
    crate::layout::get_layout(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)
}

pub async fn get_positions_layout<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(room_id): Path<Uuid>,
) -> Result<Response, AppError> {
    crate::layout::get_positions_layout(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)
}

pub async fn get_room_cabinets_with_positions<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(room_id): Path<Uuid>,
) -> Result<Response, AppError> {
    crate::layout::get_room_cabinets_with_positions(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)
}

// ==================== 拓扑可视化 ====================

pub async fn get_topology_nodes<P: DbProvider>(
    State(state): State<Arc<P>>,
) -> Result<Response, AppError> {
    crate::topology::get_topology_nodes(&state.pool()?.get_conn())
        .await
        .map_err(AppError::from)
}

pub async fn save_topology_nodes<P: DbProvider>(
    State(state): State<Arc<P>>,
    AppJson(req): AppJson<TopologyNodesRequest>,
) -> Result<Response, AppError> {
    crate::topology::save_topology_nodes(&state.pool()?.get_conn(), req)
        .await
        .map_err(AppError::from)
}

pub async fn delete_topology_node<P: DbProvider>(
    State(state): State<Arc<P>>,
    device_id: Path<Uuid>,
) -> Result<Response, AppError> {
    crate::topology::delete_topology_node(&state.pool()?.get_conn(), device_id)
        .await
        .map_err(AppError::from)
}

pub async fn get_topology_connections<P: DbProvider>(
    State(state): State<Arc<P>>,
) -> Result<Response, AppError> {
    crate::topology::get_topology_connections(&state.pool()?.get_conn())
        .await
        .map_err(AppError::from)
}

pub async fn create_topology_connection<P: DbProvider>(
    State(state): State<Arc<P>>,
    AppJson(req): AppJson<TopologyConnectionRequest>,
) -> Result<Response, AppError> {
    crate::topology::create_topology_connection(&state.pool()?.get_conn(), req)
        .await
        .map_err(AppError::from)
}

pub async fn delete_topology_connection<P: DbProvider>(
    State(state): State<Arc<P>>,
    id: Path<Uuid>,
) -> Result<Response, AppError> {
    crate::topology::delete_topology_connection(&state.pool()?.get_conn(), id)
        .await
        .map_err(AppError::from)
}

pub async fn trigger_auto_discover<P: DbProvider>(
    State(state): State<Arc<P>>,
) -> Result<Response, AppError> {
    crate::topology::trigger_auto_discover(&state.pool()?.get_conn())
        .await
        .map_err(AppError::from)
}
