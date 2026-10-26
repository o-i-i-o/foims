-- FOIMS 0.31.x → 0.32.x 数据库升级脚本（0.32 中版本唯一一份）
-- 执行前置：停止 foims 服务、完成数据库备份。
-- 变更概览（SNMP 第二层：网络侧清单/台账）：
--   1. 新建 6 张当前态台账表，均引用 devices(id) ON DELETE CASCADE 并带
--      created_at/updated_at，口径与 device_lldps 一致（覆盖式 upsert +
--      陈旧清理，不做时序、不接清理任务）：
--      - device_entities：ENTITY-MIB 物理实体清单；
--      - device_sensors：ENTITY-SENSOR-MIB 传感器读数；
--      - device_vlans：Q-BRIDGE-MIB VLAN 台账（成员端口 JSONB 数组）；
--      - device_cdps：CISCO-CDP-MIB 邻居；
--      - device_lags：IEEE8023-LAG-MIB 聚合口（成员 JSONB 数组）；
--      - device_poe_ports：PFN-MIB PoE 供电端口。
--   2. snmp_events 表结构不变：第二层新事件类型（poe_port_off/
--      cdp_neighbor_added/cdp_neighbor_removed）走既有 event_type VARCHAR，
--      值域由代码常量约束。
--   说明：foims-init 建表代码（schema/tables/device_inventory.rs）与
--   check.rs 表/列清单已同步新增。
--   注：sensor_value/measured_power_w 用 DOUBLE PRECISION 匹配 sqlx f64
--   解码，避免引入 bigdecimal 依赖。
--   3. 「MAC地址变更通知」重构为「IP地址变更通知」的数据订正（无表结构变化）：
--      - system_configs（notification/methods）JSON 键 mac_change 改名 ip_change
--        （代码侧已加 serde alias 兼容旧键，本订正将存量配置固化为新键）；
--      - notifications 历史行 notification_type='mac_change' 订正为 'ip_change'，
--        标题/正文的 i18n key（server.notification.mac_change.*）同步替换为
--        server.notification.ip_change.*；纯文本旧内容原样保留。
--   4. 「组织 → 房间 → 工位/设备」责任链收口（房间成为空间归属与责任唯一入口）：
--      - rooms.org_id 收紧为 NOT NULL（原可空，导致房间可游离于组织之外）；
--      - rooms 新增 manager_employee_id NOT NULL：机房/弱电井等无工位空间
--        也有责任人，设备 IP 通知按「工位管理人 → 房间管理人」回退；
--      - 新增触发器 trg_validate_room_manager_org / trg_validate_workstation_manager_org：
--        管理人员工必须与房间同属一个组织（员工组织不可变更，只需守被引用侧）；
--      - 存量回填：无组织房间归入唯一根组织；房间管理人取本组织下按姓名首位的员工；
--        工位管理人若不在房间组织内，改为该房间管理人。无法唯一定位时显式报错，
--        不做静默丢弃；
--      - 房型与子项匹配性检查：已有工位/机柜的房型必须相容，存量不符时
--        显式报错要求人工调整（应用层已按房型限制子项新增与房型切换）。

BEGIN;

CREATE TABLE IF NOT EXISTS device_entities (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    ent_index INTEGER NOT NULL,
    ent_class VARCHAR(32),
    name VARCHAR(255),
    descr TEXT,
    model_name VARCHAR(128),
    serial_num VARCHAR(128),
    mfg_name VARCHAR(128),
    hw_rev VARCHAR(32),
    fw_rev VARCHAR(64),
    sw_rev VARCHAR(64),
    is_fru BOOLEAN,
    parent_index INTEGER,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(device_id, ent_index)
);

CREATE TABLE IF NOT EXISTS device_sensors (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    ent_index INTEGER NOT NULL,
    sensor_type INTEGER,
    sensor_scale INTEGER,
    sensor_precision INTEGER,
    sensor_value DOUBLE PRECISION,
    sensor_status INTEGER,
    name VARCHAR(255),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(device_id, ent_index)
);

CREATE TABLE IF NOT EXISTS device_vlans (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    vlan_id INTEGER NOT NULL,
    name VARCHAR(64),
    member_ports JSONB NOT NULL DEFAULT '[]'::jsonb,
    untagged_ports JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(device_id, vlan_id)
);

CREATE TABLE IF NOT EXISTS device_cdps (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    local_port VARCHAR(50) NOT NULL,
    neighbor_device_id VARCHAR(255),
    neighbor_port VARCHAR(100),
    neighbor_address VARCHAR(64),
    neighbor_platform VARCHAR(128),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(device_id, local_port, neighbor_device_id)
);

