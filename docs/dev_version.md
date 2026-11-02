# 本文件为开发用版本号控制规则

# 开发版本号格式为vxx.yy.zz
# z 每次修改更新无条件z+1，z>99则y+1，z归零
# y 每次为模块增加小功能、功能优化 则y+1，z归零
# x 每次增加模块（级功能）x+1，y归零，z归零
# 普通小bug修复仅执行z规则，功能bug更新执行y规则，模块级功能更新执行x规则

# 主 Cargo.toml（根 crate foims 即 lib）version = "aa.bb.cc" 关联本文件 vxx.yy.zz
# version版本号改为 "aa.bb.cc"的形式 ，aa bb cc的首个0省略 06.21.10 > 6.21.10
# 映射关系：本文件 x→Cargo.toml 的 b，本文件 y→Cargo.toml 的 c
# 联动仅以下两种触发，本文件 z 段变化（普通修改）一律不触碰 Cargo.toml：
# 1. 本文件 y 段变化（小功能/功能优化，含 z>99 进位）→ Cargo.toml 仅 c+1
# 2. 本文件 x 段变化（模块级功能）→ Cargo.toml b+1，c 归零
# a由开发者根据实际情况手动变更
# 示例：本文件 v0.0.13→v0.0.14（仅 z+1）时 Cargo.toml 保持 0.21.36 不动；本文件 v0.1.0（y+1）时 Cargo.toml 才 0.21.36→0.21.37
# 目的：z 段作为高频修改审计计数，避免智能体高频次修改推高 Cargo.toml 版本号
# crate版本各自递增，某crate版本变更主Cargo.toml的版本都要关联变更（本次修改只涉及crate a则修改crate a 和lib的版本号，crate b版本号不变）
# 前端自切换 Vue 3 + Vite 工程起由 frontend/package.json 的 version 字段承载版本号，复用本文件同一套 x/y/z 判级与联动机制、与 Cargo.toml 相互独立：仅前端修改联动 package.json（z 段不动 / y 段 c+1 / x 段 b+1 且 c 归零，a 段手动），不再 bump 根 Cargo.toml；仅后端修改照旧只联动 Cargo.toml；前后端混合修改按各自维度分别联动。版本计数自 0.1.0（Vue 3 工程确立）起独立递增；旧 bump_static_version.sh/?v=/MODULE_VERSION 机制已废弃，Vite 内容指纹（assets/ 哈希文件名）仅承担构建产物缓存命名


# v0.0.1

# 2026100712171501
# 2026年10月7日12点17分15秒 第1次修改 （01用来规避极端情况，通常不可能1秒修改几次）
# 版本采用文本累加机制，每次版本更新新增此版的版本号 时间序号 更新内容。
# 此版本机制自2026年10月7日实行

### 重要！本文件只作为版本记录，智能体阅读到此处请直接跳到页尾，禁止智能体全量引用本文档造成上下文超限 

v0.43.0
2026103015120501
将开发版本记录文档归档到docs/dev_version_achived.md，智能体请勿检索此文件

v0.43.1
2026103015361201
注释审计阶段五（auth/init/data-management/x509/scheduler/visualization/services）：auth 35 个、init 11 个、visualization 13 个、data-management 6 个及主 crate 证书/服务 11 个 handler 统一为路径优先文档；删除复述式注释，保留 why/安全/并发说明；修正 scheduler、topology、test-connection 等不准确注释。仅注释改动，未改代码/错误码/日志；cargo fmt、workspace clippy -D warnings、受影响 7 crate 测试（279 项）全部通过；仅 dev_version z+1，Cargo.toml/package.json 不联动

v0.43.2
2026103016273701
注释审计阶段六（前端纯 TS，41 文件，排除 ts-rs 生成物）：删除 6 文件 10 条名称直译注释（useToast/usePagedList/useSubTabs/pagedFetch/types.visualization/viewModel），其余注释为契约/单位/浏览器坑等 why 说明，保留；未动并行在途的 3 个 SFC 与 i18n JSON。验证：仅注释守卫为空、npm run typecheck、npm run build、biome check、oxlint 均通过；仅 dev_version z+1，frontend/package.json 与 Cargo.toml 不联动

v0.43.3
2026103019014401
注释审计阶段七（前端 SFC，92 个 .vue）：30 文件删除 98 条名称直译/步骤复述注释并清理删除注释后产生的连续空行；12 处 ==== 装饰横幅降级为极简分区标签（ResourcesPage 7、LoginPage 2、SubnetUsageModal 2、DashboardPage 1），全仓 SFC 装饰横幅清零；修正 VisualizationPage 机柜注释错字（柜体会员自动跟随→拖动柜体时机位自动跟随）。保留组件头职责/设计来源、Esc 栈/Teleport/焦点、i18n 动态键、SNMP 协议与脱敏、坐标/单位等 why 注释；未动 i18n JSON 与 ts-rs 生成物。验证：仅注释守卫为空、git diff --check、npm run typecheck、npm run build、biome check、oxlint（318 文件 0 警告 0 错误）均通过；仅 dev_version z+1，frontend/package.json 与 Cargo.toml 不联动

v0.44.0
2026103103594401
SNMP 实时采集改为「每设备单端口」口径：device_interfaces 新增 monitored 列与部分唯一索引（device_id WHERE monitored），升级脚本见 upgrade/0.32/upgrade_0.32.0.sql 第 8 节；LiveCollector 新增采集端口（精确名/忽略大小写/去分隔符三级匹配，未命中或未配置回落探测首端口），仅对目标端口下发计数器 OID 并计算速率，快照 rx_bps/tx_bps 由全端口求和收敛为该端口；worker 比对 monitor_if 变化后重建；新增 GET/PUT /api/agents/{id}/monitor-port 与主机详情网络板块的采集端口下拉（改动后 ≤60s 生效）；详情弹窗 CPU/内存/温度/流量优先展示 30s 内实时快照，温度补进温度/风扇卡片，不落指标库。验证：cargo fmt、workspace clippy -D warnings、foims-snmp/foims-agent-service 单测、gen-ts-types、npm run typecheck 均通过；y 段 +1，Cargo.toml 0.32.10、frontend/package.json 0.1.34 联动

v0.45.0
2026103112243601
修复「前端 CPU/内存/温度非秒级」：根因在采集节奏而非展示层——resource_interval_secs=15 使资源组 15s 才更新、interval_secs 默认 5、SSE 节拍 2s、分块 GET 串行叠加 RTT。删除双节奏设计（config.rs 移除 resource_interval_secs 及其默认函数；collector.rs 删 ResourceSnapshot/resources 缓存与 include_resources 形参，资源组无条件并入每轮 build_items；live_poll.rs 删 WorkerState.last_resources/resource_interval），interval_secs 默认 5→1（`.max(1)` 兜底不变）；collector.rs fetch 改 join_all 分块并发，单轮耗时取最慢分块（foims-snmp 新增 futures-util）；live_api.rs SSE_TICK_SECS 2→1；config.toml.example 同步。次生风险处置：1s 采样把静默窗口内 refresh_notifications 的 UPDATE 频率放大一个量级，alert_episode.rs 新增 NOTIFY_REFRESH_SECS=60 节流与 Episode.last_refresh_at（Resolved→Pending 清零）并补单测。前端新鲜度判定不再依赖两端时钟同步：AgentDetailModal.vue 以本端观察到 collected_at 推进的时刻（liveSeenAt）+ 每秒 nowTick 计算 10s 窗口，新增「采集时间」行与断连提示，CPU/内存/网络三卡标题挂 agent-live-badge（温度卡保留行内实时标记避免重复）；i18n 新增 agents.live_updated_at/live_stream_lost。审计修正 live 模块注释与实际请求口径不一致（mod.rs 单轮 OID 量、collector.rs 分块并发）；useLiveAgents 全量摘要流改为页面级共享（引用计数，归零才 close），看板/主机列表/详情弹窗复用一条 EventSource，避免服务端逐连接重复构建并序列化全量快照（SSE 节拍提至 1s 后该开销同步放大）。验证：cargo fmt、workspace clippy -D warnings、foims-snmp 74 + foims-agent-service 79 单测、npm run typecheck/build、biome、oxlint（322 文件）均通过；pak.sh 部署后 /api/agents/live 采样 collected_at 逐秒推进、cpu/temp 秒级变化，SSE 8s 内 8 推，浏览器硬刷新复验三卡徽标与采集时间逐秒递增。y 段 +1，Cargo.toml 0.32.11、frontend/package.json 0.1.35 联动；foims-common 0.3.17、foims-snmp 0.1.6、foims-agent-service 0.1.24

