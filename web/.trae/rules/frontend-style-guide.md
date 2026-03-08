# IPMA 前端代码风格规范（ES2025版）

## 概述

本文档定义了 IPMA 项目前端代码的完整风格规范，基于 **HTML5** 和 **ECMAScript 2025 (ES2025)** 标准，确保代码一致性、可读性、可维护性和高性能。本规范通过 ESLint/Prettier 等工具强制落地。

---

## 一、通用规范

### 1. 编码与文件

- **编码统一：** 所有文件必须使用 **UTF-8** 编码，`<meta charset="UTF-8">` 必须放在 HTML 的 `<head>` 首位。

- **文件命名：**

    - HTML/Vue/React 组件：小写字母 + 连字符（**kebab-case**），如 `user-profile.html`、`shopping-cart.vue`。

    - JavaScript/TypeScript 文件：小写字母 + 连字符（**kebab-case**），如 `api-service.js`、`date-utils.ts`。

    - 样式文件：遵循框架规范（Vue 单文件组件内联或 kebab-case CSS 文件）。

- **版本控制：** 必须通过 `.gitignore` 过滤 `node_modules/`、`dist/`、`.env` 等无关文件。

### 2. 可访问性（A11y）

- **语义化：** 优先使用语义化 HTML 标签。

- **键盘操作：**所有交互元素必须可通过键盘访问（`tabindex` 合理使用）。

- **ARIA 标签：** 必要时添加 `aria-*` 属性增强屏幕阅读器支持。

- **颜色对比度：** 遵循 WCAG 2.1 AA 级别标准（对比度 ≥ 4.5:1）。

---

## 二、HTML 代码风格规范（HTML5）

### 1. 基础语法与结构

#### 1.1 文档声明

```html
<!DOCTYPE html>  <!-- 必须使用 HTML5 简洁声明 -->
<html lang="zh-CN">  <!-- 必须指定语言 -->
```

#### 1.2 标签与属性

标签名、属性名： 统一使用小写。

属性值： 必须使用双引号包裹，禁止单引号或无引号。

```html
<!-- ✅ 正确 -->
<input type="text" class="form-control">

<!-- ❌ 错误 -->
<input type='text' class=form-control>
```

布尔属性： 无需赋值，存在即表示 true。

```html
<!-- ✅ 正确 -->
<input type="checkbox" checked disabled>

<!-- ❌ 错误 -->
<input type="checkbox" checked="true" disabled="false">
```

自闭合标签： 无需添加 /（HTML5 规范）。

```html
<!-- ✅ 正确 -->
<img src="logo.png" alt="Logo">
<input type="text">

<!-- ❌ 错误（XHTML 风格） -->
<img src="logo.png" alt="Logo" />
```

#### 1.3 语义化标签

优先使用语义化标签： `<header>`、`<nav>`、`<main>`、`<article>`、`<section>`、`<aside>`、`<footer>`。

禁止滥用 `<div>`： 仅当无语义合适标签时使用。

禁止用 `<br>` 实现布局： 仅用于文本内的换行。

表单必须绑定 `<label>`：

```html
<!-- ✅ 正确：提升可访问性 -->
<label for="username">用户名：</label>
<input type="text" id="username">

<!-- ✅ 正确（隐式关联） -->
<label>
  用户名：
  <input type="text">
</label>
```

### 2. 格式与可读性

#### 2.1 缩进与换行

缩进： 统一使用 2 个空格，禁止使用 Tab。

换行规则：

块级元素（`<div>`、`<section>`、`<p>` 等）必须换行，子元素缩进一级。

内联元素（`<span>`、`<a>`、`<strong>` 等）按需换行，避免过度换行影响可读性。

```html
<!-- ✅ 正确 -->
<div class="container">
  <header class="header">
    <h1>标题</h1>
    <nav>
      <a href="/">首页</a>
      <a href="/about">关于</a>
    </nav>
  </header>
  <main>
    <p>内容段落内容段落</p>
  </main>
</div>
```

#### 2.2 注释规范

格式： `<!-- 注释内容 -->`，禁止使用 // 或 /* */。

原则： 说明“为什么”而非“是什么”。

场景： 复杂模块、临时方案、需要特别说明的逻辑必须加注释。

```html
<!-- 临时隐藏支付方式，待后端接口联调后开放 -->
<div class="payment-method" style="display: none">
  ...
</div>
```

## 三、JavaScript 代码风格规范（ES2025+）

### 1. 基础语法与命名

#### 1.1 变量与常量声明

优先使用 const： 所有不可变变量必须使用 const。

