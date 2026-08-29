# IPMA 全局前后端代码审计报告（2026-08-29）

## 审计口径

- **审计原则严格限定**：Rust 语言规范、HTML/JS/CSS 语言规范、代码逻辑（含安全逻辑）。
- **明确排除**：未参考任何文档（docs/、README、AGENTS.md、.trae/rules）、未参考代码注释内容、未参考 git 提交记录。所有结论均以代码实际行为为依据。
- **范围**：11 个 Rust crate 约 4.7 万行 + `web/static` 约 4.2 万行（JS/HTML/CSS/i18n），全部文件通读，非抽样。
- **客观基线**：`cargo clippy --release -- -D warnings` 通过（0 告警）。
- **严重度定义**：P0 = 数据错误 / 安全漏洞 / 功能崩溃；P1 = 明确规范违反或重要逻辑隐患；P2 = 轻微规范问题 / 次要隐患。
- **总计**：P0 x3 / P1 x38 / P2 x135。

---

## 一、P0（3 项）

### P0-1 前端：计划任务标签页监听器无界累积，一次点击创建 N 条重复任务

- `web/static/js/modules/systemManager.js:54-55` + `web/static/js/modules/scheduledTaskManager.js:26-29, 67-72`
- `create-scheduled-task-btn` 位于 main.html（常驻 DOM）。每次切换到 scheduled-tasks 子标签都调用 `initScheduledTasksTab()` → `setupEventListeners()`，createBtn 的绑定无幂等守卫（同函数内 taskTable 有 `dataset.handlerAttached` 守卫，createBtn 没有）。
- 每次点击标签累积 1 个监听器；之后点"创建"会 N 次执行 `openCreateScheduledTaskModal`，对同一个新建模态框的 form 绑 N 个 submit 监听，一次提交触发 N 次 `POST /api/system/scheduled-tasks`，产生 N 条重复任务，且 N 随标签点击次数无界增长。

### P0-2 前端：工位可视化渲染无并发保护，跨房间布局数据被永久损坏

- `web/static/js/modules/visualization/SVGDataManager.js:94-99`（配合 403-451）
- 机柜分支（222-229 行）有 `beginCabinetRender()` 渲染代次（token）防并发交错，工位分支完全没有。快速切换房间下拉时两次加载交错执行：两次都先清空再 `await`，旧房间绘制结果追加在新房间元素之后，画布同时含两个房间的工位。
- 此后任意一次拖拽触发 600ms 自动保存（SVGCore.js:330-337 → visualizationManager.js:33-41），`saveLayout` 对元素不做任何归属校验，把画布上全部元素连同 A 房间工位 id 一起以当前 `currentRoomId` 提交；后端 `workstation_layouts` 按 `workstation_id` upsert 且 `DO UPDATE SET room_id = EXCLUDED.room_id`，A 房间工位的布局行被永久改挂到 B 房间——布局数据损坏且用户无感知。

### P0-3 CSS：`.loading { display:flex }` 使 `hidden` 属性失效，初始化页被遮罩卡死

- `web/static/css/pages/ipma_init.css:519-533`
- init 页加载遮罩 `#loading` 初始靠 HTML `hidden` 属性隐藏（init_index.html:180），JS 仅通过 `el.hidden = true/false` 切换（ipma_init/ui.js:45,49）。但 `.loading` 无条件 `display: flex` 在层叠中优先于 UA 样式表的 `[hidden] { display: none }`，`hidden` 属性完全失效——全屏 95% 白色、z-index 1000 的遮罩常显且无法隐藏，初始化页面整体不可用。对照 login.css:596 为同类元素补了 `[hidden]` 规则，此处缺失。修复：`.loading[hidden] { display: none; }`。

---

## 二、P1（38 项）

### 后端 · 认证与安全（ipma-auth / ipma-scheduler）

