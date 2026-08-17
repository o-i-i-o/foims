//! 数据库结构校验。
//!
//! 提供「必需表存在性」与「必需列完整性」两组校验，用于初始化
//! 向导与启动自检。清单必须与 `schema/tables/` 实际创建的结构保持
//! 同步：新增表/列时需同步补充本模块（本项目无迁移框架，见
//! AGENTS.md）。

use std::collections::HashMap;

/// 必需表清单（与 `schema/tables/mod.rs` 的建表范围一致）。
#[must_use]
pub fn get_required_tables() -> Vec<&'static str> {
    vec![
        // 用户与系统
        "users",
        "encryption_keys",
        "system_configs",
        // 网络
        "network_regions",
        "network_cidrs",
        "room_networks",
        // 组织与模板
        "org_templates",
        "organizations",
        "device_templates",
        // 空间
        "rooms",
        "cabinets",
        "positions",
        "workstations",
        "element_layouts",
        // 设备
        "devices",
        "device_nics",
        "device_ports",
        "device_interfaces",
        "device_macs",
        "device_lldps",
        "net_outlets",
        "patch_panels",
        // 链路与 IP
        "cable_links",
        "ips",
        // 拓扑
        "topology_nodes",
        "topology_connections",
        // 日志/令牌/通知/任务/布局
        "operation_logs",
        "task_logs",
        "login_logs",
        "revoked_tokens",
        "token_usage",
        "notifications",
        "scheduled_tasks",
        "workstation_layouts",
        "cabinet_layouts",
    ]
}

/// 必需列清单：`表名 → 必需列`。
///
/// 仅列出业务代码强依赖的列；表名/列名拼入 information_schema 查询，
/// 值均来自本内部常量，无注入风险。
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
        vec![
            "id",
            "name",
            "description",
            "ipv4_cidrs",
            "ipv6_cidrs",
            "created_at",
            "updated_at",
        ],
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
            "org_id",
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
            "start_u",
            "end_u",
            "description",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "org_templates",
        vec![
            "id",
            "name",
            "levels",
            "icons",
            "description",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "organizations",
        vec![
            "id",
            "name",
            "type_path",
            "parent_id",
            "template_id",
            "level_index",
            "description",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "device_templates",
        vec![
            "id",
            "name",
            "device_type",
            "brand",
            "model",
            "description",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "devices",
        vec![
            "id",
            "name",
            "hostname",
            "device_type",
            "brand",
            "model",
            "serial_number",
            "workstation_id",
            "position_id",
            "room_id",
            "template_id",
            "seller",
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
        "device_nics",
        vec![
            "id",
            "device_id",
            "name",
            "card_type",
            "description",
            "sort_order",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "net_outlets",
        vec!["id", "name", "room_id", "created_at", "updated_at"],
    );
    columns.insert(
        "patch_panels",
        vec!["id", "name", "cabinet_id", "created_at", "updated_at"],
    );
    columns.insert(
        "device_ports",
        vec![
            "id",
            "device_id",
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
        "device_interfaces",
        vec![
            "id",
            "device_id",
            "nic_id",
            "name",
            "physical_type",
            "interface_role",
            "mac_address",
            "vlan_id",
            "description",
            "sort_order",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "device_macs",
        vec![
            "id",
            "device_id",
            "ip_address",
            "mac_address",
            "interface",
            "vlan_id",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "device_lldps",
        vec![
            "id",
            "device_id",
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
        "cable_links",
        vec![
            "id",
            "a_endpoint_type",
            "a_endpoint_id",
            "b_endpoint_type",
            "b_endpoint_id",
            "link_type",
            "cable_label",
            "length_m",
            "tested",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "ips",
        vec![
            "id",
            "device_interface_id",
            "network_id",
            "ip_address",
            "ip_version",
            "description",
            "status",
            "last_seen",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "topology_nodes",
        vec![
            "id",
            "device_id",
            "x",
            "y",
            "width",
            "height",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "topology_connections",
        vec![
            "id",
            "source_device_id",
            "target_device_id",
            "source_device_port_id",
            "target_device_port_id",
            "label",
            "auto_discovered",
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

/// 检查全部必需表是否已创建。
pub async fn check_required_tables_exist(pool: &sqlx::PgPool) -> bool {
    for table in get_required_tables() {
        let Ok(exists) = sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(format!(
            "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = '{table}')"
        )))
        .fetch_one(pool)
        .await
        else {
            return false;
        };

        if !exists {
            return false;
        }
    }

    true
}

/// 逐表逐列校验必需列完整性，首个缺失项以错误消息返回。
pub async fn validate_table_columns(pool: &sqlx::PgPool) -> Result<(), String> {
    for (table, columns) in get_table_columns() {
        let table_exists: bool = match sqlx::query_scalar(
            sqlx::AssertSqlSafe(format!(
                "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = '{table}')"
            )),
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
                sqlx::AssertSqlSafe(format!(
                    "SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = '{table}' AND column_name = '{column}')"
                )),
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

/// 检查系统是否已有业务数据（以 users 表是否非空为准）。
pub async fn check_has_data(pool: &sqlx::PgPool) -> bool {
    match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
    {
        Ok(count) => count > 0,
        Err(_) => false,
    }
}
