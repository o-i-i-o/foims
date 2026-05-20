use actix_web::{HttpRequest, HttpResponse, web};
use chrono::{DateTime, Utc};
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

use super::snmp::decrypt_snmp_fields_async;
use crate::app_state::AppState;
use crate::crypto::encrypt_password_async;
use crate::error::AppError;
use crate::models::{ApiResponse, Switch, SwitchCreate, SwitchUpdate, SwitchWithParent};
use crate::utils::pagination::DEFAULT_PAGE;
use crate::utils::{OperationLogParams, log_system_operation};
use tracing::warn;

const SWITCHES_DETAIL_COLUMNS: &str = r"
    id, name, model, vendor,
    location, snmp_version,
    snmp_community,
    snmp_username, snmp_auth_protocol,
    snmp_auth_password,
    snmp_priv_protocol,
    snmp_priv_password,
    snmp_port,
    position_id,
    cabinet_id, cabinet_name,
    room_id, room_name,
    start_u, end_u,
    position_network_id, network_region_id,
    description,
    device_type,
    ip_address,
    mac_address,
    created_at, updated_at";

const SWITCH_COLUMNS: &str = r"
    id, name,
    model, vendor,
    location, snmp_version,
    snmp_community,
    snmp_username, snmp_auth_protocol,
    snmp_auth_password,
    snmp_priv_protocol,
    snmp_priv_password,
    snmp_port,
    description, created_at, updated_at,
    position_id";

async fn mask_snmp_fields(switch: &mut SwitchWithParent) {
    let has_community = switch.snmp_community.is_some();
    let has_auth_password = switch.snmp_auth_password.is_some();
    let has_priv_password = switch.snmp_priv_password.is_some();

    decrypt_snmp_fields_async(switch).await;

    if has_community {
        switch.snmp_community = Some("••••••••".to_string());
    }
    if has_auth_password {
        switch.snmp_auth_password = Some("••••••••".to_string());
    }
    if has_priv_password {
        switch.snmp_priv_password = Some("••••••••".to_string());
    }
}

pub async fn get_switches(
    state: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let page: i64 = query
        .get("page")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PAGE);
    let page_size: i64 = query
        .get("page_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let search = query.get("search").cloned().unwrap_or_default();
    let name_filter = query.get("name").cloned().unwrap_or_default();
    let ip_filter = query.get("ip_address").cloned().unwrap_or_default();
    let model_filter = query.get("model").cloned().unwrap_or_default();
    let offset = (page - 1) * page_size;

    let has_filters = !search.is_empty()
        || !name_filter.is_empty()
        || !ip_filter.is_empty()
        || !model_filter.is_empty();

    let total: i64 = if has_filters {
        let mut conditions = Vec::new();
        let mut param_count = 1;

        if !search.is_empty() {
            conditions.push(format!(
                "(name ILIKE ${param_count} OR location ILIKE ${param_count} OR model ILIKE ${param_count} OR ip_address ILIKE {param_count})"
            ));
            param_count += 1;
        }

        if !name_filter.is_empty() {
            conditions.push(format!("name ILIKE ${param_count}"));
            param_count += 1;
        }

        if !ip_filter.is_empty() {
            conditions.push(format!("ip_address ILIKE ${param_count}"));
            param_count += 1;
        }

        if !model_filter.is_empty() {
            conditions.push(format!("model ILIKE ${param_count}"));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let count_query = format!("SELECT COUNT(*) FROM switches_with_details {where_clause}");

        let mut count_sql = sqlx::query_scalar(&count_query);

        if !search.is_empty() {
            let pattern = format!("%{search}%");
            count_sql = count_sql.bind(pattern);
        }

        if !name_filter.is_empty() {
            let pattern = format!("%{name_filter}%");
            count_sql = count_sql.bind(pattern);
        }

        if !ip_filter.is_empty() {
            let pattern = format!("%{ip_filter}%");
            count_sql = count_sql.bind(pattern);
        }

        if !model_filter.is_empty() {
            let pattern = format!("%{model_filter}%");
            count_sql = count_sql.bind(pattern);
        }

        count_sql.fetch_one(&state.pool()?.get_conn()).await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM switches_with_details")
            .fetch_one(&state.pool()?.get_conn())
            .await?
    };

    let switches_result = if has_filters {
        let mut conditions = Vec::new();
        let mut param_count = 1;

        if !search.is_empty() {
            conditions.push(format!(
                "(name ILIKE ${param_count} OR location ILIKE ${param_count} OR model ILIKE ${param_count} OR ip_address ILIKE {param_count})"
            ));
            param_count += 1;
        }

        if !name_filter.is_empty() {
            conditions.push(format!("name ILIKE ${param_count}"));
            param_count += 1;
        }

        if !ip_filter.is_empty() {
            conditions.push(format!("ip_address ILIKE ${param_count}"));
            param_count += 1;
        }

        if !model_filter.is_empty() {
            conditions.push(format!("model ILIKE ${param_count}"));
            param_count += 1;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let data_query = format!(
            "SELECT {} FROM switches_with_details {} ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
            SWITCHES_DETAIL_COLUMNS,
            where_clause,
            param_count,
            param_count + 1
        );

        let mut data_sql = sqlx::query_as::<_, SwitchWithParent>(&data_query);

        if !search.is_empty() {
            let pattern = format!("%{search}%");
            data_sql = data_sql.bind(pattern);
        }

        if !name_filter.is_empty() {
            let pattern = format!("%{name_filter}%");
            data_sql = data_sql.bind(pattern);
        }

        if !ip_filter.is_empty() {
            let pattern = format!("%{ip_filter}%");
            data_sql = data_sql.bind(pattern);
        }

        if !model_filter.is_empty() {
            let pattern = format!("%{model_filter}%");
            data_sql = data_sql.bind(pattern);
        }

        data_sql = data_sql.bind(page_size).bind(offset);

        data_sql.fetch_all(&state.pool()?.get_conn()).await
    } else {
        let query = format!(
            "SELECT {} FROM switches_with_details ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            SWITCHES_DETAIL_COLUMNS
        );
        sqlx::query_as::<_, SwitchWithParent>(&query)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&state.pool()?.get_conn())
            .await
    };

    let mut data = switches_result?;

    for switch in &mut data {
        mask_snmp_fields(switch).await;
    }
    let total_pages = (total + page_size - 1) / page_size;
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "items": data,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": total_pages
        }),
        "获取交换机列表成功",
    )))
}

