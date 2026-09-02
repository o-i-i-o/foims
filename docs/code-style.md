# FOIMS 代码风格规范

本文档是全项目代码风格的唯一权威来源，AGENTS.md 与 `.trae/rules/` 指向此处。
目标：多人长期迭代下风格不漂移。写代码前先读对应章节，改代码时遵守既有模式。

---

## 1. 通用

- 编码 UTF-8，换行 LF，文件末尾保留一个空行。
- 注释统一中文，解释“为什么”而非复述代码；公共 API 必须有文档注释。
- 禁止 `unwrap()`/`expect()`/`unreachable!()`/`#[allow(...)]`（测试代码除外）；
  禁止用 `_` 前缀忽略参数。**唯一例外**：仅需鉴权副作用的提取器参数
  `_admin: AdminUser` / `_user: CurrentUser`（axum 惯用法，见 routes/mod.rs）。
- 版本号规则：`0.x.yy`，每次代码更新 bump `yy`（`yy>=99` 时 `x+1, yy=0`）。
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
└── crates/foims-scheduler       定时任务
```

- 依赖方向自上而下（`resource → auth → models → common`），禁止反向依赖与环。
- 跨 crate 共享的类型与工具放 `foims-common` / `foims-models`，
  **不得在多个 crate 各存一份副本**。
- 业务 crate 不依赖主程序：状态访问经依赖倒置——handler 面向
  `foims_common::DbProvider`（连接池）与 `foims_auth::provider::AuthProvider`
  （认证扩展）泛型编写，由主程序 `AppState` 实现；主程序路由注册处
  以 turbofish（`handler::<AppState>`）单态化。
- 依赖版本统一由根 `Cargo.toml` 的 `[workspace.dependencies]` 管理，子 crate
  一律 `workspace = true`。
- `[workspace.lints.clippy]` 已启用 unwrap/expect/print/dbg/todo/unreachable/
  allow_attributes 护栏，CI 以 `-D warnings` 强制，不要新增违例。

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
- 面向用户的消息为完整中文句子（如 `"该端口号已存在"`），也要在代码里做 i18n 键。

### 2.3 数据库访问

- **动态 SQL 一律用 `sqlx::QueryBuilder`**，禁止手写 `$N` 占位符记账：

```rust
let mut builder = QueryBuilder::<Postgres>::new("SELECT COUNT(*) FROM workstations w");
push_filters(&mut builder, ...);   // 条件拼接抽成 fn，供 COUNT/数据两查共用
let total: i64 = builder.build_query_scalar().fetch_one(&pool).await?;
```

- 拼接动态 SQL 字符串（`format!` 结果）传给 `sqlx::query*` 时用
  `sqlx::AssertSqlSafe(...)` 显式声明已审计；用户输入永远走 `push_bind`。
- ILIKE 模式先经 `foims_common::net::escape_like` 转义，排序字段走 match 白名单。
- 行映射统一 `query_as::<T>` + `#[derive(sqlx::FromRow)]`；不由 SQL 携带的
  字段用 `#[sqlx(skip)]` 后在代码中填充。
- 事务约定：**多步写操作（存在性检查 + 写入 + 回读）必须包在同一事务**；
  单条 UPDATE/DELETE 可直接执行并以 `rows_affected()==0` 判 NotFound。

### 2.4 API 响应

- 响应体统一 `foims_common::ApiResponse { success, message, data }`，
  成功响应用 `foims_common::ok_json(data, "消息")`。
- 分页列表响应统一（`foims_common::pagination`）：

```rust
Ok(ok_json(paged_response(items, total, &pagination), "获取成功"))
```

响应键固定为 `items/total/page/page_size/total_pages`；`total_pages` 一律由
`pagination.total_pages(total)` 计算，禁止手写 `(total + page_size - 1) / page_size`。

### 2.5 模型与注释

- 模型按资源域拆分在 `src/models/<domain>.rs`，经 `mod.rs` glob 再导出，
  调用方路径 `crate::models::X` 不变。
- 每个模块第一行 `//!` 模块头；pub 函数（尤其 handler）`///` 注明用途与
  错误情形，范本见 `src/resource/device/nic.rs`。
