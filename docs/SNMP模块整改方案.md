# SNMP 采集模块整改方案

> 日期：2026-11-19
> 范围：SNMP 实时采集、5 分钟全量轮询、设备 ARP/FDB 同步、LLDP 同步、相关限流与日志
> 说明：本文基于静态代码排查与开源项目实践对照，未在生产环境启动服务或执行自动化测试。

## 1. 背景与总体结论

此前多次按单点需求调整参数，但在运行中被证明效果不及预期。根因不是某个参数选错，而是缺少三层边界：

1. **任务准入**：哪些任务可以同时跑、能占多少并发；
2. **单设备串行**：同一台设备任何时刻只允许一个 SNMP 任务；
3. **端到端时限**：每个任务都有明确超时，超时即失败并释放资源。

因此整改顺序应为：**先观测取证，再补边界，最后调频率**。所有改动按配置开关发布，先小范围灰度，保留回滚路径。

## 2. 开源项目 SNMP 工作机制调研

| 项目 | 采集节奏 | 报文方式 | 超时 / 重试 | 关键机制 |
|---|---|---|---|---|
| Prometheus snmp_exporter | 由 Prometheus 抓取周期决定，常见 15~60s | GETBULK，`max_repetitions=25` | timeout 5s、retries 3 | 只 walk 配置中列出的 OID；可选响应缓存；无 1s 常驻轮询 |
| LibreNMS | 默认 300s；1 分钟轮询为专门优化的特殊模式；发现/清单 6 小时 | GETBULK，`max_repeaters=30` | timeout 1s、retries 5 | 每设备独立 poller 进程天然串行；分布式 poller / poller group 扩容；监控 poll 时长 |
| Zabbix | SNMP item 常用 1m（由 item 配置决定） | GETBULK，`max repetitions=10` | retries 默认 5，且整个 walk 共享重试预算 | get[] / walk[] 分离；异步 poller；发现与采集分离 |
| Telegraf | agent 默认 10s | GETBULK，`max_repetitions=10` | timeout 5s、retries 3 | `stop_on_error`，失败即停止 walk |
| Cisco 官方建议 | 指标决定频率 | — | 生产环境 timeout ≥3.5s，优先动态超时 | 1s 超时只适用于实验室；慢变指标 300s |

### 2.1 提炼出的六条共识

1. **发现与采集分离**：清单/能力探测是低频动作（30 分钟~24 小时），不能与秒级采集混在一起。
2. **频率跟着指标走**：设备可用性 30~60s、接口计数器 30~60s、CPU/内存 60s、磁盘容量 300s；秒级只用于排障或用户正在看的设备。
3. **GETBULK + 显式上限**：表数据用 bulk，`max-repetitions` 按厂商能力配置 10~25，不做逐 OID GET。
4. **每设备串行 + 全局并发预算**：同一设备任何时刻只有一个采集任务；超时或落后就跳过，不排队堆积。
5. **超时/重试是显式预算**：walk 的重试次数是整条 walk 的预算，不是每个 OID 各重试一次。
6. **不可达设备退避**：失败指数退避，避免对宕机设备持续打满超时。

## 3. 本项目现状对照

### 3.1 已经做对的部分（不要推翻）

- 实时路径是**每设备常驻 worker**，启动错峰、失败指数退避、全局 `Semaphore` 限并发（`crates/foims-agent-service/src/live_poll.rs#L353-L404`），同设备不会自我重叠。
- 实时路径使用精确 GET、20 OID/包，按 IPv6 MTU 安全线设计（`crates/foims-snmp/src/live/mod.rs#L62-L66`）。
- 全量 walk 使用 async-snmp 默认 GETBULK（`max_repetitions=25`），与 snmp_exporter 默认值一致。
- 全量轮询 5 分钟、设备并发 8（`crates/foims-agent-service/src/snmp_poll.rs#L32-L33`），节奏符合业界常规。

### 3.2 真正的偏差

| 问题 | 现状 | 后果 |
|---|---|---|
| 跨路径无协调 | 实时 worker、5 分钟全量、手动 ARP/LLDP 同步是三套独立入口，没有共享的设备锁 | 同一台设备可能同时被 2~3 种任务打，SNMP 响应变慢、超时率升高 |
| 手动同步无边界 | 默认重试（丢包时单步最坏约 23s）+ 无总超时 + 同步阻塞 HTTP 请求（`crates/foims-snmp/src/collect/arp.rs#L148-L162`） | FDB 大表同步可挂几分钟，表现为 `waiting=1`、页面卡死 |
| 数据库写入方式重 | ARP 逐行 SAVEPOINT + upsert，最后一次大数组 DELETE（`crates/foims-resource/src/device/mac.rs#L68-L132`） | 万行 FDB 产生数万次往返、长事务、WAL/锁压力 |
| 秒级频率一刀切 | 所有 SNMP 设备默认 1s，不管是否有人查看、是否核心设备（`crates/foims-common/src/config.rs#L346-L362`） | 设备控制面 CPU 与采集端 UDP 包量随设备数线性上升，收益有限 |
| 配置是死配置 | `[snmp] timeout_secs/retries/lldp_timeout_secs/mac_scan_timeout_secs` 无任何读取方，实际硬编码 5s/默认重试（`crates/foims-common/src/config.rs#L317-L326`、`crates/foims-snmp/src/core/mod.rs#L23-L25`） | 运维以为可调，实际不生效 |
| 限流维度错位 | HTTP 用户桶 200/min，重任务只算 1 个令牌；正常页面请求反而可能被 429（`crates/foims-common/src/rate_limit.rs#L194-L247`） | 没有保护设备，只保护了 API |
| 语义混淆 | IP 页「拉取 MAC」只读库，设备页「同步 ARP」才走 SNMP | 使用者误以为点「拉取」就刷新了设备数据 |

