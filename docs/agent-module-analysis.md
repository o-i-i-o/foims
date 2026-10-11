# FOIMS Agent 模块分析报告（agent / agent-service / SNMP Trap）

> 分析日期：2026-10-11　|　范围：`crates/foims-agent`（客户端）、`crates/foims-agent-service`（服务端采集面，含 SNMP Trap/轮询）、相关数据库表与 API、前端入口。
> 目的：沉淀现状（业务逻辑 / 代码逻辑 / 数据库 / API），为后续优化完善提供依据。§8 为优化建议清单。

---

## 1. 模块总览与架构

### 1.1 crate 划分

| Crate | 角色 | 核心文件 |
|---|---|---|
| `foims-agent` | 客户端二进制（node_exporter 的 Rust 重写 + H3 上报） | [main.rs](../crates/foims-agent/src/main.rs)、[reporter.rs](../crates/foims-agent/src/reporter.rs)、[collectors/](../crates/foims-agent/src/collectors)（33 个采集器）、[metric.rs](../crates/foims-agent/src/metric.rs) |
| `foims-agent-service` | 服务端数据采集面 crate | [lib.rs](../crates/foims-agent-service/src/lib.rs)（模块索引）、report_server / ingest / renew / cert / api / manifest / packaging / agents_api / alerts / tasks / snmp_poll / snmp_metrics / trap |
| `foims-common` | 共享类型 | [report.rs](../crates/foims-common/src/report.rs)（AgentReport 等）、[config.rs](../crates/foims-common/src/config.rs)（AgentConfig）、net（token_hash/IP 归一）、x509 |
| 主程序 `src/` | 路由挂载与调度注册 | [routes/mod.rs](../src/routes/mod.rs)（/api/agents nest）、[main.rs](../src/main.rs)（spawn_report_server_task、三个调度任务注册） |

### 1.2 端到端数据流

```
【下载分发】  管理页(前端) --GET /api/agents/dist|download--> manifest 加载 → 生成 token(库存哈希)
              → 建 agents(pending) 行 → 动态组包 zip/deb/rpm(二进制+agent.toml+ca.pem+client.pem/key+install.sh) → 流式返回

【指标上报】  foims-agent(被监控机)  --QUIC/H3 mTLS POST /agent/v1/report-->  report_server
              → ingest: Bearer token→SHA-256 查 agents → 校验 → 频控闸门(原子)
              → 事务: UPDATE agents 快照列 + INSERT agent_metrics_history(精简 JSON)
              → alerts: 边沿评估 → 站内通知(admin/sysadmin/secadmin)
              → 响应控制面 {report_interval, collectors, latest_version}

【证书续期】  agent 剩余寿命<30天 --POST /agent/v1/renew(经旧证书 mTLS)--> renew
              → 请求体 PEM 必须与连接 leaf 一致 → 剩余<90 天才重签 → 服务端同步替换磁盘 client 物料
              → agent 原子落盘新证书(0644/0600)，下一轮上报生效

【SNMP 采集】 调度器(5min) → snmp_poll: 轮询 devices 表中已配 SNMP 凭据的设备
              → snmp_metrics.collect(CPU/内存/磁盘/温度/流量/负载, 差值状态存 raw_metrics._snmp_state)
              → upsert agents(source='snmp', machine_id='snmp:{device_id}') + agent_metrics_history

【SNMP Trap】 UDP 162 --trap.rs--> community 白名单/v3 USM 校验 → 冷却窗口限流 → 站内通知

【调度闭环】  agent_offline(每分钟, interval×factor) / agent_history_cleanup(每日 03:40, 默认留 30 天)
              / agent_snmp_poll(每 5 分钟)
```

---

## 2. 客户端 foims-agent

### 2.1 CLI 与运行模式（[main.rs](../crates/foims-agent/src/main.rs)）

四种互斥模式（按 `--list/--format/--listen` 组合判定）：

1. **默认：H3 mTLS 常驻上报**——读 `/etc/foims-agent/agent.toml`（`--config` 可覆盖），证书目录缺省 `/etc/foims-agent`（`--cert-dir` / `FOIMS_AGENT_CERT_DIR` 覆盖）；`--once` 单次采集上报即退。
2. `--listen ADDR:PORT`：常驻 HTTP/1.1 `/metrics` 拉模式（demo 语义），手写请求行解析、并发上限 32、读写超时 10s。
3. `--format prometheus|json`：采集一次输出 stdout。
4. `--list` / `--version`：枚举采集器 / 版本（stderr 输出，stdout 只承载指标数据）。

