use sqlx::PgPool;
use sqlx::Error;
use sqlx::Row;
use tracing::warn;

pub async fn run(pool: &PgPool) -> Result<(), Error> {
    migrate_room_fk_to_restrict(pool).await?;
    migrate_remove_device_id(pool).await?;
    migrate_remove_cabinet_layouts_room_id(pool).await?;
    migrate_workstation_layouts_simplify(pool).await?;
    migrate_add_ip_sync_trigger(pool).await?;
    migrate_cleanup_duplicate_constraints(pool).await?;
    Ok(())
}

async fn migrate_room_fk_to_restrict(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'room_fk_to_restrict'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            r"DO $$
            BEGIN
                IF EXISTS (
                    SELECT 1 FROM pg_constraint 
                    WHERE conname = 'cabinets_room_id_fkey' 
                    AND contype = 'f'
                ) THEN
                    ALTER TABLE cabinets DROP CONSTRAINT cabinets_room_id_fkey;
                END IF;
            END $$"
        )
        .execute(pool)
        .await
        {
            warn!("删除cabinets旧外键约束失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "ALTER TABLE cabinets ADD CONSTRAINT cabinets_room_id_fkey 
             FOREIGN KEY (room_id) REFERENCES rooms(id) ON DELETE RESTRICT"
        )
        .execute(pool)
        .await
        {
            warn!("添加cabinets新外键约束失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"DO $$
            BEGIN
                IF EXISTS (
                    SELECT 1 FROM pg_constraint 
                    WHERE conname = 'workstations_room_id_fkey' 
                    AND contype = 'f'
                ) THEN
                    ALTER TABLE workstations DROP CONSTRAINT workstations_room_id_fkey;
                END IF;
            END $$"
        )
        .execute(pool)
        .await
        {
            warn!("删除workstations旧外键约束失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "ALTER TABLE workstations ADD CONSTRAINT workstations_room_id_fkey 
             FOREIGN KEY (room_id) REFERENCES rooms(id) ON DELETE RESTRICT"
        )
        .execute(pool)
        .await
        {
            warn!("添加workstations新外键约束失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('room_fk_to_restrict', '修改cabinets和workstations的room_id外键为RESTRICT，防止删除有设备的房间')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_remove_device_id(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'remove_device_id'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "ALTER TABLE positions DROP COLUMN IF EXISTS device_id"
        )
        .execute(pool)
        .await
        {
            warn!("删除positions.device_id列失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('remove_device_id', '删除positions.device_id冗余字段，交换机通过switches.position_id单向关联')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_remove_cabinet_layouts_room_id(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'remove_cabinet_layouts_room_id'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "ALTER TABLE cabinet_layouts DROP COLUMN IF EXISTS room_id"
        )
        .execute(pool)
        .await
        {
            warn!("删除cabinet_layouts.room_id列失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "ALTER TABLE cabinet_layouts DROP CONSTRAINT IF EXISTS cabinet_layouts_room_cabinet_key"
        )
        .execute(pool)
        .await
        {
            warn!("删除cabinet_layouts唯一约束失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "DROP INDEX IF EXISTS idx_cabinet_layouts_room_id"
        )
        .execute(pool)
        .await
        {
            warn!("删除cabinet_layouts.room_id索引失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('remove_cabinet_layouts_room_id', '删除cabinet_layouts.room_id冗余字段，改用JOIN查询确保数据一致性')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_workstation_layouts_simplify(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'workstation_layouts_simplify'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "ALTER TABLE workstation_layouts RENAME COLUMN element_id TO workstation_id"
        )
        .execute(pool)
        .await
        {
            warn!("重命名element_id为workstation_id失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "ALTER TABLE workstation_layouts DROP COLUMN IF EXISTS element_type"
        )
        .execute(pool)
        .await
        {
            warn!("删除element_type列失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            r"DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM pg_constraint 
                    WHERE conname = 'fk_workstation_layouts_workstation'
                ) THEN
                    ALTER TABLE workstation_layouts 
                    ADD CONSTRAINT fk_workstation_layouts_workstation 
                    FOREIGN KEY (workstation_id) REFERENCES workstations(id) ON DELETE CASCADE;
                END IF;
            END $$"
        )
        .execute(pool)
        .await
        {
            warn!("添加外键约束失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('workstation_layouts_simplify', '简化workstation_layouts表，重命名element_id为workstation_id，添加外键约束')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_add_ip_sync_trigger(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'add_ip_sync_trigger'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            r"CREATE OR REPLACE FUNCTION sync_workstation_ips_room_network()
            RETURNS TRIGGER AS $$
            BEGIN
                IF NEW.room_id IS DISTINCT FROM OLD.room_id THEN
                    UPDATE ips i
                    SET room_network_id = (
                        SELECT rn.id
                        FROM room_networks rn
                        JOIN network_cidrs nc ON rn.network_id = nc.id
                        WHERE rn.room_id = NEW.room_id
                        AND (
                            (nc.ipv4_cidr IS NOT NULL AND i.ip_address <<= nc.ipv4_cidr::inet)
                            OR (nc.ipv6_cidr IS NOT NULL AND i.ip_address <<= nc.ipv6_cidr::inet)
                        )
                        LIMIT 1
                    )
                    WHERE i.workstation_id = NEW.id;
                END IF;
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql"
        )
        .execute(pool)
        .await
        {
            warn!("创建sync_workstation_ips_room_network函数失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "DROP TRIGGER IF EXISTS trg_sync_workstation_ips_room_network ON workstations"
        )
        .execute(pool)
        .await
        {
            warn!("删除旧触发器失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "CREATE TRIGGER trg_sync_workstation_ips_room_network
            AFTER UPDATE OF room_id ON workstations
            FOR EACH ROW EXECUTE FUNCTION sync_workstation_ips_room_network()"
        )
        .execute(pool)
        .await
        {
            warn!("创建trg_sync_workstation_ips_room_network触发器失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('add_ip_sync_trigger', '添加触发器自动同步工位IP的room_network_id')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_cleanup_duplicate_constraints(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'cleanup_duplicate_constraints'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "ALTER TABLE workstation_layouts DROP CONSTRAINT IF EXISTS workstation_layouts_room_id_element_id_key"
        )
        .execute(pool)
        .await
        {
            warn!("删除workstation_layouts重复约束失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "DROP INDEX IF EXISTS idx_positions_device_id"
        )
        .execute(pool)
        .await
        {
            warn!("删除positions.device_id索引失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) 
             VALUES ('cleanup_duplicate_constraints', '清理workstation_layouts重复约束和positions.device_id索引')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}
