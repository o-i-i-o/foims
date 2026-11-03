-- FOIMS 0.32.x → 0.33.x 数据库升级脚本（0.33 中版本唯一一份）
-- 执行前置：停止 foims 服务、完成数据库备份。
-- 变更概览（IPMI 带外采集集成主机监控）：
--   1. devices 新增 4 个 IPMI 凭据列：username/password 均非空即视为「已配置」，
--      不设独立开关列；password 以密文落库（AES-GCM + base64），列宽 160 容纳
--      IPMI 规范口令（≤20 字节）加密结果。受 ruipmi 实现约束：会话端口恒 623、
--      特权固定 Administrator，故 ipmi_port 仅承载展示语义、采集侧不使用；
--      ipmi_auth_algo（sha1|sha256）选择 RAKP 套件（sha256=suite17、sha1=suite3）。
--   2. monitored_hosts.source CHECK 扩为 ('agent','snmp','ipmi')；
--      idx_monitored_hosts_device_id 唯一索引由 (device_id) 改为
--      (device_id, source)——同一设备的 SNMP 行与 IPMI 行并存的前提。
--   3. devices_with_details 视图补 4 列（CREATE OR REPLACE，列顺序与建表一致）。
--   4. users 新增可空列 phone VARCHAR(20)（用户手机号码，选填；口径同
--      employees.phone：数字与 + - 空格，空串不入库）。
--   5. 通知配置结构化（收件人按类型订阅 + Webhook 多目标，无 schema 变更，
--      仅 system_configs 数据订正）：
--      a) notification/email_recipients 由 UUID 字符串数组改为对象数组
--         [{user_id, ip_change, agent_alert, snmp_event}]，存量收件人默认
--         订阅全部通知类型（与旧全局开关缺省口径一致）；
--      b) webhook 旧四键（generic_url/dingtalk_url/dingtalk_secret/wecom_key）
--         合并为 webhook/targets 单行 JSON 数组（每条目标 id/name/channel/
--         url/secret/订阅三开关），每渠道一条目标、默认订阅全部类型；
--         dingtalk_secret 密文原样迁移，wecom_key 拼接为完整群机器人地址；
--      c) 删除废弃的 notification/methods 通知方式矩阵与 webhook 旧四键。
--   6. host_metrics_history 降采样分层：新增 granularity 列（0=原始秒级 /
--      5=5min 桶 / 60=1h 桶），唯一索引扩为 (agent_id, granularity,
--      collected_at)（原始层与降采样桶层互不冲突）；存量原始层数据一次性
--      回填 5min 桶层（数值键聚合 avg/min/max/last/cnt），1h 桶层由服务端
--      定时任务从 5min 桶滚降生成，不做历史回填。
--   7. host_metrics_history 新增 (granularity, collected_at) 复合索引：
--      分层清理 DELETE、1h 滚降 pending 扫描与原始层补聚合扫描均按
--      「层 + 时间」过滤而无 agent_id 前导列，唯一索引无法服务。
--   8. devices 删除 location 自由文本列：空间归属已由房间/工位/机柜/机位
--      结构化字段承载，该列全站无展示出口，属冗余备注（存量值随删列丢弃）。
--   说明：foims-init 建表代码（schema/tables/devices.rs、agents.rs、views.rs、
--   users.rs）与 check.rs 表/列/列宽清单已同步新增。
BEGIN;

-- ===== 1. devices IPMI 凭据列 =====
ALTER TABLE devices ADD COLUMN IF NOT EXISTS ipmi_username VARCHAR(62);
ALTER TABLE devices ADD COLUMN IF NOT EXISTS ipmi_password VARCHAR(160);
ALTER TABLE devices ADD COLUMN IF NOT EXISTS ipmi_port INTEGER DEFAULT 623;
ALTER TABLE devices
    ADD COLUMN IF NOT EXISTS ipmi_auth_algo VARCHAR(10) DEFAULT 'sha256';

