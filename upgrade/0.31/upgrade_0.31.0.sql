-- FOIMS 0.30.x → 0.31.x 数据库升级脚本（0.31 中版本唯一一份）
-- 执行前置：停止 foims 服务、完成数据库备份。
-- 变更概览：
--   1. 新建 snmp_events 观测事件表：SNMP 轮询与 MAC 同步路径产出的
--      事实源事件（link_flip/device_reboot/arp_added/arp_removed/
--      arp_conflict/unknown_endpoint），detail 为 JSONB 事件上下文，
--      severity 由 CHECK 限定 info|warning|critical；
--      event_type 值域由代码常量约束（与既有枚举列口径一致）。
--   2. 配套两条查询索引（设备时间线 / 类型时间线）。
--   说明：foims-init 建表代码（schema/tables/snmp_events.rs）与
--   check.rs 表/列清单已同步新增。

BEGIN;

CREATE TABLE IF NOT EXISTS snmp_events (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    event_type VARCHAR(32) NOT NULL,
    interface VARCHAR(100),
    severity VARCHAR(10) NOT NULL DEFAULT 'info'
        CHECK (severity IN ('info', 'warning', 'critical')),
    detail JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_snmp_events_device_time
    ON snmp_events (device_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_snmp_events_type_time
    ON snmp_events (event_type, occurred_at DESC);

COMMIT;