- 更新接口的可空字段用 `Option<Option<T>>`（`deserialize_some`）：
  缺省不改、`Some(None)` 置空；`QueryBuilder` 直接 `push_bind(Option)` 编码。

## 3. 前端（HTML5 / 原生 ES2025 / CSS3）

### 3.1 模块与加载

- 纯 ES Modules，无全局变量；入口 `app.js`，业务模块经
  `resourceLoader.loadModule()` 懒加载（带 Map 缓存与 `?v=` 防双实例）。
- 新工具放 `static/js/utils/`；IP/CIDR 校验用 `utils/network.js`，
  下拉填充用 `utils/resources.js` 的 `fillSelect`，不要各写一份。

### 3.2 JS 风格

- 格式以 prettier（`.prettierrc`）为准：2 空格缩进、双引号、分号、
  模板字符串（不用 `+` 拼接）、`const` 优先。
- 异步一律 `async/await`（不用 `.then` 链）。事件绑定按节点生命周期分两类
  （详见 3.7 模态框生命周期）：
  - **常驻节点**（页面表格、document 委托）：`addEventListener` +
    `dataset.bound` 标志防重复绑定；
  - **模态框内部节点**（DOM 随 closeModal 销毁重建）：优先
    `onclick/onsubmit` 属性幂等赋值；确需 addEventListener 时必须在
    函数内成对 removeEventListener，禁止依赖"反正会重建"而裸绑。
- DOM 访问优先 `elementCache`；表格渲染统一 `renderTable`（含 loading 等
  多态的特殊页面除外，如 fail2ban/scheduledTask）。
- 用户通知一律 `showToast`；`alert/confirm` 禁用，确认框用 `showConfirm`。
- `catch` 不许静默：用户路径给 toast，后台路径 `console.error` 带上下文。
- i18n 插值统一双花括号：`t("device.conflict_summary", { count: 3 })`，
  词条写作 `"{{count}} 个端口…"`，禁止手动 `.replace()`。
- 颜色不硬编码：HTML/JS 一律 `var(--color-*)/var(--chart-*)/var(--status-*)`
  （见 `variables.css`）；SVG 属性用 `style.fill = "var(...)"` 形式。
- en/zh 词条必须双向对齐（每次改完跑 parity 校验，见 3.5）。

### 3.3 CSS

- 4 空格缩进（prettier 不处理 css 时保持现状），选择器功能语义命名
  （`.data-table`、`.pagination-btn`），不引入 BEM。
- 颜色/圆角/间距/层级一律走 `variables.css` 变量；`color: white`、
  `border-radius: 4px`、手写主色 alpha 均属违例。

### 3.4 HTML

- 页面与 modal 片段 2 空格缩进；void 元素（input/br/img/link/meta/hr…）
  不写 XHTML 自闭合斜线；SVG 内部元素保持 `/>`（foreign content 需要）。
- 微布局不写行内 style，用工具类（`utilities/helpers.css`，如
  `.form-group-inline`、`.form-checkbox`），缺类先补类再使用。
- 表单 label 必带 `for`；图标按钮带 `aria-label`；文案带 `data-i18n` 且
  以英文兜底文本。
- 模态框 HTML 抽离边界：**静态骨架**（固定表单/文案）放
  `modals/<域>/<名称>-modal.html`，经 `modalLoader.js` 注册挂载，值用
  DOM API 填充；**数据驱动正文**（`.map()` 行循环、条件分支、运行时
  计算值，如 TopologyModal 的端口表）保留
  在 JS 渲染函数中，仅注入骨架的空容器。modalLoader 不支持模板占位
  符/循环/条件，强行抽离动态内容需另造模板引擎，不做。
- 模态框表单元素统一命名（不再新增同类别名类）：
  - Field 字段：`.form-group` + `<label for=…><span data-i18n=…>名称</span>…</label>`；
  - Required field 必填项：`<span class="required">*</span>`（不用
    `<abbr>` 或其他标记承载）；
  - 描述提示文字：`.hint-text`（唯一类，`form-hint` 等别名已废弃）；
  - 描述文本域：不写固定 `rows`（高度走 `field-sizing` 自适应），
    统一带 `data-i18n-placeholder="common.description_placeholder"`。

### 3.5 缓存与版本

