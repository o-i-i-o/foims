# 本文件为后端开发用版本号控制规则
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
# crate版本各自递增，某crate版本变更主Cargo.toml的版本都要关联变更（本次修改只涉及crate a则修改crate a 和lib的版本号，crate b版本号不变，若本次修改只涉及前端，则所有crate版本号不变，前端资源按照自己的规则更新版本号，lib更新版本号）

# v0.0.1

# 2026100712171501
# 2026年10月7日12点17分15秒 第1次修改 （01用来规避极端情况，通常不可能1秒修改几次）
# 版本采用文本累加机制，每次版本更新新增此版的版本号 时间序号 更新内容。
# 此版本机制自2026年10月7日实行

v0.0.1
2026100713050101
1.优化版本号规则

v0.0.2
2026100713382201
1.子网编辑模态框描述框改为整行布局（左缘对齐第一列头、右缘对齐第二列尾），新增 form-group-block 模态框表单变体样式，前端资源版本同步 bump，lib 版本 0.21.24→0.21.25

v0.0.3
2026100714460401
1.描述框 label 对齐规则改为与行内 label 一致（最小 80px 定宽、文字居中），消除与第一列头文字的错位，前端资源版本同步 bump，lib 版本 0.21.25→0.21.26

v0.0.4
2026100714582801
1.子网编辑模态框描述框删除标题（textarea 改用 data-i18n-aria-label 保留无障碍命名），左侧以 --space-sm 缩进与第一列标题文字视觉取齐，前端资源版本同步 bump，lib 版本 0.21.26→0.21.27

v0.0.5
2026100715175401
1.描述框新样式统一推广至全部 9 个模态框（网络区域/房间/机柜/机柜位置/设备/设备端口详情/工位/组织/组织模板编辑），设备-网卡管理容器内描述字段按要求排除，前端资源版本同步 bump，lib 版本 0.21.27→0.21.28

v0.0.6
2026100715344101
1.房间模态框布局改为子网式两列 form-row（必填的房间名称+房间类型首行，所属组织+供电功率次行，行距由 0 修正为标准 1rem），动态列表区与描述框不变，前端资源版本同步 bump，lib 版本 0.21.28→0.21.29

v0.0.7
2026100715401101
1.新增 power-inline 功率组合变体（与 label 同行占满剩余宽度），房间模态框供电功率行改用，修复两列布局下功率组合换行问题，前端资源版本同步 bump，lib 版本 0.21.29→0.21.30

v0.0.8
2026100721234901
1.房间模态框所属组织调回第一项（组织+类型首行，名称+功率次行）；子网配置与工位列表参照机柜机位列头新增标题行（.dyn-columns-header 通用样式，新增 i18n 键 network.subnet），三处列表清空逻辑改为仅删条目保留列头，前端资源版本同步 bump，lib 版本 0.21.30→0.21.31

v0.0.9
2026100721362001
1.机柜模态框布局复用子网/房间样式：四字段两列 form-row（房间+名称首行，容量+功率次行），供电功率启用 power-inline 同行布局，配线架列表新增列头（复用 cabinet.patch_panel_name），机位列头与列表逻辑不变，前端资源版本同步 bump，lib 版本 0.21.31→0.21.32

v0.0.10
2026100721513601
1.设备模态框布局复用同套样式：六列位置行拆为三行两列（组织+房间类型、房间+工位、机柜+机位），序列号/主机名两列化，模板名称改为独占整行（显隐 id 上移至行，JS 不变），功率启用 power-inline；线路模态框核验已是两列样式无需改动；前端资源版本同步 bump，lib 版本 0.21.32→0.21.33

v0.0.11
2026100722032801
1.拉取MAC模态框两个下拉框改为两列 form-row 并去除冗余 form-control 类，与家族样式一致（JS 仅按 id 取元素无结构依赖），前端资源版本同步 bump，lib 版本 0.21.33→0.21.34

v0.0.12
2026100722080501
1.ip.select_device/ip.select_network 中英文案去除 -- 装饰符（同时用作行标签与下拉占位，与其它模态框占位文案风格统一），前端资源版本同步 bump，lib 版本 0.21.34→0.21.35