约定：stdout 仅指标数据、日志走 tracing→stderr；`--only` 校验名称合法性（未知即报错退出码 2）。

### 2.2 采集器体系（[collectors/](../crates/foims-agent/src/collectors)）

- 33 个采集器已移植（批次 1 全量 + 部分批次 2/3/4），指标名/help/标签与 node_exporter 逐字对齐；移植规则与剩余批次见 [docs/agent-porting.md](agent-porting.md)。
- `default_collectors(filter)` 构造、`scrape()` 输出 `Vec<MetricFamily>`、`encode_text_all()` 渲染 Prometheus 文本格式。

### 2.3 Reporter 上报循环（[reporter.rs](../crates/foims-agent/src/reporter.rs)）

核心常量：间隔缺省 60s、clamp [10,3600]；断网缓存 10 条（满弃最旧）；退避 `interval×2^n` 封顶 600s；证书续期阈值 30 天；CPU 采样窗口 200ms；握手/请求超时各 10s；响应体上限 1 MiB。

| 环节 | 逻辑 |
|---|---|
| machine_id | `/etc/machine-id` → `/var/lib/dbus/machine-id` → 本地持久化 `/var/lib/foims-agent/machine-id`（uuid v4，0755/0644）→ 写盘失败回退内存随机值并告警 |
| 采集 | `collect_report()` 把 `collect_sync`（全量 scrape + /proc 读取 + 200ms CPU 差分窗口）经 `spawn_blocking` 移入阻塞池；diskstats/netdev 累计计数器在 Reporter 内做轮间差分（IOPS/bps/util），首轮/回绕/零间隔记 0 |
| 报文体 | `AgentReport`：machine_id/hostname/agent_version/collected_at(RFC3339) + system(os/kernel/arch/uptime) + cpu(usage_pct 差分/cores/load1/5/15) + memory(total/used/swap) + disks(filesystem 容量 × diskstats 差分) + nets(netdev 差分+errs 累计) + sensors(hwmon 温度/风扇 + thermal 热区) + processes |
| 主循环 | 启动立即上报 → 成功：fail_count=0、应用服务端下发间隔、补报缓存（逐条 1s，失败放回队头）、`maybe_renew_cert`；失败：缓存入队、指数退避；间隔 ±10% 抖动防齐发 |
| 链路 | `h3_exchange()` **每次请求重建 QUIC Endpoint 与连接**（TLS 物料从磁盘实时读取，续期换证后下一轮自动生效）；ALPN h3；域名/IP/IPv6 方括号均可 |
| 续期 | 读 `client.pem` 剩余寿命 <30 天 → POST `/agent/v1/renew`（请求体携带当前 PEM）→ 校验产物可解析后 `.new` 临时文件 + rename 原子替换（证书 0644 / 私钥 0600），坏物料拒绝落盘 |
| 控制面 | `report_interval` 下发 clamp [10,3600] 生效；`collectors` 开关与 `latest_version` 仅日志记录——**采集器开关与自更新均未实现** |

### 2.4 已知边界（客户端）

- `latest_version` 通告字段保留但比较逻辑已随「agent 版本独立自管理」移除；版本推送/自更新尚未实现（立项中）。
- token 明文存 agent.toml（0600），安装脚本落盘。
- machine_id 冲突（409）后仅退避重试，无带外解决手段（需人工在服务端删旧记录）。

---

## 3. 服务端 foims-agent-service

### 3.1 report_server：QUIC/H3 接收（[report_server.rs](../crates/foims-agent-service/src/report_server.rs)）

- `spawn_report_server_task`：`agent.enabled` 为真才启动；先 `cert::ensure_agent_certs()`（失败拒绝监听、不影响主进程），再绑 UDP（默认 `[::]:9100` 双栈）。
- mTLS 强制：唯一信任锚为站点 CA（`/etc/ssl/foims-ca/ca.pem`），无客户端证书在握手阶段被拒；ALPN `h3`。
- 每连接提取对端 leaf 证书 DER（供 renew 绑定身份）+ 对端 IP（IPv4-mapped 归一后入库）。
- 单请求处理：仅放行 `POST /agent/v1/report` 与 `POST /agent/v1/renew`；请求体流式读满、超 `max_report_bytes` 回 413；响应统一 JSON。
- h3 0.0.8 形态：`accept() → RequestResolver → resolve_request()`。

### 3.2 ingest：上报入库（[ingest.rs](../crates/foims-agent-service/src/ingest.rs)）

