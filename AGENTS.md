# FOIMS 代码规范入口

**完整风格规范见 [docs/code-style.md](docs/code-style.md)**（前后端唯一权威来源，
本文件仅保留必须随时可见的硬性规则）。

## 基础规范

- 本项目基于 **Rust 2024 Edition**，风格由 rustfmt + clippy 强制落地
  （`[workspace.lints]` 已启用 unwrap/expect/print/allow 等 clippy 护栏，
  并设 `unsafe_code = "forbid"`）。
- 符合 rust、axum、api、http、pgsql 最佳实践；不考虑老旧基础设施兼容性。
- 本项目 IPv6 支持友好。
- 不使用数据库迁移代码：修改结构时直接执行 SQL，然后同步完善 foims-init 的
  建表代码与 `check.rs` 校验清单。
  - 自版本cargo.toml版本0.24.0起，需要考虑版本升级问题，每次涉及数据库的修改仍然需要同步完善 `foims-init` 的建表代码与 `check.rs` 校验清单以确保全新部署的代码的干净，另外需要基于当前 中(0.23.0的23) 版本（举例 ”强调，从0.24.0版本开始" 0.23.x 后所有版本都基于0.23.0版本数据库生成数据库迁移代码sql，0.24.x “x>0" 版本基于0.24.0版本数据库生成数据库迁移代码sql，0.24.0版本基于0.23.0版本数据库生成数据库迁移代码sql，产物放在upgrade目录，每个中版本仅保留一份最新版本升级代码，如upgrade/0.23/保存一份最新的0.23.x的sql和脚本 upgrade/0.24/文件夹保留）生成对应的数据库迁移代码和脚本做为升级方法。
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
- 业务代码每一个回退逻辑（保底逻辑）都需要评估，禁止惯性通过回退逻辑保留无效或没必要的旧代码。

## 版本与测试

- 每次代码更新后按照`docs/dev_version.md`规则更新版本记录；Cargo.toml
  仅当该文件 y/x 段变化时才联动 bump（z 段普通修改不动 Cargo.toml）；
  前端资源同步 bump 三个入口页（main/index/init_index）的 `?v=` 与
  `resourceLoader.js` 的 `MODULE_VERSION`（时间戳格式，一键脚本
  `bash da-scripts/bump_static_version.sh`；一致性由 `cargo test` 强制）。
- 每次更新后：`cargo fmt && cargo clippy --release -- -D warnings`。
- 开发直接测试：`./da-scripts/pak.sh`；
- 测试用户 admin / admin123。