pub async fn get_switch(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();

    let query = format!(
        "SELECT {} FROM switches_with_details WHERE id = $1",
        SWITCHES_DETAIL_COLUMNS
    );
    let mut data = sqlx::query_as::<_, SwitchWithParent>(&query)
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .ok_or_else(|| AppError::NotFound("交换机不存在".to_string()))?;

    mask_snmp_fields(&mut data).await;

    let ips = sqlx::query(
        r"SELECT
            m.id, m.device_type, rn.network_id,
            host(m.ip_address) as ip_address,
            m.ip_version, m.mac_address, m.hostname,
            m.status, m.last_seen, m.created_at, m.updated_at,
            n.network_region_id, nr.name as network_region
        FROM ips m
        LEFT JOIN positions p ON m.position_id = p.id
        LEFT JOIN cabinets c ON p.cabinet_id = c.id
        LEFT JOIN rooms r ON c.room_id = r.id
        LEFT JOIN room_networks rn ON r.id = rn.room_id
        LEFT JOIN network_cidrs n ON rn.network_id = n.id
        LEFT JOIN network_regions nr ON n.network_region_id = nr.id
        WHERE m.position_id = (SELECT position_id FROM switches WHERE id = $1)
        ORDER BY m.ip_address",
    )
    .bind(id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    let ips_json: Vec<serde_json::Value> = ips
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.get::<Uuid, _>(0),
                "device_type": row.get::<Option<String>, _>(1),
                "network_id": row.get::<Uuid, _>(2),
                "ip_address": row.get::<String, _>(3),
                "ip_version": row.get::<i16, _>(4),
                "mac_address": row.get::<Option<String>, _>(5),
                "hostname": row.get::<Option<String>, _>(6),
                "status": row.get::<String, _>(7),
                "last_seen": row.get::<DateTime<Utc>, _>(8),
                "created_at": row.get::<DateTime<Utc>, _>(9),
                "updated_at": row.get::<DateTime<Utc>, _>(10),
                "network_region_id": row.get::<Option<Uuid>, _>(11),
                "network_region": row.get::<Option<String>, _>(12)
            })
        })
        .collect();

    let mut response_data = serde_json::to_value(&data).unwrap_or_else(|e| {
        tracing::error!("JSON序列化失败: {}", e);
        serde_json::json!({})
    });
    response_data["ips"] = serde_json::to_value(&ips_json).unwrap_or(serde_json::json!([]));

    Ok(HttpResponse::Ok().json(ApiResponse::success(response_data, "获取交换机成功")))
}

