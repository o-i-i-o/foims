# FOIMS 代码规范入口

**完整风格规范见 [docs/code-style.md](docs/code-style.md)**（前后端唯一权威来源，
本文件仅保留必须随时可见的硬性规则）。`.trae/rules/` 下的指引亦以该文档为准。

## 基础规范

- 本项目基于 **Rust 2024 Edition**，风格由 rustfmt + clippy 强制落地
  （`[workspace.lints.clippy]` 已启用 unwrap/expect/print/allow 等护栏）。
- 符合 rust、axum、api、http、pgsql 最佳实践；不考虑老旧基础设施兼容性。
- 本项目 IPv6 支持友好。
- 不使用数据库迁移代码：修改结构时直接执行 SQL，然后同步完善 foims-init 的
  建表代码与 `check.rs` 校验清单。
- 前后端分离，nginx 托管静态资源、代理 API。

## 硬性规则

- 编码统一 UTF-8；注释统一中文。
- 禁止 `unwrap()`/`expect()`/`unreachable!()`/`#[allow(...)]`（测试代码除外）；
  禁止用 `_` 忽略参数。唯一例外：仅取鉴权副作用的提取器参数
  `_admin: AdminUser` / `_user: CurrentUser`。
- 禁止硬编码密钥/密码；所有用户输入必须验证；所有可能失败的操作必须有错误处理。
- 后端日志统一 `tracing`，禁止 `println!`；前端通知统一 `showToast`，禁止 `alert()`。
- 动态 SQL 用 `sqlx::QueryBuilder`；跨 crate 共享类型放 `foims-common`，不得复制副本。
- 分页响应用 `paged_response`（键固定 `items/total/page/page_size/total_pages`）。

## 版本与测试

- 每次代码更新后 bump `Cargo.toml` 版本号（规则 `0.x.yy`，`yy>=99` 时进位）；
  前端资源同步 bump `main.html` 的 `?v=` 与 `resourceLoader.js` 的 `MODULE_VERSION`。
- 每次更新后：`cargo fmt && cargo clippy --release -- -D warnings`。
- 测试：`cargo build --release && sudo systemctl restart foims`
  （运行需 root 监听端口）；测试用户 admin / admin123。
