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

v0.0.1
202610152341001501
1. 将原agent代码迁移到foims-agent目录下成为初始版本
2. 新增windows版本的agent代码，支持x86_64和aarch64架构

v0.0.2
2026101523493401
1. 清理agent工作空间HTTP/3上报可行性demo残留（examples/h3_demo.rs及专用dev-dependencies），主仓侧同步修正report_server.rs中h3_demo注释引用

v0.1.0
2026101601004501
1. Agent上报架构改造（模块级x+1，采集/上报解耦+批量上报）：服务端ingest批量兼容——请求体批量优先单帧回退、一次鉴权逐帧校验入库（非法帧丢弃计数）、主机快照取批次最后一帧、collected_at校验放宽（未来≤300s/过去≤24h，修复长断网补报必400）、上报路径body上限提升至9MiB；新增log.agent.ingest_batch/ingest_invalid_frames观测日志（中英i18n）
2. 配置面：AgentConfig新增collect_interval_secs采集间隔（1-60s默认5，serde缺省兼容旧配置），系统配置保存校验、/api/agents/download生成的agent.toml模板同步携带，前端Agent服务配置卡新增采集间隔表单项（中英i18n）
3. 联动：根Cargo.toml 0.23.6→0.24.0，foims-common 0.3.6→0.3.7，foims-agent-service 0.1.9→0.1.10；前端资源版本经bump_static_version.sh刷新；agent工作空间同步v0.3.0（AgentReportBatch协议双副本+批量上报+连接复用）

v0.2.0
2026101608304901
1. Agent分发模型重构（模块级x+1）：agent工作空间build-agent.sh新增OS系统包组装（Linux每目标deb+rpmbuild rpm、Windows zip，产物归集dist/packages/含SHA256SUMS；systemd unit与Windows说明为deploy/agent包内物料唯一来源，rpmbuild经_dbpath临时库免系统rpmdb），ci.yml改为独立构建Agent系统包、release任务改直传build任务artifact（tar.gz打包移入build任务）；服务端彻底移除动态组二进制链路——删/api/agents/dist与/api/agents/download路由、manifest模块、config.agent.dist_dir配置、build-deb.sh级联编译装包段、deploy/agent/install.sh死文件与主仓dist/agents旧产物
2. 共享注册令牌：config.agent新增enrollment_token（64位hex，启动为空自动生成并写回config.toml），ingest鉴权改SHA-256哈希比对配置令牌（agents表token_hash列删除）；新增GET/POST /api/agents/enrollment-token（查看/重新生成，写盘走config_write_lock+磁盘基底）；系统配置三处响应对令牌按***掩码（更新/恢复空值或掩码还原磁盘值）
3. 注册流程改首报自动注册（INSERT..ON CONFLICT(machine_id) DO UPDATE，行初始active、label取hostname），删除激活/吊销流程，状态白名单active|offline|disabled；配置包（deb=all/rpm=noarch/zip平铺）统一落/etc/foims-agent/，与程序包解耦（程序包postinst检测配置就位即自动拉起，两包先后安装均可闭环）
4. 数据库：foims-init建表与check.rs同步（agents删token_hash、status默认active），upgrade/0.25/upgrade_0.25.0.sql（revoked迁disabled、清pending、DROP token_hash；本机已执行）
5. 前端：主机监控删pending/revoked状态徽标与吊销按钮；系统设置Agent卡改为注册令牌查看/重生成 + OS×格式配置包下载（删平台/架构下拉与label输入）
6. 联动：根Cargo.toml 0.24.0→0.25.0，foims-common 0.3.7→0.3.8，foims-agent-service 0.1.10→0.1.11；agent工作空间0.3.0→0.4.0；前端资源版本经bump_static_version.sh刷新

v0.2.1
2026101609211601
1. 分发重构独立审计修复（仅z）：补前端i18n缺键common.help（zh/en，帮助系统标题fallback引用，frontend_consistency测试曾红灯）；补后端日志键log.agent.config_package_downloaded（zh/en，此前下载配置包日志输出原始键名）；主机监控agents.js删除「吊销」注释残留与悬空agent-dot-pending兜底类（CSS无定义且三合法状态全被STATUS_META覆盖，不可达死分支）；清理仓库根误生成的foims.en.log调试残留；前端资源版本经bump_static_version.sh刷新

v0.3.0
2026101609274401
1. 前端帮助体系（小功能y+1）：页面不再保留常驻帮助文字——系统页16张配置卡片与13个业务弹窗标题旁新增「?」帮助按钮（help-btn），点击弹出统一帮助弹窗（help-modal）；静态卡片按钮由新增 utils/helpManager.js 在应用初始化时注入图标并全站事件委托，模态按钮由 modalLoader 按 MODAL_REGISTRY 新增 help/helpTitle 字段经 buildModalShell 注入；帮助图标 help（Feather help-circle）入 icons.js 并新增 helpButton() 生成器；正文渲染按换行分段、「- 」行合并为列表（textContent 写入防注入），languagechange 时打开中的弹窗按新语言重绘
2. 提示迁移与补齐：删除 main.html 各卡片常驻提示共17处（hint-text/config-hint/import-hint/export-hint/clear-logs-hint 与证书卡 CA 信任教程 details 整块、fail2ban 卡 JS 渲染的日志路径提示）及12个弹窗模板13处提示（工位/组织拓扑容器与连线/Agent告警阈值/线缆标签打印/定时任务/证书与CA生成导入/修改密码/员工管理）；被手动清理的提示全部并入对应帮助文案补齐（含公共URL上报地址推导、Agent离线判定3倍间隔、CSV导入顺序与回滚、CA信任教程五段等）；输入框 placeholder、title/data-i18n-title 悬停提示、登录页与init向导流程引导、cron预览等动态功能性提示按策略保留
3. 帮助文案：i18n 新增 help 命名空间29键（help.card.*×16 + help.modal.*×13，zh/en 双语，正文含被迁移原提示全文）
4. 清理：删除迁移后无引用的 i18n 键52个（zh/en 同步，含 cert.ca_help_* 六键、data-management 导入说明/库导出说明、agent_service/sso/ldap/snmp_trap 各 *_desc、agents.alert_hint、workstation.position_hint 等）与无引用孤儿键 user.role_readonly_hint；CSS 删除失用规则 .config-hint/.cert-ca-help*/.license-note/.import-hint/.export-hint/.clear-logs-hint（.hint-text 保留给动态功能性提示）；fail2banManager.js 删除 log-path-hint 引用与渲染块
5. 联动：根Cargo.toml 0.25.0→0.25.1；前端资源版本经bump_static_version.sh刷新

v0.3.1
2026101619381001
1. CI/CD拆分优化（仅z）：ci.yml的build任务拆为主程序build与agent-build两个并行独立任务（主程序fmt/clippy/test/DEB/tar.gz；agent交叉编译zig+zigbuild+deb/rpm/zip，各自独立系统依赖与rust-cache缓存域），产物经两个artifact（foims-release/agent-release）汇入同一release任务统一上传发布页；agent任务去掉无需的主程序依赖（libpq/lintian等），主程序任务去掉rpm/zig交叉依赖；各目标rust-std由build-agent.sh内部rustup target add，agent任务工具链最小化

v0.3.2
2026101619480601
1.主程序打包格式补齐（仅z）：scripts/build-deb.sh更名build-pkgs.sh并扩展为多格式打包——deb（原有）+ rpm（rpmbuild复用deb staging打Source tar，_dbpath用户态免root，%pre运行环境强检/%post建用户证书自签nginx对齐/%preun停启用/%postun对齐purge，与deb维护脚本逐一同语义，%config(noreplace)保护本地配置）+ zip（裸二进制，替代原CI内联tar.gz）；agent侧Windows目标补裸exe产物（与zip同二进制）纳入SHA256SUMS，agent工作空间0.4.0→0.4.1；ci.yml主程序任务改跑build-pkgs.sh（artifact与release上传glob同步rpm/zip），release_check.sh恢复Agent构建检查并对齐新脚本名；同步更新foims.service与foims-services两处注释引用


v0.3.3
2026101620295001
1. 发布产物修正（仅z）：修复release任务主程序产物从未上传发布页的缺陷——upload-artifact v4多path以公共祖先为根剥离目录（target/不进artifact，artifact内平铺），而files按release-assets/target/子路径glob匹配落空且静默跳过；改为两artifact分别落release-assets/main与release-assets/agent、files改目录级通配（agent包SHA256SUMS随发布页一同上传）；agent侧Windows产物删除zip包装形态只留裸exe（README-windows.txt保留于仓库deploy/agent/作部署说明，组包工具链不再依赖zip），agent工作空间0.4.1→0.4.2；release_check.sh与ci.yml注释同步对齐
2. 修复agent多架构rpm静默丢失（产物清点时暴露的既有缺陷）：rpmbuild spec显式BuildArch在rpm 6起「可构建架构」收窄到宿主同族（compatible build archs仅x86_64系）后对异族架构报No compatible architectures found for build，aarch64/i686/armhfp/armv7hl四个rpm从未产出且失败被函数末尾rm -rf兜底掩盖为返回0（if条件上下文禁用set -e）；修复为spec删BuildArch行（包架构由--target决定，实测包元数据Architecture正确）+ rpmbuild/mv失败显式return 1；全量重跑验证5架构rpm全部产出，主仓build-pkgs.sh的spec本就无BuildArch行不受影响
3. agent构建矩阵删除arm-unknown-linux-musleabihf（ARMv6）目标：deb_arch_of曾将它与armv7目标折叠映射到同一armhf架构名致deb互相覆盖，且deb/rpm生态无armv6形态（Debian armhf与RHEL armhfp均为armv7语义），armv6包无正确分发渠道；矩阵收缩为4 Linux+1 Windows（agent侧dev_version v0.4.2第2次修改同步记录）

v0.4.0
2026101621443601
1. 修复可视化画布偶发整页冻结（功能bug，y+1）：svgCanvasBase._renderGridRuler 网格绘制循环以 gx += step 累积推进且无条数上限——节点坐标异常（保存坐标仅拒绝负值无上限）经 fitView 计算出 1e8 级 viewBox 后按 step=100 逐条创建 SVG 节点可达百万级，主线程冻结至 F5；且累积量超过 2^53 浮点精度时 += step 不再前进形成真死循环。修复为按索引有界循环（每轴条数封顶500，合法最大视野约300条不受影响）并前置非有限 viewBox 防护；机柜视图（SVGCore）与拓扑视图（TopologyCore）共用基类同步受益；前端资源版本经bump_static_version.sh刷新，根Cargo.toml 0.25.1→0.25.2

v0.3.4
2026101622450501
1. Agent配置包安装闭环修正（仅z，crates/foims-agent-service/packaging.rs）：deb侧Depends: foims-agent本已存在（实为补充确证），rpm侧补Requires: foims-agent（rpm::Dependency::any，任意版本约束——配置包Version取主程序版本而程序包版本由agent工作空间独立管理，两版本空间不同不可锁版本）；RESTART_SNIPPET由try-restart改为enable+restart foims-agent——try-restart对未运行服务是no-op，依赖强制安装顺序（程序包在前）后配置包装完时agent必然未启动，旧脚本导致首次安装服务永不拉起，改为enable（设自启）+restart（重启一次使新配置即刻生效，首次安装直接拉起）；zip包README.txt补Windows服务重启说明（sc.exe stop/start）；模块头与agent工作空间build-agent.sh分发模型注释同步；测试同步：deb control断言Depends、postinst断言enable+restart且显式拒绝try-restart回潮、rpm回读断言requires含foims-agent；cargo test -p foims-agent-service 57全绿、clippy -D warnings与fmt通过

v0.3.5
2026101623060801
1. Agent配置包zip形态补Linux卸载脚本 + agent程序包Windows服务模式配套（仅z，主仓侧只动foims-agent-service打包与注释）：packaging.rs新增UNINSTALL_SH（uninstall.sh随zip包下发，0755）——配置经deb/rpm安装（foims-agent-config）时拒绝执行提示用包管理器卸载（防双轨残留），手动解压路径则stop+disable服务并删除/etc/foims-agent/，程序本体始终提示用系统包管理器卸载；ZIP_README重写为linux+Windows双平台安装/卸载说明（Linux复制到/etc/foims-agent/+systemctl restart、Windows经程序包install.cmd注册服务）；zip测试条目5→6（uninstall.sh权限0o755）；api.rs模块头/格式校验注释「exe」→「zip」双平台同步；agent工作空间侧为SCM服务模式改造（--service + install.cmd/uninstall.cmd + Windows产物回归zip），agent 0.4.2→0.5.0独立版本记录；cargo test -p foims-agent-service全绿、clippy -D warnings与fmt通过

v0.4.0
2026101709380901
1. SNMP端口二层属性采集实现（小功能y+1）：get_device_ports_via_snmp由仅walk ifDescr扩展为完整采集——ifAdminStatus/ifOperStatus推导status（admin-down优先、oper 2..=6归down）、ifHighSpeed优先ifSpeed兜底格式化speed（整千Gbps/其余Mbps）、dot1dBasePortIfIndex+dot1qPvid采集access VLAN、dot1qVlanStaticEgressPorts位图展开按VLAN成员数推导port_type（>1 trunk、=1 access，非交换设备字段留NULL）；新增walk_index_value_map单弧索引表通用walk与log.device.snmp.ports_walk_failed日志键（zh/en）
2. 端口同步改UPDATE+INSERT双语句同事务：先以本批采集值COALESCE刷新已存在端口的port_type/status/speed/vlan_id（采集缺失保留原值，避免无VLAN表设备手动配置被默认值覆盖），再ON CONFLICT DO NOTHING插入新端口；同步消息文案改「新增/更新/跳过无效」口径（zh/en三键重写），skipped语义改为校验未通过的无效端口数
3. 前端补齐：设备MAC表加VLAN列（deviceMacLldp.js）、统一端口tooltip加VLAN行（unifiedDevicePorts.js）、拓扑详情端口表加VLAN与速率列（TopologyModal.js）
4. 死代码清理：删除无引用结构体DeviceMacCreate及其反序列化测试、仅其使用的validate_ip_address_string（后端yml本就无其引用的ip_format/interface_length键，佐证死代码）
5. 联动：根Cargo.toml 0.25.2→0.25.3，foims-models 0.2.8→0.2.9，foims-resource 0.1.6→0.1.7；前端资源版本经bump_static_version.sh刷新

v0.4.1
2026101710045801
1. 表名消歧义（普通修改z+1）：device_macs 改名 snmp_macs——该表存储设备 SNMP 观测到的 ARP/FDB 条目（IP↔MAC↔学习端口↔VLAN），原名易与端口台账 device_interfaces 的 mac_address/vlan_id 字段语义混淆；列结构与数据不变
2. 联动：foims-init 建表模块改名 snmp_macs.rs（mod/索引/updated_at触发器注册/check.rs 表与列清单同步）；mac.rs/ip.rs/topology.rs 全部 SQL 引用改表名；数据导入导出表注册名 device→snmp_macs（spec.rs/modules.rs）；模型 DeviceMac 改名 SnmpMac；新增 upgrade/0.25/upgrade_0.25.1.sql（RENAME 表+3索引，旧备份 device_macs 表导入失败属预期，观测数据可重新同步拉取）；API 路径与前端不变

v0.4.2
2026101710221801
1. 初始化模块建表文件命名对齐领域聚合惯例（纯文件组织调整，无数据库对象变化）：评估确认一表一文件方案不采纳（建表代码原按领域/模块聚合——agents/cabinets/logs/network/rooms/system/topology/users/workstations 均为领域名+同域附属表，单表文件文件名=表名）；snmp_macs.rs 改名 device_snmp.rs（文件实际承载设备 SNMP 采集域两张表：snmp_macs MAC 观测 + device_lldps LLDP 邻居，按主表命名重演歧义）；同类命名偏差一并修正：element.rs→element_layouts.rs、tokens.rs→revoked_tokens.rs（单表文件文件名=表名）；mod.rs 与头注释同步

v0.5.0
2026101712315401
1. 全库命名与语义统一批次（schema 审计修复，x+1，含破坏性 schema 变更）：①操作日志删 operation_type 冗余别名，前后端统一 action 字段；②工位管理人重构——删除 manager 手动文本列（迁移时按名字匹配 employees 回填 manager_employee_id，无法回填行删除），manager_employee_id 置必填并收紧 ON DELETE RESTRICT，前端改组织人员选择、导入导出按姓名内容匹配而非 ID；③工位改房间时同事务清理对应 workstation_layouts 行；④IP 归属校验补齐——显式子网单条与批量路径均校验 IP⊆子网（子网⊆区域已有）；⑤device_interfaces.trunk_id 改名 native_vlan_id；⑥五表改名消歧义：agents→monitored_hosts、agent_metrics_history→host_metrics_history、agent_alert_states→host_alert_states、snmp_macs→snmp_arp_entries、ips→interface_ips，索引/约束/视图/check/spec/导入导出注册名全链路联动，API 路径 /api/agents 与前端文件名不变；⑦布尔列语义化：users.status→enabled、operation_logs.result→success（与 login_logs.success 对齐），排序参数 data-sort、操作日志排序白名单与仪表盘统计 SQL 同步；⑧monitored_hosts 补 status IN (active,offline,disabled) 与 source IN (agent,snmp) CHECK 约束；⑨前端统一端口 tooltip 补显端口类型（port_type i18n 映射，未知枚举回显原值）
2. 命名规范固化进 docs/code-style.md §2.3「数据库命名规范（强制）」：表名用语义实体命名不绑定实现载体、布尔列用形容词（enabled/success）禁用 status/result 通用名词、命名歧义必须全链路统一彻底清理旧名禁止兼容别名层
3. 联动：根 Cargo.toml 0.25.3→0.26.0；新增 upgrade/0.26/upgrade_0.26.0.sql（基于 0.25.3 库：workstations 回填+删列+约束重建、trunk_id 改名、五表 RENAME+索引/约束联动、补 CHECK、users/operation_logs 列改名，单事务）；前端资源版本经 bump_static_version.sh 刷新

v0.6.0
2026101715180001
1. MAC地址拉取多设备化（IP模块小功能）：PullIpDetailsRequest 由单 device_id 改为 device_ids 数组（1..=100，去重保序），多设备按提交顺序依次执行 sync_switch_macs、后设备观测正常覆盖前设备；新增 GET /api/resources/ip/pull-defaults 查询默认拉取设备，拉取成功后设备集合持久化至 system_configs（mac_pull/default_device_ids，JSON 数组）；前端拉取模态框设备下拉改复选列表并按默认集合预选（与现存 SNMP 设备求交，读取失败不阻塞仅不预选）
2. IP 判活优化：sync_switch_macs 在观测写入后将判活范围内未被本次观测匹配的 active IP 置 inactive（指定子网取该子网、全部子网模式仅取观测触达子网避免未覆盖网段误杀；ARP 缓存为空不判活），同步结果新增 deactivated 计数并入完成消息；sync_ip_status_by_staleness 移除按陈旧 last_seen 的反向恢复逻辑（与新判活互相翻转），仅保留 active→inactive 陈旧翻转
3. 通知配置矩阵化（系统模块小功能）：「MAC地址变更通知」卡片改为「通知配置」矩阵表格（行=MAC地址变更/主机告警，列=邮件通知/Webhook 占位徽标待实现）；NotificationMethods（mac_change/agent_alert，serde 缺省全开保持存量行为）存 system_configs（notification/methods），GET/PUT /api/system/notification/settings 联动读写；MAC 变更邮件与站内通知增加来源设备注明（send_mac_change_email 加 source_device 参数），主机告警 notify_admins 接入 send_host_alert_email 邮件发送（方式开关门控，失败仅记日志不阻塞状态翻转）；前端 systemManager 读写 methods，i18n zh/en 补齐，帮助键 card.notify_mac 更名 card.notify
4. 联动：根 Cargo.toml 0.26.0→0.26.1，foims-models 0.2.9→0.2.10，foims-resource 0.1.7→0.1.8，foims-auth 0.1.3→0.1.4，foims-agent-service 0.1.11→0.1.12；前端资源版本经 bump_static_version.sh 刷新

v0.7.0
2026101716380501
1. 默认拉取配置扩展为设备+子网（IP模块小功能）：system_configs 存储由 default_device_ids JSON 数组改为 default_pull 对象 {device_ids, subnet_id}（MacPullDefaults 结构，损坏降级空默认不再报错），GET /api/resources/ip/pull-defaults 返回对象、pull_ip_details 成功路径保存本次设备集合与子网选择；拉取模态框改两列布局（左设备复选列表、右子网下拉，form-row auto-fit 窄屏自动回落单列），打开时按默认配置预选设备与子网
2. 定时任务 MAC 同步多设备化（系统模块小功能，与手动拉取默认配置不共享）：任务 config 由单 device_id 改为 device_ids 数组（1..=100 语义同手动拉取），执行器依次调用 pull_ip_details_internal、后设备观测覆盖先设备写入，任一失败即任务失败；前端任务弹窗同两列布局（设备复选列表+子网下拉，仅列 snmp_configured 设备），回填按 device_ids 勾选（已删设备自然剔除）、提交保存 device_ids+subnet_id 至任务 config
3. 死键与文案清理：删 scheduled_tasks.config_fields.select_device 与 server.ip.pull_defaults_parse_failed（均无引用）；server.task.mac_sync_missing_config 文案改 device_ids 口径，新增 server.task.mac_sync_invalid_config（配置解析失败）
4. 联动：根 Cargo.toml 0.26.1→0.26.2，foims-resource 0.1.8→0.1.9；前端资源版本经 bump_static_version.sh 刷新

