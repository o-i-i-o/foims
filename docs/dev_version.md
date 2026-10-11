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

v0.4.0
2026101100495801
1.SNMP 采集代码归位（数据采集 crate 重定位）：snmp trap 接收自 foims-resource/device 迁至 foims-agent-service/trap（路由挂载与 module_docs 同步，resource 内部 interface/mac/lldp 对 parse_auth_protocol/parse_priv_protocol 的反向依赖保留原位并改 pub 跨 crate 复用），SNMP 采集与 agent 上报统一收口「数据采集」crate
2.agent 版本独立自管理：FOIMS_AGENT_VERSION 不再绑定主程序版本（VERSION = env!("CARGO_PKG_VERSION") 读 foims-agent 自身版本），版本门控保留（manifest.version 语义为「产物对齐的服务端版本」，gate_check/build-deb.sh 不动）
3.系统页「数据采集」新增 Agent 服务端配置持久化：agent.enabled/bind_addr/report_interval_secs/download_server_addr 存系统配置（新键 download_server_addr），前端表单校验（bind host:port 正则、interval clamp [10,3600]、下载地址可选）+ sessionStorage 重启提示，下载地址缺端口安装链路 ensure_agent_port 自动补 9100
4.Agent 分发下载列表改平台×架构两级联动下拉（架构选项随平台动态过滤，包体大小提示、门控禁用状态跨重入保持），替换原全量表格

5.新增证书续期协议（POST /agent/v1/renew，docs/agent-design.md §3.4）：服务端 mTLS-only 鉴权 + 请求体 PEM 与对端 leaf DER 逐字节一致 + 剩余寿命 <90 天才签发（renew.rs，x509-parser 0.18 解析，foims-common 新增 x509::cert_remaining 共享辅助与 CertRenewRequest/CertRenewResponse 类型），issue_client_material 重签后原子替换磁盘 CLIENT_CERT/CLIENT_KEY（QUIC 监听端只锚定 CA 无需重启）；agent 端每轮上报成功后检查剩余 <30 天自动续期（reporter.rs 抽出 h3_exchange 共用收发链路，report_uri 泛化 request_uri
，续期产物临时文件+rename 原子落盘 证书 0644/私钥 0600，下一轮上报重读新证书生效）；证书已过期（握手即失败）无法自救需人工重签记入文档；lib 版本 0.22.2→0.23.0（x+1 → b+1 c 归零），foims-common 0.3.2→0.3.3、foims-agent 0.1.3→0.1.4、foims-agent-service 0.1.3→0.1.4，前端资源版本同步 bump

v0.5.0
2026101101482601
1.主机监控新增资源告警阈值（边沿触发站内通知）：新表 agent_alert_states（agent_id+metric 主键、alerting 状态位、agents 删除级联，真实库已执行建表，foims-init 建表与 check.rs 表/列清单同步）；foims-agent-service 新增 alerts 模块——全局阈值存 system_configs（config_type='agent' 新键 alert_thresholds：enabled + cpu/mem/disk 百分比与温度阈值，None 为不监控该指标），GET/PUT /api/agents/alert-thresholds（agent_admin_guard admin+secadmin），评估点两处（ingest 入库成功后按本条快照评估 + PUT 保存后对全部 active agent 快照列评估一轮），通知策略为状态翻转边沿触发（alerting false→true 先发通知再落状态、失败待重试，恢复/读数缺失静默复位，接收人为全部启用状态管理员 admin/sysadmin/secadmin，标题/正文 i18n key 存 notifications.content 由前端按语言翻译）；前端主机监控工具栏新增「告警阈值」按钮 + 配置弹窗（modalLoader 注册 agent-alert-threshold-modal，四项阈值留空不监控 + footer 启用开关，保存即评估一轮）；i18n 中英文新增 agents.alert_* 与 server.agent.alert_thresholds_* 及 server.notification.agent_alert.*，日志键 log.agent.alert_*；lib 版本 0.23.0→0.23.1（y+1 → c+1），foims-agent-service 0.1.4→0.1.5、foims-init 0.1.5→0.1.6，前端资源版本同步 bump

