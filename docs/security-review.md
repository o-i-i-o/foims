# IPMA 安全与关键模块缺陷分析报告

> 生成时间：2026-08-23 ｜ 分析范围：src/（auth、system、utils、main）、crates/ipma-init、crates/ipma-data-manager
> 方法：代码逐行审查 + 单元测试实证 + 运行环境 API 实测（UDS 直连）
> 严重程度：高（可导致权限突破/数据破坏）、中（可被利用但需条件）、低（健壮性/合规问题）

## 总体评价

代码库整体安全意识较好：SQL 全部参数化（`AssertSqlSafe` 仅用于白名单 ORDER BY）、AES-256-GCM 用法正确（随机 96-bit nonce 前置存储）、bcrypt cost=12、SSO 具备 PKCE + nonce + state 一次性消费、LDAP 过滤器做了 RFC 4515 转义、ZIP 导入有 zip-slip 与 zip-bomb 防护、备份与密钥文件 0600/0700。

但存在**一个贯穿性架构单点**（IP 头信任链）和若干边界缺陷，按模块列出如下。

---

## 一、初始化流程（main.rs / config.rs / ipma-init）

| # | 严重度 | 问题 | 位置 | 说明 |
|---|---|---|---|---|
| I-1 | **高** | UDS socket 权限 0666 + 完全信任 `X-Real-IP` | `src/main.rs:173-192`（已代码验证）、`src/auth/login.rs:143-175` | `/api/init/*` 的 localhost_only 中间件仅认 `X-Real-IP` 头；UDS 0666 允许本机任意进程直连并伪造该头，从而访问清库/重建库/导入 SQL（psql 执行任意语句）等接口。nginx 的 127.0.0.1 白名单不覆盖直连 UDS 的流量。**建议**：UDS 收紧为 0660+属组（nginx 所属组），改用 systemd socket FD 传递；去掉 X-Forwarded-For 首段回退（`utils/common.rs:427-460`） |
| I-2 | 中 | init 验证码非一次性 | `crates/ipma-init/src/verification.rs:66-100`（已代码验证） | `verify_code` 成功后不消费验证码，15 分钟内同一验证码可重复用于 init/clear/create/import 多个危险操作；生成端点 `/api/init/verification-code` 不校验 init.enabled。**建议**：验证成功即作废；生成端点加开关检查 |
| I-3 | 中 | init 完成到重启之间存在"毁库窗口" | `handlers/init.rs:103-107`、`main.rs:228` | `init.enabled` 是启动时内存快照，关闭 init 仅改写配置文件；调用 restart 前持有验证码者仍可 DROP DATABASE 重建。**建议**：配置写入成功后同步翻转 AtomicBool 内存开关 |
| I-4 | 中 | `init_system` TOCTOU 并发竞态 | `handlers/init.rs:42-101` | `SELECT COUNT(*) FROM users` 与 INSERT 之间无事务/锁，并发可创建多个管理员（role 仅长度校验）。**建议**：事务 + advisory lock |
| I-5 | 中 | `/tmp` 固定路径写入（symlink 竞争） | `handlers/database_ops.rs:184-239`（导入 SQL）、`src/system/config.rs:271-309`（`/tmp/ipma_restart.sh`） | root 服务向可预测路径写文件后执行；本地低权用户预置符号链接可劫持为任意文件写入/执行。**建议**：随机临时名 + `create_new`，或移至 /var/lib/ipma |
| I-6 | 低 | `.pgpass`/密钥/配置文件"先写后 chmod"窗口 | `ipma-init/src/utils.rs:33-53`、`ipma-data-manager/src/backup.rs:13-48`、`src/crypto.rs:47-56` | **建议**：`OpenOptions::new().mode(0o600)` 原子创建 |
| I-7 | 低 | init 状态接口回传 DB 错误原文 | `handlers/status.rs:25-34,101-146` | 错误串含连接 host/user，仅应入日志 |
| I-8 | 低 | 拼 postgres URL 未编码密码 | `handlers/database_ops.rs:56-59`、`status.rs:122-125` | 密码含 `@ : /` 时连接失败（与 `operations.rs:75-81` 的编码实现不一致） |

## 二、用户认证（src/auth）