v0.0.13
2026100800503001
1.系统区块表单家族化统一：配置卡片字段行 config-row/config-field 全量换为 form-row/form-group 家族类（系统信息/关于/会话设置/LDAP/SSO/SMTP/MAC 通知/SNMP Trap/Agent 信息卡），label 与控件同行定宽居中与模态框内一致，只读 dl 字段值框对齐控件壳 token，收件人清单与 v3 用户表换 form-group-block 整行块；同行表单变体规则自 modals.css 上移 forms.css 双作用域共享，删除 system-config.css 冗余控件样式，移动端堆叠规则纳入卡片作用域；网络区域/用户管理模态框已在此前提交家族化无需改动；前端资源版本同步 bump，lib 版本 0.21.35→0.21.36

v0.0.14
2026100920021801
1.完善版本号关联规则：明确本文件 z 段变化（普通修改）一律不触碰 Cargo.toml，仅 y 段变化触发 c+1、x 段变化触发 b+1（c 归零），目的为减缓智能体高频次修改导致的版本号膨胀；本次为纯文档规则澄清，Cargo.toml 保持 0.21.36 不变

v0.0.15
2026100920135101
1.系统-通知配置-SNMP Trap 接收卡片监听地址拆分为监听地址+端口两个输入框（加载时从 host:port 拆分回填、保存时校验后组合写盘），新增 snmp_trap.bind_port/bind_port_placeholder/bind_port_desc 三组 i18n 键并同步中英文，invalid_bind_addr 文案更新，后端 bind_addr 存储格式与 API 契约不变，前端资源版本同步 bump，Cargo.toml 不变

v0.0.16
2026101010502501
1.修复设备可视化-设备模态框（拓扑详情浮窗）页签切换异常：端口/MAC表/LLDP 三个页签由「显示/隐藏叠加」改为互斥页签切换（点击仅显示对应面板、重复点击当前页签无操作），页签文案去除"显示"前缀与页签语义对齐；面板数据加载补充打开代次+设备一致性守卫，防止快速切换设备时旧设备数据写入新浮窗，前端资源版本同步 bump，Cargo.toml 不变

v0.1.0
2026101014000001
1.新增主机监控模块（FOIMS Agent 采集闭环）：foims-common 上报协议类型与 [agent] 配置段扩展；foims-agent-service 新增 QUIC mTLS 上报接收（UDP 9100）、入库（agents 快照 + agent_metrics_history 曲线）、离线判定与历史清理调度、agent 证书自动签发、查询/管理 API；foims-agent 新增 h3 上报循环（--once 联调、断网缓存补报、指数退避）；前端新增「主机监控」页（列表/筛选/30s 自动刷新/详情弹窗 2×2 迷你曲线）；README/man/config.toml.example 文档同步；build-agent.sh 注入交叉 C 编译器（ring 交叉编译需 cc-rs 小写 CC_<target>）；下载地址缺端口自动补 9100；lib 版本 0.21.36→0.22.0（模块级功能 x+1），前端资源版本同步 bump

