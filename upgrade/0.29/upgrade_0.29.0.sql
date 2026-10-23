-- FOIMS 0.28.x → 0.29.x 数据库升级脚本（0.29 中版本唯一一份）
-- 执行前置：停止 foims 服务、完成数据库备份。本脚本使用 CREATE OR REPLACE
-- VIEW，可重复执行（幂等）：PostgreSQL 仅允许在列尾追加新列，
-- 已有列名/类型/顺序必须保持不变。
-- 变更概览：
--   1. 视图 ip_with_details 末尾追加 workstation_id（设备绑定的工位 ID）：
--      工位可视化原按 IP 行的 workstation_id 归并 IP 展示，而视图只暴露
--      workstation_name（名称不可作匹配键），导致工位画布 IP 恒为空；
--      补列后 /api/resources/ip 行可直接携带该字段。
--   说明：本次无表结构（列/索引/约束）变更，foims-init 建表代码与 check.rs
--   视图存在性清单已同步（视图列不在 check.rs 校验范围）。

BEGIN;

CREATE OR REPLACE VIEW ip_with_details AS
SELECT
    imm.id,
    imm.device_interface_id,
    dv.id AS device_id,
    imm.subnet_id,
    dv.name::text AS device_name,
    dv.device_type::text AS device_type,
    dv.hostname::text AS hostname,
    di.name::text AS interface_name,
    di.physical_type::text AS physical_type,
    di.interface_role::text AS interface_role,
    di.mac_address AS mac_address,
    w.name::text AS workstation_name,
    cp.name::text AS cabinet_position_name,
    r.name::text AS room_name,
    c.name::text AS cabinet_name,
    org.name::text AS org_name,
    COALESCE(nc.name, 'unknown')::text AS network_name,
    COALESCE(nr.name, 'unknown')::text AS network_region,
    host(imm.ip_address) as ip_address,
    imm.ip_version,
    imm.description,
    imm.status,
    imm.last_seen,
    imm.created_at,
    imm.updated_at,
    dv.position_id,
    -- 工位归属 ID：工位可视化按工位归并 IP 展示（名称列无法充当键）
    dv.workstation_id
FROM interface_ips imm
JOIN device_interfaces di ON imm.device_interface_id = di.id
JOIN devices dv ON di.device_id = dv.id
LEFT JOIN workstations w ON dv.workstation_id = w.id
LEFT JOIN positions cp ON dv.position_id = cp.id
LEFT JOIN cabinets c ON cp.cabinet_id = c.id
LEFT JOIN rooms r ON dv.room_id = r.id
LEFT JOIN organizations org ON r.org_id = org.id
LEFT JOIN network_cidrs nc ON imm.subnet_id = nc.id
LEFT JOIN network_regions nr ON nc.network_region_id = nr.id;

COMMIT;