v0.46.0
2026103114034501
新增 IPMI 带外主机监控集成（模块级）：新建纯 Rust 采集 crate foims-ipmi 0.1.0（ruipmi 0.6.2 RMCP+ 异步会话、SDR 枚举→温度/风扇/电压传感器与 Get Chassis Status 电源状态、test_ipmi 建会话探活）；devices 表新增 ipmi_username/ipmi_password(AES-GCM 密文)/ipmi_port(623)/ipmi_auth_algo(sha1|sha256) 列，monitored_hosts source CHECK 扩为 agent|snmp|ipmi、唯一索引改 (device_id, source)，升级脚本 upgrade/0.33/upgrade_0.33.0.sql；foims-resource 设备 CRUD 加解密/脱敏 + ipmi_configured 计算标记，新增 POST /api/resources/devices/test-ipmi 与 /{id}/test-ipmi（复用 SNMP 同款 SSRF IP 校验）；foims-common 增 [ipmi] 配置段（enabled/interval_secs/timeout_secs/max_concurrency）；agent-service 新增 ipmi_poll（5 分钟轮询落库 source='ipmi'、machine_id='ipmi:{device_id}'、仅 temp 进历史）与 system_agent_ipmi_poll 调度、秒级实时 IpmiLiveRegistry worker（live_poll.rs，采样即建会话+外层 timeout 封顶+退避，仅温度进告警评估），live_api.rs SSE/列表并入 ipmi 快照（LiveSummaryRow 加 source/power_on/fans/volts + LiveSensorRow），alert_episode 兜底扫描按 live_covers_ipmi 动态纳入白名单；agents_api 列表/Top 按 IP 去重 CTE（ROW_NUMBER，优先级 ipmi>agent>snmp，NULL IP 各自成组恒保留）。后端 i18n zh/en 补 log.device.ipmi.*、log.task.*、system.ipmi_live_*；前端 DeviceModal SNMP/IPMI 改左右两栏 + IPMI 连接测试按钮、AgentsPage/AgentDetailModal/DashboardPage 适配 source='ipmi'（IPMI 展温度/风扇/电压/电源、隐藏网络/端口/CPU/内存/磁盘板块与非温度曲线）、useLiveAgents 按 source|device_id 匹配、i18n device.ipmi.*/agents.* 新键。验证：cargo fmt、workspace clippy --all-targets -D warnings、gen-ts-types、IPMI 纯函数单测（machine_id/传感器聚合/去重 rank/payload 归一/test-ipmi SSRF 拒绝）、npm run typecheck/build、biome、oxlint 均通过。x 段 +1，Cargo.toml 0.32.11→0.33.0、frontend/package.json 0.1.35→0.1.36 联动；foims-common 0.3.18、foims-models 0.2.20、foims-resource 0.1.20、foims-agent-service 0.1.25、foims-ipmi 0.1.0

v0.47.0
2026103119214901
主机监控页新增管理员网络调试工具集（本页子功能，y 段 +1）：顶栏"调试"按钮打开多标签弹窗 DebugModal.vue（Ping/SNMP/MIB/端口·DNS 四标签），五端点挂 /api/agents/debug/*（ping/dns/port 主 crate src/debug.rs、snmp/mib foims-snmp::debug，均手动输入目标、角色由 nest 的 agent_admin_guard 承接）；SSRF 口径收敛为 foims-common::net 的 resolve_probe_ip/guard_probe_ip（test-snmp 内联校验改为复用同函数），ping 限 1-10 包/-W 8s、GET 上限 60 OID、walk 有界 500 行、固定 5s 短超时；MIB 对照为静态精选表（MIB-II system 组/ifTable·ifXTable 常用列/hrStorage/cisco·huawei·h3c enterprise 前缀，OID↔名称双向查，mib 双向查/name_of 4 单测）；服务端 i18n zh/en 补 server.debug.* 与 device.snmp.multicast_forbidden/link_local_forbidden（后者为 net.rs 既有引用的历史缺口一并补齐），前端 i18n 补顶级 debug.* UI 命名空间（server.debug.* 翻译已随上次提交入库），gen-ts-types 再生成 Ping/PingResult/Dns/PortResult/SnmpDebug/ MibEntry 等 8 个载荷类型。验证：cargo fmt、workspace clippy -D warnings、前端 typecheck/build、biome、oxlint 均通过。y 段 +1，Cargo.toml 0.33.0→0.33.1、frontend/package.json 0.1.36→0.1.37 联动；foims-common 0.3.19、foims-models 0.2.21、foims-snmp 0.1.7

v0.48.0
2026103120000001
通知模块新增 Webhook 推送渠道（通用/钉钉/企业微信，参考 uptime-kuma 群机器人实现，y 段 +1）：foims-auth 新建 webhook.rs（三渠道配置存取 system_configs config_type='webhook' key-value、钉钉 HMAC-SHA256 加签 timestamp+sign 与 Base64 URL 编码、企微固定端点 text 消息、通用 POST JSON title/message；统一入口 send_notification_webhook 按 WebhookNotifyKind 读矩阵开关门控、逐渠道尽力发送失败仅留痕，业务封装 send_ip_change/device_swap/host_alert/unknown_device_webhook 与 smtp 侧发送函数一一对应）；smtp.rs 通知文案提取为 *_message 共用函数（ip_change/device_swap/host_alert/unknown_device + host_alert_metric_label），邮件与 Webhook 渠道复用同一文案；NotificationMethods 扩展 webhook_ip_change/webhook_agent_alert/webhook_snmp_event（serde default_true 兼容存量 JSON，无 schema 变更）；通知触发点接入三处（snmp/events/notify.rs 两类 IP 异动、snmp/events/mod.rs 未知设备、agent-service/alert_episode.rs 主机告警，email Option 改引用模式避免 move）；主 crate 新增 GET/PUT /api/system/webhook/config（secret 掩码 has_secret 口径同 SMTP has_password、留空保留、http(s) scheme 校验）与 POST /api/system/webhook/test（分渠道测试）；workspace 增 reqwest 0.12（default-features=false, json+rustls-tls）与 hmac 0.13（foims-auth 引入）；服务端 i18n zh/en 补 log.webhook.*，前端 i18n 补顶级 webhook.* 卡片文案与 server.webhook.* 消息段、删除 notification.pending 占位；系统-通知配置页 SMTP 卡片后新增 Webhook 卡片（三渠道表单+分渠道测试按钮，system.css 补 form-hint/webhook-test-actions），通知矩阵 Webhook 列由"待实现"占位改为 checkbox；gen-ts-types 再生成 WebhookChannel/WebhookConfigResponse/UpdateWebhookConfigRequest/TestWebhookRequest。验证：cargo fmt、workspace clippy --all-targets、foims-auth/foims-snmp/foims-agent-service 单测（82+87+78，含钉钉加签标准向量 Python hmac 对照）、npm run typecheck/build、biome、oxlint 均通过。y 段 +1，Cargo.toml 0.33.1→0.33.2、frontend/package.json 0.1.37→0.1.38 联动

v0.49.0
2026103120241101
用户管理支持手机号码（小功能，y 段 +1）：users 表新增可空列 phone VARCHAR(20)（口径同 employees.phone：数字与 + - 空格、≤20 字符、空串规范化 NULL），升级脚本见 upgrade/0.33/upgrade_0.33.0.sql 第 4 节，foims-init 建表代码与 check.rs 列清单同步；foims-models User/UserCreate/UserUpdate 增加 phone（更新侧沿用 EmployeeUpdate 同款双层 Option 三态：缺省不修改/null 清空/有值更新），foims-auth 用户 CRUD（列表/详情/新增/编辑的 SELECT/INSERT/UPDATE）与登录链路（本地/两步/邮箱验证码/LDAP/SSO 的行查询与 User 构造）全部带出 phone；UserModal 第一步布局调整——状态下拉移到用户名后（同排），新增手机号码输入框（type=tel，与邮箱同排），载荷随新增/编辑提交、空串表示清空；前端 i18n 新增 user.phone/phone_placeholder 与 server.user.validation.phone_length/phone_invalid（zh/en），gen-ts-types 再生成 User/UserCreate/UserUpdate。验证：cargo fmt、workspace clippy -D warnings、foims-models/foims-auth/foims-init 单测（87+57+321，含 phone 三态与超长拒绝新用例）、npm run typecheck/build、biome、oxlint 均通过。y 段 +1，Cargo.toml 0.33.2→0.33.3、frontend/package.json 0.1.38→0.1.39 联动；foims-models 0.2.22、foims-auth 0.1.10、foims-init 0.1.19

v0.50.0
2026103122135801
邮件通知与 Webhook 通知解耦重设计（收件人与目标均按通知类型独立订阅，y 段 +1）：废弃全局通知方式矩阵（NotificationMethods 与 notification/methods 键删除，snmp/notify.rs、snmp/events/mod.rs、agent-service/alert_episode.rs 四处矩阵门控移除）；新增 NotifyScope（ip_change/agent_alert/snmp_event，serde 缺省按启用）与 NotifyKind 贯穿两渠道；邮件收件人 notification.email_recipients 由 UUID 字符串数组改对象数组 [{user_id+scope 平铺}]，send_notification_email 按收件人订阅过滤 BCC（send_*_email 公共签名不变，触发点零改动）；Webhook 改多目标单行存储 webhook.targets=[{id/name/channel/url/secret(AES-GCM 密文)/scope 平铺}]（≤20 条），WebhookChannel 补 label，企业微信改存完整群机器人 URL（废弃 wecom_key），webhook.rs 分层为 load_webhook_targets_raw（GET 解析失败向上报错防管理员覆盖丢失全部目标）/load_webhook_targets（发送侧解密失败降级空+留痕）/save_webhook_targets（secret None=按 id 保留已存密钥语义）与 test_webhook_target 按 id 测试，send_notification_webhook 按目标 scope 过滤；端点改 GET/PUT /api/system/notification/recipients 与 GET/PUT /api/system/webhook/config（WebhookTargetView has_secret 掩码、id 1-64/name≤100/url 1-2048 校验、HashSet 查 id 重复、validator 不递归 Vec 手动逐条 validate）+ POST /api/system/webhook/test；upgrade/0.33/upgrade_0.33.0.sql 追加第 5 节（收件人正则门槛保护转换对象数组保序、旧四键→三条初始目标且 dingtalk_secret 密文原样嵌入/wecom_key 拼前缀、旧键清理）；NotificationTab.vue 重写（Webhook 目标卡片列表增删+未保存禁测+每目标订阅勾选、邮件收件人订阅表格，通知矩阵删除），system.css 改 .recipients-table/.webhook-target 系列，前端 i18n webhook.*/notification.*/server.webhook.*/server.notification.** 键重写且补历史缺失 server.webhook.config_query_failed；gen-ts-types 再生成（新增 MailRecipient/NotifyScope/NotificationRecipients/WebhookTargetView/WebhookTargetPayload，删除 NotificationMethods/NotificationSettings 孤儿生成物）。验证：cargo fmt、workspace clippy --all-targets -D warnings、foims-auth/foims-snmp/foims-agent-service 单测（82+78+89 合计 249）、npm run typecheck/build、biome、oxlint 均通过。y 段 +1，Cargo.toml 0.33.3→0.33.4、frontend/package.json 0.1.39→0.1.40 联动；foims-auth 0.1.11、foims-snmp 0.1.8、foims-agent-service 0.1.26

