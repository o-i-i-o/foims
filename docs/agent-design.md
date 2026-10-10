# FOIMS Agent 主机采集方案设计

> 状态：设计定稿，待分阶段实施。本文档为唯一权威方案来源，实施各阶段前先对照本文，
> 阶段完成后更新对应小节的完成状态。

## 1. 背景与目标

FOIMS 目前对主机的感知手段有限（SNMP Trap 仅是被动告警通知）。目标：参考
prometheus/node_exporter（已 clone 到仓库根目录 `node_exporter/`），用 Rust 重写一个
精简版 agent（foims-agent），部署在任意被监控主机上，主动把 CPU、内存、磁盘、温度风扇
等信息上报给 FOIMS，服务端入库、告警并在前端可视化展示。

**核心目标：agent 接近零配置。** 管理员在 FOIMS 网页上选好平台，点击下载即可得到
可直接运行的安装包，无需手工编辑任何配置文件。

### 已确认的决策（2026-10-05 与需求方确认）

| 决策点 | 结论 |
|---|---|
| 通信方式 | FOIMS 监听 UDP 9100，HTTP/3（QUIC），独立证书 |
| 分发打包 | CI 在主程序打包时多平台编译 agent 并集成进安装包；部署后页面按 OS/架构动态打包 zip/deb/rpm 下载；版本门控：agent 版本 ≥ 主程序版本 |
| 前端展示 | 新增「主机监控」页（可视化与日志之间）+ 仪表盘小部件；SNMP trap 保留在日志-通知不动 |
| 下载入口 | 系统设置新增「Agent 采集」页签 |
| 一期采集范围 | 基础指标 + 硬件传感（CPU/内存/负载/磁盘/网络/温度/风扇/系统信息） |

### 否决「编译期嵌入配置」的理由

需要 FOIMS 服务器安装 Rust 交叉编译工具链、每次下载现场编译，慢且占资源；
预编译 + 下载时动态生成配置文件可达到同样的零配置效果，且不引入构建耦合。
多平台预编译产物由 CI 在主程序打包时一次性产出并装入安装包（见 §6.1），
部署后的服务器只做组包分发，不需要任何构建工具链。

## 2. 总体架构

```
被监控主机 × N                        FOIMS 服务器
┌─────────────────────┐              ┌──────────────────────────────┐
│ ┌─────────────────┐ │              │ ┌──────────────────────────┐ │
│ │ 采集器            │ │              │ │ 接收服务                  │ │
│ │ /proc · /sys 只读 │ │              │ │ quinn+h3 · UDP 9100      │ │
│ └────────┬────────┘ │              │ │ 独立证书（站点CA签发）     │ │
│          ▼          │   HTTP/3     │ └────────────┬─────────────┘ │
│ ┌─────────────────┐ │  Bearer 认证 │              ▼               │
│ │ foims-agent      │─┼──────────────┼─▶ ┌──────────────────────┐ │
│ │ Rust musl 静态    │ │  POST report │   │ PostgreSQL            │ │
│ └────────▲────────┘ │              │   │ agents·指标历史·告警   │ │
│          │          │              │   └──────────┬───────────┘ │
└──────────┼──────────┘              │              ▼             │
           │ 下载安装                 │ ┌──────────────────────┐ │
           └─────────────────────────┼─│ 前端：主机监控页        │ │
        系统设置页签生成 ZIP/deb/rpm   │ │ 仪表盘小部件·站内通知   │ │
                                     │ └──────────────────────┘ │
                                     └──────────────────────────┘
```

- Agent 是 HTTP/3 **客户端**，主动上报；FOIMS 进程直接监听 UDP 9100，**不经 nginx**
  （nginx 是 TCP 反代，与 443 的 web HTTP/3 互不影响）。
- 防火墙需放行 UDP 9100（部署文档与 man 必须注明）。

## 3. 通信与安全设计

### 3.1 证书（复用现成 CA 体系）

- 由 `foims-x509-management` 的站点根 CA 签发**专用证书**（独立于 web 的
  default_cert），SAN 写入服务器各网卡 IP（含 IPv6）与主机名，
  存 `/etc/ssl/foims-certs/agent_cert.pem` + `.key`。
- Agent 安装包内置站点 CA 公钥（`/etc/ssl/foims-ca/ca.pem`，公开物料）作信任锚：
  rustls 只信任此 CA 并校验 SAN，天然防中间人。
