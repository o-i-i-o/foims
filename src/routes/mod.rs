//! 路由注册与数据管理转发 handler。
//!
//! 权限守卫约定：仅鉴权无入参使用的 handler 使用 `_admin: AdminUser`
//! 形式的提取器参数（下划线前缀表示“仅用其副作用”），为规范允许的
//! 唯一下划线例外，见 docs/code-style.md。

pub mod static_files;

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Multipart, Query, State};
use axum::middleware;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::log::notification::{
    get_notifications, mark_all_notifications_read, mark_notification_read,
};
use crate::log::{get_login_logs, get_operation_logs};
use crate::routes::static_files::AppJson;
use crate::system::certificate;
use crate::system::config::{
    backup_config, disable_init_mode, get_dashboard_stats, get_notification_settings,
    get_page_timeout_config, get_password_policy, get_service_status, get_session_timeout_config,
    get_smtp_config, get_supported_languages, get_system_config, get_system_info, register_service,
    restart_application, restore_config, send_system_email, test_smtp_connection as test_smtp,
    update_language_setting, update_notification_settings, update_page_timeout_config,
    update_password_policy, update_session_timeout_config, update_smtp_config,
    update_system_config,
};
use crate::system::scheduled_task::{
    create_scheduled_task, delete_scheduled_task, get_scheduled_task, get_scheduled_tasks,
    get_task_logs, run_scheduled_task_now, toggle_scheduled_task, update_scheduled_task,
};
use ipma_common::AppError;

async fn data_export_csv(
    _admin: ipma_auth::extractor::AdminUser,
    State(state): State<Arc<AppState>>,
    type_param: Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    ipma_data_management::export_csv(state.as_ref().clone(), type_param)
        .await
        .map_err(AppError::from)
}

async fn data_import_csv(
    _admin: ipma_auth::extractor::AdminUser,
    State(state): State<Arc<AppState>>,
    payload: Multipart,
) -> Result<Response, AppError> {
    ipma_data_management::import_csv(state.as_ref().clone(), payload)
        .await
        .map_err(AppError::from)
}

async fn data_download_template(
    type_param: Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    ipma_data_management::download_template(type_param)
        .await
        .map_err(AppError::from)
}

async fn data_export_database(
    _admin: ipma_auth::extractor::AdminUser,
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    ipma_data_management::export_database(state.as_ref().clone())
        .await
        .map_err(AppError::from)
}

async fn data_clear_logs(
    _admin: ipma_auth::extractor::AdminUser,
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<ipma_data_management::ClearLogsRequest>,
) -> Result<Response, AppError> {
    ipma_data_management::clear_logs(state.as_ref().clone(), req)
        .await
        .map_err(AppError::from)
}

async fn data_get_logs_stats(
    _admin: ipma_auth::extractor::AdminUser,
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    ipma_data_management::get_logs_stats(state.as_ref().clone())
        .await
        .map_err(AppError::from)
}

async fn health_check() -> Response {
    ipma_common::ok_json(serde_json::json!({"status": "ok"}), "server.common.success")
}

