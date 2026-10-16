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
