-- 设备可视化改造：线路去重 + 拓扑连线类型化（物理/逻辑）+ 链路聚合成员表
-- 说明：本项目不使用迁移框架，此脚本为变更留档，需在库上直接执行；
-- ipma-init 的建表代码（schema/tables/*）与 check.rs 校验清单已同步。

-- 1) cable_links 去重：同两端点之间只允许一根线路（物理限制）
DELETE FROM cable_links a
USING cable_links b
WHERE a.ctid < b.ctid
  AND a.a_endpoint_type = b.a_endpoint_type
  AND a.a_endpoint_id = b.a_endpoint_id
  AND a.b_endpoint_type = b.b_endpoint_type
  AND a.b_endpoint_id = b.b_endpoint_id;

CREATE UNIQUE INDEX IF NOT EXISTS uq_cable_links_endpoint_pair
  ON cable_links (a_endpoint_type, a_endpoint_id, b_endpoint_type, b_endpoint_id);

-- 2) topology_connections：区分物理/逻辑连接
ALTER TABLE topology_connections
  ADD COLUMN IF NOT EXISTS connection_type VARCHAR(20) NOT NULL DEFAULT 'physical';

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'chk_topology_connections_type'
  ) THEN
    ALTER TABLE topology_connections
      ADD CONSTRAINT chk_topology_connections_type
      CHECK (connection_type IN ('physical', 'logical'));
  END IF;
END $$;

-- 旧唯一约束对含 NULL 端口的行无法去重（PostgreSQL NULL != NULL），废弃；
-- 物理设备连线改为基于 cable_links 实时派生，历史自动发现行清除
ALTER TABLE topology_connections DROP CONSTRAINT IF EXISTS uq_topology_connection;
DELETE FROM topology_connections WHERE auto_discovered = TRUE AND connection_type = 'physical';

-- 同一对设备之间只允许一条逻辑连接（链路聚合）
CREATE UNIQUE INDEX IF NOT EXISTS uq_topology_connections_logical
  ON topology_connections (
    LEAST(source_device_id, target_device_id),
    GREATEST(source_device_id, target_device_id)
  )
  WHERE connection_type = 'logical';

-- 3) 链路聚合成员表：逻辑连接两端的成员端口组
CREATE TABLE IF NOT EXISTS topology_connection_members (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
  connection_id UUID NOT NULL REFERENCES topology_connections(id) ON DELETE CASCADE,
  device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
  device_port_id UUID NOT NULL REFERENCES device_ports(id) ON DELETE CASCADE,
  side VARCHAR(10) NOT NULL CHECK (side IN ('source', 'target')),
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  CONSTRAINT uq_topology_connection_member_port UNIQUE (connection_id, device_port_id)
);

CREATE INDEX IF NOT EXISTS idx_tcm_connection ON topology_connection_members(connection_id);
CREATE INDEX IF NOT EXISTS idx_tcm_device_port ON topology_connection_members(device_port_id);