v0.7.1
2026101921484101
1. 新增前端静态审计工具链（脱离 Node 环境）：JS 侧 oxlint v1.88.0（独立 musl 二进制装 ~/.local/bin，仓库新增 .oxlintrc.json：correctness+suspicious 双档 warn、browser env、排除 icons 与 target）；CSS 侧 biome v2.5.15（独立二进制装 ~/.local/bin，仓库新增 biome.json：仅扫 web/static 排除 icons/lucide SVG、关闭 a11y 组噪音、CSS 格式化对齐 docs/CSS-style.md 2 空格约定、allowWrongLineComments 兼容行内注释）；两工具均有基线扫描结果（CSS 20 处 noDescendingSpecificity/noImportantStyles，JS 0 error 323 条口味告警留待甄别）
2. 本次仅新增开发工具配置文件，未触碰任何 web/static 资源与 Rust 代码，前端 ？v= 与 Cargo.toml 均不联动

v0.7.2
2026101922071401
1. 前端审计工具链严格化：.oxlintrc.json correctness/suspicious 升为 error 并新增 perf=warn（no-await-in-loop），关闭两条与项目私有约定冲突的口味规则（no-underscore-dangle 下划线私有字段、consistent-function-scoping 模块内函数），biome.json 试开 nursery 后确认均为迁移类噪音（useLayeredStyles 1303 处 @layer 改写、useLogicalProperties 904 处逻辑属性改写）而移除，CSS 侧保持 recommended 全量真实 bug 检测
2. app.js waitForStylesheets 的 cssRules 跨域探测处补行内注释与 oxlint 行内豁免（唯一 correctness 误报点，故意读取 cssRules 触发安全错误探测加载完成）；前端 ？v= 经 bump_static_version.sh 刷新，frontend_consistency 5 测试通过
3. docs/CSS-style.md 头部新增优先级声明：文档仅作意图说明，与 biome.json 冲突一律以 biome.json 为准；标注 web/.stylelintrc.json（Node 时代）已废弃。SVG 报错根因确认为 biome 将 SVG 按 HTML 解析命中 a11y noSvgWithoutTitle（vendored lucide 图标 1700+ 文件），根治即配置排除 icons 目录，不改上游图标
4. 严格档基线：biome 49 error/169 warn/19 info（useIterableCallbackReturn 49、noThisInStatic 51、useOptionalChain 41、CSS noDescendingSpecificity 33、noImportantStyles 8 等），oxlint 51 条（prefer-add-event-listener 28、no-await-in-loop 14、no-array-sort 4、no-shadow 3、no-map-spread 2）；本条为文档与配置变更，Cargo.toml 不联动

v0.8.0
2026101923204501
1. 前端 CSS @layer 层叠层重构（架构优化，y+1）：web/static/css 全部 31 个文件纳入统一层序 variables < reset < components < modals < layouts < pages < utilities < responsive（层序声明语句位于 variables.css 顶部），层序取代跨文件特异性竞争成为级联第一决定因素；styleLoader 动态加载的 visualization.css 单独成文件顶层 @layer 块，与静态引入文件层序一致（嵌套 @layer 会生成子层，跨层规则必须置于文件顶层）；foims_init.css 因不加载 reset.css 将通配重置就地归入 reset 层，避免越层覆盖组件样式
2. 重构保真验证：静态候选 19439 对选择器交叉对，经真实 DOM 运行时 matches 扫描（登录页/init 页/主页面全 DOM/8 个 section/网络区域模态）收敛至 26 组同元素双命中，逐对按「原级联胜者=保真目标」判定；除下述修复外其余均为值等价翻转（1rem==16px 等）或被更高优先级规则掩蔽，无视觉变化
3. 层序反转修复 7 处（保现行视觉）：responsive.css 表单标签组两选择器分别改 .checkbox-label 限定与 :not(.modal *) 排除（模态内非复选标签维持 forms.css 弹窗变体居中窄版，原级联下 responsive 规则从未实际胜出）；logs.css 删 .clear-logs-form label 死代码声明与被完全压制的 select/input 规则；organization.css .btn-add-child 系列加 :not(.icon-btn) 排除图标形态（着色归 buttons.css）；tables.css 的 .data-table th .btn 与 visualization.css 的页签紧凑尺寸/缩放按钮规则移至各自文件末尾顶层 @layer responsive 块，防通用移动端规则按层序反超
4. 联动：根 Cargo.toml 0.26.2→0.26.3（crate 版本不变）；前端 ？v= 与 MODULE_VERSION 经 bump_static_version.sh 刷新，frontend_consistency 5 测试通过；biome 全量 49 error/169 warn/19 info 与基线持平，cargo fmt/clippy --release -D warnings 零告警

v0.9.0
2026102001055001
1. 前端 lint 全量清零（工程化优化收尾，y+1）：oxlint（correctness+suspicious=error、perf=warn）与 biome（recommended 全量，不关规则、不加行内豁免）error/warn/info 三档全部清零；JS 侧 28 处 prefer-add-event-listener 真实修复——一次性动态元素直接 addEventListener（styleLoader.js）、模态内元素按项目既定约定以元素级 dataset.bound 守卫绑定（authManager/organization/cableLink/ipDetail/visualizationManager/systemManager 等）、跨初始化状态外置（organization.js 目标组织写入 dataset、unifiedDevicePorts.js 扩展 conflictState 并新增模块级 currentPortDetail、TopologyVisualization.js 新增 _detailConnectionId/_containerTarget 实例字段），原「onclick 赋值幂等」注释同步改写为新机制描述；13 处 noGlobalIsNan 语义安全替换；no-await-in-loop/no-array-sort/no-shadow/no-map-spread 等其余口味项全清
2. CSS 侧 33 处 noDescendingSpecificity 以源序重排修复（10 文件）：低特异度基础规则前移、等特异度真实重叠规则保持相对序——tables.css 顶部重排为「独立表格家族 mac-group-table/lldp-table 基础块 + .data-table 家族」，.empty-row td 保持 .data-table td 之后源序胜出、列级变体留底部；visualization.css 五个 rect 基础块（cabinet/cabinet-position/topology-device/lag-badge/hop-node）集中于状态与变体覆盖组之前；sidebar.css 三条 svg 尺寸基础规则集中至状态旋转与彩色化组之前；login.css 偷看态双组合类后置；organization.css 五处（toggle-icon 交换、hover 显形后移、template 行/图标尺寸前移）；foims_init.css 状态配色与加载动画基础规则前移；system-config.css 操作列基础规则前移；forms.css .required 列表换序 + .power-input-row select 前移；network-config.css 列标题列表换序；responsive.css 基础规则先于折叠态规则；全部重排以「层内高特异度规则无论先后均压低特异度、移动仅跨越等特异度规则才影响级联」为安全依据，浏览器冒烟 7 项（登录/侧边栏/表格/可视化/组织树/系统配置/空行占位）全过
3. 8 处 noImportantStyles 删除后层序保全修复（不吃豁免、不恢复 important）：cable-label-print.css 打印遮蔽块移至文件顶层 @layer responsive 末层——body 直接子代 .sidebar/.main-content 在 layouts 层带 display 声明，留在 modals 层会被层序反超导致打印泄漏；.file-input 拖放区提特异度为 .init-form-group .file-input 与 .init-form-group input 同特异度靠源序胜出（核实 JS 无内联样式来源，原注释夸大）；.init-btn:disabled transform:none 依赖 hover 变体既有 :not(:disabled) 保护即可稳定；.hidden/.ro-hidden 依赖 utilities 层序晚于组件/页面层压制；顺带清理全部 stylelint-disable-line 死注释（stylelint 已弃用）
4. 联动：根 Cargo.toml 0.26.3→0.26.4（crate 版本不变）；前端 ？v= 与 MODULE_VERSION 经 bump_static_version.sh 刷新，frontend_consistency 5 测试通过，cargo fmt/clippy --release -D warnings 零告警

v0.10.0
2026102002515001
1. 前端 Vue 3 重构启动（模块级x+1）：仓库根新增 frontend/ 独立 Vite 工程（vue3-test 分支渐进迁移，旧 web/ 保持不动渐进共存）——Vite 8 MPA 三入口（index.html 登录 / init_index.html 初始化 / main.html 主应用，产物文件名与旧布局对齐便于 nginx 平滑切换），dev server /api 代理经本机 nginx(80) 转发后端 UDS；技术栈 Vue 3.5 + Pinia 4 + Vue Router 5 + vue-i18n 11 + TanStack Query 5（query 尚未接线，随主界面壳接入），frontend/package.json 0.1.0 独立管理
2. 登录页完成 Vue 化并 e2e 通过：LoginPage.vue 移植 login.js 全部逻辑（密码/邮箱/LDAP/SSO 四种登录方式、2FA 双端点契约——本地端点 two_factor_code 字段与 LDAP 原端点携带 code 字段重试、图形验证码失败计数联动、忘记密码模态框 SFC 化免 modalLoader、SSO 回跳错误捕获、init 模式跳转、会话探测裸 fetch 静默回退）+ 动画角色保留命令式实现（元素缓存 + 状态类/内联样式逐帧写入，眨眼/偷看/互看/失败摇头全保留）；apiClient 移植保持「永不 reject」契约（401 刷新重试、公共认证端点豁免、429 Retry-After 退避、Blob 下载归一、message_key 保留原始键），删除被 TanStack Query 取代的 GET 去重与 data-mutation 事件广播
3. i18n 迁移：vue-i18n（composition）复用旧版 zh/en JSON 原文件（迁移期单一来源），导入时归一化旧占位符语法（{{x}}→{x}）并转义 @|$ 消息格式特殊字符（现网语言包已核对花括号仅来自占位符、@ 仅 1 处、| 为 0 处）；会话工具（rememberMe 双存储互斥）与 toast 组合式函数随迁
4. e2e 验证：Vite dev 下登录页渲染（角色/表单/i18n 文案）、错误路径（错密码显示错误条）、成功登录跳转 main.html 占位页、全程零 console 错误
5. 联动：根 Cargo.toml 0.26.4→0.27.0（crate 版本不变）；旧 web/ 未动不 bump 静态版本，frontend_consistency 5 测试预期不受影响

v0.10.1
2026102005092601
1. 主界面壳完成（Vue 重构第二批）：MainShell.vue 移植 main.html 壳层（侧边栏 + 主内容 RouterView + AppToast + 改密模态）——NavIcon 以形状描述数组经 `<component :is>` 渲染（避免 v-html 绕过审计）、侧边栏收起态 body class + localStorage 持久化、语言菜单 Teleport 复刻旧定位逻辑（收起态右侧飞出/默认向上 top<8 向下）、改密模态 SFC 化（不一致校验/成功后延迟跳登录）、退出登录与无会话守卫（无会话跳回 /index.html）
2. 路由与状态：Vue Router 5 采 hash 模式（保留旧版 #section 深链兼容且零 nginx 改动），NAV_ITEMS 单一数据源同时驱动路由表与侧边栏渲染；Pinia auth store 包装 session.js（HttpOnly Cookie 为真实凭证，store 仅持用户快照，删除无调用方的 syncFromSession 死代码）；TanStack Query 5 全局接线（queries retry:false，失败已归一为 {success:false} 不抛异常）
3. RBAC 声明式迁移：roleCapabilities.js 五角色隐藏/只读分区表（admin/sysadmin/secadmin/auditor/user），路由守卫兜底重定向 + 占位页只读角标；不移植旧版 DOM 扫描 + MutationObserver 写控件隐藏机制，改由 Vue 页面声明式实现；空闲超时 usePageTimeout（等保，activity 事件重置，超时先 refreshToken 成功续期、失败跳登录）与 useTooltip 委托单例随迁；SectionPlaceholder 占位页沿用 content-section active 类保留布局层贴底约定，i18n 新增 common.section_migrating
4. 验证：浏览器 e2e 9 项全过（壳渲染/侧边栏导航/收起展开/语言切换/改密模态/退出登录+无会话守卫/RBAC 隐藏分区重定向/只读角标）；oxlint 95 文件 0 告警（i18n 支持语言列表数组改 Set 修 prefer-set-has）；biome 纳入 frontend/src/styles 后 154 文件零诊断（biome migrate：recommended→preset 迁移、目录忽略 !x 去尾 /** 修正、排除 foims-agent 断链 fixture）；frontend_consistency 5 测试通过，cargo fmt/clippy --release -D warnings 零告警（本轮 Rust 零改动）
5. 联动：仅 z+1，Cargo.toml 与 frontend/package.json 均不动；旧 web/ 未动不 bump 静态版本

v0.10.2
2026102008252401
1. 日志页完成 Vue 化（Vue 重构第三批，TanStack Query 数据层首次实战）：LogsPage.vue 移植旧版 logs 三页签全量逻辑——操作日志（分页 50/页、时间/操作类型/资源类型列排序、操作类型精确搜索防抖）、登录日志（163 条分页 + 排序）、通知（分页 20/页、已读状态列排序、单条/全部标记已读）；页签状态 route.query.tab 优先 → localStorage 记忆 → 默认操作日志，hash 深链形如 #/logs?tab=login
2. TanStack Query 接线模式固化：页内 usePagedList 工厂（queryKey 用 computed 包裹 page/sort/搜索等 reactive 依赖，enabled 按页签懒加载，queryFn 对 apiClient 归一化失败 {success:false} 显式 throw 使查询进入 error 态，watch data 实现空页越界回退最后一页）；写操作 useMutation + invalidateQueries 前缀匹配失效驱动重拉；分页/排序/搜索/已读操作全部由查询状态驱动，无手工 DOM 渲染
3. 通用组件沉淀：Pagination.vue（保真旧版分页 DOM 层级与省略号算法， totalPages>7 才渲染跳页框）、ConfirmModal.vue（danger 变体、Esc/遮罩取消）、LogDetailsModal.vue（操作日志七字段网格 + details JSON 解析格式化展示）；icons.js 新增 eye 图标；formatter.js 迁移旧版时间格式化（跟随界面语言）与 server.*/枚举/通知 content 翻译工具；entries/main.js 引入 components 层四 CSS（buttons/forms/tables/badges，与旧版逐字一致），日志页特有列宽与工具栏样式入 SFC scoped
4. 验证：浏览器 e2e 13 项全过（子标签记忆/切换+hash 深链/列翻译/分页翻页/排序切换/详情弹窗/搜索空态+命中/登录日志/通知翻译与插值/单条已读/全部已读确认弹窗取消+确认双路径）；vite build 通过；oxlint 100 文件 0 告警、biome 158 文件 0 诊断、frontend_consistency 5 测试通过
5. 联动：仅 z+1，Cargo.toml 与 frontend/package.json 均不动；旧 web/ 未动不 bump 静态版本

v0.10.3
2026102009391201
1. 仪表盘完成 Vue 化（Vue 重构第四批）：DashboardPage.vue 移植旧版 dashboard.js 全部逻辑——5 张统计卡（dashboard-stats 单端点，加载中展示 0、失败如实展示 "-"）+ 5 张榜单卡（子网/IP/最近动态/房间/机柜，各分页接口 page_size=5）+ 主机负载卡（CPU/内存/温度三页签 10s 自动轮播、悬停暂停、手动切换后永久停止轮播、onUnmounted 清理定时器）；卡片点击跳转移植 data-nav/data-tab 委托语义（命中链接/按钮不跳转，view-all 维持旧版无动作；带 tab 的卡片 router.push 生成 #/resources?tab=networks 深链）
2. 榜单查询工厂 useTopList（queryKey=[tag]、queryFn 归一化失败显式 throw、staleTime 60s 对应旧版客户端缓存 TTL）；主机负载查询 enabled 按角色懒加载（hiddenSections 含 agents 不请求，该端点仅 admin/secadmin 可访问，无权限角色整卡不渲染避免噪音 403）；旧版 loadFallbackData 三接口计数兜底按回退逻辑规范评估后删除（兜底与主端点同源依赖数据库，失败场景同样不可用，仅以 0 掩盖错误）
3. formatter.js 补齐 4 个工具函数（getStatusText/getRoomTypeName/getActionIcon/formatTime，与旧版逐字一致）；entries/main.js 引入 components 层 cards.css（与旧版逐字一致），dashboard.css 除 .main-body（Vue 壳由 main-content.css 提供底色）外全量入 SFC scoped，骨架条与 agent-usage 迷你条以 scoped 片段内联
4. 验证：浏览器 e2e 12 项全过（统计卡真实值/五张榜单渲染/骨架屏加载态/主机负载空态+页签/自动轮播/手动切换停轮播/悬停暂停/移出恢复/子网卡深链/view-all 不跳转/IP 卡深链/英文切换全量）+ RBAC 两项（注入 auditor 角色后主机负载卡不渲染 dashCards=5、恢复 admin 后恢复渲染 dashCards=6）；vite build 通过；oxlint 101 文件 0 告警、biome 158 文件 0 诊断、frontend_consistency 5 测试通过
5. 联动：仅 z+1，Cargo.toml 与 frontend/package.json 均不动；旧 web/ 未动不 bump 静态版本

v0.10.4
2026102010143401
1. 组织管理页完成 Vue 化（Vue 重构第五批）：OrganizationPage.vue 移植旧版 organization.js（1444 行）全量逻辑——工具栏（搜索/全部展开/全部收起/新增根组织/组织模板）+ 组织树渲染与节点行内操作（新增下级/编辑/人员管理/删除），删除走 ConfirmModal 确认；搜索为客户端过滤（命中节点+祖先链可见、纯祖先命中强制展开，清空搜索经 renderEpoch 整树重挂载还原展开态，对应旧版全量重渲染语义）
2. 递归组件与图标结构化：OrgTreeNode/TemplateTypeNode 自引用递归渲染（provide/inject 传递操作上下文）；orgConfig.js 移植 org-icons.js/org-config.js（21 图标形状对象 + 语义配色/分组/关键词映射，resolveOrgIcon 同步纯函数按 模板 icons→API icons→关键词→默认 优先级解析），OrgIcon.vue 以 `<component :is>` 渲染形状对象避免 v-html，历史 emoji 值纯文本渲染
3. 弹窗体系：ModalShell.vue 通用外壳（Esc/遮罩关闭、dialog 85vh 滚动）+ useModalStack 弹窗层级栈（模块级栈，Esc 仅关最上层，修复嵌套弹窗连带关闭——ModalShell/ConfirmModal 均接入）；OrgNodeModal 三模式（edit 仅名称/描述、add-child 类型来自 allowed-child-types 查询、add-root 选模板后锁定 type_path="0" 根类型）；EmployeeModal 员工列表+嵌套编辑+删除确认；TemplateModal 模板列表+嵌套编辑器（响应式树数据 collectLevels/collectIcons/findDuplicateTypeName 替代旧版 DOM 收集，图标面板 @click.stop 防误关）
4. 数据层 TanStack Query：树/允许子类型/模板/员工全查询化（queryFn 归一化失败显式 throw，enabled 按弹窗开合懒加载，org-available-types staleTime Infinity），写操作 useMutation + invalidateQueries（organizations-tree/org-templates/org-available-types 前缀失效）；页面层非 scoped 全局 `<style>` 承载旧版 organization.css 592 行（org-* 类名隔离，递归组件/弹窗 DOM 需跨组件生效）
5. 验证：浏览器 e2e 8 项全过（渲染/树交互/搜索过滤与恢复/节点操作/员工弹窗/模板列表+层级编辑/add-root 模板联动锁定根类型/网络与控制台零错误；首轮误报 2 项经复查均为代理视口 762px 命中移动端媒体查询与预期偏差，非缺陷）；vite build 通过；oxlint 111 文件 0 告警、biome 158 文件 0 诊断、frontend_consistency 5 测试通过
6. 联动：仅 z+1，Cargo.toml 与 frontend/package.json 均不动；旧 web/ 未动不 bump 静态版本

v0.10.5
2026102011472101
1. 资源管理页 Vue 化第一批（Vue 重构第六批·批 A）：ResourcesPage.vue 移植旧版 resourceTabs.js + networks.js 分区骨架与「网络区域/子网」两页签——页签切换 localStorage 记忆（键名 foims_subtab_resources 与旧版一致）+ route.query.tab 深链 + 前进/后退同步；两表均分页 20/页（page_size 可选 10/20/50/100，切换重置页码）、created_at 倒序默认排序、列排序、空页越界回退末页；子网页签 4 列表头搜索弹层（防抖 300ms 服务端过滤，输入态与查询参数双份状态避免防抖窗口内 queryKey 与 URL 不一致）
2. 组件配套：ThSearch.vue 表头搜索弹层（多根双 ref 外点判断、模块级 openCloser 全页互斥展开）；Pagination.vue 扩展 pageSize/pageSizeOptions props 与 page-size-change 事件（旧版 appendPaginationToTable 的每页条数语义）；icons.js 补 chart/refresh 形状；utils/network.js 迁移 isValidIPv4/isValidIPv6/isIpInCidr 及依赖链（isValidIP 无调用方不迁）
3. 三弹窗：NetworkRegionModal（IPv4/IPv6 CIDR 动态行，全空返 null）；NetworkModal（区域下拉懒加载、网关三连校验 isValid→requires_cidr→isIpInCidr、DNS 按 /[,\s]+/ 分隔上限 5）；SubnetUsageModal（子网详情+IP 列表双查询，/24 拆分网格 256 条展示上限、利用率计算 /31·/32 计 0，旧版 token 防并发由 queryKey 含 subnetId 天然替代）；删除确认沿用旧版无名字插值文案
4. 表头排序点击守卫 onThSort（closest 排除 input/button/.th-search-popover 等，对应旧版 initSortEvents 语义）
5. 验证：浏览器 e2e 7 项全过（区域/子网表渲染与分页/表头搜索过滤命中/排序三态箭头/使用情况与编辑弹窗开关/页签记忆恢复）；vite build 通过；oxlint 117 文件 0 告警、biome 158 文件 0 诊断、frontend_consistency 5 测试通过；另经 curl 验证排序/过滤/删除/明细接口契约（误测 80 端口旧版应用一轮已纠正为 5173）
6. 联动：仅 z+1，Cargo.toml 与 frontend/package.json 均不动；旧 web/ 未动不 bump 静态版本