处理流水（无 ApiResponse 信封，错误为 `{"error": "..."}` 简单格式）：

1. **鉴权**：`Authorization: Bearer <64hex>` → SHA-256 → 查 `agents.token_hash`；未命中 401；`disabled/revoked` 403。
2. **校验**：JSON 解析 400；文本字段 trim/判空/长度上限（machine_id 128、hostname 255、os/kernel 128、arch/agent_version 32）；`cpu.usage_pct` 与各 disk `util_pct` ∈ [0,100]；温度 ∈ [-100,250]℃（防溢出 NUMERIC(5,1)），其他传感器 |v|≤9999.9；`collected_at` RFC3339 且与服务端偏差 ≤300s。
3. **machine_id 比对**：NULL 首报回填；非空不一致 → 409（令牌疑似被复用）。
4. **快照列**：mem_pct（total=0 → NULL）、disk_pct（各 FS used/total 最大值）、max_temp（kind=temp 最大值）、rx/tx_bps（网卡求和，仅入 history）。
5. **频控闸门（原子）**：`UPDATE agents … WHERE last_seen IS NULL OR last_seen < NOW() - interval/2`——闸门内嵌 UPDATE 的 WHERE，消除「查→判→写」并发绕过；rows_affected=0 时二次查行：仍存在 → 429 过频，已消失 → 401。UPDATE 失败且命中 `idx_agents_machine_id` 唯一冲突 → 409（同机重复安装）。
6. **事务**：UPDATE agents（pending/offline 首报翻 active、刷新快照列与 raw_metrics 全量 JSON、last_seen/first_seen）+ INSERT `agent_metrics_history`（`ON CONFLICT (agent_id, collected_at) DO NOTHING` 幂等去重），快照键 `{cpu,mem,disk,temp,rx_bps,tx_bps}`。
7. **告警评估**：`alerts::evaluate_report`（失败仅记日志，不影响已成功的上报响应）。
8. **响应控制面**：`ReportResponse { report_interval(≥下限), collectors(空), latest_version(服务端版本) }`。

### 3.3 renew：证书续期（[renew.rs](../crates/foims-agent-service/src/renew.rs)）

- 鉴权完全依赖 mTLS；请求体 PEM 的 DER 必须与连接对端 leaf 完全一致（防止借链路证书为任意 PEM 换发，不一致 403）。
- 剩余寿命 <90 天才签发（agent 端 30 天阈值，双阈值留余量），未到窗口 400。
- 重签同身份 client 证书（CN=foims-agent-client、clientAuth EKU、约 5 年），同步替换磁盘 CLIENT_CERT/CLIENT_KEY（下载组包即携带新证书）；QUIC 监听只锚定 CA，无需重启。
- 并发互斥：`ISSUE_LOCK` `try_lock`，并发续期直接明确报错不排队。

### 3.4 cert：证书物料管理（[cert.rs](../crates/foims-agent-service/src/cert.rs)）

- 物料目录 `/etc/ssl/foims-certs`：`agent_cert.pem/key`（server，SAN=本机全部网卡 IP + localhost，serverAuth）、`agent_client_cert.pem/key`（client，clientAuth，SAN 空）、`agent_ca_fingerprint`（签发时站点 CA 的 SHA-256 指纹）。
- 复用判定：四文件可读 + 指纹与当前 CA 一致 + 两证书含 PEM 头 → 复用；CA 轮换/指纹缺失 → 全量重签（防旧 CA 物料残留导致握手失败）。
- 不走 x509-management（其 serverAuth EKU 硬编码），直接 rcgen 手写签发；私钥 0600 独占创建；签发在 `spawn_blocking`；`ensure` 与 `renew` 共用互斥锁。

### 3.5 分发下载（[api.rs](../crates/foims-agent-service/src/api.rs)、[manifest.rs](../crates/foims-agent-service/src/manifest.rs)、[packaging.rs](../crates/foims-agent-service/src/packaging.rs))

