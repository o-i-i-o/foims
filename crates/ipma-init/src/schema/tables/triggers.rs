use tracing::warn;

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        CREATE OR REPLACE FUNCTION update_updated_at_column()
        RETURNS TRIGGER AS $$
        BEGIN
            NEW.updated_at = NOW();
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
    ",
    )
    .execute(pool)
    .await?;

    let tables_with_updated_at = [
        "users",
        "network_regions",
        "network_cidrs",
        "rooms",
        "room_networks",
        "workstation_layouts",
        "cabinets",
        "positions",
        "workstations",
        "device_templates",
        "access_points",
        "devices",
        "switch_ports",
        "switch_macs",
        "switch_lldps",
        "ips",
        "cabinet_layouts",
        "system_configs",
        "scheduled_tasks",
    ];

    for table in &tables_with_updated_at {
        let trigger_name = format!("trg_{table}_updated_at");
        if let Err(e) = sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE TRIGGER {trigger_name} BEFORE UPDATE ON {table} FOR EACH ROW EXECUTE FUNCTION update_updated_at_column()"
        )))
        .execute(pool)
        .await
        {
            warn!("updated_at触发器创建失败（可能已存在） {}: {}", table, e);
        }
    }

    sqlx::query(
        r"
        CREATE OR REPLACE FUNCTION check_position_overlap() RETURNS TRIGGER AS $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM positions 
                WHERE cabinet_id = NEW.cabinet_id 
                AND id != NEW.id
                AND (
                    (NEW.start_u BETWEEN start_u AND end_u)
                    OR (NEW.end_u BETWEEN start_u AND end_u)
                    OR (start_u BETWEEN NEW.start_u AND NEW.end_u)
                    OR (end_u BETWEEN NEW.start_u AND NEW.end_u)
                )
            ) THEN
                RAISE EXCEPTION '机位U位范围重叠';
            END IF;
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
    ",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query("DROP TRIGGER IF EXISTS trg_check_position_overlap ON positions")
        .execute(pool)
        .await
    {
        warn!("删除旧触发器失败: {}", e);
    }

    sqlx::query(
        "CREATE TRIGGER trg_check_position_overlap BEFORE INSERT OR UPDATE ON positions FOR EACH ROW EXECUTE FUNCTION check_position_overlap()"
    )
    .execute(pool)
    .await?;

    Ok(())
}
