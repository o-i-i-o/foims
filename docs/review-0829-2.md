# IPMA 全局前后端代码复审报告（第二轮 · 2026-08-29）

> 本轮为第一轮审计（见 `code-audit-20260829.md`，P0×3 / P1×38 / P2×135）全部修复完成后的独立复审。
> 审计口径与第一轮一致，**严格限定**为三类：Rust 规范 / HTML·JS·CSS 规范 / 代码逻辑（含安全逻辑）。
> **明确排除**：未参考任何文档（含本仓库内的审计报告文件）、未参考代码注释内容、未使用 git 历史。所有结论均以代码当前实际行为为依据。
>
> **复审总计：P0×3 / P1×26 / P2×121**（第一轮 176 项中，绝大多数已确认修复；本轮为在修复后代码上新发现或残留的问题）。
> 客观基线：`cargo clippy --release -- -D warnings` 0 告警；`cargo test --workspace` 582 通过；前端 eslint/stylelint/htmlhint/jest（57 用例）全部通过。

---

## 一、P0（3 项）

### P0-1 模型：三个批量同步容器缺 `#[validate(nested)]`，子项校验在同步路径完全不生效

- `crates/ipma-models/src/models/workstation.rs:159-163`（另 113-116、184-187）
- 证据：
```rust
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct RoomChildrenSync {
    pub workstations: Option<Vec<WorkstationSyncItem>>,
    pub cabinets: Option<Vec<CabinetSyncItem>>,
}
```
- 说明：`CabinetPatchPanelsSync.patch_panels` 与 `RoomNetOutletsSync.net_outlets` 同样未挂 `#[validate(nested)]`。validator derive 不会自动展开 `Vec/Option<Vec>` 元素，同文件的 `CabinetPositionsSync.positions` 与 `LayoutSaveRequest.layout` 都正确显式挂载，唯独这三个容器遗漏。三个同步 handler 只调用容器级 `req.validate()`（`ipma-resource/src/room.rs:793、858`、`patch_panel.rs:119`），循环内无逐项校验，随后直接写库。后果：`workstations:[{"name":""}]`、`cabinets:[{"name":"X","capacity":9999}]`、超长配线架名等违反模型自身声明约束的请求返回成功，错误数据持久化。

### P0-2 前端：拉取 MAC 设备下拉漏掉全部 v2c 设备（核心功能对一类设备选择性失效）

- `web/static/js/modules/ipmanager.js:79` 附近
- 证据：
```js
const devices = result.success ? result.data?.items ?? [] : null;
...
const snmpDevices = devices.filter((dev) => dev.snmp_community || dev.snmp_username);
```
- 说明：`/api/resources/devices` 列表接口在后端（`crates/ipma-resource/src/device/crud.rs:176-179`）对响应脱敏：`snmp_community` 置 `Null`（`snmp_username` 保留）。该过滤条件对仅配置 v2c community 的设备恒为 false，所有 v2c 设备（最常见的 SNMP 配置）从"拉取 MAC"目标下拉中消失并提示"无 SNMP 设备"，而按 `device_id` 执行拉取时后端实际可用存储的 community 工作。前端判定字段与接口脱敏契约不匹配。

### P0-3 CSS：≤768px 下所有模态框表单控件收缩居中，移动端表单布局明显失效

- `web/static/css/responsive.css:172-175` + `web/static/css/components/modals.css:176-194`
- 证据：
```css
/* responsive.css（后加载，同特异性覆盖 forms.css 的行向布局） */
.form-group { flex-direction: column; align-items: stretch; }
/* modals.css（特异性更高，压制 responsive） */
.modal .form-group { align-items: center; }
.modal .form-group input:not([type="checkbox"]), .modal .form-group select, .modal .form-group textarea { flex: 1; width: auto; min-width: 0; }
```
- 说明：移动端把 `.form-group` 改为纵向堆叠时，未覆盖 `.modal .form-group` 的 `align-items:center`（0,2,0 > responsive 的 0,1,0）与模态内控件 `width:auto`。纵向 flex 中 `width:auto` 退化为收缩适配（输入框约 180–210px），叠加居中；`.modal .form-row .form-group label`（0,3,1）还压过 responsive 的 label 全宽左对齐。结果：手机上所有模态表单（user/device/network 等主流程均为 `.form-row > .form-group` 结构）呈现「小标签居中 + 窄输入框悬空居中」。

---

## 二、P1（26 项）

### 后端 · src/ + ipma-common（2 项）

| # | 位置 | 问题 |
|---|------|------|
| 1 | `src/system/config.rs:404-427` | `disable_init_mode` 仍以启动内存快照为基底整写配置文件，且不持 `config_write_lock`——其他"读-改-写盘"端点都已改为读盘最新配置+持锁，唯独此处两样都缺：启动后任何写盘变更（数据库密码、JWT 密钥、限流参数）会被旧快照覆写，重启后生效被回退的旧配置；与其他写盘端点并发时存在丢失更新竞态 |
| 2 | `crates/ipma-common/src/crypto.rs:123-149` | 主密钥缺失时，备份密钥读取失败或长度非法仅告警即继续生成全新密钥，并立刻用新密钥覆盖备份文件——旧密钥唯一残存副本被销毁，SMTP 密码/SNMP 凭据等全部存量 AES-GCM 密文此后永久不可解密，且服务正常启动、故障只在逐次解密时暴露。不可恢复时应 fail-closed 报错退出 |

### 后端 · ipma-auth + ipma-scheduler（5 项）

