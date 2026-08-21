//! 路由注册与数据管理转发 handler。
//!
//! 权限守卫约定：仅鉴权无入参使用的 handler 使用 `_admin: AdminUser`
//! 形式的提取器参数（下划线前缀表示“仅用其副作用”），为规范允许的
//! 唯一下划线例外，见 docs/code-style.md。

pub mod static_files;

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Multipart, Query, State};
use axum::middleware;
use axum::response::Response;
use axum::routing::{delete, get, post, put};

use crate::app_state::AppState;
use crate::auth::ldap::{
    get_ldap_config, login_with_ldap, test_ldap_connection, update_ldap_config,
};
use crate::auth::login::{
    auth_middleware, disable_two_factor, enable_two_factor, forgot_password, get_current_user,
    init_two_factor, login, login_with_email_code, login_with_two_factor, logout, refresh_token,
    reset_password, send_login_code, send_two_factor_code,
};
use crate::auth::sso::{
    get_auth_methods, get_sso_config, sso_callback, sso_login, test_sso_connection,
    update_sso_config,
};
use crate::auth::user::{create_user, delete_user, get_user, get_users, update_user};
use crate::error::AppError;
use crate::log::notification::{
    get_notifications, mark_all_notifications_read, mark_notification_read,
};
use crate::log::{get_login_logs, get_operation_logs};
use crate::resource::{
    auto_assign_device_ip, auto_assign_ip, batch_create_ip_managers, create_cabinet,
    create_cabinet_position, create_cable_link, create_device, create_device_interface,
    create_device_ip, create_device_port, create_net_outlet, create_network, create_network_region,
    create_org_template, create_organization, create_room, create_topology_connection,
    create_workstation, delete_cabinet, delete_cabinet_position, delete_cable_link, delete_device,
    delete_device_interface, delete_device_port, delete_device_template, delete_layout,
    delete_net_outlet, delete_network, delete_network_region, delete_org_template,
    delete_organization, delete_positions_layout, delete_room, delete_topology_connection,
    delete_topology_node, delete_workstation, get_all_device_interfaces, get_all_device_ports,
    get_allowed_child_types, get_available_ips, get_available_org_types, get_cabinet,
    get_cabinet_networks, get_cabinet_position, get_cabinets, get_cabinets_by_network_region,
    get_cable_link, get_cable_links, get_cable_path, get_children, get_device,
    get_device_info_snmp, get_device_interface, get_device_interfaces, get_device_ips,
    get_device_lldp_neighbors, get_device_mac_table, get_device_macs_from_db, get_device_nics,
    get_device_port, get_device_ports, get_device_ports_snmp, get_device_template,
    get_device_templates, get_devices, get_ip_managers, get_layout, get_net_outlet,
    get_net_outlets, get_network, get_network_region, get_network_regions, get_networks,
    get_org_rooms, get_org_template, get_org_templates, get_organization, get_organization_tree,
    get_organizations, get_patch_panels, get_positions, get_positions_layout, get_room,
    get_room_cabinets_with_positions, get_room_networks, get_rooms, get_topology_connections,
    get_topology_nodes, get_workstation, get_workstations, pull_ip_managers, save_layout,
    save_topology_nodes, sync_cabinet_patch_panels, sync_cabinet_positions,
    sync_device_network_config, sync_lldp_from_snmp, sync_ports_from_snmp, sync_room_children,
    sync_room_net_outlets, test_snmp_connection, test_snmp_connection_by_id, trigger_auto_discover,
    update_cabinet, update_cabinet_position, update_cable_link, update_device,
    update_device_interface, update_device_port, update_device_template, update_net_outlet,
    update_network, update_network_region, update_org_template, update_organization, update_room,
    update_workstation,
};
use crate::routes::static_files::AppJson;
use crate::system::app_fail2ban::{
    app_ban_ip, app_unban_ip, get_app_fail2ban_status, update_app_fail2ban_config,
};
use crate::system::certificate;
use crate::system::config::{
    backup_config, disable_init_mode, get_dashboard_stats, get_notification_settings,
    get_page_timeout_config, get_service_status, get_session_timeout_config, get_smtp_config,
    get_supported_languages, get_system_config, get_system_info, register_service,
    restart_application, restore_config, send_system_email, test_smtp_connection as test_smtp,
    update_language_setting, update_notification_settings, update_page_timeout_config,
    update_session_timeout_config, update_smtp_config, update_system_config,
};
use crate::system::scheduled_task::{
    create_scheduled_task, delete_scheduled_task, get_scheduled_task, get_scheduled_tasks,
    get_task_logs, run_scheduled_task_now, toggle_scheduled_task, update_scheduled_task,
};

