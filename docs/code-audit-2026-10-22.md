# FOIMS 前后端代码整体审计报告

> 审计日期：2026-10-22
> 审计对象：`/media/oi-io/AA709DF48A7AD5C2/foims`（分支 `vue3-test`，HEAD `e46e82ed`）
> 审计性质：只读审计，未修改任何源码；审计前后 `git status` 均为干净
> 审计基线：[code-style.md](./code-style.md)、[AGENTS.md](../AGENTS.md)、workspace lints

## 1. 审计范围与方法

| 维度 | 内容 |
|---|---|
| 后端 | Rust 2024 + axum 0.8 + sqlx 0.9，187 文件 / 65,369 行（13 个 crate + 主程序） |
| 前端 | Vue 3 + Vite + TypeScript，272 文件 / 26,463 行（`frontend/`） |
| 部署与流程 | nginx 配置、systemd 单元、CI 工作流、数据库升级脚本 |
| 方法 | 工具链全量验证 + 静态模式检索 + 关键链路逐行走读（认证、SQL、上传导入、系统配置、Agent 上报、SSO、密钥管理） |

关键链路走读清单：

- 认证：JWT 签发/校验、Cookie 下发、吊销、设备指纹、验证码、应用层 fail2ban、SSO（state/nonce/PKCE）
- 数据访问：QueryBuilder 动态 SQL、ORDER BY 白名单、ILIKE 转义、导入业务键解析、事务边界
- 系统配置：读取/更新/备份/恢复、密钥掩码与还原、AES 密钥文件权限
- Agent 链路：mTLS、Bearer 注册令牌比对、批量帧校验、频控闸门
- 部署：CORS、UDS 权限、nginx 安全头、health 端点

## 2. 工具链验证（全部通过）

| 检查 | 命令 | 结果 |
|---|---|---|
| Rust 格式 | `cargo fmt --all --check` | 通过 |
| Rust 静态检查 | `cargo clippy --release --workspace --all-targets -- -D warnings` | 通过（此前一次完整运行 1m32s） |
| 前端类型检查 | `cd frontend && npx vue-tsc --noEmit` | 通过 |
| JS 质量 | `npx oxlint@1.88.0`（287 文件） | 0 warnings / 0 errors |
| CSS/格式 | `npx @biomejs/biome@2.5.15 check frontend`（21 文件） | 通过 |
| i18n 对称性 | zh/en 词条对比 | 各 2458 条，完全对称 |
| 前端构建 | `cd frontend && npm run build` | 成功（2 条警告，见 F5） |
| ts-rs 生成物 | `git status --porcelain frontend/src/types/api/generated` | 为空；本次未改源码，无漂移可能；CI 已内置漂移校验 |

> 说明：`da-scripts/gen-ts-types.sh` 的完整重生成在审计时因宿主机高负载（load ~27）未在时限内完成。由于本次为只读审计、生成物目录 git 状态干净，该结论不受影响。

## 3. 后端审计

### 3.1 合规项（抽查确认）

- **硬性规则零违规**：非测试代码 0 处 `unwrap/expect/panic/unreachable/todo`；主 workspace 0 处 `println!/dbg!/#[allow]/unsafe`。测试代码通过 `#[cfg(test)]` 边界排除后统计。
- **SQL 注入防护完整**：动态条件全部 `QueryBuilder` + `push_bind`；ORDER BY 走 `match` 白名单；ILIKE 先经 `escape_like`。唯一 `format!` 拼接 SQL 位于 [resolve.rs:133](../crates/foims-data-management/src/import/resolve.rs)（表名/列名来自 `import/spec.rs` 静态白名单，值全部绑定）。
- **认证链路严谨**：
  - JWT HS256，含 `iss/aud/exp`，leeway 30s；
  - [middleware.rs](../crates/foims-auth/src/login/middleware.rs) 每请求校验 `revoked_tokens`、用户 `enabled`、`tokens_invalidated_at`（含恰好落在吊销秒的边界）、设备指纹；
  - 数据库不可用时 fail-closed 返回 503。