CREATE TABLE IF NOT EXISTS device_lags (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    agg_ifindex INTEGER NOT NULL,
    agg_name VARCHAR(100),
    member_ifindexes JSONB NOT NULL DEFAULT '[]'::jsonb,
    member_names JSONB NOT NULL DEFAULT '[]'::jsonb,
    lacp_state VARCHAR(32),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(device_id, agg_ifindex)
);

CREATE TABLE IF NOT EXISTS device_poe_ports (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    if_index INTEGER NOT NULL,
    if_name VARCHAR(100),
    admin_status INTEGER,
    detection_status INTEGER,
    power_class INTEGER,
    measured_power_w DOUBLE PRECISION,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(device_id, if_index)
);

-- ===== 「IP地址变更通知」重构：存量数据订正 =====
-- 通知方式开关：仅当存量 JSON 含旧键 mac_change 时重写，将 mac_change 改名为
-- ip_change（其余键原样保留）；jsonb_object_agg 重建后回写为 TEXT。
-- 先以 pg_input_is_valid 校验存量值为合法 JSON（放入 MATERIALIZED CTE 的 WHERE，
-- 使 ::jsonb 仅在过滤后逐行求值），避免坏值触发整体报错连带回滚本事务内的建表 DDL。
WITH src AS MATERIALIZED (
    SELECT id, value::jsonb AS j
    FROM system_configs
    WHERE config_type = 'notification'
      AND key = 'methods'
      AND value IS NOT NULL
      AND pg_input_is_valid(value, 'jsonb')
)
UPDATE system_configs
SET value = (
        SELECT jsonb_object_agg(
                   CASE WHEN k = 'mac_change' THEN 'ip_change' ELSE k END,
                   v
               )::text
        FROM jsonb_each(src.j) AS t(k, v)
    ),
    updated_at = NOW()
FROM src
WHERE system_configs.id = src.id
  AND src.j ? 'mac_change';

-- 历史站内通知：旧 mac_change 语义为「同一登记 IP 上学到新 MAC（物理设备被替换）」，
-- 归入 device_swap 文案（其参数集 workstation/ip/old_mac/new_mac/device 与旧行逐字一致，
-- 改写为 ip_change.body 会因参数名不符渲染出空占位符、丢失取证信息）；
-- notification_type 仍并入 ip_change（与 IP 变更共用同一开关门控）。
-- content 为 JSON（含 key/params）或纯文本，replace 无命中即原样保留。
UPDATE notifications
SET notification_type = 'ip_change',
    title = replace(title, 'server.notification.mac_change.', 'server.notification.device_swap.'),
    content = replace(content, 'server.notification.mac_change.', 'server.notification.device_swap.')
WHERE notification_type = 'mac_change';

-- 历史「未知端点」站内通知：正文 i18n key 由 body_unknown_endpoint 改名为
-- body_unknown_device（参数集不变），否则前端删键后回退渲染出原始 key 字面量。
-- 不限 notification_type（该 key 仅嵌于 snmp_event 正文），replace 无命中即原样保留。
UPDATE notifications
SET content = replace(content, 'body_unknown_endpoint', 'body_unknown_device')
WHERE content LIKE '%body_unknown_endpoint%';

-- 历史观测事件：snmp_events.event_type 旧值 unknown_endpoint（重构前按未登记 IP 判定）
-- → unknown_device（按未登记 MAC 判定）。前端展示/筛选只认规范值、不留旧值枚举兼容层。
UPDATE snmp_events
SET event_type = 'unknown_device'
WHERE event_type = 'unknown_endpoint';

-- ===== 「组织 → 房间 → 工位/设备」责任链收口 =====

ALTER TABLE rooms
    ADD COLUMN IF NOT EXISTS manager_employee_id UUID REFERENCES employees(id) ON DELETE RESTRICT;

-- 存量房间缺失组织归属时回填唯一根组织：存在多个根则无法推断，显式报错
DO $$
DECLARE
  root_count BIGINT;
  orphan_count BIGINT;
BEGIN
  SELECT COUNT(*) INTO orphan_count FROM rooms WHERE org_id IS NULL;
  IF orphan_count > 0 THEN
    SELECT COUNT(*) INTO root_count FROM organizations WHERE parent_id IS NULL;
    IF root_count <> 1 THEN
      RAISE EXCEPTION '有 % 个房间未归属组织，且组织树有 % 个根节点，无法自动回填，请人工指定 rooms.org_id 后重试',
        orphan_count, root_count;
    END IF;
    UPDATE rooms SET org_id = (SELECT id FROM organizations WHERE parent_id IS NULL);
  END IF;