v0.5.1
2026101103590701
1.前端代码审计修复（无障碍与交互反馈）：reset.css 补全局 *:focus-visible 可见焦点环（--color-primary，键盘导航可达）；modalLoader 动态模态框补焦点管理——打开时统一设置 role=dialog/aria-modal 并将焦点移入首个可聚焦元素、Tab 焦点陷阱框内首尾循环、关闭时焦点归还触发元素（层叠模态仅归还本层，WeakMap 记录触发元素）；organization 组织树展开按钮（span[role=button]）补 Enter/Space 键盘触发与 aria-expanded 状态同步；toast 容器补 aria-live=polite + role=status；eventManager 两处动态 import 懒加载失败静默补 catch 并 showToast(t("common.load_failed"))（复用现有 i18n 键，无新增）
2.前端性能反模式修正：transition:all 36 处中 16 处高频交互组件（分页按钮/表头排序图标/表头搜索钮/模态 footer 按钮/关闭钮/统计卡/仪表盘卡片与列表/tab 按钮/系统配置卡/用量页 tab 与子网按钮与 IP 块/端口方块与 tooltip）改为具体过渡属性；其余 20 处（login 10/foims_init 5/visualization 5，多为装饰性动画）保留待后续处理
3.审计项核实结论：userManager/workstation/networkCardManager 的 id 内插均为服务端生成 Uuid（固定格式无注入面），豁免不改；main.html 无静态模态框（全部经 modalLoader 动态注入且外壳自带 role=dialog），该项误报；均为 z 段普通修复，前端资源版本同步 bump，Cargo.toml 不变

v0.6.0
2026101104194501
1.全仓库三维审计（代码质量/安全/Web 规范）集中修复之后端部分，涉及全部 14 个 crate：
2.鉴权与安全加固（foims-auth）：登录/令牌路径唯一约束冲突映射 409、禁用账户分支补诱饵 bcrypt 校验消除时序侧信道、密码重置令牌改单次哈希存储、SSO issuer scheme 白名单（仅 https 与 localhost http）、LDAP 配置保存对 ldap:// 明文告警；revoked_tokens 库级去重并补 UNIQUE 索引
3.资源与业务 crate（foims-resource/foims-organization/foims-x509-management/foims-data-management/foims-visualization）：SNMP 端口同步循环补 req.validate()（非法 vlan 跳过并记日志）、pull_ip_details 空态改 ok_json、网卡/接口 INSERT 唯一约束映射 409、SMTP 失败区分未配置与真实故障；模板删除加 FOR UPDATE 消除 TOCTOU、组织删除递归 CTE 等价简化、CA 导入失败清理孤儿物料目录、nginx conf 落盘继承目标权限（兜底 0644）、备份写失败清理半截文件、拓扑坐标分配改单条 INSERT...SELECT 原子化
4.采集链路（foims-agent/foims-agent-service/foims-scheduler）：--listen 服务加并发上限 32 与连接读写 10s 超时、采集器 Box→Arc 并加 30s 超时+600s 卡死冷却（中毒锁恢复）、textfile 指标名/标签键白名单、--listen 模式死代码清理；上报响应体 1MiB 上限、CPU guest 双计修复、server_addr 裸 IPv6 拒绝、续期物料落盘前校验（证书可解析+私钥 PEM 头白名单）、组包失败回删 pending 行、trap community 白名单/收件人补 sysadmin/冷却表容量上限、续期全局互斥锁、manifest 加载与二进制校验移 spawn_blocking、上报入库原子频控闸门、任务认领原子化（条件 UPDATE + advisory lock 短事务化）与运行标记 RAII 清理
5.初始化模块（foims-init/foims-common）：check_init_status 以数据探测为准（init_enabled 改附加字段 init_in_progress）、SQL_FILE_MAX 对齐 50MB、视图 GRANT 改 CURRENT_USER、host 输入字符白名单（a-zA-Z0-9.:-[]）、DROP DATABASE 加 WITH (FORCE)、REVOKE PUBLIC 失败改致命、备份响应不再回传绝对路径、重启失败恢复一次性许可、会话 SET replication_role 失败降级继续清库、localhost 守卫补 Host 头校验（防 DNS rebinding，h2c 走 uri.host() 兜底）、clear_database 验证码字段统一 verification
6.动态 SQL 治理：全仓库 AssertSqlSafe 56 处清零迁移 sqlx::QueryBuilder（值 push_bind/排序与标识符白名单/IN 列表 separated），语义等价，涉及主 crate 与 auth/resource/init/agent-service/data-management/common 等 8 crate
7.日志与 i18n：新增/修正日志键双语注册（log.agent.package_rollback/renew_conflict、log.task.next_run_sync_skipped/snmp_poll_permit_failed、log.ldap.plaintext_url、log.device.snmp_port_invalid、log.certificate.conf_tmp_chmod_failed/import_ca_cleanup_failed、log.backup.backup_file_remove_failed、log.init.restart_arm_restored、log.init.db.replication_role_set_failed）
8.明确不修项（评估后保留）：2FA 启用无密码确认（需前端改版后续立项）、Cookie Secure 依赖反代 X-Forwarded-Proto、fail2ban 进程内存态（重启重置为可接受语义）、agent --metrics 默认 127.0.0.1、SNMP 测试端点限流、sessionManager localStorage PII 取舍
9.版本联动：lib 0.23.1→0.23.2，foims-auth 0.1.2→0.1.3、foims-resource 0.1.5→0.1.6、foims-agent 0.1.4→0.1.5、foims-agent-service 0.1.5→0.1.6、foims-scheduler 0.1.1→0.1.2、foims-init 0.1.6→0.1.7、foims-common 0.3.3→0.3.4、foims-models 0.2.7→0.2.8、foims-organization 0.1.1→0.1.2、foims-x509-management 0.2.2→0.2.3、foims-data-management 0.2.3→0.2.4、foims-visualization 0.2.4→0.2.5；前端资源版本已于 v0.5.1 同步 bump，本次后端修复未触碰前端静态文件

