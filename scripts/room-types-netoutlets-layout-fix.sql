-- ============================================================
-- 本轮结构变更迁移：
--   1. rooms 房型新增 LOBBY（大厅）/ RECEPTION（前台）/ OTHER（其他）
--   2. net_outlets 信息点名称改为全局唯一（原仅同房间唯一）
--   3. workstation_layouts 回填 room_id（历史保存逻辑漏写，导致布局加载不到）
--
-- 执行方式：sudo -u postgres psql -d ipma -f scripts/room-types-netoutlets-layout-fix.sql
-- ============================================================
\set ON_ERROR_STOP on

BEGIN;
SET ROLE ipma;

-- 1) 房型约束扩展
ALTER TABLE rooms DROP CONSTRAINT IF EXISTS chk_room_type;
ALTER TABLE rooms ADD CONSTRAINT chk_room_type CHECK (
    room_type IN ('OFFICE', 'LOBBY', 'RECEPTION', 'DATA_CENTER', 'TELECOM_CLOSET', 'OTHER')
);

-- 2) 信息点名称全局唯一
-- 预检：跨房间重名会唯一约束创建失败，提前给出明确错误
DO $$
DECLARE
    dup_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO dup_count
    FROM (SELECT name FROM net_outlets GROUP BY name HAVING COUNT(*) > 1) t;
    IF dup_count > 0 THEN
        RAISE EXCEPTION 'net_outlets 存在 % 个跨房间重名名称，请先手工改名后重试', dup_count;
    END IF;
END $$;

ALTER TABLE net_outlets DROP CONSTRAINT IF EXISTS uq_net_outlets_name;
ALTER TABLE net_outlets ADD CONSTRAINT uq_net_outlets_name UNIQUE (name);

-- 3) 工位布局回填 room_id（按工位所属房间对齐，NULL 与过期值一并修复）
UPDATE workstation_layouts wl
SET room_id = w.room_id
FROM workstations w
WHERE wl.workstation_id = w.id
  AND wl.room_id IS DISTINCT FROM w.room_id;

-- 回填后收紧为非空（此后保存必写 room_id）
ALTER TABLE workstation_layouts ALTER COLUMN room_id SET NOT NULL;

COMMIT;