| # | 位置 | 问题 |
|---|------|------|
| 1 | `crates/ipma-auth/src/login.rs:423` 与 `523` | fail2ban 键错位：邮箱验证码登录入口用 `email` 检查封禁，失败计数却落在 DB `username` 键上——验证码爆破时用户名维度封禁永不触发，仅剩单 IP 维度 |
| 2 | `crates/ipma-auth/src/login.rs:240` 与 `315` | 同型键错位：登录 SQL `WHERE username=$1 OR email=$1`，攻击者用邮箱爆破密码时计数落 `user:<db_username>`、检查读 `user:<邮箱>`，用户名维度封禁对邮箱登录完全失效 |
| 3 | `crates/ipma-auth/src/ldap.rs:289-294` | LDAP 登录只查 `is_ip_banned`，不查 `is_user_banned`；失败路径却仍写入用户名封禁键——被记录但从未执行，换 IP 爆破 LDAP 账户不受用户名封禁约束 |
| 4 | `crates/ipma-auth/src/login.rs:1798-1838`（配合 ldap.rs:329-338、sso.rs:458-468） | LDAP/SSO 外部登录路径不消费 `two_factor_enabled`（ldap 中 `two_factor_verified: true` 为硬编码），外部账户即使启用了 TOTP 也不做 2FA 校验直接签发令牌；本地登录有该分支，外部路径遗漏 |
| 5 | `crates/ipma-auth/src/app_fail2ban.rs:221-225` | `username` 未过滤换行/控制字符直接拼入供 OS fail2ban 消费的平面日志（username 仅有长度校验无字符集校验），可注入伪造日志行：伪造任意 IP 失败记录使其被封禁（DoS），或稀释自身失败计数 |
| 6 | `crates/ipma-auth/src/login.rs:642-875` | `login_with_two_factor` 全函数无 `is_expired` 调用——启用 2FA 的用户可在密码过期后继续通过密码+TOTP 正常登录，等保密码有效期策略被完全绕过 |

### 后端 · 主程序与公共设施（src/ + ipma-common）

| # | 位置 | 问题 |
|---|------|------|
| 7 | `src/routes/mod.rs:126-133` + `src/system/scheduled_task.rs:435-438` | 角色矛盾：中间件明确允许 auditor 只读访问 `/api/system/logs/stats` 与 `/api/system/scheduled-tasks/logs`，但两个 handler 的 `AdminUser` 提取器严格要求 `role=="admin"`（extractor.rs:45），auditor 一律 403，权限实际不可用 |
| 8 | `src/system/task_executors.rs:93-97, 125-129` | 用户可控 i64 `days` 经 `as i32` 截断：token_usage_cleanup 无任何校验，配 2147483648 → 截断为负 → SQL 阈值落到未来 → 全表误删；log_cleanup 配 4294967296（≥1 通过校验）截断为 0 → 走 days==0 分支清空全部审计日志 |
| 9 | `src/system/config.rs:126-135` | `update_system_config` 以进程启动时的内存快照 `state.config` 为基底整写配置文件（对照同文件其他 update 均 `Config::load()` 重读磁盘）：先改语言（写盘）再改任一子配置 → 语言等后续更改被旧快照回滚（丢失更新）；后三个端点间也无互斥 |
| 10 | `src/system/config.rs:121-183, 381-414` | `UpdateSystemConfigRequest` 派生了 Validate 但从未调用 `validate()`；`restore_config` 直接反序列化落盘。可将 `jwt.secret` 写空、端口写 0——启动校验（Config::load:336-346）会拒绝这些值，导致重启后服务永久无法启动（持久化自伤 DoS） |
| 11 | `crates/ipma-common/src/rate_limit.rs:119-137` | `window_secs` 全链路无下界校验：=0 时 `current_elapsed/0.0` 产生 NaN，`NaN as u32 == 0`，`weighted >= limit` 永不成立 → 限流（含登录爆破防护）静默失效；limit=0 时全部请求被 429 |
| 12 | `crates/ipma-common/src/crypto.rs:56-94, 122` | `load_encryption_key` 生产路径共 7 处 `panic!`（密钥目录创建失败、长度不符、读写失败、备份损坏等），函数签名无错误通道，违反生产路径禁止 panic 的规范；密钥文件损坏时进程直接崩溃 |

### 后端 · 资源管理（ipma-resource）

| # | 位置 | 问题 |
|---|------|------|
| 13 | `crates/ipma-resource/src/device/mac.rs:209-221` | IPv6 邻居表 OID（1.3.6.1.2.1.4.35.1.4）索引解析差一：`addr_len` 实际位于 `oid_parts[11]`，代码读 `[12]`（地址首字节），且长度条件恒为假——IPv6 邻居 MAC 采集分支永远无法命中（死分支），IPv4 ARP 正常，功能静默失效无告警 |
| 14 | `crates/ipma-resource/src/device/nic.rs:216-229` | `apply_network_config` 无条件 `DELETE FROM cable_links WHERE ... device_managed`：任何携带 cards 的设备保存（哪怕网络配置未改动）都会静默清空托管网口上的物理布线记录（cable_label/length_m/tested 不在请求中无法回填），不可逆 |
| 15 | `patch_panel.rs:178-186`、`cabinets.rs:563-575`、`room.rs:814-823, 898-913, 973-983` | sync 系列 UPDATE 不校验 `item.id` 归属，直接把 `cabinet_id/room_id` 覆写为路径参数 id：携带其他机柜/房间的子资源 id 时被静默搬移（IDOR 类写路径），原父资源下凭空消失，也不检查 rows_affected |

### 后端 · 模型 / 可视化 / 证书（ipma-models / ipma-visualization / ipma-x509-manager）