| # | 位置 | 问题 |
|---|------|------|
| 3 | `crates/ipma-auth/src/login.rs:303-335` | `localhost_only_middleware` 与 fail2ban 的 IP 键完全信任可伪造的 `X-Real-IP`：应用直接监听 TCP 时，私网对端被 `is_trusted_proxy` 视为可信代理。任意内网主机发送 `X-Real-IP: 127.0.0.1` 即可通过回环校验（鉴权绕过）；fail2ban IP 键与邮件频控 `ip:` 键可逐请求轮换伪造，IP 封禁对内网攻击者失效，反向还可伪造他人 IP 误封无辜出口地址。应交叉验证真实对端地址 |
| 4 | `crates/ipma-auth/src/login.rs:1314-1316` + `utils.rs:190-195` | 未勾选"保持登录"的会话经一次刷新即升级为 7 天持久会话：非 remember-me 登录的 refresh token 有效期固定 86400 秒，`token_duration >= 86400` 恒真，24h 内刷新一次即按 `refresh_token_expiry`（默认 7 天）重签，Cookie 同步变 7 天。以 token 时长反推用户意图的启发式写反了默认语义，应在 claims 中显式携带 remember_me |
| 5 | `crates/ipma-auth/src/login.rs:655-759` | `login_with_email_code` 签发令牌前未检查密码有效期——`login` 与 `login_with_two_factor` 都在签发前调用 `is_expired`，唯独邮箱验证码路径缺失，密码过期用户可凭邮箱验证码直接取得完整登录态 |
| 6 | `crates/ipma-auth/src/login.rs:1226-1368` | `refresh_token` 签发前同样未检查密码有效期：密码已过期用户只要持有未过期 refresh Cookie 即可无限续期，使上述登录路径的过期检查形同虚设 |
| 7 | `crates/ipma-scheduler/src/cron.rs:61,67,97` | 星期字段编号体系错位一天：本实现按周一=0（`num_days_from_monday`）匹配，POSIX cron 与实际触发库（cron crate）为周日=0。用户写 `0 0 0 * * 1`（周一）被解释为周二，`next_run_at` 持久化为错误日期且与实际触发系统性偏差一天；文件内单测固化了该非标准语义 |

### 后端 · ipma-resource（4 项）

| # | 位置 | 问题 |
|---|------|------|
| 8 | `crates/ipma-resource/src/room.rs:79-117` | `get_rooms` 搜索条件的 OR 缺少括号：拼接结果为 `name ILIKE $1 OR room_type ILIKE $1 OR (description ILIKE $1 AND org_id IN (...))`（AND 优先级高于 OR），搜索与组织子树/房型过滤同时使用时，命中名称或房型的房间绕过组织过滤被返回（结果集错误）。同文件模式的 workstation.rs/ip.rs 均正确用括号包裹，此处遗漏 |
| 9 | `crates/ipma-resource/src/workstation.rs:395-437` | `delete_workstation` 缺少设备引用检查：`devices.workstation_id` 外键 `ON DELETE SET NULL`，直接删除工位静默清空占用该工位的设备归属且不报错。同项目其他删除链路（删机柜检查 position/device、sync 路径删除工位前检查 devices 并拒绝）均拦截此类静默清空，唯独独立删除工位路径遗漏，口径不一致 |
| 10 | `crates/ipma-resource/src/snmp.rs:497`、`mac.rs:155`、`lldp.rs:97` | SNMP 采集目标地址直接 `format!("{}:{}", ip, port)` 拼接，IPv6 设备地址必然解析失败。同文件 `snmp_target()` 已专门处理 IPv6 方括号（另两处已使用），端口同步、MAC/ARP 表同步、LLDP 采集三处未复用——三个核心采集功能对 IPv6 设备全部失效 |
| 11 | `crates/ipma-resource/src/device/mac.rs:312-362` | `get_device_mac_table` 在事务内逐条吞错：任一 upsert 失败后 PostgreSQL 事务进入 aborted 状态，后续语句全部失败，"部分成功"路径不可达，单行失败导致整单 500（现实触发点：`device_macs.interface` VARCHAR(50)，SNMP ifName 超长即触发）。应逐条 savepoint 或事务外整形。`lldp.rs:430-461` 同款问题 |

### 后端 · models / visualization / x509（3 项）

| # | 位置 | 问题 |
|---|------|------|
| 12 | `crates/ipma-x509-manager/src/ca.rs:345-363` | `import_ca` 目录名冲突防护是 check-then-act：秒级时间戳 + `try_exists` 与 `create_dir_all` 之间无排他（create_dir_all 对已存在目录成功返回），并发/双击导入可交错写同一目录，最终留下"证书 A + 私钥 B"错配的 CA（`write_key_file` 的 remove+create_new 并发同路径双双成功，无 O_EXCL 保护）。`unique_cert_stem` 属同类窗口 |
| 13 | `crates/ipma-x509-manager/src/ca.rs:299-306` | `generate_ca` 无条件覆盖站点根 CA，库函数与调用方均无"已存在"保护：重复提交或误触发即整体替换根 CA，此前签发的全部叶子证书与 HTTPS 信任立即失效，旧私钥被销毁不可恢复。且先写 key 后写 cert，cert 写失败时留下"新 key + 旧 cert"错配状态 |
| 14 | `crates/ipma-models/src/models/cabinet.rs:67-68` 等 | 一批可空字段未用双层 Option，"清空"语义不可达：`CabinetUpdate.description`、`WorkstationUpdate.manager/description`、`EmployeeUpdate.phone/email/hire_date`、`NetworkUpdate.ipv4_cidr/ipv4_gateway`、`DeviceInterfaceUpdate.vlan_id`、`CabinetPositionUpdate.cabinet_id` 等单层 Option 配合 handler 的 `COALESCE` 写入，一经设置便无法清空（null 静默变成"不修改"）。与同代码库 `DeviceUpdate.hostname`、`CableLinkUpdate.cable_label`、`RoomUpdate.org_id` 已建立的三态口径直接矛盾 |

