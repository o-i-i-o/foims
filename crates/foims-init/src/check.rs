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
        "password_history",
        "encryption_keys",
        "system_configs",
        // 子网
        "network_regions",
        "network_cidrs",
        "room_networks",
        // 组织与模板
        "org_templates",
        "organizations",
        "employees",
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
        "topology_connection_members",
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
            "auth_provider",
            "reset_token",
            "reset_token_expiry",
            "two_factor_secret",
            "two_factor_enabled",
            "two_factor_verified",
            "two_factor_email_code",
            "two_factor_email_code_expiry",
            "tokens_invalidated_at",
            "password_changed_at",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "password_history",
        vec!["id", "user_id", "password_hash", "created_at"],
    );
    columns.insert(
        "encryption_keys",
        vec![
            "id",
            "key_name",
            "encryption_key",
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
        vec!["id", "room_id", "subnet_id", "created_at", "updated_at"],
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
            "manager_employee_id",
            "description",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "employees",
        vec![
            "id",
            "org_id",
            "name",
            "gender",
            "phone",
            "email",
            "hire_date",
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
            "port_type",
            "status",
            "speed",
            "trunk_id",
            "device_managed",
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
            "subnet_id",
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
            "connection_type",
            "created_at",
            "updated_at",
        ],
    );
    columns.insert(
        "topology_connection_members",
        vec![
            "id",
            "connection_id",
            "device_id",
            "device_port_id",
            "side",
            "created_at",
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
        "element_layouts",
        vec![
            "id",
            "room_id",
            "element_type",
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
        "workstation_layouts",
        vec![
            "id",
            "workstation_id",
            "room_id",
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

/// 检查全部必需表是否已创建（含必需视图与必需约束性索引）。
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

    // 必需索引缺失（如按旧版 DDL 建的库）时同样视为结构不完整，
    // 触发 init 路径的 create_tables 幂等补建
    for (index, table) in get_required_indexes() {
        let Ok(exists) = sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(format!(
            "SELECT EXISTS(SELECT 1 FROM pg_indexes WHERE schemaname = 'public' AND tablename = '{table}' AND indexname = '{index}')"
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

    check_required_views_exist(pool).await
}

/// 必需视图清单（与 `schema/tables/views.rs` 一致）。
/// 视图缺失时列表接口直接 42P01 报错，必须纳入自检（K-5）。
#[must_use]
pub fn get_required_views() -> Vec<&'static str> {
    vec![
        "ip_with_details",
        "mac_comparison",
        "devices_with_details",
        "net_outlets_with_details",
        "patch_panels_with_details",
        "cable_links_with_details",
    ]
}

/// 检查全部必需视图是否已创建。
pub async fn check_required_views_exist(pool: &sqlx::PgPool) -> bool {
    for view in get_required_views() {
        let Ok(exists) = sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(format!(
            "SELECT EXISTS(SELECT 1 FROM information_schema.views WHERE table_schema = 'public' AND table_name = '{view}')"
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
pub async fn validate_table_columns(pool: &sqlx::PgPool) -> Result<(), foims_common::AppMessage> {
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
            Err(e) => {
                return Err(foims_common::msg("server.init.db.table_check_failed")
                    .with("table", table)
                    .with("error", e))
            }
        };

        if !table_exists {
            return Err(foims_common::msg("server.init.db.table_missing").with("table", table));
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
                Err(e) => {
                    return Err(foims_common::msg("server.init.db.column_check_failed")
                        .with("table", table)
                        .with("column", column)
                        .with("error", e))
                }
            };

            if !column_exists {
                return Err(foims_common::msg("server.init.db.column_missing")
                    .with("table", table)
                    .with("column", column));
            }
        }
    }

    // 列宽契约：密文落库的凭据列宽度不符说明库结构落后于当前 DDL
    //（本项目无迁移框架，需按 AGENTS.md 手工执行 ALTER 后重试）。
    // 独立于逐表循环执行一次，避免随表数量重复扫描
    for (width_table, width_column, expected) in get_required_column_widths() {
        let actual: Option<Option<i32>> = match sqlx::query_scalar(
            sqlx::AssertSqlSafe(format!(
                "SELECT character_maximum_length FROM information_schema.columns WHERE table_schema = 'public' AND table_name = '{width_table}' AND column_name = '{width_column}'"
            )),
        )
        .fetch_optional(pool)
        .await
        {
            Ok(row) => row,
            Err(e) => {
                return Err(foims_common::msg("server.init.db.column_check_failed")
                    .with("table", width_table)
                    .with("column", width_column)
                    .with("error", e))
            }
        };

        match actual {
            // 列不存在交由上方必需列清单报告，此处跳过
            None | Some(None) => {}
            Some(Some(len)) if len == expected => {}
            Some(Some(len)) => {
                return Err(foims_common::msg("server.init.db.column_check_failed")
                    .with("table", width_table)
                    .with("column", width_column)
                    .with("error", format!("列宽不符：期望 {expected}，实际 {len}")));
            }
        }
    }

    // 非空约束契约：归属关系列漂移为可空说明库结构落后于当前 DDL
    //（本项目无迁移框架，需按 AGENTS.md 手工执行 ALTER 后重试）。
    // 与列宽契约同口径独立执行一次，避免随表数量重复扫描
    for (nn_table, nn_column) in get_required_not_null_columns() {
        let nullable: Option<String> = match sqlx::query_scalar(
            sqlx::AssertSqlSafe(format!(
                "SELECT is_nullable FROM information_schema.columns WHERE table_schema = 'public' AND table_name = '{nn_table}' AND column_name = '{nn_column}'"
            )),
        )
        .fetch_optional(pool)
        .await
        {
            Ok(row) => row,
            Err(e) => {
                return Err(foims_common::msg("server.init.db.column_check_failed")
                    .with("table", nn_table)
                    .with("column", nn_column)
                    .with("error", e))
            }
        };

        match nullable.as_deref() {
            // 列不存在交由上方必需列清单报告，此处跳过
            None => {}
            Some("NO") => {}
            Some(actual) => {
                return Err(foims_common::msg("server.init.db.column_check_failed")
                    .with("table", nn_table)
                    .with("column", nn_column)
                    .with(
                        "error",
                        format!("非空约束不符：期望 NOT NULL，实际 {actual}"),
                    ));
            }
        }
    }

    Ok(())
}

/// 检查系统是否已有业务数据（以 users 表是否非空为准）。
///
/// 查询错误向上传播（fail-fast）：本函数的结论决定
/// `backup_and_drop_for_rebuild` 是否跳过备份直接删库，
/// 把数据库故障误判为"无数据"会销毁存量业务数据。
pub async fn check_has_data(pool: &sqlx::PgPool) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(count > 0)
}

/// 必需列宽契约：`(表名, 列名, 期望 character_maximum_length)`。
///
/// 仅约束以密文落库的 SNMP 凭据列：AES-GCM（+12 字节 nonce +16 字节
/// 认证标签）+ base64 后长度膨胀，列宽必须容纳模型允许的最长明文
/// 加密结果，否则写入期 "value too long" 500。清单与
/// `schema/tables/devices.rs` 的 DDL 保持同步。
#[must_use]
pub fn get_required_column_widths() -> Vec<(&'static str, &'static str, i32)> {
    vec![
        ("devices", "snmp_community", 255),
        ("devices", "snmp_username", 128),
        ("devices", "snmp_auth_password", 255),
        ("devices", "snmp_priv_password", 255),
    ]
}

/// 非空约束契约：`(表名, 列名)`，期望 `information_schema.columns.is_nullable`
/// 为 'NO'。仅登记承载强制归属关系的列：此类列漂移为可空后，写入路径
/// 可能产生无法归属的孤儿数据。清单与 `schema/tables/` 的 DDL 保持同步。
#[must_use]
pub fn get_required_not_null_columns() -> Vec<(&'static str, &'static str)> {
    vec![
        // IP 必须挂载在网口下（网口删除级联删除 IP）
        ("ips", "device_interface_id"),
        // IP 必须归属子网（写入路径按房间绑定子网探测/显式指定，不允许 NULL）
        ("ips", "subnet_id"),
    ]
}

/// 必需索引清单（与 `schema/tables/` 中创建的约束性索引一致）：
/// `(索引名, 表名)`。仅列承载业务不变式（并发下仍需成立）的唯一索引。
#[must_use]
pub fn get_required_indexes() -> Vec<(&'static str, &'static str)> {
    vec![
        // 同一对设备之间只允许一条逻辑连接（链路聚合）：
        // 表达式部分索引，物理连线同设备对允许多条故不纳入
        ("uq_topology_connections_logical", "topology_connections"),
        // 同一对端点之间只允许一条跳接线路（cable_links.rs 建表后补建）
        ("uq_cable_links_endpoint_pair", "cable_links"),
        // 同一网段地址（v4/v6）全域唯一（network.rs 部分唯一索引）
        ("uq_network_cidrs_ipv4", "network_cidrs"),
        ("uq_network_cidrs_ipv6", "network_cidrs"),
        // 逻辑连线成员端口全域唯一：同一端口不得同时参与两条逻辑连线
        //（topology.rs 建表后补建）
        (
            "uq_topology_connection_member_port_global",
            "topology_connection_members",
        ),
        // IP 地址全域唯一（ips.rs 建表约束 UNIQUE(ip_address)）：
        // 单条创建与批量导入的并发去重均依赖该唯一索引兜底
        ("uq_ips_ip_address", "ips"),
    ]
}