| # | 位置 | 问题 |
|---|------|------|
| 16 | `crates/ipma-x509-manager/src/generate.rs:250-267` | 私钥先 `tokio::fs::write` 再 `set_permissions(0600)`：写入时刻文件权限 0644，窗口期内本机其他用户可读 CA/叶子证书私钥；chmod 失败或中途崩溃则永久 0644。应用 `OpenOptions::mode(0o600)` 创建后写。被 generate/ca/import 共用，影响全部私钥落盘路径 |
| 17 | `crates/ipma-x509-manager/src/generate.rs:105-108`、`import.rs:61-64` | 证书/私钥文件名仅含秒级时间戳，无冲突处理：同秒两次生成/导入交错写同一组文件，可留下 cert A + key B 错配对，部署后 TLS 握手失败（对照 ca.rs:294-304 的 import_ca 有规避） |
| 18 | `crates/ipma-visualization/src/layout.rs:214-228` | `delete_layout` 跨 `workstation_layouts` 与 `element_layouts` 两表删除，两条语句独立执行无事务，部分删除不可回滚（同文件 save_layout 均正确用事务） |

### 后端 · 初始化 / 数据管理 / 组织（ipma-init / ipma-data-management / ipma-organization）

| # | 位置 | 问题 |
|---|------|------|
| 19 | `crates/ipma-data-management/src/import/mod.rs:64-71` | 组织 CSV 导入按 `parent_path.split('/').count()` 排序保证父先于子，但空串 split 计数为 1 与一级子节点相等，导出顺序 `ORDER BY id`（UUID 随机）→ 根与直接子节点顺序随机：官方导出 ZIP 导入空库约 50% 概率整批回滚失败 |
| 20 | `crates/ipma-data-management/src/import/mod.rs:240-244` | ZIP 条目解压 `take(MAX_DECOMPRESSED_SIZE)` 达 100MB 上限时 `read_to_end` 正常返回 Ok——超限被静默截断而非拒绝，截断点若在记录/UTF-8 边界则数据无声丢失且返回导入成功 |
| 21 | `crates/ipma-init/src/operations.rs:171-186` | `drop_all_tables` 先 `SET session_replication_role='replica'`，任一 DROP 失败即经 `?` 提前返回、不复位 origin：连接以 replica 会话状态归还连接池，后续静默跳过所有 FK 与触发器。当前调用方用临时池缓解，但它是 pub 导出的公共 API |
| 22 | `crates/ipma-init/src/schema/tables/views.rs:72` | `mac_comparison` 视图 JOIN 条件 `di.device_id != sm.device_id`（不等）：正常"IP 已被管理且归属同一设备"的行全部被误判为 'unmanaged'，'match' 分支只剩与其他设备 MAC 巧合相等才可达——比对语义反转，且在 check.rs 强制自检清单中 |
| 23 | `crates/ipma-organization/src/lib.rs:816-840` | 删除组织校验了子组织数与关联房间数，未校验挂在该节点下的员工；`employees.org_id ON DELETE CASCADE` 静默级联销毁该节点全部员工记录（工位 manager SET NULL），与子组织/房间的显式拦截策略不一致 |
| 24 | `crates/ipma-init/src/check.rs:662-668` | `check_has_data` 把查询错误一律当"无数据"（`Err(_) => false`）：users 查询异常而库内仍有业务数据时，`backup_and_drop_for_rebuild` 跳过备份直接 DROP DATABASE——销毁性操作的判据应 fail-fast |

### 前端 · 核心与模块