- 静态资源缓存策略在 `deploy/nginx/*.conf`：`/static/js/` 必须
  `no-cache`（ES 模块间相对导入不带版本号，长缓存会导致陈旧模块）；
  CSS/图片入口带 `?v=` 可长缓存。
- 改动 JS/CSS 后同步 bump：`main.html` 等页面的 `?v=`、
  `resourceLoader.js` 的 `MODULE_VERSION`（与前端资源版本同号）。

### 3.6 校验命令

```bash
# JS 语法门禁（59 个模块全过）
find web/static/js -name "*.js" -exec sh -c 'node --input-type=module --check < "$1"' _ {} \;

# import 死链扫描（相对导入 + MODULE_REGISTRY 全量核对）
python3 - <<'EOF'
import re, os, glob
base = "web/static/js"
for path in glob.glob(f"{base}/**/*.js", recursive=True):
    for m in re.finditer(r'from\s+["\'](\.[^"\']+)["\']', open(path, encoding="utf-8").read()):
        target = os.path.normpath(os.path.join(os.path.dirname(path), m.group(1)))
        if not os.path.exists(target):
            print("断链:", path, "->", m.group(1))
for name, p in re.findall(r'(\w+):\s+"(/static/js/[^"]+)"', open(f"{base}/utils/resourceLoader.js", encoding="utf-8").read()):
    if not os.path.exists("web" + p):
        print("注册表悬空:", name, p)
EOF

# i18n 双向 parity
node -e "const zh=require('./web/static/i18n/zh.json'),en=require('./web/static/i18n/en.json'); \
const f=(o,p='')=>Object.entries(o).flatMap(([k,v])=>typeof v==='object'?f(v,p+k+'.'):[p+k]); \
const z=new Set(f(zh)),e=new Set(f(en)); \
console.log('en缺失', [...z].filter(k=>!e.has(k)), 'zh缺失', [...e].filter(k=>!z.has(k)));"

# 前端 lint 全量（需 node/npm；格式归 prettier，代码质量归 eslint，
# CSS 归 stylelint，HTML 归 htmlhint，纯工具函数归 jest）
cd web && npm ci && npm run lint && npm run format:check && npm test
```

lint 职责划分（2026-08 起，配置见 `web/eslint.config.mjs`（ESLint 9 平面配置）/
`web/.stylelintrc.json` / `web/.htmlhintrc` / `web/.prettierrc`）：
- **eslint**：代码质量与潜在缺陷（no-unused-vars、eqeqeq、no-else-return、prefer-const 等）
  外加插件：`eslint-plugin-import`（导入规范与死链）、`eslint-plugin-sonarjs`
  （复杂度/重复串/隐患模式，认知复杂度阈值 40、重复串阈值 6，校准理由见配置内注释）、
  `eslint-config-prettier`（关闭与 prettier 冲突的格式规则，置于配置末尾）；
  格式类规则已全部移交 prettier，避免两者对模板串/三元换行的判定冲突；
- **prettier**：仅管 `static/js` 与 lint 配置文件（CSS 按上文约定保持 4 空格
  手工排版，见 `web/.prettierignore`）；
- **stylelint**：CSS 结构性检查（基线 `stylelint-config-standard` + 项目覆盖规则）；
  `!important` 默认禁止，打印隐藏/工具类等压制场景
  以 `/* stylelint-disable-line declaration-no-important -- 原因 */` 显式豁免；
  SVG 几何属性 `rx/ry` 因标准属性值表未收录而在配置中豁免；
- **htmlhint**：入口页与模态片段共用 `web/.htmlhintrc`（片段无 doctype/lang，
  相应文档级规则关闭）；入口页另跑 `npm run lint:html:entries` 补查
  doctype-first/html-lang-require/title-require；
- **jest**：`tests/` 下纯工具模块单元测试（network IP/CIDR、helpers、
  sessionManager），jsdom 环境、babel 按当前 Node 目标转译（无实验 flag），
  `npm test` 运行；
- **depcheck**：依赖健康检查（CLI 工具类依赖在 `web/.depcheckrc` 豁免），
  `npm run depcheck` 运行；