## 4. 整改方案

### P0：只加观测，不改行为（先拿到证据）

- 每次 SNMP 任务输出一条结构化日志：`path=live|full|mac|lldp`、`device_id`、`oid/表名`、`rows`、`batches`、`retries`、`duration_ms`、`result`、`queue_wait_ms`。
- 增加「同设备并发检测」：任务进入时对设备 in-flight 计数 +1，检测到 >1 时打印告警日志（暂不拦截）。
- 记录每轮实时采样的 `lag`（本轮结束时间 - 计划开始时间）、失败分块数、每设备每秒实际报文数。
- 先跑 24~72 小时基线，再决定任何默认值；避免再次「拍脑袋改参数」。

### P1：补齐边界（低风险、立刻稳住生产）

1. **共享设备闸门**：新增 `SnmpJobGate`，实时/全量/手动同步三路都先拿 `per-device Mutex`，再拿全局/分路 `Semaphore`，拿到锁才建连采样。
2. **手动同步去重**：同设备已有同步任务时返回 `409 + Retry-After`，不排队。
3. **端到端时限**：每个任务 `tokio::time::timeout`；MAC/LLDP 同步给总预算（建议 120s），超时返回明确错误并落日志，不让 HTTP 请求长时间挂起。
4. **重试预算化**：全量轮询保留 2~3 次重试（生产网络允许丢包）；手动同步改为 0~1 次重试 + 总超时；实时路径保持 `Retry::none()`（`crates/foims-agent-service/src/live_poll.rs#L549-L553`）。
5. **落后即跳过**：固定延迟调度；上一轮耗时超过间隔或队列等待超阈值时，本轮直接跳过并计数，不补跑。
6. **让配置生效**：读取 `[snmp]` 的 timeout/retries；删除无法生效的字段或真正接线，避免误导运维。

### P2：降频与订阅化（解决 1s 一刀切）

- 实时默认改为 **5s**；只有「有人正在看/排障订阅」的设备降到 **1s**，并限制实时设备数（例如 32 台）。
- 无订阅者时回落到 30~60s 基线，仅保留告警评估所需指标。
- 指标分层：资源类 60s、端口 up/down 30s、监控端口流量 10~30s、全端口计数器 300s、清单探测 6 小时 + 手动触发。
- 端口状态改为按列 GETBULK（`max-repetitions` 10~25，按厂商可调），标量资源组继续使用精确 GET；保留 20 OID/包的 MTU 安全线。
- 只对 `snmp_enabled + monitored` 设备建 worker；虚拟接口在探测期过滤，避免「先全量 walk 再截断」。

> **落地状态（v0.88.0）**：已实现订阅驱动分级采样——`LiveDemand` 四档（Baseline 30s 仅资源组 / Overview 5s 资源组+采集端口流量 / Watch 1s 仅资源组 / Detail 1s 全端口），SSE 带 `scope=dashboard|agents` 或 `device_id` 登记租约（TTL 15s），断连后自动回落基线；采集侧按 `LiveSampleSpec` 裁剪每轮 OID。尚未落地：端口状态 GETBULK、worker 只建 `snmp_enabled + monitored` 设备、实时设备数硬上限（当前仅限制单条订阅登记 500 台）。

### P3：重活改造（手动同步与写入）

- MAC/LLDP 同步改成**异步任务**：`POST` 返回 `202 + job_id`，`GET /jobs/{id}` 或 SSE 推送进度；前端不再阻塞等待。
- FDB 结果设置上限（例如 20 万行），使用 async-snmp 的 `result_limit` 中止并明确报「数据超限」，避免无限占用内存。
- 入库改为**批量 upsert**（`unnest`/临时表 + `ON CONFLICT`），每批 500~1000 行；清理使用 `NOT EXISTS`；设置 `statement_timeout` / `lock_timeout`。
- 事件记录抑制基线噪声、批量写入、限制单次事件数。
- 同步接口独立限流：每设备 1 并发、每用户 2 并发、每设备每小时 N 次；与页面读接口分开计费。
- 前端语义拆分：「刷新设备 ARP/FDB（SNMP）」与「用已有快照校准 IP 台账」两个明确动作。

### P4：规模化（确认需要再做）

