-- ============================================================
-- cable_links_with_details 视图扩展：为每个端点附带作用域信息
-- （room_id / cabinet_id / device_id），用于编辑模态框回填级联选择器。
--
-- 执行方式：sudo -u postgres psql -d ipma -f scripts/cable-link-view-v2.sql
-- ============================================================
\set ON_ERROR_STOP on

BEGIN;
SET ROLE ipma;

DROP VIEW IF EXISTS cable_links_with_details CASCADE;

CREATE VIEW cable_links_with_details AS
WITH endpoint_labels AS (
    SELECT sp.id, 'device_port'::VARCHAR AS etype,
           (sp.port_number || ' @ ' || d.name) AS label,
           d.room_id, cab.id AS cabinet_id, sp.device_id
    FROM device_ports sp
    JOIN devices d ON sp.device_id = d.id
    LEFT JOIN positions p ON d.position_id = p.id
    LEFT JOIN cabinets cab ON p.cabinet_id = cab.id
    UNION ALL
    SELECT id, 'net_outlet'::VARCHAR, name::text, room_id, NULL::UUID, NULL::UUID
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
    a_lbl.label AS a_endpoint_label,
    a_lbl.room_id AS a_room_id,
    a_lbl.cabinet_id AS a_cabinet_id,
    a_lbl.device_id AS a_device_id,
    b_lbl.label AS b_endpoint_label,
    b_lbl.room_id AS b_room_id,
    b_lbl.cabinet_id AS b_cabinet_id,
    b_lbl.device_id AS b_device_id
FROM cable_links cl
LEFT JOIN endpoint_labels a_lbl ON cl.a_endpoint_id = a_lbl.id AND cl.a_endpoint_type = a_lbl.etype
LEFT JOIN endpoint_labels b_lbl ON cl.b_endpoint_id = b_lbl.id AND cl.b_endpoint_type = b_lbl.etype;

GRANT SELECT ON cable_links_with_details TO ipma;

COMMIT;
RESET ROLE;
