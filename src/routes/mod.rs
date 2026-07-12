pub mod static_files;

use crate::app_state::AppState;
use crate::auth::login::{
    auth_middleware, disable_two_factor, enable_two_factor, forgot_password, get_current_user,
    init_two_factor, login, login_with_email_code, login_with_two_factor, logout, refresh_token,
    reset_password, send_login_code, send_two_factor_code,
};
use crate::auth::user::{create_user, delete_user, get_user, get_users, update_user};
use crate::error::AppError;
use crate::log::notification::{
    get_notifications, mark_all_notifications_read, mark_notification_read,
};
use crate::log::{get_login_logs, get_operation_logs};
use crate::resource::device::{
    create_device_port, delete_device_port, get_all_device_ports, get_device_info_snmp,
    get_device_lldp_neighbors, get_device_mac_table, get_device_macs_from_db, get_device_port,
    get_device_ports, get_device_ports_snmp, sync_lldp_from_snmp, sync_ports_from_snmp,
    test_snmp_connection, test_snmp_connection_by_id, update_device_port,
};
use crate::resource::{
    auto_assign_device_ip, auto_assign_ip, batch_create_ip_managers, connect_device,
    create_cabinet, create_cabinet_position, create_device, create_device_ip, create_net_outlet,
    create_network, create_network_region, create_org_template, create_organization, create_room,
    create_topology_connection, create_workstation, delete_cabinet, delete_cabinet_position,
    delete_device, delete_device_template, delete_layout, delete_net_outlet, delete_network,
    delete_network_region, delete_org_template, delete_organization, delete_positions_layout,
    delete_room, delete_topology_connection, delete_topology_node, delete_workstation,
    disconnect_device, get_allowed_child_types, get_available_ips, get_cabinet,
    get_cabinet_networks, get_cabinet_position, get_cabinet_position_ips, get_cabinets,
    get_cabinets_by_network_region, get_children, get_device, get_device_ips, get_device_template,
    get_device_templates, get_devices, get_ip_managers, get_layout, get_net_outlet,
    get_net_outlets, get_network, get_network_region, get_network_regions, get_networks,
    get_org_rooms, get_org_template, get_org_templates, get_organization, get_organization_tree,
    get_organizations, get_positions, get_positions_layout, get_room,
    get_room_cabinets_with_positions, get_room_networks, get_rooms, get_topology_connections,
    get_topology_nodes, get_workstation, get_workstation_ips, get_workstations, link_peer,
    pull_ip_managers, save_layout, save_topology_nodes, trigger_auto_discover, unlink_peer,
    update_cabinet, update_cabinet_position, update_device, update_net_outlet, update_network,
    update_network_region, update_org_template, update_organization, update_room,
    update_workstation,
};
use crate::system::config::{
    backup_config, disable_init_mode, download_certificate, generate_certificate,
    get_certificate_status, get_dashboard_stats, get_notification_settings,
    get_page_timeout_config, get_service_status, get_session_timeout_config, get_smtp_config,
    get_supported_languages, get_system_config, get_system_info, import_certificate,
    register_service, restart_application, restart_os, restore_config, send_system_email,
    test_smtp_connection as test_smtp, update_language_setting, update_notification_settings,
    update_page_timeout_config, update_session_timeout_config, update_smtp_config,
    update_system_config,
};
use crate::system::scheduled_task::{
    create_scheduled_task, delete_scheduled_task, get_scheduled_task, get_scheduled_tasks,
    get_task_logs, run_scheduled_task_now, toggle_scheduled_task, update_scheduled_task,
};
use actix_web::{HttpResponse, middleware, web};

async fn data_export_csv(
    _admin: crate::auth::extractor::AdminUser,
    state: web::Data<AppState>,
    type_param: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    ipma_data_manager::export_csv(state.as_ref().clone(), type_param)
        .await
        .map_err(AppError::from)
}

async fn data_import_csv(
    _admin: crate::auth::extractor::AdminUser,
    state: web::Data<AppState>,
    payload: actix_multipart::Multipart,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    ipma_data_manager::import_csv(state.as_ref().clone(), payload, query)
        .await
        .map_err(AppError::from)
}

async fn data_download_template(
    type_param: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    ipma_data_manager::download_template(type_param)
        .await
        .map_err(AppError::from)
}

async fn data_export_database(
    _admin: crate::auth::extractor::AdminUser,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    ipma_data_manager::export_database(state.as_ref().clone())
        .await
        .map_err(AppError::from)
}

async fn data_clear_logs(
    _admin: crate::auth::extractor::AdminUser,
    state: web::Data<AppState>,
    req: web::Json<ipma_data_manager::ClearLogsRequest>,
) -> Result<HttpResponse, AppError> {
    ipma_data_manager::clear_logs(state.as_ref().clone(), req)
        .await
        .map_err(AppError::from)
}