| # | 位置 | 问题 |
|---|------|------|
| 25 | `web/static/js/utils/apiClient.js:85-87, 254-270, 141-153` | `skipAuthCheck` 是死选项（login.js 6 处传入，apiClient 仅 delete 从不读取）；`/api/auth/login/ldap` 不在公共端点清单：LDAP 登录输错密码返回 401 → 先走注定失败的 refreshToken → 弹"令牌已过期"（真实错误被吞）→ 1.5s 后强制刷新登录页，已输入凭证全部丢失，LDAP 验证码失败联动失效 |
| 26 | `web/static/js/utils/networkCardManager.js:102-135` | `setRoomContext` 无请求序号/取消机制：快速切换房间 A→B 时 A 响应晚到则缓存反映 A 而 roomId 是 B——IP 行下拉显示错误房间网段，collectData 用错误 roomNetworkIds 校验，提交数据错乱 |
| 27 | `web/static/js/modules/eventManager.js:120-135`（device/cabinet/cableLink/authManager 等） | 所有主表单提交回调在 await 期间均不禁用保存按钮、无 in-flight 标志：快速双击触发两次完整提交流程，新建场景产生重复机柜/设备/线路记录（对照 deviceSnmp.js:113-118 已有正确 disabled 模式） |
| 28 | `web/static/js/modules/ipmanager.js:216, 333-354` | 列表加载无 AbortController、无请求序号：防抖只减少不消除并发，宽过滤旧响应可晚于窄过滤新响应到达并覆盖，表格与过滤框内容不一致 |
| 29 | `web/static/js/modules/ipma_init/api.js:73, 145` | 后端 `/api/init/check-pgsql`、`db-status` 的 `error` 字段返回 i18n key，前端直接展示原文不经 `t()`；且 `server.init.pgsql_connect_failed` 在 zh/en 字典均缺键——用户看到裸键名 |
| 30 | `web/static/js/modules/log.js:94, 196-215` | `initLogSearch` 无守卫：每次导航到 #logs 都给常驻 `#logs-search` 追加一个带独立 debounce 闭包的 input 监听——访问 N 次后每敲一个字符触发 N 次 `loadLogsData`（无界累积 + 重复请求） |
| 31 | `position.js / workstation.js / room.js / organization.js / userManager.js / networks.js`（经 ui.js handleFormSubmit） | 范围内所有创建/保存表单（机位/工位/房间/组织/模板/员工/用户/网段/网段区域）在请求期间均不禁用提交按钮——双击即两条 POST，产生重复记录（对照 unifiedDevicePorts.js:361-366 的正确范式） |
| 32 | `web/static/js/modules/organization.js:1121-1141` | 模板编辑器允许不同分支下创建同名类型，`collectMapping` 用类型名作对象 key，后处理分支静默覆盖先处理分支（子级丢失无提示）；根类型与子级同名时 roots 为空，整体失效；submit 对类型名重复零校验直接入库 |
| 33 | `web/static/js/modules/userManager.js:135-172` | 编辑用户 `loadUserData(userId)` 未 await 无竞态防护：A 的 GET 晚到时 `elementCache.setValue` 解析到当前 B 模态框输入框，把 A 的 username/email/role 写入 B 的表单（隐藏 id 是 B），保存即用 A 的资料覆盖 B 的账号 |

### 前端 · 可视化

| # | 位置 | 问题 |
|---|------|------|
| 34 | `web/static/js/modules/visualization/TopologyCore.js:508-519` | 画布连线模式仅拦截"同一设备同一端口"，同设备两个不同端口可互相连线形成自环；连线创建模态框入口（visualizationManager.js:388-391）明确校验并提示 no_self_connection——两条入口校验不一致 |

### CSS

| # | 位置 | 问题 |
|---|------|------|
| 35 | `web/static/css/pages/login.css:197, 263` | 引用未定义变量 `--color-text-muted`（登录页仅加载 login.css，该文件自称自包含却未定义此变量）：color 回落到继承的近黑色 `#1a1a2e`，副标题/2FA 描述失去弱化层次；同文件其他 var 均带 fallback，唯这两处没有 |
| 36 | `web/static/css/components/modals.css:213-215` | `.modal + .modal { z-index: 1100 }` 按 DOM 相邻性（=首次装载顺序）而非打开顺序：A 先装载后关闭 → 打开 B（1100）→ 从 B 内打开 A（已在 DOM，保持 1050）→ 二级弹窗 A 被 B 整体盖住，看不到也点不到 |
| 37 | `variables.css:99` vs `modals.css:214` | `--z-tooltip(1070) < --z-modal-secondary(1100)`：挂在 body 的 global-tooltip 是根层叠上下文参与者，二级模态内的 data-tooltip 必被压在模态下。确定触发路径：端口管理模态 → SNMP 冲突弹 port-conflict-modal（二级）→ 其头部 Skip All/Overwrite All 按钮的 tooltip 永不显示 |
| 38 | `web/static/css/pages/visualization.css:467, 616` | 拓扑详情弹窗 z-index 10000 / 遮罩 9999，高于全站最高约定 toast(9999) 且 DOM 更靠后：弹窗内坐标非法时 `showToast(check_input, warning)` 被完全遮挡，校验反馈被吞 |

---

## 三、P2（135 项，按主题归纳）

### 后端 · 输入校验缺失/不对称（约 14 项）

