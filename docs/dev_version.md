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