v0.10.6
2026102012183301
1. 资源管理页 Vue 化第二批（Vue 重构第六批·批 B）：ResourcesPage.vue 追加「房间/机柜」两页签——房间表 11 列（含房型映射/工位数/当前功率/电源功率/子网绑定列表）与机柜表 10 列（含容量/机位数），均分页 20/页、name 升序默认排序（usePagedList 扩展 defaultSortBy/defaultSortOrder 参数，批 A 两页签维持 created_at 倒序）、行内操作（编辑/子项列表/信息点列表、编辑/机位列表/删除）与删除确认
2. 弹窗组件：RoomModal.vue 全量移植（组织树缩进下拉/房型六选项/电源功率 W|kW 换算/网段配置行区域级联+行间去重+至少保留一行/工位与机柜动态行按房型分区显示切换清空/信息点行本地非空查重），提交链 rooms→children→net-outlets 三段式保存期间禁用提交按钮；NetworkConfigRow.vue 网段行组件（defineModel 双绑定，region_id 变更经 TanStack queryKey 缓存级联拉取）；CabinetModal.vue 全量移植（机房类房间下拉/容量/机位行 U 位 1-48 校验/配线架行），提交链 cabinets→positions→patch-panels；SimpleListModal.vue 通用只读列表弹窗（子项/信息点/机位三处共用）
3. 基础设施：ModalShell 根节点补 modal 类使 forms.css 的 .modal 表单变体规则生效（对齐旧版弹窗 label 同行视觉）；dynamic-lists.css 移植旧版 network-config.css 动态列表样式（dyn-list/dyn-list-item/list-header/dyn-columns-header/必填星号 ::after）；utils/power.js 迁移 readPowerWattsInput、formatter.js 补 formatPowerValue；icons.js 补 list 形状
4. 验证：vite build 通过；oxlint 122 文件 0 告警、biome 0 诊断、frontend_consistency 5 测试通过；curl 验证 rooms/cabinets 列表、rooms/{id}/brief 轻量回显、options 下拉接口契约与组件期望字段逐一对齐；浏览器 e2e 因本轮 WebView infra 持续不可用（3 轮代理均阻塞于登录/导航超时）未能执行，待环境恢复后补验
5. 联动：仅 z+1，Cargo.toml 与 frontend/package.json 均不动；旧 web/ 未动不 bump 静态版本

v0.10.7
2026102013114601
1. 资源管理页 Vue 化第三批（Vue 重构第六批·批 C）：ResourcesPage.vue 追加「设备/线路」两页签完成六页签全量迁移——设备表 10 列（类型映射/功率/位置工位优先否则机柜+U 区间两者皆有才显示/房间）与线路表 10 列（A/B 端类型与编号/线路类型/线缆标签/长度/已测），设备 name 升序、线路 created_at 倒序默认排序（usePagedList defaultSort 参数），行内操作（编辑/端口/MAC/LLDP/删除、编辑/单行打印/删除）与删除确认；工具栏新增设备按钮、新增线路与批量打印标签按钮
2. 设备弹窗族：DeviceModal（SNMP 配置折叠/模板联动回填/工位与机位互斥/NetworkCardEditor 网卡-网口-IP 三级行模型、遗留网段标记、网关校验、保存链卡片→网口→IP）；DeviceTemplateModal 模板管理（行内编辑/删除确认）；UnifiedDevicePortsModal 统一端口管理（size=xl、端口排序/悬浮详情、SNMP 同步任务表顺序递归执行防限流、冲突决策 skip/overwrite 弹窗、汇总 toast）；ArpModal/LldpModal MAC 与邻居表（phase 状态机、IPv4 /24 与 IPv6 fe80/ULA/240e 网段分组折叠、300ms 防抖搜索、LLDP neighbor_port 按 port_id 非 MAC>port_desc>port_id 优先级展示）
3. 线路弹窗族：CableLinkModal（A/B 端 createSide 工厂级联：类型→范围→机柜/设备→端点四级下拉、device_interface 端点值 device_interface: 前缀编码、编辑回显 populateEndpointCascade 逐级回填、seq 序号守卫丢弃过期级联响应、禁自环校验）；CableLabelPrintModal 批量标签打印（当前页勾选、每条线路 A+B/B+A 两枚标签）；CablePrintArea Teleport body 打印区（window.print + afterprint 清理，行内单行打印经 directPrint seq 强制重挂载支持重复触发）
4. 共享常量抽取 config/deviceOptions.js：DEVICE_TYPE_OPTIONS/CARD_TYPES/PHYSICAL_TYPES/INTERFACE_ROLES 与 getDeviceTypeName（未注册枚举原样回显），DeviceModal/NetworkCardEditor/DeviceTemplateModal 三处复用；device-modals.css 补 .modal-body-scroll 弹窗内滚动规则
5. 验证：vite build 通过；oxlint 61 文件 0 告警（修复 NetworkCardEditor 两处 no-map-spread：网段选项改数组浅拷贝、buildCardRow 增 portsData 参数避免 map 内对象展开）、biome 163 文件 0 诊断、frontend_consistency 5 测试通过；浏览器 e2e 登录与两页签导航可达（WebView 不稳），curl 契约验证 devices/cable-links 列表排序参数、分页键 items/total/page/page_size/total_pages 与 macs/lldp-neighbors/nics/options/rooms 弹窗依赖接口全部通过
6. 联动：仅 z+1，Cargo.toml 与 frontend/package.json 均不动；旧 web/ 未动不 bump 静态版本

v0.10.8
2026102013275701
1. IP 详情页完成 Vue 化（Vue 重构第七批）：IpPage.vue 移植旧版 main.html ip 分区 + ipDetail.js——只读 11 列表格（位置按 工位>机柜>端口设备>设备 优先级/子网列「网段名 (区域名)」未知回显/类型映射/状态徽章/最后在线与创建时间格式化），4 列表头搜索弹层（防抖 300ms 服务端过滤，输入态与查询参数双份状态），默认 updated_at 倒序、每页 100 条（旧版 IP_PAGE_SIZE），页大小可选 10/20/50/100；MAC 列头内嵌「拉取」按钮（th-inline-action，排序守卫排除 button）
2. PullMacModal 弹窗：左列 SNMP 设备复选列表（page_size=1000 全量拉取、snmp_configured 标记过滤、占位文案四态：网络异常/暂无设备/暂无 SNMP 设备）+ 右列目标子网下拉（空值=全部子网，label 名称 (CIDR)）；三查询并行（设备/网段/pull-defaults），服务端保存的上次拉取配置就绪后预选一次（设备与现存选项求交、已删除设备剔除、默认子网已被删除时赋值自然回落空值）；提交 POST /api/resources/ip/pull（子网可缺省），按钮 pending 态「拉取中...」，成功 toast result.message 并经父组件失效 ["ip"] 重拉列表
3. 共享抽取 composables/usePagedList.js：usePagedList 工厂/seqIndex 跨页序号/sortClass 三态/onThSort 守卫自 ResourcesPage 移出（纯移动，函数体不变），ResourcesPage 与 IpPage 复用；useQuery 随移出更新 import
4. 验证：vite build 通过；oxlint 64 文件 0 告警、biome 163 文件 0 诊断、frontend_consistency 5 测试通过；curl 契约验证 ip 列表 updated_at 排序、device_name 过滤（2 条）、network 按网段名过滤（2 条）、分页键与 pull-defaults 结构；浏览器 e2e 5 项全过（11 列表头与拉取按钮/数据行与 100 每页分页选择器/4 个搜索弹层开关/拉取 MAC 弹窗开与设备列表渲染/关闭），console 零错误
5. 联动：仅 z+1，Cargo.toml 与 frontend/package.json 均不动；旧 web/ 未动不 bump 静态版本