pub async fn create_switch(
    state: web::Data<AppState>,
    req: web::Json<SwitchCreate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    req.validate()?;

    let ips = match &req.ips {
        Some(ips) if !ips.is_empty() => ips,
        _ => {
            return Err(AppError::Validation(
                "交换机必须至少配置一个IP地址".to_string(),
            ));
        }
    };

    let id = Uuid::new_v4();
    let now = Utc::now();

    let encrypted_snmp_community = req
        .snmp_community
        .as_ref()
        .filter(|c| !c.is_empty())
        .cloned();
    let encrypted_snmp_auth_password = req
        .snmp_auth_password
        .as_ref()
        .filter(|p| !p.is_empty())
        .cloned();
    let encrypted_snmp_priv_password = req
        .snmp_priv_password
        .as_ref()
        .filter(|p| !p.is_empty())
        .cloned();

    let encrypted_snmp_community = if let Some(c) = encrypted_snmp_community {
        encrypt_password_async(c).await
    } else {
        None
    };
    let encrypted_snmp_auth_password = if let Some(p) = encrypted_snmp_auth_password {
        encrypt_password_async(p).await
    } else {
        None
    };
    let encrypted_snmp_priv_password = if let Some(p) = encrypted_snmp_priv_password {
        encrypt_password_async(p).await
    } else {
        None
    };

    let position_id = req.position_id.unwrap_or_else(Uuid::new_v4);

    if req.position_id.is_none() {
        let position_result = if let Some(cabinet_id) = req.cabinet_id {
            sqlx::query(
                "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, device_type, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, 'switch', $6, $7, $8)"
            )
            .bind(position_id)
            .bind(&req.name)
            .bind(cabinet_id)
            .bind(req.start_u)
            .bind(req.end_u)
            .bind(&req.description)
            .bind(now)
            .bind(now)
            .execute(&state.pool()?.get_conn())
            .await
        } else {
            sqlx::query(
                "INSERT INTO positions (id, name, device_type, description, created_at, updated_at) VALUES ($1, $2, 'switch', $3, $4, $5)"
            )
            .bind(position_id)
            .bind(&req.name)
            .bind(&req.description)
            .bind(now)
            .bind(now)
            .execute(&state.pool()?.get_conn())
            .await
        };

        if let Err(e) = position_result {
            tracing::error!("创建交换机关联机位记录失败: {}", e);
        }
    } else {
        if let Err(e) = sqlx::query(
            "UPDATE positions SET device_type = 'switch', name = COALESCE(name, $1), updated_at = $2 WHERE id = $3"
        )
        .bind(&req.name)
        .bind(now)
        .bind(position_id)
        .execute(&state.pool()?.get_conn())
        .await
        {
            tracing::error!("更新交换机关联机位记录失败: {}", e);
        }
    }

    sqlx::query(
        r"INSERT INTO switches (
            id, name, model, vendor,
            location, snmp_version, snmp_community, snmp_username,
            snmp_auth_protocol, snmp_auth_password, snmp_priv_protocol,
            snmp_priv_password, snmp_port,
            description, created_at, updated_at, position_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.model)
    .bind(&req.vendor)
    .bind(&req.location)
    .bind(req.snmp_version.as_deref().unwrap_or("v2c"))
    .bind(&encrypted_snmp_community)
    .bind(&req.snmp_username)
    .bind(&req.snmp_auth_protocol)
    .bind(&encrypted_snmp_auth_password)
    .bind(&req.snmp_priv_protocol)
    .bind(&encrypted_snmp_priv_password)
    .bind(req.snmp_port.unwrap_or(161))
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .bind(position_id)
    .execute(&state.pool()?.get_conn())
    .await?;

    for ip in ips {
        let ip_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM ips WHERE ip_address = CAST($1 AS INET))",
        )
        .bind(&ip.ip_address)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

        if ip_exists {
            return Err(AppError::Conflict(format!(
                "IP地址 {} 已存在",
                ip.ip_address
            )));
        }

        let ip_version = crate::resource::ip::detect_ip_version(&ip.ip_address)?;

        let ip_manager_id = Uuid::new_v4();
        if let Err(e) = sqlx::query(
            "INSERT INTO ips (id, device_type, ip_address, ip_version, mac_address, hostname, position_id, switch_port_id, status, last_seen, created_at, updated_at)
             VALUES ($1, $2, CAST($3 AS INET), $4, $5, $6, $7, $8, $9, $10, $11, $12)"
        )
        .bind(ip_manager_id)
        .bind(ip.device_type.as_deref().unwrap_or("cabinet_position"))
        .bind(&ip.ip_address)
        .bind(ip_version)
        .bind(&ip.mac_address)
        .bind(&ip.hostname)
        .bind(position_id)
        .bind(ip.switch_port_id)
        .bind("active")
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(&state.pool()?.get_conn())
        .await
        {
            tracing::error!("创建交换机IP记录失败: {}", e);
            return Err(AppError::Internal(format!("创建IP记录失败: {e}")));
        }
    }

    let query = format!("SELECT {} FROM switches WHERE id = $1", SWITCH_COLUMNS);
    let data = sqlx::query_as::<_, Switch>(&query)
        .bind(id)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let details = serde_json::json!({
        "name": data.name,
        "model": data.model,
        "vendor": data.vendor,
        "location": data.location,
        "ip_count": req.ips.as_ref().unwrap_or(&vec![]).len()
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "create",
            resource_type: "switch",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }
    tracing::info!("交换机 {} 创建成功, ID: {}", data.name, id);

    Ok(HttpResponse::Ok().json(ApiResponse::success(data, "创建交换机成功")))
}