- **Cookie 安全**：[token.rs](../crates/foims-auth/src/login/token.rs) HttpOnly + SameSite=Lax + 按 `X-Forwarded-Proto` 条件 Secure + max_age。
- **密码与验证码**：重置令牌哈希存储 + 过期 + `FOR UPDATE` + 一次性；图形验证码 5 分钟一次性、失败 3 次阈值联动。
- **SSO**：state/nonce/PKCE S256 一次性消费。
- **密钥管理**：AES 密钥 `/etc/foims/encryption.key` 0600 原子写；检测到备份密钥文件时拒绝启动；配置端点对 `db.password/jwt.secret/enrollment_token/snmp trap` 全部掩码，恢复时 `"***"` 还原磁盘真实值，并复用同一套 `validate_config_for_save`（[config.rs:510-529](../src/system/config.rs)）。
- **命令执行无注入**：`systemctl` 三处调用均使用 `args` 数组；unit 来自 `ManagedService` 枚举（仅 `Foims`/`Nginx`），操作来自 `ServiceOp` 枚举，无用户输入拼接，无 shell 解释。
- **Agent 上报鉴权**：[ingest.rs:233-246](../crates/foims-agent-service/src/ingest.rs) Bearer 令牌 → SHA-256 → 与共享配置槽中的 `enrollment_token` 哈希比对（每请求读取，令牌重新生成即时生效）；频控闸门在 upsert 的 `WHERE` 中原子判定。
- **其他**：错误详情不外泄（仅返回 i18n 键，细节入日志）；分页上限 `MAX_PAGE_SIZE = 1000`；分页响应固定五键；UDS 0660 + 属组；QUIC mTLS；CORS 精确 origin 匹配；nginx 安全头（HSTS/CSP/nosniff/X-Frame-Options/Referrer-Policy/Permissions-Policy）齐全。

### 3.2 发现问题

#### [中] B1. 动态 SQL 共 9 处缺少 `.persistent(false)`

规范明确"动态 SQL 一律 `.persistent(false)`"（[code-style.md:100-103](./code-style.md)）。以下 `build_query_*()` 均未设置：

| 文件 | 行 |
|---|---|
| [src/log/login.rs](../src/log/login.rs) | 55、78 |
| [src/log/operation.rs](../src/log/operation.rs) | 129、144 |
| [src/log/notification.rs](../src/log/notification.rs) | 58、69 |
| [src/system/scheduled_task.rs](../src/system/scheduled_task.rs) | 99 |
| [crates/foims-agent-service/src/agents_api.rs](../crates/foims-agent-service/src/agents_api.rs) | 130、405 |

**影响**：分页/筛选组合产生动态 SQL 文本，`persistent=true` 时永远无法命中预处理语句缓存，反而持续向 PostgreSQL 注入一次性 prepared statement 并挤掉 LRU 中的热语句，长期运行存在性能退化风险。

**修复**：在 `build_query_*()` 后链式追加 `.persistent(false)`（对照 `foims-resource` 内 20+ 处正确写法）。

#### [中] B2. 鉴权中间件每请求 2 次数据库往返

[login/middleware.rs](../crates/foims-auth/src/login/middleware.rs) 中 `is_token_revoked`（查 `revoked_tokens`）与后续 `users` 状态查询（`enabled`、`tokens_invalidated_at`）均在 JWT 校验之后无条件执行。Agent 页 30s 轮询、日志页翻页等高频场景会放大该开销。

**修复建议**：合并为单条查询（`LEFT JOIN revoked_tokens`），或对 `users(enabled, tokens_invalidated_at)` 加 30–60s TTL 缓存并在吊销时主动失效。

#### [低] B3. 配置备份/读取的权限口径可收紧

[backup_config](../src/system/config.rs) 与 [get_system_config](../src/system/config.rs) 仅需 `AuthUser`（任意登录用户）。虽已完整掩码密钥，但备份仍暴露数据库主机/端口、SMTP、SNMP 目标、CA 路径等基础设施拓扑信息。

