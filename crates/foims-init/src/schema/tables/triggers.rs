//! 跨表触发器创建。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        CREATE OR REPLACE FUNCTION update_updated_at_column()
        RETURNS TRIGGER AS $$
        BEGIN
            -- 仅当语句未显式为 updated_at 指定与原值不同的值时自动刷新：
            -- 显式 SET updated_at = <新值> 时 NEW 与 OLD 不同，保留语句给的值；
            -- 未设置该列的正常 UPDATE 中 NEW 与 OLD 相同，刷新为 now()
            IF NEW.updated_at IS NOT DISTINCT FROM OLD.updated_at THEN
                NEW.updated_at = NOW();
            END IF;
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
    ",
    )
    .execute(pool)
    .await?;

    // 清单覆盖所有带 updated_at 列的表（与 schema/tables/ 逐表核对），
    // 遗漏会导致导入 upsert/业务更新后 updated_at 不刷新
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
        "element_layouts",
        "org_templates",
        "organizations",
        "employees",
        "device_templates",
        "net_outlets",
        "patch_panels",
        "devices",
        "device_nics",
        "device_interfaces",
        "snmp_arp_entries",
        "device_lldps",
        "cable_links",
        "interface_ips",
        "topology_nodes",
        "topology_connections",
        "cabinet_layouts",
        "system_configs",
        // scheduled_tasks 例外：全部手动 UPDATE 已显式设置 updated_at，
        // 调度路径（next_run_at/last_run_at 前移）刻意不刷新——挂触发器
        // 会把"配置变更时间"语义破坏为每 5 分钟跳动
    ];

    for table in &tables_with_updated_at {
        let trigger_name = format!("trg_{table}_updated_at");
        // 表名/触发器名来自上方内部常量清单，经 QueryBuilder push 拼接
        //（CREATE TRIGGER 不支持参数绑定）
        if let Err(e) = sqlx::QueryBuilder::<sqlx::Postgres>::new("CREATE TRIGGER ")
            .push(trigger_name)
            .push(" BEFORE UPDATE ON ")
            .push(table)
            .push(" FOR EACH ROW EXECUTE FUNCTION update_updated_at_column()")
            .build()
            .execute(pool)
            .await
        {
            foims_common::log_warn!("log.init.trigger_create_failed", table = table, error = e);
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
        foims_common::log_warn!("log.init.trigger_drop_failed", error = e);
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

    // 父侧换房的兜底校验（定向补齐 I1）：设备 room_id 是工位/机位所在房间
    // 的去规范化副本，父侧（cabinets/workstations/positions）换房后必须同步。
    // 应用层 sync_devices_room 负责真正的搬迁；此处用延迟约束触发器在
    // COMMIT 时把漏搬的事务整笔拒掉，不依赖语句先后顺序
    //（BEFORE 行触发器逐行校验，无法看到父侧变更后的全局状态）。
    sqlx::query(
        r"
        CREATE OR REPLACE FUNCTION check_devices_follow_parent_room() RETURNS TRIGGER AS $$
        DECLARE
          target_room UUID;
          mismatched BIGINT;
        BEGIN
          IF TG_TABLE_NAME = 'cabinets' THEN
            IF NEW.room_id IS NOT DISTINCT FROM OLD.room_id THEN
              RETURN NULL;
            END IF;
            target_room := NEW.room_id;
            SELECT count(*) INTO mismatched
              FROM devices d JOIN positions p ON d.position_id = p.id
              WHERE p.cabinet_id = NEW.id AND d.room_id IS DISTINCT FROM target_room;
          ELSIF TG_TABLE_NAME = 'workstations' THEN
            IF NEW.room_id IS NOT DISTINCT FROM OLD.room_id THEN
              RETURN NULL;
            END IF;
            target_room := NEW.room_id;
            SELECT count(*) INTO mismatched
              FROM devices d
              WHERE d.workstation_id = NEW.id AND d.room_id IS DISTINCT FROM target_room;
          ELSE
            IF NEW.cabinet_id IS NOT DISTINCT FROM OLD.cabinet_id THEN
              RETURN NULL;
            END IF;
            -- 机位未归属机柜时无房间约束（room_id 由设备自身维护）
            IF NEW.cabinet_id IS NULL THEN
              RETURN NULL;
            END IF;
            SELECT c.room_id INTO target_room FROM cabinets c WHERE c.id = NEW.cabinet_id;
            SELECT count(*) INTO mismatched
              FROM devices d
              WHERE d.position_id = NEW.id AND d.room_id IS DISTINCT FROM target_room;
          END IF;

          IF mismatched > 0 THEN
            RAISE EXCEPTION '设备 room_id 未跟随所属空间房间（% 行）', mismatched;
          END IF;
          RETURN NULL;
        END;
        $$ LANGUAGE plpgsql;
        ",
    )
    .execute(pool)
    .await?;

    for (table, trigger_name) in [
        ("cabinets", "trg_cabinets_devices_room"),
        ("workstations", "trg_workstations_devices_room"),
        ("positions", "trg_positions_devices_room"),
    ] {
        if let Err(e) = sqlx::QueryBuilder::<sqlx::Postgres>::new("DROP TRIGGER IF EXISTS ")
            .push(trigger_name)
            .push(" ON ")
            .push(table)
            .build()
            .execute(pool)
            .await
        {
            foims_common::log_warn!("log.init.trigger_drop_failed", table = table, error = e);
        }
        if let Err(e) = sqlx::QueryBuilder::<sqlx::Postgres>::new("CREATE CONSTRAINT TRIGGER ")
            .push(trigger_name)
            .push(" AFTER UPDATE ON ")
            .push(table)
            .push(
                " DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION check_devices_follow_parent_room()",
            )
            .build()
            .execute(pool)
            .await
        {
            foims_common::log_warn!("log.init.trigger_create_failed", table = table, error = e);
        }
    }

    // 工位管理人必须与工位所在房间同属一个组织（管理人可空：未指定责任人的
    // 工位不参与校验）。员工的组织不可变更（EmployeeUpdate 不含 org_id），
    // 因此只需在被引用侧（workstations）守住建不变量；应用层由
    // foims-resource::helpers::ensure_employee_in_org 提供同口径预检，
    // 把非法引用从 500 降为 400。
    sqlx::query(
        r"
        CREATE OR REPLACE FUNCTION validate_workstation_manager_org() RETURNS TRIGGER AS $$
        DECLARE
          manager_org_id UUID;
          room_org_id UUID;
        BEGIN
          IF NEW.manager_employee_id IS NULL THEN
            RETURN NEW;
          END IF;
          SELECT org_id INTO manager_org_id FROM employees WHERE id = NEW.manager_employee_id;
          SELECT org_id INTO room_org_id FROM rooms WHERE id = NEW.room_id;
          IF manager_org_id IS DISTINCT FROM room_org_id THEN
            RAISE EXCEPTION '工位管理人所属组织 (%) 与房间组织 (%) 不一致', manager_org_id, room_org_id;
          END IF;
          RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
        ",
    )
    .execute(pool)
    .await?;

    sqlx::query("DROP TRIGGER IF EXISTS trg_validate_workstation_manager_org ON workstations")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TRIGGER trg_validate_workstation_manager_org BEFORE INSERT OR UPDATE OF room_id, manager_employee_id ON workstations FOR EACH ROW EXECUTE FUNCTION validate_workstation_manager_org()",
    )
    .execute(pool)
    .await?;

    Ok(())
}