v0.50.1
2026103122384801
通知配置页布局优化（仅前端，z 段 +1）：NotificationTab.vue 卡片顺序调整为 SMTP → 邮件通知 → Webhook（Webhook 移至页签最后），邮件收件人卡片标题 notification.settings 文案"通知配置"改"邮件通知"（en 同步 Email Notifications）；Webhook 卡片由逐条卡片块改为 data-table 表格行内编辑（名称/类型/地址/钉钉加签密钥/接收内容/操作六列，table-layout fixed 百分比列宽，非钉钉行密钥列"—"占位，操作列测试/删除按钮 readonly 也渲染仅禁用以固定 colspan，url_hint 提示上移表格上方），编辑逻辑与保存/测试流程零改动；system.css 删 .webhook-target/.webhook-target-grid/.webhook-target-footer/.webhook-target-actions 卡片系列，新增 .webhook-table 列宽与 .cell-input 控件类（控件加类避免 td input 降序特异度警告）、.webhook-actions-cell 改 flex gap 消除 .btn 结尾降序特异度警告，保留 .webhook-scope checkbox 组。验证：npm run typecheck/build、biome（0 警告）、oxlint（0 警告）均通过；仅 dev_version z+1，frontend/package.json 与 Cargo.toml 不联动

v0.51.0
2026103123031401
日志页新增「运行日志」页签（登录日志右侧，SSE 实时推送，y 段 +1）：主 crate 新建 src/log/live.rs——LiveLogLayer 挂订阅器链尾（过滤口径与控制台层一致但放行全部激活语言的 foims_log::{lang} 事件，普通 tracing 事件不带语言标记对所有连接可见），事件经 FieldCollector 收集（message 主文案+其余字段 k=v 追加）序列化为 LiveLogEvent{ts/level/target/lang/message/skipped} 投入进程级 broadcast 通道（容量 512，无订阅者发送即失败零开销）；新增 GET /api/logs/live（admin/auditor，沿用 admin_guard /api/logs/** 矩阵+AdminOrAuditorUser 提取器二次校验），lang 参数按 active_log_langs 白名单校验、缺省/非法回落控制台语言，BroadcastStream 逐条过滤下发 SSE named event "log"，慢消费者 Lagged(n) 下发 skipped 占位事件不断流，keepalive 15s + X-Accel-Buffering: no；setup_logging 接线该层；nginx foims.conf/foims-dev.conf 仿 /api/agents/live/stream 先例新增 /api/logs/live location（关缓冲+1h 读超时）；workspace 增 tokio-stream 0.1（sync feature，根 crate 引用）。前端 useLiveLogs 组合式函数（EventSource 随页签激活启停、断线浏览器自动重连、缓冲上限 1000 丢最旧、暂停=丢弃新事件不积压、复用 useLiveAgents.parseJson），LogsPage.vue 四页签+终端风格深色滚动窗（等宽字体、级别着色 error红/warn黄/debug灰、底部跟随滚动仅用户停在底部时生效、上翻不抢滚动条、暂停/清空按钮、连接状态红绿点），i18n zh/en 补 logs.runtime_logs、logs.live_* 九键与 server.logs.live_unavailable，gen-ts-types 生成 LiveLogEvent。验证：cargo fmt、workspace clippy --release -D warnings、gen-ts-types、npm run typecheck/build、biome（0 警告）、oxlint（0 警告）均通过。y 段 +1，Cargo.toml 0.33.4→0.33.5、frontend/package.json 0.1.40→0.1.41 联动

v0.51.1
2026110113465601
修复「下载 Agent 配置包提示 CA/证书缺失但文件实际存在」（普通 bug 修复，z 段 +1）：根因是历史 root 身份运行期签发的物料属主残留 root（私钥 0600 root:root），服务切换 foims 用户后读取被拒，下载接口把一切读错误（含权限不足）静默映射为「未生成」误导排查。代码修复：foims-agent-service/api.rs 新增 read_material——NotFound 才按 Conflict「未生成」引导，其余 IO 错误记 log.agent.package_material_read_failed 后按 Internal 的 server.agent.material_read_failed 透出（前端文案指引检查服务运行账户对 /etc/ssl/foims-ca 与 /etc/ssl/foims-certs 的读取权限），ca.pem 读取统一 read_to_string（组包改 as_bytes）；api.rs 新增两条单测（不存在→409、目录冒充文件读失败→500）。环境处置：/etc/ssl/foims-ca、/etc/ssl/foims-certs chown foims:foims（权限模式不变，属主修正后下载即时恢复 200）。i18n：服务端 log zh/en 补 package_material_read_failed，前端 server.agent 补 material_read_failed。验证：cargo fmt、workspace clippy --all-targets -D warnings、foims-agent-service 单测（86 项）、npm run typecheck/build 均通过；UDS 实测 GET /api/agents/config-package?format=zip 返回 200 且 zip 含 agent.toml/ca.pem/client.pem/client.key。仅 dev_version z+1，Cargo.toml/package.json 不联动

v0.52.0
2026110115453301
权限审计与监听端口读写双端校验（功能优化，y 段 +1）：权限审计确认服务以普通用户 foims 运行、UDS /run/foims/api.sock 属主 foims:www-data 0660、systemd AmbientCapabilities=CAP_NET_BIND_SERVICE 仅服务 trap 特权端口绑定、ReadWritePaths 限定 /var/log/foims /etc/foims /usr/share/foims/backups（web 目录刻意排除防改写自身资源）；历史 root 运行期残留属主已全部 chown foims（/etc/ssl/foims-ca、/etc/ssl/foims-certs、/var/log/foims 日志）；/usr/bin/foims 与 /usr/share/foims/web 按 scripts/build-pkgs.sh 打包约定保留 root 属主 755/644（防服务账户篡改自身程序与资源，服务与 nginx 均只读使用）。端口合法域收敛进 foims-common：新增 SNMP_TRAP_STANDARD_PORT=162、UNPRIVILEGED_PORT_MIN=1025、UNPRIVILEGED_PORT_MAX=65534 与 is_valid_trap_port（仅 162 或 1025-65534）、is_valid_agent_port（仅 1025-65534），Config::load 启动校验与 src/system/config.rs validate_config_for_save 落盘前校验同口径（非法端口写入即 400，防「保存后重启永久起不来」的持久化自伤），foims-common 补边界单测（0/161/443/1024/1025/65534/65535）；前端 SnmpTrapCard/AgentServiceCard 保存前同口径校验并同步 input min/max，i18n zh/en 更新 agent 与 snmp_trap 两处端口范围文案。验证：cargo fmt、workspace clippy --all-targets -D warnings、foims-common 单测（含 2 新增端口用例）、npm run typecheck/build 均通过；pak.sh 部署（v0.33.6/0.1.42）后 UDS 实测 PUT /api/system/config——trap 80/161/1024/65535 与 agent 80/161/1024/65535 全部 400（invalid_param 精确到参数名与合法域），trap 162/1525 与 agent 1025/65534/9100 落盘 200，GET 读侧复核一致；trap 启用重启后 ss 实测 0.0.0.0:162（cap_net_bind_service 特权绑定生效）与 0.0.0.0:1525（非特权）均由 foims 进程持有，验证后恢复 enabled=false 基线；新 Config::load 启动校验下服务重启正常。部署前置：仓库 config.toml 与生产漂移（缺 snmp.live/ipmi 段与 agent enrollment_token），已回同步后再部署避免 pak.sh 覆盖丢配置。y 段 +1，Cargo.toml 0.33.5→0.33.6、frontend/package.json 0.1.41→0.1.42 联动

v0.52.1
2026110116052901
数据管理页监听配置提示细化（仅前端，z 段 +1）：AgentServiceCard 与 SnmpTrapCard 的保存前校验由「地址+端口混合一条 toast」拆分为独立校验、分别提示——先校验监听地址（不合法提示 snmp_trap/agent_service.invalid_bind_addr，带 {host} 实际输入值），再校验端口（不合法提示新增的 invalid_port，带 {port} 实际输入值，合法域文案与后端同口径：trap 仅标准 162 或非特权 1025-65534、agent 仅非特权 1025-65534），命中即提示并终止提交；i18n zh/en 每段由 1 键拆为 2 键（invalid_bind_addr 文案收敛为纯地址语义 + 新增 invalid_port）。验证：npm run typecheck/build、biome（0 警告）、oxlint（0 警告）均通过；pak.sh 部署后线上产物含两条校验分支与新文案。仅 dev_version z+1，Cargo.toml/package.json 不联动

v0.52.2
2026110116273001
修复 v0.52.1 校验提示的 TypeError 回归（仅前端，z 段 +1）：v-model 对 `<input type="number">` 自动 looseToNumber，端口 ref 在合法输入时是 number（非法残留才是 string），v0.52.1 新增的 invalid_port 提示参数直接对 `.value` 调 `.trim()` 导致校验失败分支抛 `TypeError: ... .trim is not a function`——表现为填非法端口点保存时警告 toast 不弹且控制台报错。修复为 `String(...).trim()` 统一转字符串后再 trim（AgentServiceCard / SnmpTrapCard 两处），并补注释说明该隐式转换。验证：npm run typecheck/build、biome（0 警告）、oxlint（344 文件 0 警告）通过；pak.sh 部署后线上产物 SystemPage 确认两处 `String(...).trim()` 修复形态（浏览器自动化环境不可用，待用户界面复测）。仅 dev_version z+1，Cargo.toml/package.json 不联动

v0.53.0
2026110217450001
主机监控实时链路补齐 agent 来源（三来源全链路秒级显示，y 段 +1）：agent 上报此前仅入库 + 页面 30s 轮询，看板 SOURCE_RANK 已预留 agent 优先级却收不到实时行。live_poll.rs 末尾新增 AgentLiveRegistry（DashMap 以 machine_id 为键——删除主机后同机重注册以新 monitored_hosts.id 覆盖同键条目自愈，不留旧 id 幽灵行；值含 host_id/machine_id/hostname/ip/os + AgentLiveSnapshot 读数），ingest.rs 入库事务提交成功后同步 upsert（429 频控/禁用/入库失败路径不覆盖，注册表始终反映已接受上报），流量合计抽 sum_net_bps 自由函数与历史快照 json 复用；LiveSummaryRow 新增 disk_usage_pct（仅 agent 有实时读数，SNMP/IPMI 为 None），live_api.rs agent_summary_row（vendor 取 OS 名、device_id 取 monitored_hosts.id）并入 summary_rows 三来源合并排序，SSE StreamState 增 agent_revision 任一注册表写入即推送；ReportContext/spawn_report_server_task/AppState/main.rs 全链路传注册表。前端 AgentsPage liveRow 对 source='agent' 按 `agent|{行 id}` 匹配实时快照并新增 liveDisk 磁盘列实时覆盖，AgentDetailModal live 对 Agent 主机按行 id 精确匹配（删除原 IP 兜底防同 IP 误配他源行）、网络卡实时徽标收敛为仅 SNMP 来源，两处来源徽标统一 SOURCE_LABEL_KEYS 三来源映射；i18n zh/en 新增 agents.source_agent；gen-ts-types 再生成 LiveSummaryRow。验证：cargo fmt、workspace clippy --all-targets -D warnings、foims-agent-service 86 单测（含 agent 注册表同机覆盖与 agent 摘要/三来源排序新用例）、npm run typecheck/build、biome、oxlint 均通过。y 段 +1，Cargo.toml 0.33.6→0.33.7、frontend/package.json 0.1.42→0.1.43 联动；foims-agent-service 0.1.26→0.1.27

v0.54.0
2026110218000001
agent 实时秒级采集上报推送通道（y 段 +1）：此前 agent 源实时性受制于批量攒批节拍（report_interval_secs 缺省 60、服务端下限 10s），显示延迟等于上报间隔。新增 `POST /agent/v1/realtime` 实时通道：agent 采集出帧即送（best-effort 不重试不排队，通道满/失败即弃帧，下一帧自然取代），服务端不入库、仅覆盖内存实时注册表——实时粒度即 agent 采集间隔 collect_interval_secs（缺省 5s，可配 1-60s），批量通道（攒批入库/频控/退避）节奏与语义完全不变。实现：reporter.rs 将 TLS 构建/建链/单请求交换抽为 build_client_tls_config、build_h3_transport、h3_post_once 自由函数（上报循环与实时通道共用同一建链与请求模板），新增 RealtimeChannel 独立复用连接任务（mpsc 容量 2，try_send 满即丢帧保最新帧语义），采集任务每帧同时入批量队列与实时通道，report_once/--once 冒烟路径不受影响；服务端 ingest.rs 新增 realtime 处理分支（与批量通道共用 bearer_token 鉴权与 validate_report 单帧校验口径，唯一库交互为 machine_id→id/status 只读 SELECT，未注册 404/已禁用 403/库错 500，成功回 {"ok":true}），快照构造抽 agent_live_snapshot 共用助手；AgentLiveRegistry.upsert 改 collected_at 单调合并（entry 原子写，实时帧先到、批量旧帧后到不回退快照，未更新不推进代数不触发 SSE 空推送）；report_server.rs handle_request 路由放行 REALTIME_PATH（body 上限沿用单帧 max_report_bytes）。前端零改动（注册表+SSE 链路复用）。验证：cargo fmt、双 workspace clippy --all-targets -D warnings、foims-agent-service 86 单测（含注册表旧帧不回退用例）、foims-agent workspace 179 单测。y 段 +1，Cargo.toml 0.33.7→0.33.8、foims-agent-service 0.1.27→0.1.28；foims-agent workspace 0.5.2→0.5.3；前端无改动不联动

# v0.54.1

# 2026110117551801
# 2026年11月1日17点55分18秒 第1次修改

1.foims-agent 正式拆分至独立仓库（CI 拆分，主程序代码零改动）：agent 交叉构建 job（fmt/clippy/本机单测 + zig 0.13.0/llvm-mingw 20261020 交叉工具链 + build-agent.sh 多平台组包）自 ci.yml/release.yml 整体迁至独立仓库（github.com/o-i-i-o/foims-agent）自建的 .github/workflows/ci.yml（push 全分支 + tag 触发，tag v<agent 版本> 随 CI 发布至该仓 Release）与 release.yml（workflow_dispatch 手动发布，prepare 任务从该仓根 Cargo.toml [workspace.package] version 推导 tag）；主仓 ci.yml/release.yml 删除 agent-build 任务与发布物中的 agent 系统包（release needs 收敛为 [build] / [prepare, build]，files 仅留主程序产物）——此前 agent 构建依赖主仓检出 foims-agent/ 目录，而该目录已被 .gitignore 排除，远端 checkout 后并不存在，拆分同时消除该断链；da-scripts/release_check.sh 摘除 Agent 多平台安装包检查（对齐 CI 口径）；biome.json 清理 !foims-agent 死排除项；code-style.md 工程结构与依赖树说明同步独立仓库口径。验证：agent 仓 workflow 与原主仓 agent-build 任务逐步对齐复核；主程序代码/Cargo.toml 零改动，仅 dev_version z+1，Cargo.toml/package.json 不联动

# v0.54.2

# 2026110120380001
# 2026年11月1日20点38分00秒 第1次修改

1.修复主仓 CI 单元测试失败（controlled_copies_in_sync，build 任务「运行测试」步骤 exit 101，本地通过仅因本地存在 foims-agent 联调 clone）：根因是拆分遗漏——双副本漂移校验测试 crates/foims-common/tests/protocol_copy_sync.rs 硬编码读取主仓根下 foims-agent/crates/foims-protocol/ 副本文件，该目录为 .gitignore 排除的本地联调 clone，主仓 CI 检出后不存在，读取即 panic（No such file or directory）。修复方案（跳过而非删除）：漂移自动检查确有必要保留，测试内检测主仓根 foims-agent/ 目录存在性——本地联调 clone 存在时执行比对拦截漂移，主仓 CI 检出后该目录不存在时 eprintln 提示并跳过（主仓 CI 只管主仓，两项目仅通过网络通信、代码完全隔离）；模块头注释同步更新跳过口径。验证：cargo fmt、cargo clippy -p foims-common --release --all-targets 0 警告；测试双分支实测——本地（clone 存在）1 passed（比对执行）、临时移走 foims-agent 模拟 CI 环境 1 passed（走跳过分支，未跳过则会因读文件 panic），移走目录已复原；仅 dev_version z+1，Cargo.toml/package.json 不联动

v0.55.0
2026110220300001
修复主机监控 SNMP/agent 双源采集数据不准与互不一致（功能 bug 更新，y 段 +1）：实测本机（snmpd 5.9.5.2）三轮 snmpget 与 /proc/stat 对照，定位四处根因并修复——1) CPU 使用率虚高实锤：ssCpuRawSoftIrq/Steal/Guest/GuestNice 四列计数器在 snmpd 5.9.5.2 下完全失真（SoftIrq 47.9M vs /proc/stat 真实 10k、Steal 34.2M vs 真实 0、Guest≈intr 中断总数，疑似读错 /proc/stat 行），计入 total 导致 10s 窗口 3% 真实使用率被算成 37%；metrics.rs CPU_RAW_TOTAL_IDXS 由 8 列收敛为 User/Nice/System/Idle/Wait/Interrupt 六列（多轮对照与 /proc/stat 一致），注释记录实测依据与残余误差评估（total 少计真实 softirq/steal，使用率略高估 <5% 可接受）；2) SNMP 磁盘列表混入 10 个 tmpfs 噪音行：disks_from_storage 新增 is_excluded_disk_mount 过滤 /dev、/proc、/sys、/run 四子树挂载点（net-snmp 为 tmpfs 建 FixedDisk 行），与 agent 侧排除口径对齐；3) 网卡 ifSpeed=0（RFC 2863 速率未知）被归一为 0 Mbps 误导展示：speed_mbps 改 filter(|s| *s > 0) 映射 None（结构体注释「未知为 None」从未落实）；4) agent 侧 filesystem 挂载点排除 /run/credentials/.+ 扩为 /run 整树（tmpfs 内存盘不进容量统计，与 SNMP 侧一致，node_exporter 基础规则不变注明本项目扩展）。agent 服务端证书 SAN 自愈（此前 IP 变更后旧证书 SAN 不含新 IP，agent 握手永久失败需人工删指纹文件重签）：cert.rs 新增 AGENT_CERT_SANS_FILE（agent_cert_sans，签发时记录本机非回环 IP 清单），try_reuse_material 复用前经 sans_cover_current_ips 校验当前网卡枚举仍被覆盖（纯函数 ips_covered：非回环 IP 须全部覆盖、枚举失败空列表不误杀、多余旧 IP 无害），文件缺失/不覆盖即重签落盘新记录；i18n 复用 certs_reuse_rejected 错误键新增 cert_sans_missing/cert_sans_stale detail 值。配套环境修复（v0.52.0 权限收紧引入的回归）：foims.service 模板与本机部署的 ReadWritePaths 补 /etc/ssl/foims-certs——ProtectSystem=strict 下 IP 变更触发重签时写证书报 Read-only file system，证书永不更新 agent 全部握手失败。验证：cargo fmt、workspace clippy --release -D warnings、foims-snmp 88 + foims-agent-service 88 单测（新增磁盘 tmpfs 过滤、SAN 覆盖纯函数与 SAN 记录缺失重签用例）、foims-agent 仓 filesystem 7 单测均通过；pak.sh 部署（v0.33.9）后本机双源实测：SAN 自愈实锤——重启日志 cert_sans_missing 触发重签、agent_cert_sans 落盘全部非回环 IP；磁盘列表两侧 4 行完全对齐（/、/media/oi-io/data、/boot/efi、/tmp，total/used 逐字节相等，tmpfs 噪音清零）；speed_mbps 未知口 null 化（前端显示 "-"）；CPU 使用率 SNMP 10.70/12.19% vs agent 12.62/13.33%（跨轮），与 /proc/stat 真实量级吻合，修复前 SNMP 虚高 37-99% 问题消除；首轮 99.99% 瞬态确认为新旧口径 total 跨代差值的预期一次性现象，次轮起正常；live 秒级链路 CPU 走 hrProcessorLoad（每核 1 分钟均值）不涉 ssCpuRaw，无需改动。y 段 +1，Cargo.toml 0.33.8→0.33.9、foims-snmp 0.1.9、foims-agent-service 0.1.29 联动；前端无改动不联动；agent 侧 filesystem 修改见独立仓 v0.5.5

v0.56.0
2026110221300001
系统-数据管理 Agent 服务配置卡片优化清理（agent 已与主程序解耦，功能变更 y 段 +1）：1) 移除「上报间隔」「采集间隔」配置项——agent 采集/上报节奏由 agent 独立维护，服务端不再可配：AgentConfig 删 report_interval_secs/collect_interval_secs 字段（serde 缺省解析，旧配置文件遗留键自动忽略、保存回写自愈清除），系统配置保存校验同步删除；组包 agent.toml 固定 collect_interval_secs=1（每秒采集）、report_interval_secs=10（批量上报，agent 侧 clamp [1,60]/[10,3600] 兜底且取值即生效值）；上报响应控制面 report_interval 固定 10（ingest.rs REPORT_INTERVAL_SECS 常量，取代原「配置值.max(10)」动态来源），上报频控 min_gap=间隔/2=5s 基准不变；agent 离线判定与上报间隔解耦，tasks.rs 新增固定基准 AGENT_OFFLINE_BASE_INTERVAL_SECS=60（等价原生产 report=60×倍数，容忍丢批不误判），offline_factor 仍可配，AgentOfflineTaskExecutor::new 注册与 system_agent_offline 任务默认配置同源常量；ReportContext::report_interval_secs() 删除；api.rs 组包 clamp 常量删除。2) 移除「Agent 版本」展示——agent 独立仓与主程序版本各自演进，卡片内展示服务端版本号已无意义：AgentServiceCard 删 agent_dist_version 字段与仅为其服务的 getSystemInfo 查询（顺带修正下载按钮 from-group 拼写与嵌套 form-row 结构）。3) i18n 清理：zh/en 删 agent_service.report_interval、agent_service.collect_interval、system.agent_dist_version 三键。4) config.toml.example 同步删 report_interval_secs 并更新 offline_factor 注释。验证：cargo fmt、clippy --release --workspace -D warnings、foims-agent-service 88 单测、vue-tsc、vite build 全部通过；pak.sh 部署后配置卡片仅剩监听地址/端口/上报地址模式/令牌/分发项，agent 上报链路（频控 5s、下发 10s、离线 60×3）实测正常。y 段 +1，Cargo.toml 0.33.9→0.33.10、foims-common 0.3.20、foims-agent-service 0.1.30 联动；前端 package.json 0.1.43→0.1.44；agent 独立仓无代码变更不联动

v0.56.1
2026110123022601
调整系统页二级页签顺序（纯 UI 顺序变更）：系统设置（system-config）移至首位并成为默认页签，系统信息（system-info）移至末位；SystemPage.vue TABS 清单与模板 v-show 区块顺序同步调整，页签 id 未变、localStorage 记忆值与深链 query 均仍有效。验证：npm run typecheck 通过；仅 dev_version z+1，frontend/package.json 0.1.44→0.1.45，Cargo.toml 不联动

v0.57.0
2026110200124101
移除系统信息页签「运行时长」展示（冗余字段清理，功能变更 y 段 +1）：该字段与「系统配置-服务管理」FOIMS 行运行时长同源同值（同一 START_TIME、同一 now−start 公式），且其唯一可见场景即服务运行中（页面渲染依赖登录态与后端实时 API，服务未运行时整个页签无法加载数据），信息量为零；接口瞬时失败时前端回退 formatUptime(0) 显示"0秒"误导，服务管理侧则对不可用有诚实的"不可用"占位。拆除完整链路：SystemInfo 结构体删 uptime_seconds 字段与 get_system_info 内 now−START_TIME 计算（config.rs），start_time()/START_TIME 保留（services.rs 服务管理口径仍在用）；SystemInfoTab.vue 删运行时长 dl 行、uptimeText computed 与 formatUptime 导入；i18n zh/en 删 system.uptime 键（uptime_days/hours/minutes/seconds 为 formatUptime 共用、ServicesCard 仍在用，保留）；gen-ts-types 再生成 SystemInfo.ts。验证：cargo fmt、cargo clippy --release -D warnings、gen-ts-types、npm run typecheck/build、biome、oxlint 均通过。y 段 +1，Cargo.toml 0.33.10→0.33.11、frontend/package.json 0.1.45→0.1.46 联动

v0.58.0
2026110200201201
快照型时间字段前端动态推进（功能优化 y 段 +1）：系统时间/运行时长此前为取数瞬间静态快照，页面停留期间数值冻结。新增共享秒级时钟 composable useNowTick（模块级单例 interval + 引用计数，首订阅启动、末卸载停止，取代每组件自挂 interval）；服务管理运行时长以最近一次成功取数时刻（useQuery dataUpdatedAt）为基准、每行 uptime_seconds + 流逝秒数逐秒推进（取数成功自动重置基准，用户点刷新即校准）；系统信息系统时间以服务端 timestamp 基准时刻 + 本地流逝毫秒推算服务端当前时刻（两端偏移取数瞬间一次折算不重复累积），后端未携带基准时回退本机时钟同样逐秒推进；AgentDetailModal 原组件级 nowTick/interval 收敛至 useNowTick（同功能去重，行为不变）。验证：npm run typecheck/build、biome、oxlint 均通过；仅前端修改，y 段 +1，frontend/package.json 0.1.46→0.1.47，Cargo.toml 不联动

v0.58.1
2026110209140101
新增内部文档 docs/监控采集与通知模块架构.md：梳理主机监控、SNMP/Agent/IPMI 信息采集、日志-通知、通知推送六模块的业务逻辑、数据流 ASCII 图、关键文件清单、数据表与三源同构性对照表，并汇总 11 条后续演进关注点（收件人角色口径漂移、Trap 无持久化无外发、IP 变更 LIKE 去重脆弱、host_metrics_history 无分区等）。纯静态分析文档，无代码改动，行号基于撰写时 HEAD 注明会漂移。仅 dev_version z+1，Cargo.toml/package.json 不联动

v0.59.0
2026110209520101
Agent 上报形态改为逐帧实时 + 主机监控版本语义修正（功能变更 y 段 +1，主仓与 agent 独立仓 v0.7.0 配套）：1) 逐帧实时上报（定案「逐帧直发+断网缓冲」）：采集与上报同拍（collect_interval_secs，组包固定 1s），常态单帧直发 POST /agent/v1/report 逐帧入库，快照与历史与服务端同步到秒级；积压（断网缓冲/采集波动）自动多帧组批，AgentReportBatch 降级为断网补传载体（120 帧/8MiB 与对半拆分重试口径不变）；整链移除 /agent/v1/realtime 旁路通道（服务端 ingest::realtime、report_server 路径放行、REALTIME_PATH 常量，agent 端 RealtimeChannel 与第二条 QUIC 连接，每机两连接收敛为一条），AgentLiveRegistry 改由 ingest 入库路径唯一喂养；服务端频控改 MIN_REPORT_GAP_SECS=0.5s 独立闸门（原 10s 间隔的一半 5s 会误杀逐帧节拍），下发 response.report_interval=1 仅留旧协议兼容（agent 新版不消费，退避基准改采集节拍、删除 ±10% 抖动与 rand 依赖），组包 agent.toml 不再写 report_interval_secs（旧键反序列化忽略，offline 基准 60s 不随上报间隔联动仍独立）；2) 版本语义修正（定案「UI 文案+协议版本语义」）：agents.col_version/agents.label_version zh「Agent 版本」→「Client 版本」、en →「Client Version」（数据字段 agent_version 与协议不动）；ReportResponse.latest_version 废止服务端版本占位、恒空串（旧版 agent 反序列化要求字段存在故保留），ReportContext.server_version 字段与 main.rs env! 传参删除；配置包 deb/rpm 版本戳参数改名 conf_version 并注明「配置包版本=签发端演进号，非 client 本体版本」；foims-common 与 foims-protocol 受控副本注释同步。注意事项：host_metrics_history 存储量级不变——批量补收期间本就逐帧入库（每批遍历全部帧），恒为每 host 约 86,400 行/天；本次降的是入库时延（批量滞后 ≤10s → 秒级直达）与连接数（每机 QUIC 双连接 → 单连接），滚动清理按规模收紧 retention_days 的既有建议照旧有效。验证：cargo fmt、workspace clippy --release -D warnings、foims-agent-service 88 + foims-common 154 单测、npm run typecheck/build 均通过；agent 独立仓 v0.7.0（fmt、clippy --all-targets -D warnings、test 11 项全绿）。y 段 +1，Cargo.toml 0.33.11→0.33.12、frontend/package.json 0.1.47→0.1.48；agent 独立仓 v0.6.0→0.7.0

v0.59.1
2026110210465501
主机监控温度展示文案「最高温度」全面改为「温度」（仅前端，z 段 +1）：列表温度列实际取值已是实时快照优先（liveRow.temperature_c，缺实时回退最近上报 max_temp），「最高温度」命名与展示语义不符。i18n zh/en 四处同步——agents.col_temp（列表列头）、agents.alert_temp（阈值配置标签，去「最高」）、log.host_alert.body_temp/resolved_temp（告警推送文案）；AgentsPage formatTemp 注释同步（注明实时优先回退口径）。数据列 max_temp 与后端聚合函数不动（存储层语义确为帧内传感器最大值，仅展示层更名）。验证：npm run typecheck/build、biome、oxlint 均通过；仅 dev_version z+1，frontend/package.json 0.1.48→0.1.49，Cargo.toml 不联动

v0.60.0
2026110211525801
通知-推送域拆分：站内通知与外发渠道自四个 crate 收敛为独立 foims-notify（模块级拆分，功能变更 y 段 +1）。新建 crates/foims-notify（0.1.0）三层边界——原语层 in_site（create_notification 返回新行 ID 供 episode 就地刷新 / encode_notification_content / active_admin_ids 管理员口径 admin+secadmin）；渠道层 smtp（通用发信、SmtpConfig 读写、收件人订阅解析，send_notification_email 收敛为 pub(crate)）与 webhook（通用/钉钉 HMAC-SHA256 加签/企微目标解析与逐目标尽力推送）；门面层 dispatch::push（邮件按收件人订阅过滤、结果上抛调用方记日志，webhook 内部留痕；站内通知不进门面，保留 episode 按住 ID 刷新与 trap 逐管理员写行语义）；NotifyKind/NotifyScope 订阅类型随迁。业务文案留生产方：ip_change/device_swap 文案与 resolve_workstation_name 迁 foims-resource 新模块 workstation_notify（0.1.21，ip.rs 两处调用改 crate 路径，保留 log.ip_change.* 日志键）；unknown_device 文案迁 foims-snmp events/mod.rs（0.1.10，内联 admin_ids 查询换 active_admin_ids 原语，保留 log.snmp.unknown_device_email_failed）；host_alert 文案与 host_alert_metric_label 迁 foims-agent-service alert_episode.rs（0.1.31，trap.rs 改 foims_notify 原语，保留 log.agent.alert_email_send_failed）。foims-auth 删 smtp.rs/webhook.rs 瘦身（0.1.12，lettre/hmac/reqwest 依赖移除，登录验证码/两步验证/密码重置/SSO 邮件开关 5 处改 foims_notify::smtp）；foims-snmp 保留对 foims-auth 依赖（events/api.rs 鉴权提取器 AuthUser 用途，前期调研仅覆盖通知渠道存在遗漏，lib.rs 依赖方向文档同步修正）。主 crate system/config.rs 通知配置读写全部导入切 foims_notify（SMTP 收件人/Webhook 目标/测试连通性）。日志键、i18n key、DB 结构、API 契约全部不变；MailRecipient/NotifyScope/WebhookChannel 三个 ts-rs 导出经 gen-ts-types 再生成零漂移。验证：cargo fmt、workspace clippy --release -D warnings、五相关 crate 306 单测（notify 15/auth 76/snmp 77/resource 50/agent-service 88）、gen-ts-types 零漂移、vue-tsc、pak.sh 全量部署通过。y 段 +1，Cargo.toml 0.33.12→0.33.13，foims-notify 0.1.0（新增）、foims-auth 0.1.11→0.1.12、foims-snmp 0.1.9→0.1.10、foims-resource 0.1.20→0.1.21、foims-agent-service 0.1.30→0.1.31 联动；前端无改动不联动

v0.61.0
2026110215230601
组织管理页两栏布局重构（仅前端，功能优化 y 段 +1）：页面改为左右两栏——左栏保留原组织树（展开/折叠/搜索过滤 + 节点 CRUD + 模板管理入口），右栏新增 OrgDetailPanel 详情面板展示选中组织的详情；点击树节点行即选中（选中行高亮，OrgTreeNode 注入上下文新增 selectedId/onSelect），树节点行内「人员管理/房间列表」按钮改为选中该节点并直接切换右栏对应视图，人员与房间两个视图经右栏分段按钮切换、同时只显示一个。原 EmployeeModal/OrgRoomsModal 两个弹窗拆除，逻辑迁移为内联组件 OrgEmployeeList（人员增删改，编辑表单保留小号弹窗；切换组织节点自动丢弃残留编辑/确认状态）与 OrgRoomList（房间只读，展示直属组织与工位/机柜计数）；选中态按节点 id 存储、经树数据查找派生节点对象（重命名/删除自动跟随，删除后右栏回落占位提示）。organization.css 新增两栏布局/详情面板/分段页签/列表行样式，删除废弃弹窗样式（employee-modal-org/employee-table-container/org-room-table-container/modal-toolbar/employee-actions-cell），树节点 cursor 改 pointer 并补 selected 高亮，≤768px 两栏纵向堆叠；i18n zh/en 新增 organization.detail_title/detail_empty/tab_employees/tab_rooms/stat_workstations/stat_cabinets 六键。验证：npm run typecheck/build、biome（0 警告）、oxlint（346 文件 0 警告）均通过。y 段 +1，frontend/package.json 0.1.49→0.1.50，Cargo.toml 不联动

v0.61.1
2026110215535301
组织管理页两栏等宽 + 右栏表格化展示（v0.61.0 布局细化，仅前端，z 段 +1）：org-page-layout 由 flex（右栏固定 380px）改为 grid 两栏等分（repeat(2, minmax(0,1fr))，≤768px 单列堆叠）；右栏人员/房间由自定义列表行改回全局 data-table 表格口径——OrgEmployeeList 恢复姓名/性别/电话/邮箱/入职日期/操作六列（错误行 table-error-cell、空行 empty-row、操作列 org-detail-actions-cell 图标组），OrgRoomList 恢复序号/房间名称/类型/所属组织/工位数量/机柜数量六列；organization.css 删除 org-detail-row* 列表行样式系列，org-detail-list-body 改表格滚动容器，新增 org-detail-actions-cell；i18n zh/en 删不再引用的 organization.stat_workstations/stat_cabinets 两键。验证：npm run typecheck/build、biome（0 警告）、oxlint（346 文件 0 警告）均通过；仅 dev_version z+1，frontend/package.json 0.1.50→0.1.51，Cargo.toml 不联动

v0.61.2
2026110215391801
组织管理页入口收敛与人员表新增入口前移（v0.61.x 交互细化，仅前端，z 段 +1）：组织树节点行内删除「人员管理/房间列表」两个悬停按钮（右栏详情面板常驻展示，入口冗余），行内操作仅保留新增下级/编辑/删除，OrganizationPage onAction 同步移除 employees/rooms 分支；人员/房间视图切换状态由页面持有改为 OrgDetailPanel 内聚（tab ref 内部化，删除 tab prop 与 switch 事件）；人员表格末列表头由「操作」文案改为「新增员工」按钮（对齐 UsersTab 管理口径：th 内嵌 btn btn-primary btn-sm，行内保留编辑/删除），删除表格上方独立新增工具条。清理：organization.css 删 org-detail-list-toolbar 与 icon-btn.btn-employees 样式，i18n zh/en 删 organization.manage_employees/view_rooms 两键。验证：npm run typecheck/build、biome（0 警告）、oxlint（346 文件 0 警告）均通过；仅 dev_version z+1，frontend/package.json 0.1.51→0.1.52，Cargo.toml 不联动

v0.61.3
2026110215463801
组织管理页渲染反馈三处微调（v0.61.x 视觉细化，仅前端，z 段 +1）：两栏比例由等宽改 4:6（grid-template-columns minmax(0,4fr)/minmax(0,6fr)，左树 4 右详情 6）；「新增节点」按钮文案改「新增根节点」（organization.add_node zh/en 同步，OrgNodeModal add-root 模式弹窗标题随键值一并生效）；详情面板头部备注由名称行下第二行改为与名称/类型徽章同行展示（org-detail-header-desc 移入 org-detail-header-name 行内，flex 占剩余宽度、省略号截断，title 保留全文；header-name 去 flex-wrap）。验证：npm run typecheck/build、biome（0 警告）、oxlint（346 文件 0 警告）均通过；仅 dev_version z+1，frontend/package.json 0.1.52→0.1.53，Cargo.toml 不联动

v0.61.4
2026110216000201
组织管理页默认选中首个组织（v0.61.x 体验增强，仅前端，z 段 +1）：页面监听树数据（OrganizationPage watch treeData），选中态为空且存在根节点时自动选中首个根节点，右栏详情面板加载后即有内容、免空占位；已有选中态（含删除后失效 id）不劫持、搜索过滤变化不静默改选。验证：npm run typecheck/build、biome（0 警告）、oxlint（346 文件 0 警告）均通过；仅 dev_version z+1，frontend/package.json 0.1.53→0.1.54，Cargo.toml 不联动

v0.62.0
2026110216160001
服务管理全链路移除自伤风险操作（功能变更 y 段 +1）：foims 与 nginx 任一停止都会使 Web 管理界面不可达且无法经程序自身拉起，暴露启动/停止/禁用自启只会有自伤风险，全链路收敛为仅重启/重载/启用自启——foims-services 删除 start/stop/disable 方法与 ServiceOp::Start/Stop/Disable 变体（ALL 收敛为 [Restart, Reload, Enable]，模块头记录设计依据：restart 由 systemd Restart=always 兜底不会永久停机，启停与自启开关由运维在服务器上执行 systemctl），parse 对 start/stop/disable 一律 None 并补断言用例；主 crate service_operation 分发简化（Stop 分支删除，restart 延迟执行路径仅剩 nginx，foims restart 仍走自重启特例），模块头同步口径；前端 ServicesCard 删除启动/停止/禁用自启按钮（会话内先行的「启停/自启合并为单按钮」方案一并废弃），仅保留重启/重载/启用自启三按钮，启用自启在 item.enabled 时置灰不可点，ConfirmModal 仅剩 restart 确认（danger 标记随 stop 删除），ServiceOp 类型收敛为 restart|reload|enable；i18n zh/en 删 services.op_start/op_stop/op_disable/confirm_stop/confirm_stop_*/stop_done_foims/stop_sent_nginx 八键，help.card.services 文案改为「可执行重启、重载与启用自启，启停与自启开关由运维执行 systemctl」；api/system.ts runServiceOp 注释同步。验证：cargo fmt、cargo clippy -p foims-services -p foims --release --all-targets 0 警告、foims-services 12 单测（含 start/stop/disable 拒绝断言）、npm run typecheck/build、biome、oxlint 均通过。y 段 +1，Cargo.toml 0.33.13→0.33.14、foims-services 0.1.0→0.2.0、frontend/package.json 0.1.54→0.1.55 联动

