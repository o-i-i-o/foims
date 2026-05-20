use sqlx::PgPool;
use sqlx::Error;
use sqlx::Row;
use tracing::warn;

pub async fn run(pool: &PgPool) -> Result<(), Error> {
    migrate_ips_add_room_network_id(pool).await?;
    migrate_ip_with_details_room_network_id(pool).await?;
    Ok(())
}

async fn migrate_ips_add_room_network_id(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'ips_add_room_network_id'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "ALTER TABLE ips ADD COLUMN IF NOT EXISTS room_network_id UUID REFERENCES room_networks(id)"
        )
        .execute(pool)
        .await
        {
            warn!("添加 ips.room_network_id 列失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_ips_room_network_id ON ips(room_network_id)"
        )
        .execute(pool)
        .await
        {
            warn!("创建 idx_ips_room_network_id 索引失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"UPDATE ips i
            SET room_network_id = (
                SELECT rn.id
                FROM room_networks rn
                JOIN network_cidrs nc ON rn.network_id = nc.id
                LEFT JOIN workstations w ON i.workstation_id = w.id
                LEFT JOIN positions p ON i.position_id = p.id
                LEFT JOIN cabinets c ON p.cabinet_id = c.id
                LEFT JOIN rooms rw ON w.room_id = rw.id
                LEFT JOIN rooms rc ON c.room_id = rc.id
                WHERE rn.room_id = COALESCE(rw.id, rc.id)
                AND (
                    (nc.ipv4_cidr IS NOT NULL AND i.ip_address <<= nc.ipv4_cidr::inet)
                    OR (nc.ipv6_cidr IS NOT NULL AND i.ip_address <<= nc.ipv6_cidr::inet)
                )
                LIMIT 1
            )
            WHERE i.room_network_id IS NULL"
        )
        .execute(pool)
        .await
        {
            warn!("迁移现有IP的room_network_id失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('ips_add_room_network_id', '添加 ips.room_network_id 列，关联到 room_networks 表，实现IP与房间网段的直接绑定')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_ip_with_details_room_network_id(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'ip_with_details_room_network_id'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query("DROP VIEW IF EXISTS ip_with_details CASCADE")
            .execute(pool)
            .await
        {
            warn!("删除 ip_with_details 视图失败: {}", e);
        }

        if let Err(e) = sqlx::query(
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
                COALESCE(rn.name, 'unknown')::text AS network_name,
                COALESCE(rn.region_name, 'unknown')::text AS network_region,
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
        .await
        {
            warn!("更新 ip_with_details 视图失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('ip_with_details_room_network_id', '更新ip_with_details视图，使用room_network_id直接关联')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}
