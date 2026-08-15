-- ============================================================
-- 网口类型拆分迁移脚本：interface_type → physical_type + interface_role
--
-- 原单一「网口类型」枚举混淆了物理形态与接口角色两个维度，拆分为：
--   1. physical_type 物理形态：rj45/sfp/sfp_plus/sfp28/qsfp_plus/qsfp28/wifi/virtual/other
--   2. interface_role 接口角色：management/business/loopback/uplink/other
--
-- 旧值映射：
--   physical   → rj45  + business
--   management → rj45  + management
--   svi        → virtual + business
--   loopback   → virtual + loopback
--   wifi       → wifi  + business
--
-- 执行方式（先备份数据库）：
--   pg_dump -U postgres ipma > /tmp/ipma-backup.sql
--   sudo -u postgres psql -d ipma -f scripts/port-type-split.sql
-- ============================================================
\set ON_ERROR_STOP on

BEGIN;

-- 以 ipma 角色执行，保证新建对象属主与既有对象一致
SET ROLE ipma;

-- ------------------------------------------------------------
-- 1. ip_with_details 视图引用 interface_type，先删除，迁移后重建
-- ------------------------------------------------------------
DROP VIEW IF EXISTS ip_with_details CASCADE;

-- ------------------------------------------------------------
-- 2. 新增两列（先按默认值填充，再按旧值映射修正）
-- ------------------------------------------------------------
ALTER TABLE device_interfaces
    ADD COLUMN IF NOT EXISTS physical_type VARCHAR(20) NOT NULL DEFAULT 'rj45';
ALTER TABLE device_interfaces
    ADD COLUMN IF NOT EXISTS interface_role VARCHAR(20) NOT NULL DEFAULT 'business';

UPDATE device_interfaces SET physical_type = 'rj45',  interface_role = 'business'    WHERE interface_type = 'physical';
UPDATE device_interfaces SET physical_type = 'rj45',  interface_role = 'management' WHERE interface_type = 'management';
UPDATE device_interfaces SET physical_type = 'virtual', interface_role = 'business'  WHERE interface_type = 'svi';
UPDATE device_interfaces SET physical_type = 'virtual', interface_role = 'loopback'  WHERE interface_type = 'loopback';
UPDATE device_interfaces SET physical_type = 'wifi',  interface_role = 'business'    WHERE interface_type = 'wifi';

-- ------------------------------------------------------------
-- 3. 删除旧列及其 CHECK 约束，为新列补 CHECK 约束
-- ------------------------------------------------------------
ALTER TABLE device_interfaces DROP CONSTRAINT IF EXISTS device_interfaces_interface_type_check;
ALTER TABLE device_interfaces DROP COLUMN IF EXISTS interface_type;

ALTER TABLE device_interfaces DROP CONSTRAINT IF EXISTS chk_device_interfaces_physical_type;
ALTER TABLE device_interfaces
    ADD CONSTRAINT chk_device_interfaces_physical_type CHECK (physical_type IN (
        'rj45', 'sfp', 'sfp_plus', 'sfp28', 'qsfp_plus', 'qsfp28', 'wifi', 'virtual', 'other'
    ));

ALTER TABLE device_interfaces DROP CONSTRAINT IF EXISTS chk_device_interfaces_interface_role;
ALTER TABLE device_interfaces
    ADD CONSTRAINT chk_device_interfaces_interface_role CHECK (interface_role IN (
        'management', 'business', 'loopback', 'uplink', 'other'
    ));

-- ------------------------------------------------------------
-- 4. 重建 ip_with_details 视图（与 crates/ipma-init views.rs 保持一致）
-- ------------------------------------------------------------
CREATE VIEW ip_with_details AS
SELECT
    imm.id,
    imm.device_interface_id,
    imm.device_id,
    imm.network_id,
    dv.name::text AS device_name,
    dv.device_type::text AS device_type,
    di.name::text AS interface_name,
    di.physical_type::text AS physical_type,
    di.interface_role::text AS interface_role,
    w.name::text AS workstation_name,
    cp.name::text AS cabinet_position_name,
    r.name::text AS room_name,
    c.name::text AS cabinet_name,
    org.name::text AS org_name,
    COALESCE(nc.name, 'unknown')::text AS network_name,
    COALESCE(nr.name, 'unknown')::text AS network_region,
    host(imm.ip_address) as ip_address,
    imm.ip_version,
    imm.mac_address,
    imm.last_mac,
    imm.hostname,
    imm.description,
    imm.status,
    imm.last_seen,
    imm.created_at,
    imm.updated_at
FROM ips imm
JOIN devices dv ON imm.device_id = dv.id
LEFT JOIN device_interfaces di ON imm.device_interface_id = di.id
LEFT JOIN workstations w ON dv.workstation_id = w.id
LEFT JOIN positions cp ON dv.position_id = cp.id
LEFT JOIN cabinets c ON cp.cabinet_id = c.id
LEFT JOIN rooms r ON dv.room_id = r.id
LEFT JOIN organizations org ON r.org_id = org.id
LEFT JOIN network_cidrs nc ON imm.network_id = nc.id
LEFT JOIN network_regions nr ON nc.network_region_id = nr.id;

GRANT SELECT ON ip_with_details TO ipma;

-- ------------------------------------------------------------
-- 5. 更新 cable_links 相关触发器函数（线缆端点必须是物理形态接口）
-- ------------------------------------------------------------
CREATE OR REPLACE FUNCTION validate_cable_link_endpoints() RETURNS TRIGGER AS $$
DECLARE
    endpoint_exists BOOLEAN := FALSE;
    iface_ptype VARCHAR(20);
BEGIN
    CASE NEW.a_endpoint_type
        WHEN 'device_port' THEN
            SELECT EXISTS(SELECT 1 FROM device_ports WHERE id = NEW.a_endpoint_id) INTO endpoint_exists;
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
        WHEN 'device_port' THEN
            SELECT EXISTS(SELECT 1 FROM device_ports WHERE id = NEW.b_endpoint_id) INTO endpoint_exists;
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
        RAISE EXCEPTION '不允许两台设备直连，必须经过交换机或信息点';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION prevent_interface_deletion_if_linked() RETURNS TRIGGER AS $$
BEGIN
    IF OLD.physical_type <> 'virtual' AND EXISTS(
        SELECT 1 FROM cable_links
        WHERE (a_endpoint_type='device_interface' AND a_endpoint_id = OLD.id)
           OR (b_endpoint_type='device_interface' AND b_endpoint_id = OLD.id)
    ) THEN
        RAISE EXCEPTION '设备接口 % 被 cable_links 引用，不能删除', OLD.id;
    END IF;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

COMMIT;

-- 验证：两列的取值分布应只含新枚举值
SELECT physical_type, interface_role, COUNT(*) AS cnt
FROM device_interfaces
GROUP BY physical_type, interface_role
ORDER BY cnt DESC;