- 未生成 CA 时拒绝启用 agent 监听（与现有证书生成策略一致）。

### 3.2 认证（每机一 token）

- 下载安装包时服务端创建 `agents` 记录（`status=pending`）+ 随机 32 字节 token，
  数据库只存 SHA-256 哈希，token 明文仅存在于安装包内。
- Agent 每次上报带 `Authorization: Bearer <token>`；首报 machine_id 匹配后激活为
  `active`；列表中可吊销（之后上报返回 403）。
- 限流：payload 上限 256KB；上报间隔下限；agents 表容量保护。

### 3.3 上报协议

上报/响应的请求响应类型定义放 `foims-common`（硬性规则：跨 crate 共享类型不得复制副本）。

```
POST /agent/v1/report        （HTTP/3，Bearer token）
{
  "machine_id": "…", "hostname": "web-01", "agent_version": "0.1.0",
  "collected_at": "2026-10-05T12:00:00Z",
  "system":  { "os": "…", "kernel": "…", "arch": "x86_64", "uptime_secs": 864000 },
  "cpu":     { "usage_pct": 23.5, "cores": 8, "load1": 0.5, "load5": 0.4, "load15": 0.3 },
  "memory":  { "total": 0, "used": 0, "swap_total": 0, "swap_used": 0 },
  "disks":   [ { "device": "sda", "mount": "/", "total": 0, "used": 0,
                 "read_iops": 0, "write_iops": 0, "util_pct": 0 } ],
  "nets":    [ { "iface": "eth0", "rx_bps": 0, "tx_bps": 0, "errors": 0 } ],
  "sensors": [ { "label": "coretemp Package id 0", "kind": "temp", "value": 52.0 },
               { "label": "cpu_fan", "kind": "fan", "value": 1800 } ],
  "processes": 231
}

→ 200 { "report_interval": 60, "collectors": { …开关… }, "latest_version": "0.1.0" }
```

- 响应体是**控制面**：服务端可远程调整上报间隔与采集器开关、通告最新版本，
  agent 下一轮生效（一期可实现 remote interval，升级通告仅展示）。
- `machine_id`：优先 `/etc/machine-id`，回退 `/var/lib/dbus/machine-id`，
  都没有则 agent 首次启动生成本地持久化文件。IP 与主机名都会变，用它做主机去重主键。
- 错误码：401 无效 token / 403 已吊销 / 413 超限 / 429 过频。
- 时间偏差：`collected_at` 偏差过大（如 >5 分钟）拒绝入库；latest 快照一律用服务端时间。
- 防重放一期不做（TLS 1.3 + token 已够），记为后续增强。

## 4. foims-agent（Rust 重写 node_exporter）

- 新 crate `foims-agent`（bin target，加入 workspace 以共享 `foims-common`）。
- 采集器**全部直读 `/proc`、`/sys`**（statfs 用 libc），不引 sysinfo 等重依赖，
  保证二进制小、musl 友好：

| 采集器 | 数据来源 |
|---|---|
| CPU 使用率、负载 | `/proc/stat`、`/proc/loadavg` |
| 内存 + swap | `/proc/meminfo` |
| 磁盘 IO | `/proc/diskstats` |
| 文件系统占用 | `statfs()`（过滤 tmpfs/devtmpfs/overlay，可配） |
| 网卡流量 | `/proc/net/dev` |
| 温度 / 风扇 | `/sys/class/hwmon/`、`/sys/class/thermal/` |
| 系统信息 | `uname()`、`/etc/os-release`、uptime、进程数 |

- **老旧主机兼容**：
  - 目标平台 `x86_64-unknown-linux-musl` + `aarch64-unknown-linux-musl`，
    全静态链接，无 glibc 版本依赖（CentOS 6/7、Debian 8 级别可跑）；
  - rustls/quinn 纯用户态实现，无 OpenSSL 依赖；
  - 建议内核 ≥ 3.10，CentOS 7 上实测为准。
- 运行行为：启动立即上报一次 → 按 interval（默认 60s，±10% 抖动防齐发）→
  失败指数退避重试；断网时本地缓存最近 10 条，恢复后带 collected_at 补报；
  单进程无子进程，目标内存占用 < 10MB；日志走 stderr/journald。
- 配置读取顺序：内置默认值 → `/etc/foims-agent/agent.toml` → CLI 参数覆盖。

