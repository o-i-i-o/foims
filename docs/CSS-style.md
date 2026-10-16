# FOIMS 原生 CSS 代码规范

本文档是全项目 CSS 的唯一权威来源，`docs/code-style.md` §3.3 指向此处。
项目无构建流程（无 PostCSS/autoprefixer/压缩），nginx 直接托管静态 CSS；
写样式前先读本文档，改样式时遵守既有模式。

> 核心原则：**可维护、可复用、减少冲突、性能友好**
> 配套工具：Stylelint（静态检查，配置 `web/.stylelintrc.json`）。
> CSS **不归 Prettier 管**（`web/.prettierignore` 已排除），按第三节手工排版。

1. 颜色、间距、圆角、阴影、断点一律走 CSS 变量（`variables.css`），禁零散硬编码；
2. 主色 lime + 副色 gray（Open Color 色板），低饱和中性色，禁高饱和艳色与纯黑 #000；
3. 尺寸自适应优先 `auto-fit` / `min()` / `clamp()` 原生方案，媒体查询只处理结构性变化；
4. 2 空格缩进、一行一个属性，样式克制简约，不写花哨渐变，不要 AI 模板感。

---

## 目录

1. [文件与目录组织](#一文件与目录组织)
2. [命名规范](#二命名规范)
3. [代码书写格式](#三代码书写格式)
4. [属性书写顺序](#四属性书写顺序)
5. [选择器规范](#五选择器规范)
6. [值与单位](#六值与单位)
7. [颜色与主题](#七颜色与主题)
8. [响应式策略](#八响应式策略)
9. [注释规范](#九注释规范)
10. [性能与最佳实践](#十性能与最佳实践)
11. [禁止清单](#十一禁止清单)
12. [工程化配套](#十二工程化配套)
13. [完整示例](#十三完整示例)

---

## 一、文件与目录组织

### 1.1 目录结构

按职责拆分，一个组件/页面一个样式文件，避免单文件膨胀：

```
web/static/css/
├── reset.css            # 重置样式（项目统一基础，勿在别处重复 reset）
├── variables.css        # CSS 变量唯一来源（色板/控件/断点/圆角/阴影/过渡）
├── responsive.css       # 响应式集中地（仅结构性变化，见第八节）
├── components/          # 跨页面复用组件（buttons/forms/tables/modals/badges…）
├── layouts/             # 布局框架（sidebar/main-content…）
├── modals/              # 模态框专属样式（每个模态框一个文件）
├── pages/               # 页面专属样式（dashboard/organization/login…）
└── utilities/           # 工具类与动画（helpers.css / animations.css）
```

### 1.2 加载方式

- **无 `@import` 入口、无打包**：入口页 `<link>` 静态引入基础文件，
  页面级 CSS 由 `static/js/utils/styleLoader.js` 按需注入（自动带 `?v=` 版本号）。
  新增页面样式时在 `styleLoader.js` 的 `PAGE_STYLES` 注册，不要手写 `<link>`。
- 归属判断：跨页面复用进 `components/`，仅单页使用进 `pages/`，
  仅单个模态框使用进 `modals/`；拿不准时看同类先例。
- 文件名一律 kebab-case：`select-search.css`、`network-usage.css`。

---

## 二、命名规范

### 2.1 类名：kebab-case 组合式

- 一律小写短横线：`.status-badge`、`.form-group-inline`、`.notice-banner`。
- 项目**不使用 BEM 记号**（`__`/`--`），也不使用 `is-`/`js-` 前缀；
  stylelint `selector-class-pattern` 已关闭，语义靠"名词 + 状态/变体"的
  组合类表达，而不是单一长类名。
- **形状与颜色分离**（项目核心约定，范本见 `components/badges.css`）：
  形状类只管布局与外观，颜色类全局唯一定义，组合使用——
  `class="status-badge status-active"`。新徽章复用既有颜色类，不另造色值。

### 2.2 变量命名空间

| 前缀 | 用途 |
|------|------|
| `--color-*` | 色彩（含语义态 `-bg`/`-text`/`-border` 三件套） |
| `--control-*` | 表单控件（背景/边框/高度/圆角/焦点环） |
| `--chart-*` | 图表配色 |
| `--status-*` | 状态色 |
| `--space-*` / `--radius-*` / `--shadow-*` / `--transition-*` | 几何与动效 |

### 2.3 命名禁忌

- ❌ 驼峰：`cardTitle`
- ❌ 下划线 / BEM 记号：`card_title`、`card__title`、`card--large`
- ❌ 拼音 / 中文：`xiangpian`、`盒子`
- ❌ 无意义：`box1`、`aaa`、`tmp`

---

## 三、代码书写格式

CSS 手工排版（不经 Prettier），风格与 Prettier 默认输出保持一致，
方便未来切换；stylelint 只做结构检查，排版靠人遵守本节。

### 3.1 基本规则

- 选择器与 `{` 之间保留一个空格
- `{` 后换行，`}` 单独一行
- 属性：`属性名: 值;`，冒号后加一个空格，末尾必须分号
- 多选择器时，每个选择器独占一行
- 多属性时，**一行一个属性**
- 缩进：2 个空格（不使用 Tab）；嵌套规则每层 +2 空格（最深 4 层，见第五节）

### 3.2 示例

```css
/* ✅ 推荐 */
.status-badge,
.two-factor-badge {
  display: flex;
  align-items: center;
  padding: 0.25rem 0.75rem;
}

/* ❌ 禁止：压缩一行 */
.status-badge,.two-factor-badge{display:flex;align-items:center;padding:0.25rem 0.75rem;}
```

---

## 四、属性书写顺序

按"布局 → 盒模型 → 视觉 → 文本 → 其他"排列，便于阅读；
顺序由人工维护（stylelint 未启用顺序强制），新增代码尽量遵循：

| 顺序 | 类别 | 属性 |
|------|------|------|
| 1 | 定位 | `position`、`top`、`right`、`bottom`、`left`、`z-index` |
| 2 | 盒布局 | `display`、`flex` / `grid` 相关、`float`、`clear` |
| 3 | 盒模型 | `width`、`height`、`margin`、`padding`、`border`、`box-sizing` |
| 4 | 视觉 | `background`、`box-shadow`、`opacity` |
| 5 | 文本 | `font`、`line-height`、`color`、`text-align`、`white-space` |
| 6 | 其他 | `transition`、`transform`、`animation`、`cursor` |

> 重复属性合并（`declaration-block-no-duplicate-properties` 强制），
> 能写简写的不写冗余长手写（`declaration-block-no-redundant-longhand-properties` 强制）。

---

## 五、选择器规范

### 5.1 优先级原则

1. **优先使用 class 选择器**；ID 选择器每条规则至多 1 个
   （stylelint `selector-max-id: 1` 强制），仅限既有主页面表格列宽等
   按 id 定位的场景沿用（见 `components/tables.css`），新增样式不引入
2. **禁止全局标签选择器裸写样式**：`div {}`、`p {}` 会污染全局
3. **嵌套规则最深 4 层**（stylelint `max-nesting-depth` 强制），
   复合选择器至多 5 个简单选择器（`selector-max-compound-selectors`）
4. **`!important` 全面禁止**（`declaration-no-important` 强制）；
   确需压制时按行内豁免约定写：`/* stylelint-disable-line declaration-no-important -- 中文原因 */`，
   禁止无理由豁免与文件级豁免

### 5.2 正反例

```css
/* ✅ 推荐：语义化组合类 */
.status-active { color: var(--color-success-text); }

/* ❌ 禁止：深层嵌套 + 标签选择器 */
.page .wrap .card .info h3 { color: var(--color-text); }

/* ❌ 禁止：ID 选择器写组件样式 */
#header { background: var(--color-bg-white); }
```

---

## 六、值与单位

### 6.1 数字与单位

| 规则 | 正确 | 错误 | 强制方式 |
|------|------|------|----------|
| 0 不带单位 | `margin: 0;` | `margin: 0px;` | `length-zero-no-unit` |
| 十六进制小写、可简写则简写 | `#fff`、`#ced4da` | `#FFFFFF`、`#cceeff→#cef` | `color-hex-length` |
| 禁用命名色 | `var(--color-danger)` | `red` | `color-named: never` |
| 字重用数字 | `font-weight: 500;` | `font-weight: medium;` | `font-weight-notation` |

### 6.2 单位选择

| 场景 | 推荐单位 | 说明 |
|------|----------|------|
| 字号、间距 | `rem` 优先 | 跟随根字号实现整体缩放 |
| 边框、阴影偏移、控件细节 | `px` | 1px 细线等视觉固定值 |
| 容器宽高 | `%` / `fr` / `clamp()` | 布局场景 |

### 6.3 时间与动效

过渡、动画统一秒（s）；通用时长复用 `--transition-*` 变量，不散写魔数：

```css
transition: box-shadow var(--transition-normal);
```

---

## 七、颜色与主题

`variables.css` 是颜色唯一权威，配色体系为 Open Color
（https://yeun.github.io/open-color/）：

- **主色 lime**（交互强调：按钮/焦点/激活态）+ **副色 gray**
  （文字/边框/背景/深色面板）
- **语义态** green/red/yellow/cyan/blue，每色含 `-bg`/`-text`/`-border`
  三件套，配套组合使用

```css
/* 使用（真实变量名） */
.status-active {
  background: var(--color-success-bg);
  color: var(--color-success-text);
  border: 1px solid var(--color-success-border);
}
```

透明度用 CSS Color 4 空格分隔记法：`rgb(102 168 15 / 10%)`
（见 `variables.css` 惯例），不用 `rgba()` 旧式；禁止在业务样式里新增裸 hex 色值。

### 断点约定（与媒体查询配合）

| 名称 | 宽度 | 场景 |
|------|------|------|
| xs | 480px | 竖屏手机 |
| sm | 576px | 横屏手机 |
| md | 768px | 平板 |
| lg | 992px | 大屏（侧栏收窄） |

断点数值以 `variables.css` 头部注释为准，不要随手造新断点。

---

## 八、响应式策略

本项目采用"全局集中 + 组件就近"的两级分工，约定：

1. **尺寸自适应在基础样式内原生完成**：优先 `auto-fit` grid、`min()`、
   `clamp()`，不依赖断点覆盖（如 `width: min(100%, 320px)`）；
2. **媒体查询两级分工**：影响全局的结构性变化（侧栏转顶部导航、根变量
   调整、纵向堆叠）集中进 `responsive.css`；仅与单个页面/组件自身相关的
   响应式（如 login 页、dashboard 卡片、toasts 位置）写在各自文件内就近
   维护——现状 12 个页面/组件文件即是此模式；
3. **条件用现代范围语法**：`@media (width <= 768px)`，
   不写 `max-width: 768px` 旧式。

```css
/* responsive.css 中的写法 */
@media (width <= 768px) {
  /* 侧边栏折叠为顶部横向导航（结构性变化） */
  .sidebar {
    width: 100%;
    flex-direction: column;
  }
}
```

---

## 九、注释规范

- 注释统一**中文**，解释"为什么"（色板归属、组合用法、兼容性原因），
  而非复述代码。
- **只用 `/* */`**：项目无 PostCSS 构建链，`//` 在原生 CSS 中是非法语法
  （会被浏览器当选择器解析报错），一律禁止。
- 注释写在目标规则**上方**，避免行尾长注释（stylelint/eslint 行内豁免
  注释除外——工具机制要求紧跟声明行尾）。

```css
/* 状态徽章：形状类 + 颜色类组合使用（如 class="status-badge status-active"）
   颜色类为全局唯一定义，各页面徽章形状类只负责形状与布局 */
.status-badge { ... }
```

---

## 十、性能与最佳实践

1. **动画只动 `transform` 和 `opacity`**，触发 GPU 合成层，避免重排
2. **避免昂贵属性大面积使用**：`box-shadow`、`border-radius`、`filter`
3. **不用 `*` 通配符做大量重置**，reset.css 已有统一基础
4. **优先 Flex / Grid 布局**，少用 float
5. **抽取公共样式**：相同颜色、尺寸、间距用 CSS 变量或 `utilities/helpers.css` 工具类
6. **字体回退**：`font-family` 必须带系统字体回退栈

```css
font-family: -apple-system, BlinkMacSystemFont, "Segoe UI",
  "PingFang SC", "Microsoft YaHei", sans-serif;
```

---

## 十一、禁止清单

- ❌ 行内样式 `style="..."`（微布局用 `utilities/helpers.css` 工具类，缺类先补类）
- ❌ `!important`（stylelint 强制；确需压制按第五节行内豁免约定）
- ❌ 命名色 `red`/`blue`（`color-named: never`，颜色一律 `var(--color-*)`）
- ❌ 零散硬编码颜色、尺寸魔数
- ❌ BEM 记号（`__`/`--`）与 `is-`/`js-` 前缀混入（统一组合类风格）
- ❌ `//` 注释（原生 CSS 非法）
- ❌ 全局结构性媒体查询散落在组件文件里（统一进 responsive.css；组件自身的响应式就近写）
- ❌ ID 选择器写组件样式、全局标签选择器、嵌套超 4 层
- ❌ 类名驼峰、拼音、无意义缩写

---

## 十二、工程化配套

| 工具 | 作用 | 运行方式 |
|------|------|----------|
| **stylelint** | 结构性检查：`stylelint-config-standard` + 项目覆盖（配置 `web/.stylelintrc.json`） | `cd web && npm run lint` |
| **cargo test** | id/类名悬空、i18n 键、资源版本号等前后端一致性兜底 | `cargo test`（含 `tests/frontend_consistency.rs`） |

- **CSS 不归 Prettier**：`web/.prettierignore` 排除 `static/css/`，
  按第三节手工排版；Prettier 只管 `static/js` 与 lint 配置文件。
- **无构建链**：不用 autoprefixer/cssnano/PostCSS，目标为现代常青浏览器
  （项目不考虑老旧基础设施兼容性），需要前缀时手写。
- stylelint 豁免约定与 `docs/code-style.md` §3.6 一致：
  `/* stylelint-disable-line <rule> -- 中文理由 */`，禁止无理由/文件级豁免。

---

## 十三、完整示例

一个符合本规范的组件样式（形状类 + 颜色类组合，2 空格，中文注释）：

```css
/* 通知横幅：形状类管布局，颜色类组合上色（用法同 components/badges.css） */

.notice-banner {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: var(--space-sm) var(--space-md);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-bg-white);
  font-size: 0.875rem;
  color: var(--color-text);
  transition: box-shadow var(--transition-normal);
}

.notice-banner:hover {
  box-shadow: var(--shadow-sm);
}

/* 颜色变体：只覆盖颜色属性，形状交给基础类 */
.notice-success {
  border-color: var(--color-success-border);
  background: var(--color-success-bg);
  color: var(--color-success-text);
}

.notice-danger {
  border-color: var(--color-danger-border);
  background: var(--color-danger-bg);
  color: var(--color-danger-text);
}
```

响应式改动进 `responsive.css`，用现代范围语法（见第八节）：

```css
@media (width <= 480px) {
  /* 窄屏：横幅纵向堆叠（结构性变化才进本文件） */
  .notice-banner {
    flex-direction: column;
    align-items: flex-start;
  }
}
```

---
