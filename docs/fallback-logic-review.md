# 前后端回退 / 保底逻辑梳理

> 范围：`src/`、`crates/`（后端 Rust）与 `web/static/js/`（前端 JS）。
> 方法：模式扫描（`unwrap_or*` / `COALESCE` / `||` / `??` / catch 降级 / 多分支响应解析）+ 逐处人工归类。
> 评审标注：✅ 合理 · ⚠️ 可疑（建议复核）· ❌ 不合理/不必要（建议修改或移除）。
> 状态：**初审发现的 12 项 ❌/⚠️ 已全部整改并通过二次审计**（见下文整改记录）；
> 文中行号为二次审计时（v0.15.67 整改后）的快照位置。

## 一、整改记录（初审 ❌/⚠️ 项 → 处理结果）

| # | 位置 | 初审问题 | 整改方式 | 二次审计 |
| --- | --- | --- | --- | --- |
| 1 | `SVGDataManager.js` `fetchIps` | 对同一 API 响应尝试 5 种结构（数组/`items`/`data`/`ip_managers`/`ips`） | 仅保留 `items` 一种解析；失败路径统一 `showToast`；补 `page_size=1000`（原缺省会因默认分页 20 条截断 IP 映射） | ✅ 已复核 |
| 2 | `crates/ipma-data-management/src/logs.rs` `clear_logs` | 清理日志 `days` 缺省回退 `0`（=删除全部日志） | `None` 时返回 `server.logs.days_required` 校验错误；定时任务执行器维持缺省 30 天（非危险默认）；前端本就显式传值 | ✅ 已复核 |
| 3 | `src/auth/ldap.rs`、`src/auth/sso.rs` | 服务账号密码/secret 解密失败静默回退空串，以空凭据继续绑定 | 解密失败记 `log_error`（`password_decrypt_failed`/`secret_decrypt_failed`）并返回 `None`（配置视为不可用），杜绝空密码绑定 | ✅ 已复核 |
| 4 | `TopologyRenderer.js`、`TopologyVisualization.js` | 节点坐标 `x \|\| 100` 等 falsy 陷阱（合法坐标 0 被改写） | 全部改 `??`，仅 `null/undefined` 时兜底；`SVGRenderer.js` 工位/机柜坐标同步改 `??`，工位默认尺寸统一为与自动布局一致的 160×160 | ✅ 已复核 |
| 5 | `SVGRenderer.js` / `SVGDataManager.js` 机柜容量 | 容量缺省一处 42、一处 45，口径不一 | 统一常量 `DEFAULT_CABINET_CAPACITY = 42`（与后端建表 DDL `capacity INTEGER NOT NULL DEFAULT 42` 一致），两文件共享导入 | ✅ 已复核 |
| 6 | 可视化各 DataManager `catch → return []`（约 16 处） | 请求失败静默降级空数据，用户无从区分"无数据"与"加载失败" | 两文件新增 `_notifyLoadFailure`（`console.error` + `showToast(viz.data_load_failed)`），失败路径全部接入 | ✅ 已复核 |
| 7 | `src/system/config.rs` `get_notification_settings` | 收件人 JSON 解析失败 `unwrap_or_default()` 静默清空，MAC 变更通知失效 | 解析失败记 `log_error` 并返回 `server.notification.recipients_parse_failed` 错误，不再静默 | ✅ 已复核 |
| 8 | `crates/ipma-data-management/src/export.rs`（约 15 处） | 导出 CSV 字段缺失静默补空串/"?"，无日志 | 引用 ID 非空但名称解析失败时 `log_warn(log.import_export.export_ref_missing)`（含 `info_to_csv` 伴随列与拓扑连线回显两处路径）；可空列的 `Null → 空串` 行为保留（与 COALESCE 语义一致） | ✅ 已复核 |
| 9 | `src/auth/login.rs` `dummy_bcrypt_verify` | dummy 哈希生成失败回退空串，时间侧信道缓解静默失效 | 运行时生成失败回退到内置静态合法 bcrypt 哈希常量（`DUMMY_BCRYPT_FALLBACK`，cost 12 与 DEFAULT_COST 一致），缓解永不失效；空串守卫移除 | ✅ 已复核 |
| 10 | `src/auth/utils.rs` `JwtUtils::new` | JWT 时长解析失败静默回退 3600s/604800s | 解析失败直接返回 `Err`（启动失败并指明哪个字段非法）；`config.toml` 默认 `15m`/`7d` 合法不受影响 | ✅ 已复核 |
| 11 | 前端 `items \|\| result.data \|\| []` 等冗余分支（TopologyDataManager、position.js、visualizationManager.js、dashboard.js、ipmanager.js） | 后端分页键固定为 `items`，多余分支永不命中 | 统一改为 `result.data?.items ?? []`；返回裸数组的接口（拓扑节点/连线、room-cabinets）保留 `Array.isArray` 判定 | ✅ 已复核 |
| 12 | `src/resource/device/interface.rs` `create_device_interface` | `physical_type`/`interface_role` 缺省静默补 `rj45`/`business` | 字段必填：缺失返回 `physical_type_required`/`interface_role_required` 校验错误；该端点无前端 POST 调用方（设备保存走 nic 流程、前端表单自带默认值），无兼容性影响 | ✅ 已复核 |

