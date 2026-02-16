# IPMA 项目规则

## 项目概述

IPMA (IP Management Application) 是一个 IP 地址管理系统，使用 Rust (Actix-web) 后端和纯 HTML/CSS/JavaScript 前端。

## 技术栈

- **后端**: Rust + Actix-web + PostgreSQL
- **前端**: HTML5 + CSS3 + JavaScript (ES16+)
- **国际化**: i18next

## 代码风格

详细的前端代码风格规范请参阅: [frontend-style-guide.md](web/.trae/rules/frontend-style-guide.md)

### 核心原则

1. **模块化**: 前端使用 ES16 模块导入/导出，后端基于rust mod
2. **一致性**: 遵循统一的命名和格式规范
3. **可维护性**: 代码清晰、注释完整
4. **性能**: 避免重复初始化，使用事件委托

## 常用命令

### 构建和运行

```bash
# 开发模式运行
./pak.sh

# 生产模式构建
./pak.sh

# 运行测试
cargo clippy
```

### 前端开发

```bash
# 代码格式化 (需要安装 prettier)
npx prettier --write "web/static/**/*.{js,css,html}"

# 代码检查 (需要安装 eslint)
npx eslint "web/static/js/**/*.js"
```


## API 规范

### 响应格式

```json
{
  "success": true,
  "data": {},
  "message": "操作成功"
}
```

### 错误响应

```json
{
  "success": false,
  "message": "错误信息",
  "error_type": "error_code"
}
```

## 注意事项

1. **避免重复初始化**: 使用 `dataset` 属性标记初始化状态
2. **事件委托**: 在容器上使用事件委托，而不是为每个元素绑定事件
3. **CSS 变量**: 使用 CSS 变量而不是硬编码的颜色和尺寸
4. **国际化**: 所有用户可见文本都应使用 i18n
5. **错误处理**: 所有 API 调用都应有 try-catch 和错误提示
6. **数据库相关修改**: 数据库由初始化脚本创建，修复由数据库引起的问题时，先手动调整数据库验证修改，再同步修改初始化页操作数据库的代码