# FOIMS 代码风格规范

本文档是全项目代码风格的唯一权威来源，AGENTS.md 指向此处。
目标：多人长期迭代下风格不漂移。写代码前先读对应章节，改代码时遵守既有模式。
重要须知，和你合作的提问者一点代码基础都没有。

---

## 1. 通用

- 编码 UTF-8，换行 LF，文件末尾保留一个空行。
- 注释统一中文，解释“为什么”而非复述代码；公共 API 必须有文档注释。
- 禁止 `unwrap()`/`expect()`/`unreachable!()`/`#[allow(...)]`（测试代码除外）；
  禁止用 `_` 前缀忽略参数。**唯一例外**：仅需鉴权副作用的提取器参数
  `_admin: AdminUser` / `_user: CurrentUser`（axum 惯用法，见 routes/mod.rs）。
- 版本号规则统一见 `docs/dev_version.md`（x/y/z 判级分别与
  frontend/package.json、Cargo.toml 联动），前端与后端同一套机制，
  两个版本号独立计数。
- 每次改动后：`cargo fmt && cargo clippy --release -- -D warnings`；数据库结构
  变更走“直接执行 SQL + 同步完善 foims-init 的 DDL”，不用迁移框架。
- 新增功能（rust、js）、新增前端布局、新增样式代码 保障代码健壮性的前提下遵循最少代码实现原则
## 2. Rust（2024 Edition）

### 2.1 工程结构

```
foims（bin/lib）             应用组装层：路由装配/系统管理/日志/可视化包装/app_state
├── crates/foims-resource        资源管理（子网/房间/机柜/工位/设备/IP/链路…）
├── crates/foims-organization    组织管理（组织树/员工/模板）
├── crates/foims-auth            认证与用户管理（登录/JWT/fail2ban/SMTP/操作日志）
├── crates/foims-models          领域模型（请求/响应/行模型，唯一副本）
├── crates/foims-common          共享基础设施（响应/错误/配置/加密/连接池/限流/网络工具）
├── crates/foims-init            建库建表/校验/备份恢复
├── crates/foims-visualization   拓扑与布局计算
├── crates/foims-data-management CSV 导入导出
├── crates/foims-x509-management 证书管理
├── crates/foims-services        systemd 服务管理（foims/nginx）
├── crates/foims-scheduler       定时任务
├── crates/foims-agent-service   Agent 服务端（zip-deb-rpm 动态组包/下载 API/上报接收）
└── （原 crates/foims-agent 已拆出至主仓根下独立工作空间 foims-agent/
    （独立 Cargo workspace，不进 members）：
    foims-agent-core/foims-protocol/foims-agent/foims-agent-windows 四 crate）
```

- 依赖方向自上而下，禁止反向依赖与环。
- 跨 crate 共享的类型与工具放 `foims-common` / `foims-models`，
  **不得在多个 crate 各存一份副本**。
- 业务 crate 不依赖主程序：状态访问经依赖倒置——handler 面向
  `foims_common::DbProvider`（连接池）与 `foims_auth::provider::AuthProvider`
  （认证扩展）泛型编写，由主程序 `AppState` 实现；主程序路由注册处
  以 turbofish（`handler::<AppState>`）单态化。
- 依赖版本统一由根 `Cargo.toml` 的 `[workspace.dependencies]` 管理，子 crate
  一律 `workspace = true`。
- `[workspace.lints]` 已启用 clippy 护栏（unwrap/expect/print/dbg/todo/
  unreachable/allow_attributes）并在 rust lints 层 `unsafe_code = "forbid"`，
  CI 以 `-D warnings` 强制，不要新增违例。

### 2.2 错误处理

- 统一 thiserror 枚举 + `AppError` 风格；禁止 anyhow 混用。
- `From<sqlx::Error>` 一律委托 `foims_common::classify_db_error`：

```rust
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match foims_common::classify_db_error(&err) {
            DbErrorKind::Conflict(msg) => AppError::Conflict(msg),
            DbErrorKind::Validation(msg) => AppError::Validation(msg),
            DbErrorKind::NotFound => AppError::NotFound("资源不存在".to_string()),
            DbErrorKind::Database(msg) => AppError::Database(msg),
        }
    }
}
```