- `GET /api/agents/dist`：清单摘要 `{available, agent_version, ca_ok, message?, targets[]}`；产物目录不可用仍 200（available=false 供前端置灰）。
- `GET /api/agents/download?target=&format=&label=`：清单 → 目标/格式校验（zip 恒可；deb/rpm 按架构映射）→ 站点 CA 与 client 证书读取 → 二进制 sha256+size 校验（阻塞线程）→ **上报地址解析**（三级优先：`use_public_url` 开关开启取 `server.public_url` 主机名+监听端口（未配置公共URL报参数错误）→ `report_url_override` 手动覆盖（host:port，缺端口补监听端口，裸 IPv6 拒绝）→ 监听地址推导（通配主机取请求 Host/URI authority 兜底）），白名单校验 → 生成 32 字节 token（库存 SHA-256 哈希）→ INSERT agents(pending) → 生成 agent.toml（server_addr+token+interval 60）→ 组包 zip/deb/rpm → 流式返回（Content-Disposition）。
- 失败回滚：组包失败/防御分支按返回 id 精确回删 pending 行（**地址解析失败发生在入库前**，不泄漏 pending 记录）。
- install.sh：优先 dist_dir 部署版本，缺失回退内置精简脚本（含 systemd unit 内联）。
- manifest.json 由 `scripts/build-agent.sh` 构建期产出（版本/各 target 二进制 sha256/size）；agent 版本独立自管理，版本门控已于 v0.8.1 彻底移除（ConfigProvider::server_version 与 semver 依赖一并删除）。

### 3.6 agents_api：管理接口（[agents_api.rs](../crates/foims-agent-service/src/agents_api.rs)）

| 端点 | 逻辑要点 |
|---|---|
| `GET /api/agents` | 分页（键 `items/total/page/page_size/total_pages`）；过滤 status 精确 + keyword ILIKE（`escape_like` 转义）hostname/label/ip；**不含 raw_metrics**；`ORDER BY last_seen DESC NULLS LAST`；NUMERIC 统一 cast float8 |
| `GET /api/agents/{id}` | 详情含 raw_metrics / first_seen / created_at；404 |
| `GET /api/agents/{id}/history?hours=1..=168` | 曲线窗口；**抽样下推 SQL**（窗口函数 rn/n，超 200 点按 `rn % ceil(n/200) = 1` 均匀抽，首尾保留），缺键补 null；返回 `{items:[{t,cpu,mem,disk,temp,rx_bps,tx_bps}]}`（非分页） |
| `PATCH /api/agents/{id}` | label（≤100 字符）与 status 白名单 `active|disabled|revoked`（pending/offline 不可手工设置）；动态 QueryBuilder；404 |
| `DELETE /api/agents/{id}` | history/alert_states 经 FK CASCADE 级联；SNMP 行随 device 删除级联（device_id FK） |

### 3.7 alerts：阈值告警（[alerts.rs](../crates/foims-agent-service/src/alerts.rs)）

- 配置存 `system_configs (config_type='agent', key='alert_thresholds')`：`{enabled, cpu_pct?, mem_pct?, disk_pct?, temp_c?}`；百分比 1-100、温度 1-200，None=不监控；全局一套。
- 评估点两处：① ingest 每条快照（实时）；② PUT 保存成功后 fire-and-forget 对全部 `source='agent' AND status='active'` 的快照列评估一轮（调低阈值立即生效）。
- 状态机：`agent_alert_states(agent_id, metric, alerting)`；**边沿触发**——false→true 先发通知再落状态（通知失败状态不翻转，下轮重试，宁重复不遗漏）；true→false 静默复位；读数缺失视为未超标并复位。
- 通知：站内通知（type=`agent_alert`），接收人为全部启用状态 admin/sysadmin/secadmin，标题/正文为前端 i18n key。
- 未覆盖：恢复（回落）通知、SNMP 设备指标告警（二期）、按主机差异化阈值。

### 3.8 tasks：调度任务（[tasks.rs](../crates/foims-agent-service/src/tasks.rs)、[main.rs](../src/main.rs)）

| 任务 | 周期 | 逻辑 |
|---|---|---|
| `agent_offline` | 每分钟 | `active AND source='agent' AND last_seen < NOW() - interval×factor` → offline（默认 factor=3，参数注册时从 AgentConfig 注入，任务配置可覆盖）；SNMP 行由 snmp_poll 按自身间隔×2 独立判定 |
| `agent_history_cleanup` | 每日 03:40 | 删除 `collected_at < NOW()-retention_days`（默认 30，上限 3650）的 history 行 |
| `agent_snmp_poll` | 每 5 分钟 | `snmp_poll::poll_all(pool, interval_secs)`，返回 ok/fail 计数 |

### 3.9 SNMP 子系统

