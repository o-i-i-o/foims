//! 可视化模块的 axum 适配层（机房布局图纸 + 拓扑可视化）。
//!
//! `ipma-visualization` crate 只暴露纯数据库函数（不依赖 axum 与
//! 主程序 AppState），本模块负责将其包装为 HTTP handler：注入
//! AppState 连接池、转换错误类型并补记操作日志。可视化与资源管理
//! （`resource`）为平级模块，故本文件独立于 `resource/` 存放。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;

use crate::app_state::AppState;
use crate::routes::static_files::AppJson;
use crate::utils::{RequestMeta, log_op_best_effort};
use ipma_common::AppError;
use ipma_models::LayoutSaveRequest;
use ipma_visualization::{TopologyConnectionRequest, TopologyNodesRequest};
use serde_json;
use uuid::Uuid;

pub async fn save_layout(
    State(state): State<Arc<AppState>>,
    meta: RequestMeta,
    AppJson(req): AppJson<LayoutSaveRequest>,
) -> Result<Response, AppError> {
    let visualization_req = ipma_visualization::LayoutSaveRequest {
        r#type: req.r#type.clone(),
        room_id: req.room_id,
        network_region_id: req.network_region_id,
        cabinet_id: req.cabinet_id,
        layout: req
            .layout
            .iter()
            .map(|item| ipma_visualization::LayoutItem {
                id: item.id,
                position: ipma_visualization::Position {
                    x: item.position.x,
                    y: item.position.y,
                    width: item.position.width,
                    height: item.position.height,
                    rotation: item.position.rotation,
                },
                element_type: item.element_type.clone(),
            })
            .collect(),
    };

    let result = ipma_visualization::save_layout(&state.pool()?.get_conn(), visualization_req)
        .await
        .map_err(AppError::from)?;

    if let Some(room_id) = req.room_id {
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
    }

    Ok(result)
}

pub async fn delete_layout(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let result = ipma_visualization::delete_layout(&state.pool()?.get_conn(), room_id)
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

pub async fn delete_positions_layout(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    let result = ipma_visualization::delete_positions_layout(&state.pool()?.get_conn(), room_id)
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

pub async fn get_layout(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<Uuid>,
) -> Result<Response, AppError> {
    ipma_visualization::get_layout(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)
}

pub async fn get_positions_layout(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<Uuid>,
) -> Result<Response, AppError> {
    ipma_visualization::get_positions_layout(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)
}

pub async fn get_room_cabinets_with_positions(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<Uuid>,
) -> Result<Response, AppError> {
    ipma_visualization::get_room_cabinets_with_positions(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)
}

// ==================== 拓扑可视化 ====================

pub async fn get_topology_nodes(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    ipma_visualization::get_topology_nodes(&state.pool()?.get_conn())
        .await
        .map_err(AppError::from)
}

pub async fn save_topology_nodes(
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<TopologyNodesRequest>,
) -> Result<Response, AppError> {
    ipma_visualization::save_topology_nodes(&state.pool()?.get_conn(), req)
        .await
        .map_err(AppError::from)
}

pub async fn delete_topology_node(
    State(state): State<Arc<AppState>>,
    device_id: Path<Uuid>,
) -> Result<Response, AppError> {
    ipma_visualization::delete_topology_node(&state.pool()?.get_conn(), device_id)
        .await
        .map_err(AppError::from)
}

pub async fn get_topology_connections(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    ipma_visualization::get_topology_connections(&state.pool()?.get_conn())
        .await
        .map_err(AppError::from)
}

pub async fn create_topology_connection(
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<TopologyConnectionRequest>,
) -> Result<Response, AppError> {
    ipma_visualization::create_topology_connection(&state.pool()?.get_conn(), req)
        .await
        .map_err(AppError::from)
}

pub async fn delete_topology_connection(
    State(state): State<Arc<AppState>>,
    id: Path<Uuid>,
) -> Result<Response, AppError> {
    ipma_visualization::delete_topology_connection(&state.pool()?.get_conn(), id)
        .await
        .map_err(AppError::from)
}

pub async fn trigger_auto_discover(
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    ipma_visualization::trigger_auto_discover(&state.pool()?.get_conn())
        .await
        .map_err(AppError::from)
}