-- 幂等重建权限算法白名单约束（ADD CONSTRAINT 无 IF NOT EXISTS）
ALTER TABLE devices DROP CONSTRAINT IF EXISTS chk_devices_ipmi_auth_algo;
ALTER TABLE devices ADD CONSTRAINT chk_devices_ipmi_auth_algo
    CHECK (ipmi_auth_algo IN ('sha1', 'sha256'));

-- ===== 2. monitored_hosts 来源与设备唯一性 =====
-- 来源扩为 agent|snmp|ipmi：IPMI 带外轮询记录与 SNMP/Agent 行并列。
ALTER TABLE monitored_hosts DROP CONSTRAINT IF EXISTS chk_monitored_hosts_source;
ALTER TABLE monitored_hosts ADD CONSTRAINT chk_monitored_hosts_source
    CHECK (source IN ('agent', 'snmp', 'ipmi'));

-- 同设备 SNMP 行与 IPMI 行并存：device_id 唯一性按 (device_id, source) 组合。
-- 旧索引 (device_id) 会拒绝同设备多来源行，必须先 DROP 再按新键重建；
-- 存量各设备每来源至多一行，重建不会冲突。
DROP INDEX IF EXISTS idx_monitored_hosts_device_id;
CREATE UNIQUE INDEX IF NOT EXISTS idx_monitored_hosts_device_id
    ON monitored_hosts (device_id, source) WHERE device_id IS NOT NULL;

-- ===== 3. devices_with_details 视图补 IPMI 列 =====
-- IPMI 列插在 snmp_port 与 power_watts 之间，PG 的 CREATE OR REPLACE VIEW 要求同名同位置，
-- 中段插列会报 `cannot change name of view column`，因此沿用 0.30 的 DROP + CREATE 范式。
DROP VIEW IF EXISTS devices_with_details;
CREATE VIEW devices_with_details AS
SELECT
    d.id, d.name, d.hostname, d.device_type, d.brand, d.model, d.serial_number,
    d.workstation_id, d.position_id, d.room_id,
    d.template_id, d.seller,
    d.snmp_version, d.snmp_community, d.snmp_username,
    d.snmp_auth_protocol, d.snmp_auth_password,
    d.snmp_priv_protocol, d.snmp_priv_password, d.snmp_port,
    d.ipmi_username, d.ipmi_password, d.ipmi_port, d.ipmi_auth_algo,
    d.power_watts,
    d.description,
    w.name AS workstation_name,
    r.name AS room_name,
    cab.id AS cabinet_id,
    cab.name AS cabinet_name,
    p.start_u, p.end_u,
    dt.name AS template_name,
    d.created_at, d.updated_at
FROM devices d
LEFT JOIN workstations w ON d.workstation_id = w.id
LEFT JOIN positions p ON d.position_id = p.id
LEFT JOIN cabinets cab ON p.cabinet_id = cab.id
LEFT JOIN rooms r ON d.room_id = r.id
LEFT JOIN device_templates dt ON d.template_id = dt.id;

-- ===== 4. users 手机号码列 =====
-- 用户手机号码（选填可空）：新增后即可入库，存量行保持 NULL 无需回填。
ALTER TABLE users ADD COLUMN IF NOT EXISTS phone VARCHAR(20);

-- ===== 5. 通知配置结构化（system_configs 数据订正，无 schema 变更） =====

-- 5.1 邮件收件人：UUID 字符串数组 → 对象数组（每收件人默认订阅全部通知类型，
--     与旧全局开关缺省全启用口径一致）。正则门槛限定字符串数组形态，损坏
--     数据不触碰（应用层读取失败会留痕提示重配）。
UPDATE system_configs
SET value = COALESCE((
    SELECT jsonb_agg(jsonb_build_object(
               'user_id', r #>> '{}',
               'ip_change', true, 'agent_alert', true, 'snmp_event', true
           ) ORDER BY ord)::text
    FROM jsonb_array_elements(value::jsonb) WITH ORDINALITY AS t(r, ord)
), '[]')
WHERE config_type = 'notification' AND key = 'email_recipients'
  AND value ~ '^\s*\[\s*"';