### 后端 · init / data-management / organization（3 项）

| # | 位置 | 问题 |
|---|------|------|
| 15 | `crates/ipma-init/src/handlers/database_ops.rs:297-322` | SQL 导入的 psql 缺少 `-v ON_ERROR_STOP=1`：psql 默认遇 SQL 错误继续执行且退出码为 0，`backup_and_drop_for_rebuild` 已删库后的语句级失败（版本不兼容语法、约束冲突）会被当作成功返回，半成品恢复被报告为成功；用户清理备份即造成不可逆数据缺失。宜加 `--single-transaction` |
| 16 | `crates/ipma-data-management/src/export.rs:435-447` | 导出防公式注入转义与导入不对称：导出对 `=+-@` 开头单元格加 `'` 前缀，导入端无对应剥离——员工电话 `+86…` 导出为 `'+86…`，再导入把撇号一并入库（静默数据变异）；`-` 开头的合法数值再导入解析失败。导出→导入往返一致性被破坏 |
| 17 | `crates/ipma-init/src/utils.rs:21-83` 与 `crates/ipma-data-management/src/backup.rs:8-74` | `PgPassFile`（含创建/Drop 清理全套逻辑）与 `DatabaseConfig` 跨 crate 复制粘贴，两处几乎逐行相同，违反"跨 crate 共享类型放 ipma-common，不得复制副本"；单侧修复后行为将分叉 |

### 前端 · 核心（2 项）

| # | 位置 | 问题 |
|---|------|------|
| 18 | `web/static/js/login.js:414-418` | 验证码触发判断永远为 false：`messageKey === "server.auth.captcha_required"` 比较的是已就地翻译后的文案（`translateServerMessage` 已把 message 翻译，字典有译文），比较恒 false——服务端"需要验证码/验证码错误"的即时信号被丢弃，客户端 `failures < 3` 时用户收到提示但验证码框仍隐藏，需再盲试 2 次才由 `failures >= 3` 兜底显示 |
| 19 | `web/static/js/utils/apiClient.js:146` | 429 重试递归调用 `makeRequest` 未传第 5 参 `skipAuthCheck`（默认 false）：被限流的登录类请求重试后若返回 401，会按普通请求处理——先 refresh（必然失败）→ 弹"令牌已过期"→ 1.5s 跳转 `/index.html`，登录失败触发 refresh/跳转、丢失用户输入 |

### 前端 · 模块 A（2 项）

| # | 位置 | 问题 |
|---|------|------|
| 20 | `web/static/js/modules/authManager.js:144-153` | 外部登录 2FA 重试固定只发 `{username, password, remember_me, code}`，未携带验证码字段：后端 LDAP 在失败计数达阈值时强制校验验证码，重试请求缺验证码必返回 `captcha_required`，且 2FA 视图无验证码输入框可满足；TOTP 输错还继续累计失败计数。fail2ban/验证码联动激活后，外部账户即使动态码正确也无法完成登录。凭证快照应同时快照验证码字段 |
| 21 | `cabinet.js:234-313`、`cableLink.js:65-155`、`device.js:78-174` | 三大资源列表加载无请求序号防护：快速翻页/切排序时多个不同 URL 的 GET 并发，旧响应晚到按旧参数渲染，表格内容与排序图标/分页状态不一致且不自愈。同项目 ipmanager.js 已有 `ipListRequestSeq` 防护，这三处未对齐（空页回退三处均已正确实现） |

### 前端 · 模块 B（1 项）

| # | 位置 | 问题 |
|---|------|------|
| 22 | `web/static/js/modules/scheduledTaskManager.js:70-76, 253-318` | 定时任务表单提交无任何防重复提交守卫：`scheduled-task-form` 不在 eventManager 的 `RESOURCE_FORM_CALLBACK_MAP` 内、由本模块直接绑定 submit，handler 内既不禁用提交按钮也无 in-flight 标志。请求在途时双击保存/连按回车触发两次 `apiPost`，重复创建同名定时任务（mac_sync 任务会双份周期执行）。同文件集其余提交函数均有守卫，唯独此处缺失 |

### 前端 · 可视化（2 项）

| # | 位置 | 问题 |
|---|------|------|
| 23 | `visualizationManager.js:139-144` + `SVGDataManager.js:126-128` | `loadSavedLayout` 把"代次过期"与"无布局"混同为同一返回值 false：快速连续切换房间时，前一次加载因过期返回 false，调用方随即 autoDraw 并 `saveLayout()` 以过期 roomId 把自动网格排布**持久化**，覆盖该房间原有已保存布局（落库不可逆）。代次过期应返回可区分值（如 null）供调用方放弃 |
| 24 | `web/static/js/modules/visualization/SVGRenderer.js:252` | `crypto.randomUUID()` 依赖安全上下文：HTTP 部署（`http://内网IP`）下该 API 不存在，房间门未保存过（`drawDoor(null)`）时抛 TypeError，工位视图加载与自动排布两条路径整体失败。应回退 `crypto.getRandomValues` 拼装或自增标识 |

### CSS（2 项）

