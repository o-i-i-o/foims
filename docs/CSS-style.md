# 原生 CSS 代码规范

> 核心原则：**可维护、可复用、可读性强、减少冲突、性能友好**
> 配套工具：Prettier（格式化）+ Stylelint（静态检查）+ husky / lint-staged（提交校验）
1. 使用CSS变量管理颜色、间距、阴影、圆角，优先rem，减少固定px；
2. 配色使用低饱和中性色系，1主色+1辅助色，禁用高饱和艳色，不要纯黑#000；
3. 阴影使用多层弱透明度，禁止硬黑阴影；圆角区分大小，不要全部统一大圆角；
4. 不要冗余CSS属性；
5. 排版使用系统无衬线字体；
6. 不要写花哨渐变，样式克制简约，接近真实产品UI，不要AI模板感。

---

## 目录

1. [文件与目录组织](#一文件与目录组织)
2. [命名规范](#二命名规范)
3. [代码书写格式](#三代码书写格式)
4. [属性书写顺序](#四属性书写顺序)
5. [选择器规范](#五选择器规范)
6. [值与单位](#六值与单位)
7. [颜色与主题](#七颜色与主题)
8. [响应式与媒体查询](#八响应式与媒体查询)
9. [注释规范](#九注释规范)
10. [性能与最佳实践](#十性能与最佳实践)
11. [禁止清单](#十一禁止清单)
12. [工程化配套](#十二工程化配套)
13. [示例](#十三完整示例)

---

## 一、文件与目录组织

### 1.1 目录结构

按职责拆分，一个组件一个样式文件，避免单文件膨胀：

```
src/styles/
├── base/                # 基础样式
│   ├── reset.css        # 重置样式（normalize / meyer reset）
│   ├── variables.css    # CSS 变量（色板、字号、间距）
│   ├── base.css         # body、链接、字体等全局基础
│   └── utility.css      # 工具类（clearfix、文本截断等）
├── layouts/             # 布局
│   ├── header.css
│   ├── sidebar.css
│   └── footer.css
├── components/          # 组件样式（按组件拆分）
│   ├── button.css
│   ├── card.css
│   └── modal.css
├── pages/               # 页面独有样式
│   ├── home.css
│   └── profile.css
└── index.css            # 入口，按顺序 @import 以上文件
```

### 1.2 命名约定

- 文件名全部小写，使用短横线分隔（kebab-case）：`card-list.css`
- 入口文件统一引入，组件级样式通过 JS 按需引入更佳

---

## 二、命名规范

### 2.1 推荐：BEM 命名法

企业项目最常用，样式作用域清晰，避免全局冲突：

- **Block（块）**：独立模块 `.card`
- **Element（元素）**：块内部子元素，双下划线 `__` 连接 `.card__title`
- **Modifier（修饰符）**：变体或状态，双短横线 `--` 连接 `.card--primary`

```css
.card              {}  /* 块 */
.card__img         {}  /* 元素 */
.card__title       {}  /* 元素 */
.card--large       {}  /* 修饰符 */
.card--disabled    {}  /* 修饰符 */
```

### 2.2 状态类前缀

| 前缀 | 用途 | 示例 |
|------|------|------|
| `is-` | 动态状态 | `is-active`、`is-hidden`、`is-disabled` |
| `has-` | 包含某子元素 | `has-avatar`、`has-icon` |
| `js-` | 纯 JS 钩子，**不写样式** | `js-submit-btn` |

### 2.3 命名禁忌

- ❌ 驼峰：`cardTitle`
- ❌ 下划线：`card_title`
- ❌ 拼音 / 中文：`xiangpian`、`盒子`
- ❌ 无意义：`box1`、`aaa`、`tmp`

---

## 三、代码书写格式

由 Prettier 自动格式化，团队无需手动争论：

### 3.1 基本规则

- 选择器与 `{` 之间保留一个空格
- `{` 后换行，`}` 单独一行
- 属性：`属性名: 值;`，冒号后加一个空格，末尾必须分号
- 多选择器时，每个选择器独占一行
- 多属性时，**一行一个属性**
- 缩进：2 个空格（不使用 Tab）

### 3.2 示例

```css
/* ✅ 推荐 */
.card__title,
.card__desc {
  font-size: 16px;
  color: #333333;
  line-height: 1.5;
}

/* ❌ 禁止 */
.card__title,.card__desc{font-size:16px;color:#333}
```

---

## 四、属性书写顺序

按"布局 → 盒模型 → 视觉 → 文本 → 其他"排列，便于阅读：

| 顺序 | 类别 | 属性 |
|------|------|------|
| 1 | 定位 | `position`、`top`、`right`、`bottom`、`left`、`z-index` |
| 2 | 盒布局 | `display`、`flex` / `grid` 相关、`float`、`clear` |
| 3 | 盒模型 | `width`、`height`、`margin`、`padding`、`border`、`box-sizing` |
| 4 | 视觉 | `background`、`box-shadow`、`opacity` |
| 5 | 文本 | `font`、`line-height`、`color`、`text-align`、`white-space` |
| 6 | 其他 | `transition`、`transform`、`animation`、`cursor` |

```css
.box {
  position: relative;
  display: flex;
  width: 200px;
  margin: 0 auto;
  padding: 16px;
  border: 1px solid #eeeeee;
  background: #ffffff;
  font-size: 14px;
  color: #222222;
  transition: all 0.2s ease;
}
```

> 可使用 `stylelint-order` 插件在 CI 中自动校验顺序。

---

## 五、选择器规范

### 5.1 优先级原则

1. **优先使用 class 选择器**，少用或不用 ID 选择器
   - ID 权重过高，难以覆盖，复用性差
   - 仅锚点、原生行为（`#top`）可使用
2. **禁止全局标签选择器**：`div {}`、`p {}` 会污染全局
3. **避免深层嵌套**：后代选择器嵌套不超过 3 层
4. **慎用 `!important`**：仅作为最终兜底，不得用来强行覆盖样式

### 5.2 正反例

```css
/* ✅ 推荐：语义化 class */
.card__title { color: #333; }

/* ❌ 禁止：三层以上嵌套 + 标签选择器 */
.page .wrap .card .info h3 { color: #333; }

/* ❌ 禁止：ID 选择器写样式 */
#header { background: #fff; }
```

---

## 六、值与单位

### 6.1 数字与单位

| 规则 | 正确 | 错误 |
|------|------|------|
| 0 不带单位 | `margin: 0;` | `margin: 0px;` |
| 小数前导 0 保留 | `opacity: 0.5;` | `opacity: .5;` |
| 1 以上小数正常写 | `line-height: 1.5;` | `line-height: 1.5em;`（非必要不加 em） |

### 6.2 单位选择

| 场景 | 推荐单位 | 说明 |
|------|----------|------|
| 字号 | `rem`（移动端）/ `px`（固定小字号） | 配合根字号实现响应式 |
| 间距、宽高 | `px` / `rem` | 设计稿还原用 px，响应式用 rem |
| 容器宽高 | `%` / `vw` / `fr` | 布局场景 |
| 边框 | `px` | 1px 细线 |

### 6.3 时间

`transition`、`animation` 项目内统一使用秒（s）或毫秒（ms），不要混用：

```css
transition: opacity 0.2s ease;
animation: fade-in 300ms ease-out;
```

---

## 七、颜色与主题

### 7.1 颜色值

- 优先使用小写十六进制，可简写则简写：`#fff`、`#333`
- 透明度场景使用 `rgba()`
- 禁止零散硬编码颜色，**统一通过 CSS 变量管理**

```css
:root {
  /* 品牌色 */
  --color-primary: #1890ff;
  --color-success: #52c41a;
  --color-warning: #faad14;
  --color-danger:  #ff4d4f;

  /* 文本 */
  --color-text:        #333333;
  --color-text-secondary: #666666;
  --color-text-disabled:  #999999;

  /* 背景 / 边框 */
  --color-bg:       #ffffff;
  --color-bg-gray:  #f5f5f5;
  --color-border:   #e8e8e8;
}

/* 使用 */
.button {
  background: var(--color-primary);
  color: #fff;
}
```

### 7.2 间距与字号变量

```css
:root {
  /* 间距阶梯 */
  --space-xs: 4px;
  --space-sm: 8px;
  --space-md: 16px;
  --space-lg: 24px;
  --space-xl: 32px;

  /* 字号阶梯 */
  --font-size-sm:   12px;
  --font-size-base: 14px;
  --font-size-md:   16px;
  --font-size-lg:   20px;
  --font-size-xl:   24px;
}
```

---

## 八、响应式与媒体查询

### 8.1 移动优先（Mobile First）

先写移动端基础样式，再用 `min-width` 向上覆盖：

```css
.card {
  width: 100%;
  padding: var(--space-md);
}

@media (min-width: 768px) {
  .card {
    width: 50%;
    padding: var(--space-lg);
  }
}
```

### 8.2 断点约定

团队统一断点，不要随手写：

```css
/* 建议断点 */
/* <  640px  手机 */
/* >= 640px  大屏手机 */
/* >= 768px  平板 */
/* >= 1024px 桌面 */
/* >= 1280px 宽屏 */
```

### 8.3 媒体查询位置

- 就近写在对应组件样式下方，**不要全部堆在文件末尾**
- 一个组件内的媒体查询集中在一起，便于维护

---

## 九、注释规范

### 9.1 文件头注释

```css
/**
 * @desc 卡片组件样式
 * @author Zhang San
 * @date   2026-01-01
 */
```

### 9.2 块级注释

```css
/* ========== 按钮变体 ========== */
.button--primary { ... }
.button--danger   { ... }
```

### 9.3 行内注释

注释写在属性**上方**，避免行尾长注释：

```css
.card {
  /* 固定高度，配合文本截断 */
  height: 80px;
  overflow: hidden;
}
```

### 9.4 注释风格选择

| 风格 | 说明 |
|------|------|
| `/* ... */` | 会保留到生产 CSS，用于对外可读注释 |
| `// ...` | 需配合 PostCSS 构建，**不会打包到生产**，适合开发注释 |

---

## 十、性能与最佳实践

1. **动画只动 `transform` 和 `opacity`**，触发 GPU 合成层，避免重排
2. **避免昂贵属性大面积使用**：`box-shadow`、`border-radius`、`filter`
3. **不要用 `*` 通配符做大量重置**，精准 reset 即可
4. **优先 Flex / Grid 布局**，少用 float 做整体页面布局
5. **抽取公共样式**：相同的颜色、尺寸、间距用 CSS 变量或工具类
6. **避免重复选择器**：合并同选择器下的属性
7. **字体回退**：`font-family` 必须带系统字体回退栈

```css
font-family: -apple-system, BlinkMacSystemFont, "Segoe UI",
             "PingFang SC", "Microsoft YaHei", sans-serif;
```

---

## 十一、禁止清单

- ❌ 行内样式大量写在 HTML `style="..."`
- ❌ 选择器嵌套超过 3 层
- ❌ 滥用 `!important`
- ❌ 类名拼音、中文、无意义缩写
- ❌ 多个属性挤在同一行
- ❌ 到处硬编码颜色、尺寸（魔数 / Magic Number）
- ❌ 用 ID 选择器写组件样式
- ❌ 全局 `* { margin: 0; padding: 0; }`（请用 normalize）

---

## 十二、工程化配套

专业团队标配，把规范变成自动约束：

| 工具 | 作用 |
|------|------|
| **Prettier** | 自动格式化：空格、换行、引号、分号 |
| **Stylelint** | 静态检查：不符合规范直接报错 |
| **stylelint-order** | 强制属性书写顺序 |
| **autoprefixer** | 自动补浏览器前缀 |
| **cssnano** | 生产环境压缩、去重 |
| **husky + lint-staged** | git commit 前自动校验暂存文件 |

### 推荐 .stylelintrc.json 骨架

```json
{
  "extends": ["stylelint-config-standard", "stylelint-config-recess-order"],
  "rules": {
    "indentation": 2,
    "declaration-block-trailing-semicolon": "always",
    "selector-class-pattern": "^[a-z]([a-z0-9\\-]+)?(__[a-z0-9]+)?(--[a-z0-9]+)?$",
    "no-descending-specificity": null,
    "no-invalid-position-at-import-rule": null
  }
}
```

### 推荐 .prettierrc

```json
{
  "printWidth": 100,
  "singleQuote": true,
  "trailingComma": "es5",
  "tabWidth": 2
}
```

---

## 十三、完整示例

一个符合规范的组件样式文件长这样：

```css
/**
 * @desc 卡片组件样式
 * @author Zhang San
 */

/* ========== 基础卡片 ========== */
.card {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  padding: var(--space-md);
  border: 1px solid var(--color-border);
  border-radius: 8px;
  background: var(--color-bg);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.06);
  transition: box-shadow 0.2s ease;
}

.card:hover {
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.1);
}

/* ========== 子元素 ========== */
.card__img {
  width: 100%;
  height: 180px;
  object-fit: cover;
  border-radius: 4px;
}

.card__title {
  margin: var(--space-sm) 0;
  font-size: var(--font-size-md);
  font-weight: 600;
  color: var(--color-text);
}

.card__desc {
  margin: 0;
  font-size: var(--font-size-base);
  line-height: 1.6;
  color: var(--color-text-secondary);
}

/* ========== 修饰符 ========== */
.card--primary {
  border-color: var(--color-primary);
}

.card--disabled {
  opacity: 0.5;
  pointer-events: none;
}

/* ========== 响应式 ========== */
@media (min-width: 768px) {
  .card {
    flex-direction: row;
    padding: var(--space-lg);
  }

  .card__img {
    width: 240px;
    height: auto;
    margin-right: var(--space-md);
  }
}
```

---

