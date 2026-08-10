-- 迁移脚本：移除 net_outlets 的 peer_* 字段与 device_interfaces.net_outlet_ids
-- 背景：将"信息点"原 peer_* 连接逻辑统一迁移到"线路"(cable_links) 模块。
-- 适用：已部署的旧版本（0.14.10 及之前）升级到新版本。
-- 执行方式：手动连接 IPMA 数据库后执行本脚本（psql -d <ipma_db> -f 本文件）。
-- 注意：本脚本不可逆，执行前请务必备份数据库。

BEGIN;

-- 1. 重建视图（先删，因依赖 net_outlets 列）
DROP VIEW IF EXISTS net_outlets_with_details CASCADE;

-- 2. 删除 net_outlets 的 peer_* 列与相关索引
DROP INDEX IF EXISTS idx_net_outlets_peer_outlet_id;
DROP INDEX IF EXISTS idx_net_outlets_peer_switch_port_id;
ALTER TABLE net_outlets
    DROP CONSTRAINT IF EXISTS chk_peer_type;
ALTER TABLE net_outlets
    DROP COLUMN IF EXISTS peer_type,
    DROP COLUMN IF EXISTS peer_room_id,
    DROP COLUMN IF EXISTS peer_outlet_id,
    DROP COLUMN IF EXISTS peer_switch_port_id;

-- 3. 删除 device_interfaces.net_outlet_ids 列与 GIN 索引
DROP INDEX IF EXISTS idx_device_interfaces_net_outlet_ids;
ALTER TABLE device_interfaces
    DROP COLUMN IF EXISTS net_outlet_ids;

-- 4. 重建视图（去 peer_* 列后的新结构）
CREATE VIEW net_outlets_with_details AS
SELECT
    ap.id, ap.name, ap.outlet_type, ap.room_id, ap.cabinet_id,
    ap.description,
    r.name AS room_name,
    cab.name AS cabinet_name,
    ap.created_at, ap.updated_at
FROM net_outlets ap
LEFT JOIN rooms r ON ap.room_id = r.id
LEFT JOIN cabinets cab ON ap.cabinet_id = cab.id;

GRANT SELECT ON net_outlets_with_details TO ipma;

COMMIT;
