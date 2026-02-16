DROP VIEW IF EXISTS ip_managers_with_details;

CREATE VIEW ip_managers_with_details AS
SELECT 
    imm.id,
    imm.workstation_id,
    imm.position_id,
    imm.switch_id,
    imm.switch_port_id,
    imm.device_type,
    CASE
        WHEN w.id IS NOT NULL THEN w.name::text
        WHEN cp.id IS NOT NULL THEN cp.name::text
        ELSE '未知设备'
    END AS device_name,
    imm.network_id,
    CASE
        WHEN w.id IS NOT NULL THEN w.name::text
        ELSE NULL
    END AS workstation_name,
    CASE
        WHEN cp.id IS NOT NULL THEN cp.name::text
        ELSE NULL
    END AS cabinet_position_name,
    CASE
        WHEN s.id IS NOT NULL THEN s.name::text
        ELSE NULL
    END AS switch_name,
    sp.port_number::text AS switch_port_number,
    COALESCE(n.name, '未知')::text AS network_name,
    COALESCE(nt.name, '未知')::text AS network_region,
    imm.ip_address,
    imm.ip_version,
    imm.mac_address,
    imm.hostname,
    imm.status,
    imm.last_seen,
    imm.created_at,
    imm.updated_at
FROM ip_managers imm
LEFT JOIN workstations w ON imm.workstation_id = w.id
LEFT JOIN rooms r ON w.room_id = r.id
LEFT JOIN positions cp ON imm.position_id = cp.id
LEFT JOIN cabinets c ON cp.cabinet_id = c.id
LEFT JOIN switches s ON imm.switch_id = s.id
LEFT JOIN switch_ports sp ON imm.switch_port_id = sp.id
LEFT JOIN network_cidrs n ON imm.network_id = n.id
LEFT JOIN network_regions nt ON n.network_region_id = nt.id;
