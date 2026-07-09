use tracing::warn;

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let indexes = [
        "CREATE INDEX IF NOT EXISTS idx_network_cidrs_region ON network_cidrs(network_region_id)",
        "CREATE INDEX IF NOT EXISTS idx_room_networks_room_id ON room_networks(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_room_networks_network_id ON room_networks(network_id)",
        "CREATE INDEX IF NOT EXISTS idx_cabinet_layouts_cabinet_id ON cabinet_layouts(cabinet_id)",
        "CREATE INDEX IF NOT EXISTS idx_cabinets_room_id ON cabinets(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_positions_cabinet_id ON positions(cabinet_id)",
        "CREATE INDEX IF NOT EXISTS idx_positions_device_type ON positions(device_type)",
        "CREATE INDEX IF NOT EXISTS idx_workstations_room_id ON workstations(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_ports_device_id ON switch_ports(device_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_device_id ON switch_macs(device_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_ip_address ON switch_macs(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_mac_address ON switch_macs(mac_address)",
        "CREATE INDEX IF NOT EXISTS idx_switch_lldps_device_id ON switch_lldps(device_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_workstation_id ON ips(workstation_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_position_id ON ips(position_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_switch_port_id ON ips(switch_port_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_network_id ON ips(network_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_mac_address ON ips(mac_address)",
        "CREATE INDEX IF NOT EXISTS idx_ips_device_id ON ips(device_id)",
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
        "CREATE INDEX IF NOT EXISTS idx_access_points_room_id ON access_points(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_access_points_cabinet_id ON access_points(cabinet_id)",
        "CREATE INDEX IF NOT EXISTS idx_access_points_switch_port_id ON access_points(switch_port_id)",
        "CREATE INDEX IF NOT EXISTS idx_access_points_peer_ap_id ON access_points(peer_access_point_id)",
        "CREATE INDEX IF NOT EXISTS idx_devices_workstation_id ON devices(workstation_id)",
        "CREATE INDEX IF NOT EXISTS idx_devices_position_id ON devices(position_id)",
        "CREATE INDEX IF NOT EXISTS idx_devices_access_point_id ON devices(access_point_id)",
        "CREATE INDEX IF NOT EXISTS idx_devices_switch_port_id ON devices(switch_port_id)",
        "CREATE INDEX IF NOT EXISTS idx_devices_device_type ON devices(device_type)",
    ];

    for idx in &indexes {
        if let Err(e) = sqlx::query(*idx).execute(pool).await {
            warn!("索引创建失败（可能已存在）: {}", e);
        }
    }

    Ok(())
}