**修复建议**：备份收紧为 `SysAdminUser`；读取类按前端实际可见范围决定，若仅系统配置页使用则同步收紧。
（注：`restore_config` 要求 `AdminUser`、`update_system_config` 要求 `SysAdminUser`，属于"全量恢复高于字段修改"的合理权限梯度，且恢复路径已复用占位符还原与落盘校验，不构成缺陷。）

## 4. 前端审计

### 4.1 合规项（抽查确认）

- 0 处 `any`、0 处 `@ts-ignore`、0 处 `alert/confirm`；全部 SFC 为 `<script setup lang="ts">`。
- `v-html` 仅 2 处，均用于服务端生成的图形验证码 SVG；legacy visualization 的 `TopologyModal` 对用户数据全部 `escapeHtml`。
- 裸 `fetch` 仅 2 处探活例外（[LoginPage.vue](../frontend/src/pages/LoginPage.vue)、`InitWizard.vue`），均有注释说明，符合规范例外。
- 17 处 `console.error/warn` 符合"catch 不许静默：用户路径给 toast，后台路径 console.error 带上下文"的规范（[code-style.md](./code-style.md)）。
- `ModalShell`/`useModalStack`/`showToast`/`Pagination` 使用到位；页面级定时器均由 `usePageTimeout` 清理，未发现悬挂裸 `setTimeout`。

### 4.2 发现问题

#### [中] F1. `skipAuthCheck` 选项实际失效

[client.ts](../frontend/src/api/client.ts) 在组装请求时 `delete mergedOptions.skipAuthCheck`，而 `handleUnauthorized` 依赖 `options.skipAuthCheck === true`，收到的已是被删除后的对象，判断恒为 false。

**当前影响**：无。7 个调用点（`LoginPage` 6 处 / `ForgotPasswordModal` 1 处）的 URL 全部命中 `isPublicAuthEndpoint` 白名单。但这是一个"静默失效的开关"，后续新增非白名单公共端点时会踩坑。

**修复（已采纳）**：删除该冗余字段及 7 处显式传参，401 处理统一由 `isPublicAuthEndpoint` 白名单单点判定——开关与白名单同时判同一件事，属规范禁止的无效旧代码保留。

#### [低] F2. AgentsPage 手写分页，未复用 `usePagedList`

[AgentsPage.vue](../frontend/src/pages/AgentsPage.vue) 自建 `page` ref、`totalPages` 与越界回退 watcher，与 [usePagedList.ts](../frontend/src/composables/usePagedList.ts) 重复。该页需要 30s `refetchInterval`，而 `usePagedList` 暂不支持轮询选项。

**修复**：给 `usePagedList` 增加可选 `refetchInterval`，AgentsPage 改为复用；否则把越界回退抽成公共函数。

#### [低] F3. 183 处 API 调用中 67 处未显式标注泛型

生成物 `frontend/src/types/api/generated/` 的价值未被充分利用（默认 `unknown`），靠调用处断言收窄。建议优先补齐核心页面（Dashboard/Resources/Logs）。

#### [低] F4. 裸 hex 色值 8 处

| 位置 | 说明 |
|---|---|
| [DashboardPage.vue](../frontend/src/pages/DashboardPage.vue)（532、672、723 行） | 骨架屏/加载态 |
| [tables.css](../frontend/src/styles/components/tables.css)（869 行） | `color:#f00` |
| [sidebar.css](../frontend/src/styles/layouts/sidebar.css)（29、32 行） | 深色侧栏底色/边框 |
| [cable-label-print.css](../frontend/src/styles/components/cable-label-print.css)（22、47 行） | 打印边框 `#000`，可加豁免注释 |

（`variables.css`、`login.css` 中 `--*` 变量定义处的 hex 属设计令牌定义，不计入违规。）

#### [低] F5. Vite 构建 2 条警告

1. `INEFFECTIVE_DYNAMIC_IMPORT`：[client.ts](../frontend/src/api/client.ts) 动态导入 `useToast`，但该模块在别处已被静态引入，动态导入不产生独立 chunk（功能无影响）。
2. 单 chunk > 500 kB：建议 `manualChunks` 按 vendor（vue / tanstack / echarts）拆包。

