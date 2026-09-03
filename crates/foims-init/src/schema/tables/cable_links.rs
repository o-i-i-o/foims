//! 物理链路表（cable_links）结构、触发器与路径查询函数。
//!
//! 防删触发器保证被链路引用的端点资源（信息点/配线架/设备接口）
//! 不可删除；`validate_cable_link_endpoints` 校验端点存在性与物理形态
//! （接口必须为实际连接器，非 virtual），并禁止同一设备的两个端口
//! 互连（防自环）。任意两台不同设备允许直连。触发器函数体与
//! `scripts/sql/2026-08-28-unified-device-interfaces.sql` 等变更脚本
//! 保持一致，修改需同步。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS cable_links (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            a_endpoint_type VARCHAR(20) NOT NULL CHECK (a_endpoint_type IN ('net_outlet','device_interface','patch_panel')),
            a_endpoint_id   UUID NOT NULL,
            b_endpoint_type VARCHAR(20) NOT NULL CHECK (b_endpoint_type IN ('net_outlet','device_interface','patch_panel')),
            b_endpoint_id   UUID NOT NULL,
            link_type   VARCHAR(20) NOT NULL DEFAULT 'ethernet' CHECK (link_type IN ('ethernet','fiber','console')),
            cable_label VARCHAR(50),
            length_m    DOUBLE PRECISION,
            tested      BOOLEAN NOT NULL DEFAULT FALSE,
            created_at  TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at  TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_no_self_link CHECK (NOT (a_endpoint_type=b_endpoint_type AND a_endpoint_id=b_endpoint_id)),
            CONSTRAINT chk_endpoint_order CHECK (
                a_endpoint_type < b_endpoint_type OR
                (a_endpoint_type = b_endpoint_type AND a_endpoint_id < b_endpoint_id)
            )
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_cable_links_a ON cable_links(a_endpoint_type, a_endpoint_id)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_cable_links_b ON cable_links(b_endpoint_type, b_endpoint_id)",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_cable_links_link_type ON cable_links(link_type)")
        .execute(pool)
        .await?;

    // 物理限制：同两端点之间只允许一根线路（去重）
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS uq_cable_links_endpoint_pair ON cable_links(a_endpoint_type, a_endpoint_id, b_endpoint_type, b_endpoint_id)",
    )
    .execute(pool)
    .await?;

    create_triggers(pool).await?;
    create_path_function(pool).await?;

    Ok(())
}