- 日志统一 `tracing`（`error!`/`warn!`/`info!`/`debug!`），禁止 `println!`。
- 面向用户的响应 message 一律为 i18n 键 + `message_params` 插值
  （`foims_common::msg` 或键字面量，词条在 `frontend/src/i18n/locales/` 双语维护）；
  可枚举的固定情形禁止直传中文文案（范本：`ldap.rs` 的
  `"server.ldap.config_retrieved"`、`error.rs` 的 `msg("server.error.*")`）。

### 2.3 数据库访问

- **动态 SQL 一律用 `sqlx::QueryBuilder`**，禁止手写 `$N` 占位符记账：

```rust
let mut builder = QueryBuilder::<Postgres>::new("SELECT COUNT(*) FROM workstations w");
push_filters(&mut builder, ...);   // 条件拼接抽成 fn，供 COUNT/数据两查共用
let total: i64 = builder
    .build_query_scalar()
    .persistent(false) // 动态文本无法复用预处理语句，禁缓存防 LRU 污染
    .fetch_one(&pool)
    .await?;
```

- 拼接动态 SQL 字符串（`format!` 结果）传给 `sqlx::query*` 时用
  `sqlx::AssertSqlSafe(...)` 显式声明已审计；用户输入永远走 `push_bind`。
  **审计标准**：拼接内容只能来自代码内固定字面量、模块常量或 match 白名单
  （排序字段/列清单/表名），不得含任何用户可控插值；使用处必须在同函数
  注释写明拼接来源（范本：`cable_link.rs` 列清单注释、`export.rs`
  白名单注释）。
- **动态 SQL 一律 `.persistent(false)`**：QueryBuilder 的 `build_query_*`
  与 `AssertSqlSafe` 拼接的动态文本（筛选/排序组合导致 SQL 文本变化）必须
  禁用语句缓存——动态文本永远无法命中预处理语句缓存，persistent=true 只会
  污染 LRU 挤掉热语句；文本恒定的查询（建表 DDL、固定清单循环）保持默认。
- ILIKE 模式先经 `foims_common::net::escape_like` 转义，排序字段走 match 白名单。
- 行映射统一 `query_as::<T>` + `#[derive(sqlx::FromRow)]`；不由 SQL 携带的
  字段用 `#[sqlx(skip)]` 后在代码中填充。
- 事务约定：**多步写操作（存在性检查 + 写入 + 回读）必须包在同一事务**；
  单条 UPDATE/DELETE 可直接执行并以 `rows_affected()==0` 判 NotFound。

#### 数据库命名规范（强制）

- **表名用语义实体命名**，不绑定实现载体：表名应描述其存储的领域实体
  （如 `monitored_hosts`、`snmp_arp_entries`、`interface_ips`），禁止用
  过窄的实现载体命名（如 `agents`、`ips` 这类同名异义或缩写歧义词）。
  索引/约束/视图/触发器名随表名联动。
- **布尔列用形容词命名**（`enabled`、`success`、`active`、`read`），
  **禁止用 `status`/`result` 等通用名词承载布尔语义**——同名词在别处
  常为枚举（`monitored_hosts.status` 是 active/offline/disabled 枚举），
  同名异义会污染前端字段消费与排序参数。多态结果（如 serde `Result`
  的 `ok`/`err` 形状）不受此限，但必须注释说明。
- **命名歧义必须全链路统一**：一旦发现 DB 列名、Rust 字段、前端 JSON
  消费、i18n 键、文档术语之间语义漂移（如同义双列、别名冗余），统一
  术语后彻底清理旧名，禁止保留兼容别名层；破坏性 schema 变更在开发
  阶段允许，按 AGENTS.md 迁移规则出 upgrade SQL。
- **禁止读取路径的存量键名兼容（强制）**：JSON/DB 存储的键名或枚举值改名时，
  存量数据迁移一律由 upgrade SQL 负责，读取/展示/反序列化路径只认唯一规范名。
  **禁止 serde `alias`**（及任何“多键名回退读取”“前端保留旧枚举值/旧 i18n 键供
  历史行展示”等读取侧兼容写法）——那是与 upgrade SQL 重复的第二套兼容机制，
  会掩盖迁移是否执行、制造新旧双口径漂移。`#[serde(rename)]`（仅定义规范序列化名）
  与 `#[serde(default)]`（仅处理缺键，如全新部署未落值）不受此限。改名随附的
  upgrade SQL 必须同时迁移所有承载旧值的列（含嵌于 TEXT/JSONB 内的 i18n key），
  迁移后删除全部读取侧兼容代码与其对应单测。