**snmp_poll**（[snmp_poll.rs](../crates/foims-agent-service/src/snmp_poll.rs)）：
- 源：`devices` 表中「已配 SNMP community/username 且有管理 IP」的设备；并发受限逐台 `poll_and_upsert`。
- 采集：MIB-II 系统组（sysName/sysDescr/sysUpTime）+ `snmp_metrics::collect` 性能指标（CPU/内存/磁盘/温度/流量/负载，**差值状态持久化在 `agents.raw_metrics._snmp_state`**，轮间读改写）。
- 入库：upsert `agents`（source='snmp'、machine_id='snmp:{device_id}'、device_id FK）；同时写 `agent_metrics_history`。
- 离线：连续失败超阈值（间隔×2）将对应 source='snmp' 行置 offline。
- 已知约束：async-snmp 周期轮询必须 `.retry(Retry::none())`（错误 community 静默丢包 + 默认重试会拖 23s/请求）。

**snmp_metrics**（[snmp_metrics.rs](../crates/foims-agent-service/src/snmp_metrics.rs)）：二期指标集（CPU/存储/内存/swap/磁盘/温度/流量/负载），`collect` 返回 SnmpSnapshot 与新差值状态；快照键与 agent 路径一致，供详情网络曲线共用。

**trap**（[trap.rs](../crates/foims-agent-service/src/trap.rs)）：
- 自资源管理模块迁入；UDP 162 接收 Trap/Inform。
- 安全：v1/v2c **community 白名单**、v3 USM 认证校验；**冷却窗口限流**防风暴。
- 处理：记录来源/版本/OID → 查管理员 → 写站内通知（type 区分），记录成功写入数。
- 与 snmp_poll 同属「数据采集」页（前端卡片迁移完成）。

---

## 4. 数据库设计

建表代码：[foims-init/src/schema/tables/agents.rs](../crates/foims-init/src/schema/tables/agents.rs)（校验清单同步于 foims-init check.rs）。

### 4.1 agents（主机快照主表）

| 列 | 类型 | 说明 |
|---|---|---|
| id | UUID PK | uuid_generate_v4 |
| machine_id | TEXT NULL | 机器标识；唯一部分索引 `idx_agents_machine_id … WHERE machine_id IS NOT NULL`（同机重复安装判重） |
| token_hash | TEXT UNIQUE | 上报令牌 SHA-256（明文只在下载包） |
| source | TEXT DEFAULT 'agent' | `agent`（agent 上报）| `snmp`（设备轮询） |
| device_id | UUID FK→devices ON DELETE CASCADE | SNMP 关联设备；唯一部分索引 `idx_agents_device_id`（一设备一记录）；删设备级联清 agents 行 |
| label | TEXT NULL | 下载备注 |
| status | TEXT DEFAULT 'pending' | 状态机见 4.4 |
| hostname/ip/os/kernel/arch/agent_version | TEXT | 上报快照（ip 为对端归一地址） |
| cpu_usage / mem_usage_pct / disk_usage_pct | NUMERIC(5,2) | 快照使用率 |
| max_temp | NUMERIC(5,1) | 最高温度（ingest 温度越界校验即防溢出该列） |
| uptime_secs | BIGINT | |
| raw_metrics | JSONB | agent：全量 AgentReport 覆盖写；snmp：含 `_snmp_state` 差值状态 |
| first_seen / last_seen | TIMESTAMPTZ | 首报/末次上报 |
| created_at | TIMESTAMPTZ NOT NULL DEFAULT NOW() | |

### 4.2 agent_metrics_history（曲线）

- `id BIGSERIAL PK`、`agent_id UUID FK→agents ON DELETE CASCADE`、`collected_at TIMESTAMPTZ`、`metrics JSONB NOT NULL`（精简快照 `{cpu,mem,disk,temp,rx_bps,tx_bps}`）。
- 唯一索引 `(agent_id, collected_at)`：配合入库 `ON CONFLICT DO NOTHING` 幂等去重，兼作曲线范围扫描。
- 清理：每日任务按保留期 DELETE（无分区/无 BRIN，见 §8）。

### 4.3 agent_alert_states（告警状态）

- `agent_id UUID FK CASCADE` + `metric TEXT`（cpu/mem/disk/temp）复合主键；`alerting BOOLEAN DEFAULT FALSE`。
- 语义：状态翻转检测的持久依据（边沿触发、重启不重复告警）。

### 4.4 agents 状态机

