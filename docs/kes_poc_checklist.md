# KingbaseES（人大金仓）兼容性 POC 验证清单

> 目标：验证 FOIMS 后端能否以最小改动连接并运行在 KingbaseES 上。
> 前提假设：KES 是 PostgreSQL 分支（V8 R6 基于约 PG 12 内核），走 PG wire 协议，
> 理论上 sqlx `postgres` 驱动可直接连接。本清单逐项验证该假设的成立范围。
>
> 验证方式：本地拉起 KES 实例 → 跑 foims-init → 全量功能回归 → 记录失败点。
> 每个验证项标注：✅ 通过 / ❌ 失败（附报错与复现步骤）/ ⚠️ 通过但有语义差异。

## 一、环境准备

- [ ] 确定 KES 目标版本（建议 V8 R6 及以上，与供应商确认内核对应 PG 版本）
- [ ] 拉起实例：官方 Docker 镜像或裸机安装，记录系统视图/兼容模式参数（`ENABLE_CI`、大小写敏感等关键参数）[此处记录实际版本与镜像源]
- [ ] 创建测试库与账号，授予建扩展/建表/建触发器权限
- [ ] 确认实例监听地址可达（含 IPv6 场景，项目要求 IPv6 友好，见 db.rs 的 `[]` 包裹逻辑）

## 二、连接与驱动层（crates/foims-common/src/db.rs）

- [ ] sqlx `postgres` 驱动连接 KES 成功（含密码认证方式，MD5/SCRAM）
- [ ] `DbPool::new_with_config` 构造的连接串格式被 KES 接受
- [ ] 连接参数 `username/password` 含特殊字符时 `url_encode_component` 路径正常
- [ ] `after_connect` 中的 `SET statement_timeout = N` 在 KES 上生效（查询超时能中断慢查询）
- [ ] 连接池 `min_connections/max_connections/idle_timeout/max_lifetime` 行为正常
- [ ] `SELECT 1` 健康检查通过
- [ ] 连接异常恢复（重启 KES 后应用能自动重连）

## 三、初始化链路（crates/foims-init）

### 3.1 扩展

- [ ] `CREATE EXTENSION IF NOT EXISTS "uuid-ossp"` 成功（schema/tables/mod.rs:34）
- [ ] `uuid_generate_v4()` 可用（users 等全部 UUID 主键表依赖此函数）
  - 若 KES 无 uuid-ossp：验证 `gen_random_uuid()` 是否可用，评估替换成本

### 3.2 表 DDL

逐表执行 foims-init 建表 SQL，重点类型：

- [ ] `UUID`、`TIMESTAMP WITH TIME ZONE`、`BOOLEAN`、`BIGSERIAL`（agents 表）全部可建
- [ ] `JSONB` 列可建可读写（涉及 device_inventory、agents、snmp_events、org_templates、logs、system 等表）
- [ ] `INET` / `CIDR` 列可建（network、interface_ips、network_cidrs 等表）
- [ ] `INET[]` 数组列可建（network.rs:27-28 `ipv4_dns/ipv6_dns`）
- [ ] CHECK 约束、UNIQUE 约束、外键 `ON DELETE CASCADE`、复合索引（含 `DESC` 索引）全部可建

### 3.3 视图（schema/tables/views.rs）

- [ ] 5 个视图全部可创建：ip_with_details、devices_with_details、net_outlets_with_details、patch_panels_with_details、cable_links_with_details
- [ ] 视图查询语义与 PG 一致（重点看其中 INET 展开与 JOIN 写法）

### 3.4 触发器（schema/tables/triggers.rs）

- [ ] PL/pgSQL 函数 `CREATE OR REPLACE FUNCTION ... $$ LANGUAGE plpgsql` 可创建
  （update_updated_at_column、check_position_overlap 等）
- [ ] `IS NOT DISTINCT FROM` 语法支持
- [ ] `CREATE TRIGGER ... FOR EACH ROW EXECUTE FUNCTION ...` 语法支持（注意 KES 是否要求 `EXECUTE PROCEDURE`）
- [ ] 触发器行为验证：正常 UPDATE 后 updated_at 自动刷新；显式 SET updated_at 时不被覆盖
- [ ] 位置重叠校验触发器（check_position_overlap）与 PG 行为一致

### 3.5 check.rs 校验清单

- [ ] `check.rs` 全部结构校验查询通过（校验 SQL 里若有系统目录查询需逐条核对）

## 四、SQL 方言核心特性（业务代码逐类验证）

> 这部分是风险集中区，按特性分类跑最小用例；来源行号代表代码中已确认的使用位置。