### 2.4 API 响应

- 响应体统一 `foims_common::ApiResponse { success, message, data }`，
  成功响应用 `foims_common::ok_json(data, "common.xxx")`（message 为 i18n 键）。
- 分页列表响应统一（`foims_common::pagination`）：

```rust
Ok(ok_json(paged_response(items, total, &pagination), "common.fetch_success"))
```

`message` 必须为 i18n 键（前端查词条翻译展示），禁止直传中文文案。
响应键固定为 `items/total/page/page_size/total_pages`；`total_pages` 一律由
`pagination.total_pages(total)` 计算，禁止手写 `(total + page_size - 1) / page_size`。

### 2.5 模型与注释

- 模型按资源域拆分在 `crates/foims-models/src/models/<domain>.rs`，
  经 `mod.rs` glob 再导出，调用方路径 `crate::models::X` 不变。
- 每个模块第一行 `//!` 模块头；pub 函数（尤其 handler）`///` 注明用途与
  错误情形，范本见 `crates/foims-resource/src/device/nic.rs`。
- 更新接口的可空字段用 `Option<Option<T>>`（`deserialize_some`）：
  缺省不改、`Some(None)` 置空；`QueryBuilder` 直接 `push_bind(Option)` 编码。

## 3. 前端（Vue 3 + Vite）

前端为仓库根 `frontend/` 的 Vue 3 + Vite 工程（MPA 三入口）。旧版原生 JS
前端整体存档于 `web-archive/`（只读参考，禁止在其上继续开发）；本文档
前端规范只针对 `frontend/`。

### 3.1 工程结构与构建

```
frontend/
├── index.html / init_index.html / main.html  MPA 三入口（登录/初始化向导/主应用）
├── vite.config.js         构建配置（入口映射；dev 代理 /api → 本机 nginx:80）
├── public/icons/          lucide 图标（URL /icons/lucide/，dev/生产均静态直服；
│                          仅保留拓扑设备类型实际使用的 10 个 SVG，新增设备类型时按
│                          utils/visualization/icons.ts 的 DEVICE_ICON_FILES 取用同名文件）
└── src/
    ├── entries/           入口装配（login/init/main：createApp + 样式 + 插件）
    ├── pages/             页面级组件（MainShell + 各分区 Page）
    ├── components/        通用组件 + 按域分组组件（<域>/<名称>Modal.vue 等）
    ├── composables/       组合式函数（useToast/usePagedList/useSubTabs/useModalStack…）
    ├── api/client.ts      API 客户端（契约见 3.3，全站唯一请求出口）
    ├── api/<域>.ts        域级传输层（logs/visualization/system/users/auth…：路径 + 载荷类型集中于此）
    ├── types/api/         载荷类型（generated/ 为 ts-rs 只读产物，contracts.ts 手写契约）
    ├── i18n/              vue-i18n 实例 + 双语词条 locales/{zh,en}.json
    ├── stores/            pinia store（auth）
    ├── router/            vue-router（hash 模式，NAV_ITEMS 即侧边栏数据源）
    ├── styles/            CSS（规范见 docs/CSS-style.md，@layer 层序体系）
    ├── utils/             纯工具（network/pagedFetch/formatter/session…）
    │   └── visualization/  画布算法与视图模型（geometry/viewModel/resources/icons）
    ├── config/            静态配置（deviceOptions/icons/orgConfig）
    └── modules/           已退役：可视化画布生态已全量 Vue + TS 化（见 3.6）
```

- dev：`cd frontend && npm run dev`（5173）；构建：`npm run build`，产物
  `dist/` 部署到 `/usr/share/foims/web`（内容直接铺开，无 `static/` 中间层）。
- 缓存与版本：Vite 内容指纹（`assets/` 哈希文件名）承担静态资源长缓存，
  旧原生前端的 `?v=`/MODULE_VERSION 独立版本号机制已随框架切换废弃；
  nginx 对 `/assets/` 长缓存、入口页 `no-cache`（见 `deploy/nginx/foims.conf`）。
  前端版本由 `frontend/package.json` 的 `version` 承载，按 `docs/dev_version.md`
  的 x/y/z 判级独立联动（与 Cargo.toml 同机制、独立计数）。

### 3.2 Vue 风格

- 组件一律 `<script setup>` 组合式 API，组件头用块注释说明来源与职责
  （范本：`components/ModalShell.vue`）；新组件按域放 `components/<域>/`，
  跨分区复用放 `components/` 根。
