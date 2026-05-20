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
            imm.device_type,
            CASE
                WHEN s.id IS NOT NULL THEN s.name::text
                WHEN w.id IS NOT NULL THEN w.name::text
                WHEN cp.id IS NOT NULL THEN cp.name::text
                ELSE 'unknown device'
            END AS device_name,
            imm.room_network_id,
            rn.network_id AS network_id,
            CASE
                WHEN w.id IS NOT NULL THEN w.name::text
                ELSE NULL
            END AS workstation_name,
            CASE
                WHEN cp.id IS NOT NULL THEN cp.name::text
                ELSE NULL
            END AS cabinet_position_name,
            CASE
                WHEN s.id IS NOT NULL THEN s.name::text
                ELSE NULL
            END AS switch_name,
            sp.port_number::text AS switch_port_number,
            CASE
                WHEN r.id IS NOT NULL THEN r.name::text
                ELSE NULL
            END AS room_name,
            CASE
                WHEN c.id IS NOT NULL THEN c.name::text
                ELSE NULL
            END AS cabinet_name,
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
        LEFT JOIN workstations w ON imm.workstation_id = w.id
        LEFT JOIN rooms r ON w.room_id = r.id
        LEFT JOIN positions cp ON imm.position_id = cp.id
        LEFT JOIN cabinets c ON cp.cabinet_id = c.id
        LEFT JOIN switches s ON s.position_id = cp.id
        LEFT JOIN switch_ports sp ON imm.switch_port_id = sp.id
        LEFT JOIN room_networks rn ON imm.room_network_id = rn.id
        LEFT JOIN network_cidrs nc ON rn.network_id = nc.id
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
            sm.switch_id,
            s.name AS switch_name,
            host(sm.ip_address) AS ip_address,
            sm.mac_address AS snmp_mac,
            im.mac_address AS managed_mac,
            CASE
                WHEN im.id IS NULL THEN 'unmanaged'
                WHEN sm.mac_address = im.mac_address THEN 'match'
                ELSE 'mismatch'
            END AS comparison_result
        FROM switch_macs sm
        JOIN switches s ON sm.switch_id = s.id
        LEFT JOIN ips im ON sm.ip_address = im.ip_address AND im.device_type != 'switch'
    ",
    )
    .execute(pool)
    .await?;

    Ok(())
}