- **lighthouse-ci**（`web/lighthouserc.json`）：对登录页与初始化向导做
  性能/可访问性/最佳实践/SEO 审计，`npm run lighthouse` 运行；
  需本机可用 Chromium/Chrome（经 `CHROME_PATH` 指定，chrome-headless-shell
  须连同 `icudtl.dat` 等伴随文件整目录使用）。2026-08 基线：两页
  performance 100 / accessibility 100 / seo 100 / best-practices 96
  （仅 errors-in-console：静态服务器把 /api/* 回 404 HTML 所致的环境产物，
  生产 nginx 有后端代理，故该断言关闭）。类别断言 warn 级 minScore 0.9，
  结果不外传。
- **抑制约定**：个别经评估的误报以 `// eslint-disable-next-line <rule> -- 中文理由`
  行内豁免（与 stylelint 同款约定），禁止无理由豁免与文件级豁免。

### 3.6.1 已知豁免清单

- `sonarjs/no-duplicate-string` 白名单：`device-*-id` 等模态框元素 id
  必须保持字面量（`tests/frontend_consistency.rs` 按字面量正则做悬空校验）；
- `sonarjs/no-hardcoded-passwords`：`login.js` 的 `"password-login"` 为登录
  方式枚举值（与 index.html 的 data-tab 联动），非密钥；
- `sonarjs/pseudo-random` / `sonarjs/void-use`：登录页角色动画的
  `Math.random` 抖动延迟与 `void offsetHeight` 强制 reflow，均非安全场景。

### 3.7 模态框生命周期（唯一权威范式）

全站模态框统一走 `modalLoader.js`，生命周期为：
**`closeModal` 即销毁 DOM（`modal.remove()`）→ 下次打开从 `htmlCache`
重新注入全新节点**。所有绑定与填充策略都建立在这个事实上：

- **打开范式**："数据预取并行、填充串行于 DOM 就绪后"。
  `Promise.all` 只并行原始数据请求与 `openModal`；`fillSelect` 内部
  会等待目标元素出现（`waitForElement`），因此与 `openModal` 并行调用
  安全；其余手写填充必须在 `await openModal()` 之后执行。
- **绑定范式**：模态内按钮用 `onclick = fn` 幂等赋值（重复打开/
  刷新重绑不叠加监听）；容器级委托用 `dataset.bound` 标志。
  历史教训（device.js 网卡区域注释）：模态 DOM 每次销毁重建，模块级
  "已绑定"标志位会导致第二次打开时新 DOM 零监听 —— 禁止使用。
- **禁止跨模态复用 id**：两个模态框同屏时 `getElementById` 只返回文档序
  第一个，监听会绑错元素。同类别按钮 id 必须加模态前缀区分。
- **层叠模态**：`openModal/closeModal` 内部维护打开计数，只有归零才恢复
  `body.overflow`；不要在业务代码里直接改 `document.body.style.overflow`。
- **并行加载**：模态框 HTML（`openModal`）与首屏数据请求互不依赖时放同
  一个 `Promise.all`；首次打开的 HTML 已由 `prefetchModalsOnIdle` 在空闲
  时预热进内存缓存，无需担心串行 RTT。
- **安全**：任何拼进 `innerHTML`/模板字符串的用户可控值（名称、描述、
  枚举回退值）必须过 `escapeHtml`；`renderTable` 的 render 回调返回
  字符串时同样适用。

### 3.8 校验命令（cargo test 集成）

id/注册表/i18n 键/版本号四类一致性校验已固化为 Rust 集成测试
（`tests/frontend_consistency.rs`），随 `cargo test` 运行，无需 Node
工具链。

## 4. 布局约定速查

| 场景 | 标准做法 |
| --- | --- |
| 列表接口 | `Pagination::from_query` → QueryBuilder → `paged_response` |
| 更新可空字段 | `Option<Option<T>>` + QueryBuilder `push_bind` |
| crate 间共享类型 | 放 `foims-common`，原路径 `pub use` 兼容 |
| 建表/视图/触发器 | foims-init `schema/tables/`，重复结构数据驱动（见 views.rs） |
| 结构校验清单 | `foims-init/src/check.rs` 与 schema 同步增补 |
| 前端下拉 | `fillSelect(selectId, url, { placeholderKey, filter, itemToLabel })` |
| 前端表格 | `renderTable` + `appendPaginationToTable` + `createSortState` |
| 前端 IP 校验 | `import { isValidIP, isIpInCidr } from "../utils/network.js"` |