v0.10.9
2026102014021101
1. 可视化页完成 Vue 化（Vue 重构第八批）：VisualizationPage.vue 提供 section DOM 壳（三页签 工位/机柜/设备可视化，全部旧版 id 保留）+ onMounted 调 initVisualization() 挂载画布生态；MainShell 的 RouterView 对该页启用 KeepAlive（include 组件名，旧版分区 DOM 常驻不销毁，画布模块级单例/elementCache/document 级监听依赖该语义）；路由登记 visualization 替换 SectionPlaceholder
2. 画布生态原样迁入（用户确认「画布原生+弹窗 Vue 化」方案）：11 个 visualization JS（svgCanvasBase/SVGCore/SVGDataManager/SVGRenderer/SVGVisualization/TopologyCore/TopologyDataManager/TopologyModal/TopologyRenderer/TopologyVisualization/visualizationManager.js 共 5414 行）逐字复制到 frontend/src/modules/visualization/，仅改 import 指向——apiClient→api/client.js、i18n/ui/confirm/helpers→legacyBridge、modalLoader→legacyModalLoader、pagedFetch 同路径直迁、resources 三函数→legacyResources；visualizationManager 的 loadModule 动态加载改静态 import（try/catch 保留）
3. 适配层三件：legacyBridge.js（t 经 vue-i18n global、showToast 转发 useToast、showConfirm/confirmDelete/elementCache/setActiveSubtab/getActiveSubtab/createSeqGuard/debounce/escapeHtml/nextFrame 逐字）；legacyModalLoader.js（旧 modalLoader 机制裁剪 5 条目：confirm-modal + topology 4 弹窗，HTML 经 templates/modals/ 下 vite ?raw 内联，z-index 递增栈/焦点陷阱/[data-modal-id] 关闭委托保留）；legacyResources.js（组织/可视化房间下拉与 getOrgSubtreeIds）
4. 编辑弹窗 Vue 化：legacyBridge 以可变 holder vueModals + 委托包装导出 editWorkstation/editCabinet/editCabinetPosition（ES module 导出绑定只读不可直接覆写），VisualizationPage 挂载时注入三个 Vue 弹窗实现；WorkstationModal.vue（办公类房间级联管理人按房间组织取员工、X/Y 坐标仅随 foims:workstation-saved CustomEvent 交画布静默应用不进 API）；PositionModal.vue（房间/机柜级联，房间选项自机柜数据源去重派生，编辑经 cabinet_id 反查房间回显）；CabinetModal 复用批 B 组件（与 ResourcesPage 同模式 GET 详情后打开）
5. 页面样式：visualization.css 851 行迁入 frontend/src/styles/pages/visualization.css（@layer pages/responsive 原样），main.js 全局引入——画布/拓扑/弹窗 DOM 由原生 JS 动态创建拿不到 scoped 属性，页面级样式必须走全局层；vite dev 补 /static 代理（拓扑运行时 fetch /static/icons/lucide/*.svg，dev 与生产 nginx 同链路，修复 WebView 首验的图标解析报错）
6. 验证：vite build 通过；oxlint 82 文件 0 告警、biome 164 文件 0 诊断、frontend_consistency 5 测试通过；迁移 diff 核对仅 import 头与 loadModule 段变化、5 个弹窗 HTML 与旧版字节一致；浏览器 e2e 5 项全过（三页签 tab-btn/三画布容器/工位组织 119 选项+房间下拉+SVG 渲染/机柜画布 SVG/拓扑画布 SVG 零 JS 异常，lucide 报错经 /static 代理修复后 curl 契约确认 SVG 可达）
7. 联动：仅 z+1，Cargo.toml 与 frontend/package.json 均不动；旧 web/ 未动不 bump 静态版本

v0.10.10
2026102015084901
1. 主机监控页完成 Vue 化（Vue 重构第九批）：AgentsPage.vue 移植旧版 main.html agents 分区 + modules/agents.js——工具栏（状态下拉直提交/关键字草稿态回车与查询按钮提交并回第 1 页/刷新与 30s 轮询沿用已提交筛选与当前页）、12 列表格（状态点/主机名+SNMP 来源徽标+备注标签/AgentUsageBar 迷你使用率条 >85% 红 >60% 橙/温度 ℃ 格式化/相对时间最近上报）、分页与末页越界回退；TanStack Query refetchInterval 30s 轮询（窗口失焦自动暂停，同旧版 document.hidden 语义），路由登记 agents 替换 SectionPlaceholder（仅 admin/secadmin 可达，RBAC 守卫不变）
2. 行内管理操作：详情（眼睛）/禁用（锁，PATCH status=disabled，仅 Agent 行显示）/启用（check，PATCH status=active）/删除（ConfirmModal 确认后 DELETE 级联历史），成功后失效 ["agents"] 重拉；SNMP 采集行不经上报鉴权，禁用/启用隐藏（旧版语义）
3. 阈值弹窗 AgentThresholdModal.vue：四项阈值（cpu/mem/disk 1-100、temp 1-200）留空不监控、整数范围前端校验，enabled 复选框随表单提交 PUT，成功失效 ["agent-thresholds"] 并关闭；ModalShell + footer 槽（复选框+保存按钮 form 属性关联），v-if 挂载每次打开重拉配置
4. 详情弹窗 AgentDetailModal.vue：标题主机名接续显示、状态徽章三色/IP 概要，系统信息/CPU 大数字+负载/内存容量条/磁盘（容量条+读写 IOPS+利用率，宽卡）/网卡（↓↑ 人性化速率+错误数）/温度风扇（℃/RPM）六张分组卡片 + 近 24h 趋势五卡；AgentSparkline.vue 声明式 SVG 曲线（固定 viewBox 320x96、null 断点分段 polyline、孤立单点圆点、max/min 标注、网络卡 rx/tx 双线图例），无需旧版 nextFrame 延迟测量时序；detail/history 各一条 useQuery（agentId 变化即 refetch）
5. 资源联动：agents.css 462 行原样迁入 frontend/src/styles/pages/agents.css（@layer pages，全局引入同 visualization.css 前例）；config/icons.js 补 lock 图标（与旧版 icons.js 逐字同形）；AgentUsageBar 列表与弹窗共用
6. 验证：vite build 通过；oxlint 87 文件 0 告警（修复 AgentsPage watch 内 totalPages 遮蔽外层 computed 命名）、biome 165 文件 0 诊断、frontend_consistency 5 测试通过；浏览器 e2e WebView 不稳（登录成功后无法取快照），按流程回退 curl 契约验证——列表分页键 items/total/page/page_size/total_pages、阈值配置结构、详情 raw_metrics 八字段、历史 144 点（cpu/disk/mem/rx_bps/tx_bps/temp 键与曲线取数对齐）、未登录 401 拒绝全部通过，vite transform 六个新模块均 200
7. 联动：仅 z+1，Cargo.toml 与 frontend/package.json 均不动；旧 web/ 未动不 bump 静态版本

v0.10.11
2026102016211301
1. 系统管理页完成 Vue 化（Vue 重构第十批·最后分区）：SystemPage.vue 七页签壳（系统信息/用户管理/系统配置/SMTP 配置/导入导出/定时任务/安全，旧版 i18n 键原样），页签常驻 v-show + localStorage foims_subtab_system + hash query 深链（与 LogsPage 同机制），子组件统一 props.active 控制 TanStack Query enabled 按需加载；路由此前已登记 system 替换 SectionPlaceholder
2. 用户管理：UsersTab（usePagedList 8 列含 2FA 徽章与 6 列表头排序、新增按钮入表尾 th、行内编辑/删除 icon-btn + ConfirmModal 删除确认）+ UserModal 两步弹窗（第一步账号表单角色五选/密码与确认校验/密码有效期 0-36500，第二步 2FA 三态：未保存提示/扫码启用（init 返回二维码 data URI + 6 位验证码）/已启用管理（禁用嵌套 ConfirmModal 确认后重走 init），编辑态 detailQuery 回填且密码留空沿用、has_password 动态占位）
3. 系统配置：SystemConfigTab（会话设置 server 段 spread 原样带回避免丢配置、page_timeout/public_url，保存成功双 toast + 重启提示；LDAP/SSO 配置卡 config-switch 布局、连接测试、bind_password/client_secret 留空沿用占位切换）+ ServicesCard（服务徽章四态 unregistered/active/failed/inactive 含 active_state 副本提示、start/stop/restart/reload/enable/disable 六操作、stop 与 restart 先 ConfirmModal、restart 恢复轮询 40s 成功后 foims 服务自动 reload 页面）+ CertificateCard（CA 与证书双表合并展示含来源与私钥徽章、剩余天数过期红/30 天警示、生成·导入·应用·删除、CA 下载新窗打开、CA 生成前 danger 确认）+ 四弹窗（CaGenerate/CaImport/CertGenerate/CertImport，CertSubjectFields 共用主体五字段组件以 prefix 区分双弹窗 id）
4. 通知与数据管理：NotificationTab（SMTP secureType 三态折叠为 secure 布尔、has_password 占位、通知方式矩阵 Webhook 列保留占位徽章、收件人复选清单 Set 交互 page_size=1000）+ AgentServiceCard（agent 段 spread 回带、bind_addr 拆分与正则校验、上报/采集间隔 clamp 10-3600 与 1-60、use_public_url 联动 override 显隐与校验、分发块复用 ["system-info"] 显示版本、注册令牌加载失败渲染字段文案替代旧版 error toast 避免查询重试风暴、重生成 danger 确认后 invalidate、配置包按系统格式过滤下载 downloadBlobResult）+ SnmpTrapCard（USM 用户行数组增删、协议下拉大小写归一与无法识别回落、密码掩码 *** 沿用规则、逐行校验：username 必填/去重/priv_requires_auth/RFC 3414 八字符下限、bind_addr 与冷却窗口校验）+ LogsManagementCard（统计三卡含最早记录、清理天数空值拦截与 0-36500 钳制 + 全量清理确认、外发 syslog 四字段与测试报文，成功失败均 toast 后端 message）+ DataManagementTab（CSV 导入 50MB 上限 + 扩展名与 magic bytes 双重防线 zip PK 头/csv 无 NUL、导入结果逐表明细弹窗、模板下载、八模块导出、数据库 SQL 导出）
5. 定时任务与安全：ScheduledTasksTab（无分页仅排序，reactive sort + 兼容形态复用 sortClass/onThSort、8 列含任务类型动态键缺翻译回退枚举、五行操作：立即执行（确认后 ok/err 分支 toast）/启停（togglingId 防抖）/编辑（详情接口回填 config）/日志（最近 50 条）/删除）+ ScheduledTaskModal（cron 五字段逐段校验：逗号列表/区间边界/步进≥1/L 仅日字段、非法标红 + 预览与示例弹窗叠加、mac_sync SNMP 设备复选与全部子网下拉、log_cleanup/ip_status_sync 天数与判停阈值≥1）+ TaskLogsModal/CronExamplesModal + SecurityTab（限流七字段 enabled 缺省视为开启共享 ["system-config"]、密码策略四复选默认 upper/lower/digit 开 special 关）+ Fail2banCard（状态徽章、findtime/max_retry/bantime 配置、已封禁 IP 解封（行内与手动共用 ConfirmModal）、追踪 IP 失败计数、手动封禁/解禁仅成功清空输入）
6. 基础设施：utils/download.js 新增 downloadBlobResult（apiClient blob 归一结果触发浏览器保存，Content-Disposition 文件名优先）；icons.js 补 play/power/unlock、formatter.js 补 formatDate/formatUptime（本批次前置已就绪）；styles/pages/system.css 全量迁入（旧 system 相关 CSS 合并，修正 4 处 noDescendingSpecificity：数据管理卡按钮组与导出下拉、清理表单 label 抬升父级限定，匹配面等价）
7. 验证：vite build 通过；oxlint 112 文件 0 告警（修复 CertificateCard mergedCerts map 内对象展开改 decorateCert 具名函数）、biome 166 文件 0 诊断、frontend_consistency 5 测试通过；浏览器 e2e 七页签逐一切换全过（系统信息卡片/用户表含 admin 行/系统配置表单与服务证书表/SMTP 与通知矩阵/数据管理 Agent·SNMP·导入导出·日志四卡片/定时任务空态表格/安全限流·密码策略·Fail2ban 三卡），console 零未捕获错误
8. 联动：仅 z+1，Cargo.toml 与 frontend/package.json 均不动；旧 web/ 未动不 bump 静态版本

v0.10.12
2026102020503701
1. 修复资源管理/日志页二级页签全部空白（根因）：旧版页签 CSS 契约要求显示必须依赖 `.tab-content.active`（main-content.css 自 v0.10.1 壳层落地即有 `.tab-content { display: none }`），而 ResourcesPage 六处（network-regions/networks/rooms/cabinets/devices/cable-links）与 LogsPage 三处（operation/login/notifications）页签容器只绑 `v-show` 未绑 `:class="{ active: ... }"`——数据实际已渲染进 DOM 但整层 display:none 视觉空白；逐处补齐 active 类绑定（v-show 与 active 类随同一状态联动，与 SystemPage 组件范式一致）
2. 修复 IDE 预览面板在途请求被中止后查询永久错误（次生叠加问题）：预览 iframe 挂起恢复瞬间 fetch 被浏览器掐断（net::ERR_ABORTED → TypeError: Failed to fetch），此前全局 retry:false 使查询一次失败即进错误态不再自愈；TanStack Query 全局 queries.retry 由 false 改为 1，恢复后的立即重试时渲染帧已激活可成功，真实 API 失败多试一次幂等 GET 无副作用，写操作走 mutation（默认不重试）不受影响；服务端链路验证无恙（六资源接口 curl 压测 30/30 全通、vite 代理与模块转换均 200）
3. 修复 MainShell 模板根节点复用 id="app" 的嵌套重复 id（main.html 挂载容器已占用该 id，模板根再占用使 querySelector("#app") 语义被污染、DOM id 唯一性被破坏）；样式与脚本均不依赖内层 #app，直接移除
4. 验证：oxlint 112 文件 0 告警、biome 166 文件 0 诊断、vite build 通过、frontend_consistency 5 测试通过；e2e 以 getComputedStyle 实测可见性——rooms-tab display:flex 12 行、devices-tab flex 13 行、#operation-tab flex 50 行、非激活页签均 none，console 零错误。教训沉淀：JS 读 textContent/tr 数对 CSS visibility 全盲（隐藏元素照样可读），历次 e2e 仅凭 DOM 文本验证而漏检本缺陷，可见性验证必须显式读 computed style
5. 联动：仅 z+1，Cargo.toml 与 frontend/package.json 均不动；旧 web/ 未动不 bump 静态版本

v0.11.0
2026102102290901
1. 前端整体切换 Vue 3 工程为本仓唯一前端（模块级x+1，切换收口）：旧原生 JS 前端 web/ 整体 git mv 为 web-archive/ 存档（只读参考，不再参与构建/部署/CI，AGENTS.md 声明禁止在其上继续开发）；初始化向导完整移植为 InitWizard.vue 替换占位页（5 步流程，check-pgsql/db-test/db-create/import-file/init/restart 全接口对齐旧版契约，失败统一归一 {success:false} 走错误态，与旧版语义一致）
2. 图标布局与 URL 契约变更：lucide 图标库迁入 frontend/public/icons（随 Vite 构建产物部署），引用 URL 由 /static/icons/lucide/ 统一改为 /icons/lucide/（消除 public/static 与部署目录 static/ 的双层嵌套矛盾；唯一代码引用点 TopologyRenderer.js LUCIDE_ICON_BASE）；nginx（deploy/nginx/foims.conf 与本机 foims-dev.conf）静态 location 同步调整为 /assets/、/icons/、favicon.ico（alias $web_dir/static/ 下对应目录，长缓存与安全头逐项不变）
3. 构建链与 CI：build-pkgs.sh/pak.sh/release_check.sh 改为 frontend/ 下 npm ci && npm run build 产出 dist/（部署布局 $web_dir/static/：入口页 index/main/init_index.html + assets/ + icons/ + favicon.ico）；ci.yml 新增 frontend job（Node 22 锁版本跑 biome/oxlint/vite build）并将产物接入 build job；删除 bump_static_version.sh 与 tests/frontend_consistency.rs（Vite 内容哈希指纹替代 ?v= 手工 bump，一致性测试随之失义），biome.json/.oxlintrc.json/.gitignore 同步收口到 frontend/
4. 文档同步：AGENTS.md 前端章节改 Vue 3 + Vite 并移除 ?v=/MODULE_VERSION bump 规则；code-style.md §3 整章重写为「前端（Vue 3 + Vite）」（工程结构与构建/Vue 风格/apiClient 永不 reject 契约/i18n {{name}} 占位符归一化/校验命令/拓扑遗留模块维护策略）；CSS-style.md 重写（frontend/src/styles 实际布局、@layer 全局层序与 scoped 优先级、工程化配套改 Biome+Vite）；README/README_EN 补 Node.js 22+ 前置、npm ci && npm run build/dev 步骤与 web-archive 存档说明
5. 验证：vite build 通过；cargo fmt / clippy --release -D warnings 通过；cargo test --workspace 710 通过 0 失败；biome（frontend CSS）与 oxlint（113 文件）全绿；部署冒烟 curl 7 路径全 200（入口页 no-store、assets/icons immutable 缓存分级正确、lucide SVG 内容抽查正确）；浏览器 e2e 初始化向导 5 步指示器渲染正常、console 零错误；已初始化环境（init.enabled=false）下后端不挂载 /api/init/*（main.rs 路由门控）返回 404 属预期行为
6. 联动：根 Cargo.toml 0.27.0→0.28.0（仅 lib）；frontend/package.json 版本未动（Vite 内容哈希管理资源版本，无需手工 bump）

v0.12.0
2026102112171601
1. 前端全量 TypeScript 化（模块级x+1）：api/client、composables 5 件（usePagedList/useToast/useModalStack/useTooltip/usePageTimeout）、utils 7 件、config 3 件、i18n/index、stores/auth、router、entries 3 件共 22 个 .js 全部改写为 .ts 并删除原文件；64 个组件 + 12 个页面 SFC 全部 <script setup lang="ts">（props/emits 接口化、defineProps 泛型签名）；三入口 HTML 模块引用同步 .ts；i18n schema 宽化为 MessageTree 递归类型解决 vue-i18n 巨型 JSON 推导导致的 TS2589 实例化过深；visualization 11 件遗留 .js 与 legacyBridge/legacyModalLoader/legacyResources 保持 .js（tsconfig exclude，边界三件补全 JSDoc 类型注解，vueModals 用三键精确 typedef 对齐消费方）
2. ts-rs 代码生成：#[derive(ts_rs::TS)] + #[ts(export)] 共 172 个 API 载荷类型（主 lib 20：system/config.rs 18 + routes 2；foims-models 106；foims-auth 21；foims-visualization 9；x509 6；init 4；resource/agent-service 各 2；data-management/common 各 1），TS_RS_EXPORT_DIR 导出至 frontend/src/types/api/generated/ 提交入库，TS_RS_LARGE_INT=number（i64/u64→number）；新增 da-scripts/gen-ts-types.sh 一键再生成，ci.yml 漂移校验步骤（cargo test 后 git diff --exit-code）；organization/scheduler 无本地导出类型不加依赖
3. vue-tsc strict 接入：frontend/tsconfig.json 全严格（strict/noUncheckedIndexedAccess/verbatimModuleSyntax/noUnusedLocals 等，lib ES2023），npm run typecheck 可单跑，npm run build 前置 vue-tsc --noEmit（CI build 步骤即含类型门禁，无需单独步骤）；oxlint 原生覆盖 .ts（根 .oxlintrc.json 无需改动）
4. 泛型化 API 客户端：request<T> 内部泛型贯通 + apiGet<T>/apiPost<T,B>/apiPut/apiDelete 类型签名，ApiResult<T> 失败归一 data:null，Blob 下载载荷 as unknown as ApiResult<T> 边界注释；类型驱动等价修正一处——usePageTimeout 原读 result.data.config?.server?.page_timeout 死路径（SystemInfo 无 config 字段恒 undefined），改为 apiGet<PageTimeoutConfig>("/api/system/page-timeout") 真实取值；ArpModal 排序随 lib ES2023 由展开拷贝 sort() 改 toSorted()（oxlint unicorn/no-array-sort）
5. 文档与规范：code-style.md 新增 §3.7 TypeScript 规范（禁 any/生成物只读/手写契约边界/import 后缀规则/i18n 宽化防 TS2589），§3.5 补 typecheck 与 gen-ts-types.sh 命令，§4 速查表同步；AGENTS.md 校验命令补 typecheck/gen-ts-types.sh 与 SFC lang=ts 硬性规则
6. 版本联动：根 Cargo.toml 0.28.0→0.29.0（仅 lib）；子 crate z+1：foims-common 0.3.9、foims-models 0.2.11、foims-auth 0.1.5、foims-resource 0.1.10、foims-agent-service 0.1.13、foims-visualization 0.2.6、foims-x509-management 0.2.4、foims-init 0.1.8、foims-data-management 0.2.5
7. 验证：cargo fmt --check、clippy --release -D warnings 通过；cargo test --workspace 882 通过 0 失败（含 172 export_bindings）；vue-tsc 0 错、vite build 通过；biome 0 诊断、oxlint 287 文件 0 错；gen-ts-types.sh 重跑 172 生成物哈希与入库版本一致（零漂移）；pak.sh 部署后 curl 冒烟三入口 200、登录/auth/me/system info/资源 rooms 全 200；浏览器 e2e 登录→资源页→可视化页（17 个 SVG 拓扑元素）→系统页七页签全过，console 零 JS 运行时错误（3 条 ERR_ABORTED 为路由切换在途查询取消的预期行为）
8. 遗留漂移清理与管理员改密补全（同批追加）：清理后端从未产出的展示层死键——PullMacModal 设备行 ip_address（响应无此键展示恒「-」，整行删除）、IpPage/SubnetUsageModal/SVGRenderer 四处 port_device_name·port_device_number（全仓后端零产出，逐字迁移遗留死分支，SVGRenderer 端口行保持「无」展示行为不变）；LoginPage CA 探测改用生成 CaStatus 类型替换临时内联收窄；补全管理员改密（编辑弹窗密码框此前静默无效的历史缺陷）：UserUpdate 新增可选 password（ts-rs 类型联动再生成，生成物仅 UserUpdate.ts +6/-1），update_user 过等保策略（复杂度+与当前密码及历史不重复）并预哈希，用户信息更新与密码历史同事务，重置即吊销历史令牌并刷新 password_changed_at（同请求重新启用到期用户不再误触 409），审计日志补 password_reset 布尔（不落密码明文）；UserModal 编辑态用户名置只读（后端忽略不可改的诚实 UI）；tsconfig target/lib 升至 ES2024（当前 TS 5.9.3 支持上限，ES2025 需待 typescript 升级）；验证 clippy/test 882 全过、vue-tsc/build/biome/oxlint 全绿、pak.sh 部署后临时用户全链路实测编辑重置密码→新密码登录成功→清理删除

v0.13.0
2026102115062001
1. 监听配置拆分为主机+端口两键（功能优化y+1）：config.toml [snmp.trap] 与 [agent] 段不再用 bind_addr 合写 host:port，改为 bind_addr（主机部分）+ port（u16，业界通行的 host/port 分离写法，与 [database] 段对齐）——SnmpTrapConfig 默认 0.0.0.0/162、AgentConfig 默认 [::]/9100，两结构各新增 bind_endpoint() 供 UDP/QUIC 绑定与日志拼接端点；config.toml.example 同步，本地 config.toml 顺带清理 v0.2.0 已删除却残留的 agent.dist_dir 死键
2. 消费方联动：foims-agent-service trap.rs/report_server.rs 改用 bind_endpoint()；api.rs 删 DEFAULT_AGENT_PORT 兜底与 split_bind_addr（监听地址不再需要拆分），新增 split_override_addr 仅服务手动覆盖地址解析，resolve_report_addr/resolve_server_addr 签名改收 bind_addr+bind_port，测试同步；主程序 validate_config_for_save 的 agent 段校验改为 bind_addr 非空 + port 1-65535 两查
3. 前端：AgentServiceCard/SnmpTrapCard 删除加载时 host:port 正则拆分与保存时拼接逻辑，直接读写 bind_addr/port 两字段（界面双输入框不变）
4. 升级注意：旧配置文件中合写的 bind_addr 需手工拆分为 bind_addr+port（无兼容回退层，属预期破坏性变更）；agent 侧 agent.toml 的 server_addr 合写形式不变（agent 端独立配置格式）
5. 联动：根 Cargo.toml 0.29.0→0.29.1，foims-common 0.3.9→0.3.10，foims-agent-service 0.1.13→0.1.14

v0.13.1
2026102223150001
1. 审计报告（docs/code-audit-2026-10-22.md）P1 修复批次（bug 修复/规范一致性，z+1，不动 Cargo.toml）：
2. B1 动态 SQL 缺 persistent(false)：给分页/筛选/排序组合导致 SQL 文本变化的 QueryBuilder 追加 .persistent(false)，避免一次性 prepared statement 污染 LRU——src/log/login.rs（2 处）、src/log/operation.rs（2 处）、src/log/notification.rs（count+list 2 处）、src/system/scheduled_task.rs（1 处）、crates/foims-agent-service/src/agents_api.rs（count/列表/patch 3 处，其中列表 build() 为审计核验时新发现的第 10 处，同为 push_filters 动态文本）；get_top_metrics 循环内 format! 列名来自编译期常量清单、文本恒定，按规范保持默认不加
3. F1 skipAuthCheck 静默失效：frontend/src/api/client.ts 请求入口删除 `delete mergedOptions.skipAuthCheck`（该 delete 使字段在传入 handleUnauthorized 前被抹掉，判断恒 false），改注释说明其为 RequestOptions 扩展字段、透传 fetch 时被 RequestInit 忽略；开关现对 LoginPage/ForgotPasswordModal 的 7 处显式调用真正生效
4. R1 CI 分支覆盖：.github/workflows/ci.yml push 触发由固定 [main,master,Dev] 改为 ["**"]（全分支）并保留 tag，消除开发分支更名/新增后 fmt/clippy/test/ts-rs 漂移校验的漏检窗口
5. R2 upgrade/0.25 双脚本违反「每中版本仅一份」：将 upgrade_0.25.0.sql（0.24.0→0.25.0）与 upgrade_0.25.1.sql（device_macs→snmp_macs）按顺序合并为单份累计脚本（0.24.0→0.25.x 最新），删除 upgrade_0.25.1.sql；合并因 0.26 升级脚本要求「0.25.x 最新库」基线，直接删旧会破坏升级链，故取合并而非删除
6. 验证：cargo fmt --check 通过；cargo clippy --release --workspace --lib 与 CI 同口径 cargo clippy --release -- -D warnings 均 -D warnings 零告警通过；前端 npm run typecheck（vue-tsc）0 错、npm run build 通过（仅既存 chunk>500kB 警告，属 F5 P2 拆包项）；本次未改后端载荷类型，无需 gen-ts-types.sh 再生成

v0.13.2
2026102223580001
1. 关闭 CI clippy 覆盖盲区（规范一致性，z+1，不动 Cargo.toml）：原 CI 命令仅检查 root crate 默认目标，不覆盖成员 crate 测试代码；升级为 cargo clippy --release --workspace --all-targets -- -D warnings，ci.yml 同步加注释
2. 为 9 个含测试的成员 crate 根 lib.rs 各加 #![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]，仅测试构建放开、生产仍强约束
3. 修好 --all-targets 暴露的测试代码既有 lint：services/lib.rs 两处 needless_borrow、agent-service/ingest.rs 一处 type_complexity、scheduler/error.rs 与 visualization/layout.rs 与 auth/jwt.rs 三处 items_after_test_module（尾部生产项上移到 #[cfg(test)] 前）
4. 验证：cargo fmt --all --check 通过；cargo clippy --release --workspace --all-targets -- -D warnings 全目标零告警通过

v0.13.3
2026102300081101
1. 工作区改动独立审计修复批次（bug 修复/规范一致性，z+1，不动 Cargo.toml）：
2. 升级链断裂（阻断）：upgrade/0.25/upgrade_0.25.0.sql 的 DROP COLUMN token_hash、ALTER TABLE device_macs RENAME 及 3 条索引改名全部补 IF EXISTS 守卫。原合并脚本在已执行过旧 0.25.0 的库上首条 DROP 即报 column does not exist 并整体回滚，改名不生效，随后 0.26 脚本找不到 snmp_macs 而中断；加守卫后可从 0.24.0 / 0.25.0 / 0.25.1+ 任一基线重入。头部版本口径由「最新 0.25.1」更正为 0.25.3，并注明 0.25.2/0.25.3 无数据库对象变更
3. F1 收敛为单一判定：frontend/src/api/client.ts 删除 RequestOptions.skipAuthCheck 字段与 handleUnauthorized 的 || 并行判断（7 处显式调用 URL 全部命中 isPublicAuthEndpoint，两套机制判同一件事），LoginPage.vue 6 处、ForgotPasswordModal.vue 1 处第三参数一并移除
4. CI 测试覆盖：cargo test 由 root-only 升级为 cargo test --release --workspace，连带修复 ts-rs 漂移校验空转——#[ts(export)] 全部位于成员 crate（root crate 无），原 root-only 测试不执行任何 export_bindings，git diff --exit-code 恒通过；现由 workspace 测试真实重生成后再校验。clippy 注释更正：cfg(test) 豁免不覆盖 tests/ 集成测试独立 crate
5. root crate 补齐测试豁免：src/lib.rs 加 #![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]，与 9 个成员 crate 对齐，消除「root 测试模块写 unwrap 即打破 -D warnings」的不对称陷阱
6. 审计报告事实更正：docs/code-audit-2026-10-22.md 调用点 10→7、clippy 命令补 --workspace --all-targets、F1 修复方案改为删除冗余字段
7. 验证：cargo fmt --all --check；cargo clippy --release --workspace --all-targets -- -D warnings；cargo test --release --workspace export_bindings（生成物无漂移）；cd frontend && npm run typecheck && npm run build

v0.14.0
2026102319534701
1. 修复 SNMP 轮询设备不可达误判在线（功能 bug，y+1）：poll_and_upsert 中系统组三 OID（sysName/sysDescr/sysUpTime）全部无响应（超时）时原实现仍走 upsert——status 翻 active、last_seen=NOW()，导致不可达设备在主机监控页恒显示在线且离线翻转逻辑（依赖 last_seen 停更）永不触发，仪表盘主机负载卡片（top-metrics 仅统计快照列非空的 active 行）与主机状态展示互相矛盾；修复为系统组全空时按采集失败处理（return Err，不 upsert、不刷新 last_seen），连续两轮失败后由既有离线翻转统一置 offline
2. 仪表盘榜单卡片重排（UI 调整）：主机负载卡片移至 dashboard-grid 首位，子网列表由第二位移至第四位（新序：主机负载/IP/日志/子网/机房/机柜）
3. 联动：根 Cargo.toml 0.29.1→0.29.2，foims-agent-service 0.1.14→0.1.15

v0.14.1
2026102321055601
1. 修复设备 SNMP 信息保存被掩码清空（功能 bug，y+1）：DeviceModal 编辑回显对 community/auth_password/priv_password 显示掩码，提交时 maskToNull 将掩码归一为 null——后端 DeviceUpdate 双层 Option 语义中 null=清空，导致仅修改其他字段保存时已配置的 SNMP 凭据被静默清空；修复为 snmpSecretPayload 三态归一（掩码→undefined 省略键=保留原值、空串→null=主动清空、有值→更新），发送载荷类型相应放宽三个掩码键为可缺省
2. SNMP 测试端点回环地址从拒绝改为告警放行：环回是机房管理正常目标（本机 snmpd 联调、环回对照设备），与私有地址同为 log_warn 放行，组播/链路本地保留拒绝；同步删除 test_snmp_rejects_loopback_addresses 单测与前端 loopback_forbidden 消息键，新增 log.device.snmp.loopback_ip_allowed 日志键（zh/en）
3. 修复 SNMP CPU 采集永远为空（功能 bug）：snmp_metrics.rs 的 ssCpuRaw* 11 个计数器 OID 缺少 .0 标量实例后缀，agent 全部返回 No Such Instance 异常 → counters 恒 None → usage_pct 恒空（负载/内存/温度/磁盘/网络正常佐证仅 CPU 分支断裂）；补齐 .0 后缀
4. 本机 snmpd 联调验证：环回对照设备（移动笔记本，IP=127.0.0.1）配置 SNMP 后下一轮轮询自动纳入主机监控（source=snmp，sysName 识别），加密存库/解密使用链路（AES-256-GCM）实测通畅
5. 联动：无 Cargo.toml 版本变化（z 段内功能 bug 修复批次随 v0.14.0 联动已完成，本条仅 y+1 记录）

v0.14.2
2026102321331201
1. 修复 SNMP trap 监听绑定失败（部署配置修正 + 文档对齐）：早期 bind_addr 字段语义为 URL（host:port），后改为独立 host + port 字段，部署配置 /etc/foims/config.toml（及 Debian 包安装留存模板）未同步仍为 "0.0.0.0:162"，与 port 默认值 162 拼接成 "0.0.0.0:162:162" 导致启动报错；按设计保留配置异常报错行为（不加兼容逻辑），修正部署配置 bind_addr 为纯主机部分并配置 community 白名单
2. 统一修正三处与实现矛盾的文档（空白名单实为拒绝处理而非接受任意）：config.toml.example 注释、SnmpTrapConfig 字段文档（foims-common/config.rs）、系统配置页 snmp_trap 帮助文案（zh/en）
3. 实测验证：trap 监听 0.0.0.0:162 启动、v2c trap（白名单命中）接收、站内通知落库全链路通畅

v0.14.3
2026102323151801
1. 恢复模态框原版带页头/页脚布局（布局修复）：重构期原版 modals.css 的外壳段未移植，遗留体系弹窗（全局确认框、拓扑连接/容器/详情）无外壳样式渲染破版，Vue 体系 ModalShell 又将原版视觉简化（浅色页头、无分隔线页脚、整体滚动）；新建全局 styles/components/modals.css 逐字移植外壳段并桥接两套类名（遗留 .modal/.modal-content/.close 与 Vue .modal-overlay/.modal-dialog/.modal-close），恢复深色标题栏（背景+白色竖条装饰）、关闭钮白色圆形悬停旋转、页脚上分隔线与按钮最小宽 90px、body 独立滚动（页头页脚常驻）、弹窗偏上 2rem 布局与入场动画
2. 消除四份重复外壳样式：ModalShell.vue/ConfirmModal.vue/ChangePasswordModal.vue/LogDetailsModal.vue 删除各自 scoped 外壳样式改走全局，仅保留弹窗专属宽度与内容样式；ChangePasswordModal 页脚外置出 form 滚动区（submit 按钮 form 属性关联）保证页脚常驻；ConfirmModal 走全局按钮样式
3. 浏览器实测通过：修改密码弹窗/资源管理弹窗深色页头+分隔线页脚+body 独立滚动全部符合原版视觉

v0.14.4
2026102400592801
1. 修复工位/机位可视化数据链断裂（功能缺陷修复，z+1，不动 Cargo.toml）：
2. 工位画布 IP 恒空：/api/resources/ip 行（IpDetailWithNames）不含 workstation_id——视图 ip_with_details 只暴露 workstation_name（名称不可作匹配键），而 SVGVisualization.js 与 SVGDataManager.js 按 ipDetail.workstation_id 归并 IP，映射恒空导致工位卡片 IP 恒显示 viz.no_ip。修复为视图列尾追加 dv.workstation_id，ip.rs 列表查询列与 IpDetailWithNames 结构体同步补字段（列尾追加，兼容 CREATE OR REPLACE VIEW 只允许尾部加列的限制）；前端归并逻辑无需改动。
3. 工位/机位「端口」恒显示无端口：SVGRenderer.js 两处硬编码 viz.no_port，注释称后端 IP 行不含端口归属字段（实际 interface_name/physical_type/interface_role 已随视图与查询返回）。改为取 ipDetail.interface_name（缺失时兜底 viz.no_port）并删除过时注释，机位 tooltip 同步。
4. 升级脚本 upgrade/0.29/upgrade_0.29.0.sql（0.29 中版本唯一一份）：CREATE OR REPLACE VIEW ip_with_details 追加 workstation_id，幂等可重入；无表结构（列/索引/约束）变更，foims-init 建表代码（schema/tables/views.rs）已同步，check.rs 仅校验视图存在性（视图列不在其校验范围）。
5. 生成类型：cargo test --workspace export_bindings 重新导出，仅 IpDetailWithNames.ts 增加 workstation_id，其余生成物无漂移。
6. 验证：cargo fmt --all --check 通过；cargo clippy --release --workspace --all-targets -- -D warnings 零告警（EXIT=0）；cd frontend && npm run typecheck 0 错、npm run build 通过（仅既存 useToast 动态导入与 chunk>500kB 警告）。

v0.15.0
2026102419061001
1. 主配置搜索路径收敛（功能优化 y+1）：foims-common CONFIG_PATHS 由三项降为两项——/etc/foims/config.toml（生产唯一落点）→ ./config.toml（仅系统路径缺失时回退，适配开发/手动部署），删除 /opt/foims/config.toml 备用路径；/opt/foims 自此只存程序资源（web/static），避免同一配置存在两个可写副本导致运维改错文件、升级与回滚互相覆盖
2. 备份目录统一为 /usr/share/foims/backups（全仓唯一落点）：foims-init 新增 BACKUP_DIR 常量并对外导出，get_backup_dir 去掉 $HOME/foims_backups 与 /opt/foims/backups 两套回退；定时备份任务（src/system/task_executors.rs）由硬编码 /var/lib/foims/backups 改为复用 foims_init::BACKUP_DIR——原路径既与向导备份目录不一致，也不在 systemd ProtectSystem=strict 的 ReadWritePaths 白名单内，发布包部署下会写盘失败
3. 部署与打包同步：deploy/services/foims.service 增加 ReadWritePaths=/usr/share/foims 并更新路径约定注释；scripts/build-pkgs.sh deb postinst / rpm %post 创建 /usr/share/foims/backups（foims:foims 0750），deb purge 与 rpm %postun 清理该目录，man FILES 补条目；da-scripts/pak.sh 配置落点由 /opt/foims/ 改为 /etc/foims/config.toml 并预建备份目录
4. 文档与示例：README.md / README_EN.md 配置搜索顺序同步为两条路径，config.toml.example 顶部补搜索顺序说明（含 /opt/foims 不再读配置）
5. 核实 nginx 默认证书链路无误（不改代码）：deb postinst / rpm %post 在安装时用 openssl 自签 /etc/ssl/foims-certs/default_cert.pem/.key（已存在且剩余≥1 天则跳过），nginx 配置注释与实现一致；foims 本体只监听 UDS（/run/foims/api.sock），自身不需要证书，agent QUIC 使用的 agent_cert.pem 由站点 CA 单独签发，与 HTTPS 默认证书互不影响
6. 联动：根 Cargo.toml 0.29.2→0.29.3，foims-common 0.3.10→0.3.11，foims-init 0.1.8→0.1.9；无数据库结构变更（无需 upgrade SQL 与 foims-init 建表/校验同步）
7. 验证：cargo fmt --all --check；cargo clippy --release --workspace --all-targets -- -D warnings；cargo test --release --workspace（含新增备份目录固定值单测）

v0.16.0
2026102419450101
1. 前端资源目录改为 Linux 标准 share 布局（功能优化 y+1）：/opt/foims/web/static → /usr/share/foims/web，dist/ 内容直接铺开（入口页 index/init_index/main.html 与 assets/、icons/、favicon.ico 同级，取消 static/ 中间层；nginx 路由 URL 不变，仍为 /index.html、/assets/、/icons/、/favicon.ico）；/opt/foims 目录自此完全废弃
2. deploy/nginx/foims.conf：$web_dir 改为 /usr/share/foims/web，6 处 alias（入口页/favicon/assets/icons）去掉 static/ 段，头注释同步产物布局
3. deploy/services/foims.service：WorkingDirectory 改为 /usr/share/foims（相对配置回退 ./config.toml 不再命中旧 /opt/foims）；ReadWritePaths 由整目录 /usr/share/foims 收窄为 /usr/share/foims/backups，静态资源目录对服务进程只读
4. scripts/build-pkgs.sh：deb 暂存/复制路径改 /usr/share/foims/web（cp -r dist/. 镜像，避免 web/dist 嵌套），删除 /opt 目录创建与 chmod；deb postinst / rpm %post 的 adduser|useradd home 改 /usr/share/foims、install -d 去掉 /opt/foims，并清理旧版 /opt/foims/web 与 /opt/foims/config.toml 残留后 rmdir；purge / %postun 改 rm -rf /opt/foims；rpm %files 改 /usr/share/foims/web；man FILES 条目同步
5. da-scripts/pak.sh 同步手动部署：清理 /opt/foims，静态资源改为 /usr/share/foims/web（cp dist/. + 目录 755/文件 644，与安装包一致），删去 /opt 相关空目录与 ls 步骤
6. 文档与注释统一：README.md / README_EN.md / AGENTS.md / docs/code-style.md 的静态目录路径；config.toml.example 说明 /opt/foims 已废弃；foims-common CONFIG_PATHS 注释与 foims-agent-service alerts.rs 的 i18n 引用（web/static/i18n JSON → frontend/src/i18n）
7. 全仓路径复查（本次新统一口径）：配置 /etc/foims、日志 /var/log/foims（log_dir 取 CARGO_PKG_NAME）、socket /run/foims、证书 /etc/ssl/foims-*、备份 /usr/share/foims/backups、静态 /usr/share/foims/web；剩余绝对路径仅 /tmp/foims_import（初始化导入 SQL 临时件，进程 PrivateTmp 下隔离）与 agent 侧独立的 /etc/foims-agent、/var/lib/foims-agent
8. 联动：根 Cargo.toml 0.29.3→0.29.4，foims-common 0.3.11→0.3.12，foims-agent-service 0.1.15→0.1.16（均仅注释变更）；无数据库结构变更
9. 验证：cargo fmt --all --check；cargo clippy --release --workspace --all-targets -- -D warnings；cargo test --release --workspace；bash -n scripts/build-pkgs.sh 与 sh -n da-scripts/pak.sh 语法通过；全仓 grep 确认除废弃清理语句与历史记录外无 /opt/foims、web/static 残留

v0.16.1
2026102420353001
1. 修正 foims.service 的工作目录语义（部署配置纠偏，z+1，不动 Cargo.toml）：该单元仅用于生产（开发以 da-scripts/pak.sh 模拟生产部署），与 CONFIG_PATHS 的 ./config.toml 相对回退无关；删除 v0.16.0 引入的 WorkingDirectory=/usr/share/foims，回归 systemd 默认 /，避免相对路径被意外命中
2. 口径统一：foims.service 路径约定改为“配置 /etc/foims/config.toml（生产唯一落点）”，并注明 ./config.toml 仅供开发直接运行二进制；crates/foims-common CONFIG_PATHS 注释、config.toml.example 搜索顺序说明、README.md / README_EN.md 同步（去掉“手动部署”歧义）
3. 验证：systemd-analyze verify deploy/services/foims.service 无输出（通过）；cargo fmt --all --check；cargo clippy --release --workspace --all-targets -- -D warnings；grep 确认全仓除历史记录外无 WorkingDirectory 残留

v0.17.0
2026102508590001
1. 可视化画布生态彻底 Vue 3 + TS 重写（模块级 x+1）：删除 modules/visualization/ 下 11 个命令式 JS（共 5,414 行）与桥接层三件（legacyBridge/legacyModalLoader/legacyResources）及 templates/modals/ 5 份 HTML 片段，不再保留任何 .js 源文件；新分层：pages/VisualizationPage.vue（唯一状态持有者：页签、下拉、三套数据编排、弹窗与确认流）+ components/visualization/（WorkstationCanvas/CabinetCanvas/TopologyCanvas 三块声明式画布 + CanvasGridLayer 共享网格层 + 拓扑四弹窗 ConnectionCreate/ConnectionDetail/ContainerCoord/DeviceDetailPanel）+ composables/useCanvasViewport.ts·useCanvasTooltip.ts（视口与浮层，三画布共享）+ utils/visualization/geometry.ts（纯几何与布局算法）·viewModel.ts（载荷→视图模型）·resources.ts·icons.ts + api/visualization.ts
2. 渲染层声明式化：不再命令式创建/删除 SVG 节点，设备/机位/容器/连线全部由 nodes+connections 经 computed 派生，拖动期间仅在组件内保留预览矩形，连线路径随之自动重算（删除旧 updateConnectionPaths 增量重画与 _updateChildPositions 回写）；机位矩形由柜体矩形+U 区间实时推导
3. 坐标唯一真相收口到响应式视图模型：拖动落定由画布 emit 矩形、页面写回，保存经 buildLayoutItems 投影，彻底取消「保存时从 SVG DOM 反读属性」；同步收敛视图模型 API（WorkstationCanvas 只留 nodes/door，CabinetCanvas 只留 nodes/totalWidth，删除 container/viewBox/ownerIds/containerWidth/uHeight 等无人消费的派生字段与归属过滤死逻辑）
4. 旧语义逐项保留：三态返回（saved/empty/stale/error，布局请求失败不得触发自动排布落库）、渲染代次 token 丢弃过期响应、机柜分批渲染（16 柜/批，批内统一拉机位 IP，重排命缓存零请求）与 cabinetSettled 保存闸门、600ms（工位）/500ms（拓扑）/150ms（机柜容器 resize）防抖、房间级（ROOM_GAP 80）与机柜级（40）分组推挤、缩放 ±2% 预设回显与 fit 档、子页签 localStorage 持久化、组织子树筛选、自动发现/自动布局/连线模式
5. 交互与修复：WorkstationModal 不再向 document 派发 foims:workstation-saved CustomEvent，改为类型化 saved 载荷（id/roomId/x/y）；工位弹窗保存后按当前房间重载以同步标签与房间归属，机柜/机位弹窗保存后重载以重算 U 高与机位矩形；拓扑节点删除、工位/机柜布局删除统一走 ConfirmModal；设备详情面板的移除改为确认后执行
6. 工程收尾：tsconfig 关闭 allowJs 并删除 src/modules/visualization 的 exclude 例外；MainShell 去掉 <KeepAlive include="VisualizationPage"> 特例（离场即销毁，状态由页面自行持有，重新进入重新加载）；modals.css 删除遗留体系死选择器（.modal:not(.modal-dialog)/.modal-content/.close/[id$="-ips-container"]）并改注释为 ModalShell 单体系；docs/code-style.md §3.6 重写为「可视化画布（工位/机柜/拓扑）」职责分层与四条约束（坐标唯一真相/三态语义/渲染代次/模板 ref 传递方式），§3.1 目录树与 §3.7 import 后缀规则同步
7. 验证：cd frontend && npm run typecheck 0 错；npm run build 通过（仅既存 useToast 动态导入与 chunk>500kB 警告）；biome check frontend 0 诊断；oxlint 292 文件 0 warning 0 error；两个可执行自检脚本全过（入口 `cd frontend && npm run selfcheck:viz`，带 ts-resolve-hook 补无扩展名 import 的 .ts 解析；geometry 24 项含推挤/锚点错开/正交折线，viewmodel 11 项含坐标回退与分组筛选）；浏览器冒烟（vite dev + 已运行后端）：工位 10 个节点/网格 19 条/标尺/门、机柜 1 柜 10 个 U 刻度、拓扑 13 设备 9 容器 2 连线且缩放 24%→30% 生效，console 零错误、无 ≥400 请求
8. 联动：根 Cargo.toml 0.29.4→0.30.0（仅 lib，本次仅前端；子 crate 版本不动）；无数据库结构变更（无需 upgrade SQL 与 foims-init 同步）
9. 遗留说明：布局为空时选定房间仍会自动排布并落库一次（沿用旧版 showWorkstationRoom 语义，首次进入即出图）；拓扑画布缩放初始为 fit（适应画布）而非固定 100%，与旧版 _fitView 一致

v0.17.1
2026102509244401
1. 版本规则收口——前端改用 Vue 3 框架后版本更新复用后端同一套规则（普通修改 z+1，仅文档不动 Cargo.toml）：本文件规则头删除「前端资源按照自己的规则更新版本号」旧条款，明确前端无独立版本号体系、按 x/y/z 判级联动 Cargo.toml，旧 bump_static_version.sh/?v=/MODULE_VERSION 机制随框架切换废弃
2. 文档同步：AGENTS.md 版本与测试条目改写（前端复用 x/y/z 判级与 Cargo.toml 联动，Vite 内容指纹仅承担静态资源缓存）；docs/code-style.md §1 版本号规则改为指向本文件（消除「每次代码更新 bump yy」与「z 段不动 Cargo.toml」的规则冲突）、§3.1 缓存与版本同步；deploy/nginx/foims.conf main.html 段删除 ?v= 版本号旧机制残留注释

v0.17.2
2026102509335001
1. 前端版本承载落点细化（普通修改 z+1，仅文档）：frontend/package.json 的 version 字段确立为前端版本号唯一落点，复用本文件 x/y/z 判级与 Cargo.toml 同机制独立联动（z 段不动 / y 段 c+1 / x 段 b+1 且 c 归零 / a 段手动，沿用 0.1.0 起独立计数）；仅前端修改不再 bump 根 Cargo.toml（联动对象由 lib 改为 package.json），仅后端修改照旧联动 Cargo.toml，混合修改按各自维度分别联动
2. 文档同步：dev_version.md 规则头改写前端条款（删除「仅前端修改 lib 更新版本号」旧规则）；AGENTS.md 与 docs/code-style.md §1/§3.1 同步，「无独立前端版本号」表述废除

v0.17.3
2026102510341201
1. 前端 i18n 补键（普通缺陷 z+1）：VisualizationPage 三处组织筛选下拉的 aria-label 引用了不存在的 organization.title（vue-i18n 回退输出原始键名），新增 organization.filter_label（zh「组织筛选」/en「Filter by organization」）并替换工位/机柜/拓扑三页签的引用
2. api/client.ts 两处修复：非 JSON 非下载响应的兜底分支先 response.json() 再 response.text()，body 已被消费导致 text() 必抛 TypeError（错误被外层 catch 归一为 network_error，且 data:text 返回值不可达）——改为先取原文再 JSON.parse；showToastAuthError 的动态 import 改静态导入（useToast 仅依赖 vue，不存在其注释所称的循环引用，原写法反复触发 vite INEFFECTIVE_DYNAMIC_IMPORT 告警）
3. 死代码与静态资源清理：删除无引用的 pages/SectionPlaceholder.vue（router 经 SECTION_COMPONENTS 直连各分区页），MainShell 头注释「当前统一为 SectionPlaceholder 占位」同步改写；public/icons/lucide 由 2142 个 vendored SVG 裁剪至 DEVICE_ICON_FILES 实际使用的 10 个（network/cpu/server/router/cctv/phone/monitor/laptop/printer/package，LICENSE 保留），部署产物 icons/lucide 120K→48K
4. 文档同步：docs/code-style.md §3.1 目录树标注图标裁剪约定，并修正 api/client.js → api/client.ts
5. 联动：仅前端修改，frontend/package.json 0.1.0→0.1.1（z+1）并同步 package-lock.json 根版本，根 Cargo.toml 不动；无数据库结构变更
6. 验证：cd frontend && npm run typecheck 0 错；npm run build 通过（INEFFECTIVE_DYNAMIC_IMPORT 告警消失，仅余 chunk>500kB）；npm run selfcheck:viz 全过

v0.18.0
2026102510554901
1. 路由懒加载拆包（功能优化 y+1）：router 的 8 个分区组件由静态 import 改为 () => import()，SECTION_COMPONENTS 用 RouteRecordRaw["component"] 约束（键仍由 SectionName 派生）；入口 main 546.60 kB→78.30 kB（gzip 137.18→26.62），VisualizationPage/ResourcesPage/SystemPage/LogsPage/AgentsPage/IpPage/DashboardPage/OrganizationPage 各自独立 chunk，chunk>500kB 构建告警消除
2. 子页签逻辑收口到 composables/useSubTabs.ts（消除 SystemPage/ResourcesPage/LogsPage/VisualizationPage 四份重复的 readInitialTab/switchTab/watch）：入参 storageKey + tabs 元组（首项即默认，非空由类型保证）+ syncQuery（默认 true，可视化画布页签传 false）+ onSwitch（拓扑页签拉数据/离开时关连线详情）；页签 id 经 as const satisfies SubTabMeta 收窄为字面量联合，activeTab 用 shallowRef + Ref<Id> 断言避免 ref 深层解包宽化泛型；TABS 字段 labelKey 统一为 titleKey
3. api/client.ts 补 apiPatch（此前 AgentsPage 状态切换裸调 request 传 method）——AgentsPage 改 apiPatch(`/api/agents/${id}`, { status: next })，AgentServiceCard 配置包下载裸 request 改 apiGet，request 随之降为模块内部函数（不再对外暴露），全站请求出口收敛为 apiGet/apiPost/apiPut/apiPatch/apiDelete
4. 配置备份/恢复入口回归（旧版 systemManager.js 有、Vue 版无任何入口，后端 /api/system/config/backup|restore 因此长期未接线）：数据管理页「导入导出」卡新增「配置文件」区块——备份→GET backup 下载服务端文件名；恢复→选 .json →JSON.parse 并校非数组对象→ConfirmModal 二次确认→POST restore（整体 Config 载荷，字段校验与脱敏值还原由服务端完成）→成功提示重启生效
5. client 下载判定纠偏：改为「Content-Disposition: attachment 或下载类 Content-Type 优先于 JSON 解析」，修复 backup 端点以 application/json+attachment 下发时被当普通 JSON 消费、丢掉文件名与 Blob 的问题（其他下载端点行为不变）；恢复文件类型按实际产物 JSON 校验（旧版 accept=.toml 与后端 JSON 产物不符）
6. i18n：data-management 新增 config_file/config_backup/config_restore/config_restore_confirm，import_export 新增 backup_success/config_invalid（zh/en 同步）
7. 文档：docs/code-style.md §3.1/§3.2/§3.3/§3.7/§4 同步（useSubTabs 统一子页签、apiPatch、下载判定顺序、禁止绕过封装直调 request/fetch、client.ts 与 download.ts 后缀）
8. 联动：仅前端修改，frontend/package.json 0.1.1→0.1.2（y+1 对应 c+1）并同步 package-lock.json；根 Cargo.toml 不动；无数据库结构变更
9. 验证：npm run typecheck 0 错；npm run build 无告警（已无 chunk>500kB）；npm run selfcheck:viz 全过；biome check frontend 22 文件 0 诊断；oxlint frontend/src 288 文件 0 warning 0 error

v0.19.0
2026102511095901
1. 域级 API 传输层落地（功能优化 y+1）：新增 api/logs.ts，把日志/通知域的 URL 与载荷类型收进模块（操作/登录日志与通知分页 URL 拼装、通知已读两个 PUT、系统日志统计/清理/外发配置三组读写、外发测试），职责边界与既有 api/visualization.ts 一致（只组 URL/请求体返回 ApiResult，不做 toast 与 i18n）；LogsPage 与 LogsManagementCard 不再拼 /api/ 字面量，组件内手写的 LogsStatsPayload/LogForwardingPayload/LogCategoryStats 迁为模块导出类型（LogsStats/LogForwarding/LogCategoryStats）
2. LogsPage 退回共享分页实现：该页曾保留一份页内克隆的 local usePagedList（三参 buildUrl、无页大小切换，与共享版并行演化）——删除 86 行克隆代码改用 composables/usePagedList，buildUrl 改四参签名（pageSize 显式传入），页大小常量改 defaultPageSize；共享实现是唯一分页来源
3. 手写契约去重：RoomBrief（Room + networks/workstations/cabinets/net_outlets 四个 brief 列表，后端 get_room_brief 跨表组装）原在 ResourcesPage 与 RoomModal 各定义一份，统一抽到 types/api/contracts.ts 并由两处 type-only 导入，顺带清掉五个随之无用的 generated 类型导入；四份同名 interface SystemConfigPayload 按段改名为 RateLimitConfigPayload/ServerConfigPayload/AgentConfigPayload/SnmpConfigPayload（形状本就不同，同名易误判为重复定义）
4. 消除类型硬转：usePagedList 新增 SortableTable（sort + toggleSort）作为表头排序工具的最小依赖，PagedList<T> 改为 extends 它；sortClass/onThSort 入参由 PagedList<T> 放宽为 SortableTable，ScheduledTasksTab 的 as unknown as PagedList<ScheduledTask> 边界断言删除（无分页列表不再需要伪造分页字段）
5. 无障碍标签全量 i18n 化：55 处硬编码英文 aria-label（表格/区块/表单控件/仅图标按钮，涉及 18 个组件与页面）全部改为 :aria-label="t('<键>')"，优先复用既有词条（nav.*/resources.*/logs.*/smtp.*/ldap.*/sso.*/agent_service.*/cert.*/config.*/two_factor.*/user.*/services.title/system.scheduled_tasks），新增 5 键：nav.main_navigation、common.unit、language.select、user.list、data-management.export_module（zh/en 同步）
6. 文档：docs/code-style.md §3.1/§3.2/§3.3/§3.4/§3.7/§4 同步（api/<域>.ts 目录位、usePagedList 四参签名与 SortableTable、域级封装与渐进迁移、aria-label 必须走 i18n、手写契约落点与禁同名重复定义）
7. 联动：仅前端修改，frontend/package.json 0.1.2→0.1.3（y+1 对应 c+1）并同步 package-lock.json；根 Cargo.toml 不动；无数据库结构变更
8. 验证：npm run typecheck 0 错；npm run build 通过（入口与各页面 chunk 体积不变，无 chunk>500kB 告警）；npm run selfcheck:viz 全过；npx oxlint frontend/src 290 文件 0 warning 0 error。注意：本轮发现审计命令缺口——根仓未安装 @biomejs/biome，`npx biome` 解析到同名旧包 biome@0.3.3（无 check 子命令，静默 exit 0），故此前「biome 0 诊断」结论不可信；本轮未改 CSS，需在补齐 @biomejs/biome devDependency 后重跑全量样式审计