- `vlan_id/trunk_id` 无 1..=4094 校验（device.rs 模型、nic.rs、PortSyncItem 三处一致缺失），负数与 4095+ 可入库。
- `length_m`无非负校验（cable_link.rs:255/384，-5 米可入库）。
- 密码仅设下界无上界（models/user.rs:28,57,74；init/types.rs:36）：bcrypt 只用前 72 字节静默截断，前 72 字节相同的口令哈希等价均可登录；`EmailLoginRequest.email` 唯一缺 max=100。
- `start_u <= end_u` 无跨字段校验（position.rs/workstation.rs），倒挂机位（40-2U）可入库；前端 cabinet-position-modal 上限 48U 与机柜容量上限 45U 不一致。
- `IpManagerUpdate.ip_address` 无格式校验（Create 有），INET 列接受 `10.0.0.1/24` 带掩码值入库；`ip_version` 无 4/6 约束。
- 布局坐标无有限性/范围校验（layout.rs），`1e400` 解为 inf 到写库才饱和钳制；拖拽保存路径无负坐标校验（与手工录入 min=0 不一致）。
- 拓扑节点未派生 Validate（负宽高、极值坐标直写）；重复 device_id 时 COUNT≠len 合法 id 被误报。
- 模板 device_type 仅长度校验（Create 是白名单）；brand/model/description 无长度校验，SNMP 凭据模型长度上限与密文列宽不匹配（超限 500）。
- 初始管理员 role 仅长度校验未限定合法角色集合；scheduled_tasks 的 task_type/cron 仅长度校验。
- `limit` 参数无范围校验（scheduled_task.rs:441，`?limit=-1` PG 等价无 LIMIT 全表返回）；批量创建 IP 无条数上限且不校验 network_id 归属（与单条路径不一致）。

### 后端 · TOCTOU / 无事务 / 错误映射缺失（约 12 项）

- IP 唯一性先查后插无 23505 映射（ip.rs:278-288, 350-365，并发 500；auto_assign_ip 有映射，口径不一）。
- cable_link 重复端点对无预检无 23505 映射（正常业务冲突返回 500 而非 409）。
- create_cabinet/workstation/room/position/network_region 等重名预检无事务无冲突映射；update 系列不做重名预检；FK 引用存在性不校验靠 500 兜底。
- delete_room/cabinet/network 计数检查与 DELETE 无事务：cabinet 的 positions 是 CASCADE、devices.position_id SET NULL——竞态窗口内新建机位被级联删除且设备关联被清空。
- 拓扑连线"同设备对仅一条"为 check-then-insert，表无唯一约束，READ COMMITTED 下并发可产生代码明确阻止的重复连线。
- 网段更新不校验存量 IP 归属（改 CIDR 后已有 IP 不在新网段照常提交）；重叠网段（/25 vs /24）可共存，设备 IP 的网段匹配 LIMIT 1 无排序命中不确定；区域 CIDR 收缩不复查已辖网段。
- `get_cabinets_by_network_region` 提供 network_id 时完全绕过 region 归属校验。

### 后端 · IPv6 拼接遗漏（3 项）

- `db.rs:243-250`：连接 URL 未给 IPv6 主机加 `[]`，纯 IPv6 地址的数据库无法连接（项目定位 IPv6 友好）。
- `forwarding.rs:143`：syslog 外发地址同样未处理 IPv6，裸 v6 地址生成的 SocketAddr 非法，发送永远失败。
- `certificate.rs:51-59`：public_url 为 IPv6 字面量时 split(':') 得 `"["` 注入证书 SAN，生成损坏 SAN 的证书。

### 后端 · 错误被吞 / 日志失真（约 6 项）

- `lldp.rs:465-474`：陈旧邻居清理 DELETE...RETURNING 失败 `unwrap_or(0)` 吞掉，事务照常提交，removed 计数失真。
- `organization/lib.rs:191-199`：模板批量加载 `unwrap_or_default()` 吞 DB 错误，故障时 org_type 静默回退。
- `config.rs:596-610`：收件人查询 Err 与 Ok(None) 同分支返回空列表 + 200，DB 故障被吞成"无收件人"。
- `verification.rs:107-111`：锁中毒时验证码更新被跳过仍返回"已生成"，日志新码与存储旧码不一致。
- `task_log.rs:16-33`：任务日志 start==end 且 duration 恒 0（写死 Utc::now()/0），审计数据失真。
- `SVGDataManager.js:488-491`（前端同类）：删除布局请求失败仍清空画布，行为等同成功。

### 后端 · 死代码 / 假指标（约 6 项）

- `db.rs:58-122`：`record_request_start/complete` 全仓库无调用，waiting/total/failed/avg_wait 恒 0——`get_system_info` 展示假指标，`waiting_requests > 0` 告警永不触发；`pool: Arc<ArcSwap<PgPool>>` 与 `config: RwLock<PoolConfig>` 的写侧未使用。
- `rate_limit.rs:158, 174`：`trusted_proxies` 仅初始化为空 Vec 无赋值入口，`is_trusted_proxy` 恒 false，且两分支结果相同——无效死逻辑。
- `helpers.rs:88-121`：`get_room_id_by_workstation/position` 全仓库无引用。
- `transfer.rs:48-71`：`read_certificate`（可读私钥内容）无调用方，属遗留死代码且是潜在误用面。
- `static_files.rs:38-72` vs `json.rs:10-43`：主 crate 复制了一份逐行相同的 AppJson/map_json_rejection 实现，未复用 ipma-common。

