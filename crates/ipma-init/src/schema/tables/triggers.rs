//! 跨表触发器创建。

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
        "net_outlets",
        "patch_panels",
        "devices",
        "device_ports",
        "device_interfaces",
        "device_macs",
        "device_lldps",
        "cable_links",
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

    sqlx::query(
        r"
        CREATE OR REPLACE FUNCTION validate_device_room_consistency() RETURNS TRIGGER AS $$
        DECLARE
          ws_room_id UUID;
          pos_room_id UUID;
        BEGIN
          IF NEW.room_id IS NULL THEN
            RAISE EXCEPTION '设备必须属于一个房间 (room_id 不能为空)';
          END IF;

          IF NEW.workstation_id IS NOT NULL THEN
            SELECT room_id INTO ws_room_id FROM workstations WHERE id = NEW.workstation_id;
            IF ws_room_id IS DISTINCT FROM NEW.room_id THEN
              RAISE EXCEPTION '工位所属房间 (%) 与设备 room_id (%) 不一致', ws_room_id, NEW.room_id;
            END IF;
          END IF;

          IF NEW.position_id IS NOT NULL THEN
            SELECT c.room_id INTO pos_room_id
            FROM positions p JOIN cabinets c ON p.cabinet_id = c.id
            WHERE p.id = NEW.position_id;
            IF pos_room_id IS DISTINCT FROM NEW.room_id THEN
              RAISE EXCEPTION '机位所属机房 (%) 与设备 room_id (%) 不一致', pos_room_id, NEW.room_id;
            END IF;
          END IF;

          RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
        ",
    )
    .execute(pool)
    .await?;

    sqlx::query("DROP TRIGGER IF EXISTS trg_validate_device_room_consistency ON devices")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TRIGGER trg_validate_device_room_consistency BEFORE INSERT OR UPDATE OF room_id, workstation_id, position_id ON devices FOR EACH ROW EXECUTE FUNCTION validate_device_room_consistency()"
    )
    .execute(pool)
    .await?;

    Ok(())
}
