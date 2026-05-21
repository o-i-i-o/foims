use sqlx::Error;
use sqlx::PgPool;
use sqlx::Row;
use tracing::warn;

pub async fn run(pool: &PgPool) -> Result<(), Error> {
    migrate_ips_add_network_id(pool).await?;
    migrate_update_views(pool).await?;
    migrate_remove_device_room_network_id(pool).await?;
    Ok(())
}

async fn migrate_ips_add_network_id(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'ips_add_network_id_v2'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "ALTER TABLE ips ADD COLUMN IF NOT EXISTS network_id UUID REFERENCES network_cidrs(id)",
        )
        .execute(pool)
        .await
        {
            warn!("添加 ips.network_id 列失败: {}", e);
        }

        if let Err(e) =
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_ips_network_id ON ips(network_id)")
                .execute(pool)
                .await
        {
            warn!("创建 idx_ips_network_id 索引失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"UPDATE ips i
            SET network_id = (
                SELECT rn.network_id
                FROM workstations w
                LEFT JOIN room_networks rn ON w.room_network_id = rn.id
                WHERE i.workstation_id = w.id
                UNION
                SELECT rn.network_id
                FROM positions p
                LEFT JOIN room_networks rn ON p.room_network_id = rn.id
                WHERE i.position_id = p.id
                LIMIT 1
            )
            WHERE i.network_id IS NULL",
        )
        .execute(pool)
        .await
        {
            warn!("迁移 ips.network_id 数据失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('ips_add_network_id_v2', '为 ips 表添加 network_id 列，直接关联 network_cidrs')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_update_views(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'update_views_for_ips_network_id'",
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
            r"CREATE VIEW ip_with_details AS
            SELECT 
                imm.id,
                imm.workstation_id,
                imm.position_id,
                imm.switch_port_id,
                imm.device_type,
                imm.network_id,
                CASE
                    WHEN s.id IS NOT NULL THEN s.name::text
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
            LEFT JOIN network_cidrs nc ON imm.network_id = nc.id
            LEFT JOIN network_regions nr ON nc.network_region_id = nr.id",
        )
        .execute(pool)
        .await
        {
            warn!("创建 ip_with_details 视图失败: {}", e);
        }

        if let Err(e) = sqlx::query("DROP VIEW IF EXISTS switches_with_details CASCADE")
            .execute(pool)
            .await
        {
            warn!("删除 switches_with_details 视图失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"CREATE VIEW switches_with_details AS
            SELECT 
                s.id, s.name, s.model, s.vendor,
                s.location, s.snmp_version, 
                s.snmp_community,
                s.snmp_username, s.snmp_auth_protocol, 
                s.snmp_auth_password,
                s.snmp_priv_protocol, 
                s.snmp_priv_password,
                s.snmp_port,
                s.position_id,
                p.cabinet_id, c.name as cabinet_name,
                r.id as room_id, r.name as room_name,
                p.start_u, p.end_u,
                i.network_id,
                nc.network_region_id,
                s.description,
                'switch'::text as device_type,
                host(i.ip_address) as ip_address,
                i.mac_address,
                s.created_at, s.updated_at
            FROM switches s
            LEFT JOIN positions p ON s.position_id = p.id
            LEFT JOIN cabinets c ON p.cabinet_id = c.id
            LEFT JOIN rooms r ON c.room_id = r.id
            LEFT JOIN LATERAL (
                SELECT ips.ip_address, ips.mac_address, ips.network_id
                FROM ips
                WHERE ips.position_id = p.id
                LIMIT 1
            ) i ON true
            LEFT JOIN network_cidrs nc ON i.network_id = nc.id",
        )
        .execute(pool)
        .await
        {
            warn!("创建 switches_with_details 视图失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('update_views_for_ips_network_id', '更新视图以使用 ips.network_id 直接关联 network_cidrs')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_remove_device_room_network_id(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'remove_device_room_network_id'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) =
            sqlx::query("ALTER TABLE workstations DROP COLUMN IF EXISTS room_network_id")
                .execute(pool)
                .await
        {
            warn!("删除 workstations.room_network_id 列失败: {}", e);
        }

        if let Err(e) = sqlx::query("ALTER TABLE positions DROP COLUMN IF EXISTS room_network_id")
            .execute(pool)
            .await
        {
            warn!("删除 positions.room_network_id 列失败: {}", e);
        }

        if let Err(e) = sqlx::query("DROP INDEX IF EXISTS idx_workstations_room_network_id")
            .execute(pool)
            .await
        {
            warn!("删除 idx_workstations_room_network_id 索引失败: {}", e);
        }

        if let Err(e) = sqlx::query("DROP INDEX IF EXISTS idx_positions_room_network_id")
            .execute(pool)
            .await
        {
            warn!("删除 idx_positions_room_network_id 索引失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('remove_device_room_network_id', '移除 workstations 和 positions 的 room_network_id 列，改为 ips.network_id 直接关联')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}
