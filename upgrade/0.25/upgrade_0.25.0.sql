-- FOIMS 0.24.0 -> 0.25.0 数据库迁移（每个中版本仅保留一份最新升级脚本）
-- 执行方式：以 foims 角色执行（本项目建库后已回收 PUBLIC 权限）
--   psql -U foims -d foims -f upgrade/0.25/upgrade_0.25.0.sql
--
-- 变更内容（agent 部署模型重构：共享注册令牌 + 首报自动注册）：
--   1. agents.status 值域收缩为 active|offline|disabled：
--      - revoked（吊销）并入 disabled（禁用，同为拒收上报）；
--      - pending（下载占位）行删除：新版下载配置包不建行，注册由
--        agent 首报自动完成；
--   2. status 默认值 'pending' -> 'active'（首报自动注册即 active）；
--   3. 删除 token_hash 列：per-install 令牌废除，上报鉴权改为比对
--      系统配置的共享注册令牌（agent.enrollment_token）哈希。
--
-- 注意：本脚本执行后，已部署 agent 的旧令牌不再被接受，需在 Web 端
-- 重新下载配置包覆盖安装（二进制安装包改由 Release 页按需下载，
-- 配置包由服务端按所选操作系统现场打包）。

BEGIN;

-- 1. revoked 吊销态并入 disabled（语义一致：拒收该机上报）
UPDATE agents SET status = 'disabled' WHERE status = 'revoked';

-- 2. 删除下载占位产生的 pending 行（从未上报过，无数据可保留）
DELETE FROM agents WHERE status = 'pending' AND source = 'agent';

-- 3. 默认状态改为 active（首报自动注册即在线）
ALTER TABLE agents ALTER COLUMN status SET DEFAULT 'active';

-- 4. 删除 per-install 令牌列（列级 UNIQUE 索引随列一并删除）
ALTER TABLE agents DROP COLUMN token_hash;

COMMIT;
