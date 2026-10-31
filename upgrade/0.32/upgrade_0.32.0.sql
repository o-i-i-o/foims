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
--   4. 「组织 → 房间 → 工位 → 设备」责任链收口（房间只做空间归属，责任下沉到工位）：
--      - rooms.org_id 收紧为 NOT NULL（原可空，导致房间可游离于组织之外）；
--      - 房间不设责任人：责任人唯一载体是工位（workstations.manager_employee_id
--        放开为可空，未指定管理人是合法状态），设备 IP 通知投递给所在工位管理人；
--      - 触发器 trg_validate_workstation_manager_org：工位管理人非空时必须与房间
--        同属一个组织（员工组织不可变更，只需守被引用侧）；
--      - 存量订正：无组织房间归入唯一根组织；跨组织的工位管理人清空为未指定
--        （新模型下的合法状态，NOTICE 报告条数）；无法唯一定位时显式报错，
--        不做静默丢弃；
--      - 房型与子项匹配性检查：已有工位/机柜的房型必须相容，存量不符时
--        显式报错要求人工调整（应用层已按房型限制子项新增与房型切换）。
--   6. IP 状态全面自动化：下线人工设置状态入口（PATCH /api/resources/ip/{id}），
--      interface_ips.status 值域由 active/inactive/reserved 收窄为
--      active/inactive；存量 reserved 归为 active，交由判活与陈旧停用任务接管。
--   7. 主机资源告警 episode 化（实时路径接入告警）：新建 host_alert_episodes
--      承载「一次持续超标」的过程态（状态 + 计时 + 累计次数 + 通知行 ID），
--      DROP 旧 host_alert_states（只有 alerting 布尔，无法承载计时；告警态
--      可丢，重评即重建）。阈值配置的窗口字段由代码 serde 默认补齐，存量
--      JSON 无需订正。

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

-- ===== 「组织 → 房间 → 工位 → 设备」责任链收口 =====

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

-- 撤回房间责任人列：房间只做空间归属，责任人唯一载体是工位
-- （workstations.manager_employee_id，可为空）。0.26 开发分支曾要求
-- rooms.manager_employee_id 非空，按旧口径升级过的库必须先摘触发器再删列，
-- 否则残留触发器会在 rooms 写入时引用不存在的列而报错；索引随删列自动移除。
DROP TRIGGER IF EXISTS trg_validate_room_manager_org ON rooms;
DROP FUNCTION IF EXISTS validate_room_manager_org();
ALTER TABLE rooms DROP COLUMN IF EXISTS manager_employee_id;

-- 工位管理人放开为可空：责任人下沉到工位，未指定管理人是合法状态
ALTER TABLE workstations ALTER COLUMN manager_employee_id DROP NOT NULL;

-- 存量工位管理人若不在房间组织内（违反新不变量），清空为未指定，由管理员
-- 重新指定；仅清引用不清工位，故必须以 NOTICE 报告条数而不静默处理
DO $$
DECLARE
  cleaned BIGINT;
BEGIN
  UPDATE workstations w
  SET manager_employee_id = NULL
  FROM rooms r
  WHERE r.id = w.room_id
    AND w.manager_employee_id IS NOT NULL
    AND NOT EXISTS (
        SELECT 1 FROM employees e
        WHERE e.id = w.manager_employee_id AND e.org_id = r.org_id
    );
  GET DIAGNOSTICS cleaned = ROW_COUNT;
  IF cleaned > 0 THEN
    RAISE NOTICE '已清空 % 个跨组织的工位管理人（新模型下工位管理人可为空），请重新指定责任人', cleaned;
  END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_rooms_org_id ON rooms(org_id);

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

-- 工位管理人非空时必须与房间同属一个组织（0.26 曾置 NOT NULL，本次放开
-- 为可空：未指定责任人的工位不参与校验）
CREATE OR REPLACE FUNCTION validate_workstation_manager_org() RETURNS TRIGGER AS $$
DECLARE
  manager_org_id UUID;
  room_org_id UUID;
BEGIN
  IF NEW.manager_employee_id IS NULL THEN
    RETURN NEW;
  END IF;
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

