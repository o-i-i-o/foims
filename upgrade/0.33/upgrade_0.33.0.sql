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
--   说明：foims-init 建表代码（schema/tables/devices.rs、agents.rs、views.rs、
--   users.rs）与 check.rs 表/列/列宽清单已同步新增。存量数据无需订正
--  （新列均为可空/带默认）。
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
    d.template_id, d.seller, d.location,
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

COMMIT;
