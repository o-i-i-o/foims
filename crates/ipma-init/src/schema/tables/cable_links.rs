use tracing::warn;

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS cable_links (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            a_endpoint_type VARCHAR(20) NOT NULL CHECK (a_endpoint_type IN ('switch_port','net_outlet','device_interface')),
            a_endpoint_id   UUID NOT NULL,
            b_endpoint_type VARCHAR(20) NOT NULL CHECK (b_endpoint_type IN ('switch_port','net_outlet','device_interface')),
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

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_switch_ports_device_id ON switch_ports(device_id)")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_switch_ports_port_number ON switch_ports(port_number)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_device_interfaces_device_id ON device_interfaces(device_id)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_device_interfaces_mac ON device_interfaces(mac_address) WHERE mac_address IS NOT NULL",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_ips_device_interface_id ON ips(device_interface_id)",
    )
    .execute(pool)
    .await?;

    create_triggers(pool).await?;
    create_path_function(pool).await?;

    Ok(())
}

async fn create_triggers(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE OR REPLACE FUNCTION validate_cable_link_endpoints() RETURNS TRIGGER AS $$
        DECLARE
            endpoint_exists BOOLEAN := FALSE;
            iface_type VARCHAR(20);
        BEGIN
            CASE NEW.a_endpoint_type
                WHEN 'switch_port' THEN
                    SELECT EXISTS(SELECT 1 FROM switch_ports WHERE id = NEW.a_endpoint_id) INTO endpoint_exists;
                WHEN 'net_outlet' THEN
                    SELECT EXISTS(SELECT 1 FROM net_outlets WHERE id = NEW.a_endpoint_id) INTO endpoint_exists;
                WHEN 'device_interface' THEN
                    SELECT interface_type FROM device_interfaces WHERE id = NEW.a_endpoint_id INTO iface_type;
                    endpoint_exists := iface_type IS NOT NULL AND iface_type IN ('physical','wifi');
                    IF iface_type IS NOT NULL AND NOT endpoint_exists THEN
                        RAISE EXCEPTION 'A 端点 device_interface 类型必须为 physical/wifi，实际为 % (id=%)',
                            iface_type, NEW.a_endpoint_id;
                    END IF;
                ELSE
                    RAISE EXCEPTION '未知的 a_endpoint_type: %', NEW.a_endpoint_type;
            END CASE;

            IF NOT endpoint_exists THEN
                RAISE EXCEPTION 'A 端点不存在: type=%, id=%', NEW.a_endpoint_type, NEW.a_endpoint_id;
            END IF;

            endpoint_exists := FALSE;
            iface_type := NULL;
            CASE NEW.b_endpoint_type
                WHEN 'switch_port' THEN
                    SELECT EXISTS(SELECT 1 FROM switch_ports WHERE id = NEW.b_endpoint_id) INTO endpoint_exists;
                WHEN 'net_outlet' THEN
                    SELECT EXISTS(SELECT 1 FROM net_outlets WHERE id = NEW.b_endpoint_id) INTO endpoint_exists;
                WHEN 'device_interface' THEN
                    SELECT interface_type FROM device_interfaces WHERE id = NEW.b_endpoint_id INTO iface_type;
                    endpoint_exists := iface_type IS NOT NULL AND iface_type IN ('physical','wifi');
                    IF iface_type IS NOT NULL AND NOT endpoint_exists THEN
                        RAISE EXCEPTION 'B 端点 device_interface 类型必须为 physical/wifi，实际为 % (id=%)',
                            iface_type, NEW.b_endpoint_id;
                    END IF;
                ELSE
                    RAISE EXCEPTION '未知的 b_endpoint_type: %', NEW.b_endpoint_type;
            END CASE;

            IF NOT endpoint_exists THEN
                RAISE EXCEPTION 'B 端点不存在: type=%, id=%', NEW.b_endpoint_type, NEW.b_endpoint_id;
            END IF;

            IF NEW.a_endpoint_type = 'device_interface' AND NEW.b_endpoint_type = 'device_interface' THEN
                RAISE EXCEPTION '不允许两台设备直连，必须经过交换机或信息点';
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

    sqlx::query(
        r"CREATE OR REPLACE FUNCTION prevent_switch_port_deletion_if_linked() RETURNS TRIGGER AS $$
        BEGIN
            IF EXISTS(
                SELECT 1 FROM cable_links
                WHERE (a_endpoint_type='switch_port' AND a_endpoint_id = OLD.id)
                   OR (b_endpoint_type='switch_port' AND b_endpoint_id = OLD.id)
            ) THEN
                RAISE EXCEPTION '交换机端口 % 被 cable_links 引用，不能删除', OLD.id;
            END IF;
            RETURN OLD;
        END;
        $$ LANGUAGE plpgsql;",
    )
    .execute(pool)
    .await?;
    sqlx::query("DROP TRIGGER IF EXISTS trg_switch_ports_prevent_delete_linked ON switch_ports")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TRIGGER trg_switch_ports_prevent_delete_linked BEFORE DELETE ON switch_ports FOR EACH ROW EXECUTE FUNCTION prevent_switch_port_deletion_if_linked()"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE OR REPLACE FUNCTION prevent_net_outlet_deletion_if_linked() RETURNS TRIGGER AS $$
        BEGIN
            IF EXISTS(
                SELECT 1 FROM cable_links
                WHERE (a_endpoint_type='net_outlet' AND a_endpoint_id = OLD.id)
                   OR (b_endpoint_type='net_outlet' AND b_endpoint_id = OLD.id)
            ) THEN
                RAISE EXCEPTION '信息点 % 被 cable_links 引用，不能删除', OLD.id;
            END IF;
            RETURN OLD;
        END;
        $$ LANGUAGE plpgsql;",
    )
    .execute(pool)
    .await?;
    sqlx::query("DROP TRIGGER IF EXISTS trg_net_outlets_prevent_delete_linked ON net_outlets")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TRIGGER trg_net_outlets_prevent_delete_linked BEFORE DELETE ON net_outlets FOR EACH ROW EXECUTE FUNCTION prevent_net_outlet_deletion_if_linked()"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE OR REPLACE FUNCTION prevent_interface_deletion_if_linked() RETURNS TRIGGER AS $$
        BEGIN
            IF OLD.interface_type IN ('physical','wifi') AND EXISTS(
                SELECT 1 FROM cable_links
                WHERE (a_endpoint_type='device_interface' AND a_endpoint_id = OLD.id)
                   OR (b_endpoint_type='device_interface' AND b_endpoint_id = OLD.id)
            ) THEN
                RAISE EXCEPTION '设备接口 % 被 cable_links 引用，不能删除', OLD.id;
            END IF;
            RETURN OLD;
        END;
        $$ LANGUAGE plpgsql;",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "DROP TRIGGER IF EXISTS trg_device_interfaces_prevent_delete_linked ON device_interfaces",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TRIGGER trg_device_interfaces_prevent_delete_linked BEFORE DELETE ON device_interfaces FOR EACH ROW EXECUTE FUNCTION prevent_interface_deletion_if_linked()"
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_path_function(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let sql = r"CREATE OR REPLACE FUNCTION find_cable_path(
        p_from_type VARCHAR, p_from_id UUID,
        p_to_type   VARCHAR, p_to_id   UUID
    ) RETURNS TABLE(
        hop_idx     INTEGER,
        node_type   VARCHAR,
        node_id     UUID,
        node_label  TEXT,
        cable_id    UUID,
        cable_label VARCHAR
    ) AS $$
    WITH RECURSIVE path_cte AS (
        SELECT
            0 AS hop_idx,
            p_from_type::VARCHAR AS node_type,
            p_from_id   AS node_id,
            NULL::UUID  AS cable_id,
            NULL::VARCHAR AS cable_label
        UNION ALL
        SELECT
            pc.hop_idx + 1,
            CASE
                WHEN cl.a_endpoint_id = pc.node_id THEN cl.b_endpoint_type
                ELSE cl.a_endpoint_type
            END AS node_type,
            CASE
                WHEN cl.a_endpoint_id = pc.node_id THEN cl.b_endpoint_id
                ELSE cl.a_endpoint_id
            END AS node_id,
            cl.id,
            cl.cable_label
        FROM path_cte pc
        JOIN cable_links cl ON
            cl.a_endpoint_id = pc.node_id OR cl.b_endpoint_id = pc.node_id
        WHERE pc.hop_idx < 20
          AND NOT (pc.node_type = p_to_type AND pc.node_id = p_to_id)
    )
    SELECT
        p.hop_idx,
        p.node_type,
        p.node_id,
        CASE p.node_type
            WHEN 'switch_port'      THEN (SELECT sp.port_number || ' @ ' || d.name FROM switch_ports sp JOIN devices d ON sp.device_id = d.id WHERE sp.id = p.node_id)
            WHEN 'net_outlet'       THEN (SELECT name FROM net_outlets WHERE id = p.node_id)
            WHEN 'device_interface' THEN (SELECT di.name || ' @ ' || d.name FROM device_interfaces di JOIN devices d ON di.device_id = d.id WHERE di.id = p.node_id)
        END AS node_label,
        p.cable_id,
        p.cable_label
    FROM path_cte p
    ORDER BY p.hop_idx;
    $$ LANGUAGE sql STABLE;";

    if let Err(e) = sqlx::query(sql).execute(pool).await {
        warn!("find_cable_path 函数创建失败: {}", e);
        return Err(e);
    }

    Ok(())
}
