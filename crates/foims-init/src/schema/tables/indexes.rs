//! 跨表索引创建。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let indexes = [
        "CREATE INDEX IF NOT EXISTS idx_network_cidrs_region ON network_cidrs(network_region_id)",
        "CREATE INDEX IF NOT EXISTS idx_room_networks_room_id ON room_networks(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_room_networks_network_id ON room_networks(network_id)",
        "CREATE INDEX IF NOT EXISTS idx_cabinet_layouts_cabinet_id ON cabinet_layouts(cabinet_id)",
        "CREATE INDEX IF NOT EXISTS idx_cabinets_room_id ON cabinets(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_positions_cabinet_id ON positions(cabinet_id)",
        "CREATE INDEX IF NOT EXISTS idx_workstations_room_id ON workstations(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_device_interfaces_device_id ON device_interfaces(device_id)",
        "CREATE INDEX IF NOT EXISTS idx_device_interfaces_mac ON device_interfaces(mac_address) WHERE mac_address IS NOT NULL",
        "CREATE INDEX IF NOT EXISTS idx_device_interfaces_nic_id ON device_interfaces(nic_id) WHERE nic_id IS NOT NULL",
        "CREATE INDEX IF NOT EXISTS idx_device_interfaces_device_managed ON device_interfaces(device_managed) WHERE device_managed",
        "CREATE INDEX IF NOT EXISTS idx_device_nics_device_id ON device_nics(device_id)",
        "CREATE INDEX IF NOT EXISTS idx_device_macs_device_id ON device_macs(device_id)",
        "CREATE INDEX IF NOT EXISTS idx_device_macs_ip_address ON device_macs(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_device_macs_mac_address ON device_macs(mac_address)",
        "CREATE INDEX IF NOT EXISTS idx_device_lldps_device_id ON device_lldps(device_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_device_interface_id ON ips(device_interface_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_network_id ON ips(network_id)",
        "CREATE INDEX IF NOT EXISTS idx_cable_links_a ON cable_links(a_endpoint_type, a_endpoint_id)",
        "CREATE INDEX IF NOT EXISTS idx_cable_links_b ON cable_links(b_endpoint_type, b_endpoint_id)",
        "CREATE INDEX IF NOT EXISTS idx_cable_links_link_type ON cable_links(link_type)",
        "CREATE INDEX IF NOT EXISTS idx_operation_logs_user_id ON operation_logs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_operation_logs_created_at ON operation_logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_login_logs_username ON login_logs(username)",
        "CREATE INDEX IF NOT EXISTS idx_login_logs_created_at ON login_logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_revoked_tokens_token_hash ON revoked_tokens(token_hash)",
        "CREATE INDEX IF NOT EXISTS idx_revoked_tokens_expiry ON revoked_tokens(expiry)",
        "CREATE INDEX IF NOT EXISTS idx_token_usage_token_hash ON token_usage(token_hash)",
        "CREATE INDEX IF NOT EXISTS idx_token_usage_created_at ON token_usage(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_notifications_user_id ON notifications(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_enabled ON scheduled_tasks(enabled)",
        "CREATE INDEX IF NOT EXISTS idx_rooms_org_id ON rooms(org_id)",
        "CREATE INDEX IF NOT EXISTS idx_net_outlets_room_id ON net_outlets(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_patch_panels_cabinet_id ON patch_panels(cabinet_id)",
        "CREATE INDEX IF NOT EXISTS idx_devices_workstation_id ON devices(workstation_id)",
        "CREATE INDEX IF NOT EXISTS idx_devices_position_id ON devices(position_id)",
        "CREATE INDEX IF NOT EXISTS idx_devices_device_type ON devices(device_type)",
    ];

    for idx in &indexes {
        if let Err(e) = sqlx::query(*idx).execute(pool).await {
            foims_common::log_warn!("log.init.index_create_failed", error = e);
        }
    }

    Ok(())
}