使用 let 声明可变变量： 禁止使用 var（避免变量提升和作用域污染）。

变量命名： 小驼峰（camelCase），前缀体现语义。

- 布尔值：is/has/should 开头，如 isValid、hasPermission。

- 数值/字符串：名词，如 userName、maxCount。

- DOM 元素：el 前缀，如 elModal、elSubmitBtn。

常量命名： 全大写 + 下划线（UPPER_SNAKE_CASE）。

```javascript
// ✅ 正确
const MAX_RETRY_COUNT = 3;
const API_BASE_URL = 'https://api.example.com';

// ❌ 错误
const maxRetryCount = 3;  // 应视为变量，但实际为常量
```

禁止单字符变量名： 除循环索引 i/j/k 和极简回调参数外。

#### 1.2 函数与类命名

函数命名： 小驼峰，动词开头。

- 获取数据：get/fetch 开头，如 getUserInfo、fetchOrderList。

- 设置数据：set/update 开头，如 setTheme、updateProfile。

- 事件处理：handle 开头，如 handleClick、handleInputChange。

- 判断函数：is/has/can 开头，如 isValidEmail、hasPermission。

类/构造函数命名： 大驼峰（PascalCase）。

```javascript
// ✅ 正确
class UserService { ... }
class OrderController { ... }
```

#### 1.3 模块命名

ES2025 模块导入/导出： 统一使用 ES Module 语法，禁止混用 require()。

导出原则：

- 主组件/类：使用 export default。

- 工具函数/常量：使用命名导出 export const。

```javascript
// ✅ 正确
// user.service.js
export class UserService { ... }
export const DEFAULT_AVATAR = 'default.png';

// main.js
import { UserService, DEFAULT_AVATAR } from './user.service';
import App from './App.vue';  // 主组件默认导出
```

### 2. 格式与注释

#### 2.1 缩进与分号

缩进： 2 个空格。

分号： 必须使用分号结尾（避免 ASI 自动插入带来的隐患）。

```javascript
// ✅ 正确
const name = 'Tom';
console.log(name);

// ❌ 错误
const name = 'Tom'
console.log(name)  // 无分号
```

#### 2.2 注释规范

单行注释： // 注释内容，放在代码上一行，与代码保持相同缩进。禁止行尾注释（除非极简单逻辑）。

```javascript
// 计算总价（包含税费）
const total = price * quantity * (1 + taxRate);
```

多行注释（JSDoc）： 函数/类/复杂逻辑必须使用 JSDoc。

```javascript
/**
 * 格式化日期时间
 * @param {Date|string} date - 日期对象或日期字符串
 * @param {string} format - 格式模板，如 'YYYY-MM-DD'
 * @returns {string} 格式化后的日期字符串
 * @throws {Error} 当日期无效时抛出异常
 */
function formatDate(date, format) {
  // 实现逻辑
}
```

### 3. 现代语法特性（ES2025+）

#### 3.1 变量与解构

对象解构： 优先使用解构赋值获取对象属性。

```javascript
// ✅ 正确
const { name, age, address: { city } } = user;

// ❌ 错误
const name = user.name;
const age = user.age;
```

数组解构： 优先使用解构赋值。

```javascript
// ✅ 正确
const [first, second, ...rest] = array;

// ❌ 错误
const first = array[0];
```

对象简写： 属性名与变量名相同时必须使用简写。

```javascript
// ✅ 正确
const name = 'Tom';
const obj = { name, age: 18 };

// ❌ 错误
const obj = { name: name, age: 18 };
```

#### 3.2 函数与参数

默认参数： 使用函数默认参数，避免在函数体内判断。

```javascript
// ✅ 正确
function fetchData(url, timeout = 5000, retries = 3) {
  // 实现逻辑
}

// ❌ 错误
function fetchData(url, timeout, retries) {
  timeout = timeout || 5000;  // 无法处理 timeout = 0 的场景
  retries = retries ?? 3;     // 勉强可用，但不如默认参数清晰
}
```

剩余参数： 使用 ...rest 代替 arguments。

```javascript
// ✅ 正确
function sum(name, ...numbers) {
  return numbers.reduce((acc, cur) => acc + cur, 0);
}

// ❌ 错误
function sum() {
  const numbers = Array.from(arguments);  // 丢失了参数名语义
}
```

参数数量： 不超过 4 个，超过则使用对象传参。

```javascript
// ✅ 正确
function createUser({ name, email, age, address, phone }) {
  // 解构参数，顺序无关
}

// ❌ 错误
function createUser(name, email, age, address, phone, isAdmin) {  // 6个参数
}
```