| # | 严重度 | 问题 | 位置 | 说明 |
|---|---|---|---|---|
| A-1 | **高** | 爆破防护依赖可伪造 IP 头 | `utils/rate_limit.rs:338-344`、`system/app_fail2ban.rs:195-232` | 与 I-1 同根因：绕过 nginx 后每个请求换一个假 `X-Real-IP` 即可令 login_limit=5/min 与 fail2ban 全部失效。fail2ban 仅按 IP 计数（无用户名维度）。**建议**：修复信任链；fail2ban 增加用户名维度 |
| A-2 | 中 | 资源/日志/SNMP 接口无 RBAC | `routes/mod.rs:199-484` | 资源 CRUD 仅要求登录，无 AdminUser。**已实测确认**（见 `api-test-report.md`"普通用户创建组织"用例 200 通过）：普通用户可任意增删改组织/网段/设备数据、发起 SNMP 探测、读登录日志。**建议**：写操作与敏感查询至少限 admin，或引入权限矩阵 |
| A-3 | 中 | `update_system_config` 响应回传明文 DB 密码与 JWT secret | `system/config.rs:174-177`（已代码验证） | 对比 `get_system_config` 有 `***` 脱敏；拿到 JWT secret 可伪造任意管理员令牌。**建议**：返回前同样脱敏 |
| A-4 | 中 | 登录用户名枚举（时间侧信道） | `auth/login.rs:195-217` | 用户不存在时跳过 bcrypt 直接返回。**建议**：不存在时执行一次 dummy bcrypt verify |
| A-5 | 中 | 邮箱验证码登录无 fail2ban 联动 | `auth/login.rs:333-480` | 对比密码登录/LDAP 登录均有 `is_ip_banned` + `record_login_failure`；6 位数字码爆破仅剩可绕过的限流；错误尝试不作废验证码 |
| A-6 | 中 | TOTP 重放防护仅记录最后 1 码 | `auth/login.rs:36-64,629-663` | skew=1 时相邻窗口历史码可重放。**建议**：保留最近 2-3 个已用码或记录 `last_used_step` |
| A-7 | 低 | 角色固化于 JWT | `auth/extractor.rs:40-53` | 降权后旧 access token 有效期内仍为 admin（15m）。敏感路由建议复查 DB 角色 |
| A-8 | 低 | 刷新令牌旋转时撤销旧 token 失败仍签发新 token | `auth/login.rs:916-920` | 撤销失败仅记日志，旧 refresh token 继续有效 |
| A-9 | 低 | LDAP/SSO 登录不在严格限流名单 | `utils/rate_limit.rs:297-317` | 仅受 ip_limit=1000/min 约束 |
| A-10 | 低 | CORS 前缀匹配可被域名后缀绕过 | `main.rs:118-142` | `starts_with(allowed) && ends_with(":80")` 使 `http://localhost.evil.com:80` 被接受且 `allow_credentials(true)`。**建议**：精确匹配 Origin 的 host:port |
| A-11 | 低 | 空 Bearer 返回 `Some("")` | `auth/utils.rs:313-324` | `Authorization: Bearer `（空）应返回 None（单元测试已覆盖该现状） |

正面确认：SSO 有 state 一次性消费 + PKCE + nonce + iss/aud/exp 校验（`sso.rs:348-392`）；重置密码 `FOR UPDATE` + 单次 token + 全局令牌吊销（`login.rs:1039-1075`）；LDAP RFC 4515 转义 + 独立连接二次绑定 + 多条目拒绝。

## 三、数据库连接池 / 事务 / SQL

| # | 严重度 | 问题 | 位置 | 说明 |
|---|---|---|---|---|
| D-1 | 中 | `drop_all_tables` 的 `session_replication_role` 跨池连接失效/泄漏 | `ipma-init/src/operations.rs:154-184` | `SET session_replication_role` 是会话级，但后续每条 `.execute(pool)` 各取连接：DROP 可能落在未 SET 的连接上（FK 报错清库失败）；恢复 `'origin'` 也可能落在别的连接，使某条池连接永久停留 replica 模式（触发器/FK 静默失效）。**建议**：acquire 单一连接顺序执行整段 |
| D-2 | 低 | 多步写入无事务 | `auth/user.rs:104-140,201-225` | create_user 检查-插入竞态（靠唯一约束兜底返回 500）；UPDATE 与 tokens_invalidated_at 两条语句分离，第二条失败则降权未强制下线 |
| D-3 | 低 | 过滤参数未做 LIKE 转义（通配符注入） | `resource/network.rs:117-133,208-230` | `name_filter` 直接 `format!("%{}%")`；仅 `search` 走 `escape_like` |

