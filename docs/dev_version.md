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