pub async fn update_switch(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    req: web::Json<SwitchUpdate>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();

    req.validate()?;

    let exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM switches WHERE id = $1)")
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await?;

    if !exists {
        return Err(AppError::NotFound("交换机不存在".to_string()));
    }

    let now = Utc::now();

    let encrypted_snmp_community = req
        .snmp_community
        .as_ref()
        .filter(|c| !c.is_empty())
        .cloned();
    let encrypted_snmp_auth_password = req
        .snmp_auth_password
        .as_ref()
        .filter(|p| !p.is_empty())
        .cloned();
    let encrypted_snmp_priv_password = req
        .snmp_priv_password
        .as_ref()
        .filter(|p| !p.is_empty())
        .cloned();

    let encrypted_snmp_community = if let Some(c) = encrypted_snmp_community {
        encrypt_password_async(c).await
    } else {
        None
    };
    let encrypted_snmp_auth_password = if let Some(p) = encrypted_snmp_auth_password {
        encrypt_password_async(p).await
    } else {
        None
    };
    let encrypted_snmp_priv_password = if let Some(p) = encrypted_snmp_priv_password {
        encrypt_password_async(p).await
    } else {
        None
    };

    sqlx::query(
        r"UPDATE switches SET
            name = COALESCE($1, name),
            model = COALESCE($2, model),
            vendor = COALESCE($3, vendor),
            location = COALESCE($4, location),
            snmp_version = COALESCE($5, snmp_version),
            snmp_community = COALESCE($6, snmp_community),
            snmp_username = COALESCE($7, snmp_username),
            snmp_auth_protocol = COALESCE($8, snmp_auth_protocol),
            snmp_auth_password = COALESCE($9, snmp_auth_password),
            snmp_priv_protocol = COALESCE($10, snmp_priv_protocol),
            snmp_priv_password = COALESCE($11, snmp_priv_password),
            snmp_port = COALESCE($12, snmp_port),
            description = COALESCE($13, description),
            updated_at = $14,
            position_id = COALESCE($15, position_id)
        WHERE id = $16",
    )
    .bind(&req.name)
    .bind(&req.model)
    .bind(&req.vendor)
    .bind(&req.location)
    .bind(&req.snmp_version)
    .bind(&encrypted_snmp_community)
    .bind(&req.snmp_username)
    .bind(&req.snmp_auth_protocol)
    .bind(&encrypted_snmp_auth_password)
    .bind(&req.snmp_priv_protocol)
    .bind(&encrypted_snmp_priv_password)
    .bind(req.snmp_port)
    .bind(&req.description)
    .bind(now)
    .bind(req.position_id)
    .bind(id)
    .execute(&state.pool()?.get_conn())
    .await?;

    if let Some(new_position_id) = req.position_id
        && let Err(e) = sqlx::query(
            "UPDATE positions SET device_type = 'switch', updated_at = $1 WHERE id = $2"
        )
        .bind(now)
        .bind(new_position_id)
        .execute(&state.pool()?.get_conn())
        .await
    {
        tracing::error!("更新新机位关联失败: {}", e);
    }

    if let Some(ips) = &req.ips {
        let position_id: Option<Uuid> =
            sqlx::query_scalar("SELECT position_id FROM switches WHERE id = $1")
                .bind(id)
                .fetch_optional(&state.pool()?.get_conn())
                .await
                .unwrap_or(None);

        if let Err(e) = sqlx::query(
            "DELETE FROM ips WHERE position_id = (SELECT position_id FROM switches WHERE id = $1)",
        )
        .bind(id)
        .execute(&state.pool()?.get_conn())
        .await
        {
            tracing::error!("删除交换机旧IP记录失败: {}", e);
        }

        for ip in ips {
            let ip_exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM ips WHERE ip_address = CAST($1 AS INET) AND position_id IS DISTINCT FROM (SELECT position_id FROM switches WHERE id = $2))",
            )
            .bind(&ip.ip_address)
            .bind(id)
            .fetch_one(&state.pool()?.get_conn())
            .await
            ?;

            if ip_exists {
                return Err(AppError::Conflict(format!(
                    "IP地址 {} 已被其他设备使用",
                    ip.ip_address
                )));
            }

            let ip_version = crate::resource::ip::detect_ip_version(&ip.ip_address)?;

            let ip_manager_id = Uuid::new_v4();
            if let Err(e) = sqlx::query(
                "INSERT INTO ips (id, device_type, ip_address, ip_version, mac_address, hostname, position_id, switch_port_id, status, last_seen, created_at, updated_at)
                 VALUES ($1, $2, CAST($3 AS INET), $4, $5, $6, $7, $8, $9, $10, $11, $12)"
            )
            .bind(ip_manager_id)
            .bind(ip.device_type.as_deref().unwrap_or("cabinet_position"))
            .bind(&ip.ip_address)
            .bind(ip_version)
            .bind(&ip.mac_address)
            .bind(&ip.hostname)
            .bind(position_id)
            .bind(ip.switch_port_id)
            .bind("active")
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&state.pool()?.get_conn())
            .await
            {
                tracing::error!("更新交换机IP记录失败: {}", e);
                return Err(AppError::Internal(format!("创建IP记录失败: {e}")));
            }
        }
    }

    let query = format!("SELECT {} FROM switches WHERE id = $1", SWITCH_COLUMNS);
    let data = sqlx::query_as::<_, Switch>(&query)
        .bind(id)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let details = serde_json::json!({
        "name": data.name,
        "model": data.model,
        "vendor": data.vendor,
        "location": data.location,
        "ip_count": req.ips.as_ref().unwrap_or(&vec![]).len()
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "update",
            resource_type: "switch",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }
    tracing::info!("交换机 {} 更新成功, ID: {}", data.name, id);

    Ok(HttpResponse::Ok().json(ApiResponse::success(data, "更新交换机成功")))
}

