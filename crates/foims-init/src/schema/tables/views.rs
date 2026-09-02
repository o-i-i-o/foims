//! 数据库视图定义。
//!
//! 每个视图按「DROP（容忍失败，仅告警）→ CREATE → GRANT（容忍失败）」
//! 的统一流程重建；CREATE 失败会中止初始化。视图列名与各资源查询
//! （如 `src/resource/ip.rs`）约定耦合，修改时需同步业务查询。

/// 视图清单：`(视图名, CREATE VIEW 语句)`。
///
/// 名称仅来自本内部常量，拼入 DROP/GRANT 语句无注入风险。
const VIEWS: &[(&str, &str)] = &[
    (
        "ip_with_details",
        r"
        CREATE VIEW ip_with_details AS
        SELECT
            imm.id,
            imm.device_interface_id,
            dv.id AS device_id,
            imm.subnet_id,
            dv.name::text AS device_name,
            dv.device_type::text AS device_type,
            dv.hostname::text AS hostname,
            di.name::text AS interface_name,
            di.physical_type::text AS physical_type,
            di.interface_role::text AS interface_role,
            di.mac_address AS mac_address,
            w.name::text AS workstation_name,
            cp.name::text AS cabinet_position_name,
            r.name::text AS room_name,
            c.name::text AS cabinet_name,
            org.name::text AS org_name,
            COALESCE(nc.name, 'unknown')::text AS network_name,
            COALESCE(nr.name, 'unknown')::text AS network_region,
            host(imm.ip_address) as ip_address,
            imm.ip_version,
            imm.description,
            imm.status,
            imm.last_seen,
            imm.created_at,
            imm.updated_at,
            dv.position_id
        FROM ips imm
        JOIN device_interfaces di ON imm.device_interface_id = di.id
        JOIN devices dv ON di.device_id = dv.id
        LEFT JOIN workstations w ON dv.workstation_id = w.id
        LEFT JOIN positions cp ON dv.position_id = cp.id
        LEFT JOIN cabinets c ON cp.cabinet_id = c.id
        LEFT JOIN rooms r ON dv.room_id = r.id
        LEFT JOIN organizations org ON r.org_id = org.id
        LEFT JOIN network_cidrs nc ON imm.subnet_id = nc.id
        LEFT JOIN network_regions nr ON nc.network_region_id = nr.id
    ",
    ),
    (
        "mac_comparison",
        r"
        CREATE VIEW mac_comparison AS
        SELECT
            sm.device_id,
            sdv.name AS device_name,
            host(sm.ip_address) AS ip_address,
            sm.mac_address AS snmp_mac,
            di.mac_address AS managed_mac,
            CASE
                WHEN i.id IS NULL OR di.id IS NULL OR di.mac_address IS NULL THEN 'unmanaged'
                WHEN sm.mac_address = di.mac_address THEN 'match'
                ELSE 'mismatch'
            END AS comparison_result
        FROM device_macs sm
        JOIN devices sdv ON sm.device_id = sdv.id
        LEFT JOIN ips i ON sm.ip_address = i.ip_address
        LEFT JOIN device_interfaces di ON i.device_interface_id = di.id AND di.device_id = sm.device_id
    ",
    ),
    (
        "devices_with_details",
        r"
        CREATE VIEW devices_with_details AS
        SELECT
            d.id, d.name, d.hostname, d.device_type, d.brand, d.model, d.serial_number,
            d.workstation_id, d.position_id, d.room_id,
            d.template_id, d.seller, d.location,
            d.snmp_version, d.snmp_community, d.snmp_username,
            d.snmp_auth_protocol, d.snmp_auth_password,
            d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
            d.description,
            w.name AS workstation_name,
            r.name AS room_name,
            cab.id AS cabinet_id,
            cab.name AS cabinet_name,
            p.start_u, p.end_u,
            dt.name AS template_name,
            d.created_at, d.updated_at
        FROM devices d
        LEFT JOIN workstations w ON d.workstation_id = w.id
        LEFT JOIN positions p ON d.position_id = p.id
        LEFT JOIN cabinets cab ON p.cabinet_id = cab.id
        LEFT JOIN rooms r ON d.room_id = r.id
        LEFT JOIN device_templates dt ON d.template_id = dt.id
    ",
    ),
    (
        "net_outlets_with_details",
        r"
        CREATE VIEW net_outlets_with_details AS
        SELECT
            ap.id, ap.name, ap.room_id,
            r.name AS room_name,
            ap.created_at, ap.updated_at
        FROM net_outlets ap
        LEFT JOIN rooms r ON ap.room_id = r.id
    ",
    ),
    (
        "patch_panels_with_details",
        r"
        CREATE VIEW patch_panels_with_details AS
        SELECT
            pp.id, pp.name, pp.cabinet_id,
            c.name AS cabinet_name,
            c.room_id,
            r.name AS room_name,
            pp.created_at, pp.updated_at
        FROM patch_panels pp
        JOIN cabinets c ON pp.cabinet_id = c.id
        LEFT JOIN rooms r ON c.room_id = r.id
    ",
    ),
    (
        "cable_links_with_details",
        r"
        CREATE VIEW cable_links_with_details AS
        WITH endpoint_labels AS (
            SELECT id, 'net_outlet'::VARCHAR AS etype, name::text AS label,
                   room_id, NULL::UUID AS cabinet_id, NULL::UUID AS device_id
            FROM net_outlets
            UNION ALL
            SELECT di.id, 'device_interface'::VARCHAR, (di.name || ' @ ' || d.name),
                   d.room_id, cab.id, di.device_id
            FROM device_interfaces di
            JOIN devices d ON di.device_id = d.id
            LEFT JOIN positions p ON d.position_id = p.id
            LEFT JOIN cabinets cab ON p.cabinet_id = cab.id
            UNION ALL
            SELECT pp.id, 'patch_panel'::VARCHAR, pp.name::text,
                   c.room_id, pp.cabinet_id, NULL::UUID
            FROM patch_panels pp
            JOIN cabinets c ON pp.cabinet_id = c.id
        )
        SELECT
            cl.id, cl.link_type, cl.cable_label, cl.length_m, cl.tested,
            cl.created_at, cl.updated_at,
            cl.a_endpoint_type, cl.a_endpoint_id,
            cl.b_endpoint_type, cl.b_endpoint_id,
            a_lbl.label AS a_endpoint_label,
            a_lbl.room_id AS a_room_id,
            a_lbl.cabinet_id AS a_cabinet_id,
            a_lbl.device_id AS a_device_id,
            b_lbl.label AS b_endpoint_label,
            b_lbl.room_id AS b_room_id,
            b_lbl.cabinet_id AS b_cabinet_id,
            b_lbl.device_id AS b_device_id
        FROM cable_links cl
        LEFT JOIN endpoint_labels a_lbl ON cl.a_endpoint_id = a_lbl.id AND cl.a_endpoint_type = a_lbl.etype
        LEFT JOIN endpoint_labels b_lbl ON cl.b_endpoint_id = b_lbl.id AND cl.b_endpoint_type = b_lbl.etype
    ",
    ),
];

/// 创建全部视图（重复执行安全：先删后建）。
pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    for (name, ddl) in VIEWS {
        // 旧视图残留时删除失败不致命（如权限差异），告警后继续重建
        if let Err(e) = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP VIEW IF EXISTS {name} CASCADE"
        )))
        .execute(pool)
        .await
        {
            foims_common::log_warn!("log.init.view_drop_failed", name = name, error = e);
        }

        sqlx::query(sqlx::AssertSqlSafe((*ddl).to_string()))
            .execute(pool)
            .await?;

        // 应用角色缺省时 GRANT 失败不致命，告警后继续
        if let Err(e) = sqlx::query(sqlx::AssertSqlSafe(format!(
            "GRANT SELECT ON {name} TO foims"
        )))
        .execute(pool)
        .await
        {
            foims_common::log_warn!("log.init.view_grant_failed", name = name, error = e);
        }
    }
    Ok(())
}