async fn data_export_csv(
    _admin: crate::auth::extractor::AdminUser,
    State(state): State<Arc<AppState>>,
    type_param: Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    ipma_data_manager::export_csv(state.as_ref().clone(), type_param)
        .await
        .map_err(AppError::from)
}

async fn data_import_csv(
    _admin: crate::auth::extractor::AdminUser,
    State(state): State<Arc<AppState>>,
    payload: Multipart,
) -> Result<Response, AppError> {
    ipma_data_manager::import_csv(state.as_ref().clone(), payload)
        .await
        .map_err(AppError::from)
}

async fn data_download_template(
    type_param: Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    ipma_data_manager::download_template(type_param)
        .await
        .map_err(AppError::from)
}

async fn data_export_database(
    _admin: crate::auth::extractor::AdminUser,
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    ipma_data_manager::export_database(state.as_ref().clone())
        .await
        .map_err(AppError::from)
}

async fn data_clear_logs(
    _admin: crate::auth::extractor::AdminUser,
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<ipma_data_manager::ClearLogsRequest>,
) -> Result<Response, AppError> {
    ipma_data_manager::clear_logs(state.as_ref().clone(), req)
        .await
        .map_err(AppError::from)
}

async fn data_get_logs_stats(
    _admin: crate::auth::extractor::AdminUser,
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    ipma_data_manager::get_logs_stats(state.as_ref().clone())
        .await
        .map_err(AppError::from)
}

async fn health_check() -> Response {
    crate::error::ok_json(serde_json::json!({"status": "ok"}), "server.common.success")
}