### 后端 · 其他逻辑隐患（约 10 项）

- `auth_middleware` 不校验 `tokens_invalidated_at` 与用户状态：降权/禁用用户的旧 access token 在 TTL 内（默认 15 分钟）仍以原角色通过鉴权（延迟到 refresh 才生效）。
- JWT 密钥缺失时生成临时随机密钥继续运行（utils.rs:103-107）：重启后所有会话静默失效，故障降级为一条日志。
- `derive_redirect_uri` 用请求 Host 推导 SSO 回调地址（sso.rs:211-231，Host 头注入面）。
- 群发邮件收件人全部置于 TO（smtp.rs:294-300），邮箱列表互相可见，应 BCC。
- `send_login_code/send_two_factor_code/forgot_password` 三个公开邮件端点无限流/验证码（邮件轰炸面）；其中 send_two_factor_code 请求含 password 字段但完全不校验。
- `login_with_two_factor`/`login_with_email_code` 未接入 captcha::enforce（可切换端点绕过验证码递进）。
- fail2ban 配置仅存内存重启即失（app_fail2ban.rs:462-488）。
- `delete_user` 无自删/末位管理员保护（user.rs:240-281，可删至管理员清零锁死系统）。
- remember_me 推断边界 `> 86400`（login.rs:1045，恰 24h 配置时刷新后静默降级）。
- LDAP 内部故障（连接失败等）与认证失败同走 record_login_failure（ldap.rs:305-327），故障期间正常用户被误封。
- cron `calculate_next_run` 秒位只对齐最小命中值、日/星期同时受限按 AND 而非 OR（与 tokio-cron-scheduler 实际触发不一致）；create 对非法 cron 仅告警落库（任务静默永不执行）而 update 返回 422，行为不一致。
- scheduler 系统任务无重叠执行保护。
- `main.rs:64-76`：以子串 `contains("v=")` 判断版本参数，`?dev=1` 也命中一年 immutable 强缓存；`main.rs:597-611` 单实例检测与 remove_file 间 TOCTOU；`main.rs:633-641` serve 出错仅记日志主流程仍等信号可长期空转；`main.rs:200-254` async main 中同步阻塞 IO。
- `config.rs:80-84`：uptime 普通减法时钟回拨下溢（同文件 872 行已用 saturating_sub，行为不一致）；`config.rs:304-317` 重启脚本 spawn 失败仍 2 秒后 exit(0)（standalone 模式服务直接下线）。
- 证书：`set_ca_cert_only` 先写新证书后删旧私钥，失败留错配状态且 rcgen Issuer 不校验匹配，后续签发静默产出废证书；import_certificate 不解析私钥、不校验证书/私钥匹配（内容为 "Hello" 的假私钥可通过）；validate_ca 不校验有效期与密钥强度（过期 CA、RSA-1024 可导入）；叶子证书有效期不与 CA not_after 比较；SAN 解析失败静默丢弃（可生成无 SAN 的不可用证书且报成功）；GenerateCert/GenerateCaRequest 无 Validate 派生（兆级 CN 可进证书），country 校验一处 trim 一处不 trim。
- init：备份文件/目录未设权限（对照 data-management/backup.rs 的 0700/0600）；config.toml 读改写非原子（承载安全开关）；两个 GET 状态探测端点内部会建库建 schema（只读语义带副作用）；上传 SQL 临时文件仅成功路径清理（失败路径遗留 /tmp 无限累积）；organizations.name 创建/更新不禁止 `/` 而导入端禁止（导出→导入必然失败的两端不一致）；org_template 环检测只从根 DFS，游离环可通过校验；组织 description 用 COALESCE 无法清空（对照 employee 的可清空语义）；upsert 导入不更新 updated_at（triggers.rs 清单遗漏 7 张表）。
- 布局：workstation 分支无归属校验（可把 B 房间工位 UPSERT 进 A 房间布局，对照 cabinet 分支有校验）；非 "door" 的任意 element_type 一律按工位处理。
- options.rs:52-59：过滤参数非法 UUID 被静默忽略（退化为全量列表，对照 net_outlet/patch_panel 显式 422）。
- create_room 返回 `req.room_type` 原文而库存 to_uppercase 后的值；create_workstation 返回 `req.manager` 而库是 COALESCE 值（回显与后续查询不一致）。
- update_device save_as_template 模板名可回退为空串插入空名模板。
- `DeviceUpdate` 双层 Option（可 null 清除）与单层 Option（null=不修改）混用，description/hostname/snmp_community 等十余字段永远无法置空。

