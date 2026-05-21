use std::collections::HashMap;

#[must_use]
pub fn get_required_tables() -> Vec<&'static str> {
    vec![
        "users",
        "network_cidrs",
        "network_regions",
        "rooms",
        "room_networks",
        "cabinets",
        "workstations",
        "positions",
        "switches",
        "switch_ports",
        "switch_macs",
        "switch_lldps",
        "ips",
        "operation_logs",
        "task_logs",
        "login_logs",
        "revoked_tokens",
        "token_usage",
        "notifications",
        "system_configs",
        "scheduled_tasks",
        "workstation_layouts",
        "cabinet_layouts",
    ]
}

#[must_use]
pub fn get_table_columns() -> HashMap<&'static str, Vec<&'static str>> {
    let mut columns: HashMap<&'static str, Vec<&'static str>> = HashMap::new();

    columns.insert(
        "users",
        vec![
            "id",
            "username",
            "password_hash",
            "email",
            "role",
            "status",
            "reset_token",
            "reset_token_expiry",
            "two_factor_secret",
            "two_factor_enabled",
            "two_factor_verified",
            "two_factor_email_code",
            "two_factor_email_code_expiry",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "network_regions",
        vec!["id", "name", "description", "created_at", "updated_at"],
    );
    columns.insert(
        "network_cidrs",
        vec![
            "id",
            "name",
            "network_region_id",
            "ipv4_cidr",
            "ipv6_cidr",
            "ipv4_gateway",
            "ipv6_gateway",
            "ipv4_dns",
            "ipv6_dns",
            "description",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "rooms",
        vec![
            "id",
            "name",
            "room_type",
            "description",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "room_networks",
        vec!["id", "room_id", "network_id", "created_at", "updated_at"],
    );
    columns.insert(
        "cabinets",
        vec![
            "id",
            "name",
            "room_id",
            "capacity",
            "description",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "workstations",
        vec![
            "id",
            "name",
            "room_id",
            "room_network_id",
            "manager",
            "description",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "positions",
        vec![
            "id",
            "name",
            "cabinet_id",
            "room_network_id",
            "start_u",
            "end_u",
            "description",
            "device_type",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "switches",
        vec![
            "id",
            "name",
            "position_id",
            "model",
            "vendor",
            "location",
            "snmp_version",
            "snmp_community",
            "snmp_username",
            "snmp_auth_protocol",
            "snmp_auth_password",
            "snmp_priv_protocol",
            "snmp_priv_password",
            "snmp_port",
            "description",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "switch_ports",
        vec![
            "id",
            "switch_id",
            "port_number",
            "port_name",
            "port_type",
            "vlan_id",
            "status",
            "speed",
            "description",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "switch_macs",
        vec![
            "id",
            "switch_id",
            "ip_address",
            "mac_address",
            "interface",
            "vlan_id",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "switch_lldps",
        vec![
            "id",
            "switch_id",
            "local_port",
            "neighbor_chassis_id",
            "neighbor_port_id",
            "neighbor_port_desc",
            "neighbor_sys_name",
            "neighbor_sys_desc",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "ips",
        vec![
            "id",
            "workstation_id",
            "position_id",
            "switch_port_id",
            "device_type",
            "ip_address",
            "ip_version",
            "mac_address",
            "hostname",
            "status",
            "last_seen",
            "last_mac",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "operation_logs",
        vec![
            "id",
            "user_id",
            "action",
            "resource_type",
            "resource_id",
            "details",
            "result",
            "ip_address",
            "created_at",
        ],
    );
    columns.insert(
        "task_logs",
        vec![
            "id",
            "task_name",
            "status",
            "details",
            "start_time",
            "end_time",
            "duration",
        ],
    );
    columns.insert(
        "login_logs",
        vec![
            "id",
            "username",
            "ip_address",
            "user_agent",
            "success",
            "error_message",
            "created_at",
        ],
    );
    columns.insert(
        "revoked_tokens",
        vec!["id", "token_hash", "user_id", "revoked_at", "expiry"],
    );
    columns.insert(
        "token_usage",
        vec![
            "id",
            "token_hash",
            "user_id",
            "ip_address",
            "user_agent",
            "request_path",
            "created_at",
        ],
    );
    columns.insert(
        "notifications",
        vec![
            "id",
            "user_id",
            "title",
            "content",
            "notification_type",
            "read",
            "created_at",
        ],
    );
    columns.insert(
        "system_configs",
        vec![
            "id",
            "config_type",
            "key",
            "value",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "scheduled_tasks",
        vec![
            "id",
            "name",
            "task_type",
            "cron_expression",
            "enabled",
            "config",
            "last_run_at",
            "next_run_at",
            "last_result",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "workstation_layouts",
        vec![
            "id",
            "workstation_id",
            "x",
            "y",
            "width",
            "height",
            "rotation",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "cabinet_layouts",
        vec![
            "id",
            "cabinet_id",
            "x",
            "y",
            "width",
            "height",
            "rotation",
            "created_at",
            "updated_at",
        ],
    );

    columns
}

pub async fn check_required_tables_exist(pool: &sqlx::PgPool) -> bool {
    let required_tables = get_required_tables();

    for table in &required_tables {
        let Ok(exists) = sqlx::query_scalar::<_, bool>(
            &format!("SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = '{table}')")
        )
        .fetch_one(pool)
        .await
        else {
            return false
        };

        if !exists {
            return false;
        }
    }

    true
}

pub async fn validate_table_columns(pool: &sqlx::PgPool) -> Result<(), String> {
    let required_columns = get_table_columns();

    for (table, columns) in required_columns {
        let table_exists: bool = match sqlx::query_scalar(
            &format!("SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = '{table}')")
        )
        .fetch_one(pool)
        .await
        {
            Ok(exists) => exists,
            Err(e) => return Err(format!("检查表 {table} 是否存在时出错: {e}")),
        };

        if !table_exists {
            return Err(format!("表 {table} 不存在"));
        }

        for column in columns {
            let column_exists: bool = match sqlx::query_scalar(
                &format!(
                    "SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = '{table}' AND column_name = '{column}')"
                )
            )
            .fetch_one(pool)
            .await
            {
                Ok(exists) => exists,
                Err(e) => return Err(format!("检查列 {table}.{column} 是否存在时出错: {e}")),
            };

            if !column_exists {
                return Err(format!("表 {table} 缺少必需的列: {column}"));
            }
        }
    }

    Ok(())
}

pub async fn check_has_data(pool: &sqlx::PgPool) -> bool {
    match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
    {
        Ok(count) => count > 0,
        Err(_) => false,
    }
}
