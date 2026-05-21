use sqlx::PgPool;
use sqlx::Error;
use sqlx::Row;
use tracing::warn;

pub async fn run(pool: &PgPool) -> Result<(), Error> {
    migrate_workstations_add_room_network_id(pool).await?;
    migrate_positions_add_room_network_id(pool).await?;
    migrate_ips_remove_room_network_id(pool).await?;
    migrate_update_views(pool).await?;
    migrate_remove_old_triggers(pool).await?;
    Ok(())
}

async fn migrate_workstations_add_room_network_id(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'workstations_add_room_network_id'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "ALTER TABLE workstations ADD COLUMN IF NOT EXISTS room_network_id UUID REFERENCES room_networks(id)"
        )
        .execute(pool)
        .await
        {
            warn!("添加 workstations.room_network_id 列失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_workstations_room_network_id ON workstations(room_network_id)"
        )
        .execute(pool)
        .await
        {
            warn!("创建 idx_workstations_room_network_id 索引失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"UPDATE workstations w
            SET room_network_id = (
                SELECT rn.id
                FROM room_networks rn
                JOIN network_cidrs nc ON rn.network_id = nc.id
                WHERE rn.room_id = w.room_id
                LIMIT 1
            )
            WHERE w.room_network_id IS NULL"
        )
        .execute(pool)
        .await
        {
            warn!("迁移 workstations.room_network_id 数据失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('workstations_add_room_network_id', '为 workstations 表添加 room_network_id 列')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_positions_add_room_network_id(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'positions_add_room_network_id'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "ALTER TABLE positions ADD COLUMN IF NOT EXISTS room_network_id UUID REFERENCES room_networks(id)"
        )
        .execute(pool)
        .await
        {
            warn!("添加 positions.room_network_id 列失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_positions_room_network_id ON positions(room_network_id)"
        )
        .execute(pool)
        .await
        {
            warn!("创建 idx_positions_room_network_id 索引失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"UPDATE positions p
            SET room_network_id = (
                SELECT rn.id
                FROM room_networks rn
                JOIN cabinets c ON p.cabinet_id = c.id
                WHERE rn.room_id = c.room_id
                LIMIT 1
            )
            WHERE p.room_network_id IS NULL"
        )
        .execute(pool)
        .await
        {
            warn!("迁移 positions.room_network_id 数据失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('positions_add_room_network_id', '为 positions 表添加 room_network_id 列')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_ips_remove_room_network_id(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'ips_remove_room_network_id'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "DROP INDEX IF EXISTS idx_ips_room_network_id"
        )
        .execute(pool)
        .await
        {
            warn!("删除 idx_ips_room_network_id 索引失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "ALTER TABLE ips DROP COLUMN IF EXISTS room_network_id"
        )
        .execute(pool)
        .await
        {
            warn!("删除 ips.room_network_id 列失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('ips_remove_room_network_id', '从 ips 表移除 room_network_id 列，改为从设备关联获取')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_update_views(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'update_views_for_device_network'",
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
                CASE
                    WHEN s.id IS NOT NULL THEN s.name::text
                    WHEN w.id IS NOT NULL THEN w.name::text
                    WHEN cp.id IS NOT NULL THEN cp.name::text
                    ELSE 'unknown device'
                END AS device_name,
                COALESCE(w.room_network_id, p.room_network_id) AS room_network_id,
                COALESCE(rnw.network_id, pnw.network_id) AS network_id,
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
                COALESCE(nc.name, npc.name, 'unknown')::text AS network_name,
                COALESCE(nr.name, npr.name, 'unknown')::text AS network_region,
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
            LEFT JOIN positions p ON imm.position_id = p.id
            LEFT JOIN room_networks rnw ON w.room_network_id = rnw.id
            LEFT JOIN room_networks pnw ON p.room_network_id = pnw.id
            LEFT JOIN network_cidrs nc ON rnw.network_id = nc.id
            LEFT JOIN network_cidrs npc ON pnw.network_id = npc.id
            LEFT JOIN network_regions nr ON nc.network_region_id = nr.id
            LEFT JOIN network_regions npr ON npc.network_region_id = npr.id
            LEFT JOIN rooms r ON w.room_id = r.id
            LEFT JOIN positions cp ON imm.position_id = cp.id
            LEFT JOIN cabinets c ON cp.cabinet_id = c.id
            LEFT JOIN switches s ON s.position_id = cp.id
            LEFT JOIN switch_ports sp ON imm.switch_port_id = sp.id"
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
                p.room_network_id,
                rn.network_id as position_network_id,
                nc.network_region_id,
                s.description,
                'switch'::text as device_type,
                host(im.ip_address) as ip_address,
                im.mac_address,
                s.created_at, s.updated_at
            FROM switches s
            LEFT JOIN positions p ON s.position_id = p.id
            LEFT JOIN cabinets c ON p.cabinet_id = c.id
            LEFT JOIN rooms r ON c.room_id = r.id
            LEFT JOIN room_networks rn ON p.room_network_id = rn.id
            LEFT JOIN network_cidrs nc ON rn.network_id = nc.id
            LEFT JOIN LATERAL (
                SELECT ips.ip_address, ips.mac_address
                FROM ips
                WHERE ips.position_id = p.id
                LIMIT 1
            ) im ON true"
        )
        .execute(pool)
        .await
        {
            warn!("创建 switches_with_details 视图失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('update_views_for_device_network', '更新视图以从设备获取 room_network_id')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_remove_old_triggers(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'remove_old_sync_triggers'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        let triggers = vec![
            ("trg_sync_workstation_ips_room_network", "workstations"),
            ("trg_sync_cabinet_ips_room_network", "cabinets"),
            ("trg_sync_position_ips_room_network", "positions"),
        ];

        for (trigger, table) in triggers {
            if let Err(e) = sqlx::query(&format!(
                "DROP TRIGGER IF EXISTS {} ON {}", trigger, table
            ))
            .execute(pool)
            .await
            {
                warn!("删除触发器 {}.{} 失败: {}", table, trigger, e);
            }
        }

        let functions = vec![
            "sync_workstation_ips_room_network",
            "sync_cabinet_ips_room_network",
            "sync_position_ips_room_network",
        ];

        for func in functions {
            if let Err(e) = sqlx::query(&format!(
                "DROP FUNCTION IF EXISTS {}() CASCADE", func
            ))
            .execute(pool)
            .await
            {
                warn!("删除函数 {} 失败: {}", func, e);
            }
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('remove_old_sync_triggers', '删除旧的 IP 同步触发器，改为设备关联网络')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}
