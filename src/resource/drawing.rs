use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::LayoutSaveRequest;
use crate::utils::{OperationLogParams, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, web};
use ipma_visualization::{TopologyConnectionRequest, TopologyNodesRequest};
use serde_json;
use tracing::warn;
use uuid::Uuid;

pub async fn save_layout(
    state: web::Data<AppState>,
    req: web::Json<LayoutSaveRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
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
    }

    Ok(result)
}

pub async fn delete_layout(
    state: web::Data<AppState>,
    room_id: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let room_id = *room_id;

    let result = ipma_visualization::delete_layout(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)?;

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

    Ok(result)
}

pub async fn delete_positions_layout(
    state: web::Data<AppState>,
    room_id: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let room_id = *room_id;

    let result = ipma_visualization::delete_positions_layout(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)?;

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

    Ok(result)
}

pub async fn get_layout(
    state: web::Data<AppState>,
    room_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let room_id = *room_id;

    ipma_visualization::get_layout(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)
}

pub async fn get_positions_layout(
    state: web::Data<AppState>,
    room_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let room_id = *room_id;

    ipma_visualization::get_positions_layout(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)
}

pub async fn get_room_cabinets_with_positions(
    state: web::Data<AppState>,
    room_id_path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let room_id = *room_id_path;

    ipma_visualization::get_room_cabinets_with_positions(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)
}

// ==================== 拓扑可视化 ====================

pub async fn get_topology_nodes(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    ipma_visualization::get_topology_nodes(&state.pool()?.get_conn())
        .await
        .map_err(AppError::from)
}

pub async fn save_topology_nodes(
    state: web::Data<AppState>,
    req: web::Json<TopologyNodesRequest>,
) -> Result<HttpResponse, AppError> {
    ipma_visualization::save_topology_nodes(&state.pool()?.get_conn(), req.into_inner())
        .await
        .map_err(AppError::from)
}

pub async fn delete_topology_node(
    state: web::Data<AppState>,
    device_id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    ipma_visualization::delete_topology_node(&state.pool()?.get_conn(), device_id)
        .await
        .map_err(AppError::from)
}

pub async fn get_topology_connections(
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    ipma_visualization::get_topology_connections(&state.pool()?.get_conn())
        .await
        .map_err(AppError::from)
}

pub async fn create_topology_connection(
    state: web::Data<AppState>,
    req: web::Json<TopologyConnectionRequest>,
) -> Result<HttpResponse, AppError> {
    ipma_visualization::create_topology_connection(&state.pool()?.get_conn(), req.into_inner())
        .await
        .map_err(AppError::from)
}

pub async fn delete_topology_connection(
    state: web::Data<AppState>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    ipma_visualization::delete_topology_connection(&state.pool()?.get_conn(), id)
        .await
        .map_err(AppError::from)
}
