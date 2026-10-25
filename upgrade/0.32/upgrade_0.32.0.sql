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

COMMIT;