### 前端 · 竞态/重复提交/监听器（约 15 项）

- 2FA 凭证 `tempAuthData` 在请求发出前清空（login.js:544-545）：输错一次动态码后再次点击静默弹回账号密码视图，需重输全部凭证。
- 2FA 每次非公共请求前额外发注定失败的 refresh（`#lastRefreshTime` 恒 0 逻辑，apiClient.js:317-322）。
- `Retry-After` 为 HTTP-date 时 parseInt 得 NaN 立即重试（apiClient.js:155-160）；catch 内 `error.message.includes` 对无 message 抛出值会 TypeError 击穿"永不 reject"契约（apiClient.js:235-243）。
- 空列表早退发生在 appendPagination 之前 + 无越界页回退：删除最后一页最后一条后停在空页无法翻回（userManager/log notifications/cabinet/cableLink/device 系列）。
- 子网使用弹窗无竞态防护（networks.js:392-468，旧网段数据覆盖新弹窗）；cableLink 下拉选项并发交错追加过期重复选项。
- toggleScheduledTask 双击切回原样（无防抖）；标记单条通知已读后重置过滤态与页码。
- 快速双击刷新按钮行为异常（networks.js refreshButton.innerHTML 用纯文本恢复，图标永久丢失）。
- `showWorkstationRoom` 判定无布局后 autoDraw 内部再 loadSavedLayout（三类请求全部双发）；自动绘制路径显式 saveLayout 后 500ms 定时器再保存一次。
- dashboard 请求 devices top5 存缓存但从不渲染（死数据）；进入日志页同一列表加载两次（initLogTabs + whenVisible 重复触发）。
- loadTopology 无代次保护，快速连续触发画布出现重复节点与连线（重新加载可自愈）。
- `updateConnectionPaths` 全局清零平行偏移计数器却只重画子集，连线锚点重叠。

### 前端 · 转义遗漏（纵深防御）与选择器（约 6 项）

- cabinet.js:87,90：startU/endU 未过 escapeHtml 插入属性（同模板其他字段均转义）。
- cableLink.js:57-105：未知端点/链路类型原始字符串未转义直插 td.innerHTML。
- organization.js:802：`tpl.id` 未转义拼进 data 属性（同组 tpl.name 已转义）。
- scheduledTaskManager.js:151：`t()` 恒真值使 `|| escapeHtml(...)` 为死分支，缺键时未转义 key 进 innerHTML。
- login.js:391：验证码 SVG 未经校验直接 innerHTML（当前同源受信 + CSP 阻断内联，实际可利用性低，与全站模式不一致）。
- TopologyCore.js:744 选择器插值未 CSS.escape（同文件其他处已转义）；log.js:143 URL 参数 `?tab=` 未 CSS.escape 可使 querySelector 抛错中断日志页初始化。

### 前端 · HTML/CSS 规范与逻辑（约 12 项）

- index.html 仅加载 login.css 而 showToast 在登录页有真实触发路径——toast 以无样式裸 DOM 出现（init_index 已正确引入 toasts.css）。
- init_index.html 密码框缺 `autocomplete="new-password"`（main.html 均已正确标注）。
- scheduled-task-modal 5 个控件与打印模态框全选 checkbox 缺 `name`；arp-modal 搜索框无 label/aria-label/name；log-details-modal 悬空 label。
- `.modal-content.modal-xl` 重复定义（modals.css:39 vs 304，后条静默覆盖前条窄视口保护）。
- lldp-table 引用 6 个不存在的变量（--bg-secondary 等，仅 fallback 生效，全局换色不跟随）。
- `.viz-port-item` 死代码（全站无引用，且 status-admin-down 未同步纳入）。
- `.skeleton` 无尺寸，骨架屏 0×0 完全不可见（animations.css:65-70）。
- `--color-primary-light`（10% 透明 tint）误用作边框色/2px 分隔线（dashboard.css:36、system-config.css:19/44/370），视觉上消失。
- ipma_init.css 状态类命中容器（JS 把 status-success 加到容器上），字号被放大到 24px。
- networks.js /31 网段 totalIps=0、/32 为 -1（unused 显示负数）；统计口径为整个 CIDR 而图上仅展示首个 /24。
- 可视化硬编码 `page_size=1000` 不读 total/total_pages（工位、IP、设备下拉，超 1000 静默截断）；同一工位多条 IP 两处取值策略不同（last-wins vs active 优先）。
- `_fitView` 组织筛选隐藏全部节点时 setViewBox(Infinity,...) 写非法 viewBox，fit 失效、部分浏览器网格消失；`port_id`/`cable_id` 为 null 且无 number/label 时 `.slice` 抛 TypeError 中断渲染/弹窗。
- 死代码：TopologyRenderer.drawDeleteMarker、TopologyCore.resetZoom、SVGCore.setGridSize/toggleSnapToGrid/toggleAlignmentLines（吸附开关无 UI 入口，恒默认值）、SVGRenderer.clearElements、SVGVisualization 四个从未读取的成员、systemManager.initSmtpFunctions 空函数、login.js dataset.originalText 死写入、room.js/t():`||` 死分支。
- i18n：systemManager 运行时长、navigation/resourceTabs 加载失败文案等硬编码中文（英文界面输出中文）；ENDPOINT_TYPE_LABELS 等模块加载时求值一次，languagechange 免刷新切换后旧语言残留；fail2banManager 三处 `result.error` 恒 undefined（响应结构是 message/errorType），具体错误永远只显示通用文案。
- 前端角色隐藏只改菜单可见性，hash 直达仍完整加载模块（前端无二次拦截，防线完全依赖后端）；2FA 初始化的 admin 校验仅前端提示。
- fail2banManager/systemManager 多处 `elementCache.get(...)` 直接 `.value`/`.textContent` 无空值保护（同文件其他处有守卫）；清空日志 days 输入非数字时确认文案显示 "NaN"、请求体 null。
- position.js:171-174 removeEventListener 引用每次新建的闭包，移除永远无效（靠 modal DOM 销毁兜底的死防护）；organization.js:702-703 检查 `templateSelect.style.display` 而显隐实际用 classList（条件恒真，无效检查）；organization.js:455 把已转义 HTML 片段赋 textContent 二次转义显示 `&amp;` 字面量。
- styleLoader 加载失败的 link 永久残留 head 且每次重试再插入（坏链接累积）。

