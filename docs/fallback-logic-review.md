# 前后端回退 / 保底逻辑梳理

> 范围：`src/`、`crates/`（后端 Rust）与 `web/static/js/`（前端 JS）。
> 方法：模式扫描（`unwrap_or*` / `COALESCE` / `||` / `??` / catch 降级 / 多分支响应解析）+ 逐处人工归类。
> 评审标注：✅ 合理 · ⚠️ 可疑（建议复核）· ❌ 不合理/不必要（建议修改或移除）。
> 文中行号为本文档撰写时（v0.15.65 之后）的快照位置。

## 一、结论摘要（不合理 / 不必要项）

| # | 位置 | 问题 | 建议 |
| --- | --- | --- | --- |
| 1 | `web/static/js/modules/visualization/SVGDataManager.js:39-55` `fetchIps` | 对同一 API 响应尝试 5 种结构（数组 / `items` / `data` / `ip_managers` / `ips`）。后端固定 `paged_response` 形状，后 3 种键根本不存在，属历史遗留的多分支兜底 | 只保留 `items` 一种解析，其余按错误处理 |
| 2 | `crates/ipma-data-manager/src/logs.rs:117` | 清理日志 `days` 缺省回退 `0`，而 `0` 语义是**删除全部日志**。前端有确认框，但 API 层"缺省=全删"是危险默认值 | `None` 时返回校验错误，强制显式传值 |
| 3 | `src/auth/ldap.rs:92`、`src/auth/sso.rs:149` | LDAP/SSO 服务账号密码**解密失败静默回退空串**，随后以空密码尝试绑定，故障被掩盖成"认证失败" | 解密失败应返回配置错误并记日志，而非空串降级 |
| 4 | `web/static/js/modules/visualization/TopologyRenderer.js:161-164`、`TopologyVisualization.js:165-168` | 节点坐标 `device.x \|\| 100`：合法坐标 `0` 会被 falsy 陷阱改成 `100`（y/width/height 同理）。拓扑未保存节点确实可能为 `0` | 改用 `??`，仅在 `null/undefined` 时兜底 |
| 5 | `web/static/js/modules/visualization/SVGRenderer.js:131` 与 `SVGDataManager.js:278,284` | 机柜容量默认值一处 `42`、一处 `45`，口径不一致；后端建表另有默认值 | 统一为一个常量并注明与后端默认一致 |
| 6 | 可视化各 DataManager 的 `catch → return []`（`TopologyDataManager.js`、`SVGDataManager.js` 等约 16 处） | 请求失败静默降级为空数据，画布/下拉直接显示"无数据"，用户无从区分"没数据"与"加载失败" | 失败时 `showToast` 提示（模块内已有该工具） |
| 7 | `src/system/config.rs:604` | 通知收件人配置 JSON 解析失败 `unwrap_or_default()` → 静默变成空收件人列表，MAC 变更通知会静默失效 | 解析失败记 `log_error` 并返回错误或空列表+告警 |
| 8 | `crates/ipma-data-manager/src/export.rs:277-462`（约 15 处） | 导出 CSV 时字段缺失 `unwrap_or_default()`/`unwrap_or("?")` 静默补空串，可能产出不完整行且无任何日志 | 至少对缺关键字段（设备名等）记录告警 |
| 9 | `src/auth/login.rs:1858` | dummy bcrypt 哈希失败 `unwrap_or_default()` 得空串，虽有 `hash.is_empty()` 守卫（直接 return），但用户名枚举时间侧信道缓解随之失效 | 哈希失败应 `panic`/启动时预生成并校验非空 |
| 10 | `src/auth/utils.rs:60-63` | JWT 过期时间解析失败静默回退 3600s/604800s，配置错误被掩盖 | 解析失败应在启动时报配置错误 |
| 11 | `web/static/js/modules/visualization/SVGDataManager.js` / `TopologyDataManager.js` 等 `result.data.items \|\| result.data \|\| []` | 后端分页键固定为 `items`（项目规范），`\|\| result.data` 分支不会命中，属冗余 | 保留 `items ?? []` 即可（低优先级） |
| 12 | `src/resource/device/interface.rs:123-125` | 创建接口时 `physical_type` 缺省静默补 `"rj45"`、角色补 `"business"`，用户输入缺省被业务默认值掩盖（项目规范要求输入显式验证） | 改为请求字段必填或至少在响应中回显生效值 |

其余扫描项（约 200+ 处后端 `unwrap_or*`、百余处前端 `||`）归类为合理用法，见下表分组。

## 二、后端（Rust）清单

