-- 2026-08-28 统一端口模型：device_ports 合并进 device_interfaces
-- （现有部署直接执行；全新部署由 ipma-init 建表代码自动完成，无需执行本文件）
--
-- 背景：原代码严格区分「设备端口」（device_ports，交换机二层端口）与
-- 「设备网口」（device_interfaces，网卡-网口-IP 层级）。现将两表合并为
-- device_interfaces 统一模型：
--   * 新增 port_type / status / speed（二层属性，SNMP 维护）与
--     device_managed（是否在设备编辑模态框展示维护）四列；
--   * 存量 device_interfaces 一律视为设备模态框托管（device_managed=TRUE）；
--   * 存量 device_ports 迁移为非托管网口，并按端口名前缀自动生成板卡网卡
--     （如 xg1/0/0/1 的网卡为「设备名-xg1」，与运行期 nic.rs 的
--     port_group_prefix 规则一致）；
--   * cable_links 的 device_port 端点类型并入 device_interface；
--   * 拓扑表外键改指 device_interfaces 后删除 device_ports 表。

BEGIN;

-- 1. device_interfaces 扩展统一端口列
ALTER TABLE device_interfaces
    ADD COLUMN IF NOT EXISTS port_type VARCHAR(20) NOT NULL DEFAULT 'access'
        CHECK (port_type IN ('access', 'trunk', 'hybrid', 'uplink', 'stack', 'console')),
    ADD COLUMN IF NOT EXISTS status VARCHAR(20) NOT NULL DEFAULT 'up',
    ADD COLUMN IF NOT EXISTS speed VARCHAR(20),
    ADD COLUMN IF NOT EXISTS trunk_id INTEGER,
    ADD COLUMN IF NOT EXISTS device_managed BOOLEAN NOT NULL DEFAULT FALSE;

-- 存量网口全部来自设备模态框，标记为托管
UPDATE device_interfaces SET device_managed = TRUE;

-- 2. 计算每个端口的分组前缀（与 nic.rs port_group_prefix 一致：
--    含 / 取首段；否则去尾部数字；结果为空回退整名）
CREATE TEMP TABLE port_group_map ON COMMIT DROP AS
SELECT sp.id AS port_id, sp.device_id,
       COALESCE(
           NULLIF(split_part(sp.port_number, '/', 1), ''),
           NULLIF(regexp_replace(sp.port_number, '\d+$', ''), ''),
           sp.port_number
       ) AS grp
FROM device_ports sp;

-- 3. 按分组前缀生成板卡网卡（网卡名 = 设备名-前缀，超长截断到列宽 50）
INSERT INTO device_nics (id, device_id, name, card_type, sort_order, created_at, updated_at)
SELECT uuid_generate_v4(), m.device_id,
       left(d.name || '-' || m.grp, 50),
       'other', 1000, NOW(), NOW()
FROM (SELECT DISTINCT device_id, grp FROM port_group_map) m
JOIN devices d ON d.id = m.device_id
ON CONFLICT (device_id, name) DO NOTHING;

-- 4. 存量端口迁移为非托管网口（保留原 id/时间戳；port_name 并入描述）
INSERT INTO device_interfaces (
    id, device_id, nic_id, name, physical_type, interface_role,
    mac_address, vlan_id, description, sort_order,
    port_type, status, speed, device_managed, created_at, updated_at
)
SELECT sp.id, sp.device_id, n.id, sp.port_number, 'other',
       CASE WHEN sp.port_type = 'uplink' THEN 'uplink' ELSE 'business' END,
       NULL, sp.vlan_id,
       NULLIF(concat_ws(' - ',
           NULLIF(sp.port_name, ''),
           NULLIF(sp.description, '')), ''),
       0, sp.port_type, sp.status, sp.speed, FALSE,
       sp.created_at, sp.updated_at
FROM device_ports sp
JOIN port_group_map m ON m.port_id = sp.id
JOIN devices d ON d.id = sp.device_id
JOIN device_nics n ON n.device_id = sp.device_id
                  AND n.name = left(d.name || '-' || m.grp, 50)
ON CONFLICT (device_id, name) DO NOTHING;

-- 5. 端点类型合并前先禁用校验触发器：合并后按端点类型判断的
--    「两台设备直连」规则不再成立（任意两台不同设备允许直连），
--    迁移完成后按「同一设备的两个端口禁止互连（防环路）」重建规则
ALTER TABLE cable_links DISABLE TRIGGER trg_cable_links_validate_endpoints;

-- 6. cable_links 端点类型合并：device_port → device_interface，
--    随后修正 A/B 规范序（同类型时按 id 升序）满足 chk_endpoint_order
UPDATE cable_links SET a_endpoint_type = 'device_interface' WHERE a_endpoint_type = 'device_port';
UPDATE cable_links SET b_endpoint_type = 'device_interface' WHERE b_endpoint_type = 'device_port';
UPDATE cable_links cl
SET (a_endpoint_type, a_endpoint_id, b_endpoint_type, b_endpoint_id)
  = (cl.b_endpoint_type, cl.b_endpoint_id, cl.a_endpoint_type, cl.a_endpoint_id)
WHERE cl.a_endpoint_type > cl.b_endpoint_type
   OR (cl.a_endpoint_type = cl.b_endpoint_type AND cl.a_endpoint_id > cl.b_endpoint_id);

