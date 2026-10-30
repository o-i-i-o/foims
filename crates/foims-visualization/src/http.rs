//! 可视化 HTTP handler 层（机房布局图纸 + 拓扑可视化）。
//!
//! handler 面向 `P: DbProvider` 泛型编写，由主程序 `AppState` 实现，
//! 与 resource/organization 的 crate 结构一致。业务与 SQL 位于
//! `layout` / `topology` 模块；此处负责 axum 提取器解析、审计日志
//! 与错误转换（`VisualizationError` → `AppError`）。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;

use foims_auth::meta::{RequestMeta, log_op_best_effort};
use foims_common::{AppError, AppJson, DbProvider};
use foims_models::LayoutSaveRequest;
use uuid::Uuid;
use validator::Validate;

use crate::topology::{TopologyConnectionRequest, TopologyNodesRequest};

/// POST /api/resources/layouts：保存机房布局（`type` 仅 workstation/cabinet；
/// 工位布局校验元素类型白名单与工位归属，请求即全量期望状态，未提交的
/// 既有布局行删除；类型非法或归属不符 400）。
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

/// DELETE /api/resources/layouts/workstation/{room_id}：清空房间的工位与
/// 门元素布局（两张表同事务删除，部分失败不落库）。
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

/// DELETE /api/resources/layouts/positions/{room_id}：清空房间的机柜布局。
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

/// GET /api/resources/layouts/workstation/{room_id}：读取房间的工位与门
/// 元素布局（无布局时返回空列表，不报 404）。
pub async fn get_layout<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(room_id): Path<Uuid>,
) -> Result<Response, AppError> {
    crate::layout::get_layout(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)
}

/// GET /api/resources/layouts/positions/{room_id}：读取房间的机柜布局
/// （无布局时返回空列表，不报 404）。
pub async fn get_positions_layout<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(room_id): Path<Uuid>,
) -> Result<Response, AppError> {
    crate::layout::get_positions_layout(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)
}

/// GET /api/resources/layouts/room-cabinets/{room_id}：读取房间内机柜及其
/// 机位与布局（三表批量查询后内存分组，避免逐柜查询）。
pub async fn get_room_cabinets_with_positions<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(room_id): Path<Uuid>,
) -> Result<Response, AppError> {
    crate::layout::get_room_cabinets_with_positions(&state.pool()?.get_conn(), room_id)
        .await
        .map_err(AppError::from)
}

/// GET /api/resources/topology/nodes：读取拓扑节点（联表回填设备、工位、
/// 房间、组织、机柜及最近一次 ARP 学习到的 IP）。
pub async fn get_topology_nodes<P: DbProvider>(
    State(state): State<Arc<P>>,
) -> Result<Response, AppError> {
    crate::topology::get_topology_nodes(&state.pool()?.get_conn())
        .await
        .map_err(AppError::from)
}

/// POST /api/resources/topology/nodes：保存拓扑节点坐标（`nodes` 非空、
/// 坐标/尺寸在 models 校验范围、device_id 去重后须全部存在，否则 400；
/// 按 device_id UPSERT，不删除未提交节点）。
pub async fn save_topology_nodes<P: DbProvider>(
    State(state): State<Arc<P>>,
    AppJson(req): AppJson<TopologyNodesRequest>,
) -> Result<Response, AppError> {
    // 节点坐标范围校验（0<=坐标<=100000、0<宽高<=10000）
    req.validate()?;

    crate::topology::save_topology_nodes(&state.pool()?.get_conn(), req)
        .await
        .map_err(AppError::from)
}

/// DELETE /api/resources/topology/nodes/{device_id}：删除设备的拓扑节点
/// （无该节点 404）。
pub async fn delete_topology_node<P: DbProvider>(
    State(state): State<Arc<P>>,
    device_id: Path<Uuid>,
) -> Result<Response, AppError> {
    crate::topology::delete_topology_node(&state.pool()?.get_conn(), device_id)
        .await
        .map_err(AppError::from)
}

/// GET /api/resources/topology/connections：读取连线（线路数据实时派生的
/// 物理连线 + 手动物理连线 + 逻辑连线）。
pub async fn get_topology_connections<P: DbProvider>(
    State(state): State<Arc<P>>,
) -> Result<Response, AppError> {
    crate::topology::get_topology_connections(&state.pool()?.get_conn())
        .await
        .map_err(AppError::from)
}

/// POST /api/resources/topology/connections：创建连线（类型仅 physical/
/// logical；两端须为不同且存在的设备、label ≤100 字符、端口须归属对应
/// 设备，否则 400；成员端口已被聚合、同设备对逻辑/物理连线重复 409）。
pub async fn create_topology_connection<P: DbProvider>(
    State(state): State<Arc<P>>,
    AppJson(req): AppJson<TopologyConnectionRequest>,
) -> Result<Response, AppError> {
    crate::topology::create_topology_connection(&state.pool()?.get_conn(), req)
        .await
        .map_err(AppError::from)
}

/// DELETE /api/resources/topology/connections/{id}：删除连线（不存在 404）。
pub async fn delete_topology_connection<P: DbProvider>(
    State(state): State<Arc<P>>,
    id: Path<Uuid>,
) -> Result<Response, AppError> {
    crate::topology::delete_topology_connection(&state.pool()?.get_conn(), id)
        .await
        .map_err(AppError::from)
}

/// POST /api/resources/topology/auto-discover：按线路数据批量补建缺失的
/// 拓扑节点并清理历史自动发现连线，返回新增节点数与派生连线数。
pub async fn trigger_auto_discover<P: DbProvider>(
    State(state): State<Arc<P>>,
) -> Result<Response, AppError> {
    crate::topology::trigger_auto_discover(&state.pool()?.get_conn())
        .await
        .map_err(AppError::from)
}
