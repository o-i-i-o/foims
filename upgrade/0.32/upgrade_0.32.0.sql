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

COMMIT;