- 服务端状态用 TanStack Query（`@tanstack/vue-query`）管理缓存与失效；
  客户端会话态用 pinia（`stores/auth.js`）。禁止手工维护平行的缓存副本。
- 分页列表统一 `usePagedList`（排序/页大小/越界回退/跨页序号）+
  `Pagination.vue`，不要各页面自写一套（历史克隆版已全部退回共享实现）。
  `buildUrl(page, pageSize, sortBy, sortOrder)` 为四参签名；无分页列表
  （如定时任务）复用 `sortClass/onThSort` 时只需提供 `SortableTable`
  （`sort` + `toggleSort`），禁止用 `as unknown as PagedList` 补形状。
- 分区子页签统一 `useSubTabs`（route.query.tab 深链优先 → localStorage 记忆 →
  清单首项默认，切换回写并用 replace 更新 query）；画布类页面传
  `syncQuery: false`，切换副作用走 `onSwitch`。不要再在各页面手写
  `readInitialTab/switchTab/watch` 三件套。
- 用户通知一律 `showToast`（`composables/useToast.js`）；确认框用
  `ConfirmModal.vue`；`alert/confirm` 禁用。
- 弹窗统一 `ModalShell.vue` 外壳（遮罩/标题栏/插槽/Esc 关闭）；嵌套子弹窗
  的 Esc 层级由 `useModalStack` 保证（只关最上层），禁止手写遮罩或操作
  `document.body.style.overflow`。
- 异步一律 `async/await`；`catch` 不许静默：用户路径给 toast，后台路径
  `console.error` 带上下文。
- 页面级定时器用 `usePageTimeout`（分区切换自动清理），禁止悬挂裸
  `setTimeout`。
- 颜色不硬编码：一律 `var(--color-*)/var(--chart-*)/var(--status-*)`。

### 3.3 API 客户端契约

- 全站请求统一走 `api/client.ts`（`apiGet/apiPost/apiPut/apiPatch/apiDelete`），
  **永不 reject**：
  失败归一为 `{ success: false, message, … }`，调用方按 `success` 分支，
  **不写 try/catch 死代码**。唯一例外：探活类 raw fetch（连接失败属预期
  的静默重试，如初始化向导等待后端重启），必须注释说明。
- 服务端 `message` 为 i18n 键，客户端经 `translateServerMessage` 就地翻译，
  原始键保留在 `message_key` 供调用方分支（如验证码联动）。
- 认证用 HttpOnly Cookie（`credentials: include`）；401 自动刷新重试
  （公共端点除外，`isPublicAuthEndpoint` 白名单），429 按 `Retry-After`
  退避。
- 文件下载归一为 `{ success, data: Blob, isBlob: true, filename }`，落盘用
  `utils/download.ts`。判定顺序：`Content-Disposition: attachment` 或下载类
  Content-Type 优先于 JSON 解析（配置备份等端点以 `application/json` +
  `attachment` 下发）。
- 域级封装：同一后端子域的 URL 与载荷类型收进 `api/<域>.ts`（参照
  `api/logs.ts`、`api/system.ts`、`api/auth.ts`、`api/visualization.ts`），页面与
  组件不再拼 `/api/...` 字面量。
  该模块只做「组装 URL / 请求体 → 返回 `ApiResult`」，不做 toast、不做 i18n；
  存量页面按域渐进迁移，新增与改动代码必须走域模块。
- 域模块导出名与被调方局部函数重名时，用 `import { x as y } from "../../api/<域>"`
  别名导入（如 `testSmtpConnection as requestSmtpTest`），不改组件对外暴露的函数名。
- 按段读写的配置端点（`/api/system/config`）用 `getSystemConfig<T>()` /
  `updateSystemConfig(payload)` 泛型承载：各卡片只声明自己消费的段类型，不复制整份配置契约。

### 3.4 i18n

- 词条在 `frontend/src/i18n/locales/{zh,en}.json` 双语维护，en/zh 必须
  双向对齐（校验命令见 3.5）。
- 词条占位符维持旧版 `{{name}}` 写法，导入时自动归一化为 vue-i18n 的
  `{name}` 语法（`@`/`|`/`$` 特殊字符由 `i18n/index.js` 统一转义，
  词条文件无需关心）；组件内 `t("device.conflict_summary", { count: 3 })`。