```
pending --首报成功--> active --agent_offline 超时--> offline --再上报--> active
   ↑                                    │
   └── 下载建行                    PATCH 手工: active | disabled | revoked
                                       revoked/disabled --上报--> 403 拒绝
```
- pending：下载创建、未首报；offline：失联（可自动恢复）；disabled/revoked：人工封禁（不上报、不可手工设 pending/offline）。
- 注意：冒烟伪造 `status='active'` 会被离线调度器按过期 last_seen 翻回 offline（须同步刷新 last_seen）。

### 4.5 其他关联

- 告警阈值存 `system_configs (config_type='agent', key='alert_thresholds')`（UPSERT）。
- 通知写 `notifications`（type=`agent_alert` / SNMP trap 专用 type）。

---

## 5. API 契约

### 5.1 机器接口（QUIC/H3，无 ApiResponse 信封，错误 `{"error":...}`）

| 端点 | 鉴权 | 请求 | 响应/状态码 |
|---|---|---|---|
| POST /agent/v1/report | mTLS + Bearer token | AgentReport JSON | 200 ReportResponse；400 校验失败；401 令牌无效；403 禁用/吊销；409 machine_id 冲突；413 超限；429 过频（间隔/2 原子闸门）；500 入库失败 |
| POST /agent/v1/renew | 仅 mTLS | CertRenewRequest{client_cert_pem} | 200 CertRenewResponse{cert_pem,key_pem,not_after}；400 未到窗口/解析失败；403 证书不一致/无客户端证书；500 签发失败 |

共享类型：[foims-common/src/report.rs](../crates/foims-common/src/report.rs)——AgentReport{machine_id,hostname,agent_version,collected_at,system{os,kernel,arch,uptime_secs},cpu{usage_pct,cores,load1/5/15},memory{total,used,swap_total,swap_used},disks[{device,mount,total,used,read_iops,write_iops,util_pct}],nets[{iface,rx_bps,tx_bps,errors}],sensors[{label,kind,value}],processes}；ReportResponse{report_interval,collectors,latest_version}。

### 5.2 Web API（/api/agents，JWT + agent_admin_guard(admin/secadmin) 中间件 + 提取器纵深防御；全响应 ApiResponse 信封）

| 方法/路径 | 角色 | 说明 |
|---|---|---|
| GET / | admin/secadmin | 分页列表（status/keyword 过滤） |
| GET /{id} | | 详情（含 raw_metrics） |
| GET /{id}/history?hours= | | 曲线（1-168h，SQL 抽样 ≤200 点） |
| PATCH /{id} | | label ≤100 / status∈{active,disabled,revoked} |
| DELETE /{id} | | 级联删 history/alert_states |
| GET /alert-thresholds | | 阈值配置（AdminOrSecAdminUser 提取器） |
| PUT /alert-thresholds | | 保存并异步全量评估一轮 |
| GET /dist | | 清单摘要（available=false 不报错） |
| GET /download | | 组包下载（组包失败回删 pending；上报地址按公共URL→覆盖→监听推导解析） |

挂载：[src/routes/mod.rs](../src/routes/mod.rs) `nest("/api/agents")` + `route_layer(agent_admin_guard)`。

---

## 6. 配置体系

### 6.1 AgentConfig（[foims-common/src/config.rs](../crates/foims-common/src/config.rs)，config 文件段，经 SharedConfig 热更新）

| 字段 | 默认 | 说明 |
|---|---|---|
| dist_dir | /opt/foims/agents | 预编译产物目录（manifest.json + <target>/foims-agent + install.sh） |
| enabled | false | QUIC 上报监听开关（UDP 不经 nginx，需防火墙放行） |
| bind_addr | [::]:9100 | 监听地址；亦作下载包内 server_addr 推导源 |
| report_interval_secs | 60 | 响应控制面下限（ingest 频控=间隔/2、agent_offline 基准） |
| max_report_bytes | 262144 | 上报/续期请求体上限（413） |
| history_retention_days | 30 | history 保留期 |
| offline_factor | 3 | 离线判定倍数 |
| use_public_url | false | 下载包上报地址取 server.public_url 主机名+监听端口（v0.8.1） |
| report_url_override | "" | 手动上报地址覆盖 host:port，缺端口补监听端口（v0.8.1） |

### 6.2 agent 客户端 agent.toml（下载时服务端生成）

`server_addr`（按 公共URL开关 → 覆盖地址 → 监听地址推导 三级解析）、`token`（64hex 明文）、`report_interval_secs = 60`；未知字段容忍（便于扩展）；证书三件套固定在 cert_dir：client.pem / client.key / ca.pem。

### 6.3 前端

