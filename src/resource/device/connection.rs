use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{ApiResponse, DeviceConnectRequest, DeviceWithDetails};
use crate::utils::{OperationLogParams, log_system_operation};
use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use tracing::warn;
use uuid::Uuid;

pub async fn connect_device(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    req: web::Json<DeviceConnectRequest>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM devices WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_none() {
        return Err(AppError::NotFound("设备未找到".to_string()));
    }

    let resolved_outlet_id = match &req.net_outlet_id {
        Some(Some(outlet_id)) => {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM net_outlets WHERE id = $1)")
                    .bind(outlet_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if !exists {
                return Err(AppError::NotFound("信息点未找到".to_string()));
            }
            Some(*outlet_id)
        }
        Some(None) => None,
        None => {
            let current: Option<Uuid> =
                sqlx::query_scalar("SELECT net_outlet_id FROM devices WHERE id = $1")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
            current
        }
    };

    let now = Utc::now();

    sqlx::query(
        "UPDATE devices SET
         net_outlet_id = CASE WHEN $1::boolean THEN $2 ELSE net_outlet_id END,
         updated_at = $3
         WHERE id = $4",
    )
    .bind(req.net_outlet_id.is_some())
    .bind(resolved_outlet_id)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let updated_device = sqlx::query_as::<_, DeviceWithDetails>(
        "SELECT d.id, d.name, d.device_type, d.brand, d.model, d.serial_number,
                d.workstation_id, d.position_id, d.net_outlet_id,
                d.template_id, d.vendor, d.location,
                d.snmp_version, d.snmp_community, d.snmp_username,
                d.snmp_auth_protocol, d.snmp_auth_password,
                d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
                d.description,
                d.workstation_name, d.room_id, d.room_name, d.cabinet_id, d.cabinet_name,
                d.start_u, d.end_u, d.net_outlet_name, d.outlet_type,
                d.template_name,
                d.created_at::TIMESTAMPTZ, d.updated_at::TIMESTAMPTZ
         FROM devices_with_details d
         WHERE d.id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "net_outlet_id": updated_device.net_outlet_id
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "connect",
            resource_type: "device",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::<DeviceWithDetails>::success(
            updated_device,
            "设备连接设置成功",
        )),
    )
}

pub async fn disconnect_device(
    state: web::Data<AppState>,
    id_path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = *id_path;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM devices WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_none() {
        return Err(AppError::NotFound("设备未找到".to_string()));
    }

    let old_net_outlet_id: Option<Uuid> =
        sqlx::query_scalar("SELECT net_outlet_id FROM devices WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

    let now = Utc::now();

    sqlx::query("UPDATE devices SET net_outlet_id = NULL, updated_at = $1 WHERE id = $2")
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let updated_device = sqlx::query_as::<_, DeviceWithDetails>(
        "SELECT d.id, d.name, d.device_type, d.brand, d.model, d.serial_number,
                d.workstation_id, d.position_id, d.net_outlet_id,
                d.template_id, d.vendor, d.location,
                d.snmp_version, d.snmp_community, d.snmp_username,
                d.snmp_auth_protocol, d.snmp_auth_password,
                d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
                d.description,
                d.workstation_name, d.room_id, d.room_name, d.cabinet_id, d.cabinet_name,
                d.start_u, d.end_u, d.net_outlet_name, d.outlet_type,
                d.template_name,
                d.created_at::TIMESTAMPTZ, d.updated_at::TIMESTAMPTZ
         FROM devices_with_details d
         WHERE d.id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool()?.get_conn())
    .await?;

    let details = serde_json::json!({
        "disconnected": true,
        "previous_net_outlet_id": old_net_outlet_id
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "disconnect",
            resource_type: "device",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::<DeviceWithDetails>::success(
            updated_device,
            "设备断开连接成功",
        )),
    )
}