v0.1.1
2026101020225101
1.修复 agent 证书物料复用缺少 CA 链校验：cert.rs 新签时向物料目录写入 agent_ca_fingerprint 指纹文件（站点 CA 证书 DER 的 SHA-256 十六进制小写），复用条件收紧为四物料可读+指纹一致+证书含 PEM 起始头，任一不满足自动重签并更新指纹文件（拒绝复用时记 warn 日志，新增 i18n 键 agent.certs_reuse_rejected 中英文），防止站点 CA 轮换后旧 agent 证书被复用导致 mTLS 握手失败；新增 ensure_agent_certs_in 可测核心与 5 个单测（指纹匹配复用/不匹配重签/指纹缺失重签/指纹格式/比对规则）
2.Agent 监控链路代码审计修复（服务端侧）：上报入库输入校验加固（文本字段 trim+长度上限 machine_id/hostname/os/kernel/arch/agent_version，温度读数 [-100,250]℃、其他传感器 ±9999.9 防 NUMERIC(5,1) 溢出 500）；新增上报频控（last_seen 距今小于生效间隔一半返回 429，i18n 键 server.agent.report_too_frequent）；指标历史 INSERT 改 ON CONFLICT (agent_id, collected_at) DO NOTHING 幂等去重；历史曲线抽样下推 SQL 窗口函数（超 200 点不再全量拉取内存抽样，保留内存二次抽样防御）；server_addr 校验拒绝尾冒号（修复产出 host::9100 非法形式）；install.sh 内置兜底副本 systemd 分支 enable+restart 对齐部署脚本；agent_metrics_history 索引改 UNIQUE（真实库已执行 DROP+CREATE UNIQUE，foims-init 建表与 check.rs 清单同步且校验 indisunique）
3.Agent 监控链路代码审计修复（agent 端与前端）：collect_report 拆同步纯函数经 tokio::task::spawn_blocking 执行（CPU 200ms 差分窗口与 /proc 同步读取不再阻塞 tokio worker，--once 与循环路径共用自动覆盖）；前端详情历史曲线加载失败 showToast 提示并清空渲染；迷你曲线单点序列补 circle 圆点渲染（原 length>1 才画 polyline 导致单点空白）；deploy/agent/install.sh systemd 分支 enable+restart、sysvinit 分支 restart 语义（升级安装后旧进程不再残留）
以上均为 z 段普通修复，前端资源版本同步 bump（新增 i18n 键触发），Cargo.toml 不变

v0.1.2
2026101111503001
1.系统页「Agent 采集」子标签更名「数据采集」（i18n 键 system.agent_collect 中英文同步，英文 Data Collection），SNMP Trap 接收卡片自「通知」子标签迁入该子标签（HTML 结构原样移动，Agent 分发卡片图标改 📦 避免与 SNMP 📡 重复）；子标签内卡片标题保留原文案，拆独立键 system.agent_collect_card（中文「Agent 采集」/英文 Agent Collection）；子标签激活分支 system-notification 移除 loadSnmpTrapConfig、agent-collect 补挂 loadSnmpTrapConfig（表单 submit 与 v3 用户行按钮绑定在 initSystemTabs 通用区，不受迁移影响），前端资源版本同步 bump，Cargo.toml 不变

v0.2.0
2026101021201501
1.新增 SNMP 设备自动纳入主机监控（一期）：agents 表加 source（agent|snmp）与 device_id 外键（设备删除级联清理），token_hash 放宽可空（真实库已执行 ALTER，foims-init 建表与 check.rs 清单同步，新增 idx_agents_device_id 部分唯一索引）；foims-agent-service 新增 snmp_poll 模块与 agent_snmp_poll 调度任务（每 5 分钟轮询已配置 SNMP 凭据且有管理地址的设备，GET sysName/sysDescr/sysUpTime → 以 machine_id='snmp:{device_id}'、source='snmp' upsert agents 行，成功刷新 last_seen 并翻转 offline→active，连续两轮失败置 offline；agent_offline 任务限定 source='agent' 不受 SNMP 轮询间隔影响）；/api/agents 列表与详情返回 source；前端主机列表 SNMP 行显示来源徽标并隐藏禁用/启用/吊销（无令牌管理语义），新增 i18n 键 agents.source_snmp 与任务文案中英文；一期仅 MIB-II 系统组，流量曲线/更多指标留二期，lib 版本 0.22.0→0.22.1，前端资源版本同步 bump
2.联调修正：snmp_poll 的 SNMP 客户端关闭内建重试（async-snmp 默认 Retry 为 3 次 × 5s 超时 + 1s 退避，单个 OID 卡 23s、三个 OID 共 69s，设备不可达时会挤占下一轮 5 分钟调度窗口），改 Retry::none() 单次超时；e2e 联调定位轮询超时根因为测试数据团体字与 snmpd 配置不符（strace 证实请求正常发出、snmpd 对错误 community 静默丢弃导致超时），非代码缺陷

