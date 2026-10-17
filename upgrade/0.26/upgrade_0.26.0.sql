-- FOIMS 0.25.x → 0.26.0 数据库升级脚本
-- 执行前置：停止 foims 服务、完成数据库备份；本脚本仅允许在 0.25.x 最新
-- （0.25.3）库上执行一次，ALTER 语句无幂等保护，重复执行将报错回滚。
-- 变更概览：
--   1. 工位管理人重构：manager 文本列按名字回填 manager_employee_id 后删除，
--      manager_employee_id 置必填并收紧为 ON DELETE RESTRICT；
--   2. device_interfaces.trunk_id 改名 native_vlan_id（消除与 VLAN 语义混淆）；
--   3. 五表改名消歧义：agents→monitored_hosts、agent_metrics_history→
--      host_metrics_history、agent_alert_states→host_alert_states、
--      snmp_macs→snmp_arp_entries、ips→interface_ips（索引/约束随表名联动）；
--   4. monitored_hosts 补 status/source 枚举 CHECK 约束（与其余枚举列一致）；
--   5. 布尔列语义化：users.status→enabled、operation_logs.result→success。
-- 视图（ip_with_details）与外键经 PostgreSQL OID 依赖自动跟随表改名，
-- updated_at 触发器绑定表对象同样自动跟随，均无需处理。

BEGIN;

-- ---------- 1. 工位管理人重构 ----------
-- 按旧 manager 文本列的名字匹配 employees.name 回填 manager_employee_id
UPDATE workstations w
SET manager_employee_id = e.id
FROM employees e
WHERE w.manager_employee_id IS NULL AND w.manager = e.name;

-- 无法回填（manager 为空或无同名员工）的工位删除：布局行级联清理、
-- 设备引用置空（workstation_layouts ON DELETE CASCADE、devices ON DELETE SET NULL）
DELETE FROM workstations WHERE manager_employee_id IS NULL;

ALTER TABLE workstations DROP CONSTRAINT workstations_manager_employee_id_fkey;
ALTER TABLE workstations ALTER COLUMN manager_employee_id SET NOT NULL;
ALTER TABLE workstations ADD CONSTRAINT workstations_manager_employee_id_fkey
    FOREIGN KEY (manager_employee_id) REFERENCES employees(id) ON DELETE RESTRICT;
ALTER TABLE workstations DROP COLUMN manager;

-- ---------- 2. 设备接口列改名 ----------
ALTER TABLE device_interfaces RENAME COLUMN trunk_id TO native_vlan_id;

-- ---------- 3. 五表改名 ----------
ALTER TABLE agents RENAME TO monitored_hosts;
ALTER TABLE agent_metrics_history RENAME TO host_metrics_history;
ALTER TABLE agent_alert_states RENAME TO host_alert_states;
ALTER TABLE snmp_macs RENAME TO snmp_arp_entries;
ALTER TABLE ips RENAME TO interface_ips;

-- 索引随表名联动
ALTER INDEX idx_agents_machine_id RENAME TO idx_monitored_hosts_machine_id;
ALTER INDEX idx_agents_device_id RENAME TO idx_monitored_hosts_device_id;
ALTER INDEX idx_agent_metrics_history_agent_time RENAME TO idx_host_metrics_history_agent_time;
ALTER INDEX idx_snmp_macs_device_id RENAME TO idx_snmp_arp_entries_device_id;
ALTER INDEX idx_snmp_macs_ip_address RENAME TO idx_snmp_arp_entries_ip_address;
ALTER INDEX idx_snmp_macs_mac_address RENAME TO idx_snmp_arp_entries_mac_address;
ALTER INDEX idx_ips_device_interface_id RENAME TO idx_interface_ips_device_interface_id;
ALTER INDEX idx_ips_subnet_id RENAME TO idx_interface_ips_subnet_id;

-- 约束随表名联动
ALTER TABLE monitored_hosts RENAME CONSTRAINT agents_device_id_fkey TO monitored_hosts_device_id_fkey;
ALTER TABLE host_metrics_history RENAME CONSTRAINT agent_metrics_history_agent_id_fkey TO host_metrics_history_agent_id_fkey;
ALTER TABLE host_alert_states RENAME CONSTRAINT agent_alert_states_agent_id_fkey TO host_alert_states_agent_id_fkey;
ALTER TABLE interface_ips RENAME CONSTRAINT uq_ips_ip_address TO uq_interface_ips_ip_address;
ALTER TABLE interface_ips RENAME CONSTRAINT chk_ips_status TO chk_interface_ips_status;

-- ---------- 4. monitored_hosts 枚举 CHECK 约束 ----------
ALTER TABLE monitored_hosts ADD CONSTRAINT chk_monitored_hosts_source
    CHECK (source IN ('agent', 'snmp'));
ALTER TABLE monitored_hosts ADD CONSTRAINT chk_monitored_hosts_status
    CHECK (status IN ('active', 'offline', 'disabled'));

-- ---------- 5. 布尔列语义化 ----------
ALTER TABLE users RENAME COLUMN status TO enabled;
ALTER TABLE operation_logs RENAME COLUMN result TO success;

COMMIT;
