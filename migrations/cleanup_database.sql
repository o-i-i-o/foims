-- 数据库清理迁移脚本
-- 目的：移除冗余字段和表，统一使用 ip_managers 管理 IP/MAC 和交换机端口关联

-- ============================================
-- 第一步：备份现有数据到 ip_managers
-- ============================================

-- 将 switches 表中的 IP/MAC 数据迁移到 ip_managers（如果不存在）
INSERT INTO ip_managers (id, switch_id, device_type, network_id, ip_address, ip_version, mac_address, status, last_seen, created_at, updated_at)
SELECT 
    uuid_generate_v4(),
    s.id,
    'switch',
    s.network_id,
    s.ip_address,
    CASE WHEN family(s.ip_address) = 6 THEN 6 ELSE 4 END,
    s.mac_address,
    'active',
    NOW(),
    NOW(),
    NOW()
FROM switches s
WHERE s.ip_address IS NOT NULL 
AND NOT EXISTS (
    SELECT 1 FROM ip_managers im 
    WHERE im.switch_id = s.id AND im.device_type = 'switch'
);

-- ============================================
-- 第二步：删除冗余的关联表
-- ============================================

-- 删除 workstation_ports 表（交换机端口关联已在 ip_managers.switch_port_id）
DROP TABLE IF EXISTS workstation_ports CASCADE;

-- 删除 position_ports 表（交换机端口关联已在 ip_managers.switch_port_id）
DROP TABLE IF EXISTS position_ports CASCADE;

-- 删除 workstation_networks 表（网段关联已在 ip_managers.network_id）
DROP TABLE IF EXISTS workstation_networks CASCADE;

-- 删除 position_networks 表（网段关联已在 ip_managers.network_id）
DROP TABLE IF EXISTS position_networks CASCADE;

-- 删除 system_configs_backup 表（备份表）
DROP TABLE IF EXISTS system_configs_backup CASCADE;

-- ============================================
-- 第三步：修改 switches 表结构
-- ============================================

-- 删除 switches 表中的冗余字段
ALTER TABLE switches DROP COLUMN IF EXISTS ip_address;
ALTER TABLE switches DROP COLUMN IF EXISTS mac_address;
ALTER TABLE switches DROP COLUMN IF EXISTS management_ip;

-- 删除相关索引
DROP INDEX IF EXISTS idx_switches_ip_address;
DROP INDEX IF EXISTS switches_ip_address_key;

-- ============================================
-- 第四步：修改 rooms 表结构
-- ============================================

-- rooms 表已有关联 room_networks 表，无需修改

-- ============================================
-- 第五步：修改 cabinets 表结构
-- ============================================

-- 添加继承网段字段（如果需要）
-- cabinet_networks 表已存在，用于机柜默认网段

-- ============================================
-- 第六步：更新视图
-- ============================================

-- 更新 ip_managers_with_details 视图
DROP VIEW IF EXISTS ip_managers_with_details CASCADE;

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
        WHEN s.id IS NOT NULL THEN s.name::text
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
    COALESCE(nr.name, '未知')::text AS network_region,
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
LEFT JOIN positions cp ON imm.position_id = cp.id
LEFT JOIN switches s ON imm.switch_id = s.id
LEFT JOIN switch_ports sp ON imm.switch_port_id = sp.id
LEFT JOIN network_cidrs n ON imm.network_id = n.id
LEFT JOIN network_regions nr ON n.network_region_id = nr.id;

-- ============================================
-- 第七步：清理无用数据
-- ============================================

-- 删除没有关联任何设备的 ip_managers 记录
DELETE FROM ip_managers 
WHERE workstation_id IS NULL 
  AND position_id IS NULL 
  AND switch_id IS NULL;

-- ============================================
-- 完成
-- ============================================