v0.3.0
2026101022450101
1.SNMP 设备纳入主机监控二期（性能指标与流量曲线）：foims-agent-service 新增 snmp_metrics 模块（CPU 用 UCD ssCpuRaw* 11 个 Counter32 计数器差值算使用率——net-snmp 5.9 已移除 ssCpu 百分比标量；内存/磁盘走 hrStorageTable（Ram 行 + "Available memory" 行/FixedDisk 行），内存两行不全回落 UCD memTotalReal/memAvailReal，swap 用 memTotalSwap/memAvailSwap；温度走 LM-SENSORS-MIB lmTempSensorsTable 毫度归一并按 [-100,250]℃ 剔除；流量 walk ifTable+ifXTable（HC 64 位计数器优先回落 32 位 + 回绕校正 + 按 ifSpeed 合理性校验）差值算速率，ifName 优先命名、过滤 up 非环回、按名排序 cap 24；负载 GET laLoad.1/2/3）；计数器差值状态存 agents.raw_metrics 私有键 _snmp_state（时间戳+CPU 计数器+网卡收发字节，采集后整体写回，首轮/结构不合法降级无差值）；snmp_poll upsert 扩展 4 个热列（cpu_usage/mem_usage_pct/disk_usage_pct/max_temp）COALESCE 仅新值覆盖、raw_metrics 整体写回，RETURNING id 后向 agent_metrics_history 追加快照 {cpu,mem,disk,temp,rx_bps,tx_bps}（collected_at=采集时刻，冲突幂等跳过）；ingest 上报路径快照同步加 rx_bps/tx_bps（nets 求和），get_agent_history 返回补两键；纯函数（Counter32 回绕差值/CPU 使用率/速率合理性/温度归一/聚合求和/状态反序列化降级/上报 JSON 合成）7 组单测覆盖
2.前端主机详情弹窗曲线区新增第 5 张「网络」卡：agents.js renderSparkline 重构为 renderSparklineSeries 多序列（每序列独立配色 polyline+单点圆点，max/min 由 formatValue 人性化），网络卡 rx/tx 双线（formatBps 标注）+ 标题行 rx/tx 图例；agents.css 新增 agent-chart-line-rx/-tx 配色与图例样式；i18n 新增 agents.chart_net/chart_net_rx/chart_net_tx 中英文（英文 RX/TX）；net.errors 未上报渲染为 0、磁盘 util/IOPS 缺失渲染 "-" 均复用现有 null 容忍逻辑，详情弹窗结构除新增卡外无改动
3.部署注意：Linux snmpd 默认 view 仅放行 systemonly 子树，需在 /etc/snmp/snmpd.conf 的 view systemonly included 补 .1.3.6.1.2.1.2（ifTable）、.1.3.6.1.2.1.31（ifXTable）、.1.3.6.1.2.1.25.2（hrStorage）、.1.3.6.1.4.1.2021（UCD）与 LM-SENSORS-MIB 子树后重启 snmpd，否则 CPU/内存/磁盘/温度/流量全部采不到；net-snmp 5.9 无 ssCpu 百分比标量属正常（本实现用计数器差值不受影响）；lib 版本 0.22.1→0.22.2（y+1 → c+1），foims-agent-service 0.1.2→0.1.3，前端资源版本同步 bump

v0.3.1
2026101023085901
1.设备网口同名语义澄清与新增网口默认名修复：核实不同设备网口可同名（device_interfaces/device_nics 均为 UNIQUE(device_id, name)，建单口/整体同步/SNMP 同步三条写入路径全为 per-device 判重，跨设备同名 eth0 经 API 与 UI 端到端验证通过，无全局唯一限制）；真正缺陷在设备模态框「添加网口」默认名固定 eth0——设备创建即自带 eth0，不改名直接保存必触发同设备 409，报错文案「该接口名已存在」易被误读为跨设备全局唯一。修复：networkCardManager 新增 nextDefaultPortName()（扫描表单内全部网口名输入框取首个空闲 ethN），新增网口默认名自动避开表单内已有名称；i18n 键 server.device.interface.name_exists 中英文改为「该设备下已存在同名网口 / A port with this name already exists on this device」明确作用域；前端资源版本同步 bump，Cargo.toml 不变