v0.6.1
2026101108342501
1.发布检查脚本 test-scripts/release_check.sh 按 CI 工作流（.github/workflows/ci.yml）重写：前端段对齐 frontend job（lint 依赖/npm ci、JS 模块语法、i18n JSON、ESLint、Stylelint、HTMLHint 双轮、Prettier、Jest、Depcheck，Node 缺失时明确报错并跳过该段），后端段对齐 build job（cargo fmt --check、clippy --release -D warnings、test --release、build --release），末段对齐 build-deb 步骤（bash scripts/build-deb.sh，AGENT_TARGETS 环境变量透传）；检查输出落临时日志、失败时展示末尾 30 行便于诊断，退出即清理
2.cargo test 已知环境性失败降级为警告不阻断：/etc/foims/encryption.key 为 root 0600 时非 root 用户下 crypto::tests 固定权限拒绝（CI 无此文件与代码无关），判定标准=全部 FAILED 测试均属 crypto::tests 且输出含 Permission denied
3.顺带修复脚本首跑暴露的前端阻断项（CI 同样拦截）：organization.js 组织树展开按钮 aria-expanded 嵌套三元提取独立变量（sonarjs/no-nested-conditional）、systemManager.js /api/system/config 字面量提取 SYSTEM_CONFIG_API 常量（sonarjs/no-duplicate-string，6 处）、agents.js 与 systemManager.js prettier --write 格式修复；纯开发工具脚本与前端 lint 修复，Cargo.toml 不变，前端资源版本因 JS 变更同步 bump

v0.6.2
2026101108544001
1.设备网卡配置空 cards 硬编码默认网口设计缺陷修复（v0.3.1 审计遗留）：apply_network_config 删除「cards 为空自动生成默认网卡1+eth0」特例分支与 default_card_sync_item/DEFAULT_CARD_NAME/DEFAULT_PORT_NAME 常量，空 cards 改为 validate_cards_not_empty 校验拒绝（新文案键 server.device.nic.cards_required，中英文），网卡配置成为必填项、统一走正常网卡→网口校验链路；设备创建链路 req.cards.unwrap_or_default() 改为 as_deref().unwrap_or(&[])（空/未提供即 422 校验错误），更新链路 None 不动网口语义保持不变、Some(空) 由同一校验拒绝；前端设备模态框 collectData 恒提交非空 cards（至少 1 卡 1 口）不受影响。删除 nic.rs 默认配置测试并新增 test_validate_cards_not_empty；i18n 语言包变更前端资源版本同步 bump，Cargo.toml 不变