async fn data_get_logs_stats(
    _admin: crate::auth::extractor::AdminUser,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    ipma_data_manager::get_logs_stats(state.as_ref().clone())
        .await
        .map_err(AppError::from)
}

// 初始化相关路由将根据配置动态添加
// 这里只定义基础路由
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg
        // 认证相关路由（不需要认证）
        .service(
            web::scope("/api/auth")
                .route("/login", web::post().to(login))
                .route("/login/email", web::post().to(login_with_email_code))
                .route("/login/send-code", web::post().to(send_login_code))
                .route("/login/two-factor", web::post().to(login_with_two_factor))
                .route("/login/send-2fa-code", web::post().to(send_two_factor_code))
                .route("/logout", web::post().to(logout))
                .route("/refresh", web::post().to(refresh_token))
                .route("/forgot-password", web::post().to(forgot_password))
                .route("/reset-password", web::post().to(reset_password))
                .service(
                    web::resource("/me")
                        .wrap(middleware::from_fn(auth_middleware))
                        .route(web::get().to(get_current_user)),
                ),
        )
        // 健康检查（不需要认证）
        .service(
            web::scope("/health").route(
                "",
                web::get()
                    .to(|| async { HttpResponse::Ok().json(serde_json::json!({"status": "ok"})) }),
            ),
        )
        // 需要认证的API路由
        .service(
            web::scope("/api")
                .wrap(middleware::from_fn(auth_middleware))
                // 用户管理路由
                .service(
                    web::scope("/users")
                        .route("", web::get().to(get_users))
                        .route("", web::post().to(create_user))
                        .route("/{id}", web::get().to(get_user))
                        .route("/{id}", web::put().to(update_user))
                        .route("/{id}", web::delete().to(delete_user)),
                )
                // 2FA管理路由
                .service(
                    web::scope("/two-factor")
                        .route("/init", web::post().to(init_two_factor))
                        .route("/enable", web::post().to(enable_two_factor))
                        .route("/disable", web::post().to(disable_two_factor)),
                )
                // 资源管理路由
                .service(
                    web::scope("/resources")
                        // 网络管理
                        .service(
                            web::scope("/networks")
                                .route("", web::get().to(get_networks))
                                .route("", web::post().to(create_network))
                                .route("/{id}", web::get().to(get_network))
                                .route("/{id}", web::put().to(update_network))
                                .route("/{id}", web::delete().to(delete_network)),
                        )
                        // 网络区域管理
                        .service(
                            web::scope("/network-regions")
                                .route("", web::get().to(get_network_regions))
                                .route("", web::post().to(create_network_region))
                                .route("/{id}", web::get().to(get_network_region))
                                .route("/{id}", web::put().to(update_network_region))
                                .route("/{id}", web::delete().to(delete_network_region))
                                .route(
                                    "/{id}/cabinets",
                                    web::get().to(get_cabinets_by_network_region),
                                ),
                        )
                        // 房间管理
                        .service(
                            web::scope("/rooms")
                                .route("", web::get().to(get_rooms))
                                .route("", web::post().to(create_room))
                                .route("/{id}", web::get().to(get_room))
                                .route("/{id}", web::put().to(update_room))
                                .route("/{id}", web::delete().to(delete_room))
                                .route("/{id}/networks", web::get().to(get_room_networks)),
                        )
                        // 机柜管理
                        .service(
                            web::scope("/cabinets")
                                .route("", web::get().to(get_cabinets))
                                .route("", web::post().to(create_cabinet))
                                .route("/{id}", web::get().to(get_cabinet))
                                .route("/{id}", web::put().to(update_cabinet))
                                .route("/{id}", web::delete().to(delete_cabinet))
                                .route("/{id}/networks", web::get().to(get_cabinet_networks)),
                        )
                        // 工位管理
                        .service(
                            web::scope("/workstations")
                                .route("", web::get().to(get_workstations))
                                .route("", web::post().to(create_workstation))
                                .route("/{id}", web::get().to(get_workstation))
                                .route("/{id}", web::put().to(update_workstation))
                                .route("/{id}", web::delete().to(delete_workstation)),
                        )
                        // 机位管理
                        .service(
                            web::scope("/positions")
                                .route("", web::get().to(get_positions))
                                .route("", web::post().to(create_cabinet_position))
                                .route("/{id}", web::get().to(get_cabinet_position))
                                .route("/{id}", web::put().to(update_cabinet_position))
                                .route("/{id}", web::delete().to(delete_cabinet_position)),
                        )
                        // IP管理
                        .service(
                            web::scope("/ip")
                                .route("", web::get().to(get_ip_managers))
                                .route("/pull", web::post().to(pull_ip_managers))
                                .route("/available/{network_id}", web::get().to(get_available_ips))
                                .route("/auto-assign", web::post().to(auto_assign_ip))
                                .route("/batch", web::post().to(batch_create_ip_managers))
                                .route("/workstation/{id}", web::get().to(get_workstation_ips))
                                .route(
                                    "/cabinet-position/{id}",
                                    web::get().to(get_cabinet_position_ips),
                                ),
                        )
                        // 布局管理
                        .service(
                            web::scope("/layouts")
                                .route("", web::post().to(save_layout))
                                .route("/workstation/{room_id}", web::get().to(get_layout))
                                .route("/workstation/{room_id}", web::delete().to(delete_layout))
                                .route("/positions/{room_id}", web::get().to(get_positions_layout))
                                .route(
                                    "/positions/{room_id}",
                                    web::delete().to(delete_positions_layout),
                                )
                                .route(
                                    "/room-cabinets/{room_id}",
                                    web::get().to(get_room_cabinets_with_positions),
                                ),
                        )
                        // 拓扑可视化
                        .service(
                            web::scope("/topology")
                                .route("/nodes", web::get().to(get_topology_nodes))
                                .route("/nodes", web::post().to(save_topology_nodes))
                                .route("/nodes/{device_id}", web::delete().to(delete_topology_node))
                                .route("/connections", web::get().to(get_topology_connections))
                                .route("/connections", web::post().to(create_topology_connection))
                                .route(
                                    "/connections/{id}",
                                    web::delete().to(delete_topology_connection),
                                )
                                .route("/auto-discover", web::post().to(trigger_auto_discover)),
                        )
                        // 组织管理
                        .service(
                            web::scope("/organizations")
                                .route("", web::get().to(get_organizations))
                                .route("", web::post().to(create_organization))
                                .route("/tree", web::get().to(get_organization_tree))
                                .route("/{id}", web::get().to(get_organization))
                                .route("/{id}", web::put().to(update_organization))
                                .route("/{id}", web::delete().to(delete_organization))
                                .route("/{id}/children", web::get().to(get_children))
                                .route(
                                    "/{id}/allowed-child-types",
                                    web::get().to(get_allowed_child_types),
                                )
                                .route("/{id}/rooms", web::get().to(get_org_rooms)),
                        )
                        // 组织模板管理
                        .service(
                            web::scope("/org-templates")
                                .route("", web::get().to(get_org_templates))
                                .route("", web::post().to(create_org_template))
                                .route("/{id}", web::get().to(get_org_template))
                                .route("/{id}", web::put().to(update_org_template))
                                .route("/{id}", web::delete().to(delete_org_template)),
                        )
                        // 网络端口管理
                        .service(
                            web::scope("/net-outlets")
                                .route("", web::get().to(get_net_outlets))
                                .route("", web::post().to(create_net_outlet))
                                .route("/{id}", web::get().to(get_net_outlet))
                                .route("/{id}", web::put().to(update_net_outlet))
                                .route("/{id}", web::delete().to(delete_net_outlet))
                                .route("/{id}/link-peer", web::post().to(link_peer))
                                .route("/{id}/unlink-peer", web::post().to(unlink_peer)),
                        )
                        // 设备模板管理
                        .service(
                            web::scope("/device-templates")
                                .route("", web::get().to(get_device_templates))
                                .route("/{id}", web::get().to(get_device_template))
                                .route("/{id}", web::delete().to(delete_device_template)),
                        )
                        // 设备管理（含交换机端口/MAC/LLDP/SNMP 功能）
                        .service(
                            web::scope("/devices")
                                .route("", web::get().to(get_devices))
                                .route("", web::post().to(create_device))
                                .route("/ports", web::get().to(get_all_device_ports))
                                .route("/test-snmp", web::post().to(test_snmp_connection))
                                .route("/{id}", web::get().to(get_device))
                                .route("/{id}", web::put().to(update_device))
                                .route("/{id}", web::delete().to(delete_device))
                                .route("/{id}/ips", web::get().to(get_device_ips))
                                .route("/{id}/ips", web::post().to(create_device_ip))
                                .route(
                                    "/{id}/auto-assign-ip",
                                    web::post().to(auto_assign_device_ip),
                                )
                                .route("/{id}/connect", web::post().to(connect_device))
                                .route("/{id}/disconnect", web::post().to(disconnect_device))
                                .route("/{id}/ports", web::get().to(get_device_ports))
                                .route("/{id}/ports", web::post().to(create_device_port))
                                .route(
                                    "/{id}/ports/sync-snmp",
                                    web::post().to(sync_ports_from_snmp),
                                )
                                .route(
                                    "/{id}/test-snmp",
                                    web::post().to(test_snmp_connection_by_id),
                                )
                                .route("/{id}/macs", web::get().to(get_device_macs_from_db))
                                .route("/{id}/macs/sync", web::post().to(get_device_mac_table))
                                .route(
                                    "/{id}/lldp-neighbors",
                                    web::get().to(get_device_lldp_neighbors),
                                )
                                .route("/{id}/lldp/sync", web::post().to(sync_lldp_from_snmp))
                                .route("/{id}/snmp-info", web::get().to(get_device_info_snmp))
                                .route("/{id}/snmp-ports", web::get().to(get_device_ports_snmp))
                                .route("/ports/{port_id}", web::get().to(get_device_port))
                                .route("/ports/{port_id}", web::put().to(update_device_port))
                                .route("/ports/{port_id}", web::delete().to(delete_device_port)),
                        ),
                )
                // 日志管理路由
                .service(
                    web::scope("/logs")
                        .route("/operation", web::get().to(get_operation_logs))
                        .route("/login", web::get().to(get_login_logs)),
                )
                // 通知管理路由
                .service(
                    web::scope("/notifications")
                        .route("", web::get().to(get_notifications))
                        .route("/{id}/read", web::put().to(mark_notification_read))
                        .route("/mark-all-read", web::put().to(mark_all_notifications_read)),
                )
                // 系统管理路由
                .service(
                    web::scope("/system")
                        // 系统信息
                        .route("/info", web::get().to(get_system_info))
                        // 服务状态
                        .route("/service-status", web::get().to(get_service_status))
                        .route("/register-service", web::post().to(register_service))
                        // 仪表盘统计
                        .route("/dashboard-stats", web::get().to(get_dashboard_stats))
                        // 重启应用系统
                        .route("/restart-application", web::post().to(restart_application))
                        // 重启操作系统
                        .route("/restart-os", web::post().to(restart_os))
                        // 关闭初始化模式
                        .route("/disable-init", web::post().to(disable_init_mode))
                        // SMTP配置
                        .route("/smtp/config", web::get().to(get_smtp_config))
                        .route("/smtp/config", web::put().to(update_smtp_config))
                        .route("/smtp/test", web::post().to(test_smtp))
                        .route("/smtp/send", web::post().to(send_system_email))
                        // 证书管理
                        .route("/certificate/status", web::get().to(get_certificate_status))
                        .route(
                            "/certificate/generate",
                            web::post().to(generate_certificate),
                        )
                        .route("/certificate/import", web::post().to(import_certificate))
                        .route("/certificate/download", web::get().to(download_certificate))
                        // 配置管理
                        .route("/config", web::get().to(get_system_config))
                        .route("/config", web::put().to(update_system_config))
                        .route("/config/backup", web::get().to(backup_config))
                        .route("/config/restore", web::post().to(restore_config))
                        // 语言设置
                        .route("/languages", web::get().to(get_supported_languages))
                        .route("/language", web::put().to(update_language_setting))
                        // 页面超时配置
                        .route("/page-timeout", web::get().to(get_page_timeout_config))
                        .route("/page-timeout", web::put().to(update_page_timeout_config))
                        // 会话超时配置
                        .route(
                            "/session-timeout",
                            web::get().to(get_session_timeout_config),
                        )
                        .route(
                            "/session-timeout",
                            web::put().to(update_session_timeout_config),
                        )
                        // 通知设置
                        .route(
                            "/notification/settings",
                            web::get().to(get_notification_settings),
                        )
                        .route(
                            "/notification/settings",
                            web::put().to(update_notification_settings),
                        )
                        // 导入导出功能
                        .service(
                            web::scope("/import-export")
                                .route("/import/csv", web::post().to(data_import_csv))
                                .route("/export/csv", web::get().to(data_export_csv))
                                .route("/export/database", web::get().to(data_export_database))
                                .route("/template", web::get().to(data_download_template)),
                        )
                        // 日志清理功能
                        .route("/logs/stats", web::get().to(data_get_logs_stats))
                        .route("/logs/clear", web::post().to(data_clear_logs))
                        // 定时任务管理
                        .service(
                            web::scope("/scheduled-tasks")
                                .route("", web::get().to(get_scheduled_tasks))
                                .route("", web::post().to(create_scheduled_task))
                                .route("/{id}", web::get().to(get_scheduled_task))
                                .route("/{id}", web::put().to(update_scheduled_task))
                                .route("/{id}", web::delete().to(delete_scheduled_task))
                                .route("/{id}/toggle", web::post().to(toggle_scheduled_task))
                                .route("/{id}/run", web::post().to(run_scheduled_task_now))
                                .route("/logs", web::get().to(get_task_logs)),
                        ),
                ),
        );

    // 注意：初始化相关路由将在main.rs中根据配置动态添加
}