v0.19.1
2026102511373301
1. system 域迁移（z，续做域级传输层）：新增 api/system.ts（按段读写的 getSystemConfig<T>/updateSystemConfig + 配置备份恢复 + ldap/sso/password-policy/smtp/notification-settings/fail2ban/证书库存与 CA·证书生成导入应用删除/services/导入导出/系统信息），SystemConfigTab、SecurityTab、SnmpTrapCard、NotificationTab、Fail2banCard、ServicesCard、SystemInfoTab、CertificateCard、CaGenerateModal、CaImportModal、CertGenerateModal、CertImportModal、DataManagementTab、AgentServiceCard 共 17 个文件约 68 处 /api/ 字面量归零；SmtpConfigResponse/ServiceStatusItem/ImportResultRow/ImportCsvResponse/BanActionResponse 五个手写载荷类型上移为模块导出，组件内重复定义删除
2. 新增 api/users.ts（与 usePagedList buildUrl 同签名的四参 usersUrl、收件人候选 page_size=1000、用户 CRUD、2FA init/enable/disable）与 api/scheduledTasks.ts（列表/详情/增改/启停/立即执行/执行日志）、api/agents.ts（注册令牌查看与重生成、配置包下载）：UsersTab、UserModal、ScheduledTasksTab、ScheduledTaskModal、TaskLogsModal、AgentServiceCard 的 URL 归零，TaskRunResponse/UserFormPayload/TwoFactorInitData 上移；ScheduledTaskModal 的 /api/resources/devices 与 /api/resources/networks 属 resources 域，留待该域迁移
3. 认证域 api/auth.ts：LoginPage 的 11 处认证 URL 收口（登录页专用的 /api/auth/me 裸 fetch 探测保留原语义并封装为 probeSession，注释说明为何不走统一客户端；SSO 发起与站点 CA 状态/下载一并归位），LoginBody/LdapTwoFactorBody/LoginPayload/AuthMethods/CaptchaResponse 类型上移；模板 CA 下载链接改绑 CA_DOWNLOAD_URL
4. 同名符号冲突：域模块导出名与组件局部函数重名（testLdapConnection/testSsoConnection/testSmtpConnection/initTwoFactor/banIp/unbanIp）时统一 as 别名导入，不改组件对外函数名；api/system.ts 整库导出改名 downloadDatabaseExport 避免与 DataManagementTab 局部 exportDatabase 冲突
5. 存量收敛进度：非 api 模块的 /api/ 字面量 247 处/56 文件 → 142 处/35 文件；logs、system、auth 三个域在页面与组件侧已无字面量
6. 文档：docs/code-style.md §3.1/§3.3 同步（域模块清单、同名符号 as 别名导入、按段配置的泛型读写）
7. 未做（下一轮）：LoginPage 吉祥物动画内核（597-1021 行，约 425 行命令式 DOM + 定时器）外提为 composables/useLoginMascot.ts 需像素级行为等价与浏览器冒烟，本轮只做认证端点收敛
8. 验证：npm run typecheck 0 错；npm run build 通过（main 78.38 kB、client 372.34 kB、SystemPage 119.05 kB，无 chunk>500kB）；npm run selfcheck:viz 全过；npx oxlint frontend/src 295 文件 0 warning 0 error；biome 仍不可用（根仓未装 @biomejs/biome，本轮未改 CSS）；仅前端修改，package.json 保持 0.1.3（z 段不联动版本），根 Cargo.toml 不动，无数据库结构变更

