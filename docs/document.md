# FOIMS — 组织基础设施管理平台

FOIMS（F-OIMS，Organization Infrastructure Management System）是基于 Rust 与现代 Web 技术构建的组织基础设施管理系统，为网络管理员提供 IP 地址、交换机、物理资产（机房/机柜/工位）、组织人员、可视化与监控的一站式管理界面。

## 系统架构

| 组件 | 说明 |
| --- | --- |
| 后端主程序 | Rust Workspace 多 crate 架构，Axum 0.8，以 h2c 监听 Unix Domain Socket（默认 `/run/foims/api.sock`），经 nginx 反向代理 |
| 前端 | Vue 3 + Vite 工程（MPA 三入口：登录 / 初始化向导 / 主应用），nginx 托管静态资源 |
| FOIMS Agent | 独立工作空间，多平台 musl 静态二进制，HTTP/3 mTLS 主动上报指标（UDP 9100） |
| FOIMS Portal | 官网门户独立二进制，静态页 + Demo 临时账号 API |
| 数据库 | PostgreSQL 16+ + SQLx 异步访问 |
| 例外 UDP 监听 | Agent 指标上报（HTTP/3）、SNMP Trap 接收（默认 UDP 162） |

## 功能模块总览

前端主应用侧边导航共 8 个模块：