END $$;

ALTER TABLE rooms ALTER COLUMN org_id SET NOT NULL;

-- 存量房间管理人回填：取所属组织下按姓名首位的员工（部门内首位责任人）
UPDATE rooms r
SET manager_employee_id = (
    SELECT e.id FROM employees e
    WHERE e.org_id = r.org_id
    ORDER BY e.name, e.id
    LIMIT 1
)
WHERE r.manager_employee_id IS NULL;

-- 组织下无员工的房间无法自动确定责任人：要求先补员工，而非静默留空
DO $$
DECLARE
  missing BIGINT;
BEGIN
  SELECT COUNT(*) INTO missing FROM rooms WHERE manager_employee_id IS NULL;
  IF missing > 0 THEN
    RAISE EXCEPTION '有 % 个房间所属组织下没有员工，无法确定房间管理人，请先补充员工后重试', missing;
  END IF;
END $$;

ALTER TABLE rooms ALTER COLUMN manager_employee_id SET NOT NULL;

-- 存量工位管理人若不在房间组织内，统一改挂房间管理人（空间兜底责任人）
UPDATE workstations w
SET manager_employee_id = r.manager_employee_id
FROM rooms r
WHERE r.id = w.room_id
  AND NOT EXISTS (
      SELECT 1 FROM employees e
      WHERE e.id = w.manager_employee_id AND e.org_id = r.org_id
  );

CREATE INDEX IF NOT EXISTS idx_rooms_org_id ON rooms(org_id);
CREATE INDEX IF NOT EXISTS idx_rooms_manager_employee_id ON rooms(manager_employee_id);

-- 房型与子项匹配性检查（与 helpers::room_allows_workstation/cabinet 同口径）：
-- 办公类房型不放机柜、机房类房型不放工位。存量不符时显式报错而非静默迁移，
-- 因为“该子项应搬到哪个房间”无法自动推断
DO $$
DECLARE
  bad_workstations BIGINT;
  bad_cabinets BIGINT;
BEGIN
  SELECT COUNT(*) INTO bad_workstations
  FROM workstations w JOIN rooms r ON r.id = w.room_id
  WHERE r.room_type NOT IN ('OFFICE', 'LOBBY', 'RECEPTION', 'OTHER');
  SELECT COUNT(*) INTO bad_cabinets
  FROM cabinets c JOIN rooms r ON r.id = c.room_id
  WHERE r.room_type NOT IN ('DATA_CENTER', 'TELECOM_CLOSET', 'OTHER');
  IF bad_workstations > 0 OR bad_cabinets > 0 THEN
    RAISE EXCEPTION '有 % 个工位处于不允许工位的房型、% 个机柜处于不允许机柜的房型，请先调整房间房型或迁移子项后重试',
      bad_workstations, bad_cabinets;
  END IF;
END $$;

CREATE OR REPLACE FUNCTION validate_room_manager_org() RETURNS TRIGGER AS $$
DECLARE
  manager_org_id UUID;
BEGIN
  SELECT org_id INTO manager_org_id FROM employees WHERE id = NEW.manager_employee_id;
  IF manager_org_id IS DISTINCT FROM NEW.org_id THEN
    RAISE EXCEPTION '房间管理人所属组织 (%) 与房间组织 (%) 不一致', manager_org_id, NEW.org_id;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_validate_room_manager_org ON rooms;
CREATE TRIGGER trg_validate_room_manager_org
    BEFORE INSERT OR UPDATE OF org_id, manager_employee_id ON rooms
    FOR EACH ROW EXECUTE FUNCTION validate_room_manager_org();

CREATE OR REPLACE FUNCTION validate_workstation_manager_org() RETURNS TRIGGER AS $$
DECLARE
  manager_org_id UUID;
  room_org_id UUID;
BEGIN
  SELECT org_id INTO manager_org_id FROM employees WHERE id = NEW.manager_employee_id;
  SELECT org_id INTO room_org_id FROM rooms WHERE id = NEW.room_id;
  IF manager_org_id IS DISTINCT FROM room_org_id THEN
    RAISE EXCEPTION '工位管理人所属组织 (%) 与房间组织 (%) 不一致', manager_org_id, room_org_id;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_validate_workstation_manager_org ON workstations;
CREATE TRIGGER trg_validate_workstation_manager_org
    BEFORE INSERT OR UPDATE OF room_id, manager_employee_id ON workstations
    FOR EACH ROW EXECUTE FUNCTION validate_workstation_manager_org();

COMMIT;