-- 5.2 Webhook 旧四键 → targets 单行 JSON 数组（每渠道一条目标，默认订阅全部
--     类型）。三渠道各自只在其旧键有值时生成目标；dingtalk_secret 为 AES-GCM
--     密文（应用层加密产物），原样嵌入由新代码按密文口径解密；wecom_key 拼
--     接为完整群机器人地址。ON CONFLICT 幂等，重跑不重复生成。
INSERT INTO system_configs (config_type, key, value)
SELECT 'webhook', 'targets', COALESCE(jsonb_agg(t ORDER BY ord), '[]'::jsonb)::text
FROM (
    SELECT jsonb_build_object(
               'id', gen_random_uuid()::text,
               'name', '通用 Webhook',
               'channel', 'generic',
               'url', value,
               'secret', '',
               'ip_change', true, 'agent_alert', true, 'snmp_event', true
           ) AS t, 1 AS ord
    FROM system_configs
    WHERE config_type = 'webhook' AND key = 'generic_url'
      AND value IS NOT NULL AND value <> ''
    UNION ALL
    SELECT jsonb_build_object(
               'id', gen_random_uuid()::text,
               'name', '钉钉',
               'channel', 'dingtalk',
               'url', value,
               'secret', COALESCE((
                   SELECT s.value FROM system_configs s
                   WHERE s.config_type = 'webhook' AND s.key = 'dingtalk_secret'
               ), ''),
               'ip_change', true, 'agent_alert', true, 'snmp_event', true
           ), 2
    FROM system_configs
    WHERE config_type = 'webhook' AND key = 'dingtalk_url'
      AND value IS NOT NULL AND value <> ''
    UNION ALL
    SELECT jsonb_build_object(
               'id', gen_random_uuid()::text,
               'name', '企业微信',
               'channel', 'wecom',
               'url', 'https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=' || value,
               'secret', '',
               'ip_change', true, 'agent_alert', true, 'snmp_event', true
           ), 3
    FROM system_configs
    WHERE config_type = 'webhook' AND key = 'wecom_key'
      AND value IS NOT NULL AND value <> ''
) t
ON CONFLICT (config_type, key) DO NOTHING;

-- 5.3 删除废弃数据：webhook 旧四键（已并入 targets）与通知方式矩阵
--     （每渠道/每收件人的订阅开关已取代全局矩阵）。
DELETE FROM system_configs
WHERE config_type = 'webhook'
  AND key IN ('generic_url', 'dingtalk_url', 'dingtalk_secret', 'wecom_key');
DELETE FROM system_configs
WHERE config_type = 'notification' AND key = 'methods';

-- ===== 6. host_metrics_history 降采样分层 =====
-- granularity：降采样粒度（0=原始秒级 / 5=5min 桶 / 60=1h 桶）。
-- 存量行均为原始层，默认 0 即正确语义，无需回填该列。
ALTER TABLE host_metrics_history
    ADD COLUMN IF NOT EXISTS granularity SMALLINT NOT NULL DEFAULT 0;

-- 幂等重建粒度白名单约束（ADD CONSTRAINT 无 IF NOT EXISTS）
ALTER TABLE host_metrics_history
    DROP CONSTRAINT IF EXISTS chk_host_metrics_history_granularity;
ALTER TABLE host_metrics_history ADD CONSTRAINT chk_host_metrics_history_granularity
    CHECK (granularity IN (0, 5, 60));

-- 唯一索引由 (agent_id, collected_at) 改为 (agent_id, granularity, collected_at)：
-- 原始层与降采样桶层互不冲突。旧索引会拒绝同 (agent_id, collected_at) 的
-- 分层行，必须先 DROP 再按新键重建；存量仅原始层一行，重建不会冲突。
DROP INDEX IF EXISTS idx_host_metrics_history_agent_time;
CREATE UNIQUE INDEX IF NOT EXISTS idx_host_metrics_history_agent_time
    ON host_metrics_history (agent_id, granularity, collected_at);