/// 创建链路端点校验触发器（A/B 端点存在性 + 接口物理形态 + 禁止同设备端口互连）。
async fn create_triggers(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE OR REPLACE FUNCTION validate_cable_link_endpoints() RETURNS TRIGGER AS $$
        DECLARE
            endpoint_exists BOOLEAN := FALSE;
            iface_ptype VARCHAR(20);
            a_dev_id UUID;
            b_dev_id UUID;
        BEGIN
            CASE NEW.a_endpoint_type
                WHEN 'net_outlet' THEN
                    SELECT EXISTS(SELECT 1 FROM net_outlets WHERE id = NEW.a_endpoint_id) INTO endpoint_exists;
                WHEN 'device_interface' THEN
                    SELECT physical_type FROM device_interfaces WHERE id = NEW.a_endpoint_id INTO iface_ptype;
                    endpoint_exists := iface_ptype IS NOT NULL AND iface_ptype <> 'virtual';
                    IF iface_ptype IS NOT NULL AND NOT endpoint_exists THEN
                        RAISE EXCEPTION 'A 端点 device_interface 物理形态必须为实际连接器（非 virtual），实际为 % (id=%)',
                            iface_ptype, NEW.a_endpoint_id;
                    END IF;
                WHEN 'patch_panel' THEN
                    SELECT EXISTS(SELECT 1 FROM patch_panels WHERE id = NEW.a_endpoint_id) INTO endpoint_exists;
                ELSE
                    RAISE EXCEPTION '未知的 a_endpoint_type: %', NEW.a_endpoint_type;
            END CASE;

            IF NOT endpoint_exists THEN
                RAISE EXCEPTION 'A 端点不存在: type=%, id=%', NEW.a_endpoint_type, NEW.a_endpoint_id;
            END IF;

            endpoint_exists := FALSE;
            iface_ptype := NULL;
            CASE NEW.b_endpoint_type
                WHEN 'net_outlet' THEN
                    SELECT EXISTS(SELECT 1 FROM net_outlets WHERE id = NEW.b_endpoint_id) INTO endpoint_exists;
                WHEN 'device_interface' THEN
                    SELECT physical_type FROM device_interfaces WHERE id = NEW.b_endpoint_id INTO iface_ptype;
                    endpoint_exists := iface_ptype IS NOT NULL AND iface_ptype <> 'virtual';
                    IF iface_ptype IS NOT NULL AND NOT endpoint_exists THEN
                        RAISE EXCEPTION 'B 端点 device_interface 物理形态必须为实际连接器（非 virtual），实际为 % (id=%)',
                            iface_ptype, NEW.b_endpoint_id;
                    END IF;
                WHEN 'patch_panel' THEN
                    SELECT EXISTS(SELECT 1 FROM patch_panels WHERE id = NEW.b_endpoint_id) INTO endpoint_exists;
                ELSE
                    RAISE EXCEPTION '未知的 b_endpoint_type: %', NEW.b_endpoint_type;
            END CASE;

            IF NOT endpoint_exists THEN
                RAISE EXCEPTION 'B 端点不存在: type=%, id=%', NEW.b_endpoint_type, NEW.b_endpoint_id;
            END IF;

            IF NEW.a_endpoint_type = 'device_interface' AND NEW.b_endpoint_type = 'device_interface' THEN
                SELECT device_id INTO a_dev_id FROM device_interfaces WHERE id = NEW.a_endpoint_id;
                SELECT device_id INTO b_dev_id FROM device_interfaces WHERE id = NEW.b_endpoint_id;
                IF a_dev_id IS NOT NULL AND a_dev_id = b_dev_id THEN
                    RAISE EXCEPTION '不允许同一台设备的两个端口互相直连（可能形成环路）: device=%, port_a=%, port_b=%',
                        a_dev_id, NEW.a_endpoint_id, NEW.b_endpoint_id;
                END IF;
            END IF;

            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;",
    )
    .execute(pool)
    .await?;

    sqlx::query("DROP TRIGGER IF EXISTS trg_cable_links_validate_endpoints ON cable_links")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TRIGGER trg_cable_links_validate_endpoints BEFORE INSERT OR UPDATE OF a_endpoint_type, a_endpoint_id, b_endpoint_type, b_endpoint_id ON cable_links FOR EACH ROW EXECUTE FUNCTION validate_cable_link_endpoints()"
    )
    .execute(pool)
    .await?;

    // 端点资源防删触发器：(端点类型, 所在表, 函数名, 触发器名, 报错文案资源名, 附加守卫)
    // 附加守卫用于放行无需保护的记录（如 virtual 接口未被链路引用语义覆盖）
    const PREVENT_DELETION_TRIGGERS: &[(&str, &str, &str, &str, &str, &str)] = &[
        (
            "net_outlet",
            "net_outlets",
            "prevent_net_outlet_deletion_if_linked",
            "trg_net_outlets_prevent_delete_linked",
            "信息点",
            "TRUE",
        ),
        (
            "patch_panel",
            "patch_panels",
            "prevent_patch_panel_deletion_if_linked",
            "trg_patch_panels_prevent_delete_linked",
            "配线架",
            "TRUE",
        ),
        (
            "device_interface",
            "device_interfaces",
            "prevent_interface_deletion_if_linked",
            "trg_device_interfaces_prevent_delete_linked",
            "设备接口",
            "OLD.physical_type <> 'virtual'",
        ),
    ];

    for (endpoint_type, table, func_name, trigger_name, label, guard) in PREVENT_DELETION_TRIGGERS {
        // 标识符与文案均来自上方内部常量，拼入 DDL 无注入风险
        let func = format!(
            r"CREATE OR REPLACE FUNCTION {func_name}() RETURNS TRIGGER AS $$
            BEGIN
                IF {guard} AND EXISTS(
                    SELECT 1 FROM cable_links
                    WHERE (a_endpoint_type='{endpoint_type}' AND a_endpoint_id = OLD.id)
                       OR (b_endpoint_type='{endpoint_type}' AND b_endpoint_id = OLD.id)
                ) THEN
                    RAISE EXCEPTION 'ERR_CABLE_LINK_REFERENCE: {label} % 被 cable_links 引用，不能删除', OLD.id;
                END IF;
                RETURN OLD;
            END;
            $$ LANGUAGE plpgsql;"
        );
        sqlx::query(sqlx::AssertSqlSafe(func)).execute(pool).await?;

        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TRIGGER IF EXISTS {trigger_name} ON {table}"
        )))
        .execute(pool)
        .await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE TRIGGER {trigger_name} BEFORE DELETE ON {table} FOR EACH ROW EXECUTE FUNCTION {func_name}()"
        )))
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// 创建线缆路径查询函数（递归 CTE，BFS 逐跳返回，深度上限 20）。
async fn create_path_function(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("DROP FUNCTION IF EXISTS find_cable_path(VARCHAR, UUID, VARCHAR, UUID)")
        .execute(pool)
        .await?;

    sqlx::query(
        r"CREATE OR REPLACE FUNCTION find_cable_path(
        p_from_type VARCHAR, p_from_id UUID,
        p_to_type   VARCHAR, p_to_id   UUID
    ) RETURNS TABLE(
        hop_idx     INTEGER,
        node_type   VARCHAR,
        node_id     UUID,
        node_label  TEXT,
        cable_id    UUID,
        cable_label VARCHAR,
        hop_type    VARCHAR
    ) AS $$
    WITH RECURSIVE
    all_edges AS (
        SELECT a_endpoint_id AS from_node, a_endpoint_type AS from_type,
               b_endpoint_id AS to_node,   b_endpoint_type AS to_type,
               id AS cable_id, cable_label, 'cable'::VARCHAR AS hop_type
        FROM cable_links
        UNION ALL
        SELECT b_endpoint_id, b_endpoint_type, a_endpoint_id, a_endpoint_type, id, cable_label, 'cable'::VARCHAR
        FROM cable_links
        UNION ALL
        SELECT di1.id, 'device_interface', di2.id, 'device_interface', NULL::UUID, NULL::VARCHAR, 'internal'::VARCHAR
        FROM device_interfaces di1
        JOIN device_interfaces di2 ON di1.device_id = di2.device_id AND di1.id <> di2.id
    ),
    path_cte AS (
        SELECT
            0 AS hop_idx,
            p_from_type::VARCHAR AS node_type,
            p_from_id   AS node_id,
            NULL::UUID  AS cable_id,
            NULL::VARCHAR AS cable_label,
            'start'::VARCHAR AS hop_type,
            ARRAY[p_from_id]::UUID[] AS visited
        UNION ALL
        SELECT
            pc.hop_idx + 1,
            e.to_type,
            e.to_node,
            e.cable_id,
            e.cable_label,
            e.hop_type,
            pc.visited || ARRAY[e.to_node]
        FROM path_cte pc
        JOIN all_edges e ON e.from_node = pc.node_id AND e.from_type = pc.node_type
        WHERE pc.hop_idx < 20
          AND NOT (pc.node_type = p_to_type AND pc.node_id = p_to_id)
          AND NOT (e.to_node = ANY(pc.visited))
    )
    SELECT
        p.hop_idx,
        p.node_type,
        p.node_id,
        CASE p.node_type
            WHEN 'net_outlet'       THEN (SELECT name FROM net_outlets WHERE id = p.node_id)
            WHEN 'device_interface' THEN (SELECT di.name || ' @ ' || d.name FROM device_interfaces di JOIN devices d ON di.device_id = d.id WHERE di.id = p.node_id)
            WHEN 'patch_panel'      THEN (SELECT name FROM patch_panels WHERE id = p.node_id)
        END AS node_label,
        p.cable_id,
        p.cable_label,
        p.hop_type
    FROM path_cte p
    ORDER BY p.hop_idx;
    $$ LANGUAGE sql STABLE;",
    )
    .execute(pool)
    .await?;

    Ok(())
}