| # | 位置 | 问题 |
|---|------|------|
| 25 | `index.html:9-10` + `toasts.css:14-43` | 登录页未加载 variables.css，toasts.css 引用的十余个变量全部悬空且无 fallback：含 var 的 border-left/box-shadow/transition 整条声明失效——登录页 toast 变成无彩色边条、无圆角、无阴影、无进出场过渡（类切换 + transition 是其唯一显隐机制）的白块 |
| 26 | `variables.css:101` + `modalLoader.js:17-18,187` | tooltip 固定 1200 与无上限递增的模态内联 z-index 不自洽：第 4 层模态内联 z-index 即 1200（tooltip 首次悬停才创建、modal 均在其后 appendChild，tooltip 反而被盖），第 5 层起必然沉入模态之下。设备→统一端口→端口冲突→确认弹窗已可达 4 层，模态内大量 `data-tooltip` 按钮提示在该深度不可见。应将 tooltip 提到远高于堆叠上限或限制模态最大层数 |

---

## 三、P2（121 项，按域归纳）

### 后端 · src/ + ipma-common（11 项）

1. `config.rs:527-536,645-654`：`AppState.config` 是启动快照且无刷新路径——`update_session_timeout_config` 等只写磁盘，GET 永远返回旧值，JWT 有效期也按旧值签发，变更重启才生效且无提示。
2. `src/log/notification.rs:63-65`：`page_size/offset` 用 `format!` 进 SQL + AssertSqlSafe 而非 bind，仅靠 Pagination 钳制不变量保证安全（同项目 login.rs/operation.rs 已 bind），属不必要的 AssertSqlSafe 误用。
3. `ipma-common/src/db.rs:146,192-215`：`query_timeout_secs` 被校验、传递、持久化，但 `create_pool` 从未使用——"查询超时"配置对用户完全无效，死配置项。
4. `src/log/forwarding.rs:44-52`：`load()` 吞掉所有 DB 错误返回"关闭"默认配置，DB 故障时安全管理员看到看似正常的默认配置，真实配置被静默掩盖。
5. `forwarding.rs:146-158`：syslog 仅连接有 3s 超时，`write_all` 无超时——服务端接受连接但不读取时 `test_forwarding` 请求永久挂起，后台路径任务与连接持续泄漏。
6. `forwarding.rs:74-86`：`protocol` 空串可绕过校验并被持久化，send 中非 "tcp" 一律按 UDP 处理。
7. `forwarding.rs:160-166`：UDP 套接字固定绑定 IPv6 通配地址，`bindv6only=1` 主机上所有 IPv4 syslog 目标发送失败（静默丢失）。
8. `ipma-common/src/rate_limit.rs:22-56`：`extract_user_id_from_parts` 直接 base64 解码 JWT payload 取 `sub` 不验签——可伪造任意 `sub` 填满受害者限流桶（定向 DoS）或为自己开桶绕过用户维度限流。
9. `src/main.rs:194-200`：localhost CORS 逃生门用前缀匹配，`http://localhost:8080.evil.com` 同样通过且 `allow_credentials(true)`（受默认关闭的开关缓解）。
10. `ipma-common/src/net.rs:180,245-255`：`is_trusted_proxy` 将全部 RFC1918 私网判为可信且 `peer_trusted` 缺省 true——TCP 直接暴露时内网任意客户端可伪造 `X-Real-IP` 轮换身份绕过 IP 限流与 fail2ban。
11. `config.rs:1008-1085`：`register_service` 写入 `Restart=always` 并 `systemctl start`，但当前独立进程仍持有 UDS——systemd 单元以 5 秒间隔无限崩溃重启循环，handler 却立即返回成功；缺"注册后让位"流程。

### 后端 · ipma-auth + ipma-scheduler（15 项）

1. `captcha.rs:106-140`：验证码答案以明文文本嵌入 SVG `<text>`，脚本正则提取即可 100% 通过，验证码门槛对自动化零成本失效（fail2ban 兜底仍在）。
2. `sso.rs:455-481`：SSO 回调路径完全未接 fail2ban/验证码，五个登录端点中唯独此处签发前不查封禁。
3. `login.rs:1091-1176`：`send_two_factor_code` 不做封禁检查，被封禁攻击者仍可持续在此端点爆破口令（每次真实 bcrypt）。
4. `user.rs:262-292`：`delete_user` 末位管理员保护的 `FOR UPDATE` 仅锁目标行，COUNT 为普通 MVCC 快照读——两个 secadmin 并发分别删除两个 admin 时双方均看到对方未提交删除，最终管理员归零。
5. `task_log.rs:83-91`：`update_next_run_at` 每 5 分钟同步覆盖 `updated_at`，破坏"最后配置变更时间"语义。
6. `scheduler.rs:80-153`：任务重叠保护标志在 panic 后永久卡死（`running.store(false)` 不执行），该任务静默停摆无自愈。
7. `login.rs:70-88`：`EMAIL_SEND_RECORDS` 键永不回收（仅命中同键时 retain），公开端点可用海量伪造 IP+邮箱组合无上限增长内存。
8. `login.rs:245` vs `1295`：吊销点比较口径不一致（`<=` vs `<`），恰落在吊销秒上签发的令牌出现"中间件拒绝但可刷新"缝隙。
9. `login.rs:436-440`：外部账户存在性可被枚举（专属错误且无 dummy bcrypt 延时），泄露 LDAP/SSO 账户名单。
10. `login.rs:861-872,906-913`：`login_with_two_factor` 用户不存在与禁用分支漏写 login_logs，登录审计存在盲区。
11. `app_fail2ban.rs:634-660`：手动封禁在 fail2ban 停用时静默无效（写入 banned_until 但 is_key_banned 恒 false），操作成功但不生效，无任何提示。
12. `user.rs:104-120`：`create_user` 唯一性先查后插无事务，并发同名一方 500 而非 409。
13. `cron.rs:111-179`：字段无数值范围校验（分钟写 75 被接受，靠整年扫描后才报错）；`1-5/2` 标准范围步进语法被整体拒绝。
14. `sso.rs:339-341,456-461`：TOTP 动态码经 URL query 传输，落入反代日志/浏览器历史/Referer。
15. `login.rs:637-653`：邮箱验证码登录的禁用账户路径未计入 fail2ban。

