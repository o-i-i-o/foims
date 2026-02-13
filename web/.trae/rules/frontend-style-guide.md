# IPMA 前端代码风格规范

## 概述

本文档定义了 IPMA 项目前端代码的风格规范，确保代码一致性、可读性和可维护性。

---

## JavaScript 代码规范

### 1. 模块导入

```javascript
// 推荐：按类型分组导入
// 1. 外部库/框架
import { something } from "library";

// 2. 工具函数
import { apiGet, apiPost } from "../utils/apiClient.js";

// 3. UI 组件/函数
import { showToast, renderTable } from "../utils/ui.js";

// 4. 模块
import { openModal, closeModal } from "../utils/modal.js";
```

### 2. 命名规范

| 类型 | 命名风格 | 示例 |
|------|----------|------|
| 变量 | camelCase | `const userName = "test";` |
| 常量 | UPPER_SNAKE_CASE | `const MAX_RETRY_COUNT = 3;` |
| 函数 | camelCase | `function loadData() {}` |
| 类 | PascalCase | `class ApiClient {}` |
| 私有属性 | #前缀 | `#privateField` |
| 事件处理 | handle前缀 | `handleClick`, `handleSubmit` |
| 布尔变量 | is/has/can前缀 | `isLoading`, `hasError` |

### 3. 函数定义

```javascript
// 推荐：使用箭头函数或 function 声明
// 工具函数使用箭头函数
const formatDate = (date) => {
  return new Date(date).toLocaleDateString();
};

// 主要功能函数使用 function 声明
async function loadUserData(userId) {
  try {
    const result = await apiGet(`/api/users/${userId}`);
    return result.data;
  } catch (error) {
    console.error("加载用户数据失败:", error);
    return null;
  }
}

// 类方法
class UserManager {
  async getUser(id) {
    return await apiGet(`/api/users/${id}`);
  }
}
```

### 4. 异步处理

```javascript
// 推荐：使用 async/await
async function fetchData() {
  try {
    const result = await apiGet("/api/data");
    if (result.success) {
      processData(result.data);
    }
  } catch (error) {
    console.error("获取数据失败:", error);
    showToast("获取数据失败", "error");
  }
}

// 并行请求
async function loadAllData() {
  const [users, networks, rooms] = await Promise.all([
    apiGet("/api/users"),
    apiGet("/api/networks"),
    apiGet("/api/rooms")
  ]);
  return { users, networks, rooms };
}
```

### 5. 错误处理

```javascript
// 推荐：统一的错误处理模式
async function saveData(data) {
  try {
    const result = await apiPost("/api/data", data);
    if (result.success) {
      showToast("保存成功", "success");
      return result.data;
    } else {
      showToast(result.message || "保存失败", "error");
      return null;
    }
  } catch (error) {
    console.error("保存数据失败:", error);
    showToast("保存失败，请稍后重试", "error");
    return null;
  }
}
```

### 6. DOM 操作

```javascript
// 推荐：使用事件委托
document.getElementById("container").addEventListener("click", (e) => {
  const button = e.target.closest(".action-btn");
  if (!button) return;
  
  const id = button.dataset.id;
  handleAction(id);
});

// 推荐：使用可选链
const value = element?.value ?? "";

// 推荐：使用 dataset
const id = element.dataset.id;
```

### 7. 注释规范

```javascript
/**
 * 函数说明
 * @param {string} param1 - 参数1说明
 * @param {object} options - 选项对象
 * @param {number} options.timeout - 超时时间
 * @returns {Promise<Object>} 返回值说明
 */
async function functionName(param1, options = {}) {
  // 单行注释：简要说明
  
  /*
   * 多行注释：
   * 详细说明复杂逻辑
   */
}

// TODO: 待办事项
// FIXME: 需要修复的问题
// NOTE: 重要说明
```

---

## CSS 代码规范

### 1. 命名规范

```css
/* BEM 命名法 */
.block {}
.block__element {}
.block--modifier {}

/* 示例 */
.card {}
.card__header {}
.card__body {}
.card--featured {}

/* 状态类 */
.is-active {}
.is-loading {}
.has-error {}

/* 工具类 */
.text-center {}
.mt-1 {}
.d-flex {}
```

### 2. CSS 变量

```css
:root {
  /* 颜色 */
  --color-primary: #667eea;
  --color-primary-hover: #5568d3;
  
  /* 间距 */
  --space-xs: 0.25rem;
  --space-sm: 0.5rem;
  --space-md: 1rem;
  
  /* 圆角 */
  --radius-sm: 4px;
  --radius-md: 6px;
  
  /* 过渡 */
  --transition-fast: 0.15s ease;
}

/* 使用 */
.button {
  background-color: var(--color-primary);
  padding: var(--space-sm) var(--space-md);
  border-radius: var(--radius-md);
  transition: all var(--transition-fast);
}
```