### 2.1 查询参数缺省（✅ 合理：缺省=不过滤/不排序）

| 位置 | 模式 | 兜底行为 | 评审 |
| --- | --- | --- | --- |
| `src/auth/user.rs:30-33`、`src/log/{login,operation,notification}.rs`、`src/resource/network.rs:46-57`、`src/resource/net_outlet.rs:25-34` 等约 40 处 | `query.get("search").cloned().unwrap_or_default()` | `search/sort_by/sort_order/各类 filter` 缺省为空串=不过滤 | ✅ 列表接口标准做法；`sort_by` 白名单在各自 handler 内校验 |
| `src/auth/login.rs:341,520,800`、`src/auth/ldap.rs:332` | `req.remember_me.unwrap_or(false)` | 未勾选记住我 | ✅ |
| `src/resource/device/lldp.rs:166,245,261` | subtype 映射缺省 `0/1` | LLDP 协议未知枚举值 | ✅ 协议缺省 |

### 2.2 配置与密钥

| 位置 | 模式 | 兜底行为 | 评审 |
| --- | --- | --- | --- |
| `src/auth/ldap.rs:92`、`src/auth/sso.rs:149` | `decrypt_password_async(..).unwrap_or_default()` | 解密失败 → 空密码 | ❌ 见摘要 #3 |
| `src/auth/utils.rs:60-63` | `parse_duration(..).unwrap_or(3600/604800)` | JWT 时长解析失败 → 默认时长 | ⚠️/❌ 见摘要 #10 |
| `src/db.rs:335` | `leak_detection_threshold` 读取失败 → `0.9` | 连接池告警阈值兜底 | ✅ 仅影响监控灵敏度 |
| `src/system/config.rs:39,81,604,866` | DB 读配置失败/JSON 解析失败 → 默认值 | 配置缺失回退默认 | ⚠️ `:604` 见摘要 #7 |
| `src/auth/sso.rs:213,222` | scheme/redirect URI 推导失败 → 按请求头推导或空 | SSO 回调地址兜底 | ✅ |
| `crates/ipma-x509-manager/src/ca.rs:380,482` | `try_exists().await.unwrap_or(false)` | stat 失败视为文件不存在 | ⚠️ 权限异常时会误报"无证书"，建议记日志 |
| `crates/ipma-x509-manager/src/generate.rs:446` | PEM 解析失败 → 空列表 | 证书导入解析兜底 | ⚠️ 空列表后续应报错，确认调用点有校验 |

### 2.3 SQL / 数据层

| 位置 | 模式 | 兜底行为 | 评审 |
| --- | --- | --- | --- |
| `src/resource/network.rs:292,293,523,524,…` | `COALESCE(json_agg(...), '[]')` | 聚合无行时输出空数组 | ✅ 标准用法 |
| `src/resource/network.rs:684-691,1002-1003` | `UPDATE ... = COALESCE($n, 原值)` | 未传字段保持原值（PATCH 语义） | ✅ |
| `src/resource/network.rs:27` | `db_err.constraint().unwrap_or_default()` | 约束名缺失时空串匹配 | ✅ 仅影响错误分类 |
| `src/resource/network.rs:363,374,611,645` | `ipv4_cidrs.clone().unwrap_or_default()` | 数组字段 NULL → 空数组 | ✅ 与 COALESCE 语义一致 |
| `src/system/certificate.rs:56-59` | SAN 缺省回退 `public_url` | 证书域名兜底 | ✅ |
| `crates/ipma-visualization/src/topology.rs:545,583-584` | 连线类型缺省 `"physical"`、成员端口缺省空数组 | 拓扑存储缺省 | ✅ |
| `crates/ipma-data-manager/src/export.rs`（约 15 处） | `row.get(..).unwrap_or_default()` / `unwrap_or("?")` | 导出字段缺失补空串/"?" | ❌ 见摘要 #8 |
| `crates/ipma-data-manager/src/logs.rs:117` | `req.days.unwrap_or(0)` | 缺省 0=全删 | ❌ 见摘要 #2 |

### 2.4 认证与安全