- 主机监控页：[web/static/js/modules/agents.js](../web/static/js/modules/agents.js)——列表（SNMP 徽标 + agent 操作分流：禁用/启用/吊销对 snmp 行隐藏）、详情弹窗（2×2 迷你曲线）、告警阈值弹窗（GET/PUT alert-thresholds）、下载面板（dist 驱动）。
- 系统页 Agent 配置卡片（v0.8.1 重构，「Agent 服务配置」+「Agent 配置」两节）：[web/static/main.html](../web/static/main.html)（服务：启用/监听地址/端口/上报间隔/CA 状态；Agent：公共URL上报开关+覆盖地址/版本/下载面板）+ [web/static/js/modules/systemManager.js](../web/static/js/modules/systemManager.js) 加载保存。

---

## 7. 安全机制汇总

| 层 | 机制 |
|---|---|
| 传输 | QUIC/H3 + TLS1.3 强制 mTLS（站点 CA 唯一锚）；h2c 场景 Host 兜底走 URI authority |
| 上报身份 | Bearer token（64hex）→ SHA-256 库存哈希；machine_id 绑定防令牌复用；频控原子闸门防写库风暴 |
| 续期 | 请求体 PEM ≡ 连接 leaf；<90 天窗口；签发互斥；agent 端坏物料拒绝落盘 |
| Web API | JWT 中间件 + agent_admin_guard + `_user` 提取器三层；keyword `escape_like`；status 白名单；payload 上限 413 |
| 分发 | 二进制 sha256 校验；上报地址三级解析 + 白名单防配置注入；token 明文仅入包；组包失败回滚 pending |
| 证书 | CA 指纹复用校验（轮换自动重签）；私钥 0600 独占创建；client 证书无 SAN（身份由链保证） |
| SNMP | trap community 白名单 / v3 USM / 冷却限流；轮询差值状态持久化 |
| 校验 | collected_at 时钟偏差 ≤300s；数值范围与 NUMERIC 精度对齐（温度 [-100,250] 等） |

---

## 8. 优化建议清单（按优先级）

### P1（性能 / 资源）

| # | 现状 | 风险/成本 | 建议 |
|---|---|---|---|
| 1 | 每条上报触发 `load_thresholds`(system_configs 查询) + `admin_recipient_ids`(users 查询) + `evaluate_agent`(alert_states 查询)，共 ≥3 次额外查询 | agent 数量增长后 DB QPS 线性放大 | 阈值与收件人加进程内缓存（ArcSwap/TTL 30-60s，PUT 时主动失效）；或把评估改为任务消费队列 |
| 2 | `GET /download` 每次全量 sha256 校验二进制 + 内存动态组包 | 高频下载/多目标重复开销 | 按 (target, format, manifest 版本, client 证书指纹) 缓存组包产物（临时文件 + LRU），证书续期/CA 轮换时失效 |
| 3 | agent 每轮全量 scrape 33 采集器（HTTP 模式 671 指标），上报仅用其中 ~20 个族 | 被监控机 CPU 浪费（尤其低配主机、10s 高频场景） | 上报模式使用裁剪采集器集合（复用 `--only` 机制按名白名单构建）；HTTP /metrics 拉模式保持全量 |
| 4 | agent `h3_exchange` 每轮重建 QUIC 连接 + 完整握手 | 每轮 ~1 RTT 握手开销、服务端连接 churn | 连接保活复用 + 失效重连（TLS 物料 mtime 变化或握手失败时重建）；保底保留单次重建路径 |
| 5 | `agent_metrics_history` 无分区，清理靠每日 DELETE | 高频上报下行膨胀、DELETE 空间回收依赖 autovacuum | 评估按月 RANGE 分区（DROP PARTITION 替代 DELETE）；或先补 `collected_at` BRIN 索引 |
| 6 | `alerts::evaluate_all` 全量主机逐台串行 await | 主机数百台时保存阈值后评估拖长 | `futures::stream` 有界并发（如 16）评估 |

### P2（功能补全）