-- 7. 重建 cable_links 端点约束（去掉 device_port 枚举值）
ALTER TABLE cable_links DROP CONSTRAINT IF EXISTS cable_links_a_endpoint_type_check;
ALTER TABLE cable_links DROP CONSTRAINT IF EXISTS cable_links_b_endpoint_type_check;
ALTER TABLE cable_links
    ADD CONSTRAINT cable_links_a_endpoint_type_check
        CHECK (a_endpoint_type IN ('net_outlet', 'device_interface', 'patch_panel')),
    ADD CONSTRAINT cable_links_b_endpoint_type_check
        CHECK (b_endpoint_type IN ('net_outlet', 'device_interface', 'patch_panel'));

-- 8. 重建端点校验触发器函数（移除 device_port 分支；设备间直连一律放行，
--    仅禁止同一设备的两个端口互连以防环路，与 ipma-init cable_links.rs
--    保持同步），随后恢复触发器
CREATE OR REPLACE FUNCTION validate_cable_link_endpoints() RETURNS TRIGGER AS $$
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
$$ LANGUAGE plpgsql;

ALTER TABLE cable_links ENABLE TRIGGER trg_cable_links_validate_endpoints;

-- 9. 拓扑表外键改指 device_interfaces（原 8）
ALTER TABLE topology_connections DROP CONSTRAINT IF EXISTS topology_connections_source_device_port_id_fkey;
ALTER TABLE topology_connections DROP CONSTRAINT IF EXISTS topology_connections_target_device_port_id_fkey;
ALTER TABLE topology_connection_members DROP CONSTRAINT IF EXISTS topology_connection_members_device_port_id_fkey;
ALTER TABLE topology_connections
    ADD CONSTRAINT topology_connections_source_port_fkey
        FOREIGN KEY (source_device_port_id) REFERENCES device_interfaces(id) ON DELETE SET NULL,
    ADD CONSTRAINT topology_connections_target_port_fkey
        FOREIGN KEY (target_device_port_id) REFERENCES device_interfaces(id) ON DELETE SET NULL;
ALTER TABLE topology_connection_members
    ADD CONSTRAINT topology_connection_members_port_fkey
        FOREIGN KEY (device_port_id) REFERENCES device_interfaces(id) ON DELETE CASCADE;

-- 9. 重建线路视图（端点分支合并；列序与原视图保持一致）
DROP VIEW IF EXISTS cable_links_with_details;
CREATE VIEW cable_links_with_details AS
WITH endpoint_labels AS (
    SELECT id, 'net_outlet'::VARCHAR AS etype, name::text AS label,
           room_id, NULL::UUID AS cabinet_id, NULL::UUID AS device_id
    FROM net_outlets
    UNION ALL
    SELECT di.id, 'device_interface'::VARCHAR, (di.name || ' @ ' || d.name),
           d.room_id, cab.id, di.device_id
    FROM device_interfaces di
    JOIN devices d ON di.device_id = d.id
    LEFT JOIN positions p ON d.position_id = p.id
    LEFT JOIN cabinets cab ON p.cabinet_id = cab.id
    UNION ALL
    SELECT pp.id, 'patch_panel'::VARCHAR, pp.name::text,
           c.room_id, pp.cabinet_id, NULL::UUID
    FROM patch_panels pp
    JOIN cabinets c ON pp.cabinet_id = c.id
)
SELECT
    cl.id, cl.link_type, cl.cable_label, cl.length_m, cl.tested,
    cl.created_at, cl.updated_at,
    cl.a_endpoint_type, cl.a_endpoint_id,
    cl.b_endpoint_type, cl.b_endpoint_id,
    ael.label AS a_endpoint_label, ael.room_id AS a_room_id,
    ael.cabinet_id AS a_cabinet_id, ael.device_id AS a_device_id,
    bel.label AS b_endpoint_label, bel.room_id AS b_room_id,
    bel.cabinet_id AS b_cabinet_id, bel.device_id AS b_device_id
FROM cable_links cl
JOIN endpoint_labels ael ON ael.etype = cl.a_endpoint_type AND ael.id = cl.a_endpoint_id
JOIN endpoint_labels bel ON bel.etype = cl.b_endpoint_type AND bel.id = cl.b_endpoint_id;

-- 10. 重建线路路径函数（同设备接口互通的 internal 边改用 device_interfaces）
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
$$ LANGUAGE sql STABLE;

-- 11. 设备端口防删触发器与索引随表移除，随后删除 device_ports
DROP TRIGGER IF EXISTS trg_device_ports_prevent_delete_linked ON device_ports;
DROP FUNCTION IF EXISTS prevent_device_port_deletion_if_linked();
DROP TRIGGER IF EXISTS trg_device_ports_updated_at ON device_ports;
DROP INDEX IF EXISTS idx_device_ports_device_id;
DROP INDEX IF EXISTS idx_device_ports_port_number;
DROP TABLE device_ports;

-- 12. 统一模型辅助索引（托管网口过滤）
CREATE INDEX IF NOT EXISTS idx_device_interfaces_device_managed
    ON device_interfaces(device_managed) WHERE device_managed;

COMMIT;
