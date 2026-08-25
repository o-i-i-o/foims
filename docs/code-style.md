# IPMA 代码风格规范

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
  变更走“直接执行 SQL + 同步完善 ipma-init 的 DDL”，不用迁移框架。
- 新增功能（rust、js）、新增前端布局、新增样式代码 保障代码健壮性的前提下遵循最少代码实现原则
## 2. Rust（2024 Edition）

### 2.1 工程结构

```
ipma（bin/lib）            业务 handler 与模型
├── crates/ipma-common      ApiResponse / ok_json / PG 错误归类（唯一副本）
├── crates/ipma-init        建库建表/校验/备份恢复
├── crates/ipma-visualization
├── crates/ipma-data-manager CSV 导入导出
└── crates/ipma-scheduler   定时任务
```

- 跨 crate 共享的类型与工具放 `ipma-common`，**不得在多个 crate 各存一份副本**。
- 依赖版本统一由根 `Cargo.toml` 的 `[workspace.dependencies]` 管理，子 crate
  一律 `workspace = true`。
- `[workspace.lints.clippy]` 已启用 unwrap/expect/print/dbg/todo/unreachable/
  allow_attributes 护栏，CI 以 `-D warnings` 强制，不要新增违例。

### 2.2 错误处理

- 统一 thiserror 枚举 + `AppError` 风格；禁止 anyhow 混用。
- `From<sqlx::Error>` 一律委托 `ipma_common::classify_db_error`：

```rust
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match ipma_common::classify_db_error(&err) {
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
- ILIKE 模式先经 `crate::utils::escape_like` 转义，排序字段走 match 白名单。
- 行映射统一 `query_as::<T>` + `#[derive(sqlx::FromRow)]`；不由 SQL 携带的
  字段用 `#[sqlx(skip)]` 后在代码中填充。
- 事务约定：**多步写操作（存在性检查 + 写入 + 回读）必须包在同一事务**；
  单条 UPDATE/DELETE 可直接执行并以 `rows_affected()==0` 判 NotFound。

### 2.4 API 响应

- 响应体统一 `ipma_common::ApiResponse { success, message, data }`，
  成功响应用 `crate::error::ok_json(data, "消息")`。
- 分页列表响应统一（`src/utils/pagination.rs`）：

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
- 异步一律 `async/await`（不用 `.then` 链）；事件一律 `addEventListener`
  （不用 `.onclick=` 赋值）；常驻节点防重复绑定用 `dataset.bound` 标志。
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
node -e "const zh=require('./web/static/js/i18n/zh.json'),en=require('./web/static/js/i18n/en.json'); \
const f=(o,p='')=>Object.entries(o).flatMap(([k,v])=>typeof v==='object'?f(v,p+k+'.'):[p+k]); \
const z=new Set(f(zh)),e=new Set(f(en)); \
console.log('en缺失', [...z].filter(k=>!e.has(k)), 'zh缺失', [...e].filter(k=>!z.has(k)));"

# 格式化（有 node 工具链时）
npx prettier --config web/.prettierrc --write "web/static/js/**/*.js" "web/static/css/**/*.css"
```

## 4. 布局约定速查

| 场景 | 标准做法 |
| --- | --- |
| 列表接口 | `Pagination::from_query` → QueryBuilder → `paged_response` |
| 更新可空字段 | `Option<Option<T>>` + QueryBuilder `push_bind` |
| crate 间共享类型 | 放 `ipma-common`，原路径 `pub use` 兼容 |
| 建表/视图/触发器 | ipma-init `schema/tables/`，重复结构数据驱动（见 views.rs） |
| 结构校验清单 | `ipma-init/src/check.rs` 与 schema 同步增补 |
| 前端下拉 | `fillSelect(selectId, url, { placeholderKey, filter, itemToLabel })` |
| 前端表格 | `renderTable` + `appendPaginationToTable` + `createSortState` |
| 前端 IP 校验 | `import { isValidIP, isIpInCidr } from "../utils/network.js"` |