- 切换语言统一 `changeLanguage`（`i18n/index.js`：持久化 localStorage 并
  同步 `<html lang>`），禁止组件自行改 `i18n.global.locale`。
- 无障碍属性同样走 i18n：`aria-label` 必须绑 `:aria-label="t('…')"`
  （表格、区块、仅图标按钮一律给标签），禁止写死英文或其他单语种文案。

### 3.5 校验命令

```bash
# 类型检查（vue-tsc 全量；npm run build 已前置含此步，开发循环可单跑）
cd frontend && npm run typecheck

# 裸 tsc（TS 7 原生编译器，构建/脚本用；.vue 与全量类型检查入口仍是 vue-tsc）
cd frontend && npm run tsc -- --version

# 后端载荷 TS 类型再生成（ts-rs export_bindings；生成物提交入库，CI 漂移校验）
bash da-scripts/gen-ts-types.sh

# 构建（含 vue-tsc 类型检查 + SFC 编译校验，失败即挂）
cd frontend && npm run build

# CSS 结构与格式 lint（根 biome.json；CSS 归 Biome，无 stylelint）
# 规范命令（与 CI 完全一致，版本固定）：
npx --yes @biomejs/biome@2.6.3 check frontend
# 本机已装同版本 devDependency 时可直接：npx biome check frontend

# JS 质量 lint（根 .oxlintrc.json；web-archive/ 与 frontend/dist/ 已忽略）
npx oxlint@1.89.0

# lint 工具链版本升级：一次改写全部引用点（ci.yml / package.json /
# biome.json 的 $schema / 本文），再安装本机二进制 + 探针验证 + 全量检查
bash da-scripts/upgrade-lint-tools.sh           # 升到 npm latest
bash da-scripts/upgrade-lint-tools.sh --check   # 只校验四处一致性与本机版本

# 可视化纯函数自检（geometry 与 viewmodel 两个可执行脚本）
cd frontend && npm run selfcheck:viz

# i18n 双向 parity
node -e "const zh=require('./frontend/src/i18n/locales/zh.json'),en=require('./frontend/src/i18n/locales/en.json'); \
const f=(o,p='')=>Object.entries(o).flatMap(([k,v])=>typeof v==='object'?f(v,p+k+'.'):[p+k]); \
const z=new Set(f(zh)),e=new Set(f(en)); \
console.log('en缺失', [...z].filter(k=>!e.has(k)), 'zh缺失', [...e].filter(k=>!z.has(k)));"
```

- lint 职责划分：CSS 结构/格式归 **Biome**（`biome.json`，含
  `useLayeredStyles` 强制 @layer 层序），JS 质量归 **Oxlint**
  （`.oxlintrc.json`）。
- 版本策略：两个工具一律**固定精确版本**，不用 `@latest`——lint 是 CI 阻断门，
  需可复现（上游发版不应让同一提交时红时绿），且 `biome.json` 的 `$schema`
  只能写具体版本。“忘记同步”这个真痛点用
  `da-scripts/upgrade-lint-tools.sh` 解决：它把版本改全四处（`ci.yml` 的 npx
  版本、根 `devDependencies`、`biome.json` 的 `$schema`、本文命令示例）、
  安装本机二进制、跑探针与全量检查；手改漏一处可被 `--check` 拦下。
- Biome 工具就位：根 `package.json`/`package-lock.json` 被 `.gitignore` 的
  `/package*` 排除（不入库），因此本地 `@biomejs/biome` 属本机前置，CI 由
  `npx --yes @biomejs/biome@<version>` 自取；两者版本必须相同。
  `da-scripts/release_check.sh` 优先调用 `node_modules/.bin/` 中的二进制，
  PATH 上的同名旧包不再可能冒充检查。
- 镜像缺包处置：npm 镜像（如 npmmirror）可能未同步 Biome 的平台二进制，
  表现为安装后只剩 musl 变体、`biome` 报
  `Cannot find module '@biomejs/cli-linux-x64/biome'`；补装走官方源：
  `npm i --no-save --no-package-lock --registry=https://registry.npmjs.org`
  `@biomejs/cli-linux-x64@<同期版本>`。
