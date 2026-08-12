use tracing::warn;

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    if let Err(e) = sqlx::query("DROP VIEW IF EXISTS ip_with_details CASCADE")
        .execute(pool)
        .await
    {
        warn!("删除旧视图失败: {}", e);
    }

    sqlx::query(
        r"
        CREATE VIEW ip_with_details AS
        SELECT
            imm.id,
            imm.device_interface_id,
            imm.device_id,
            imm.network_id,
            dv.name::text AS device_name,
            dv.device_type::text AS device_type,
            di.name::text AS interface_name,
            di.interface_type::text AS interface_type,
            w.name::text AS workstation_name,
            cp.name::text AS cabinet_position_name,
            r.name::text AS room_name,
            c.name::text AS cabinet_name,
            org.name::text AS org_name,
            COALESCE(nc.name, 'unknown')::text AS network_name,
            COALESCE(nr.name, 'unknown')::text AS network_region,
            host(imm.ip_address) as ip_address,
            imm.ip_version,
            imm.mac_address,
            imm.last_mac,
            imm.hostname,
            imm.description,
            imm.status,
            imm.last_seen,
            imm.created_at,
            imm.updated_at
        FROM ips imm
        JOIN devices dv ON imm.device_id = dv.id
        LEFT JOIN device_interfaces di ON imm.device_interface_id = di.id
        LEFT JOIN workstations w ON dv.workstation_id = w.id
        LEFT JOIN positions cp ON dv.position_id = cp.id
        LEFT JOIN cabinets c ON cp.cabinet_id = c.id
        LEFT JOIN rooms r ON dv.room_id = r.id
        LEFT JOIN organizations org ON r.org_id = org.id
        LEFT JOIN network_cidrs nc ON imm.network_id = nc.id
        LEFT JOIN network_regions nr ON nc.network_region_id = nr.id
    ",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query("GRANT SELECT ON ip_with_details TO ipma")
        .execute(pool)
        .await
    {
        warn!("授予ip_with_details视图权限失败: {}", e);
    }

    if let Err(e) = sqlx::query("DROP VIEW IF EXISTS mac_comparison CASCADE")
        .execute(pool)
        .await
    {
        warn!("删除旧视图失败: {}", e);
    }

    sqlx::query(
        r"
        CREATE VIEW mac_comparison AS
        SELECT
            sm.device_id,
            sdv.name AS device_name,
            host(sm.ip_address) AS ip_address,
            sm.mac_address AS snmp_mac,
            im.mac_address AS managed_mac,
            CASE
                WHEN im.id IS NULL THEN 'unmanaged'
                WHEN sm.mac_address = im.mac_address THEN 'match'
                ELSE 'mismatch'
            END AS comparison_result
        FROM device_macs sm
        JOIN devices sdv ON sm.device_id = sdv.id
        LEFT JOIN ips im ON sm.ip_address = im.ip_address AND im.device_id != sm.device_id
    ",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query("GRANT SELECT ON mac_comparison TO ipma")
        .execute(pool)
        .await
    {
        warn!("授予mac_comparison视图权限失败: {}", e);
    }

    if let Err(e) = sqlx::query("DROP VIEW IF EXISTS devices_with_details CASCADE")
        .execute(pool)
        .await
    {
        warn!("删除旧视图失败: {}", e);
    }

    sqlx::query(
        r"
        CREATE VIEW devices_with_details AS
        SELECT
            d.id, d.name, d.device_type, d.brand, d.model, d.serial_number,
            d.workstation_id, d.position_id, d.room_id,
            d.template_id, d.vendor, d.location,
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
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query("GRANT SELECT ON devices_with_details TO ipma")
        .execute(pool)
        .await
    {
        warn!("授予devices_with_details视图权限失败: {}", e);
    }

    if let Err(e) = sqlx::query("DROP VIEW IF EXISTS net_outlets_with_details CASCADE")
        .execute(pool)
        .await
    {
        warn!("删除旧视图失败: {}", e);
    }

    sqlx::query(
        r"
        CREATE VIEW net_outlets_with_details AS
        SELECT
            ap.id, ap.name, ap.outlet_type, ap.room_id, ap.cabinet_id,
            r.name AS room_name,
            cab.name AS cabinet_name,
            ap.created_at, ap.updated_at
        FROM net_outlets ap
        LEFT JOIN rooms r ON ap.room_id = r.id
        LEFT JOIN cabinets cab ON ap.cabinet_id = cab.id
    ",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query("GRANT SELECT ON net_outlets_with_details TO ipma")
        .execute(pool)
        .await
    {
        warn!("授予net_outlets_with_details视图权限失败: {}", e);
    }

    if let Err(e) = sqlx::query("DROP VIEW IF EXISTS cable_links_with_details CASCADE")
        .execute(pool)
        .await
    {
        warn!("删除旧视图失败: {}", e);
    }

    sqlx::query(
        r"
        CREATE VIEW cable_links_with_details AS
        WITH endpoint_labels AS (
            SELECT sp.id, 'device_port'::VARCHAR AS etype,
                   (sp.port_number || ' @ ' || d.name) AS label
            FROM device_ports sp JOIN devices d ON sp.device_id = d.id
            UNION ALL
            SELECT id, 'net_outlet'::VARCHAR, name::text FROM net_outlets
            UNION ALL
            SELECT di.id, 'device_interface'::VARCHAR, (di.name || ' @ ' || d.name)
            FROM device_interfaces di JOIN devices d ON di.device_id = d.id
        )
        SELECT
            cl.id, cl.link_type, cl.cable_label, cl.length_m, cl.tested,
            cl.created_at, cl.updated_at,
            cl.a_endpoint_type, cl.a_endpoint_id,
            cl.b_endpoint_type, cl.b_endpoint_id,
            a_lbl.label AS a_endpoint_label,
            b_lbl.label AS b_endpoint_label
        FROM cable_links cl
        LEFT JOIN endpoint_labels a_lbl ON cl.a_endpoint_id = a_lbl.id AND cl.a_endpoint_type = a_lbl.etype
        LEFT JOIN endpoint_labels b_lbl ON cl.b_endpoint_id = b_lbl.id AND cl.b_endpoint_type = b_lbl.etype
    ",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query("GRANT SELECT ON cable_links_with_details TO ipma")
        .execute(pool)
        .await
    {
        warn!("授予cable_links_with_details视图权限失败: {}", e);
    }

    Ok(())
}
