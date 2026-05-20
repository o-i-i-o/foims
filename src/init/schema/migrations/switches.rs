use sqlx::PgPool;
use sqlx::Error;
use sqlx::Row;
use tracing::warn;

pub async fn run(pool: &PgPool) -> Result<(), Error> {
    migrate_switch_position_fields(pool).await?;
    migrate_switch_view_add_room(pool).await?;
    migrate_switch_parent_columns_removal(pool).await?;
    migrate_fix_switch_view_duplicate(pool).await?;
    Ok(())
}

async fn migrate_switch_position_fields(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'switch_position_fields'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('switch_position_fields', 'switches表通过position_id关联positions获取位置信息，不再使用cabinet_id/start_u/end_u字段')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_switch_view_add_room(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'switch_view_add_room'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            r"CREATE OR REPLACE VIEW switches_with_details AS
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
                p.description as position_description,
                s.description,
                s.created_at, s.updated_at
            FROM switches s
            LEFT JOIN positions p ON s.position_id = p.id
            LEFT JOIN cabinets c ON p.cabinet_id = c.id
            LEFT JOIN rooms r ON c.room_id = r.id"
        )
        .execute(pool)
        .await
        {
            warn!("创建 switches_with_details 视图失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('switch_view_add_room', '更新switches_with_details视图添加room_id和room_name')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_switch_parent_columns_removal(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'switch_parent_columns_removal'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query(
            "ALTER TABLE switches DROP COLUMN IF EXISTS parent_switch_id"
        )
        .execute(pool)
        .await
        {
            warn!("删除switches.parent_switch_id列失败: {}", e);
        }

        if let Err(e) = sqlx::query(
            "ALTER TABLE switches DROP COLUMN IF EXISTS parent_port_id"
        )
        .execute(pool)
        .await
        {
            warn!("删除switches.parent_port_id列失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('switch_parent_columns_removal', '删除switches表的parent_switch_id和parent_port_id列')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_fix_switch_view_duplicate(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'fix_switch_view_duplicate'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        if let Err(e) = sqlx::query("DROP VIEW IF EXISTS switches_with_details CASCADE")
            .execute(pool)
            .await
        {
            warn!("删除switches_with_details视图失败: {}", e);
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
                p.description as position_description,
                s.description,
                'switch'::text as device_type,
                host(im.ip_address) as ip_address,
                im.mac_address,
                s.created_at, s.updated_at
            FROM switches s
            LEFT JOIN positions p ON s.position_id = p.id
            LEFT JOIN cabinets c ON p.cabinet_id = c.id
            LEFT JOIN rooms r ON c.room_id = r.id
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
            warn!("修复switches_with_details视图失败: {}", e);
        }

        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('fix_switch_view_duplicate', '修复switches_with_details视图，移除room_networks JOIN避免重复记录')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}