1. [仪表盘](#仪表盘)
2. [组织管理](#组织管理)
3. [资源管理](#资源管理)
4. [设备详情（IP）](#设备详情ip)
5. [可视化](#可视化)
6. [主机监控](#主机监控)
7. [日志](#日志)
8. [系统管理](#系统管理)

外围子系统：[初始化向导](#初始化向导)、[认证与登录](#认证与登录)、[FOIMS Agent](#foims-agent-被监控端) 

---

## 仪表盘

- **页头统计卡**（5 张，单端点聚合）：子网总数、网络区域、IP 地址总数、活跃 IP、今日操作
- **榜单卡**（6 张）：
  - 主机负载（同时消费 Agent 快照与 SNMP 实时快照 SSE，CPU/内存/温度/流量四页签 10s 轮播）
  - 最近 IP 分配
  - 最近活动（操作日志）
  - 子网列表
  - 房间列表
  - 机柜列表

## 组织管理

- **组织树**：多级组织架构维护（树形查询、节点增删改查、子节点遍历）
- **模板管理**：组织模板（levels 类型树模板、type_path 校验与解析）
- **节点管理**：组织节点的层级类型与责任分配（责任下沉至工位）
- **人员管理**：员工台账维护

对应后端 crate：`foims-organization`（organization / org_template / employee）。

## 资源管理

页签：网络区域、网段（子网）、房间、机柜、设备、线路。

- **网络区域（Region）**：区域的增删改查，按区域查询关联机柜
- **子网（Network）**：IPv4/IPv6 CIDR 网段管理，归属网络区域
- **房间（Room）**：机房/功能房间管理，房间关联网段、子级单元、网络出口点同步
- **机柜（Cabinet）**：机柜管理与关联网段，U 位（Position）机位同步，配线架（Patch Panel）同步
- **工位（Workstation）**：工位台账管理
- **设备（Device）**：
  - 设备台账 CRUD、设备类型模板、设备网口（Interface）管理
  - SNMP 集成（v1/v2c/v3）：端口、MAC 表、LLDP 邻居、CDP 邻居、VLAN 台账、LAG 聚合、PoE 供电、硬件实体清单与传感器（含 IPMI 传感器）、系统信息、ARP 表采集与同步
  - 主机监控联动（host_monitor）、设备定位
- **线路（Cable Link）**：线缆链路全量纳管（网口—配线架—出口点链路）
- **下拉选项端点**：各资源 id+name 精简选项（仅登录可读）

对应后端 crate：`foims-resource`、`foims-snmp`、`foims-ipmi`、`foims-models`。

## 设备详情（IP）

- **IP 观测数据列表**：10 列（设备/位置/类型/子网/IP/MAC/设备端口/状态/最后在线），位置按「房间（工位）」「房间（机柜-机位）」组合展示
- **自动判活**：状态完全由自动判活与陈旧停用定时任务维护，无人工设置入口
- **MAC 拉取**：从 SNMP 设备批量拉取 MAC 表联动入库
- **自动分配**：子网内可用 IP 查询与自动分配、批量创建
- **筛选与搜索**：表头搜索弹层（服务端过滤、防抖）、分页（10/20/50/100 条）

## 可视化

- **工位可视化**：基于 SVG 的工位布局图，支持拖拽调整与自动绘图，布局保存/删除
- **机柜可视化**：机柜 U 位布局图（房间—机柜—机位联合视图）
- **全局拓扑（设备可视化）**：
  - 拓扑节点管理（保存/删除）、连线管理（创建/删除/详情）
  - 自动发现触发（基于 LLDP/CDP 邻居自动构建拓扑）
  - 前端纯函数几何/视图模型自检（`npm run selfcheck:viz`）

对应后端 crate：`foims-visualization`（layout / topology / http）。

## 主机监控

多采集源统一纳管（source ∈ agent / snmp / ipmi），含多源去重：

- **Agent 通道**：
  - QUIC/HTTP/3 mTLS 指标接收（强制客户端证书），入库 monitored_hosts + host_metrics_history
  - Agent 列表/详情/历史曲线/管理（启用、禁用、删除），30s 自动刷新（窗口失焦暂停）
  - 安装包 Web 下载：CI 直发二进制包（deb/rpm/zip）+ 配置包现场组装（zip/deb/rpm，含 agent.toml + CA/客户端证书物料）
  - 证书签发与续期：站点 CA 签发 Agent 客户端证书，POST /agent/v1/renew mTLS 续期
- **SNMP 轮询采集**：全局周期轮询交换机性能指标并落库（趋势）；**SNMP 实时采集**：秒级定向 GET + 内存快照（不落库，供实时展示与 Top 榜，SSE 推送）
- **IPMI 带外采集**：BMC RMCP+ 会话、SDR/传感器轮询纳入主机监控页
- **告警**：全局阈值配置 + 抖动抑制窗口；告警 episode 状态机（持续超标 → 落库 → 站内通知）
- **SNMP Trap/Inform 接收**：常驻 UDP 监听，v1/v2c community 白名单、v3 USM 用户、冷却窗口防风暴，逐管理员写站内通知
- **SNMP 观测事件**：MAC/ARP 新增、移除、冲突等事件台账与终端定位

对应后端 crate：`foims-agent-service`、`foims-snmp`、`foims-ipmi`。

## 日志

页签：操作日志、登录日志、通知。

- **操作日志**：全链路操作审计，双语输出，分页检索
- **登录日志**：登录成功/失败审计
- **站内通知**：系统通知（告警、Trap、IP/MAC 变更等）列表与处理
- **日志外发（syslog）**：操作/登录日志实时转发外部采集主机（RFC 3164，UDP/TCP 可配，尽力而为旁路）

## 系统管理

页签：系统信息、用户管理、系统配置、通知配置、数据管理、定时任务、安全。

- **系统信息**：版本、数据库状态、运行状态展示
- **用户管理**：用户 CRUD、角色、2FA（TOTP）启用/禁用、密码有效期单用户设置
- **系统配置**：SMTP、通知参数、超时、备份目录等配置卡片（单 form-row 四列流式布局）
- **通知系统**：站内通知 + 邮件通知（SMTP），IP/MAC 变更告警
- **数据管理**：
  - 按模块 CSV 导入/导出（含字段校验与结果回报）
  - 数据库全量备份（pg_dump）与恢复（含 pgpass 口令文件管理）
- **定时任务**：内置 cron 调度（tokio-cron-scheduler），任务增删改查、启停、执行日志。执行器类型：
  - `backup` 数据库备份
  - `token_cleanup` 令牌清理
  - `log_cleanup` 日志清理（天数可配 1–3650）
  - `mac_sync` MAC 表同步
  - `ip_status_sync` IP 状态判活停用（天数可配）
  - `password_expiry_sync` 密码到期禁用
- **证书管理（X.509）**：
  - 站点根 CA 生成/导入，导入 CA 池管理
  - 证书生成（自签名/CA 签发）、导入、清点、下载（CA 公开端点仅 PEM，私钥永不下发）
  - nginx TLS 部署联动
- **服务管理**：网页端管理 foims / nginx 两个 systemd 服务（start/stop/restart/reload/enable/disable），先响应后执行
- **安全**：
  - 多层认证：本地账号 + LDAP + OIDC SSO，均支持 TOTP 2FA
  - 双令牌机制：短效 Access Token + 长效 Refresh Token，前端静默续期
  - 防爆破：应用层 fail2ban（IP/用户/登录/邮箱多维限流封禁）+ OS 层 fail2ban 配置样例
  - 密码策略：复杂度、历史重复校验、有效期到期自动禁用
  - 请求限流：滑动窗口加权计数（IP/用户/邮箱独立桶）
  - AES-GCM 加密存储（SNMP 团体名/密码等敏感凭据）

## 认证与登录

- 登录方式：本地密码、LDAP、OIDC SSO（跳转 + 回调）、邮箱验证码
- 2FA 双因素：TOTP 初始化/启用/禁用、验证码校验
- 找回密码：forgot-password / reset-password（邮件通道）
- 验证码（Captcha）、登录方式探测（/api/auth/methods）、登出、令牌刷新
- 当前用户信息与改密端点（鉴权 + 用户级限流）


## 初始化向导

独立入口页（init_index），首次部署流程：

- PostgreSQL 连接检查（数据库存在性、权限）
- 数据库配置（连接信息填写 + 连接测试）
- 建表初始化（自动创建所有必需数据表）与完备的建表校验（`check.rs` 校验清单）
- 导入已有数据库（升级/迁移路径，配合 `upgrade/` 目录 SQL）
- 初始化状态查询与管理员账号建立


## FOIMS Agent（被监控端）

独立 Cargo 工作空间（`foims-agent/`），与服务端通过网络解耦、独立版本发布：

- **采集器**（foims-agent-core）：CPU/内存/磁盘/网络/温度/负载等指标，模块化 collector 架构
- **上报**：HTTP/3 + mTLS 主动上报（reporter），平台抽象（foims-protocol 协议类型）
- **运行模式**：常驻上报（读 /etc/foims-agent/agent.toml）、`--once` 联调、`--listen` Prometheus /metrics 输出、`--format json` 单次采集
- **平台支持**：Linux（musl 静态二进制 + systemd 单元）、Windows（服务安装/卸载脚本）
- **部署**：deb/rpm/zip 安装包 + Web 端现场组装配置包（证书 + 配置），安装即 enable + restart 闭环
