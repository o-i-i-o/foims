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