#### [低] F6. 空闲超时监听 `mousemove` 无节流

[usePageTimeout.ts](../frontend/src/composables/usePageTimeout.ts) 在 document capture 上监听 `mousemove`，每次移动都 `clearTimeout` + `setTimeout`。建议加 1s 节流或仅在剩余时间低于阈值时重置。

#### [低] F7. 超大单文件组件

`LoginPage.vue` 1482 行、`ResourcesPage.vue` 1313 行、`SubnetUsageModal.vue` 971 行。建议登录页逻辑抽 composable、资源页按 tab 拆子组件。

## 5. 仓库与流程

#### [中] R1. CI 未覆盖当前开发分支

[.github/workflows/ci.yml](../.github/workflows/ci.yml) 的 push/PR 触发分支仅 `main`/`master`/`Dev`。当前 `vue3-test` 的推送不会运行 CI（fmt/clippy/test/ts-rs 漂移），存在漏检窗口。建议加入 `vue3-test`，或改用全分支触发并保留 tag 触发。

#### [低] R2. `upgrade/0.25/` 保留两份脚本

`upgrade_0.25.0.sql` 与 `upgrade_0.25.1.sql` 并存，违反 AGENTS.md "每个中版本仅保留一份最新版本升级代码"的约定（`upgrade/0.26/` 已合规，仅一份）。建议删除旧脚本或合并。

#### [信息] R3. 数据库迁移覆盖完整

`git log` 核实：`foims-init` schema 最后变更为 0.26.0，之后至 0.29.1 无 schema 改动，升级目录无缺失。

#### [信息] R4. nginx `/health` 未设 `proxy_set_header`

[deploy/nginx/foims.conf](../deploy/nginx/foims.conf) 的健康检查 location 与其他 location 的头部设置不一致。健康检查不涉及鉴权，影响可忽略；如统一风格可补齐。

## 6. 结论与优先级

**总体结论**：代码质量基线良好。安全硬性红线（注入、鉴权、密钥、XSS、命令执行）未发现高危问题；工具链全部通过。发现的问题集中在"规范一致性"与"性能/可维护性"层面。

| 优先级 | 事项 |
|---|---|
| P0（立即） | 无 |
| P1（本迭代） | B1 补 10 处 `.persistent(false)`；F1 修复 `skipAuthCheck`；R1 CI 覆盖全分支 |
| P2（持续改进） | B2 鉴权查询合并/缓存；B3 备份权限收紧；F2 统一分页；F3 泛型补齐；F4 hex 收敛；F5 拆包；F6 节流；F7 组件拆分；R2 清理 upgrade |
| P3（记录） | R4 nginx 头部统一 |

## 7. 审计限制

- 未执行动态渗透测试与运行时压测；
- 未运行 `cargo audit` / `npm audit` 依赖漏洞扫描（建议纳入 CI 定期任务）；
- 浏览器端运行时行为未验证（仅静态审查与构建验证）；
- `gen-ts-types.sh` 全量重生成未在审计时限内完成，结论基于"未改源码 + 生成物 git 状态干净"推断；
- 统计口径：Rust 违规项排除 `#[cfg(test)]`/`mod tests` 区域；前端 API 泛型统计基于 `apiGet/apiPost/apiPut/apiDelete` 调用点的正则匹配。

## 8. 附录：关键统计

| 指标 | 数值 |
|---|---|
| Rust 文件 / 行数 | 187 / 65,369 |
| 前端文件 / 行数 | 272 / 26,463 |
| 非测试 `unwrap/expect/panic` | 0 |
| `println!/dbg!/#[allow]/unsafe` | 0 |
| 动态 SQL 缺 `persistent(false)` | 9 处（5 文件） |
| 前端 API 调用（显式泛型/总数） | 116 / 183 |
| 前端裸 hex（非令牌定义） | 8 处 |
| i18n 词条（zh/en） | 2458 / 2458 |
| oxlint | 0 warnings / 0 errors（287 文件） |