v0.63.0
2026110217050001
证书管理引入用途字段并按用途执行操作（功能变更 y 段 +1）：证书不再仅用于 nginx（还用于 foims-agent mTLS 及后续新增用途），生成/导入时必须声明用途——foims-x509-management 新增 CertPurpose 枚举（Nginx/Agent，serde 小写、Default=Nginx、ts_rs 导出），用途随文件名前缀持久化（生成/导入前缀由 create_/import_ 改为 nginx_/agent_，listing::purpose_from_stem 按「系统物料精确匹配 agent_cert/agent_client_cert → 用途前缀 → 历史/未知前缀按 nginx 兜底」派生，历史证书无需迁移）；GenerateCertRequest 新增 purpose 字段（serde default，历史请求体兼容），EKU 按用途签发（nginx→ServerAuth、agent→ClientAuth，与 agent 系统物料同构），import_certificate 新增 purpose 参数（multipart purpose 字段，缺省 nginx、非法值 400）；操作按用途收敛：apply_certificate_to_nginx 前置守卫拒绝非 nginx 用途（agent 系统物料一并拒绝），delete_certificate 拒绝删除 agent 系统物料（误删会导致 agent mTLS 永久失效，此前列表可直接删除属隐患），生成 handler 的 public_url 主机名 SAN 注入仅对 nginx 用途执行，操作日志 details 记录 purpose；主 crate certificate.rs 模块头同步用途口径。前端 CertGenerateModal/CertImportModal 新增用途下拉（默认 nginx），CertificateCard 清单新增「用途」列（系统物料单独标注）、应用按钮仅 nginx 用途显示、系统物料隐藏删除按钮；gen-ts-types 再生成（GenerateCertRequest/CertFileInfo 变更、CertPurpose 新增）；i18n zh/en 增 cert.col_purpose/purpose/purpose_nginx/purpose_agent/purpose_system 与 server.certificate.purpose_invalid/apply_not_applicable/delete_system_managed。验证：cargo fmt、cargo clippy -p foims-x509-management -p foims --release --all-targets 0 警告、foims-x509-management 53 单测（新增用途解析/派生/EKU 随用途/守卫拒绝用例）、npm run typecheck/build、biome、oxlint 均通过。y 段 +1，Cargo.toml 0.33.14→0.33.15、foims-x509-management 0.2.4→0.3.0、frontend/package.json 0.1.55→0.1.56 联动