### 4.1 demo 阶段决策与现状（2026-10-05）

- **采集范围**：由「基础+硬件传感」改为**一次性完整移植**（对齐原版全部
  Linux 采集器，跨会话长期工程）；demo 已完成批次 1 的 13 个核心采集器，
  进度对照表见 [agent-porting.md](agent-porting.md)。
- **平台矩阵**：全架构对齐 `.promu.yml`。tier1/2 五个 musl 目标
  （x86_64/aarch64/i686/arm/armv7）已全部产出静态二进制并验证；
  tier3（ppc64le/s390x/riscv64/loongarch/mips 系/powerpc）留 `--tier3` 开关，
  实施阶段需 musl 交叉工具链解决 crt 启动对象；OpenBSD 暂缓。
  构建入口：`scripts/build-agent.sh`（镜像缺 std 组件时自动官方源离线兜底）。
- **demo 形态**：`foims-agent` CLI 已可用——`--list` / `--only` /
  `--format prometheus|json` / `--listen ADDR:PORT`（HTTP `/metrics`），
  输出与 node_exporter 文本格式逐项对照。
- **statfs/uname 实现**：经 rustix 安全封装（`unsafe_code=forbid` 全局约束），
  非设计初稿的直调 libc。
- **HTTP/3 mTLS 可行性 demo（2026-10-05 验证通过）**：
  `crates/foims-agent/examples/h3_demo.rs`（server/client 双子命令）+
  `scripts/gen-h3-demo-certs.sh`（自签 CA + serverAuth 服务端证书
  + clientAuth 客户端证书）。技术栈 quinn 0.11.12（rustls-ring）+
  h3 0.0.8 + h3-quinn 0.0.10（按 §7 风险对策锁定小版本）。验证结论：
  1) mTLS——`WebPkiClientVerifier` 强制客户端证书，携带证书连接成功
     （`certs=1`），无证书连接握手期被拒（TLS alert 116
     `peer sent no certificates`）；2) 加密——tcpdump 抓 UDP 9100 共 25 包、
     指标明文标记 0 命中（TLS 1.3 全程加密）；3) 上报——真实采集器 174 个
     指标族 60KB 经 POST /api/v1/agent/report 上报成功并回执 200。
  demo 依赖仅入 `[dev-dependencies]`，不进入 agent 发布二进制。

## 5. 服务端设计

### 5.1 新 crate `foims-agent-service`

| 模块 | 职责 |
|---|---|
| `listener.rs` | quinn + h3 监听 UDP 9100；任务模型照抄 `foims-resource/src/trap.rs`：enabled 开关、bind 失败仅记日志不终止进程、订阅 shutdown receiver |
| `auth.rs` | token 哈希校验、pending 首报激活、吊销判断 |
| `ingest.rs` | 解析上报 → upsert agents 热列 → 追加 history → 阈值评估告警 |
| `cert.rs` | 调 foims-x509-management 签发/更换 agent 专用证书，SAN 收集本机地址（含 IPv6） |
| `packaging.rs` | 安装包组包（见 §6），运行时从 `/opt/foims/agents/` 读预编译二进制（不用 include_bytes!，避免构建耦合与主程序膨胀） |

### 5.2 数据库（手动 SQL + 同步 foims-init 建表代码与 check.rs）

```sql
CREATE TABLE agents (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  -- machine_id 下载时未知（agent 所在主机生成），首报激活时回填；
  -- 唯一性用 partial unique index（仅对已回填行生效）
  machine_id TEXT,
  token_hash TEXT NOT NULL UNIQUE,
  label TEXT,                                          -- 下载时的备注
  status TEXT NOT NULL DEFAULT 'pending',              -- pending|active|offline|disabled|revoked
  hostname TEXT, ip TEXT, os TEXT, kernel TEXT, arch TEXT, agent_version TEXT,
  cpu_usage NUMERIC(5,2), mem_usage_pct NUMERIC(5,2), disk_usage_pct NUMERIC(5,2),
  max_temp NUMERIC(5,1), uptime_secs BIGINT,
  raw_metrics JSONB,                                   -- 详情页全量渲染
  first_seen TIMESTAMPTZ, last_seen TIMESTAMPTZ,
  created_at TIMESTAMPTZ DEFAULT now()
);

CREATE UNIQUE INDEX idx_agents_machine_id
  ON agents (machine_id) WHERE machine_id IS NOT NULL;

CREATE TABLE agent_metrics_history (
  id BIGSERIAL PRIMARY KEY,
  agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  collected_at TIMESTAMPTZ NOT NULL,
  metrics JSONB NOT NULL
);
CREATE INDEX idx_agent_metrics_history_agent_time
  ON agent_metrics_history (agent_id, collected_at);
```