---

## 四、明确未发现的问题类别（正面结论）

- **SQL 注入**：后端全部动态 SQL（QueryBuilder / format!+AssertSqlSafe / ILIKE escape_like / ORDER BY 白名单 / LIMIT-OFFSET 钳制数值）均为白名单常量或参数绑定，未发现注入面。
- **unwrap()/expect()/panic!/unreachable!()/#[allow]**：生产路径未发现（全部命中在 #[cfg(test)]）；唯一例外是 crypto.rs 的 panic! 族（已列 P1-12）。clippy 全量 0 告警。
- **XSS（实际可利用）**：前端所有拼接用户/API 数据的 innerHTML/insertAdjacentHTML/SVG text 点经逐一核对，均经 escapeHtml（实现正确：`& < > " '` 五实体单重替换）或走 textContent/createElement；发现的仅为枚举契约下的纵深防御遗漏（P2）。
- **alert()/confirm()/console 调试残留**：未发现，均走 showToast/showConfirm。
- **路径穿越**：证书导入/导出/删除（字符白名单 + 拒 `..`/分隔符）、ZIP 条目名（拒 `..`/`/`/`\`）、静态文件（tower-http）均防护到位。
- **鉴权遗漏（完全裸奔的受保护端点）**：未发现；存在的问题是角色矛盾（P1-7）与粒度不一致（模板下载、通知设置、仪表盘统计等仅登录可读，P2）。
- **handler 提取器与路由挂载**：各 crate handler 经路由层 auth_middleware/admin_admin_guard 核对一致。
- **分页契约**：前端 appendPaginationToTable 消费 total/page/page_size/total_pages 与后端完全一致；Pagination clamp(1..=1000) 与饱和 offset 正确。
- **i18n 键缺失（前端静态键）**：全部字面量键在 zh/en 字典存在（唯一缺键是后端侧 server.init.pgsql_connect_failed，已列 P1-29）。
- **重复 id / label 断链 / form 内按钮缺 type**：模态框 HTML 经机械校验未发现。
- **CSV 公式注入**：escape_csv_formula 覆盖 =+-@ 及 Tab/CR 前缀，实现正确。

---

## 五、统计

| 范围 | P0 | P1 | P2 |
|------|----|----|----|
| src/ + ipma-common | 0 | 6 | 17 |
| ipma-auth + ipma-scheduler | 0 | 5 | 13 |
| ipma-resource | 0 | 3 | 16 |
| ipma-models + ipma-visualization + ipma-x509-manager | 0 | 3 | 18 |
| ipma-init + ipma-data-management + ipma-organization | 0 | 6 | 12 |
| 前端核心（app/login/utils）+ 主页面 HTML | 0 | 2 | 9 |
| 前端模块 A（auth/cabinet/cableLink/device/dashboard/ipma_init 等） | 0 | 3 | 9 |
| 前端模块 B（log/networks/organization/userManager/scheduledTask 等） | 1 | 4 | 21 |
| 可视化 JS + 模态框 HTML | 1 | 1 | 15 |
| CSS 全量 | 1 | 5 | 5 |
| **合计** | **3** | **38** | **135** |