### 后端 · ipma-resource（15 项）

1. `network.rs:601-604,631-707`：仅换区域不改 CIDR 时跳过 CIDR 归属新区域的校验，存量 CIDR 可能不属于目标区域。
2. `network.rs:766-769`：存量 IP 越界检查失败日志沿用 IPv4 查重键（复制粘贴），误导排障。
3. `network.rs:1220-1244`：`delete_network_region` 检查与 DELETE 不在同一事务（TOCTOU），并发创建网段以 FK 500 收场。
4. `network.rs:454-483,743-809`：CIDR 重叠校验与写入不在同一事务且无唯一约束兜底，并发创建重叠网段可同时落库。
5. `room.rs:1048-1068`：`sync_cabinet_children` 删除机柜未预检被线路引用的配线架，级联触发自定义异常 → 500（独立删除路径已有预检）。
6. `cable_link.rs:271-303,347-490`：不预检端点存在性，依赖触发器 P0001 → 500 而非 422。
7. `ip.rs:290-316`：请求显式指定 network_id 可被自动检测静默覆盖（与批量路径语义不一致）。
8. `ip.rs:946-956`：auto_assign_ip 把并发 23505 映射为 422 而同文件 create_device_ip 映射 409，口径不一。
9. `workstation.rs:75`、`position.rs:66-69`、`cabinets.rs:43`、`crud.rs:29-39`、`network.rs:86-88`：列表过滤非法 UUID 静默退化为全量（与其他端点显式 422 不一致）。
10. `ip.rs:209`、`cable_link.rs:168`：`pagination.offset as i32` 截断，超大 page 时 OFFSET 为负 → 500。
11. `lldp.rs:430-461`：事务内逐条吞错同 mac.rs（P1-11 已述及）。
12. `crud.rs:391-416,738-763`：create/update_device 响应携带 SNMP 凭据密文（列表路径已脱敏，口径不一）。
13. `crud.rs:113/119`：get_devices SELECT 列重复选取 `d.room_id`（冗余死列）。
14. `network.rs:882-893`：delete_network 的 cabinet_network_count 检查为不可达死逻辑。
15. `room.rs:795-801`：`sync_room_children` 在事务外读取 room_type（轻微 TOCTOU）。

### 后端 · models / visualization / x509（10 项）

1. `ipma-visualization/src/layout.rs:200-208`：cabinet 布局分支未对 id 去重，重复条目被误判为归属非法（workstation 分支与 topology 都已去重）。
2. `layout.rs:26-34` vs `87-89`：rotation 校验允许 ±1e6 任意值但落库钳到 0..=360，-90 存为 0、450 存为 360，保存与回读不一致且无报错。
3. `layout.rs:129-167`：布局归属预检与 upsert 同事务但非原子（READ COMMITTED 两快照，TOCTOU）。
4. `generate.rs:129`：CA 剩余不足 1 天时 `max(1)` 使叶子 not_after 晚于 CA（链提前断裂）；过期 CA 仍可按 1 天签发（签发前不校验 CA 有效期）。
5. `generate.rs:251-260`：无法解析的 SAN 被静默丢弃（仅 warn 日志），部分无效时证书照常签发，用户无感知请求的主机名不在 SAN 中。
6. `ca.rs:176-197`、`generate.rs:54-83`：DN 的 OU/state/locality 无长度上限直接进入证书。
7. `models/ip.rs:95-140`、`device.rs:452`：`DeviceCreate.cards → ports → ips` 嵌套链未挂 `#[validate(nested)]`，模型声明与实际校验点（handler 内逐项）脱节，防御纵深缺口。
8. `models/cable_link.rs:60,74` 等一批：`length_m` 无范围校验、多个枚举字段无白名单/长度校验、CIDR/网关格式不校验、`DeviceMacCreate.ip_address/vlan_id` 无校验、`IpManagerUpdate.status` 无白名单——均依赖 handler/DB 兜底且错误口径不一。
9. `topology.rs:646-658`：逻辑连线成员端口防重仅靠预检，DB 无 device_port_id 全局唯一约束，并发可让同一端口属于两条聚合链路。
10. `topology.rs:501-514`：存储连线成员解析静默吞错，side 之外一律归 target（解析层无防御）。

### 后端 · init / data-management / organization（11 项）