| # | 特性 | 代码位置（示例） | 结果 |
|---|------|------------------|------|
| 1 | `ON CONFLICT (…) DO UPDATE`（约 54 处） | config.rs:884、password_policy.rs:103、import 路径等 | ⬜ |
| 2 | `INSERT … DO UPDATE … RETURNING` 组合 | agents_api.rs、ingest.rs、ipmi_poll.rs 等（13 处） | ⬜ |
| 3 | `ILIKE` 大小写不敏感匹配（大量列表/搜索接口） | user.rs:108、cabinets.rs:135 等 | ⬜ |
| 4 | `DISTINCT ON`（拓扑去重） | topology.rs:356 | ⬜ |
| 5 | `FOR UPDATE` 行锁、`FOR UPDATE OF 别名` | position.rs:435（`FOR UPDATE OF r`） | ⬜ |
| 6 | `make_interval(days => N)` / 命名参数 | password_policy.rs:270、ip.rs:942、ip.rs:999 | ⬜ |
| 7 | `::TIMESTAMPTZ` / `::uuid` / `::jsonb` 等强转 | 全仓大量 | ⬜ |
| 8 | `CAST($n AS INET)` 与 `<<=` 包含运算符 | ip.rs:311-312、import/mod.rs:673-674 | ⬜ |
| 9 | 数组绑定参数 `$8::INET[]` 及数组列读写 | network.rs:419 | ⬜ |
| 10 | `= ANY(ARRAY[...])` / `array_agg` | helpers.rs、topology.rs 等（57 处分布 19 文件） | ⬜ |
| 11 | JSONB 操作符 `->>`、`#>>`、`jsonb_agg`、`jsonb_array_elements … WITH ORDINALITY` | tasks.rs:306、upgrade/0.33 SQL | ⬜ |
| 12 | `NOW()`、`CURRENT_TIMESTAMP` 时间默认值 | 全仓 | ⬜ |
| 13 | `sqlx::QueryBuilder` 动态拼接 SQL（分页/过滤全路径） | helpers.rs、crud.rs 等 | ⬜ |
| 14 | `NULLS FIRST/LAST` 排序（如有） | 全仓检索 | ⬜ |

## 五、功能回归（全量业务流）

- [ ] `./da-scripts/pak.sh` 全量构建通过
- [ ] 初始化向导（init 入口）完整走通
- [ ] 登录/登出/令牌吊销（revoked_tokens）、fail2ban、密码策略、2FA
- [ ] 用户/组织/员工/模板 CRUD
- [ ] 房间/机柜/工位/设备/线缆全量 CRUD（含功率上限并发校验路径）
- [ ] IP 管理（INET 查询、CIDR 匹配）、网络管理
- [ ] 导入/导出（data-management，含事务内 FOR UPDATE 锁序）
- [ ] 备份/恢复（backup.rs）
- [ ] 通知链路：SMTP/Webhook/站内信（template 渲染不依赖库，但落库配置读写要过）
- [ ] 定时任务调度（scheduled_task + scheduler，含 FOR UPDATE）
- [ ] Agent 采集：ingest、alerts、alert_episode、metrics_aggregator（JSONB 高频写入路径）
- [ ] SNMP/IPMI 采集与事件落库
- [ ] 拓扑/可视化（DISTINCT ON、layout）
- [ ] 操作日志/登录日志/日志转发
- [ ] 分页响应（paged_response 各接口）字段正确
- [ ] 前端联调冒烟（admin/admin123 全菜单走查）

## 六、升级链路验证（upgrade/0.30 ~ 0.33+）

- [ ] 从 0.23.0 基线库逐步执行 upgrade/0.30…0.33 全部 SQL 成功
  （0.33 含 jsonb_agg/jsonb_array_elements/WITH ORDINALITY 的 JSON 迁移逻辑，重点验证）
- [ ] 迁移后 check.rs 校验通过
- [ ] 结论：确认「KES 版升级脚本可与 PG 版共用一份」还是「需要 KES 专用分支」

## 七、并发与稳定性

- [ ] 双实例并发写入（设备/导入/采集同时跑）无死锁异常（锁序依赖 FOR UPDATE，KES 锁行为需实测）
- [ ] 长时间采集压测（≥2h）连接池稳定，无连接泄漏告警
- [ ] 大 JSONB 写入（metrics_aggregator 聚合路径）性能可接受

## 八、POC 结论记录

| 项 | 结论 |
|----|------|
| KES 实测版本 | （记录版本号/兼容模式参数） |
| 连接驱动 | 原生 sqlx postgres 直接可用 / 需要改动： |
| 建表 DDL | 全部通过 / 失败清单： |
| 方言差异汇总 | 需改写特性列表（对应第四节编号）： |
| 共用迁移脚本可行性 | 可共用 / 需分支： |
| 整体评估 | 仅需文档级适配 / 需少量代码适配 / 需方言层重构 |
| 后续动作 | （如：是否需要为 KES 维护独立 upgrade 目录等） |

## 通过标准

1. 第 2~3 节全部通过（连接 + 初始化干净库成功）；
2. 第四节无 ❌，⚠️ 项均有明确的语义差异说明与影响评估；
3. 第五节功能回归全部通过；
4. 第六节得出迁移脚本共用结论；
5. 第七节无稳定性问题。

满足全部条件即判定：FOIMS 支持以「现状代码 + 适配项清单」直接对接 KingbaseES。
