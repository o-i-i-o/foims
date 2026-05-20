use tracing::warn;

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let indexes = [
        "CREATE INDEX IF NOT EXISTS idx_users_username ON users(username)",
        "CREATE INDEX IF NOT EXISTS idx_users_email ON users(email)",
        "CREATE INDEX IF NOT EXISTS idx_network_cidrs_region ON network_cidrs(network_region_id)",
        "CREATE INDEX IF NOT EXISTS idx_room_networks_room_id ON room_networks(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_room_networks_network_id ON room_networks(network_id)",
        "CREATE INDEX IF NOT EXISTS idx_workstation_layouts_room_id ON workstation_layouts(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_cabinet_layouts_cabinet_id ON cabinet_layouts(cabinet_id)",
        "CREATE INDEX IF NOT EXISTS idx_cabinets_room_id ON cabinets(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_positions_cabinet_id ON positions(cabinet_id)",
        "CREATE INDEX IF NOT EXISTS idx_positions_device_type ON positions(device_type)",
        "CREATE INDEX IF NOT EXISTS idx_workstations_room_id ON workstations(room_id)",
        "CREATE INDEX IF NOT EXISTS idx_switches_position_id ON switches(position_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_ports_switch_id ON switch_ports(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_switch_id ON switch_macs(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_ip_address ON switch_macs(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_mac_address ON switch_macs(mac_address)",
        "CREATE INDEX IF NOT EXISTS idx_switch_lldps_switch_id ON switch_lldps(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_workstation_id ON ips(workstation_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_position_id ON ips(position_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_switch_port_id ON ips(switch_port_id)",
        "CREATE INDEX IF NOT EXISTS idx_ips_ip_address ON ips(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_ips_room_network_id ON ips(room_network_id)",
        "CREATE INDEX IF NOT EXISTS idx_operation_logs_user_id ON operation_logs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_operation_logs_created_at ON operation_logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_login_logs_username ON login_logs(username)",
        "CREATE INDEX IF NOT EXISTS idx_login_logs_created_at ON login_logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_revoked_tokens_token_hash ON revoked_tokens(token_hash)",
        "CREATE INDEX IF NOT EXISTS idx_revoked_tokens_expiry ON revoked_tokens(expiry)",
        "CREATE INDEX IF NOT EXISTS idx_token_usage_token_hash ON token_usage(token_hash)",
        "CREATE INDEX IF NOT EXISTS idx_token_usage_created_at ON token_usage(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_notifications_user_id ON notifications(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_name ON scheduled_tasks(name)",
        "CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_enabled ON scheduled_tasks(enabled)",
    ];

    for idx in &indexes {
        if let Err(e) = sqlx::query(idx).execute(pool).await {
            warn!("索引创建失败（可能已存在）: {}", e);
        }
    }

    Ok(())
}