1. `ipma-organization/src/lib.rs:763-779`：组织更新中 description 未传（None）与空串同样被置 NULL，部分更新请求会意外清空描述。
2. `database_ops.rs:149-177`：`import_database_api` 是 `create_database_api` 的完全重复（无任何导入动作却返回"已导入"），误导性死代码。
3. `database_ops.rs:348-381`：`clear_database` 不备份直接删表，与其他重建路径的备份判据不一致，误操作无恢复手段。
4. `status.rs:21-38`：`check_db_status` 仍带建库副作用（`ensure_database_and_schema`），与 `check_init_status` 的只读口径矛盾。
5. `check.rs:673-705`：列宽契约检查嵌套在逐表循环内，重复执行约 37×4 次冗余查询。
6. `check.rs:741-747`：必需索引自检仅含拓扑逻辑连线一条，`uq_cable_links_endpoint_pair`、`uq_network_cidrs_ipv4/ipv6` 缺失时自检照过，旧库不触发幂等补建。
7. `database_ops.rs:245-258`：上传 SQL 文件在"已创建、未接管"窗口失败时泄漏（write_all 失败早于 TempSqlFile 接管）。
8. `init/config.rs:27-42`：原子写临时文件仅在 rename 失败路径清理，write 中途失败残留；rename 前未 fsync。
9. `operations.rs:54-85`：备份文件先按 umask（0644）落盘、成功后才补 chmod 0600；pg_dump 失败不清理残留（对照 data-management 的 create_new(0600) 口径）。
10. `backup.rs:142-157`：备份文件名秒级时间戳 + create_new(true)，同秒第二次备份（手动+定时并发）直接报错。
11. `import/mod.rs:290-336`、`spec.rs:82-113`：CSV 导入对组织模板/组织的结构校验弱于 API 端（levels 不走根唯一/环检测，type_path 不校验可解析性），可导入病态数据使相关接口持续报错。

### 前端 · 核心（12 项）

1. `apiClient.js`：`isPublicAuthEndpoint` 未收录登录页实际调用的 `/api/auth/methods`、`/api/auth/init-status`、`/api/auth/captcha`、`/api/certificate/ca/info`——这些端点 401 时登录页弹"令牌已过期"并定时跳回自身形成循环。
2. `modalLoader.js:184-188`：z-index 计数随关闭递减，"先关低层再开新层"序列下新模态框 z-index 低于仍打开的旧模态框，被遮挡。
3. `modalLoader.js:213-220`：`closeModal` 只重置模态框内第一个 form，多表单模态框关闭后其余表单残留旧输入（含 hidden id，可能误走更新分支）。
4. `sessionManager.js:5-11,41-47`：主存储由全局 localStorage 的 rememberMe 标志决定，两标签以不同 rememberMe 登录时读写漂移，可能产生两份不一致用户副本。
5. `network.js:27-29`：`"00"`/`"000"` 因 `num === 0` 被豁免，`192.168.00.1` 可通过 isValidIPv4，与"拒绝前导零"口径不一致。
6. `ui.js:497-516`：`openSimpleListModal` 的 col.label 与 cells 未转义直接拼 innerHTML，完全依赖调用方契约，任一调用方传用户数据即成 XSS 汇点。
7. `icons.js:139-146`：`iconButton` 的 attrs 原样拼入按钮 HTML，调用方把用户数据拼进 attrs 时破坏属性边界。
8. `init_index.html:71,91-96,104`：四处硬编码中文无 data-i18n（"正在检查数据库状态..."等），英文界面仍显示中文。
9. `main.html:100`：`user-info` 的 `data-tooltip="Current user"` 缺 `data-i18n-tooltip`，永不翻译。
10. `helpers.js:75-80`：`saveToStorage` 使用迭代器 helper 链（很新的 ES 特性），不支持的引擎抛 TypeError 且被静默吞掉，缓存持久化无声失效。
11. `helpers.js:47-81` + `sessionManager.js:49-53`：`ipma_cache` 持久化缓存不随登出清除，共享浏览器换账号后 TTL 内可读到前一账号的缓存数据。
12. `apiClient.js:28-37`：`#hashBody` 对 FormData File 值字符串化、长 body 截断 200 字符，非 GET 同键覆盖 `#pendingRequests`（当前仅 GET 去重，触发面窄）。

### 前端 · 模块 A（13 项）

1. `fail2banManager.js:94,125`：`banned_ips`/`tracked_ips` 无空值防护（字段缺失抛 TypeError 被吞，页面显示加载失败）；配置回填无 `?? ""` 兜底，缺失时输入框填成 "undefined"。
2. `dashboard.js:168`：IP 状态值未转义直接插入含 class 属性的 innerHTML（同文件 :378 已转义，此处不一致；当前写入路径硬编码，属二阶防御缺失）。
3. 多处 `t(key) || fallback` 兜底永不生效（fail2banManager 15 处、cabinet.js:179、cableLink.js:35,489）——`t()` 缺键返回键名恒真值，键改名时输出裸键名而非英文兜底。
4. `eventManager.js:117-127,145,157,166`：提交/打开回调的 Promise 拒绝未捕获，`loadModule` 失败或回调抛错成为 unhandled rejection，用户无提示（inFlight 集合 finally 清理正确，防重入本体无误）。
5. `cableLink.js:350-378`：fillSelect 的 errorLabel 硬编码中文（"范围"/"机柜"/"设备"）。
6. `cableLink.js:354-382,508-546`、`device.js:540-549`：设备接口级联（范围→机柜→设备）走 fillSelect 无并发防护，快速切换旧响应覆盖新选项。
7. `ipma_init/api.js:82,99`：checkPostgreSQL 状态类只加不减，languagechange 重查后互斥状态类残留并存。
8. `ipma_init/ui.js:35-42`：showError 共用定时器互相覆盖，连续两条错误时第一条的定时器提前隐藏第二条。
9. `ipma_init/api.js:260-282`：handleAdminAccountSubmit 无防重入，遮罩只拦指针，焦点在输入框时连按 Enter 重复 POST。
10. `ipma_init/api.js:122,129`：success 但 data 为 null 时抛 TypeError，被 catch 后误报"网络错误"并暴露内部错误细节。
11. `fail2banManager.js:111-114,213`：showConfirm 第二参传字符串（应为对象），IP 上下文被静默丢弃。
12. `ipmanager.js:190-318`：IP 列表无空页回退（末页删完停留空页，其他列表均已实现）。
13. `device.js:746-788`：编辑回显时房间请求失败/不在列表时 room_id 提交为 null 而工位/机位仍被赋值，产生"有机位无房间"的不一致数据且无提示。