/// 资源写操作与敏感查询的管理员守卫（security-review A-2/S-2）。
///
/// 此前资源 CRUD 仅要求登录：普通用户可任意增删改组织/网段/设备数据、
/// 发起 SNMP 探测与读取审计日志。守卫规则：
/// - `/api/resources/**` 下所有非 GET/HEAD/OPTIONS 请求（写操作、同步、探测）；
/// - 实时 SNMP 探测的 GET 端点（snmp-info / snmp-ports，向任意内网目标发包）；
/// - `/api/logs/**` 审计日志查询。
///
/// 该守卫必须挂在 auth_middleware 之内（先由其校验令牌并注入 JwtClaims）。
async fn admin_guard_middleware(req: axum::extract::Request, next: Next) -> Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let is_read = method == axum::http::Method::GET
        || method == axum::http::Method::HEAD
        || method == axum::http::Method::OPTIONS;

    // 等保三权分立的角色矩阵：
    // - 审计日志（/api/logs/**、日志统计、任务日志查询）：admin 或 auditor（审计管理员只读）
    // - 其余受守卫资源（资源写、SNMP 查询）：admin（secadmin/auditor 由各自提取器按端点放行）
    // - 日志清理等写操作仍由 handler 层 AdminUser 提取器约束
    let needs_admin_with_roles: Option<&[&str]> = if path.starts_with("/api/logs/")
        || (is_read
            && (path == "/api/system/logs/stats" || path == "/api/system/scheduled-tasks/logs"))
    {
        Some(&["admin", "auditor"])
    } else {
        None
    };
    let needs_admin = needs_admin_with_roles.is_some()
        || (path.starts_with("/api/resources") && !is_read)
        || (is_read && (path.ends_with("/snmp-info") || path.ends_with("/snmp-ports")));

    if !needs_admin {
        return next.run(req).await;
    }

    let allowed_roles = needs_admin_with_roles.unwrap_or(&["admin"]);

    match req.extensions().get::<ipma_auth::utils::JwtClaims>() {
        Some(claims) if allowed_roles.contains(&claims.role.as_str()) => next.run(req).await,
        Some(_) => (
            StatusCode::FORBIDDEN,
            Json(ipma_common::ApiResponse::<()>::error(ipma_common::msg(
                "server.auth.admin_required",
            ))),
        )
            .into_response(),
        None => (
            StatusCode::UNAUTHORIZED,
            Json(ipma_common::ApiResponse::<()>::error(ipma_common::msg(
                "server.auth.auth_failed",
            ))),
        )
            .into_response(),
    }
}

/// 公开的初始化状态查询（不需要认证）
/// 供前端登录页判断：当 init_enabled=true 时跳转到初始化页 /init_index.html
/// 仅返回 init_enabled 一个布尔字段，避免泄露系统是否已初始化等额外信息。
pub async fn get_init_status(State(state): State<Arc<AppState>>) -> Response {
    ipma_common::ok_json(
        serde_json::json!({
            "init_enabled": state.config.init.enabled,
        }),
        "server.common.success",
    )
}