- TypeScript 7 双安装：`vue-tsc` 通过 `typescript/lib/tsc` 定位编译器，
  TypeScript 7.x（原生化重写）不再导出该子路径，单装 TS 7 会让 typecheck 直接报
  `ERR_PACKAGE_PATH_NOT_EXPORTED`。已采用 vue-tsc 3.3.8+ 官方双安装方案
  （vuejs/language-tools PR #6123）：`typescript` 别名指向
  `npm:@typescript/typescript6@latest`（TS 6 JS API 兼容层，vue-tsc 检测到该别名后
  经其内部依赖 `@typescript/old` 拿到完整 TS 6 编译器），`typescript7` 别名指向
  `npm:typescript@latest`（TS 7 原生编译器）；`npm run tsc` 显式走 typescript7，
  因为 npm 对 `.bin/tsc` 同名冲突无优先级保证，会被提升的 `@typescript/old` 的
  bin 抢占为 TS 6；typecheck/build 的类型检查仍由 vue-tsc（TS 6 链路）承担。
  TS 7.1 发布新 API、vue-tsc 官方适配后可收敛双安装。
- 缓存与版本策略分开治理：`@latest` 的解析结果是 registry 的 dist-tag，
  同一时间点在不同机器/CI 上拉取结果一致；本地与 CI 出现版本差异，原因是
  本地 npm/npx 缓存或镜像同步滞后，**修法是刷新缓存**（`npx --prefer-online`、
  `npm cache verify`、`npm view --prefer-online`），不是去改版本号。固定版本
  命中缓存时内容不变（按版本号与 tarball 校验），保留固定版本的理由只有两条：
  阻断门需跨时间可复现、`$schema` 只能写具体版本。
- 依赖声明策略（与上面的“lint 工具固定版本”不同，两者不要混用）：
  - 前端 `frontend/package.json` 的 npm 依赖用 `latest` 声明，实际版本由**入库的
    `package-lock.json`** 锁定（CI 走 `npm ci`，严格按 lock 安装），所以声明浮动
    不影响构建可复现性；升级即 `npm install <包>@latest` 后跑 typecheck + build；
    `typescript`/`typescript7` 同样走 latest（npm 别名双安装，见上条）。
  - 后端 Rust 无 `latest` 语义（crates.io 只有具体版本号）：`Cargo.toml` 写具体
    版本、入库的 `Cargo.lock` 锁实际版本；升级 = 改版本串后跑
    `cargo check/clippy/test`，跨 major 需适配 API。
  - 仓库有三棵独立依赖树：主 workspace、`foims-agent/Cargo.toml`（独立 workspace）、
    `foims-portal/`（`.gitignore` 排除，本地部署，升级后需单独 `cargo check`）。
- 空跑防护：审计命令的“0 诊断”须先证明命令确实在执行——历史上 `npx biome`
  曾解析到 npm 上同名旧包 biome@0.3.3（无 check 子命令，静默 exit 0）；修改
  biome 依赖/配置后可放一个不合格式的 CSS 探针（如 `.probe{color:red}`）确认
  `check` 报错后再删除。
- 抑制约定：经评估的误报以 `// biome-ignore lint/<rule>: 中文理由` 行内
  豁免，禁止无理由豁免与文件级豁免。

### 3.6 可视化画布（工位 / 机柜 / 拓扑）

可视化分区已全量 Vue + TS 化（旧 `modules/visualization/` 命令式 JS 生态与
`templates/modals/`、桥接层 `legacyBridge/legacyModalLoader/legacyResources`
均已删除，不得重建）。职责分层：

- `pages/VisualizationPage.vue`：唯一状态持有者——页签、组织/房间下拉、三套
  数据编排（加载/自动排布/保存）、弹窗开关与确认流；
- `components/visualization/`：`WorkstationCanvas/CabinetCanvas/TopologyCanvas`
  三块声明式画布（`CanvasGridLayer` 共享网格层）+ 拓扑四弹窗；
- `composables/useCanvasViewport.ts`/`useCanvasTooltip.ts`：视口（viewBox/缩放/
  坐标换算/容器测量）与悬浮提示，三画布共享；
- `utils/visualization/geometry.ts`：纯几何与布局算法（自动排布/机柜 U 位/
  分组包围盒与推挤/连线路径锚点），无 DOM 与 i18n 依赖；
- `utils/visualization/viewModel.ts`：载荷 → 画布视图模型（节点矩形、分组）；
- `api/visualization.ts`：该分区全部接口。

约束：

- **坐标唯一真相是响应式视图模型**：拖动落定由画布 emit 矩形、页面写回，保存经
  `buildLayoutItems` 投影，禁止从 SVG DOM 反读属性。