### 前端 · 模块 B（12 项）

1. `room.js:782`、`scheduledTaskManager.js:419`：room_type 未知值 / log.status 未转义直出 innerHTML（受后端枚举约束，纵深防御缺失）。
2. `log.js:318-333`：total_pages<=1 时完全跳过分页渲染，旧分页条残留可点击（通知表与其他列表均无条件走 appendPagination，行为不一致）。
3. `networks.js:49,161`、`room.js:757`、`log.js:387`：接口 success:false 被静默渲染为"空数据"，无任何错误提示（对照 userManager/log 的正确做法）。
4. `networks.js:36-213`、`room.js:747-843`：删除末页最后一条后无空页回退（log 通知与 userManager 已实现）。
5. `systemManager.js:273,874,286`：sessionStorage `configUpdated` 标记只置不清，此后每次进系统页都误弹"需重启"。
6. 多文件：openModal 返回值被忽略 + elementCache 空值防护不一致——模态框模板加载失败路径在 networks/position/workstation/room/organization/systemManager 多处抛未处理 TypeError（对照 userManager/log 的完整防护）。
7. `room.js:74-83,341-344`：EventHandler.clear() 对 WeakMap 调用 .entries()（必然 TypeError 的不可达死代码）。
8. `organization.js:300,268-275`：presetType/data-child-type 死代码（data-child-type 从未被设置）。
9. `navigation.js:73-75,229-232,286-296`：初始重定向触发 dashboard 双载；hashchange 对未知 hash 无兜底，主内容区整体空白。
10. `systemManager.js:80-148`：smtp/ldap/sso/系统配置表单及通知/密码策略/日志外发保存按钮无防重复提交（幂等写入，影响低于 P1）。
11. `organization.js:450-496`：新增根节点时 typeSelect 的 disabled 与旧选项不复位，历史多根模板数据下可能提交与模板不符的根类型。
12. `room.js:37,52`、`scheduledTaskManager.js:100,108`、`unifiedDevicePorts.js:712`：errorLabel 硬编码中文残留、同步结果 toast 用全角中文逗号作固定分隔符。

### 前端 · 可视化 + 模态框（11 项）

1. `TopologyRenderer.js:656-679`：updateConnectionPaths 重画集合不含"邻居的非邻居连线"但计数器全局清零，重画连线从 slot 0 取锚点与未重算的旧连线重叠/错位。
2. `TopologyVisualization.js:559-568`：机柜级推挤未传 fixedKey，被拖机柜落定后可能被对称推移，落点偏离拖放位置。
3. `TopologyVisualization.js:537-538`：拖拽路径负坐标直接入库（无钳制），与坐标模态框"拒绝负值"、工位视图"钳制为 0"三处口径互不一致。
4. `TopologyModal.js:94-95`：坐标输入清空后 `Number("") === 0` 通过校验，节点被静默移到原点（输入框无 required，保存按钮 type=button 不触发表单校验）。
5. `visualizationManager.js:309-331`：loadTopologyConnectionPorts 无请求代次，慢响应覆盖新设备端口列表，提交 port_id 与 device_id 不匹配的组合。
6. `TopologyModal.js:28,41-110`：open/_render 无重入保护，连续打开两个节点产生孤儿浮窗（旧浮窗"移除"按钮引用被覆盖的 currentDeviceId 会删错设备）；打开后立即 close 浮窗仍会弹出。
7. `TopologyDataManager.js:214` + `TopologyVisualization.js:359`：autoDiscover 回退对象键名 `added_connections` 与展示读取的 `discovered_connections` 不一致，回退分支连线数恒显示 0。
8. `TopologyVisualization.js:132-135`：autoDrawCabinetPositions 中刚赋值的 token 自比较恒为假，不可达死代码。
9. `TopologyCore.js:507-518`、`visualizationManager.js:391-394`：两个创建连线入口只校验自环不校验重复连线（已有 this.connections 数据可低成本补齐）；另 fetchDevicePorts 固定 page_size=200，超 200 口设备端口列表静默截断。
10. 模态框 HTML：topology-connection-modal 4 个、cable-link-modal 6 个、pull-mac-modal 2 个、org-template-editor-modal 1 个、scheduled-task-modal enabled 复选框等控件缺 `name`（值由 JS 按 id 读取，表单语义不完整）。
11. 模态框 HTML：多数对话框 role="dialog" 无 aria-labelledby/aria-label（仅 5 个已关联）；arp/subnet-usage 的 role="tab" 面板缺 role="tabpanel" 关联。

### CSS（11 项）