### 3. 选择器

```css
/* 推荐：避免过深嵌套 */
/* 不好 */
.container .content .card .header .title {}

/* 好 */
.card__title {}

/* 推荐：使用 CSS 变量代替硬编码 */
/* 不好 */
.element {
  color: #667eea;
}

/* 好 */
.element {
  color: var(--color-primary);
}
```

### 4. 属性顺序

```css
.element {
  /* 1. 定位 */
  position: relative;
  top: 0;
  left: 0;
  z-index: 1;
  
  /* 2. 盒模型 */
  display: flex;
  flex-direction: column;
  width: 100%;
  height: auto;
  padding: 1rem;
  margin: 0;
  
  /* 3. 边框和背景 */
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background-color: var(--color-bg-white);
  
  /* 4. 文本 */
  color: var(--color-text);
  font-size: 0.9rem;
  line-height: 1.5;
  text-align: center;
  
  /* 5. 其他 */
  cursor: pointer;
  transition: all var(--transition-fast);
}
```

---

## HTML 代码规范

### 1. 文档结构

```html
<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>页面标题</title>
  <!-- 样式表 -->
  <link rel="stylesheet" href="/static/css/main.css">
</head>
<body>
  <!-- 主要内容 -->
  <main class="main-content">
    <!-- 内容区域 -->
  </main>
  
  <!-- 脚本 -->
  <script type="module" src="/static/js/app.js"></script>
</body>
</html>
```

### 2. 语义化标签

```html
<!-- 推荐：使用语义化标签 -->
<header class="header">
  <nav class="nav">
    <ul class="nav-list">
      <li class="nav-item"><a href="#" class="nav-link">首页</a></li>
    </ul>
  </nav>
</header>

<main class="main-content">
  <article class="card">
    <header class="card__header">
      <h2 class="card__title">标题</h2>
    </header>
    <section class="card__body">
      <!-- 内容 -->
    </section>
    <footer class="card__footer">
      <!-- 操作按钮 -->
    </footer>
  </article>
</main>

<footer class="footer">
  <!-- 页脚内容 -->
</footer>
```

### 3. 属性顺序

```html
<!-- 推荐：属性顺序 -->
<button
  id="submit-btn"
  class="btn btn-primary"
  type="submit"
  data-action="submit"
  aria-label="提交表单"
>
  提交
</button>
```

### 4. 可访问性

```html
<!-- 推荐：添加 ARIA 属性 -->
<button aria-label="关闭对话框" aria-expanded="false">
  <span aria-hidden="true">&times;</span>
</button>

<!-- 推荐：使用 sr-only 类隐藏视觉内容但保留屏幕阅读器可读 -->
<h2 class="sr-only">页面标题</h2>
```

---

## 文件组织

```
web/static/
├── css/
│   ├── base/           # 基础样式
│   ├── components/     # 组件样式
│   ├── layouts/        # 布局样式
│   ├── pages/          # 页面样式
│   ├── utilities/      # 工具类
│   ├── variables.css   # CSS 变量
│   └── main.css        # 主入口
├── js/
│   ├── modules/        # 功能模块
│   ├── utils/          # 工具函数
│   ├── lib/            # 第三方库
│   └── app.js          # 主入口
└── main.html           # 主页面
```

---

## 最佳实践

### 1. 防止重复初始化

```javascript
// 使用 dataset 标记初始化状态
function initModule() {
  const container = document.getElementById("container");
  if (container.dataset.initialized === "true") return;
  
  // 初始化逻辑...
  container.dataset.initialized = "true";
}
```

### 2. 事件委托

```javascript
// 推荐：在容器上使用事件委托
container.addEventListener("click", (e) => {
  const target = e.target.closest(".action-btn");
  if (!target) return;
  
  // 处理点击
});
```

### 3. 内存管理

```javascript
// 清理事件监听器
const controller = new AbortController();
element.addEventListener("click", handler, { signal: controller.signal });

// 清理时
controller.abort();
```

### 4. 性能优化

```javascript
// 使用防抖/节流
const debouncedSearch = debounce((query) => {
  searchAPI(query);
}, 300);

// 使用 requestAnimationFrame
function animate() {
  requestAnimationFrame(() => {
    // 动画逻辑
  });
}

// 使用 Intersection Observer
const observer = new IntersectionObserver((entries) => {
  entries.forEach(entry => {
    if (entry.isIntersecting) {
      // 元素进入视口
    }
  });
});
```