/// 公开的初始化状态查询（不需要认证）
/// 供前端登录页判断：当 init_enabled=true 时跳转到初始化页 /init_index.html
/// 仅返回 init_enabled 一个布尔字段，避免泄露系统是否已初始化等额外信息。
pub async fn get_init_status(State(state): State<Arc<AppState>>) -> Response {
    crate::error::ok_json(
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
        .route("/api/auth/login", post(login))
        .route("/api/auth/login/email", post(login_with_email_code))
        .route("/api/auth/login/send-code", post(send_login_code))
        .route("/api/auth/login/two-factor", post(login_with_two_factor))
        .route("/api/auth/login/send-2fa-code", post(send_two_factor_code))
        .route("/api/auth/login/ldap", post(login_with_ldap))
        .route("/api/auth/sso/login", get(sso_login))
        .route("/api/auth/sso/callback", get(sso_callback))
        .route("/api/auth/methods", get(get_auth_methods))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/refresh", post(refresh_token))
        .route("/api/auth/forgot-password", post(forgot_password))
        .route("/api/auth/reset-password", post(reset_password));

    // /me 路由（单独应用认证中间件）
    let me_routes = Router::new()
        .route("/api/auth/me", get(get_current_user))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // 健康检查（不需要认证）
    let health_routes = Router::new().route("/health", get(health_check));

    // 需要认证的 API 路由
    let protected_api_routes = Router::new()
        // 用户管理路由
        .route("/api/users", get(get_users).post(create_user))
        .route(
            "/api/users/{id}",
            get(get_user).put(update_user).delete(delete_user),
        )
        // 2FA管理路由
        .route("/api/two-factor/init", post(init_two_factor))
        .route("/api/two-factor/enable", post(enable_two_factor))
        .route("/api/two-factor/disable", post(disable_two_factor))
        // 资源管理路由
        // 网络管理
        .route(
            "/api/resources/networks",
            get(get_networks).post(create_network),
        )
        .route(
            "/api/resources/networks/{id}",
            get(get_network).put(update_network).delete(delete_network),
        )
        // 网络区域管理
        .route(
            "/api/resources/network-regions",
            get(get_network_regions).post(create_network_region),
        )
        .route(
            "/api/resources/network-regions/{id}",
            get(get_network_region)
                .put(update_network_region)
                .delete(delete_network_region),
        )
        .route(
            "/api/resources/network-regions/{id}/cabinets",
            get(get_cabinets_by_network_region),
        )
        // 房间管理
        .route("/api/resources/rooms", get(get_rooms).post(create_room))
        .route(
            "/api/resources/rooms/{id}",
            get(get_room).put(update_room).delete(delete_room),
        )
        .route("/api/resources/rooms/{id}/networks", get(get_room_networks))
        .route(
            "/api/resources/rooms/{id}/children",
            put(sync_room_children),
        )
        .route(
            "/api/resources/rooms/{id}/net-outlets",
            put(sync_room_net_outlets),
        )
        // 机柜管理
        .route(
            "/api/resources/cabinets",
            get(get_cabinets).post(create_cabinet),
        )
        .route(
            "/api/resources/cabinets/{id}",
            get(get_cabinet).put(update_cabinet).delete(delete_cabinet),
        )
        .route(
            "/api/resources/cabinets/{id}/networks",
            get(get_cabinet_networks),
        )
        .route(
            "/api/resources/cabinets/{id}/positions",
            put(sync_cabinet_positions),
        )
        .route(
            "/api/resources/cabinets/{id}/patch-panels",
            put(sync_cabinet_patch_panels),
        )
        // 工位管理
        .route(
            "/api/resources/workstations",
            get(get_workstations).post(create_workstation),
        )
        .route(
            "/api/resources/workstations/{id}",
            get(get_workstation)
                .put(update_workstation)
                .delete(delete_workstation),
        )
        // 机位管理
        .route(
            "/api/resources/positions",
            get(get_positions).post(create_cabinet_position),
        )
        .route(
            "/api/resources/positions/{id}",
            get(get_cabinet_position)
                .put(update_cabinet_position)
                .delete(delete_cabinet_position),
        )
        // IP查询
        .route("/api/resources/ip", get(get_ip_managers))
        .route("/api/resources/ip/pull", post(pull_ip_managers))
        .route(
            "/api/resources/ip/available/{network_id}",
            get(get_available_ips),
        )
        .route("/api/resources/ip/auto-assign", post(auto_assign_ip))
        .route("/api/resources/ip/batch", post(batch_create_ip_managers))
        // 布局管理
        .route("/api/resources/layouts", post(save_layout))
        .route(
            "/api/resources/layouts/workstation/{room_id}",
            get(get_layout).delete(delete_layout),
        )
        .route(
            "/api/resources/layouts/positions/{room_id}",
            get(get_positions_layout).delete(delete_positions_layout),
        )
        .route(
            "/api/resources/layouts/room-cabinets/{room_id}",
            get(get_room_cabinets_with_positions),
        )
        // 拓扑可视化
        .route(
            "/api/resources/topology/nodes",
            get(get_topology_nodes).post(save_topology_nodes),
        )
        .route(
            "/api/resources/topology/nodes/{device_id}",
            delete(delete_topology_node),
        )
        .route(
            "/api/resources/topology/connections",
            get(get_topology_connections).post(create_topology_connection),
        )
        .route(
            "/api/resources/topology/connections/{id}",
            delete(delete_topology_connection),
        )
        .route(
            "/api/resources/topology/auto-discover",
            post(trigger_auto_discover),
        )
        // 组织管理
        .route(
            "/api/resources/organizations",
            get(get_organizations).post(create_organization),
        )
        .route(
            "/api/resources/organizations/tree",
            get(get_organization_tree),
        )
        .route(
            "/api/resources/organizations/{id}",
            get(get_organization)
                .put(update_organization)
                .delete(delete_organization),
        )
        .route(
            "/api/resources/organizations/{id}/children",
            get(get_children),
        )
        .route(
            "/api/resources/organizations/{id}/allowed-child-types",
            get(get_allowed_child_types),
        )
        .route(
            "/api/resources/organizations/{id}/rooms",
            get(get_org_rooms),
        )
        // 组织模板管理
        .route(
            "/api/resources/org-templates",
            get(get_org_templates).post(create_org_template),
        )
        .route(
            "/api/resources/org-templates/available-types",
            get(get_available_org_types),
        )
        .route(
            "/api/resources/org-templates/{id}",
            get(get_org_template)
                .put(update_org_template)
                .delete(delete_org_template),
        )
        // 信息点管理
        .route(
            "/api/resources/net-outlets",
            get(get_net_outlets).post(create_net_outlet),
        )
        .route(
            "/api/resources/net-outlets/{id}",
            get(get_net_outlet)
                .put(update_net_outlet)
                .delete(delete_net_outlet),
        )
        // 配线架管理（隶属机柜，列表供线路端点选择）
        .route("/api/resources/patch-panels", get(get_patch_panels))
        // 物理链路管理
        .route(
            "/api/resources/cable-links",
            get(get_cable_links).post(create_cable_link),
        )
        .route("/api/resources/cable-links/path", get(get_cable_path))
        .route(
            "/api/resources/cable-links/{id}",
            get(get_cable_link)
                .put(update_cable_link)
                .delete(delete_cable_link),
        )
        // 设备模板管理
        .route("/api/resources/device-templates", get(get_device_templates))
        .route(
            "/api/resources/device-templates/{id}",
            get(get_device_template)
                .put(update_device_template)
                .delete(delete_device_template),
        )
        // 设备管理（含交换机端口/设备接口/MAC/LLDP/SNMP 功能）
        .route(
            "/api/resources/devices",
            get(get_devices).post(create_device),
        )
        .route(
            "/api/resources/devices/device-ports",
            get(get_all_device_ports),
        )
        .route(
            "/api/resources/devices/interfaces",
            get(get_all_device_interfaces),
        )
        .route(
            "/api/resources/devices/test-snmp",
            post(test_snmp_connection),
        )
        .route(
            "/api/resources/devices/{id}",
            get(get_device).put(update_device).delete(delete_device),
        )
        .route("/api/resources/devices/{id}/nics", get(get_device_nics))
        .route("/api/resources/devices/{id}/ips", get(get_device_ips))
        .route("/api/resources/devices/{id}/ips", post(create_device_ip))
        .route(
            "/api/resources/devices/{id}/auto-assign-ip",
            post(auto_assign_device_ip),
        )
        .route(
            "/api/resources/devices/{id}/device-ports",
            get(get_device_ports).post(create_device_port),
        )
        .route(
            "/api/resources/devices/{id}/device-ports/sync-snmp",
            post(sync_ports_from_snmp),
        )
        .route(
            "/api/resources/devices/{id}/test-snmp",
            post(test_snmp_connection_by_id),
        )
        .route(
            "/api/resources/devices/{id}/macs",
            get(get_device_macs_from_db),
        )
        .route(
            "/api/resources/devices/{id}/macs/sync",
            post(get_device_mac_table),
        )
        .route(
            "/api/resources/devices/{id}/lldp-neighbors",
            get(get_device_lldp_neighbors),
        )
        .route(
            "/api/resources/devices/{id}/lldp/sync",
            post(sync_lldp_from_snmp),
        )
        .route(
            "/api/resources/devices/{id}/snmp-info",
            get(get_device_info_snmp),
        )
        .route(
            "/api/resources/devices/{id}/snmp-ports",
            get(get_device_ports_snmp),
        )
        .route(
            "/api/resources/devices/{id}/interfaces",
            get(get_device_interfaces).post(create_device_interface),
        )
        .route(
            "/api/resources/devices/{id}/network-config",
            put(sync_device_network_config),
        )
        .route(
            "/api/resources/devices/device-ports/{port_id}",
            get(get_device_port)
                .put(update_device_port)
                .delete(delete_device_port),
        )
        .route(
            "/api/resources/devices/interfaces/{interface_id}",
            get(get_device_interface)
                .put(update_device_interface)
                .delete(delete_device_interface),
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
            get(get_ldap_config).put(update_ldap_config),
        )
        .route("/api/system/ldap/test", post(test_ldap_connection))
        // SSO（OIDC）配置
        .route(
            "/api/system/sso/config",
            get(get_sso_config).put(update_sso_config),
        )
        .route("/api/system/sso/test", post(test_sso_connection))
        // 配置管理
        .route(
            "/api/system/config",
            get(get_system_config).put(update_system_config),
        )
        .route("/api/system/config/backup", get(backup_config))
        .route("/api/system/config/restore", post(restore_config))
        // 证书管理（生成 /etc/ssl/ipma-certs，导入 /etc/ssl/ipma-import-certs）
        .route("/api/system/certificate/list", get(certificate::list))
        .route(
            "/api/system/certificate/generate",
            post(certificate::generate),
        )
        .route("/api/system/certificate/import", post(certificate::import))
        .route(
            "/api/system/certificate/download/{kind}/{filename}",
            get(certificate::download),
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
            get(get_app_fail2ban_status),
        )
        .route(
            "/api/system/fail2ban/app/config",
            put(update_app_fail2ban_config),
        )
        .route("/api/system/fail2ban/app/ban", post(app_ban_ip))
        .route("/api/system/fail2ban/app/unban", post(app_unban_ip))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
        .merge(public_auth_routes)
        .merge(me_routes)
        .merge(health_routes)
        .merge(protected_api_routes)
}