- **三态语义**：布局加载区分 `saved/empty/stale/error`，请求失败或已被取代时
  禁止触发自动排布落库（否则整表 upsert 会覆盖已保存布局）。
- **渲染代次**：切房间/组织用递增 token 丢弃过期响应；机柜分批渲染在途
  （`cabinetSettled=false`）时拒绝整表保存。
- 模板 ref 由组件自行 `ref()` 持有后作为参数传入 composable（模板里写
  `ref="x"`），禁止在 composable 内隐式创建模板 ref。

### 3.7 TypeScript 规范

前端为全量 TS 工程（strict 全开 + `noUncheckedIndexedAccess` +
`verbatimModuleSyntax`，tsconfig 为唯一权威配置）：

- **禁 any**：未知结构用 `unknown` + 类型收窄（typeof/instanceof/in）；
  边界处的 `as` 断言必须附中文注释说明依据，禁止无理由断言与 `!` 滥用。
- **生成物只读**：`types/api/generated/`（ts-rs 生成，后端载荷类型）禁止
  手改；后端类型变更后跑 `bash da-scripts/gen-ts-types.sh` 再生成并提交，
  CI 对生成物做漂移校验（`git diff --exit-code`）。
- **手写契约**：`types/api/index.ts` 的 `ApiResult<T>`（client 层归一化
  信封，`data: T | null`）与 `Paged<T>`（`paged_response` 五键）手写维护，
  与 Rust 信封结构有意不同，不从后端导出；后端 json!/跨表组装出的响应
  （如 `GET /rooms/{id}/brief`、日志统计、日志外发）放
  `types/api/contracts.ts` 或对应 `api/<域>.ts` 并导出，禁止在多个组件里
  各定义一份同名接口。
- **API 调用泛型化**：`apiGet<T>` / `apiPost<T, B>` / `apiPut<T, B>` /
  `apiPatch<T, B>` / `apiDelete<T, B>` 必须显式给出载荷类型，优先消费
  generated 类型；T 缺省为 `unknown`，禁止用 `as` 绕过泛型标注。
  禁止绕过封装直接调 `request()` 或裸 `fetch`（探活例外见 3.3）。
- **SFC 类型化**：`<script setup lang="ts">`；props 用 `defineProps<{...}>()`
  （默认值 `withDefaults`），emits 用元组签名 `defineEmits<{ save: [v: X] }>()`；
  ref/reactive 依赖推导，复杂表单状态建 interface。
- **import 后缀规则**：一律无后缀（`from "../api/client"`），工程内已无保留
  的 `.js` 源文件（`tsconfig.json` 的 `allowJs` 已关闭）。
- **类型导入**：`verbatimModuleSyntax` 开启，类型导入必须 `import type`。
- **i18n 消息类型**：消息键为运行时数据（i18n JSON 维护），i18n 实例按
  宽化 schema 消费（`t(key: string): string`），禁止为消息键生成字面量
  联合类型（大型 JSON 深度实例化会触发 TS2589）。

## 4. 布局约定速查

| 场景 | 标准做法 |
| --- | --- |
| 列表接口 | `Pagination::from_query` → QueryBuilder → `paged_response` |
| 更新可空字段 | `Option<Option<T>>` + QueryBuilder `push_bind` |
| crate 间共享类型 | 放 `foims-common`，原路径 `pub use` 兼容 |
| 建表/视图/触发器 | foims-init `schema/tables/`，重复结构数据驱动（见 views.rs） |
| 结构校验清单 | `foims-init/src/check.rs` 与 schema 同步增补 |
| 前端分页列表 | `usePagedList` + `Pagination.vue`（排序/页大小/越界回退） |
| 前端分区子页签 | `useSubTabs`（深链 + 记忆 + 切换副作用） |
| 前端域级接口 | `api/<域>.ts`（URL + 载荷类型集中，页面不拼 `/api/` 字面量） |
| 前端通知 | `showToast`（`composables/useToast.ts`）；确认框 `ConfirmModal.vue` |
| 前端弹窗 | `ModalShell.vue` + `useModalStack`（Esc 只关最上层） |
| 前端 IP 校验 | `import { isValidIP, isIpInCidr } from "../utils/network"`（frontend/src/utils/） |
| 前端 API 载荷类型 | `types/api/generated/`（ts-rs 生成只读）+ 手写信封 `types/api/index.ts`；后端类型变更后跑 `da-scripts/gen-ts-types.sh` |