- 按设备 hash 分片到多 poller，使用数据库 advisory lock 保证同一设备只被一个实例采集。
- 若后续需要保留秒级趋势，再引入时序存储；当前实时快照「只进内存、不落库」的设计是合理的，不要为了实时把 1s 数据写入 OLTP。

## 5. 建议的初始参数（灰度起点，不是最终值）

```toml
[snmp]
timeout_secs = 3
retries = 2
max_repetitions = 25          # 厂商有兼容问题时按设备/厂商下调到 10

[snmp.live]
interval_secs = 5             # 默认
realtime_interval_secs = 1    # 仅订阅/排障设备
realtime_max_devices = 32
only_when_subscribed = true
timeout_secs = 2
max_concurrency = 32

[snmp.poll]
interval_secs = 300
max_concurrency = 8
inventory_refresh_secs = 21600

[snmp.sync]
total_timeout_secs = 120
max_concurrent = 2
fdb_result_limit = 200000
batch_size = 1000
```

## 6. 验收标准与回滚

- 24 小时内**同一设备并发任务数为 0**；所有任务都在 deadline 内结束。
- 实时默认 5s 后，采集端 UDP 包量和设备 SNMP 超时率下降 ≥50%，告警时效不变。
- 5 分钟全量轮询 p95 周期 < 300s；DB 连接池 active 峰值 < 50%。
- 正常页面操作不再出现 429；同步接口由任务准入返回 409/429，并带 `Retry-After`。
- 1 万行 ARP/FDB 同步 p95 < 30s（改造后）。
- 每个阶段独立配置开关；按设备分组灰度（先 5% 非核心设备），异常可单独回退，不影响其他路径。

## 7. 明确不要做的事

- 不要把全设备都设成 1s；不要为了「实时」牺牲设备稳定性。
- 不要把所有路径重试都设为 0（丢包网络会误判离线）；用手动同步的总超时兜底。
- 不要用「调高 200/min」消除 429，那是症状不是根因。
- 不要对大数据表逐 OID GET，也不要在没有结果上限的情况下定时跑全量 FDB。
- 不要一次性全量上线；按设备分组灰度并保留回滚开关。

## 8. 附录：两条现场日志的解读

### 8.1 `Rate limit triggered: key=user:<uuid>`

- 来源：`charge_user`（`crates/foims-common/src/rate_limit.rs#L194-L247`），表示该用户 60 秒内累计 API 请求达到 200 次。
- 与 SNMP 数据量无关；SNMP 重任务每次只计 1 个令牌。
- 该日志没有 `method/path`，无法定位具体接口；用户维度拦截也不会输出带 path 的 `log.rate_limit.blocked`。
- 常见来源：页面切换/弹窗批量查询、多标签页、SSE 断线重连（每次重连都是一次请求）。

### 8.2 `Pool status - active: 1, idle: 6, waiting: 1, size: 7`

- `waiting` 不是数据库连接池排队数，而是「仍在执行中的 API 请求数」（`src/main.rs#L93-L110`）；`active/idle/size` 才是 DB 连接。
- 含义：DB 池健康（1 条连接在用、6 条空闲），当前有 1 个 HTTP 请求尚未返回。
- 该日志在 `waiting>0` 或连接使用率 >80% 时按健康检查周期打印（`crates/foims-common/src/db.rs#L466-L476`），本身不是错误。
- 结合时间线，最合理的解释是有一个耗时较长的 SNMP 同步请求正在执行，与「手动同步无总超时、默认重试」的问题对应。

## 9. 参考资料

- [Prometheus snmp_exporter generator 配置（max_repetitions / retries / timeout）](https://pkg.go.dev/github.com/prometheus/snmp_exporter/generator)
- [Grafana Agent snmp_exporter_config 文档](https://grafana.com/docs/agent/v0.32/configuration/integrations/snmp-config/)
- [LibreNMS 性能与轮询文档](https://docs.librenms.org/Support/Performance/)
- [LibreNMS 1 分钟轮询说明](https://docs.librenms.org/Support/1-Minute-Polling/)
- [LibreNMS Dispatcher / 分布式 Poller](https://docs.librenms.org/Extensions/Dispatcher-Service/)
- [LibreNMS 自动发现默认 6 小时](https://docs.librenms.org/Extensions/Auto-Discovery/)
- [Zabbix SNMP agent（bulk 重试与最大重复数）](https://www.zabbix.com/documentation/current/en/manual/config/items/itemtypes/snmp)
- [Telegraf SNMP input 配置（timeout / retries / max_repetitions）](https://docs.influxdata.com/telegraf/v1/input-plugins/snmp/)
- [LogicMonitor SNMP 规模化实践（60s/30s/300s 分层）](https://www.logicmonitor.com/blog/the-comprehensive-guide-to-snmp/)
- [Netdata：SNMP 超时与重试的调优取舍](https://www.netdata.cloud/guides/network/network-snmp-timeouts-retries/)
- [Cisco IOS XR SNMP Best Practices（生产超时 ≥3.5s）](https://www.cisco.com/c/en/us/td/docs/ios_xr_sw/iosxr_r3-9-1/mib/guide/crs-gsr_appe.pdf)