v0.19.2
2026102511542101
1. 恢复真实的 CSS/格式审计（z，工具缺口修复）：根 devDependencies 安装 @biomejs/biome 2.5.15（与 ci.yml 固定版本、biome.json schema 2.5.15 三者一致），npx biome 不再解析到 npm 上同名旧包 biome@0.3.3；根 package.json/package-lock.json 受 .gitignore 的 /package* 排除，属本机前置不入库，CI 仍由 npx --yes @biomejs/biome@2.5.15 自取，两者版本必须相同
2. 空跑防护与验证：临时放不合格式的 CSS 探针 frontend/src/styles/__audit_probe.css（.probe{color:red}），biome check 报 formatter 2 错（Checked 23 files）确认命令真在执行，随后删除探针恢复 22 文件
3. 真实审计结果：npx biome check frontend 与 CI 等价命令 npx --yes @biomejs/biome@2.5.15 check frontend 均为 Checked 22 files、0 诊断、退出码 0（范围即 frontend/src/styles 全部 22 个 CSS：components/layouts/pages 子目录 + toasts/init/login/reset/variables）；v0.19.0 与 v0.19.1 中「biome 0 诊断不可信」的记录就此关闭，此前结论复核为真
4. 文档：docs/code-style.md §3.5 审计命令统一为「npx --yes @biomejs/biome@2.5.15 check frontend」（与 CI 一致）并补本机直跑方式；新增两条约定——Biome 工具就位（根 package.json 不入库、本地与 CI 版本必须相同）与空跑防护（“0 诊断”需先用不合格式探针证明命令真在检查）
5. 验证：本轮未改前端源码与样式，typecheck/build/selfcheck:viz 不受影响未重跑；biome 真实运行 0 诊断；按 CI 精确版本复核 oxlint：本地 npx oxlint 解析为 1.89.0、CI 固定 1.88.0，用 npx --yes oxlint@1.88.0 frontend/src 重跑仍为 295 文件 0 warning 0 error（版本漂移未影响结论，是否同步固定待定）；根 package.json 仅新增 devDependencies，不入库无版本联动（frontend/package.json 保持 0.1.3，Cargo.toml 不动）

v0.19.3
2026102512112201
1. 审计工具链升到 latest 并固定：@biomejs/biome 2.5.15 → 2.6.3（根 devDependencies 精确固定 + biome.json $schema + ci.yml npx 版本 + docs 命令示例四处联动），oxlint 1.88.0 → 1.89.0（根 devDependencies 新增精确固定 + ci.yml + docs），本地与 CI 版本漂移消除
2. biome 2.6.3 安装踩坑：npmmirror 未同步 @biomejs/cli-linux-x64@2.6.3（404），npm 只装到 musl 变体并报 Cannot find module '@biomejs/cli-linux-x64/biome'；改用官方源 npm i --no-save --no-package-lock --registry=https://registry.npmjs.org @biomejs/cli-linux-x64@2.6.3 补齐，恢复 biome 2.6.3 可执行
3. 配置迁移：biome migrate --write 把 $schema 升到 2.6.3（迁移把 files.includes 压成单行，已按仓库风格手工恢复多行）；迁移后 check 22 文件 0 诊断，并用 .probe{color:red} 探针验证新版本确实在执行后删除
4. 前端 TypeScript 5.9.3 → 6.0.3（^6.0.3）：实测 typescript 7.0.2 与 vue-tsc 3.3.13 硬不兼容（vue-tsc 用 typescript/lib/tsc 定位编译器，TS 7 不再导出该子路径，直接 ERR_PACKAGE_PATH_NOT_EXPORTED），6.0.3 下 typecheck 与 build 均通过，具体版本边界写入文档
5. 其余前端依赖复查无需变更：npm outdated 仅 typescript 一行，@tanstack/vue-query 5.104.5、pinia 4.1.0、vue 3.5.44、vue-i18n 11.4.16、vue-router 5.4.0、@vitejs/plugin-vue 6.0.10、vite（lock 实装 8.3.5）、vue-tsc 3.3.13 均已是 latest；根 vue-tsc ^3.3.13 亦为 latest
6. 有意未动项：ci.yml 的 GitHub Actions 版本（checkout v4→v7、setup-node v4→v7、upload-artifact v4→v7、download-artifact v4→v8、action-gh-release v1→v3）跨多个大版本且涉及发布产物路径行为（注释依赖 v4 公共祖先规则），本地无法实跑验证，待单独一轮带冒烟升级；node-version 24 与 zig 0.13.0 为注释明确锁定项，保持
7. 文档：docs/code-style.md §3.5 命令版本更新，并新增三条约定——升级四处联动（根 devDependencies / $schema / ci.yml / 文档命令）、镜像缺包处置（官方源补装平台二进制）、TypeScript 版本边界（vue-tsc 与 TS 7 不兼容）
8. 验证：npm run typecheck 0 错（TS 6.0.3）；npm run build 通过（client 372.34 kB，无 chunk>500kB）；npm run selfcheck:viz 全过；biome 22 文件 0 诊断（本地 2.6.3 与 CI 等价命令 exit 0）；oxlint 1.89.0 在 frontend/src 295 文件与仓库根全量 299 文件均 0 warning 0 error；仅工具链与依赖变更，frontend/package.json version 保持 0.1.3，根 Cargo.toml 不动，无数据库变更

v0.19.4
2026102512280601
1. 版本策略定案：lint 工具继续精确固定版本，不用 @latest。理由——CI 的 lint 步骤是阻断门需可复现（上游发版不应让同一提交时红时绿），biome.json 的 $schema 只能写具体版本无法跟随 latest，latest 会把“定期升级”变成“随机构建中断”；真正痛点（版本散落多处、手工升级漏改）改用脚本解决。上一轮试改 latest 的两处改动已全部回退，无入库残留
2. 新增 da-scripts/upgrade-lint-tools.sh（可执行）：一键同步四处引用点——ci.yml 的 npx 版本、根 devDependencies、biome.json 的 $schema、docs/code-style.md 命令示例；因采用正则全量替换，对“某处漏改”的半改状态也能收敛；随后官方源安装本机二进制（缺平台包时自动补装 @biomejs/cli-<平台>）、探针验证（不合格式 CSS 必须被检出）、跑 biome check 与 oxlint，并提示大版本改动时的 migrate 事项
3. 新增 --check 模式：只校验四处一致性 + 本机二进制版本 + registry latest，不一致即 exit 1；实测人为把 ci.yml 改为 2.6.2、docs 改为 1.88.0 后被拦下（exit 1），upgrade 一键恢复为 2.6.3/1.89.0 且与漂移前备份逐字节一致（无副作用）；已最新时走幂等路径只校验不改写
4. ci.yml 注释同步：明确写明“版本有意固定（不用 @latest：阻断门需可复现）”，升级统一走脚本
5. da-scripts/release_check.sh 加固：fe_biome / fe_oxlint 改为优先调用 node_modules/.bin/ 下按 package.json 固定的二进制，缺失才回退 PATH 并告警，避免 PATH 上的同名旧包（biome@0.3.3 静默 exit 0）冒充检查
6. 文档：docs/code-style.md §3.5 新增脚本命令（upgrade / --check）与“版本策略”条目（固定精确版本 + 脚本治理漏同步 + release_check 优先本机固定二进制）
7. 验证：upgrade-lint-tools.sh --check exit 0（四处 2.6.3/1.89.0 一致）；upgrade 幂等路径 exit 0（探针验证通过、biome 22 文件 0 诊断、oxlint 299 文件 0/0）；release_check 两个前端函数实测通过（走 node_modules 二进制）；npm run build 通过（client 372.34 kB）；脚本均通过 bash -n / sh -n 语法检查；frontend/package.json version 保持 0.1.3，根 Cargo.toml 不动

v0.19.5
2026102512412401
1. 论证修正（不改决策）：此前把“本地 npx 缓存可能命中旧版本”当作反对 @latest 的理由不成立——@latest 的解析结果是 registry 的 dist-tag，同一时间点在不同机器/CI 上结果一致（镜像同步滞后除外）；固定版本命中缓存时内容不变（按版本号与 tarball 校验），缓存滞后属缓存问题，正确修法是刷新缓存而不是同步版本号。保留精确版本的实质理由收窄为两条：阻断门需跨时间可复现、biome.json 的 $schema 只能写具体版本
2. da-scripts/upgrade-lint-tools.sh：npm view 全部改为 npm view --prefer-online（packument 本地缓存会导致漏掉刚发布的版本，使“已是最新”判断失真），report_latest 与 upgrade 判定共三处
3. docs/code-style.md §3.5 新增“缓存与版本策略分开治理”条目：明确同一时间点一致性由 dist-tag 保证，本地与 CI 版本差异用 npx --prefer-online / npm cache verify / npm view --prefer-online 解决，不得改用浮动版本号
4. 验证：--check exit 0（四处 2.6.3/1.89.0 一致，registry latest 一致）；upgrade 幂等路径 exit 0（探针通过、biome 22 文件 0 诊断、oxlint 0/0）；bash -n 语法通过

v0.19.6
2026102513141201
1. 前端依赖声明改为 latest（vue / vue-router / pinia / vue-i18n / @tanstack/vue-query / @vitejs/plugin-vue / vite / vue-tsc）：实际解析版本未变（全部本就是 latest），入库的 package-lock.json 锁定实际版本，CI 走 npm ci 不影响可复现性；typescript 例外保留 ^6.0.3（实测 TS 7.0.2 与 vue-tsc 3.3.13 硬不兼容）
2. 主 workspace 升级到各 crate 最新稳定版：zip 8.6.0→9.1.0（major）、tokio 1.53.3→1.54.1、uuid 1.28.0→1.29.0、rustls 0.23→0.23.46、arc-swap 1.9.2→1.9.3、openidconnect 4.0→4.0.1、ts-rs 12→12.0.1、x509-parser 0.18→0.18.1（含 foims-x509-management 独立声明）、regex 1→1.13.1（dev）、rcgen 0.14→0.14.10（agent-service / common / x509-management 三处）
3. zip 9 破坏性变更适配 2 处：write::FileOptions 新增两个生命周期参数（原 FileOptions::<'_, ()> 编译失败）改为 write::SimpleFileOptions；ZipFile::name() 改为返回 Result<Cow<str>, ZipError>，在导入端 map_err 转 DataError 并沿用既有 i18n 键（新增注释说明）
4. foims-agent 独立 workspace 同步升级：tokio 1.53.2→1.54.1、serde_json 1.0.151→1.0.154、rustls 0.23→0.23.46、toml 1.1.6→1.1.8、uuid 1.27.0→1.29.0、x509-parser 0.18→0.18.1、rustix 1.1→1.1.5、rcgen 0.14→0.14.10（两处）、tempfile 3→3.27.0
5. foims-portal（已被 .gitignore 排除、不入库，仅本机部署）同步升级：anyhow、axum-server 0.7→0.8（0.x 跨 minor）、rustls、serde_json、tokio、toml、uuid；cargo check 通过，axum-server 0.8 无需改代码
6. 验证：cargo check 三棵依赖树全过；cargo fmt 已跑；cargo clippy --release --workspace --all-targets -D warnings 0 警告；cargo test --release --workspace 除 crypto::tests 14 项环境性失败（/etc/foims/encryption.key 属 foims:foims 0600、当前用户无读权限，与 release_check.sh 既有判定一致）外全过；ts-rs export_bindings 重跑后 frontend/src/types/api/generated 无漂移；npm ci --dry-run 通过（latest 声明与 lock 一致）；前端 typecheck/build/selfcheck:viz 全过
7. 文档：docs/code-style.md §3.5 新增“依赖声明策略”条目（前端 latest + lock 兼底、后端写具体版本 + Cargo.lock、三棵依赖树各自维护，与 lint 工具固定版本策略明确区分）；临时审计脚本用后即删，未入库
8. 版本：Cargo.toml 仅依赖版本变更，[package].version 与 frontend/package.json version 均不变（z 段普通修改）

v0.19.7
2026102513353801
1. TypeScript 升入 7 时代官方双安装（vuejs/language-tools PR #6123 方案，vue-tsc 3.3.13 支持）：frontend devDependencies 改为 typescript: npm:@typescript/typescript6@latest（TS 6 JS API 兼容层，实装 6.0.2；vue-tsc 的 resolveTscPath 检测到该别名后经其内部依赖 @typescript/old——实装 typescript 6.0.3——拿完整 TS 6 编译器供 typecheck/build 用）+ typescript7: npm:typescript@latest（TS 7.0.2 原生编译器）；scripts 新增 tsc 入口（node node_modules/typescript7/bin/tsc）让 tsc 命令稳定走 TS 7 原生——npm 对 .bin/tsc 同名冲突无优先级保证（实测被提升的 @typescript/old 的 tsc 6.0.3 抢占，故不用裸 .bin/tsc）
2. 安装踩坑：从 ^6.0.3 单装改为别名声明后直接 npm install 不重建 node_modules/typescript（残留 TS 6.0.3，lock 内 root 别名 spec 与节点条目不一致），需 rm -rf node_modules package-lock.json 干净重建；最小复现确认别名方案在 npm 11.19.1 下本身正常
3. 文档：docs/code-style.md §3.5「TypeScript 版本边界」改写为「TypeScript 7 双安装」（含 .bin/tsc 抢占与 TS 7.1 后收敛条件），新增 npm run tsc 命令示例，依赖声明策略中 typescript 例外条目移除（别名同样走 latest + lock 兼底）
4. 验证：npm run tsc → Version 7.0.2；vue-tsc 解析链路实测 typescript→@typescript/typescript6→@typescript/old/lib/tsc.js（TS 6.0.3）；npm run typecheck 0 错；npm run build 通过（client 372.30 kB）；npm run selfcheck:viz 全过；frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更

v0.19.8
2026102515021101
1. CSS 规范工具标准落地（z 段）：biome.json 的 linter.rules.complexity 新增 noImportantStyles=error，把 !important 禁令从「人工约定」升格为工具强制（该规则归属 complexity 域，非 style 域）；启用后回归 biome check frontend 原 22 文件 0 违规
2. CSS-style.md 按审计勘误修订：顶部优先级声明拆为「工具强制（层序/noImportantStyles/2 空格排版/recommended）/评审强制（属性顺序/命名/嵌套/颜色值）」两档效力来源；明示媒体查询条件必须硬编码 px 值（CSS 变量不能用于 @media，不生成 --breakpoint-* 变量，修正开头「断点走 CSS 变量」旧表述）；补充 init.css 入口级 reset 豁免说明（独立入口不加载全局 reset.css，属既定例外）；全档标注 Biome files.includes 仅覆盖 frontend/src/styles/**、不含 .vue 的 <style> 块，scoped 块禁 @layer 靠评审保障；!important 措辞第五/十一/十二节同步为工具强制
3. utilities 层废除（z 段）：全库检索 @layer utilities 实现块零使用（旧 web-archive 遗产占位，同名片段已就地移植），从 variables.css 头部层序声明移除，新层序 variables→reset→components→modals→layouts→pages→responsive；同步修正引用旧层序的注释（entries/main.ts 头注释、CSS-style.md 层序与目录清单）
4. 两个非 scoped <style> 块归位：SubnetUsageModal.vue 的 usage/ipv6 前缀类实测全部仅命中本组件模板（ModalShell 无 Teleport，表格行为 v-for Vue 渲染），改 <style scoped> 消除 .table/.ipv6-address/.no-ipv6-info 等通用类名与后代裸标签选择器的 unlayered 全局泄漏；OrganizationPage.vue 的 org-* 类被递归子组件 OrgTreeNode.vue/TemplateTypeNode.vue 跨组件命中（scoped 无法穿透），整体上收至新建全局 styles/pages/organization.css（@layer pages，entries/main.ts 追加导入），org-fade-in keyframes 随迁，SFC 减重 561 行并入 Biome 兜底范围
5. 版本：本次为纯前端样式与配置修订（无后端载荷变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；验证 biome check frontend 23 文件 0 违规、npm run typecheck 0 错、npm run build 通过

v0.19.9
2026102514190601
1. 前端重构完成度核实：frontend/src 共 212 个 .ts + 83 个 .vue（83/83 全部 <script setup lang="ts">）+ 23 个 .css，无任何 .js 源码残留；三入口 HTML 均指向 /src/entries/*.ts，i18n 语言包已入库 src/i18n/locales/（zh/en），确认原生 JS → Vue 3 + TS 重构已全量完成
2. 前端注释去旧版化（z 段，约 230 处/150+ 文件）：清除「移植自旧版 modules/xxx.js」「对应旧版 main.html NNN 行」「旧版 X 语义」等考古性溯源注释，统一改写为基于当前代码逻辑的行为描述（pages/components/composables/utils/config/api/router/entries/i18n/styles 与 vite.config.js）；与事实不符的注释同步纠偏（i18n/index.ts 语言包位置、api/client.ts 改直陈当前契约、ConfirmModal/LogDetailsModal 样式来源表述、visualization.css/main.html 挂载描述、tables.css 列宽对应模板等）
3. 合理保留：代码标识符 legacy 系列（legacyRegionList/registerLegacy 表“失联网段数据”、.org-icon-legacy 表遗留图标形态、vue-i18n legacy:false 库 API）不动；web-archive/ 只读存档不动
4. 版本：纯注释修订，frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无后端载荷变更无需再生成 TS 类型；验证 npm run typecheck 0 错、npm run build 通过、biome check frontend 23 文件 0 违规，grep 旧版/web-archive/原生 JS/存档 引用归零

v0.19.10
2026102515335801
1. 模态框页脚按钮样式全局统一为独立色彩变体类写法（btn-primary 等，不带 btn 基类）：此前全库存在两种写法（资源管理 network/room/cabinet/workstation 与 org/system 密码类弹窗用变体独用，设备/线路/可视化/agents/ip 等弹窗用 btn+变体双写），页脚布局样式仅靠 modals.css 的 `.modal-footer .btn` 与 buttons.css 的 `.btn` 基类命中双写方，独用方退化为浏览器默认按钮外观；本次将 18 个弹窗组件（device 5、system 6、cable 2、visualization 3、agents 1、ip 1）共 34 处页脚按钮类名 `btn btn-*` 改为 `btn-*`，全库页脚按钮归一
2. modals.css 页脚按钮基座重写：`.modal-footer .btn` 改为显式枚举 `.modal-footer .btn-primary/.btn-secondary/.btn-danger/.btn-info`，补齐 border/cursor/display:flex/对齐/height:var(--btn-height)/box-shadow 等原由 btn 基类提供、去掉基类后会丢失的视觉属性，并补 `:hover` 位移（原依赖 `.btn:hover`）；新增 `.modal-footer .btn-sm` 小档覆盖（含 ≤768px 压缩，命中连接详情弹窗）；变体色彩/hover 换色仍由 buttons.css 提供不重复
3. 范围收敛：仅改模态框页脚（`<template #footer>`/ConfirmModal 自建 modal-footer）按钮，页面正文与卡片内 `btn btn-*` 工具栏按钮（system 各 Tab/Card、DataManagementTab 等）与 CIDR 动态行 `btn btn-icon-sm` 图标按钮不在页脚基座选择器命中范围，保持原写法不动；登录/初始化入口的 login-btn-*/init-btn-* 独立家族不涉及
4. 版本：纯前端样式修订（无后端载荷变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；验证 npm run typecheck 0 错、npm run build 通过、biome check frontend 23 文件 0 违规、oxlint 295 文件 0 警告 0 错误