池参数本身合理：max 10-20 / acquire 15s / idle 60s / lifetime 1800s / test_before_acquire / 30s 健康检查（`db.rs:275-285`）；角色级 statement_timeout=30s、lock_timeout=5s。

## 四、加密与密钥（src/crypto.rs）

- AES-256-GCM 用法正确：随机 96-bit nonce 前置存储、32 字节随机密钥、无硬编码；SNMP 口令日志只记 "set"/"none"。单元测试已覆盖篡改/错钥/短密文拒绝。
- C-1（低）：密钥路径硬编码 `/etc/ipma/`（`crypto.rs:28-33`），无注入点，不可测试也不可重定位；启动即 `fs::copy` 备份主密钥，每次启动产生新备份。
- C-2（信息）：CSV 导出将 SNMP 凭据解密为明文（`export.rs:25-27,467-482`，admin-only、注释声明为迁移设计）；TOTP secret 以二维码返回给发起者；init 验证码打印进日志。属既定设计，建议在文档中明示。

## 五、上传 / 导出 / SSRF

| # | 严重度 | 问题 | 位置 | 说明 |
|---|---|---|---|---|
| S-1 | 中 | CSV 导出公式注入 | `ipma-data-manager/src/export.rs:253-273,426-465` | 名称/描述等用户可控字段以 `= + - @` 开头导出后可触发 Excel DDE。**建议**：危险前缀加 `'` 转义 |
| S-2 | 中 | SNMP 任意目标探测（SSRF 面） | `resource/device/snmp.rs:511-647` | 任意登录用户提供任意 ip:port（仅禁回环/组播/链路本地，私网放行）+ A-2 叠加构成内网扫描原语。**已实测**回环防护生效（返回 `loopback_forbidden`）。**建议**：限 admin；修 IPv6 目标缺 `[]` 问题（`snmp.rs:264,336,447`） |
| S-3 | 低 | `169.254.169.254` 元数据端点检查为死代码 | `snmp.rs:588-604` | 该地址先命中 `is_link_local()` 返回 `link_local_forbidden`，专用分支不可达（单元测试已记录） |
| S-4 | 低 | SNMP "v1" 分支实际按 v2c 发包 | `snmp.rs:180` | `"v1" \| "v2c" => Auth::v2c(...)`，v1 选项形同虚设（单元测试已记录） |

ZIP 导入防护到位：zip-slip 拒绝、单条目 100MB/总量 500MB 限额、UTF-8+NUL 真实类型校验、整批事务、导入凭据重加密；证书下载/删除有文件名白名单（`x509-manager/src/transfer.rs:9-18`，已有 10 个单元测试覆盖）。

## 六、其他实证发现（源自本轮新增单元测试）

| 问题 | 位置 |
|---|---|
| 限流器浮点截断 off-by-one：`weighted_count` 的 `as u32` 截断使加权计数恒少 1（ip_limit=3 时第 5 次才拒绝） | `utils/rate_limit.rs` |
| validator 对 `Option<Option<String>>` 的 `#[validate(length)]` 完全不生效：`CableLinkUpdate.cable_label`、`DeviceInterfaceUpdate.mac_address/description` 等超长不报错，最终撞 DB VARCHAR 长度 → 500 | `models/cable_link.rs`、`models/device.rs` |
| `IpManagerUpdate.device_interface_id`、`RoomUpdate.org_id` 声明双层 Option 但未挂 `deserialize_some`，JSON null 清除语义不可达 | `models/ip.rs`、`models/room.rs` |
| `matches_cron_field` 步长 0 整数除零 panic（`*/0` 表达式）；`calculate_next_run` 5 字段表达式因秒位缺陷永不命中 | `ipma-scheduler/src/cron.rs` |
| `verify_code` 时钟回拨 u64 下溢 panic（debug） | `ipma-init/src/verification.rs:79` |
| `find_available_ips_in_cidr` 的 `max_count=Some(0)` 仍返回 1 个地址 | `resource/ip.rs:715-760` |

## 七、优先修复建议（Top 6）

1. **I-1 / A-1**：UDS 权限 0660 + IP 信任链重建（同时收敛初始化边界、限流、fail2ban 三个攻击面）
2. **A-3**：`update_system_config` 响应脱敏（JWT secret 泄露 = 任意伪造管理员）
3. **I-2 / I-3**：init 验证码一次性化 + 关闭开关内存即时生效
4. **I-5**：/tmp 固定路径写入改随机临时文件
5. **A-2 / S-2**：资源写操作与 SNMP 探测的 RBAC
6. **S-1**：CSV 导出公式注入转义
