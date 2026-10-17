-- FOIMS 0.25.0 -> 0.25.1 数据库迁移（每个中版本仅保留一份最新升级脚本）
-- 执行方式：以 foims 角色执行（本项目建库后已回收 PUBLIC 权限）
--   psql -U foims -d foims -f upgrade/0.25/upgrade_0.25.1.sql
--
-- 变更内容（消除表名歧义）：
--   device_macs -> snmp_macs：该表存储设备 SNMP 观测到的 ARP/FDB
--   条目（IP ↔ MAC ↔ 学习端口 ↔ VLAN），原名易与端口台账表
--   device_interfaces 的 mac_address 字段语义混淆，更名以明确其
--   「SNMP 观测数据」定位。仅改名，列结构与数据不变。
--
-- 注意：
--   1. 索引随表改名语义不变，但名称同步更名保持命名一致；
--   2. 数据导入/导出的表注册名同步变更为 snmp_macs，旧版本导出的
--      备份中 device_macs 表导入会因表名未知而失败，属预期行为
--      （观测数据可由「数据采集 → SNMP MAC 同步」重新拉取）。

BEGIN;

-- 表改名（列/约束/触发器随表自动继承）
ALTER TABLE device_macs RENAME TO snmp_macs;

-- 索引同步更名
ALTER INDEX idx_device_macs_device_id RENAME TO idx_snmp_macs_device_id;
ALTER INDEX idx_device_macs_ip_address RENAME TO idx_snmp_macs_ip_address;
ALTER INDEX idx_device_macs_mac_address RENAME TO idx_snmp_macs_mac_address;

COMMIT;