热列（cpu_usage 等）用于列表排序与阈值 SQL；raw JSONB 存全量供详情页。
首报激活：`UPDATE agents SET machine_id=…, status='active', first_seen=…, last_seen=…`
`WHERE token_hash=$1 AND status='pending'`（token 唯一，凭 token 定位记录；
machine_id 冲突时视为同机重复安装，返回 409 由运营处理）。

### 5.3 告警与调度

- **阈值告警**：ingest 时同步评估（不做全表扫描）；连续 N 次超限才告警（防抖）+
  冷却窗口（借鉴 trap.rs 的 DashMap 冷却）；复用 `create_notification`
  写站内通知（`notification_type = "agent_alert"`，收件人 admin/secadmin，与 trap 一致）。
- **离线判定**：调度任务扫描 `last_seen > 3 × interval` → status=offline，
  离线/恢复均发通知。
- **history 清理**：调度任务每日删除超过保留期（默认 30 天，可配）的行。
- 阈值参数存数据库单行配置表（网页可改，不需重启）：CPU/内存/磁盘 %、温度 ℃、
  连续次数、冷却秒数、离线倍数、history 保留天数。

### 5.4 Web API（挂 protected_api_routes，显式鉴权提取器纵深防御）

| 方法与路径 | 说明 | 权限 |
|---|---|---|
| GET /api/agents | 分页列表（paged_response） | 管理角色读，user 可配 |
| GET /api/agents/:id | 详情（含 raw_metrics） | 同上 |
| GET /api/agents/:id/history | 曲线数据（?hours=24） | 同上 |
| PATCH /api/agents/:id | label / enabled | admin/secadmin |
| DELETE /api/agents/:id | 删除记录 | admin/secadmin |
| GET/PUT /api/agents/settings | 阈值等配置 | 读管理角色 / 写 admin+secadmin |
| GET /api/agents/cert/status · POST …/cert/issue | agent 证书状态与签发 | admin/secadmin |
| GET /api/agents/dist | 产物清单摘要（版本/门控状态/平台列表，驱动下载面板） | admin/secadmin |
| GET /api/agents/download | 生成安装包（创建 pending agent）并流式返回（§6.3） | admin/secadmin |

## 6. 分发打包

> 优化方案（2026-10-05 定稿）：agent 由 CI 在主程序打包时多平台编译并集成进
> 安装包；部署后页面按操作系统/架构动态组包下载；版本门控确保 agent 版本
> 不低于 FOIMS 主程序版本。

### 6.1 构建期（CI 集成进主程序安装包）

- **版本注入**：`scripts/build-agent.sh` 从根 `Cargo.toml` 读取主程序版本，
  以 `FOIMS_AGENT_VERSION` 环境变量注入编译（agent 内 `option_env!` 读取，
  缺省回退 `CARGO_PKG_VERSION`）。**agent 版本 = 打包时的主程序版本**，
  天然满足「agent 版本不低于 foims」；`foims-agent --version` 输出该版本。
- **产物清单**：`build-agent.sh` 归集产物时生成 `dist/agents/manifest.json`
  （`version` + 各 `target` 的 `sha256`/`size`）与 `SHA256SUMS`，作为运行期
  分发的唯一事实来源。
- **集成进安装包**：`scripts/build-deb.sh` 调用 `build-agent.sh`（默认
  `--only x86_64-unknown-linux-musl,aarch64-unknown-linux-musl`，控制包体）
  后按**显式清单**（硬性规则，禁通配符）复制进包：
  - `dist/agents/<target>/foims-agent` → `/opt/foims/agents/<target>/foims-agent`（0755）
  - `dist/agents/manifest.json` → `/opt/foims/agents/manifest.json`（0644）
  - `deploy/agent/install.sh` → `/opt/foims/agents/install.sh`（0755，ZIP 下载时组包用）
  - 任一文件缺失即失败退出（与 init-pgsql.sh 同策略），不带缺口发版；
    并校验 manifest 内 version 与主程序版本一致（防产物错配）。
