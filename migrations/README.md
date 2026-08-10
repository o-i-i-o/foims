# 数据库迁移脚本

本目录存放 IPMA 升级时需要手动执行的数据库迁移脚本。

IPMA 没有内置的自动迁移框架（初始化仅使用 `CREATE ... IF NOT EXISTS`，不会修改已存在的表结构）。
因此当现有表结构需要变更时，需要在此目录提供 SQL 脚本，由运维人员手动执行。

## 执行方式

1. 先备份数据库。
2. 连接到 IPMA 数据库后执行对应脚本，例如：

   ```bash
   psql -d <ipma_db_name> -f migrations/20260810_remove_net_outlet_peer_and_ids.sql
   ```

3. 执行完成后重启 IPMA 服务。

## 脚本清单

| 脚本 | 适用版本 | 说明 |
|---|---|---|
| `20260810_remove_net_outlet_peer_and_ids.sql` | 0.14.10 → 新版 | 移除 `net_outlets` 表的 peer_* 字段（peer_type/peer_room_id/peer_outlet_id/peer_switch_port_id）及 `device_interfaces.net_outlet_ids` 列。信息点之间的连接逻辑统一迁移到"线路"（cable_links）模块。 |
