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
            imm.workstation_id,
            imm.position_id,
            imm.switch_port_id,
            imm.device_id,
            imm.device_type,
            imm.network_id,
            CASE
                WHEN dv.id IS NOT NULL THEN dv.name::text
                WHEN w.id IS NOT NULL THEN w.name::text
                WHEN cp.id IS NOT NULL THEN cp.name::text
                ELSE 'unknown device'
            END AS device_name,
            CASE
                WHEN w.id IS NOT NULL THEN w.name::text
                ELSE NULL
            END AS workstation_name,
            CASE
                WHEN cp.id IS NOT NULL THEN cp.name::text
                ELSE NULL
            END AS cabinet_position_name,
            sdv.name::text AS switch_name,
            sp.port_number::text AS switch_port_number,
            CASE
                WHEN dv.id IS NOT NULL THEN dv.name::text
                ELSE NULL
            END AS connected_device_name,
            CASE
                WHEN dv.id IS NOT NULL THEN dv.device_type::text
                ELSE NULL
            END AS connected_device_type,
            ap.name::text AS access_point_name,
            ap2.name::text AS peer_access_point_name,
            CASE
                WHEN r.id IS NOT NULL THEN r.name::text
                ELSE NULL
            END AS room_name,
            CASE
                WHEN c.id IS NOT NULL THEN c.name::text
                ELSE NULL
            END AS cabinet_name,
            org.name::text AS org_name,
            COALESCE(nc.name, 'unknown')::text AS network_name,
            COALESCE(nr.name, 'unknown')::text AS network_region,
            host(imm.ip_address) as ip_address,
            imm.ip_version,
            imm.mac_address,
            imm.last_mac,
            imm.hostname,
            imm.status,
            imm.last_seen,
            imm.created_at,
            imm.updated_at
        FROM ips imm
        LEFT JOIN devices dv ON imm.device_id = dv.id
        LEFT JOIN access_points ap ON dv.access_point_id = ap.id
        LEFT JOIN access_points ap2 ON ap.peer_access_point_id = ap2.id
        LEFT JOIN workstations w ON imm.workstation_id = w.id
        LEFT JOIN positions cp ON imm.position_id = cp.id
        LEFT JOIN cabinets c ON cp.cabinet_id = c.id
        LEFT JOIN switch_ports sp ON imm.switch_port_id = sp.id
        LEFT JOIN devices sdv ON sp.device_id = sdv.id
        LEFT JOIN rooms r ON COALESCE(w.room_id, (SELECT ws.room_id FROM devices d2 JOIN workstations ws ON d2.workstation_id = ws.id WHERE d2.id = dv.id), (SELECT cab2.room_id FROM devices d3 JOIN positions p2 ON d3.position_id = p2.id JOIN cabinets cab2 ON p2.cabinet_id = cab2.id WHERE d3.id = dv.id)) = r.id
        LEFT JOIN organizations org ON r.org_id = org.id
        LEFT JOIN network_cidrs nc ON imm.network_id = nc.id
        LEFT JOIN network_regions nr ON nc.network_region_id = nr.id
    ",
    )
    .execute(pool)
    .await?;

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
        FROM switch_macs sm
        JOIN devices sdv ON sm.device_id = sdv.id
        LEFT JOIN ips im ON sm.ip_address = im.ip_address AND im.device_type != 'switch'
    ",
    )
    .execute(pool)
    .await?;

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
            d.workstation_id, d.position_id, d.access_point_id, d.switch_port_id,
            d.template_id, d.vendor, d.location,
            d.snmp_version, d.snmp_community, d.snmp_username,
            d.snmp_auth_protocol, d.snmp_auth_password,
            d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
            d.description,
            w.name AS workstation_name,
            COALESCE(w.room_id, (SELECT cab.room_id FROM positions p JOIN cabinets cab ON p.cabinet_id = cab.id WHERE p.id = d.position_id)) AS room_id,
            COALESCE(r.name, (SELECT cab2.name FROM positions p2 JOIN cabinets cab2 ON p2.cabinet_id = cab2.id WHERE p2.id = d.position_id)) AS room_name,
            (SELECT cab3.id FROM positions p3 JOIN cabinets cab3 ON p3.cabinet_id = cab3.id WHERE p3.id = d.position_id) AS cabinet_id,
            (SELECT cab4.name FROM positions p4 JOIN cabinets cab4 ON p4.cabinet_id = cab4.id WHERE p4.id = d.position_id) AS cabinet_name,
            p.start_u, p.end_u,
            ap.name AS access_point_name,
            ap.ap_type AS access_point_type,
            sp.port_number AS connected_switch_port,
            sdv.name AS connected_switch_name,
            dt.name AS template_name,
            d.created_at, d.updated_at
        FROM devices d
        LEFT JOIN workstations w ON d.workstation_id = w.id
        LEFT JOIN rooms r ON w.room_id = r.id
        LEFT JOIN positions p ON d.position_id = p.id
        LEFT JOIN access_points ap ON d.access_point_id = ap.id
        LEFT JOIN switch_ports sp ON d.switch_port_id = sp.id
        LEFT JOIN devices sdv ON sp.device_id = sdv.id
        LEFT JOIN device_templates dt ON d.template_id = dt.id
    ",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query("DROP VIEW IF EXISTS access_points_with_details CASCADE")
        .execute(pool)
        .await
    {
        warn!("删除旧视图失败: {}", e);
    }

    sqlx::query(
        r"
        CREATE VIEW access_points_with_details AS
        SELECT
            ap.id, ap.name, ap.ap_type, ap.room_id, ap.cabinet_id,
            ap.peer_access_point_id, ap.switch_port_id, ap.description,
            r.name AS room_name,
            cab.name AS cabinet_name,
            pap.name AS peer_access_point_name,
            sp.port_number AS connected_switch_port,
            sdv.name AS connected_switch_name,
            ap.created_at, ap.updated_at
        FROM access_points ap
        LEFT JOIN rooms r ON ap.room_id = r.id
        LEFT JOIN cabinets cab ON ap.cabinet_id = cab.id
        LEFT JOIN access_points pap ON ap.peer_access_point_id = pap.id
        LEFT JOIN switch_ports sp ON ap.switch_port_id = sp.id
        LEFT JOIN devices sdv ON sp.device_id = sdv.id
    ",
    )
    .execute(pool)
    .await?;

    Ok(())
}