#### 3.3 现代操作符

可选链（Optional Chaining）?.： 强制使用，避免冗长的空判断。

```javascript
// ✅ 正确
const city = user?.address?.city;
const firstHobby = user?.hobbies?.[0];
const element = document.querySelector('.btn')?.textContent;

// ❌ 错误
const city = user && user.address && user.address.city;  // 冗长
```

空值合并（Nullish Coalescing）??： 用于处理 null 或 undefined 的默认值。

```javascript
// ✅ 正确
const count = inputCount ?? 0;  // 仅当 inputCount 为 null/undefined 时使用 0
const name = userName ?? 'Guest';  // 允许空字符串 '' 作为有效值

// ❌ 错误
const count = inputCount || 0;  // 会将 0、''、false 也转为默认值
```

逻辑赋值运算符： 合理使用 &&=、||=、??= 简化代码。

```javascript
// ✅ 正确
let user = { name: 'Tom' };
user.isAdmin ??= false;  // 仅当 isAdmin 不存在时赋值

// 等价于
if (user.isAdmin === null || user.isAdmin === undefined) {
  user.isAdmin = false;
}
```

### 4. 类型与判断

#### 4.1 严格相等

必须使用 === / !==： 禁止使用 == / !=（避免隐式类型转换）。

```javascript
// ✅ 正确
if (value === null) { ... }
if (count !== 0) { ... }

// ❌ 错误
if (value == null) { ... }  // 可能匹配 undefined
```

#### 4.2 空值判断

if (!variable) 适用场景： 仅用于布尔值、空字符串 ''、数字 0、null、undefined。

复杂类型显式判断：

```javascript
// ✅ 正确
if (array && array.length > 0) { ... }
if (object && Object.keys(object).length > 0) { ... }

// ❌ 错误
if (array) { ... }  // 空数组 [] 为 true，不会进入 else
```

#### 4.3 数组与对象初始化

优先使用字面量： 使用 [] / {}，禁止使用 new Array() / new Object()。

```javascript
// ✅ 正确
const list = [];
const config = {};

// ❌ 错误
const list = new Array();
const config = new Object();
```

### 5. 函数与异步编程

#### 5.1 函数定义

箭头函数 vs 普通函数：

- 回调函数/简单函数：优先使用箭头函数。

- 需要动态 this 的方法：使用普通函数。

```javascript
// ✅ 正确（回调）
['a', 'b'].map(item => item.toUpperCase());

// ✅ 正确（需要 this）
const obj = {
  name: 'obj',
  getName: function() { return this.name; }
};
```

立即执行函数（IIFE）： 使用箭头函数包裹。

```javascript
(() => {
  // 初始化逻辑
})();
```

#### 5.2 异步编程

优先使用 async/await： 禁止嵌套 Promise.then（避免回调地狱）。

```javascript
// ✅ 正确
async function fetchUserData(userId) {
  try {
    const user = await api.getUser(userId);
    const orders = await api.getOrders(user.id);
    return { user, orders };
  } catch (error) {
    console.error('获取用户数据失败:', error);
    throw error;
  }
}

// ❌ 错误
function fetchUserData(userId) {
  return api.getUser(userId)
    .then(user => {
      return api.getOrders(user.id)
        .then(orders => ({ user, orders }));
    })
    .catch(error => {
      console.error(error);
    });
}
```

错误处理：

- async/await：必须使用 try/catch 包裹。

- Promise：必须链式调用 .catch()。

顶层 await（ES2022）： 允许在模块顶层使用，需确保项目环境支持。

```javascript
// ✅ 正确（顶层 await）
const data = await fetchData();
export const processedData = processData(data);
```

Promise.finally（ES2018）： 用于清理操作（如关闭 loading）。

```javascript
// ✅ 正确
showLoading();
try {
  await saveData();
} catch (error) {
  showError(error);
} finally {
  hideLoading();  // 无论成功失败都执行
}
```

### 6. 数据结构与遍历

#### 6.1 数组方法

优先使用现代数组方法：

- map：转换数组。

- filter：过滤数组。

- reduce：归约/聚合。

- find / findIndex：查找元素/索引（ES2015）。

- some / every：判断是否存在/全部满足。

- includes：判断是否包含（ES2016）。

```javascript
// ✅ 正确
const adults = users.filter(user => user.age >= 18);
const firstAdult = users.find(user => user.age >= 18);
const hasAdmin = users.some(user => user.role === 'admin');
const allAdults = users.every(user => user.age >= 18);
const names = users.map(user => user.name);
const totalAge = users.reduce((sum, user) => sum + user.age, 0);
```

