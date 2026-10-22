-- FOIMS 0.24.0 -> 0.25.x（最新 0.25.3）数据库迁移（每个中版本仅保留一份最新升级脚本）
-- 执行方式：以 foims 角色执行（本项目建库后已回收 PUBLIC 权限）
--   psql -U foims -d foims -f upgrade/0.25/upgrade_0.25.0.sql
--
-- 本脚本为 0.25 中版本的累计升级代码，按顺序整合原 0.25.0 与 0.25.1 两次
-- 数据库迁移（0.25.2 为纯前端修复、0.25.3 为纯文件组织，均无数据库对象
-- 变更），一次性将 0.24.0 库推进到 0.26 升级脚本所要求的 0.25.x 最新基线。
--
-- 可重入基线：0.24.0 / 0.25.0 / 0.25.1+ 任一状态均可执行。删列与改名语句
-- 带 IF EXISTS 守卫，已完成过的步骤自动跳过，不会因对象缺失中断事务。
--
-- 变更内容（一、agent 部署模型重构：共享注册令牌 + 首报自动注册）：
--   1. agents.status 值域收缩为 active|offline|disabled：
--      - revoked（吊销）并入 disabled（禁用，同为拒收上报）；
--      - pending（下载占位）行删除：新版下载配置包不建行，注册由
--        agent 首报自动完成；
--   2. status 默认值 'pending' -> 'active'（首报自动注册即 active）；
--   3. 删除 token_hash 列：per-install 令牌废除，上报鉴权改为比对
--      系统配置的共享注册令牌（agent.enrollment_token）哈希。
--
-- 变更内容（二、消除表名歧义：device_macs -> snmp_macs）：
--   该表存储设备 SNMP 观测到的 ARP/FDB 条目（IP ↔ MAC ↔ 学习端口 ↔
--   VLAN），原名易与端口台账表 device_interfaces 的 mac_address 字段语义
--   混淆，更名以明确其「SNMP 观测数据」定位。仅改名，列结构与数据不变。
--   索引随表改名语义不变，名称同步更名保持命名一致；数据导入/导出的表
--   注册名同步变更为 snmp_macs，旧版本导出的备份中 device_macs 表导入会
--   因表名未知而失败，属预期行为（观测数据可由「数据采集 → SNMP MAC
--   同步」重新拉取）。
--
-- 注意：本脚本执行后，已部署 agent 的旧令牌不再被接受，需在 Web 端
-- 重新下载配置包覆盖安装（二进制安装包改由 Release 页按需下载，
-- 配置包由服务端按所选操作系统现场打包）。

BEGIN;

-- ---------- 一、agent 部署模型重构 ----------
-- 1. revoked 吊销态并入 disabled（语义一致：拒收该机上报）
UPDATE agents SET status = 'disabled' WHERE status = 'revoked';

-- 2. 删除下载占位产生的 pending 行（从未上报过，无数据可保留）
DELETE FROM agents WHERE status = 'pending' AND source = 'agent';

-- 3. 默认状态改为 active（首报自动注册即在线）
ALTER TABLE agents ALTER COLUMN status SET DEFAULT 'active';

-- 4. 删除 per-install 令牌列（列级 UNIQUE 索引随列一并删除）
--    IF EXISTS：0.25.0 基线库已删过该列，重入时跳过
ALTER TABLE agents DROP COLUMN IF EXISTS token_hash;

-- ---------- 二、消除表名歧义：device_macs -> snmp_macs ----------
-- 表改名（列/约束/触发器随表自动继承）
--    IF EXISTS：0.25.1+ 基线库表名已是 snmp_macs，重入时整条跳过
ALTER TABLE IF EXISTS device_macs RENAME TO snmp_macs;

-- 索引同步更名（同样按已改名状态跳过）
ALTER INDEX IF EXISTS idx_device_macs_device_id RENAME TO idx_snmp_macs_device_id;
ALTER INDEX IF EXISTS idx_device_macs_ip_address RENAME TO idx_snmp_macs_ip_address;
ALTER INDEX IF EXISTS idx_device_macs_mac_address RENAME TO idx_snmp_macs_mac_address;

COMMIT;