- 源码部署（非 deb）场景：手工执行 `build-agent.sh` 后把三项目录内容同步到
  `[agent].dist_dir`（默认 `/opt/foims/agents`），下载 API 才可用。

### 6.2 版本门控（下载时）

- 下载 API 读取 `manifest.json` 的 `version`，与**运行中主程序版本**
  （`env!("CARGO_PKG_VERSION")`）按 semver 比较：
  - `manifest.version < 主程序版本` → 409 拒绝下载（提示重装/检查 dist_dir，
    典型场景：升级后产物目录残留旧版本）；
  - `>=` 放行（CI 打包产物理应相等，`>` 允许现场先行更新产物的运维操作）。
- Agent 上报协议的 `latest_version` 响应字段（§3.3）沿用 manifest 版本，
  供旧版 agent 展示升级通告（阶段 2 实现）。

### 6.3 下载时（packaging.rs，按 OS/架构动态组包）

1. **查询**：`GET /api/agents/dist` 返回 manifest 摘要（版本、门控状态、
   可用平台列表），前端据此渲染平台/格式选项，产物缺失时置灰并提示；
2. **下载**：`GET /api/agents/download?target=…&format=zip|deb|rpm&label=…&server_addr=…`：
   - 创建 pending agent 记录（label = 页面备注）+ 随机 32 字节 token
     （库内存 SHA-256 哈希，明文仅写入包内配置）；
   - 生成 `agent.toml`：`server_addr`（默认取请求 Host 的主机名/地址
     + 9100 端口，可在下载面板覆盖为指定 IP）、`token`、上报间隔；
   - 读取站点 CA 公钥 `/etc/ssl/foims-ca/ca.pem` 作信任锚（未生成 CA 时
     409 拒绝下载，提示先在证书管理页生成站点 CA）；
   - 组包（全部内存中完成，不落临时文件）：
     - **zip**：平台二进制（0755）+ `agent.toml`（0600）+ `ca.pem` + `install.sh` + README；
     - **deb**：纯 Rust 生成（tar + flate2 + 手写 ar 归档：debian-binary +
       control.tar.gz + data.tar），内含二进制、`/etc/foims-agent/` 配置、
       systemd unit 与 postinst 自动启用；
     - **rpm**：`rpm` crate（纯 Rust，无需 rpmbuild），等价内容 + %config 标记；
   - 流式返回（Content-Disposition 文件名
     `foims-agent-<版本>-<架构>.<格式>`）；pending 记录留在 agents 列表
     （阶段 2 提供列表/吊销 API）。

### 6.4 install.sh 行为（zip 场景）

检测 arch 选二进制 → 装到 `/usr/local/bin/foims-agent` →
写 `/etc/foims-agent/{agent.toml, ca.pem}`（0600/0644）→
有 systemd 装 unit 并 `systemctl enable --now`；
无 systemd（sysvinit）装 init.d 脚本 + `chkconfig`/`update-rc.d` 兜底；
非 root 执行时提示 sudo。

### 6.5 包内含 token 属敏感物料

下载接口限 admin/secadmin（Router::nest + route_layer 角色守卫）；
泄露可在列表吊销该 token。

## 7. 前端设计

### 7.1 新页面「主机监控」（main.html 新 section，nav 位于可视化与日志之间）

- **页签 1 主机列表**：状态点（在线/离线/告警/停用）、主机名+label、IP、
  CPU/内存/磁盘使用率条、最高温度、agent 版本、最近上报、
  操作（详情/禁用/吊销/删除）；30s 自动刷新。
- **页签 2 故障设备**：超阈值指标 + 离线主机列表，含触发时间、当前值/阈值。
- **详情 modal**：分组卡片（CPU / 内存 / 磁盘 / 网络 / 温度风扇 / 系统信息）+
  关键指标 24h 曲线（自绘 SVG，参考 `web/static/js/modules/visualization/svgCanvasBase.js`
  模式；CSP `script-src 'self'`，禁外部图表库）。

### 7.2 仪表盘小部件

在线主机 x/y + 活跃告警数，点击跳转主机监控页。

### 7.3 系统设置新页签「Agent 采集」