-- ---------------------------------------------------------------------------
-- 5. 全局一致性审计整改（同属 0.32 中版本，随本脚本唯一一份下发）
--   5.1 设备空间挂载点唯一（工位与机位互斥）：新增 CHECK
--      chk_devices_single_location，并把存量设备 room_id 按父侧房间对齐
--      （父侧是权威源）；同时删除不落在设备房间已绑网段上的 IP。
--   5.2 父侧换房兜底：新增延迟约束触发器 trg_*_devices_room，在 COMMIT
--      时把「父侧换了房、设备 room_id 未跟」的事务整笔拒掉（应用层
--      sync_devices_room 负责真正的搬迁，触发器只防腐）。
--   5.3 网口 MAC 归一口径统一：设备网卡同步、ARP 拉取、CSV 导入三条
--      写入路径改为同一归一函数（translate(upper(mac), ':-. ', '')），
--      旧普通索引 idx_device_interfaces_mac 换掉，改表达式部分唯一索引
--      uq_device_interfaces_mac_key——同一块物理网卡只能登记在一个网口
--      下（大小写/分隔符差异视为同一 MAC）。
--   5.4 系统任务可见可管：scheduled_tasks 新增 is_system 标记，系统任务由
--      进程启动时按代码/配置播种（按 name UPSERT）并回读注册，期间不参
--      与 DB 派发；管理接口对该类任务的增删改与启停一律拒绝。存量的同名
--      用户任务由播种直接接管（同名冲突以系统定义为准）。
-- 存量冲突处理原则：无法判定归属的行直接删除（不做静默猜测合并），
-- 可判定归属的按父侧权威源回填。
-- ---------------------------------------------------------------------------

-- 5.1 位置双挂（既在工位又在机位）无法判定归属，直接删除；网口/IP/连线
-- 级联清理，由管理员重新登记
DELETE FROM devices
WHERE workstation_id IS NOT NULL AND position_id IS NOT NULL;

-- 设备 room_id 按父侧房间对齐（工位优先于机位，因两向已互斥不会再重叠）
UPDATE devices d SET room_id = w.room_id
FROM workstations w
WHERE w.id = d.workstation_id AND d.room_id IS DISTINCT FROM w.room_id;

UPDATE devices d SET room_id = c.room_id
FROM positions p JOIN cabinets c ON c.id = p.cabinet_id
WHERE p.id = d.position_id AND d.room_id IS DISTINCT FROM c.room_id;

-- 不落在设备房间已绑网段上的 IP：与 sync_devices_room 同口径直接删除，
-- 由设备保存/自动分配路径重新分配（不跨房间改挂）
DELETE FROM interface_ips m
USING device_interfaces di, devices d
WHERE m.device_interface_id = di.id
  AND di.device_id = d.id
  AND NOT EXISTS (
      SELECT 1 FROM room_networks rn
      WHERE rn.room_id = d.room_id AND rn.subnet_id = m.subnet_id
  );

DO $$
BEGIN
  IF NOT EXISTS (
      SELECT 1 FROM pg_constraint WHERE conname = 'chk_devices_single_location'
  ) THEN
    ALTER TABLE devices
      ADD CONSTRAINT chk_devices_single_location
      CHECK (workstation_id IS NULL OR position_id IS NULL);
  END IF;
END $$;

-- 5.2 父侧换房后设备 room_id 必须跟随（延迟约束触发器，不依赖语句先后）
CREATE OR REPLACE FUNCTION check_devices_follow_parent_room() RETURNS TRIGGER AS $$
DECLARE
  target_room UUID;
  mismatched BIGINT;
BEGIN
  IF TG_TABLE_NAME = 'cabinets' THEN
    IF NEW.room_id IS NOT DISTINCT FROM OLD.room_id THEN
      RETURN NULL;
    END IF;
    target_room := NEW.room_id;
    SELECT count(*) INTO mismatched
      FROM devices d JOIN positions p ON d.position_id = p.id
      WHERE p.cabinet_id = NEW.id AND d.room_id IS DISTINCT FROM target_room;
  ELSIF TG_TABLE_NAME = 'workstations' THEN
    IF NEW.room_id IS NOT DISTINCT FROM OLD.room_id THEN
      RETURN NULL;
    END IF;
    target_room := NEW.room_id;
    SELECT count(*) INTO mismatched
      FROM devices d
      WHERE d.workstation_id = NEW.id AND d.room_id IS DISTINCT FROM target_room;
  ELSE
    IF NEW.cabinet_id IS NOT DISTINCT FROM OLD.cabinet_id THEN
      RETURN NULL;
    END IF;
    -- 机位未归属机柜时无房间约束（room_id 由设备自身维护）
    IF NEW.cabinet_id IS NULL THEN
      RETURN NULL;
    END IF;
    SELECT c.room_id INTO target_room FROM cabinets c WHERE c.id = NEW.cabinet_id;
    SELECT count(*) INTO mismatched
      FROM devices d
      WHERE d.position_id = NEW.id AND d.room_id IS DISTINCT FROM target_room;
  END IF;

  IF mismatched > 0 THEN
    RAISE EXCEPTION '设备 room_id 未跟随所属空间房间（% 行）', mismatched;
  END IF;
  RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_cabinets_devices_room ON cabinets;