// 初始化相关路由将根据配置动态添加（在 main.rs 中）
// 这里只定义基础路由
pub fn init_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    // 公开认证路由（不需要认证）
    let public_auth_routes = Router::new()
        .route("/api/auth/login", post(ipma_auth::login::login::<AppState>))
        .route(
            "/api/auth/login/email",
            post(ipma_auth::login::login_with_email_code::<AppState>),
        )
        .route(
            "/api/auth/login/send-code",
            post(ipma_auth::login::send_login_code::<AppState>),
        )
        .route(
            "/api/auth/login/two-factor",
            post(ipma_auth::login::login_with_two_factor::<AppState>),
        )
        .route(
            "/api/auth/login/send-2fa-code",
            post(ipma_auth::login::send_two_factor_code::<AppState>),
        )
        .route(
            "/api/auth/login/ldap",
            post(ipma_auth::ldap::login_with_ldap::<AppState>),
        )
        .route(
            "/api/auth/sso/login",
            get(ipma_auth::sso::sso_login::<AppState>),
        )
        .route(
            "/api/auth/sso/callback",
            get(ipma_auth::sso::sso_callback::<AppState>),
        )
        .route(
            "/api/auth/methods",
            get(ipma_auth::sso::get_auth_methods::<AppState>),
        )
        .route("/api/auth/captcha", get(ipma_auth::get_captcha))
        .route(
            "/api/auth/logout",
            post(ipma_auth::login::logout::<AppState>),
        )
        .route(
            "/api/auth/refresh",
            post(ipma_auth::login::refresh_token::<AppState>),
        )
        .route(
            "/api/auth/forgot-password",
            post(ipma_auth::login::forgot_password::<AppState>),
        )
        .route(
            "/api/auth/reset-password",
            post(ipma_auth::login::reset_password::<AppState>),
        );

    // 公开的站点 CA 端点：CA 证书是公开数据，登录页提供下载入口（仅 PEM）；
    // 私钥不存在任何公开通道
    let public_ca_routes = Router::new()
        .route(
            "/api/certificate/ca/info",
            get(crate::system::certificate::ca_info),
        )
        .route(
            "/api/certificate/ca/download",
            get(crate::system::certificate::ca_download),
        );

    // /me 路由（单独应用认证中间件）
    let me_routes = Router::new()
        .route(
            "/api/auth/me",
            get(ipma_auth::login::get_current_user::<AppState>),
        )
        .route(
            "/api/auth/change-password",
            post(ipma_auth::login::change_password::<AppState>),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            ipma_auth::login::auth_middleware::<AppState>,
        ));

    // 健康检查（不需要认证）
    let health_routes = Router::new().route("/health", get(health_check));

    // 需要认证的 API 路由
    let protected_api_routes = Router::new()
        // 用户管理路由
        .route(
            "/api/users",
            get(ipma_auth::user::get_users::<AppState>)
                .post(ipma_auth::user::create_user::<AppState>),
        )
        .route(
            "/api/users/{id}",
            get(ipma_auth::user::get_user::<AppState>)
                .put(ipma_auth::user::update_user::<AppState>)
                .delete(ipma_auth::user::delete_user::<AppState>),
        )
        // 2FA管理路由
        .route(
            "/api/two-factor/init",
            post(ipma_auth::login::init_two_factor::<AppState>),
        )
        .route(
            "/api/two-factor/enable",
            post(ipma_auth::login::enable_two_factor::<AppState>),
        )
        .route(
            "/api/two-factor/disable",
            post(ipma_auth::login::disable_two_factor::<AppState>),
        )
        // 资源管理路由
        // 下拉专用精简选项端点（id+name，仅登录即可读）
        .route(
            "/api/resources/options/{resource}",
            get(ipma_resource::options::get_resource_options::<AppState>),
        )
        // 网络管理
        .route(
            "/api/resources/networks",
            get(ipma_resource::get_networks::<AppState>)
                .post(ipma_resource::create_network::<AppState>),
        )
        .route(
            "/api/resources/networks/{id}",
            get(ipma_resource::get_network::<AppState>)
                .put(ipma_resource::update_network::<AppState>)
                .delete(ipma_resource::delete_network::<AppState>),
        )
        // 网络区域管理
        .route(
            "/api/resources/network-regions",
            get(ipma_resource::get_network_regions::<AppState>)
                .post(ipma_resource::create_network_region::<AppState>),
        )
        .route(
            "/api/resources/network-regions/{id}",
            get(ipma_resource::get_network_region::<AppState>)
                .put(ipma_resource::update_network_region::<AppState>)
                .delete(ipma_resource::delete_network_region::<AppState>),
        )
        .route(
            "/api/resources/network-regions/{id}/cabinets",
            get(ipma_resource::get_cabinets_by_network_region::<AppState>),
        )
        // 房间管理
        .route(
            "/api/resources/rooms",
            get(ipma_resource::get_rooms::<AppState>).post(ipma_resource::create_room::<AppState>),
        )
        .route(
            "/api/resources/rooms/{id}",
            get(ipma_resource::get_room::<AppState>)
                .put(ipma_resource::update_room::<AppState>)
                .delete(ipma_resource::delete_room::<AppState>),
        )
        .route(
            "/api/resources/rooms/{id}/brief",
            get(ipma_resource::get_room_brief::<AppState>),
        )
        .route(
            "/api/resources/rooms/{id}/networks",
            get(ipma_resource::get_room_networks::<AppState>),
        )
        .route(
            "/api/resources/rooms/{id}/children",
            put(ipma_resource::sync_room_children::<AppState>),
        )
        .route(
            "/api/resources/rooms/{id}/net-outlets",
            put(ipma_resource::sync_room_net_outlets::<AppState>),
        )
        // 机柜管理
        .route(
            "/api/resources/cabinets",
            get(ipma_resource::get_cabinets::<AppState>)
                .post(ipma_resource::create_cabinet::<AppState>),
        )
        .route(
            "/api/resources/cabinets/{id}",
            get(ipma_resource::get_cabinet::<AppState>)
                .put(ipma_resource::update_cabinet::<AppState>)
                .delete(ipma_resource::delete_cabinet::<AppState>),
        )
        .route(
            "/api/resources/cabinets/{id}/networks",
            get(ipma_resource::get_cabinet_networks::<AppState>),
        )
        .route(
            "/api/resources/cabinets/{id}/positions",
            put(ipma_resource::sync_cabinet_positions::<AppState>),
        )
        .route(
            "/api/resources/cabinets/{id}/patch-panels",
            put(ipma_resource::sync_cabinet_patch_panels::<AppState>),
        )
        // 工位管理
        .route(
            "/api/resources/workstations",
            get(ipma_resource::get_workstations::<AppState>)
                .post(ipma_resource::create_workstation::<AppState>),
        )
        .route(
            "/api/resources/workstations/{id}",
            get(ipma_resource::get_workstation::<AppState>)
                .put(ipma_resource::update_workstation::<AppState>)
                .delete(ipma_resource::delete_workstation::<AppState>),
        )
        // 机位管理
        .route(
            "/api/resources/positions",
            get(ipma_resource::get_positions::<AppState>)
                .post(ipma_resource::create_cabinet_position::<AppState>),
        )
        .route(
            "/api/resources/positions/{id}",
            get(ipma_resource::get_cabinet_position::<AppState>)
                .put(ipma_resource::update_cabinet_position::<AppState>)
                .delete(ipma_resource::delete_cabinet_position::<AppState>),
        )
        // IP查询
        .route(
            "/api/resources/ip",
            get(ipma_resource::get_ip_managers::<AppState>),
        )
        .route(
            "/api/resources/ip/pull",
            post(ipma_resource::pull_ip_managers::<AppState>),
        )
        .route(
            "/api/resources/ip/available/{network_id}",
            get(ipma_resource::get_available_ips::<AppState>),
        )
        .route(
            "/api/resources/ip/auto-assign",
            post(ipma_resource::auto_assign_ip::<AppState>),
        )
        .route(
            "/api/resources/ip/batch",
            post(ipma_resource::batch_create_ip_managers::<AppState>),
        )
        // 布局管理
        .route(
            "/api/resources/layouts",
            post(ipma_visualization::http::save_layout::<AppState>),
        )
        .route(
            "/api/resources/layouts/workstation/{room_id}",
            get(ipma_visualization::http::get_layout::<AppState>)
                .delete(ipma_visualization::http::delete_layout::<AppState>),
        )
        .route(
            "/api/resources/layouts/positions/{room_id}",
            get(ipma_visualization::http::get_positions_layout::<AppState>)
                .delete(ipma_visualization::http::delete_positions_layout::<AppState>),
        )
        .route(
            "/api/resources/layouts/room-cabinets/{room_id}",
            get(ipma_visualization::http::get_room_cabinets_with_positions::<AppState>),
        )
        // 拓扑可视化
        .route(
            "/api/resources/topology/nodes",
            get(ipma_visualization::http::get_topology_nodes::<AppState>)
                .post(ipma_visualization::http::save_topology_nodes::<AppState>),
        )
        .route(
            "/api/resources/topology/nodes/{device_id}",
            delete(ipma_visualization::http::delete_topology_node::<AppState>),
        )
        .route(
            "/api/resources/topology/connections",
            get(ipma_visualization::http::get_topology_connections::<AppState>)
                .post(ipma_visualization::http::create_topology_connection::<AppState>),
        )
        .route(
            "/api/resources/topology/connections/{id}",
            delete(ipma_visualization::http::delete_topology_connection::<AppState>),
        )
        .route(
            "/api/resources/topology/auto-discover",
            post(ipma_visualization::http::trigger_auto_discover::<AppState>),
        )
        // 组织管理
        .route(
            "/api/resources/organizations",
            get(ipma_organization::get_organizations::<AppState>)
                .post(ipma_organization::create_organization::<AppState>),
        )
        .route(
            "/api/resources/organizations/tree",
            get(ipma_organization::get_organization_tree::<AppState>),
        )
        .route(
            "/api/resources/organizations/{id}",
            get(ipma_organization::get_organization::<AppState>)
                .put(ipma_organization::update_organization::<AppState>)
                .delete(ipma_organization::delete_organization::<AppState>),
        )
        .route(
            "/api/resources/organizations/{id}/children",
            get(ipma_organization::get_children::<AppState>),
        )
        .route(
            "/api/resources/organizations/{id}/allowed-child-types",
            get(ipma_organization::get_allowed_child_types::<AppState>),
        )
        .route(
            "/api/resources/organizations/{id}/rooms",
            get(ipma_organization::get_org_rooms::<AppState>),
        )
        // 员工管理（挂在组织节点下；GET 登录即可读供下拉使用）
        .route(
            "/api/resources/employees",
            get(ipma_organization::employee::get_employees::<AppState>)
                .post(ipma_organization::employee::create_employee::<AppState>),
        )
        .route(
            "/api/resources/employees/{id}",
            get(ipma_organization::employee::get_employee::<AppState>)
                .put(ipma_organization::employee::update_employee::<AppState>)
                .delete(ipma_organization::employee::delete_employee::<AppState>),
        )
        // 组织模板管理
        .route(
            "/api/resources/org-templates",
            get(ipma_organization::get_org_templates::<AppState>)
                .post(ipma_organization::create_org_template::<AppState>),
        )
        .route(
            "/api/resources/org-templates/available-types",
            get(ipma_organization::get_available_org_types::<AppState>),
        )
        .route(
            "/api/resources/org-templates/{id}",
            get(ipma_organization::get_org_template::<AppState>)
                .put(ipma_organization::update_org_template::<AppState>)
                .delete(ipma_organization::delete_org_template::<AppState>),
        )
        // 信息点管理
        .route(
            "/api/resources/net-outlets",
            get(ipma_resource::get_net_outlets::<AppState>)
                .post(ipma_resource::create_net_outlet::<AppState>),
        )
        .route(
            "/api/resources/net-outlets/{id}",
            get(ipma_resource::get_net_outlet::<AppState>)
                .put(ipma_resource::update_net_outlet::<AppState>)
                .delete(ipma_resource::delete_net_outlet::<AppState>),
        )
        // 配线架管理（隶属机柜，列表供线路端点选择）
        .route(
            "/api/resources/patch-panels",
            get(ipma_resource::get_patch_panels::<AppState>),
        )
        // 物理链路管理
        .route(
            "/api/resources/cable-links",
            get(ipma_resource::get_cable_links::<AppState>)
                .post(ipma_resource::create_cable_link::<AppState>),
        )
        .route(
            "/api/resources/cable-links/path",
            get(ipma_resource::get_cable_path::<AppState>),
        )
        .route(
            "/api/resources/cable-links/{id}",
            get(ipma_resource::get_cable_link::<AppState>)
                .put(ipma_resource::update_cable_link::<AppState>)
                .delete(ipma_resource::delete_cable_link::<AppState>),
        )
        // 设备模板管理
        .route(
            "/api/resources/device-templates",
            get(ipma_resource::get_device_templates::<AppState>),
        )
        .route(
            "/api/resources/device-templates/{id}",
            get(ipma_resource::get_device_template::<AppState>)
                .put(ipma_resource::update_device_template::<AppState>)
                .delete(ipma_resource::delete_device_template::<AppState>),
        )
        // 设备管理（含统一端口接口/MAC/LLDP/SNMP 功能）
        .route(
            "/api/resources/devices",
            get(ipma_resource::get_devices::<AppState>)
                .post(ipma_resource::create_device::<AppState>),
        )
        .route(
            "/api/resources/devices/interfaces",
            get(ipma_resource::get_all_device_interfaces::<AppState>),
        )
        .route(
            "/api/resources/devices/test-snmp",
            post(ipma_resource::test_snmp_connection::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}",
            get(ipma_resource::get_device::<AppState>)
                .put(ipma_resource::update_device::<AppState>)
                .delete(ipma_resource::delete_device::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/nics",
            get(ipma_resource::get_device_nics::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/ips",
            get(ipma_resource::get_device_ips::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/ips",
            post(ipma_resource::create_device_ip::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/auto-assign-ip",
            post(ipma_resource::auto_assign_device_ip::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/test-snmp",
            post(ipma_resource::test_snmp_connection_by_id::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/macs",
            get(ipma_resource::get_device_macs_from_db::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/macs/sync",
            post(ipma_resource::get_device_mac_table::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/lldp-neighbors",
            get(ipma_resource::get_device_lldp_neighbors::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/lldp/sync",
            post(ipma_resource::sync_lldp_from_snmp::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/snmp-info",
            get(ipma_resource::get_device_info_snmp::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/snmp-ports",
            get(ipma_resource::get_device_ports_snmp::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/interfaces",
            get(ipma_resource::get_device_interfaces::<AppState>)
                .post(ipma_resource::create_device_interface::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/interfaces/sync-snmp",
            post(ipma_resource::sync_ports_from_snmp::<AppState>),
        )
        .route(
            "/api/resources/devices/{id}/network-config",
            put(ipma_resource::sync_device_network_config::<AppState>),
        )
        .route(
            "/api/resources/devices/interfaces/{interface_id}",
            get(ipma_resource::get_device_interface::<AppState>)
                .put(ipma_resource::update_device_interface::<AppState>)
                .delete(ipma_resource::delete_device_interface::<AppState>),
        )
        // 日志管理路由
        .route("/api/logs/operation", get(get_operation_logs))
        .route("/api/logs/login", get(get_login_logs))
        // 通知管理路由
        .route("/api/notifications", get(get_notifications))
        .route("/api/notifications/{id}/read", put(mark_notification_read))
        .route(
            "/api/notifications/mark-all-read",
            put(mark_all_notifications_read),
        )
        // 系统管理路由
        // 系统信息
        .route("/api/system/info", get(get_system_info))
        // 服务状态
        .route("/api/system/service-status", get(get_service_status))
        .route("/api/system/register-service", post(register_service))
        // 仪表盘统计
        .route("/api/system/dashboard-stats", get(get_dashboard_stats))
        // 重启应用系统
        .route("/api/system/restart-application", post(restart_application))
        // 关闭初始化模式
        .route("/api/system/disable-init", post(disable_init_mode))
        // SMTP配置
        .route(
            "/api/system/smtp/config",
            get(get_smtp_config).put(update_smtp_config),
        )
        .route("/api/system/smtp/test", post(test_smtp))
        .route("/api/system/smtp/send", post(send_system_email))
        // LDAP 配置
        .route(
            "/api/system/ldap/config",
            get(ipma_auth::ldap::get_ldap_config::<AppState>)
                .put(ipma_auth::ldap::update_ldap_config::<AppState>),
        )
        .route(
            "/api/system/ldap/test",
            post(ipma_auth::ldap::test_ldap_connection::<AppState>),
        )
        // SSO（OIDC）配置
        .route(
            "/api/system/sso/config",
            get(ipma_auth::sso::get_sso_config::<AppState>)
                .put(ipma_auth::sso::update_sso_config::<AppState>),
        )
        .route(
            "/api/system/sso/test",
            post(ipma_auth::sso::test_sso_connection::<AppState>),
        )
        // 配置管理
        .route(
            "/api/system/config",
            get(get_system_config).put(update_system_config),
        )
        .route("/api/system/config/backup", get(backup_config))
        .route("/api/system/config/restore", post(restore_config))
        // 证书管理（生成 /etc/ssl/ipma-certs，导入 /etc/ssl/ipma-import-certs，
        // 站点根 CA /etc/ssl/ipma-ca，导入 CA 池 /etc/ssl/ipma-import-cas；
        // CA 的公开下载走 /api/certificate/ca/*。证书仅程序自用，无下载端点）
        .route("/api/system/certificate/list", get(certificate::list))
        .route(
            "/api/system/certificate/generate",
            post(certificate::generate),
        )
        .route("/api/system/certificate/import", post(certificate::import))
        .route(
            "/api/system/certificate/ca/generate",
            post(certificate::ca_generate),
        )
        .route(
            "/api/system/certificate/ca/import",
            post(certificate::ca_import),
        )
        .route(
            "/api/system/certificate/{kind}/{file_stem}",
            delete(certificate::delete),
        )
        // 语言设置
        .route("/api/system/languages", get(get_supported_languages))
        .route("/api/system/language", put(update_language_setting))
        // 页面超时配置
        .route(
            "/api/system/page-timeout",
            get(get_page_timeout_config).put(update_page_timeout_config),
        )
        // 会话超时配置
        .route(
            "/api/system/session-timeout",
            get(get_session_timeout_config).put(update_session_timeout_config),
        )
        // 通知设置
        .route(
            "/api/system/notification/settings",
            get(get_notification_settings).put(update_notification_settings),
        )
        // 等保密码策略（安全管理员管辖）
        .route(
            "/api/system/password-policy",
            get(get_password_policy).put(update_password_policy),
        )
        // 导入导出功能
        .route(
            "/api/system/import-export/import/csv",
            post(data_import_csv).layer(axum::extract::DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route("/api/system/import-export/export/csv", get(data_export_csv))
        .route(
            "/api/system/import-export/export/database",
            get(data_export_database),
        )
        .route(
            "/api/system/import-export/template",
            get(data_download_template),
        )
        // 日志清理功能
        .route("/api/system/logs/stats", get(data_get_logs_stats))
        .route("/api/system/logs/clear", post(data_clear_logs))
        // 日志外发（syslog，安全管理员管辖）
        .route(
            "/api/system/logs/forwarding",
            get(crate::log::forwarding::get_forwarding)
                .put(crate::log::forwarding::update_forwarding),
        )
        .route(
            "/api/system/logs/forwarding/test",
            post(crate::log::forwarding::test_forwarding),
        )
        // 定时任务管理
        .route(
            "/api/system/scheduled-tasks",
            get(get_scheduled_tasks).post(create_scheduled_task),
        )
        .route(
            "/api/system/scheduled-tasks/{id}",
            get(get_scheduled_task)
                .put(update_scheduled_task)
                .delete(delete_scheduled_task),
        )
        .route(
            "/api/system/scheduled-tasks/{id}/toggle",
            post(toggle_scheduled_task),
        )
        .route(
            "/api/system/scheduled-tasks/{id}/run",
            post(run_scheduled_task_now),
        )
        .route("/api/system/scheduled-tasks/logs", get(get_task_logs))
        // Fail2ban 安全管理（应用层）
        .route(
            "/api/system/fail2ban/app/status",
            get(ipma_auth::app_fail2ban::get_app_fail2ban_status::<AppState>),
        )
        .route(
            "/api/system/fail2ban/app/config",
            put(ipma_auth::app_fail2ban::update_app_fail2ban_config::<AppState>),
        )
        .route(
            "/api/system/fail2ban/app/ban",
            post(ipma_auth::app_fail2ban::app_ban_ip::<AppState>),
        )
        .route(
            "/api/system/fail2ban/app/unban",
            post(ipma_auth::app_fail2ban::app_unban_ip::<AppState>),
        )
        // admin_guard_middleware 先注册（位于 auth_middleware 之内）：
        // 请求先经 auth_middleware 校验令牌注入 claims，再由守卫做角色判定
        .route_layer(middleware::from_fn(admin_guard_middleware))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            ipma_auth::login::auth_middleware::<AppState>,
        ));

    Router::new()
        .merge(public_auth_routes)
        .merge(public_ca_routes)
        .merge(me_routes)
        .merge(health_routes)
        .merge(protected_api_routes)
}