-- 存量原始层数据一次性回填 5min 桶层：jsonb_each 展开数值型指标键，按
-- (agent_id, 300s 桶, 键) 聚合 avg/min/max/last（last 取桶内最新样本值）
-- 与样本数 cnt，再按 (agent_id, 桶) 重组为 {"key":{"avg","min","max","last",
-- "cnt"}}，与服务端 MetricsAggregator 落库格式一致。ON CONFLICT DO NOTHING
-- 幂等，重跑不覆盖已有桶（含服务端已聚合的桶）。
INSERT INTO host_metrics_history (agent_id, granularity, collected_at, metrics)
SELECT agent_id, 5, bucket,
       jsonb_object_agg(k, jsonb_build_object(
           'avg', round((sum_v / cnt)::numeric, 4),
           'min', min_v, 'max', max_v, 'last', last_v, 'cnt', cnt
       ))
FROM (
    SELECT agent_id, bucket, k,
           sum(nv) AS sum_v,
           min(nv) AS min_v,
           max(nv) AS max_v,
           (array_agg(nv ORDER BY collected_at DESC))[1] AS last_v,
           count(*) AS cnt
    FROM (
        SELECT h.agent_id,
               to_timestamp(floor(extract(epoch FROM h.collected_at) / 300) * 300) AS bucket,
               e.k,
               (e.v #>> '{}')::double precision AS nv,
               h.collected_at
        FROM host_metrics_history h
        CROSS JOIN LATERAL jsonb_each(h.metrics) AS e(k, v)
        WHERE h.granularity = 0
          AND jsonb_typeof(h.metrics) = 'object'
          AND jsonb_typeof(e.v) = 'number'
    ) samples
    GROUP BY agent_id, bucket, k
) key_stats
GROUP BY agent_id, bucket
ON CONFLICT (agent_id, granularity, collected_at) DO NOTHING;

-- ===== 7. host_metrics_history 层 + 时间复合索引 =====
-- 分层清理 DELETE（WHERE granularity = $1 AND collected_at < ...）、1h 滚降
-- pending 扫描（WHERE granularity = 5 AND 小时窗已完结）与原始层补聚合扫描
-- （WHERE granularity = 0 AND collected_at >= ...）均无 agent_id 前导列可用
-- 唯一索引，补建复合索引避免例行任务全表扫描。
CREATE INDEX IF NOT EXISTS idx_host_metrics_history_granularity_time
    ON host_metrics_history (granularity, collected_at);

-- ===== 8. check_position_overlap 自定义 SQLSTATE P0401 =====
-- U 位重叠由「默认 P0001 + 中文消息文本匹配」改为自定义错误码 P0401，
-- 应用层（cabinets/position 预检兜底）按错误码匹配，跨语言/客户端稳定。
-- 重复执行安全（CREATE OR REPLACE）。
CREATE OR REPLACE FUNCTION check_position_overlap() RETURNS TRIGGER AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM positions
        WHERE cabinet_id = NEW.cabinet_id
        AND id != NEW.id
        AND (
            (NEW.start_u BETWEEN start_u AND end_u)
            OR (NEW.end_u BETWEEN start_u AND end_u)
            OR (start_u BETWEEN NEW.start_u AND NEW.end_u)
            OR (end_u BETWEEN NEW.start_u AND NEW.end_u)
        )
    ) THEN
        RAISE EXCEPTION '机位U位范围重叠' USING ERRCODE = 'P0401';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ===== 9. devices 删除 location 自由文本列 =====
-- 空间归属由房间/工位/机柜/机位结构化字段承载，location 为全站无展示出口的
-- 冗余备注列。依赖顺序：第 3 节已按新结构重建 devices_with_details 视图
-- （不含该列），此处删列不再受视图依赖阻断；存量 location 值随删列丢弃。
-- 注意：数据管理导入按 spec 表头精确匹配（ensure_header_matches），升级前
-- 导出的 devices CSV 含 location 列，升级后导入将报 header_mismatch——
-- 存量数据应在升级前完成导入，或升级后从旧版本导出文件中手工剔除该列。
ALTER TABLE devices DROP COLUMN IF EXISTS location;

COMMIT;