| 位置 | 模式 | 兜底行为 | 评审 |
| --- | --- | --- | --- |
| `src/auth/login.rs:1858` | dummy bcrypt 哈希失败 → 空串 | 空 `hash` 时跳过校验 | ⚠️ 见摘要 #9 |
| `src/auth/login.rs:885,904` | 过期时间缺省 → now+1h / now+7d | 2FA/邮件验证码有效期兜底 | ✅ |
| `src/auth/login.rs:1006` | `DateTime::from_timestamp(..).unwrap_or_else(Utc::now)` | token exp 解析失败按当前时间 | ✅ 结果是 token 判过期 |
| `src/auth/login.rs:1650` | 邮箱缺省 `{user}@{provider}.invalid` | 外部认证无邮箱时占位 | ✅ 明确的 `.invalid` 保留域 |
| `src/auth/login.rs:1899` | `error_message.unwrap_or("-")` | 日志显示占位 | ✅ |
| `src/auth/password_policy.rs:191` | `bcrypt::verify(..).unwrap_or(false)` | 校验异常视为不匹配 | ✅ 安全侧默认拒绝 |
| `src/auth/ldap.rs:189,242,482`、`sso.rs` | 各类 `unwrap_or(false)` | 外部认证异常拒绝 | ✅ |
| `src/crypto.rs:309,321,342,461` | `result.err().unwrap_or_default()` | 取错误消息失败 → 空错误对象 | ⚠️ 影响错误信息完整性，非正确性 |

### 2.5 日志 i18n 降级链（✅ 合理：每级降级都有日志提示）

| 位置 | 兜底行为 | 评审 |
| --- | --- | --- |
| `src/log/mod.rs:33-64` | 时间格式候选逐级降级 | ✅ |
| `src/log/mod.rs:137-154` | 日志文件创建失败降级到当前目录，再失败仅控制台输出 | ✅ 降级路径均有记录 |
| `src/config.rs:173`、`src/log/mod.rs:9` | i18n 配置非法时整体回退英文日志 | ✅ |
| `crates/ipma-common/src/log_i18n.rs:20-21` | 未注册翻译钩子时回显 key + 参数 | ✅ 便于排查缺失翻译 |
| `src/i18n/`（前端同款约定） | 翻译缺失返回 key 本身 | ✅ |

### 2.6 系统信息 / 时间

| 位置 | 模式 | 兜底行为 | 评审 |
| --- | --- | --- | --- |
| `src/system/config.rs:38-44,80-84` | `SystemTime..unwrap_or_default()` | 系统时间异常按 epoch | ✅ 仅影响 uptime 显示 |
| `crates/ipma-x509-manager/src/ca.rs:300,434,456` | 布尔状态缺省 false | 证书状态兜底 | ✅ |
| `src/resource/device/interface.rs:123-125` | `physical_type.unwrap_or("rj45")` 等 | 接口缺省值 | ⚠️ 见摘要 #12 |
| `serde(default)` 共 14 处（`src/models/*`、config 结构体） | 反序列化缺省 | 字段默认值 | ✅ 与校验器配合使用 |

## 三、前端（JS）清单

### 3.1 API 响应多分支兜底

| 位置 | 兜底行为 | 评审 |
| --- | --- | --- |
| `SVGDataManager.js:39-55` `fetchIps` | 5 种结构依次尝试 | ❌ 见摘要 #1 |
| `SVGDataManager.js:28-32`（工位）、`77-82`（机位）、`355-360`（批量 IP） | 数组 / `items` 两分支 + catch → `[]` | ⚠️ 后端形状固定，`Array.isArray` 分支多余；catch 静默见摘要 #6 |
| `TopologyDataManager.js:17,70,117,130,143,156` | `Array.isArray ? data : []`、`items \|\| data \|\| []` | ⚠️ 同上（#6/#11） |
| `dashboard.js:106-110` | 数组 / `data.data` 两分支 | ⚠️ 冗余但无害 |
| `ipmanager.js:48,89-91` | 数组 / `items` / `data.data` 三分支 | ⚠️ 冗余 |
| `room.js:45,410`、`organization.js:543`、`unifiedDevicePorts.js:186`、`resources.js:13,270`、`networkCardManager.js:105,120` | `success && Array.isArray ? data : []`、缓存 `get(key) \|\| []` | ✅ 形状统一、失败显式走空 |
| `position.js:108,138` | `result.data.items \|\| result.data` | ⚠️ #11 |
| `visualizationManager.js:300` | `result.data.items \|\| result.data \|\| []` | ⚠️ #11 |

### 3.2 catch 静默降级

| 位置 | 兜底行为 | 评审 |
| --- | --- | --- |
| 可视化各 DataManager（`TopologyDataManager.js`、`SVGDataManager.js` 共约 16 处） | `console.error` + `return []` | ❌ 建议加 `showToast`（#6） |
| `utils/modalLoader.js:101` | 模态框加载失败 → `null`（调用方判空） | ✅ |
| `utils/ui.js:306`、`utils/apiClient.js:369` | 失败 → `false`，错误以结果对象/日志表达 | ✅ |
| `modules/networks.js:298`、`devicePorts.js:592`、`dashboard.js:497` | 失败 → `[]`/`null`，多数有 toast | ✅ |

