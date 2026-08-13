-- ============================================================
-- 信息点/配线架概念分离迁移脚本
--
-- 综合布线规范中信息点（网络插座）与配线架是两个不同概念：
--   1. net_outlets 精简为纯信息点（网络插座），仅隶属房间；
--   2. 新建 patch_panels 独立表，隶属机柜；
--   3. cable_links 的 patch_panel 端点改指 patch_panels 表。
--
-- 执行方式（先备份数据库）：
--   pg_dump -U postgres ipma > /tmp/ipma-backup.sql
--   sudo -u postgres psql -d ipma -f scripts/patch-panel-split.sql
-- ============================================================
\set ON_ERROR_STOP on

BEGIN;

-- 以 ipma 角色执行，保证新建对象属主与既有对象一致
SET ROLE ipma;

-- ------------------------------------------------------------
-- 1. 前置检查：被线路引用但丢失机柜的配线架无法迁移，需人工处理
-- ------------------------------------------------------------
DO $$
DECLARE
    orphan_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO orphan_count
    FROM net_outlets no
    WHERE no.outlet_type = 'patch_panel'
      AND no.cabinet_id IS NULL
      AND EXISTS (
          SELECT 1 FROM cable_links cl
          WHERE (cl.a_endpoint_type IN ('net_outlet', 'patch_panel') AND cl.a_endpoint_id = no.id)
             OR (cl.b_endpoint_type IN ('net_outlet', 'patch_panel') AND cl.b_endpoint_id = no.id)
      );
    IF orphan_count > 0 THEN
        RAISE EXCEPTION '存在 % 个被线路引用但未关联机柜的配线架，请先人工指定机柜后再执行迁移', orphan_count;
    END IF;
END $$;

-- ------------------------------------------------------------
-- 2. 数据修正：endpoint_type='net_outlet' 实际指向配线架行的端点
--    改为 'patch_panel'，并按 chk_endpoint_order（类型、id 字典序）重排 A/B 端
--    （触发器校验时行仍在 net_outlets 中，可通过）
-- ------------------------------------------------------------
WITH retype AS (
    SELECT cl.id,
           CASE WHEN cl.a_endpoint_type = 'net_outlet' AND noa.outlet_type = 'patch_panel'
                THEN 'patch_panel' ELSE cl.a_endpoint_type END AS ta,
           cl.a_endpoint_id AS ia,
           CASE WHEN cl.b_endpoint_type = 'net_outlet' AND nob.outlet_type = 'patch_panel'
                THEN 'patch_panel' ELSE cl.b_endpoint_type END AS tb,
           cl.b_endpoint_id AS ib
    FROM cable_links cl
    LEFT JOIN net_outlets noa ON cl.a_endpoint_id = noa.id
    LEFT JOIN net_outlets nob ON cl.b_endpoint_id = nob.id
    WHERE (cl.a_endpoint_type = 'net_outlet' AND noa.outlet_type = 'patch_panel')
       OR (cl.b_endpoint_type = 'net_outlet' AND nob.outlet_type = 'patch_panel')
)
UPDATE cable_links cl
SET a_endpoint_type = CASE WHEN r.ta < r.tb OR (r.ta = r.tb AND r.ia < r.ib) THEN r.ta ELSE r.tb END,
    a_endpoint_id   = CASE WHEN r.ta < r.tb OR (r.ta = r.tb AND r.ia < r.ib) THEN r.ia ELSE r.ib END,
    b_endpoint_type = CASE WHEN r.ta < r.tb OR (r.ta = r.tb AND r.ia < r.ib) THEN r.tb ELSE r.ta END,
    b_endpoint_id   = CASE WHEN r.ta < r.tb OR (r.ta = r.tb AND r.ia < r.ib) THEN r.ib ELSE r.ia END
FROM retype r
WHERE cl.id = r.id;