### 1.x 顺带清理（二次审计新增发现）

| 位置 | 问题 | 处理 |
| --- | --- | --- |
| `SVGDataManager.js` `fetchWorkstationsByRoom` | 未带 `page_size`，房间工位 >20 时被默认分页截断 | 补 `page_size=1000` 并收敛为 `items` 单一解析 |
| `SVGDataManager.js` `fetchCabinetPositions` | 死代码（无调用方，机位已内嵌于 room-cabinets 响应） | 移除 |
| `update_device_interface`（interface.rs） | PUT 缺 `physical_type`/`interface_role` 时无条件绑 `None` 会将列写 NULL，与注释声称的 COALESCE 保留旧值不符 | ⚠️ 留待后续修复（超出本次保底清理范围，涉及 PATCH 语义调整） |

## 二、二次审计残留 ⚠️ 项（维持现状，建议后续迭代处理）

| 位置 | 模式 | 说明 |
| --- | --- | --- |
| `crates/ipma-x509-manager/src/ca.rs` `try_exists().await.unwrap_or(false)` | stat 失败视为文件不存在 | 权限异常时会误报"无证书"，建议记日志 |
| `crates/ipma-x509-manager/src/generate.rs` PEM 解析失败 → 空列表 | 证书导入解析兜底 | 空列表后续有校验，风险低 |
| `src/crypto.rs` `result.err().unwrap_or_default()` | 取错误消息失败 → 空错误对象 | 影响错误信息完整性，非正确性 |
| `TopologyCore.js` `parseFloat(...) \|\| 200/100` | DOM 属性必存在，兜底不触发 | **不可**改 `??`（`NaN ?? x` 仍为 `NaN`），保持现状 |
| `SVGCore.js` `_setElementPosition` 各 `dataset.relX \|\| 0` | 相对坐标缺省 0 | ✅ 合理（0 为合法缺省） |

## 三、后端（Rust）合理用法分组（整改后复核）

### 3.1 查询参数缺省（✅ 缺省=不过滤/不排序）

| 位置 | 模式 | 评审 |
| --- | --- | --- |
| `src/auth/user.rs`、`src/log/{login,operation,notification}.rs`、`src/resource/network.rs`、`src/resource/net_outlet.rs` 等约 40 处 | `query.get("search").cloned().unwrap_or_default()` | ✅ 列表接口标准做法；`sort_by` 白名单在各自 handler 内校验 |
| `src/auth/login.rs`、`src/auth/ldap.rs` | `req.remember_me.unwrap_or(false)` | ✅ |
| `src/resource/device/lldp.rs` | subtype 映射缺省 `0/1` | ✅ 协议缺省 |