### 3.3 网络层（apiClient.js）

| 位置 | 兜底行为 | 评审 |
| --- | --- | --- |
| `apiClient.js:199-205` | 429 时 `Retry-After \|\| 2` 秒后重试（≤3 次） | ✅ |
| `apiClient.js:186-198` | 401 → 静默刷新 token 重试 → 仍失败跳登录 | ✅ |
| `apiClient.js:260-271` | 非 JSON 响应回退文本 + `api.parse_failed` | ✅ |
| `apiClient.js:272-295` | 网络异常按名称分类（Abort/timeout/Network） | ✅ |
| `apiClient.js:344-376` `refreshToken` | 刷新失败 → `false` | ✅ |

### 3.4 显示层占位

| 位置 | 兜底行为 | 评审 |
| --- | --- | --- |
| `utils/formatter.js:15,24,36` | `map[type] \|\| type \|\| "-"` | ✅ |
| `SVGRenderer.js:26-31` | IP/端口缺失 → `viz.no_ip`/`no_port` | ✅ |
| `TopologyRenderer.js:90` | 设备名缺失 → `Unknown` | ✅ |
| `utils/i18n.js:9` | `navigator.language \|\| "en"` | ✅ |
| `utils/i18n.js:145,204-207` | 语言名缺失 → 固定表 → code | ✅ |
| `utils/pagination.js:49,70` | `pageSize \|\| 首档`、`total \|\| 0` | ✅ |

### 3.5 可视化坐标 / 尺寸默认值

| 位置 | 兜底行为 | 评审 |
| --- | --- | --- |
| `TopologyRenderer.js:161-164`、`TopologyVisualization.js:165-168,388-391` | `x \|\| 100` 等 | ❌ falsy 陷阱（#4） |
| `TopologyCore.js:532-533` | `parseFloat(...) \|\| 200/100` | ⚠️ 同上（DOM 属性必存在，兜底不触发） |
| `SVGRenderer.js:14-17`（工位 `\|\| 100/\|\| 240/\|\| 120`）、`113-117`（机柜 `\|\| 50/\|\| 45U`） | 位置/尺寸缺省 | ⚠️ 同 falsy 模式；工位默认宽 240 与实际绘制 160 不一致，历史遗留 |
| `SVGDataManager.js:278,284` | `capacity \|\| 45` | ⚠️ 与 `SVGRenderer.js:131` 的 42 不一致（#5） |
| `SVGDataManager.js:258-265` `waitForContainerHeight` | 容器高度 <100 → 600 | ✅ 隐藏 tab 刚切换的兜底 |
| `SVGCore.js` `_setElementPosition` 各 `dataset.relX \|\| 0` | 相对坐标缺省 0 | ✅ |
| `visualizationManager.js:496` | 节点坐标 `{x: node.x \|\| 0}` | ✅（0 合法保留不了但此处仅回填显示） |

### 3.6 会话 / 本地存储

| 位置 | 兜底行为 | 评审 |
| --- | --- | --- |
| `utils/sessionManager.js:20-43` | `JSON.parse` 失败 → `null`；rememberMe 未勾选回退 sessionStorage → 再回退 localStorage | ✅ |
| `utils/helpers.js:181,202-204,255` | 错误类型缺省 `api`、消息缺省 `common.unknown_error`、`defaultValue ?? null` | ✅ |
| `modules/navigation.js`（localStorage 折叠状态） | 读取缺省展开 | ✅ |

### 3.7 其余模块（`||` 默认值约 70 处/最大单文件 device.js）

`device.js`、`systemManager.js`、`room.js`、`networks.js`、`devicePorts.js`、`organization.js`、`cableLink.js`、`cabinet.js`、`fail2banManager.js`、`scheduledTaskManager.js` 中的 `||` 绝大多数为：表单回填缺省空串、显示占位（`-`）、分页参数缺省、`document.getElementById` 后的可空链处理 —— 均为 ✅ 显示层/输入层占位，未发现改变控制流的危险兜底。

## 四、建议的处理顺序

1. **先修危险项**：摘要 #2（日志全删）、#3（LDAP/SSO 空密码）、#7（通知收件人静默清空）。
2. **再修正确性**：#4（坐标 falsy 陷阱，随本版本一并改为 `??` 前需逐点确认 0 值语义）、#5（容量默认统一）。
3. **体验优化**：#6（可视化加载失败提示）。
4. **低风险清理**：#1、#8、#9、#10、#11、#12 可随后续迭代处理。