CREATE CONSTRAINT TRIGGER trg_cabinets_devices_room
    AFTER UPDATE ON cabinets
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION check_devices_follow_parent_room();

DROP TRIGGER IF EXISTS trg_workstations_devices_room ON workstations;
CREATE CONSTRAINT TRIGGER trg_workstations_devices_room
    AFTER UPDATE ON workstations
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION check_devices_follow_parent_room();

DROP TRIGGER IF EXISTS trg_positions_devices_room ON positions;
CREATE CONSTRAINT TRIGGER trg_positions_devices_room
    AFTER UPDATE ON positions
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION check_devices_follow_parent_room();

-- 5.3 MAC 归一后重复的网口只保留最早登记的一行
DELETE FROM device_interfaces di
USING device_interfaces keeper
WHERE di.id <> keeper.id
  AND di.mac_address IS NOT NULL
  AND keeper.mac_address IS NOT NULL
  AND translate(upper(di.mac_address), ':-. ', '') <> ''
  AND translate(upper(di.mac_address), ':-. ', '')
      = translate(upper(keeper.mac_address), ':-. ', '')
  AND (di.created_at, di.id) > (keeper.created_at, keeper.id);

DROP INDEX IF EXISTS idx_device_interfaces_mac;

CREATE UNIQUE INDEX IF NOT EXISTS uq_device_interfaces_mac_key
    ON device_interfaces(translate(upper(mac_address), ':-. ', ''))
    WHERE mac_address IS NOT NULL
      AND translate(upper(mac_address), ':-. ', '') <> '';

-- 5.4 系统任务标记
ALTER TABLE scheduled_tasks ADD COLUMN IF NOT EXISTS is_system BOOLEAN NOT NULL DEFAULT FALSE;

-- ===== 6. IP 状态全面自动化：下线人工设置状态（reserved） =====
-- 人工状态入口已下线，reserved 不再产生。存量保留地址先归为 active，由
-- 自动判活（ARP 命中刷新并恢复 active）与 ip_status_sync 陈旧停用任务按
-- last_seen 统一收敛，不再保留「人工豁免自动化」的语义。
UPDATE interface_ips SET status = 'active' WHERE status = 'reserved';

-- 状态值域收窄：与 foims-init 建表 CHECK 保持一致
ALTER TABLE interface_ips DROP CONSTRAINT IF EXISTS chk_interface_ips_status;
ALTER TABLE interface_ips ADD CONSTRAINT chk_interface_ips_status
    CHECK (status IN ('active', 'inactive'));

-- ===== 7. 主机资源告警 episode 化 =====
-- 告警由「布尔翻转态」升级为「过程态」：一台主机一个指标的一次持续超标落一行
-- episode，pending/active/resolved 三段生命周期 + 持续计时 + 静默窗口内的累计
-- 次数与通知行引用（notify_ids 逗号串，供就地刷新通知文案）。旧表
-- host_alert_states 只有 alerting 布尔，无法承载上述状态，直接重建：告警态是
-- 可由读数重算的事实，删除后下一轮评估自然重建，最坏结果是一次重复通知。
CREATE TABLE IF NOT EXISTS host_alert_episodes (
    host_id UUID NOT NULL REFERENCES monitored_hosts(id) ON DELETE CASCADE,
    metric TEXT NOT NULL CONSTRAINT chk_host_alert_episodes_metric
        CHECK (metric IN ('cpu', 'mem', 'disk', 'temp')),
    status TEXT NOT NULL CONSTRAINT chk_host_alert_episodes_status
        CHECK (status IN ('pending', 'active', 'resolved')),
    started_at TIMESTAMP WITH TIME ZONE NOT NULL,
    opened_at TIMESTAMP WITH TIME ZONE,
    resolved_at TIMESTAMP WITH TIME ZONE,
    last_exceed_at TIMESTAMP WITH TIME ZONE NOT NULL,
    last_notified_at TIMESTAMP WITH TIME ZONE,
    last_value DOUBLE PRECISION,
    threshold SMALLINT NOT NULL,
    exceed_count BIGINT NOT NULL DEFAULT 0,
    notify_ids TEXT NOT NULL DEFAULT '',
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    PRIMARY KEY (host_id, metric)
);

DROP TABLE IF EXISTS host_alert_states;

-- ===== 8. SNMP 实时采集端口 =====
-- 每设备最多一个采集端口：实时流量只采该口（未配置时回落探测到的第一个端口），
-- 站内秒级展示用，不落指标表
ALTER TABLE device_interfaces ADD COLUMN IF NOT EXISTS monitored BOOLEAN NOT NULL DEFAULT FALSE;
CREATE UNIQUE INDEX IF NOT EXISTS uq_device_interfaces_monitored
    ON device_interfaces(device_id) WHERE monitored;

COMMIT;