v0.19.11
2026102514523901
1. 网络区域弹窗主体布局收齐至资源管理模块「同行表单变体」范式（上轮页脚按钮统一时未覆盖的遗漏项）：NetworkRegionModal 此前是模块内唯一未采「标量字段入 .form-row 网格 + label 左定宽 + 控件右 flex:1」的弹窗——名称字段为裸 .form-group（label 100px 居中、无 form-row 的 margin-bottom 行距节奏）、IPv4/IPv6 CIDR 为 .cidr-column「标题在上整列居中」堆叠双列（与同子模块 NetworkModal 的 IPv4/IPv6 同行布局分属两套语法，三块内容左缘参差为「不协调」主因）；现名称并入独立 .form-row（label 基准 80px 与其余行同源），IPv4/IPv6 各占一整行 .cidr-group（左标题 + 右动态输入列表），描述块 textarea 维持 form-group-block 不动；ModalShell 加 size="md" 与 NetworkModal 宽度档对齐
2. forms.css CIDR 样式块重写：废弃堆叠式 .cidr-column 一组规则（flex-direction:column + 标题 max-width:320px 左对齐），改为 .cidr-group 同行基座（align-items:flex-start 顶对齐多行列表、group-label 覆盖为 width:auto/min-width:80px/上内边距取 --control-padding-y 与首行输入框取齐、cidr-input-list flex:1 撑满右侧）；.cidr-input-list/.cidr-input-row/.cidr-input/.cidr-btn-group 行内结构与 +/− 图标按钮不变；.cidr-* 类仅 NetworkRegionModal 单点使用，无其它组件回归风险
3. 验证 grep 全库 cidr-column 零残留；版本：纯前端布局修订（无后端载荷变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；npm run typecheck 0 错、npm run build 通过、biome check frontend 23 文件 0 违规、oxlint 295 文件 0 警告 0 错误

v0.19.12
2026102515035401
1. 网络区域弹窗 CIDR 区改回双列（用户反馈修正 v0.19.11 的方向选择）：IPv4 CIDR / IPv6 CIDR 回到同一 .form-row 网格下左右两列、标题在上整列居中的堆叠式（.cidr-group 单列全行语法废弃，恢复 .cidr-column 类与 forms.css 对应规则）；名称字段保留上轮成果仍居独立 .form-row
2. size="md" 回退为默认档：经计算 form-row 两列需 280×2+16(gap)=576px，md 弹窗 max-width 600px 扣除 .modal-body 左右 0.75rem 内边距后仅余 576px 属临界值（auto-fit 会回落单列且亚像素环境下不确定），默认档 640px 内部 616px 才能稳定落两列，宽度统一目标让位于双列需求
3. 验证：grep 全库 cidr-group 零残留；版本：纯前端布局修订（无后端载荷变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；npm run typecheck 0 错、npm run build 通过、biome check frontend 23 文件 0 违规

v0.19.13
2026102515132201
1. 网络区域弹窗 CIDR 区最终形态：两列 × 同行内联（用户反馈 v0.19.12 的「标题在上整列居中」堆叠式视觉上回到初始不协调状态）：在 v0.19.11 已认可的「同行表单变体」基座（标题左定宽居中 + 动态列表右 flex:1 + 顶对齐）上把 IPv4/IPv6 两个 .cidr-group 同置于一个 .form-row，靠 auto-fit minmax(280px,1fr) 网格自然分栏成左右两列，与子网弹窗 NetworkModal 的两列同行布局同构；最终结构为名称一列（整行）、CIDR 两列、描述整行
2. forms.css 再次重写该块：删除 .cidr-column 堆叠规则（flex-direction:column/align-items:center/标题 max-width:320px 左对齐），恢复 .cidr-group 内联基座三组规则；.cidr-input-list/.cidr-input-row/.cidr-input/.cidr-btn-group 行内结构与 +/− 图标按钮不变；标题取 min-width:80px 与 form-row 内 label 同源，宽度档保持默认（640px，body 内部 616px，双列需 576px 可稳定成立；md 档 600px 属临界不采用）；本轮回溯修正 v0.19.11/v0.19.12 两次方向试探，最终收敛为「同行内联 + 两列」
3. 版本：纯前端布局修订（无后端载荷变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；grep 全库 cidr-column 零残留；npm run typecheck 0 错、npm run build 通过、biome check frontend 23 文件 0 违规、oxlint 295 文件 0 警告 0 错误

v0.19.14
2026102515284101
1. 系统模块各子模块卡片布局统一至资源管理/表单范式（字段栅格归一）：容器层经核实已统一（各卡片均用 config-card/config-card-header/config-card-title/icon）；真正的遗留不协调在于部分卡片字段区用了设备网卡编辑器专用体系 network-card.css 的 .nc-fields/.nc-field（标签在上堆叠式、.nc-input 控件）而非模块统一的 form-row/form-group 同行表单变体；本次转换三个偏离组件：SecurityTab（限流 7 字段 + 密码策略 2 输入 + 4 复选，共 13 nc-field → form-row/form-group，复选沿用 AgentServiceCard 已有的 form-group+label.checkbox-label 范式）、Fail2banCard（配置 4 nc-field → form-row/form-group）、LogsManagementCard（日志转发 4 nc-field → form-row/form-group，其 clear-logs-form 此前已是 form-row）；转换后统一为控件补 :aria-label 与 SystemConfigTab 范例对齐；nc-field 系统目录全域清零
2. 范例核对与外层包裹统一：SystemInfoTab（form-row + dl.config-field 只读字段）、UsersTab/ScheduledTasksTab（tab-content > table-container > data-table）已完全符合资源管理范式无需改动；NotificationTab 外层包裹类从孤例 .notification-config-section 改为与其他页签一致的 .system-config-section（二者原本同一 ruleset 样式等价，属命名不一致），同步删除 system.css 中三处冗余的 .notification-config-section 选择器
3. 范围收敛：Fail2banCard 手动封禁行（input + 封/解按钮）为特例动作控件非配置字段栅格，本轮不动；ServicesCard/CertificateCard 为表/动作区无表单字段、AgentServiceCard/SnmpTrapCard/SystemConfigTab 已用 form-row，无需转换；.form-checkbox 无样式定义但为模块现有 checkbox 标记惯例（AgentServiceCard 已用），保留以同构
4. 版本：纯前端布局与样式修订（无后端载荷变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；grep 全库 nc-field（系统目录）与 notification-config-section 零残留；npm run typecheck 0 错、npm run build 通过、biome check frontend 23 文件 0 违规、oxlint 295 文件 0 警告 0 错误

v0.19.15
2026102515434101
1. 清理前端死样式（全库「定义 class vs 活动应用引用」扫描，src/**/*.vue|ts|js + 根 html 入口为准，排除已归档自带独立 CSS 的 web-archive）：删除 9 个在活动应用零引用的死类——agents.css 的 .agent-spark-box、toasts.css 的 .app-fatal-error 整块（含注释，原 web-archive app.js 注入）、network-card.css 的 .nc-address-group .auto-assign-ip-btn、buttons.css 的 .icon-btn svg.combo-icon 与 .icon-btn .icon-badge 与 .help-btn 整块（含注释，原 helpManager.js）、system.css 的 5 处 .database-management-container co-selector（仅删该选择器，保留在用的 .data-management-container）、login.css 的 .login-modal-footer .login-btn-secondary 三处（仅删该类，保留在用的 .login-btn-primary）、tables.css 的 .mac-lldp-text-syncing；同步删除 modals.css 中引用已废弃 .modal-header .help-btn 的过时注释
2. 假阳性甄别与保留：modal-sm/md/lg/xl（ModalShell 动态 :class="modal-${size}"）、toast-*（AppToast toast-${type}）、status-*（UnifiedDevicePortsModal/WorkstationCanvas 动态拼接）、type-*（TopologyCanvas type-${device_type}）、nav-*（MainShell nav-${item.name}）均为运行时拼接、字面 grep 匹配不到，全部保留；.w3 系误匹配 SVG data-URI 内的 http://www.w3.org 非类名；init-body（init_index.html）、login-body（index.html）在 HTML 入口在用，保留
3. 验证：grep 9 个死类在 src/styles 定义数全部归零，在用同族类（data-management-container 8、login-btn-primary 4）保留完好；版本：纯前端死样式清理（无后端载荷变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；biome check frontend 23 文件 0 违规、npm run build 通过

v0.19.16
2026102516133601
1. 系统页卡片布局调整（清单第 1 项）：数据管理 tab 内 AgentServiceCard（“Agent 配置”）的“Agent 服务配置”段（section_service）内容由单列竖排改为固定两栏。该段处于 .data-management-container（1fr 1fr）的半宽 section 内，内部一个 .form-row 的 4 个字段（监听地址/端口/上报间隔/采集间隔）因 auto-fit minmax(280px,1fr) 在窄容器中回落为单列；新增可复用工具类 .form-row.form-row-2col（grid-template-columns: repeat(2, minmax(0,1fr))，特异性 0,2,0 覆盖 .form-row 基样式 0,1,0；minmax(0,1fr) 防窄列内容撑爆），仅挂在服务配置那一 form-row，不影响 Agent 配置段（section_agent）与其他卡片
2. 版本：纯前端布局修订（无后端载荷变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；npm run typecheck 0 错、npm run build 通过、biome check frontend 23 文件 0 违规

v0.19.17
2026102516241301
1. SNMP Trap 接收卡片 v3 USM 用户表“添加用户”按钮位置调整（清单第 2 项）：从表格上方标题行移到所属表格删除按钮列的标题行（表头单元格）；沿用既有表头动作单元格范式 .data-table th.th-inline-action（IpPage 已用，含 btn-sm 覆盖与保持 table-cell 不破坏 fixed 列宽的注释），th v-if="!readonly" 内放 btn btn-secondary btn-sm
2. 标题行包裹层清理：原 .group-label-row（仅此一处使用、无任何 CSS 定义）是个裸包裹层，使 system.css:173 的直接子选择器 `.form-group-block > .group-label`（注释已点名“SNMP v3 用户表”、标题左对齐取齐正文）一直未命中；去掉该包裹层后 group-label 成为 form-group-block 直接子元素，恰好套用既定块标题样式；grep 全库 group-label-row 零残留
3. 列宽平衡：删除列原按 2 字“删除”设为 10%，现表头放 4 字“添加用户”，为避免 fixed 布局横向溢出（snmp-trap-users-container 注释明确防横滚）将第 6 列 10%→14%，从两个密码列（nth-child(3)/(5)）各让 2%（20%→18%）保持列宽和 100%
4. 版本：纯前端布局与样式修订（无后端载荷变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；npm run typecheck 0 错、npm run build 通过、biome check frontend 23 文件 0 违规

v0.19.18
2026102516284401
1. 日志管理卡片按钮上移到模块标题行右侧（清单第 3 项）：“日志清理”模块的“清理”按钮与“日志外发设置”模块的“测试/保存”两按钮，从各自表单区下方移到对应 h3 标题行的右侧；新增可复用块标题行范式 .data-management-container .section-title-row（display:flex + justify-content:space-between + align-items:center，分隔线下划到行容器以占满整宽、`> h3` 去除自身 border-bottom/margin-bottom）；清理按钮同时由 btn btn-danger 降为 btn-sm 与标题带高度协调（与项目 config-card-header 右侧动作统一 btn-sm 一致）；外发两按钮复用已有 .action-buttons（flex+gap）作为标题行右侧按钮组
2. 配套修正：“清理”按钮移出后 .clear-logs-form 内 form-row 变为尾子元素，其 margin-bottom 由 1rem 改为 0（避免卡片底部多余留白）；stats 标题（logs.stats）无按钮保持普通 .section h3 不变，三处分隔线宽度一致
3. 版本：纯前端布局与样式修订（无后端载荷变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；npm run typecheck 0 错、npm run build 通过、biome check frontend 23 文件 0 违规

v0.19.19
2026102516305001
1. 日志管理卡片“日志外发设置”模块（logs.forwarding_title）内容改为固定两栏（清单第 4 项，与第 1 项同类）：该模块在 .data-management-container（1fr 1fr）的半宽 section 内，内部一个 .form-row 的 4 个字段（启用/协议/主机/端口）因 auto-fit minmax(280px,1fr) 回落单列；直接复用 v0.19.16 新增的工具类 .form-row.form-row-2col（仅挂类，无新增样式），不影响清理模块与其他卡片
2. 版本：纯前端布局修订（无后端载荷变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；npm run typecheck 0 错、npm run build 通过、biome check frontend 23 文件 0 违规

v0.19.20
2026102516364701
1. SNMP Trap 卡片字段标签文案修正（用户反馈“v1/v2c Community 白名单”描述不合理）：snmp_trap.communities 标签英文由“v1/v2c Community Whitelist”改为“v1/v2c Community”、中文由“v1/v2c Community 白名单”改为“v1/v2c 团体字”（仅改 i18n en.json/zh.json 两处标签值，不改键名与组件）
2. 未改动说明：卡片帮助文（guide.snmp_trap）中仍以“v1/v2c Community 白名单/Whitelist”描述该字段的白名单机制（属于概念性解释而非字段标题），本次未改动，如需术语同步可再确认；设备 SNMP 的 snmp_community（“Community字符串/Community String”）为不同上下文，未动
3. 版本：纯前端文案修订（无后端载荷变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；npm run build 通过、biome check frontend 23 文件 0 违规

v0.19.21
2026102516395201
1. SNMP community 术语全局统一（接 v0.19.20，用户要求“全局调整”）：确定单一口径——英文统一为 Community（去掉 String/Whitelist）、中文统一为团体字。改动均为 i18n 展示文案（不改键名）：① 设备 SNMP device.snmp_community 英“Community String”→“Community”、中“Community字符串”→“团体字”；② 校验提示 device.snmp_community_length 英去“string”、中“SNMP团体字符串…”→“SNMP团体字…”；③ SNMP Trap 帮助文 guide.snmp_trap 中英文去掉“Community Whitelist/Community 白名单”的白名单框定，改为“v1/v2c Community/v1/v2c 团体字”并保留“留空拒绝、需至少配置一个”的机制描述（与上轮已改的 snmp_trap.communities 字段标签口径一致）
2. 同步修正：SnmpTrapCard.vue 顶部文档注释“团体名白名单”→“v1/v2c 团体字”（注释基于当前逻辑）；未动项：src/types/api/generated/ 生成物类型与内部注释、DeviceModal 的 snmpCommunity 等代码标识符（非展示文案，禁手改生成物）；grep 确认 SNMP 语境旧术语（Community String/白名单/Whitelist/团体字符串/Community字符串）零残留
3. 版本：纯前端文案与注释修订（无后端载荷变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；npm run build 通过、biome check frontend 23 文件 0 违规

v0.19.22
2026102516482601
1. SNMP community 术语再校正（用户反馈：英文用标准 “string”、中文经考证用“团体名”更标准）：接 v0.19.21 将口径改为——英文统一为 Community String（补回 String）、中文统一为团体名。覆盖设备与 Trap 两处 i18n 展示文案（不改键名）：① device.snmp_community 英“Community”→“Community String”、中“团体字”→“团体名”；② snmp_community_length 英补回“string”、中“SNMP团体字…”→“SNMP团体名…”；③ snmp_trap.communities 标签英“v1/v2c Community”→“v1/v2c Community String”、中“v1/v2c 团体字”→“v1/v2c 团体名”；④ guide.snmp_trap 英“v1/v2c Community…at least one community…”→“v1/v2c Community String…at least one community string…”、中“团体字”→“团体名”
2. 注释与生成物同步：DeviceModal.vue/SnmpTrapCard.vue 顶部文档注释“团体字”→“团体名”；因 generated/DeviceUpdate.ts 的“SNMP 团体字”源自后端 #[derive(TS)] 字段文档注释（禁手改生成物），改后端 crates/foims-models/src/models/device.rs:522 snmp_community 的 `/// 双层 Option：null 清空 SNMP 团体字`→“团体名”，再跑 bash da-scripts/gen-ts-types.sh 重生生成物同步；grep 全库（frontend/src + crates + src）团体字 零残留，git diff 仅 7 个预期文件无意外漂移
3. 版本：后端为文档注释修订（无功能/结构/载荷/DB 变更）、前端文案与注释修订；cargo fmt rc=0、cargo test export_bindings 全 workspace 编译通过；frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动，无数据库变更；npm run build 通过、biome check frontend 23 文件 0 违规

v0.19.23
2026102517240801
1. 设备模态框网卡编辑器 IP 容器删除“描述”字段（NetworkCardEditor.vue）：移除 IP 行模板末尾的 .nc-field（common.description 标签 + v-model=ipRow.description 的文本输入框，nc-ip-desc-* ），仅保留网络区域/网络名称/IP 地址三个字段
2. 保留数据通路说明：ipRow.description 仍由 loadCardData 回填并在提交载荷中带回 description——IP 同步为整体替换语义，若连同 payload 一起删除会让后端已存的既有 IP 描述在下次保存时被静默置 null（数据丢失），故本次仅移除 UI 输入控件不改保存语义；.nc-field 与 common.description 均为其他字段复用的共享类/文案，无死样式残留
3. 版本：纯前端 UI 修订（无后端载荷/结构/DB 变更），frontend/package.json version 保持 0.1.3（z 段普通修改），根 Cargo.toml 不动；npm run typecheck 0 错、npm run build 通过、biome check frontend 23 文件 0 违规

v0.20.0
2026102517382001
1. 删除 IP 描述字段（interface_ips.description）全链路下线（接 v0.19.23，用户要求“从后端代码和数据库中删除”）：数据库结构变更属功能级 y+1（z 归零 v0.19.23→v0.20.0）
2. 数据库：执行 upgrade/0.30/upgrade_0.30.0.sql（0.30 中版本唯一一份，幂等）——DROP VIEW ip_with_details → ALTER TABLE interface_ips DROP COLUMN description → 重建视图（列序同旧版仅去 description）；已对开发库 localhost:5432/foims 执行，复核列与视图均无 description
3. 后端：foims-models/models/ip.rs 从 6 个 IP 结构删 description 字段（IpDetail/IpDetailWithNames/IpDetailCreate/IpSyncItem/AutoAssignIpRequest/IpDetailUpdate，保留 Port/NetworkCard 自身 description）+ 删除 test_ip_detail_create_description_too_long、修正其余相关测试；foims-resource/ip.rs 删除搜索子句 description ILIKE、ip_with_details SELECT 列、3 处 interface_ips INSERT（列名去 description 且绑定参 $6..$10 重编为 $6..$9、删对应 .bind）、pull 两处 SELECT m.description、create/auto_assign/batch 三处响应 IpDetail 字面量与审计日志 json 的 description；nic.rs 同步网卡 IP INSERT 重编号与 SELECT、workstation.rs/position.rs IP 子查询删 m.description；foims-data-management/spec.rs 删 interface_ips 的 Col::Plain("description")；snmp.rs:748 的 description 属 SnmpPort（网口）未动
4. foims-init：schema/tables/ips.rs 建表删 description TEXT、schema/tables/views.rs 视图删 imm.description、check.rs interface_ips 列清单删 "description"
5. 前端：NetworkCardEditor.vue 删除 IP 行六处 description 通路（IpRowState 字段/emptyIpRow/loadCardData 回填/validate 全空行判定与 ipDescription 局部量/提交 payload）；bash da-scripts/gen-ts-types.sh 重生 6 个 IP 生成物均已无 description（IpDetail/IpDetailWithNames/IpDetailCreate/IpSyncItem/AutoAssignIpRequest/IpDetailUpdate）
6. 联动：根 Cargo.toml 0.30.0→0.30.1（y+1 对应 c+1），foims-models 0.2.11→0.2.12、foims-resource 0.1.10→0.1.11、foims-data-management 0.2.5→0.2.6、foims-init 0.1.9→0.1.10（cargo update --workspace --offline 同步 Cargo.lock）；本次为后端驱动的功能删除，前端仅随之清理数据通路（非独立前端功能变更），frontend/package.json version 保持 0.1.3；验证 cargo clippy --release --workspace --all-targets -D warnings 0 告警、cargo test -p foims-models 314 passed、npm run typecheck 0 错、npm run build 通过、biome check frontend 23 文件 0 违规