遍历性能： 简单遍历用 for，复杂逻辑用数组方法（可读性优先）。

#### 6.2 对象遍历

使用 Object.keys() / Object.values() / Object.entries()：

```javascript
// ✅ 正确
Object.entries(config).forEach(([key, value]) => {
  console.log(`${key}: ${value}`);
});

// ❌ 错误
for (const key in config) {  // 可能遍历原型链属性
  if (config.hasOwnProperty(key)) { ... }
}
```

#### 6.3 Set / Map

优先使用 Set/Map 处理唯一值/键值对集合：

```javascript
// ✅ 正确
const uniqueIds = new Set([1, 2, 3, 2, 1]);  // Set(3) {1, 2, 3}
const userMap = new Map();
userMap.set(user.id, user);
```

### 7. 性能与兼容性

#### 7.1 禁止的操作

- 禁止修改原生对象原型： 如 Array.prototype.push = ...。

- 禁止使用 eval()： 存在安全风险。

#### 7.2 DOM 操作优化

避免频繁 DOM 操作： 批量操作时，先隐藏元素、操作完成后显示。

使用 DocumentFragment： 批量添加子元素时。

```javascript
// ✅ 正确
const fragment = document.createDocumentFragment();
items.forEach(item => {
  const li = document.createElement('li');
  li.textContent = item;
  fragment.appendChild(li);
});
listElement.appendChild(fragment);  // 一次重绘
```

#### 7.3 防抖与节流

高频事件： 必须使用防抖（debounce）或节流（throttle）。

- 输入搜索：防抖。

- 滚动/窗口调整：节流。

## 四、TypeScript 补充规范（如使用）

若项目使用 TypeScript，补充以下规范：

### 1. 类型定义

- 优先使用 interface 定义对象类型，type 定义联合类型/工具类型。

- 禁止使用 any： 必须使用 unknown 并配合类型守卫。

- 函数返回值必须显式声明类型。

```typescript
// ✅ 正确
interface User {
  id: number;
  name: string;
  email?: string;  // 可选属性
}

type Status = 'idle' | 'loading' | 'success' | 'error';

function fetchUser(id: number): Promise<User> {
  // 实现
}
```

### 2. 类型守卫

使用类型守卫收窄类型：

```typescript
function isUser(obj: unknown): obj is User {
  return typeof obj === 'object' && obj !== null && 'id' in obj;
}
```

## 五、工具配置

### 1. ESLint 配置（.eslintrc.js）

```javascript
module.exports = {
  env: {
    browser: true,
    es2025: true,  // 或 es2022: true
    node: true,
  },
  extends: ['eslint:recommended'],
  parserOptions: {
    ecmaVersion: 'latest',
    sourceType: 'module',
  },
  rules: {
    // 变量声明
    'no-var': 'error',
    'prefer-const': 'error',
    
    // 现代语法
    'prefer-object-spread': 'error',
    'prefer-rest-params': 'error',
    
    // 最佳实践
    'eqeqeq': ['error', 'always'],
    'no-eval': 'error',
    'no-implied-eval': 'error',
    'no-extend-native': 'error',
    
    // 风格
    'indent': ['error', 2],
    'quotes': ['error', 'single'],
    'semi': ['error', 'always'],
    'comma-dangle': ['error', 'always-multiline'],
  },
};
```

### 2. Prettier 配置（.prettierrc）

```json
{
  "printWidth": 100,
  "tabWidth": 2,
  "useTabs": false,
  "semi": true,
  "singleQuote": true,
  "trailingComma": "es5",
  "bracketSpacing": true,
  "arrowParens": "always"
}
```

## 六、团队协作

### 1. 代码审查

- 必须通过 ESLint/Prettier 检查 方可提交 PR。

- PR 描述必须清晰说明改动内容和影响范围。

### 2. Git 提交规范

提交信息格式： <type>(<scope>): <subject>

- type: feat(新功能)、fix(修复)、docs(文档)、style(格式)、refactor(重构)、perf(性能)、test(测试)、chore(构建/工具)

- scope: 影响范围（可选）

- subject: 简短描述，动词开头，不超过 50 字符

```bash
# ✅ 正确
feat(user): add login with Google
fix(cart): resolve total price calculation error
docs(readme): update setup instructions

# ❌ 错误
updated code
fix bug
```

### 3. 规范落地

- IDE 配置： 统一配置保存时自动格式化（Format on Save）。

- CI/CD： 在 CI 流程中集成 ESLint 检查，失败则阻断合并。

文档版本： 1.0.0

最后更新： 2026年3月