-- ------------------------------------------------------------
-- 3. 新建配线架表（隶属机柜，机柜删除时级联删除，
--    被线路引用的配线架由删除保护触发器阻止）
-- ------------------------------------------------------------
CREATE TABLE IF NOT EXISTS patch_panels (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(100) NOT NULL,
    cabinet_id UUID NOT NULL REFERENCES cabinets(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_patch_panels_name UNIQUE (cabinet_id, name)
);

CREATE INDEX IF NOT EXISTS idx_patch_panels_cabinet_id ON patch_panels(cabinet_id);

-- ------------------------------------------------------------
-- 4. 迁移配线架数据（保留原 id，线路引用不断链）
-- ------------------------------------------------------------
INSERT INTO patch_panels (id, name, cabinet_id, created_at, updated_at)
SELECT no.id, no.name, no.cabinet_id, no.created_at, no.updated_at
FROM net_outlets no
WHERE no.outlet_type = 'patch_panel' AND no.cabinet_id IS NOT NULL
ON CONFLICT (id) DO NOTHING;

-- ------------------------------------------------------------
-- 5. 删除 net_outlets 中的配线架行（含未被引用的孤儿行）
-- ------------------------------------------------------------
DELETE FROM net_outlets WHERE outlet_type = 'patch_panel';

-- ------------------------------------------------------------
-- 6. net_outlets 精简为纯信息点（网络插座）
--    （依赖 net_outlets 旧列的视图先行删除，末尾重建）
-- ------------------------------------------------------------
DROP VIEW IF EXISTS net_outlets_with_details CASCADE;
DROP VIEW IF EXISTS cable_links_with_details CASCADE;

DROP INDEX IF EXISTS idx_net_outlets_cabinet_id;
ALTER TABLE net_outlets DROP CONSTRAINT IF EXISTS chk_outlet_type;
ALTER TABLE net_outlets DROP COLUMN IF EXISTS outlet_type;
ALTER TABLE net_outlets DROP COLUMN IF EXISTS cabinet_id;

-- ------------------------------------------------------------
-- 7. 线路端点校验：patch_panel 端点改查 patch_panels 表
-- ------------------------------------------------------------
CREATE OR REPLACE FUNCTION validate_cable_link_endpoints() RETURNS TRIGGER AS $$
DECLARE
    endpoint_exists BOOLEAN := FALSE;
    iface_type VARCHAR(20);
BEGIN
    CASE NEW.a_endpoint_type
        WHEN 'device_port' THEN
            SELECT EXISTS(SELECT 1 FROM device_ports WHERE id = NEW.a_endpoint_id) INTO endpoint_exists;
        WHEN 'net_outlet' THEN
            SELECT EXISTS(SELECT 1 FROM net_outlets WHERE id = NEW.a_endpoint_id) INTO endpoint_exists;
        WHEN 'device_interface' THEN
            SELECT interface_type FROM device_interfaces WHERE id = NEW.a_endpoint_id INTO iface_type;
            endpoint_exists := iface_type IS NOT NULL AND iface_type IN ('physical','wifi');
            IF iface_type IS NOT NULL AND NOT endpoint_exists THEN
                RAISE EXCEPTION 'A 端点 device_interface 类型必须为 physical/wifi，实际为 % (id=%)',
                    iface_type, NEW.a_endpoint_id;
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
    iface_type := NULL;
    CASE NEW.b_endpoint_type
        WHEN 'device_port' THEN
            SELECT EXISTS(SELECT 1 FROM device_ports WHERE id = NEW.b_endpoint_id) INTO endpoint_exists;
        WHEN 'net_outlet' THEN
            SELECT EXISTS(SELECT 1 FROM net_outlets WHERE id = NEW.b_endpoint_id) INTO endpoint_exists;
        WHEN 'device_interface' THEN
            SELECT interface_type FROM device_interfaces WHERE id = NEW.b_endpoint_id INTO iface_type;
            endpoint_exists := iface_type IS NOT NULL AND iface_type IN ('physical','wifi');
            IF iface_type IS NOT NULL AND NOT endpoint_exists THEN
                RAISE EXCEPTION 'B 端点 device_interface 类型必须为 physical/wifi，实际为 % (id=%)',
                    iface_type, NEW.b_endpoint_id;
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
        RAISE EXCEPTION '不允许两台设备直连，必须经过交换机或信息点';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ------------------------------------------------------------
-- 8. 信息点删除保护：仅保护 net_outlet 类型引用
-- ------------------------------------------------------------
CREATE OR REPLACE FUNCTION prevent_net_outlet_deletion_if_linked() RETURNS TRIGGER AS $$
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
$$ LANGUAGE plpgsql;

-- ------------------------------------------------------------
-- 9. 配线架删除保护（级联删除时同样触发，保护被线路引用的配线架）
-- ------------------------------------------------------------
CREATE OR REPLACE FUNCTION prevent_patch_panel_deletion_if_linked() RETURNS TRIGGER AS $$
BEGIN
    IF EXISTS(
        SELECT 1 FROM cable_links
        WHERE (a_endpoint_type='patch_panel' AND a_endpoint_id = OLD.id)
           OR (b_endpoint_type='patch_panel' AND b_endpoint_id = OLD.id)
    ) THEN
        RAISE EXCEPTION '配线架 % 被 cable_links 引用，不能删除', OLD.id;
    END IF;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_patch_panels_prevent_delete_linked ON patch_panels;
CREATE TRIGGER trg_patch_panels_prevent_delete_linked BEFORE DELETE ON patch_panels
FOR EACH ROW EXECUTE FUNCTION prevent_patch_panel_deletion_if_linked();

DROP TRIGGER IF EXISTS trg_patch_panels_updated_at ON patch_panels;
CREATE TRIGGER trg_patch_panels_updated_at BEFORE UPDATE ON patch_panels
FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ------------------------------------------------------------
-- 10. find_cable_path：patch_panel 标签改查 patch_panels
-- ------------------------------------------------------------
DROP FUNCTION IF EXISTS find_cable_path(VARCHAR, UUID, VARCHAR, UUID);

CREATE OR REPLACE FUNCTION find_cable_path(
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
    SELECT sp1.id, 'device_port', sp2.id, 'device_port', NULL::UUID, NULL::VARCHAR, 'internal'::VARCHAR
    FROM device_ports sp1
    JOIN device_ports sp2 ON sp1.device_id = sp2.device_id AND sp1.id <> sp2.id
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
        WHEN 'device_port'      THEN (SELECT sp.port_number || ' @ ' || d.name FROM device_ports sp JOIN devices d ON sp.device_id = d.id WHERE sp.id = p.node_id)
        WHEN 'net_outlet'       THEN (SELECT name FROM net_outlets WHERE id = p.node_id)
        WHEN 'device_interface' THEN (SELECT di.name || ' @ ' || d.name FROM device_interfaces di JOIN devices d ON di.device_id = d.id WHERE di.id = p.node_id)
        WHEN 'patch_panel'      THEN (SELECT name FROM patch_panels WHERE id = p.node_id)
    END AS node_label,
    p.cable_id,
    p.cable_label,
    p.hop_type
FROM path_cte p
ORDER BY p.hop_idx;
$$ LANGUAGE sql STABLE;

-- ------------------------------------------------------------
-- 11. 视图重建（旧视图已在第 6 步删除）
-- ------------------------------------------------------------
CREATE VIEW net_outlets_with_details AS
SELECT
    ap.id, ap.name, ap.room_id,
    r.name AS room_name,
    ap.created_at, ap.updated_at
FROM net_outlets ap
LEFT JOIN rooms r ON ap.room_id = r.id;

GRANT SELECT ON net_outlets_with_details TO ipma;

CREATE VIEW cable_links_with_details AS
WITH endpoint_labels AS (
    SELECT sp.id, 'device_port'::VARCHAR AS etype,
           (sp.port_number || ' @ ' || d.name) AS label
    FROM device_ports sp JOIN devices d ON sp.device_id = d.id
    UNION ALL
    SELECT id, 'net_outlet'::VARCHAR, name::text FROM net_outlets
    UNION ALL
    SELECT di.id, 'device_interface'::VARCHAR, (di.name || ' @ ' || d.name)
    FROM device_interfaces di JOIN devices d ON di.device_id = d.id
    UNION ALL
    SELECT pp.id, 'patch_panel'::VARCHAR, pp.name::text
    FROM patch_panels pp
)
SELECT
    cl.id, cl.link_type, cl.cable_label, cl.length_m, cl.tested,
    cl.created_at, cl.updated_at,
    cl.a_endpoint_type, cl.a_endpoint_id,
    cl.b_endpoint_type, cl.b_endpoint_id,
    a_lbl.label AS a_endpoint_label,
    b_lbl.label AS b_endpoint_label
FROM cable_links cl
LEFT JOIN endpoint_labels a_lbl ON cl.a_endpoint_id = a_lbl.id AND cl.a_endpoint_type = a_lbl.etype
LEFT JOIN endpoint_labels b_lbl ON cl.b_endpoint_id = b_lbl.id AND cl.b_endpoint_type = b_lbl.etype;

GRANT SELECT ON cable_links_with_details TO ipma;

DROP VIEW IF EXISTS patch_panels_with_details CASCADE;

CREATE VIEW patch_panels_with_details AS
SELECT
    pp.id, pp.name, pp.cabinet_id,
    c.name AS cabinet_name,
    c.room_id,
    r.name AS room_name,
    pp.created_at, pp.updated_at
FROM patch_panels pp
JOIN cabinets c ON pp.cabinet_id = c.id
LEFT JOIN rooms r ON c.room_id = r.id;

GRANT SELECT ON patch_panels_with_details TO ipma;

COMMIT;

RESET ROLE;