| # | 现状 | 建议 |
|---|---|---|
| 7 | ReportResponse.`latest_version` 下发但 agent 不比较；自更新未实现 | 落地版本推送：agent 比较后经带外 URL 下载新包 → 原子替换二进制 → systemd 自动重启；需防降级与哈希校验 |
| 8 | ReportResponse.`collectors` 恒空 map，采集器开关未实现 | 服务端配置采集器白名单下发生效（与 #3 裁剪机制天然衔接） |
| 9 | 告警恢复（true→false）静默复位 | 增加「已恢复」通知（低级别或聚合），避免管理员只知告警不知恢复 |
| 10 | SNMP 设备指标未接入告警评估（evaluate_all 仅 source='agent'） | 二期：snmp_poll 轮询完成后同样走 evaluate_report，metric 集合按需扩展 |
| 11 | agent 证书过期（停机 >60 天）后 mTLS 握手必败、续期死锁，仅能人工换发 | 可选：服务端 renew 放宽「已过期但链可验」证书的换发（需评估安全面）；或 agent 启动时证书已过期直接显式报错提示人工重装，避免静默重试 |
| 12 | machine_id 409 冲突后 agent 仅无限退避重试 | 上报响应附带结构化错误码（如 conflict），agent 侧停止重试并输出明确人工处置指引 |
| 13 | 断网缓存固定 10 条 | 可配置化 + 本地落盘（重启不丢）；补报间隔 1s 可随缓存长度自适应 |

### P3（工程质量）

| # | 现状 | 建议 |
|---|---|---|
| 14 | ingest 对每条上报全量覆盖写 `agents.raw_metrics`（含 disks/nets/sensors 数组） | 高频小间隔下写放大；可评估 raw_metrics 降采样（如仅存变更字段/压缩）或迁移入 history |
| 15 | trap/agent 通知写入逐管理员串行 create_notification | 批量插入通知表 |
| 16 | report_server 单连接请求串行处理（循环 accept） | 当前 agent 单连接单请求，无影响；若 #4 连接复用落地需同步改并发处理 |
| 17 | `agent.toml` 模板硬编码 `report_interval_secs = 60` | 改为读取 AgentConfig.report_interval_secs，与控制面初始值一致 |
| 18 | agents 快照列 NUMERIC 与 API float8 cast 分散多处 | 可抽共享 SELECT 片段常量，减少口径漂移 |

---

## 9. 附录：关键文件索引

| 文件 | 说明 |
|---|---|
| [crates/foims-agent/src/reporter.rs](../crates/foims-agent/src/reporter.rs) | 上报循环/差分/续期/落盘（1255 行，单测齐全） |
| [crates/foims-agent/src/main.rs](../crates/foims-agent/src/main.rs) | CLI 四模式 |
| [crates/foims-agent/src/collectors/](../crates/foims-agent/src/collectors) | 33 采集器（批次清单见 docs/agent-porting.md） |
| [crates/foims-agent-service/src/report_server.rs](../crates/foims-agent-service/src/report_server.rs) | QUIC/H3 监听与请求分发 |
| [crates/foims-agent-service/src/ingest.rs](../crates/foims-agent-service/src/ingest.rs) | 上报鉴权/校验/频控/事务入库 |
| [crates/foims-agent-service/src/renew.rs](../crates/foims-agent-service/src/renew.rs) | 续期端点 |
| [crates/foims-agent-service/src/cert.rs](../crates/foims-agent-service/src/cert.rs) | 证书签发/复用/续期 |
| [crates/foims-agent-service/src/api.rs](../crates/foims-agent-service/src/api.rs) | 分发下载 + 守卫 |
| [crates/foims-agent-service/src/agents_api.rs](../crates/foims-agent-service/src/agents_api.rs) | 管理 API |
| [crates/foims-agent-service/src/alerts.rs](../crates/foims-agent-service/src/alerts.rs) | 阈值告警 |
| [crates/foims-agent-service/src/tasks.rs](../crates/foims-agent-service/src/tasks.rs) | 三个调度任务 |
| [crates/foims-agent-service/src/snmp_poll.rs](../crates/foims-agent-service/src/snmp_poll.rs) / [snmp_metrics.rs](../crates/foims-agent-service/src/snmp_metrics.rs) / [trap.rs](../crates/foims-agent-service/src/trap.rs) | SNMP 采集与 Trap |
| [crates/foims-common/src/report.rs](../crates/foims-common/src/report.rs) / [config.rs](../crates/foims-common/src/config.rs) | 共享类型与 AgentConfig |
| [crates/foims-init/src/schema/tables/agents.rs](../crates/foims-init/src/schema/tables/agents.rs) | 三表建表与索引 |
| [src/routes/mod.rs](../src/routes/mod.rs) / [src/main.rs](../src/main.rs) | 路由挂载 / 任务注册 / 上报服务启动 |
| [web/static/js/modules/agents.js](../web/static/js/modules/agents.js) | 主机监控前端 |
| docs/agent-design.md | 原始设计文档（§3.3/§5/§6 为本模块依据） |
