//! 下拉选项精简端点：仅返回 id + name，避免下拉场景拉取整行全表
//!（此前前端以 page_size=1000 触顶全量拉取，列多行宽时序列化开销大）。

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Response;
use uuid::Uuid;

use foims_common::DbProvider;
use foims_common::{AppError, msg};

/// 单个选项（id + 名称）
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct ResourceOption {
    pub id: Uuid,
    pub name: String,
}

/// 下拉资源白名单：资源名 → 基础查询。
/// 支持的可选过滤参数见 [`get_resource_options`]。
const OPTION_QUERIES: &[(&str, &str)] = &[
    ("rooms", "SELECT id, name FROM rooms"),
    ("organizations", "SELECT id, name FROM organizations"),
    ("network-regions", "SELECT id, name FROM network_regions"),
    ("networks", "SELECT id, name FROM network_cidrs"),
    ("cabinets", "SELECT id, name FROM cabinets"),
    ("workstations", "SELECT id, name FROM workstations"),
    ("net-outlets", "SELECT id, name FROM net_outlets"),
    ("devices", "SELECT id, name FROM devices"),
    ("device-templates", "SELECT id, name FROM device_templates"),
];

/// GET /api/resources/options/{resource}——下拉专用 id+name 列表。
///
/// 可选过滤参数（按资源生效，不适用时被忽略）：
/// - `room_id`：cabinets / workstations / net-outlets / devices
/// - `region_id`：networks
/// - `cabinet_id` / `workstation_id`：devices（cabinet_id 经机位关联）；
///   devices 的三个过滤参数互斥，同时给出多个时返回 422
pub async fn get_resource_options<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(resource): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let Some((_, base_sql)) = OPTION_QUERIES.iter().find(|(name, _)| *name == resource) else {
        return Err(AppError::Validation(
            msg("server.common.invalid_param").with("param", "resource"),
        ));
    };

    // 过滤参数非法 UUID 显式 422（与 net_outlet/patch_panel 口径一致），
    // 不再静默忽略退化为全量列表
    let parse_filter = |key: &str, param: &str| -> Result<Option<Uuid>, AppError> {
        match query.get(key) {
            Some(v) if !v.is_empty() => Uuid::parse_str(v).map(Some).map_err(|_| {
                AppError::Validation(msg("server.common.invalid_param").with("param", param))
            }),
            _ => Ok(None),
        }
    };
    let room_id = parse_filter("room_id", "room_id")?;
    let region_id = parse_filter("region_id", "region_id")?;
    let cabinet_id = parse_filter("cabinet_id", "cabinet_id")?;
    let workstation_id = parse_filter("workstation_id", "workstation_id")?;

    // devices 资源的过滤参数互斥：room_id / cabinet_id / workstation_id
    // 同时给出多个时显式 422（并列罗列冲突参数），不再静默忽略其一
    if resource == "devices" {
        let mut given: Vec<&str> = Vec::with_capacity(3);
        if room_id.is_some() {
            given.push("room_id");
        }
        if cabinet_id.is_some() {
            given.push("cabinet_id");
        }
        if workstation_id.is_some() {
            given.push("workstation_id");
        }
        if given.len() > 1 {
            return Err(AppError::Validation(
                msg("server.common.invalid_param").with("param", given.join("/")),
            ));
        }
    }

    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(*base_sql);

    // 过滤条件按资源白名单拼接，值经 Uuid 解析后 bind（参数化）
    let push_cond =
        |builder: &mut sqlx::QueryBuilder<sqlx::Postgres>, column: &str, value: Uuid| {
            builder.push(" WHERE ");
            builder.push(column);
            builder.push(" = ");
            builder.push_bind(value);
        };

    match (
        resource.as_str(),
        room_id,
        region_id,
        cabinet_id,
        workstation_id,
    ) {
        ("cabinets" | "workstations" | "net-outlets" | "devices", Some(rid), ..) => {
            push_cond(&mut builder, "room_id", rid);
        }
        ("networks", _, Some(rid), ..) => {
            push_cond(&mut builder, "network_region_id", rid);
        }
        // devices 与机柜的关联经机位（positions.cabinet_id）传递
        ("devices", _, _, Some(cid), _) => {
            builder.push(" WHERE position_id IN (SELECT id FROM positions WHERE cabinet_id = ");
            builder.push_bind(cid);
            builder.push(")");
        }
        ("devices", _, _, _, Some(wid)) => {
            push_cond(&mut builder, "workstation_id", wid);
        }
        _ => {}
    }

    builder.push(" ORDER BY name");

    let options: Vec<ResourceOption> = builder
        .build_query_as()
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    Ok(foims_common::ok_json(options, "server.common.success"))
}