- 监听状态卡：enabled/端口/监听地址（config.toml 修改后需重启生效，与 snmp.trap 同风格，页面提示）；
- 证书卡：状态 / 一键签发 / 有效期 / SAN 说明；
- 下载卡：数据源为 `/api/agents/dist`——展示 agent 版本与门控状态（低于主程序版本时
  红条提示并禁用下载）、平台（manifest 实际产出的 target 列表）+ 格式（zip / deb / rpm）
  + 可选上报地址覆盖 + 备注 → 下载按钮；
- 阈值配置：各指标阈值、连续次数、冷却、离线倍数、history 保留天数。

### 7.4 其他

- SNMP trap 保留在日志-通知页不动；agent 告警走站内通知（铃铛天然生效）。
- i18n：`zh.json` / `en.json` 同步补全。
- 权限：读=管理角色（user 是否可见遵循现有角色矩阵可配），写=admin/secadmin。

## 8. 实施计划（分阶段，每阶段可独立验证）

- [x] **阶段 1 协议与存储**（2026-10-10）
  foims-common 上报/响应类型；config.toml `[agent]` 段
  （enabled / bind_addr / cert / key / max_report_bytes / 保留天数）；
  手动执行建表 SQL；foims-init 建表代码与 check.rs 校验清单同步。
  验证：`cargo test -p foims-init` 通过，check.rs 校验全绿。
- [x] **阶段 2 服务端接收**（2026-10-10；阈值告警一期未做，离线/清理调度已实现）
  foims-agent-service crate：quinn/h3 监听、认证、入库、阈值告警、离线/清理调度；
  agent 证书签发入口（cert.rs）；Web API 与权限。
  验证：curl/测试客户端模拟上报入库；`cargo clippy --release -- -D warnings`。
- [x] **阶段 3 Agent**（2026-10-10；默认运行模式为循环上报，`--once` 供联调）
  foims-agent crate：各采集器（/proc 文本 fixture 单测）+ h3 客户端上报；
  `scripts/build-agent.sh` musl 构建；本地回环联调（agent → FOIMS 全链路）。
  验证：本机运行 agent，网页能看到本机指标。
- [x] **阶段 4 分发**（CI 集成已随 §6 优化先行落地，2026-10-05）
  build-agent.sh 版本注入 + manifest.json；build-deb.sh 集成多平台产物 +
  install.sh 装包（fail-fast）；packaging.rs（zip/deb/rpm 动态组包）+
  版本门控下载 API（/api/agents/dist、/api/agents/download，pending 记录 +
  token）+ 系统设置「Agent 采集」页签下载卡。
  验证：网页下载 zip → 另一台机器/容器安装 → 上报成功（上报依赖阶段 2/3）。
- [x] **阶段 5 前端**（2026-10-10；列表 + 详情曲线已做，故障设备页签与仪表盘小部件待后续）
  主机监控页（列表/故障/详情）+ nav + i18n + 仪表盘小部件。
  验证：浏览器冒烟（列表刷新、详情曲线、告警通知出现）。
- [x] **阶段 6 收尾**（2026-10-10）
  版本 bump（Cargo.toml `0.21.37` + 三入口 `?v=` 时间戳 + resourceLoader.js
  MODULE_VERSION）；`cargo fmt && cargo clippy --release -- -D warnings`；
  `./test/pak.sh` 冒烟（admin/admin123）；README / README_EN 架构段注明
  UDP 9100/162 直监听、man NETWORK 段注明防火墙放行、config.toml.example
  `[agent]` 段全量注释；build-agent.sh 注入交叉 C 编译器（ring 交叉编译需要，
  cc-rs 认小写 `CC_<target>` 环境变量）。

## 9. 风险与备注

- UDP 9100 防火墙放行（nginx 不管 UDP，部署文档必须写明）。
- `h3` crate 尚为 0.0.x：锁定版本，上报协议处理封一层薄 trait 隔离 API 变动。
- history 量级：100 台 × 60s ≈ 14 万行/天，30 天 ≈ 430 万行 JSONB——可接受，
  靠清理任务兜底；量大后再做降采样聚合。
- agent 用 IP 直连时证书 SAN 必须含该 IP（签发时收集所有网卡地址，含 IPv6）。
- 包体：两平台静态二进制约使 deb 增大 ~10MB。
- musl 静态二进制在极老内核上的 quinn 行为需在 CentOS 7 实测（GSO/ECN 自动降级）。