v0.21.0
2026102610300001
1. SNMP 第一层观测事件与告警接入（新增独立 crate foims-snmp + 6 项功能链路），模块级 x+1：① 第 0 步将寄居 foims-resource/foims-agent-service 的 SNMP 协议层与采集层纯移动抽出为 crates/foims-snmp（core 协议/config/collect lldp·arp·ports·metrics/handlers），依赖仅 common+models+auth，不留 re-export 垫片，更新引用路径并移除旧模块；验收 cargo check --workspace + cargo test 全绿（零行为漂移门槛）
2. 数据库：新建 snmp_events 表（foims-init/schema/tables/snmp_events.rs + check.rs 列校验 + upgrade/0.31/upgrade_0.31.0.sql 幂等建表与索引），已对开发库 localhost:5432/foims 执行并复核（8 列、双索引、severity CHECK、device_id FK ON DELETE CASCADE）
3. foims-models：新增 snmp_event.rs——6 事件类型常量（link_flip/device_reboot/arp_added/arp_removed/arp_conflict/unknown_endpoint）+ 3 级别常量 + SnmpEvent/SnmpEventQuery/DeviceLocateResult（FromRow + TS）
4. foims-snmp/events：record_events 批量落库 + 关键事件逐条 create_notification（i18n-key 机制，失败仅日志、事件表为事实源）、GET /api/snmp/events 分页查询、GET /api/snmp/locate 终端定位（MAC 归一 + ARP/static UNION ALL、registered 判定）、diff_arp_entries 纯函数（基线/新增/移除/冲突/未知端点判定）；collect/notify helper 自 foims-resource 下放；routes/mod.rs 注册两条 /api/snmp GET
5. foims-snmp/collect/metrics：IfaceCounters 增错误/丢弃计数、SnmpState 增 if_status/if_admin/last_boot_uptime/last_poll_ts、NetRate 增 status/admin_status/speed_mbps/错误丢弃增量（serde default 兼容旧状态 JSON）；collect_nets 采错误计数器（标准 IF-MIB 列 inErrors/outErrors/inDiscards/outDiscards）；新增纯函数 detect_link_flips（基线降噪 + admin-down 标注）、detect_reboot（uptime 回退估算停机时长）并补中文命名单测；snmp_poll.rs 采集后产出事件经 record_events 落表、成功 upsert 后接入 alerts::evaluate_report 阈值告警；alerts::evaluate_all 纳入 source='snmp'；tasks.rs 同清理任务追加 snmp_events 保留期清理（system_configs snmp_event_retention_days 缺省 30、范围 1-3650）
6. foims-auth：NotificationMethods 增 snmp_event 邮件开关（default true）+ send_unknown_endpoint_email；后端 i18n zh.yml/en.yml 补 log.snmp.*（event_recorded/event_record_failed/notification_failed/unknown_endpoint_email_failed/locate_query/events_cleanup_completed）与 log.task.agent_snmp_poll_failed
7. 前端：新增 components/device/SnmpEventsModal.vue（分页 + 设备/类型/级别/时间范围筛选、severity 徽标）、LocateModal.vue（MAC/IP 切换查询、登记/来源徽标，ArpModal 逐行“定位此终端”预填 MAC）；AgentDetailModal.vue SNMP 主机“端口流量与错误”改为表格（网口/状态/速率/rx/tx/本轮错误与丢弃增量，读 raw_metrics.nets）；NotificationTab.vue 通知方式矩阵补 snmp_event 行；locales 补 device.snmp_events.*/device.locate.*/agents.nets_table.*/server.notification.snmp_event.*/server.snmp.*/server.task.snmp_event_retention_invalid；bash da-scripts/gen-ts-types.sh 重生 SnmpEvent/SnmpEventQuery/DeviceLocateResult/NotificationMethods
8. 联动：根 Cargo.toml 0.30.1→0.31.0（x+1 对应 b+1、c 归零）；新 crate foims-snmp 0.1.0；存量 crate bump：foims-agent-service 0.1.16→0.1.17、foims-resource 0.1.11→0.1.12、foims-models 0.2.12→0.2.13、foims-init 0.1.10→0.1.11、foims-auth 0.1.5→0.1.6（cargo update --workspace --offline 同步 Cargo.lock）；前后端混合修改，frontend/package.json 0.1.3→0.1.4；验证 cargo clippy --release --workspace --all-targets -D warnings、cargo test、npm run typecheck/build、biome check 全绿

v0.22.0
2026102612000001
1. SNMP 第二层网络侧清单/台账（ENTITY/VLAN/CDP/LAG/PoE 五子系统，按需同步 + 关键变化产事件），模块级 x+1：范围仅网络侧 5 项，混合触发——5 类台账均每设备 POST .../sync（沿用 lldp/sync 的 upsert + 过期清理 + 逐行 SAVEPOINT 事务模板），其中 PoE 供电中断与 CDP 邻居增删在同步时对新旧行做纯函数 diff 产出 snmp_events（与第一层 macs/sync 的 diff_arp_entries 同构，不改 5 分钟轮询路径）；ENTITY/VLAN/LAG 为配置型台账只落表不发事件；非目标：主机侧 MIB、端口级历史曲线
2. 数据库：新增 6 张当前态表（foims-init/schema/tables/device_inventory.rs：device_entities/device_sensors/device_vlans/device_cdps/device_lags/device_poe_ports，均 device_id FK ON DELETE CASCADE + created_at/updated_at + 各自 UNIQUE 自然键）+ tables/mod.rs 注册与 create 汇总 + check.rs 表/列清单登记 + upgrade/0.32/upgrade_0.32.0.sql（幂等 CREATE TABLE IF NOT EXISTS）
3. foims-models：新增 device_inventory.rs——6 数据库行模型（DeviceEntity/DeviceSensor/DeviceVlan/DeviceCdp/DeviceLag/DevicePoePort，FromRow + Serialize + TS）与 6 采集原始记录（*Record，仅 SNMP 可得列，不含 id/device_id/时间戳，JSONB 成员端口列用 serde_json::Value）；snmp_event.rs 追加 SNMP_EVENT_POE_OFF/CDP_ADDED/CDP_REMOVED 常量
4. foims-snmp/collect：新增 entity.rs（entPhysicalTable + entSensorTable，含 custom 双弧 walk）、vlan.rs（dot1qVlanStaticTable，复用 port_list_members 位图解析经 dot1dBasePortIfIndex 换算 ifIndex）、cdp.rs（cdpCacheTable + diff_cdp_neighbors 纯函数与单测）、poe.rs（pethPsePortTable + diff_poe_power 纯函数与单测）、lag.rs（ifType=161 聚合口 + dot3adAggPortAttachedAggID 成员映射）；ports.rs 的 walk_index_value_map/value_as_i32/port_list_members 由私有改 pub(crate) 供复用；vendor 私有增强 OID 缺失一律降级 NULL、walk 失败记 log_warn 空集不阻断
5. foims-snmp/events：should_notify 纳入 POE_OFF（CDP 增删 info 只落表不通知）、notification_body_key 增 poe_port_off→body_poe_off、notify_event params 补 POE 分支（device/interface/class）
6. foims-resource/device：新增 entity/vlan/cdp/lag/poe.rs（get_device_* 读库 + sync_*_from_snmp 落库，逐字套用 lldp.rs 模板；cdp/poe 同步读旧行→diff→record_events 失败仅日志）；device/mod.rs 注册并 pub use re-export（含补漏的 poe）
7. 路由（src/routes/mod.rs，接 lldp 之后）：5 子系统各 GET 读库 + POST .../sync（entities 额外 GET .../sensors），共 11 条；/api/resources 非 GET 已被 admin_guard 判为 admin、GET 不守卫；不新增向任意目标发包的 GET 探测端点
8. i18n：后端 src/i18n/zh.yml/en.yml 补 log.device.{cdp,entity,lag,poe,vlan}.record_write_failed、log.device.entity.sensor_write_failed、log.device.snmp.{cdp,entity,lag,poe,sensor}_walk_failed；前端 locales 补 server.message.device.{cdp,entity,lag,poe,sensors,vlan}.*、server.message.device.snmp.*_fetch_failed、server.notification.snmp_event.body_poe_off，以及 device.{entity,sensors,vlan_ledger,cdp,lag,poe}.* UI 文案
9. 前端：新增 components/device/{EntityModal,VlanLedgerModal,CdpModal,LagModal,PoeModal}.vue（仿 LldpModal 状态机 + 同步按钮，PoE detection_status 徽标配色，VLAN/LAG 成员 JSONB 数组按 number[]/string[] 收窄）；ResourcesPage.vue 设备行操作区加 5 图标按钮入口 + 5 模态框挂载；config/icons.ts 补 listE/listV/listC/listG 字母角标图标
10. 联动：根 Cargo.toml 0.31.0→0.32.0（x+1 对应 b+1、c 归零）；crate bump foims-snmp 0.1.0→0.1.1、foims-resource 0.1.12→0.1.13、foims-models 0.2.13→0.2.14、foims-init 0.1.11→0.1.12（cargo update --workspace --offline 同步 Cargo.lock）；前后端混合修改，frontend/package.json 0.1.4→0.1.5；bash da-scripts/gen-ts-types.sh 重生 DeviceEntity/DeviceSensor/DeviceVlan/DeviceCdp/DeviceLag/DevicePoePort

v0.22.1
2026102600493601
1. foims-snmp 全量审计修复（高危 A1-A3/中危 B1-B7/低危 C1-C10，均后端 z 级）：
① A1 ARP 冲突链打通——采集去重改为 (ip,mac) 二元组（此前 seen_ips 按 IP 去重使同 IP 多 MAC 第二帧被丢、冲突永不可达）、diff 冲突事件排序去重且同 IP 仅报一条、通知模板新增 macs 多 MAC 拼接（events/mod.rs macs_field）；② A2 空 ARP 表陈旧清理——mac.rs 去除 `!seen_ips.is_empty()` 守卫，空表时 NOT IN 恒真全清；③ A3/B1 凭据层——空串凭据归一为 None（normalize_blank）、v3 隐私已配但认证缺失显式报错；④ B2 计数器回绕位宽分道（iface_rate_bps：仅 prev≤u32::MAX 才按 2^32 校正，Counter64 清零判 None 防下溢/虚假峰值）；⑤ B3 异常字面量清洗（core 新增 is_exception_literal/clean_snmp_text，劣质固件把 NoSuchObject 等写成 DisplayString 逃过 is_exception，ports/arp/sysinfo/cdp/vlan/lldp/lag/poe/entity 入口统一丢弃）；⑥ B5 CPU 口径排除 net-snmp ssCpuRaw 重叠项（Kernel/Guest/GuestNice）并将 Wait 归入非 busy；⑦ B6 PoE 实采 cpeExtPsePortPwrConsumption（0.1W→W）与头注释对齐；⑧ C1/C2 台账采集改多列并集驱动防丢行、列号 match 显式枚举越界拒绝兼防走错列；⑨ C5 接口消失补报（prev-up 而本轮 ifTable 不再报告的接口补发 vanished down 事件）；⑩ C6 移除 LLDP 邻居原始字节调试日志；⑪ C7 record_events 重写——设备名查询失败降级不阻断、UNNEST 批量插入、(event_type, interface, detail) 30 分钟窗口抑制、interface 按列宽 100 截断；⑫ C8 终端定位改列侧纯等值 `= ANY(text[])`（mac_match_forms 展开输入形态）与 INET 直比代替表达式匹配以走索引；⑬ C10 时间解析失败改用专用 server.snmp.invalid_time 文案 key
2. 超时口径统一（C3/C9）：core 新增共享常量 SNMP_TIMEOUT_SECS=5，to_snmp_params_async（原 10s）与 snmp_poll 会话同源；test_snmp 去第二超时参数、handlers 探测路径由 params.timeout_secs 承载；sysinfo 注释与实现对齐；snmp_poll 陈旧注释（“回写 agents”→upsert monitored_hosts）修正
3. 落库截断补齐（B7）：truncate_opt_to_column_width 入 foims-snmp core；lldp（chassis_id/port_id 100、port_desc/sys_name 255）、cdp（neighbor_port 100/address 64/platform 128）、vlan.name(64)、lag.agg_name(100)、entity/sensor 各变长列（class 32/name 255/model·serial·mfg 128/hw 32/fw·sw 64）入库前按列宽截断；前端 locales 新增 server.snmp.invalid_time 词条（zh/en）并优化 body_arp_conflict 多 MAC 文案
4. 版本：均为 bug 修复级（z+1），根 Cargo.toml 0.32.0 与 frontend/package.json 0.1.5 不动；无数据库结构变更（仅既有列的写入前截断与查询谓词改写）、无后端载荷类型变更（未重生 TS）；验证 cargo fmt、cargo clippy --release --workspace --all-targets -D warnings 0 告警、cargo test -p foims-snmp(54)/-p foims-resource/-p foims-agent-service 全绿（workspace 仅余 14 个 foims-common::crypto 既有环境失败：沙箱读 /etc/foims/encryption.key Permission denied，非本次引起且该 crate 零改动）、npm run typecheck/build 通过

v0.23.0
2026102601093401
1. 设备列表 SNMP 操作入口收敛（前端功能优化，y+1，纯前端）：资源管理-设备列表每行原本的 9 个 SNMP 相关图标按钮（端口/MAC 表/LLDP/硬件清单/VLAN/CDP/LAG/PoE/观测事件）收敛为单个 `snmp` 图标按钮（NavIcon name="snmp"，icons.ts 以 activity 折线字形入表），点击唤起新增的汇聚弹窗 components/device/DeviceSnmpModal.vue——弹窗内以顶部标签（沿用页面级 .tabs/.tab-btn 视觉，snmp-hub-tabs 覆盖 flex 等分改按需换行）切换各功能，仅挂载当前标签对应子组件（v-if，切换即按需加载，避免一次性触发 9 路请求），各标签文案复用设备行原按钮的同源 i18n 键
2. 内联渲染机制：ModalShell.vue 新增 embedded 内联模式（无遮罩/页头/Esc 栈，仅内联渲染 body/footer 为普通内联块，跳过 useModalStack 注册防抢占栈顶误关内联内容，层级由外层汇聚弹窗统一承载）；9 个子弹窗（UnifiedDevicePortsModal/ArpModal/LldpModal/EntityModal/VlanLedgerModal/CdpModal/LagModal/PoeModal/SnmpEventsModal）各加可选 embedded prop 并透传给 ModalShell；modals.css 新增 .modal-embedded/.modal-embedded-body（内联体不设固定高度，滚动由子弹窗自身 .modal-body-scroll 承接）
3. 保留说明：设备工具栏「SNMP 事件」（全局不限设备 deviceId=''）与「终端定位」两按钮及其 SnmpEventsModal/LocateModal 挂载非设备行入口，未收敛，保持独立；ResourcesPage 设备行编辑/删除按钮不变，snmpHub reactive 状态（open/deviceId/deviceName）替换原散落各功能的行内触发状态
4. 版本：纯前端功能优化（y 段 c+1），frontend/package.json 0.1.5→0.1.6（package-lock.json 顶层与 packages[""] 版本同步归正至 0.1.6，此前遗留 0.1.3 未随历次前端 bump 同步）；无后端改动、根 Cargo.toml 0.32.0 不动、无数据库结构变更、无后端载荷类型变更（未重生 TS）；验证 npm run typecheck 0 错、npm run build 通过、biome check frontend 23 文件 0 违规、oxlint 312 文件 0 告警

v0.23.1
2026102601331401
1. foims-common crypto 单测脱钩真实 /etc 密钥文件（修复 CI 与本机 14 个既有环境性失败，均 z 级）：① 密钥加载逻辑抽出路径全参数化的 load_key_material 核心（分支语义与 fail-closed 规则零漂移），load_encryption_key 收窄为「解析 /etc 路径 + 建目录 + 调核心」的生产装配；② AES-GCM 加解密抽出 encrypt_with_key/decrypt_with_key 核心（密钥与 nonce 显式注入），encrypt_password/decrypt_password 改为「ensure_encryption_key + 随机 nonce + 调核心」装配，错误语义不变；③ 单测重写为无特权可复现：加解密往返/篡改/边界用例改用固定测试密钥直调核心，密钥加载的生成落盘(0600)/复用/备份恢复/长度非法拒绝/不可读备份 fail-closed 五分支用临时目录（沿用 foims-services temp_dir 先例）覆盖，仅存真实 /etc 装配验证用例（密钥形状与 OnceLock 稳定性、check_key_integrity、非法密文公开链路）以 real_key_available() 守卫在不可读环境静默跳过；新增错误密钥拒解密用例；backup_path_for/get_key_paths 收敛备份命名规则
2. 版本：均为测试重构级（z+1），根 Cargo.toml 0.32.0 不动；foims-common 0.3.12→0.3.13（cargo update --workspace --offline 同步 Cargo.lock）；无数据库结构变更、无后端载荷类型变更（未重生 TS）、生产行为零改动；验证 cargo fmt、cargo clippy --release --workspace --all-targets -D warnings 0 告警、cargo test --workspace 全绿（crypto 22/22 通过，无环境性失败）

v0.23.2
2026102609000001
1. 安全与采集关键路径纯逻辑单测补齐（均为测试新增，z 级）：
① foims-auth：two_factor.rs 覆盖 TOTP 重放防护状态机（首次占位/同窗口重放拒绝/不同码互不影响/校验失败回滚可重试/超出 TOTP_REPLAY_KEEP 后最旧码驱逐/用户间隔离）；email_code.rs 覆盖滑动窗口频控（窗口内达上限拒绝并回等待秒数/不同键独立计数）；sso.rs 覆盖 issuer scheme 校验（https 通过/回环 http 豁免含 localhost 与 127.0.0.1 与 [::1]/非回环 http 与空 authority 与无 scheme 拒绝防认证绕过）、redirect_uri 去空白与空值拒绝、错误跳转 303 Location；smtp.rs 覆盖 parse_address 合法/非法、build_smtp_transport 的 secure 与 smtp.qq.com 走 relay 与明文 builder_dangerous 仅构造不连接、NotificationMethods::default 全启用；token.rs 覆盖 create_clear_cookie 字段（name/空值/path/http_only/secure 开关/max_age 归零）；login/mod.rs 覆盖 generate_six_digit_code 长度与字符集（不断言具体值防 flaky）。
② foims-snmp/collect：lldp.rs 覆盖 is_mac_address/format_mac_address/format_lldp_id 子类型分派/is_printable_string/get_local_port_name 优先级回退；entity.rs 覆盖 ent_class_name 已知与未知 class<N> 回退与 truth_to_bool 仅识别 1/2；ports.rs 覆盖 derive_port_status 管理态优先、format_speed 整千进吉比特否则兆比特、derive_port_speed 优先 ifHighSpeed 回退 ifSpeed、port_list_members 高位在前位图解析。
③ foims-agent-service/tasks.rs 覆盖 config_u64（正值采纳与缺失/0/负/非数字回落默认）；foims-resource/helpers.rs 覆盖 validated_identifier（合法标识符原样返回与空串/引号/空白/分号/连字符注入载荷拒绝）。
2. 测试模块定位与 clippy 合规：上述新增 mod tests 一律置于各文件 EOF，满足 clippy::items_after_test_module；sso redirect_uri 用例改结构体更新语法规避 field_reassign_with_default；测试函数命名去除大写英文片段防 non_snake_case。均为 #[cfg(test)] 追加与注释位置调整，生产代码零改动。
3. CI 测试盲区补齐（.github/workflows/ci.yml）：build 任务新增 foims-portal 独立 workspace 的 cargo test --manifest-path（此前 portal 13 测试写了从不运行）；agent-build 任务系统依赖补 build-essential/pkg-config 并新增本机 target 的 cargo test --release（此前 agent 179 测试随编译受 clippy 约束却从不运行，交叉产物不可执行故仅测本机 x86_64）；frontend 任务新增 npm run selfcheck:viz（可视化 geometry/viewmodel 纯函数自检此前 CI 不跑）；clippy 步骤对 tests/ 集成测试的注释由断言其存在改为前瞻性表述，消除与现状（无 tests/ 目录）的文档漂移。
4. 版本：均为测试新增与 CI 配置变更（z+1），根 Cargo.toml 0.32.0 与各 crate 版本不动（无生产行为改动）；无数据库结构变更、无后端载荷类型变更（未重生 TS）；验证 cargo fmt、cargo clippy --release --workspace --all-targets -D warnings 0 告警、cargo test --release --workspace 全绿 951 passed（较原 919 增 32）、foims-agent 独立 workspace 179 passed、foims-portal 13 passed、cd frontend && npm run selfcheck:viz 全通过