pub async fn delete_switch(
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    http_req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();

    if let Err(e) = sqlx::query(
        "DELETE FROM ips WHERE position_id = (SELECT position_id FROM switches WHERE id = $1)",
    )
    .bind(id)
    .execute(&state.pool()?.get_conn())
    .await
    {
        tracing::error!("删除交换机IP记录失败: {}", e);
    }

    if let Err(e) = sqlx::query(
        "DELETE FROM positions WHERE id = (SELECT position_id FROM switches WHERE id = $1)",
    )
    .bind(id)
    .execute(&state.pool()?.get_conn())
    .await
    {
        tracing::error!("删除交换机关联机位失败: {}", e);
    }

    let result = sqlx::query("DELETE FROM switches WHERE id = $1")
        .bind(id)
        .execute(&state.pool()?.get_conn())
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("交换机不存在".to_string()));
    }

    let details = serde_json::json!({
        "switch_id": id.to_string()
    });
    if let Err(e) = log_system_operation(
        &state.pool()?.get_conn(),
        OperationLogParams {
            req: &http_req,
            action: "delete",
            resource_type: "switch",
            resource_id: &id,
            details: &details,
            result: true,
        },
    )
    .await
    {
        warn!("记录操作日志失败: {}", e);
    }
    tracing::info!("交换机删除成功, ID: {}", id);

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "删除交换机成功")))
}