v0.64.0
2026110216564101
Webhook 目标编辑由表格直填改为模态框（交互功能优化 y 段 +1，仅前端）：新建 WebhookTargetModal.vue（名称可选/渠道类型/URL 必填原生校验/钉钉加签密钥仅钉钉渠道展示且留空保留已存密钥/接收内容三勾选，url_hint 提示移入弹窗，确认 emit 表单结果仅回写页面草稿、统一落库仍由卡片「保存」承担，草稿类型 WebhookTargetDraft/WebhookTargetFormResult 由弹窗 export 供列表侧复用）；NotificationTab.vue Webhook 表格由六列行内编辑收敛为四列纯展示（名称无值"—"占位/类型渠道文案/接收内容 scopeText 拼接无勾选"—"占位/操作），操作列新增「编辑」按钮打开弹窗，「添加目标」由直接 push 空行改为弹窗确认后写入完整草稿，未保存目标禁测与删除逻辑零改动；system.css webhook-table 列宽收敛为 20/16/42/22 四列并删除 .cell-input 行内控件类，.webhook-scope 注释同步弹窗归属；i18n zh/en 增 webhook.edit_target。验证：npm run typecheck/build、biome（0 警告）、oxlint（348 文件 0 警告）均通过。y 段 +1，frontend/package.json 0.1.56→0.1.57 联动，Cargo.toml 不联动