### 3.2 配置与密钥

| 位置 | 模式 | 评审 |
| --- | --- | --- |
| `src/db.rs` | `leak_detection_threshold` 读取失败 → `0.9` | ✅ 仅影响监控灵敏度 |
| `src/system/config.rs:39,81` 等 | `SystemTime..unwrap_or_default()` | ✅ 仅影响 uptime 显示 |
| `src/auth/sso.rs` `derive_redirect_uri` | scheme/redirect URI 按请求头推导或空 | ✅ |

### 3.3 SQL / 数据层

| 位置 | 模式 | 评审 |
| --- | --- | --- |
| `src/resource/network.rs` 等 | `COALESCE(json_agg(...), '[]')`、`UPDATE = COALESCE($n, 原值)` | ✅ 标准用法（PATCH 语义） |
| `src/resource/network.rs` | `ipv4_cidrs.clone().unwrap_or_default()` | ✅ 与 COALESCE 语义一致 |
| `src/system/certificate.rs` | SAN 缺省回退 `public_url` | ✅ |
| `crates/ipma-visualization/src/topology.rs` | 连线类型缺省 `"physical"` 等 | ✅ |
| `crates/ipma-data-management/src/logs.rs` 定时任务 `days.unwrap_or(30)` | 缺省 30 天保留 | ✅ 非危险默认（API 层已强制显式传值） |

### 3.4 认证与安全

| 位置 | 模式 | 评审 |
| --- | --- | --- |
| `src/auth/login.rs` 2FA/邮件验证码过期缺省、token exp 解析失败按当前时间、外部认证占位邮箱 `.invalid`、日志占位 `-` | 各类占位 | ✅ |
| `src/auth/password_policy.rs` | `bcrypt::verify(..).unwrap_or(false)` | ✅ 安全侧默认拒绝 |
| `src/auth/ldap.rs`、`sso.rs` 各 `unwrap_or(false)` | 外部认证异常拒绝 | ✅ |

### 3.5 日志 i18n 降级链（✅ 每级降级都有日志提示）

`src/log/mod.rs` 时间格式逐级降级、日志文件创建失败降级当前目录、i18n 非法回退英文、翻译缺失回显 key —— 均 ✅。

### 3.6 其余

`serde(default)` 14 处（与校验器配合）、布尔状态缺省 false —— ✅。

## 四、前端（JS）合理用法分组（整改后复核）

### 4.1 网络层（apiClient.js）

429 `Retry-After || 2` 重试、401 静默刷新、非 JSON 回退文本、网络异常分类、刷新失败 → `false` —— 均 ✅。

### 4.2 显示层占位

`formatter.js` 类型映射占位 `-`、`SVGRenderer.js` 无 IP/端口文案、设备名 `Unknown`、`i18n.js` 语言回退、`pagination.js` 分页缺省 —— 均 ✅。

### 4.3 可视化尺寸/会话存储

`waitForContainerHeight` 容器 <100 → 600（隐藏 tab 兜底）、`sessionManager` sessionStorage → localStorage 回退、`helpers.js` 错误缺省、`navigation.js` 折叠状态缺省展开、`visualizationManager.js` `{x: node.x || 0}`（仅回填显示）—— 均 ✅。

### 4.4 其余模块

`device.js`、`systemManager.js`、`room.js` 等约 70 处 `||` 为表单回填/显示占位/分页缺省 —— ✅。

## 五、结论

- 初审 12 项 ❌/⚠️ 已全部整改，方式见第一节；整改涉及 i18n 新增键：`server.logs.days_required`、`server.notification.recipients_parse_failed`、`server.device.interface.{physical_type,interface_role}_required`、`log.{ldap.password_decrypt_failed,sso.secret_decrypt_failed,system.recipients_parse_failed,import_export.export_ref_missing}`、`viz.data_load_failed`。
- 二次审计未发现新的危险兜底；残留 ⚠️ 项见第二节，均为低风险且已有明确处理建议。