1. `helpers.css:43-47` vs `forms.css:7-16`：`.form-group-inline label` 重置规则特异性不足整体失效（0,1,1 被 0,2,1 压制），属死规则。
2. `ipma_init.css:237-247`：状态颜色类选择器仍会命中容器本身（颜色规则未像字号规则那样用 `>` 限定），新增无色子元素即被整块染成状态色。
3. `network-usage.css:376-379`：`.no-ipv6-hint` 用边框色变量（#ced4da）当正文文字色，对比度约 1.4:1 近乎不可见，应使用 --color-text-muted。
4. `network-usage.css:271-287`：`.usage-status-badge/-active/-inactive` 三条死选择器（实际使用 badges.css 的类）。
5. `cards.css:1-32`：`.card`、`.card h3`、`.card-header` 死选择器（全仓只有 dashboard-card/chart-card）。
6. `visualization.css:28-39,437-446,839-844`：`#visualization .viz-filter-group`、`.zoom-indicator`、`#global-visualization .toolbar .toolbar-right` 三处死选择器。
7. `responsive.css:211-218`：`.notifications-header` 死选择器。
8. `forms.css:154-173,248-263`、`buttons.css:235-242`、`helpers.css:3-5,27-29`：`.form-row-container`、`.search-container`、`.inline-select`、`.btn-nowrap`、`.btn-full-width`、`.mt-sm`、`.d-inline-block` 批量死工具类。
9. `visualization.css:198-211`：画布 `.tooltip` 硬编码 z-index:1000，与全局 `.global-tooltip`（--z-tooltip 1200）形成双轨层级源；另 `--z-dropdown/--z-sticky/--z-fixed/--z-modal-backdrop/--z-modal-secondary` 五个令牌定义后零引用。
10. `unified-device-ports.css:4-10,96-117`：首行端口的 `.port-tooltip` 被滚动容器 `overflow-y:auto` 裁剪（组织树 `.org-icon-picker` 属同类边界）。
11. var() 交叉核对与 hidden 冲突排查全部通过（见下文正面结论）；断点数值、骨架屏尺寸均无问题。

---

## 四、复审正面结论（确认已修复 / 未发现问题）

第一轮 176 项中的绝大多数在本轮复审中确认已修复且实现正确，包括（抽样列举）：

- **P0×3 全部确认修复**：创建任务按钮监听器幂等守卫（dataset.handlerAttached）、工位渲染代次 + saveLayout 归属过滤（layoutOwnerIds 两条绘制路径均重建）、`.loading[hidden]` 规则。
- **fail2ban 检查键与记录键**经逐一核对（含 LDAP/SSO/邮箱码/2FA）均同源且经同一 `sanitize_identifier` 清洗，无键错位；TOTP 原子占位→校验→失败回滚正确；auth_middleware 对状态/tokens_invalidated_at/吊销逐请求查库且 fail-closed。
- **SQL 注入**：全部动态拼接为白名单/参数绑定，未发现注入面（notification.rs 的 AssertSqlSafe 误用属风格问题，见 P2）。
- **导入排序**：空串特判 0、深度单调、稳定排序，父严格先于子；**ZIP 上限**显式拒绝；**DROP 会话复位与备份判据**失败语义正确；**视图/触发器/索引 DDL 与 check 清单**逐项核对一致（37 表、6 视图 JOIN、29 表触发器、拓扑唯一索引、列宽契约）。
- **删除组织**子组织/房间/递归员工三层拦截完备；**环检测**全起点染色覆盖游离环；**密码 ≤72 字节、role 白名单、密文列宽匹配**计算精确。
- **私钥文件** create_new+0600 无"先写后 chmod"窗口；**证书/私钥匹配**导入路径公钥字节比对 fail-closed；**路径穿越**防护有效。
- **前端 XSS**：10 路审计逐一核对了全部 innerHTML/insertAdjacentHTML/SVG text 插值点，未发现实际可利用的 XSS；**alert/confirm 残留**为零；**前端监听器幂等守卫**（常驻元素）逐一核对无泄漏；**userManager 竞态 token**、**组织模板查重**、**cron 前端校验**、**hashchange 权限重定向**、**CSS.escape 覆盖**、**textContent 二次转义**、**版本号一致性（?v=01360 与 MODULE_VERSION）**、**分页契约**、**i18n 静态键**（复审中静态键全量比对 187+336 个均在字典）均确认正确。
- **hidden 属性与 display 覆盖**冲突排查：全站逐一核对，除已修复的两处外无同类隐患。

---

## 五、统计

| 复审范围 | P0 | P1 | P2 |
|------|----|----|----|
| src/ + ipma-common | 0 | 2 | 11 |
| ipma-auth + ipma-scheduler | 0 | 5 | 15 |
| ipma-resource | 0 | 4 | 15 |
| ipma-models + ipma-visualization + ipma-x509-manager | 1 | 3 | 10 |
| ipma-init + ipma-data-management + ipma-organization | 0 | 3 | 11 |
| 前端核心（app/login/utils）+ 主页面 HTML | 0 | 2 | 12 |
| 前端模块 A | 1 | 2 | 13 |
| 前端模块 B | 0 | 1 | 12 |
| 可视化 JS + 模态框 HTML | 0 | 2 | 11 |
| CSS 全量 | 1 | 2 | 11 |
| **合计** | **3** | **26** | **121** |

**与第一轮对比**：P1 由 38 → 26（且第一轮 P0×3 全部闭环，本轮 P0 均为新发现问题），P2 由 135 → 121。本轮发现集中在：修复引入或修复未覆盖的邻接路径（如 `disable_init_mode` 漏改、同步容器 nested 校验遗漏、429 重试丢参数）、更深层的一致性口径问题（三态 Option 未覆盖全部模型、删除链路保护不齐、cron 星期编号体系）、以及部署形态相关的新暴露面（X-Real-IP 信任、HTTP 无安全上下文）。
