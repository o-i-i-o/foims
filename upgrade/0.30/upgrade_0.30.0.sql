-- FOIMS 0.29.x → 0.30.x 数据库升级脚本（0.30 中版本唯一一份）
-- 执行前置：停止 foims 服务、完成数据库备份。
-- 变更概览：
--   1. 删除 interface_ips.description 列：IP 备注描述从产品能力中整体移除
--      （设备网卡编辑器不再提供 IP 描述输入，独立 IP 管理页/可视化/导入导出
--      均未使用该字段），后端模型、SQL、视图、导入导出 spec 同步下线。
--   2. 视图 ip_with_details 依赖该列，须先 DROP 再重建（PostgreSQL 不允许在
--      视图列中间删除，仅可在列尾追加；此处为删除列，故整体重建）。
--      重建后的视图列顺序与旧版一致，仅少了 description。
--   说明：foims-init 建表代码（schema/tables/ips.rs、views.rs）与 check.rs
--   interface_ips 列清单已同步删除 description。

BEGIN;

-- 视图依赖 description 列，先删除视图再删除列
DROP VIEW IF EXISTS ip_with_details;

ALTER TABLE interface_ips DROP COLUMN IF EXISTS description;

CREATE VIEW ip_with_details AS
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
