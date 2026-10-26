# FOIMS 代码规范入口

**完整风格规范见 [docs/code-style.md](docs/code-style.md)**（前后端唯一权威来源，
本文件仅保留必须随时可见的硬性规则）。
你作为AI编程助手，需遵循“古法编程”核心原则：
1. 全局一致性优先：输出代码需保证功能、命名、逻辑、抽象的全局统一，禁止出现同一功能多实现、同一概念多命名、重复防御性处理、过度抽象的问题。
2. 可维护性优先：代码需符合人类程序员的阅读和维护习惯，每个模块、函数、变量的设计要贴合人类思维逻辑，禁止生成仅语法正确但难以理解的“完美但不可维护”的代码。
3. 业务逻辑优先：输出前先明确业务需求的核心目标，代码需围绕业务逻辑展开，禁止为了追求“完美语法”“过度设计”而偏离业务本身。
4. 可解释性优先：输出代码时需同步说明设计思路、核心逻辑、潜在风险，让人类能快速理解代码的来龙去脉，禁止输出无解释的“黑盒代码”。

## 基础规范

- 本项目基于 **Rust 2024 Edition**，风格由 rustfmt + clippy 强制落地
  （`[workspace.lints]` 已启用 unwrap/expect/print/allow 等 clippy 护栏，
  并设 `unsafe_code = "forbid"`）。
- 符合 rust、axum、api、http、pgsql 最佳实践；不考虑老旧基础设施兼容性。
- 本项目 IPv6 支持友好。
- 不使用数据库迁移代码：修改结构时直接执行 SQL，然后同步完善 foims-init 的
  建表代码与 `check.rs` 校验清单。
  - 自版本cargo.toml版本0.24.0起，需要考虑版本升级问题，每次涉及数据库的修改仍然需要同步完善 `foims-init` 的建表代码与 `check.rs` 校验清单以确保全新部署的代码的干净，另外需要基于当前 中(0.23.0的23) 版本（举例 ”强调，从0.24.0版本开始" 0.23.x 后所有版本都基于0.23.0版本数据库生成数据库迁移代码sql，0.24.x “x>0" 版本基于0.24.0版本数据库生成数据库迁移代码sql，0.24.0版本基于0.23.0版本数据库生成数据库迁移代码sql，产物放在upgrade目录，每个中版本仅保留一份最新版本升级代码，如upgrade/0.23/保存一份最新的0.23.x的sql和脚本 upgrade/0.24/文件夹保留）生成对应的数据库迁移代码和脚本做为升级方法。
- 前后端分离，nginx 托管静态资源、代理 API。前端为 Vue 3 + Vite 工程
  （仓库根 `frontend/`，MPA 三入口 index/init_index/main），产物经
  `vite build` 部署到 `/usr/share/foims/web`（dist/ 内容直接铺开）；旧版原生 JS 前端已存档于
  `web-archive/`（只读参考，不再维护，禁止在其上继续开发）。

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
  仅当该文件 y/x 段变化时才联动 bump（z 段普通修改不动 Cargo.toml）。
  前端改用 Vue 3 工程后版本由 frontend/package.json 的 version 承载，
  复用同一套 x/y/z 判级机制与 Cargo.toml 独立联动（仅前端修改不再 bump
  Cargo.toml）；Vite 内容指纹（assets/ 哈希文件名）仅承担静态资源缓存，
  入口 HTML 引用变化随构建自动生效。
- 每次更新后：`cargo fmt && cargo clippy --release -- -D warnings`；
  前端改动后：`cd frontend && npm run typecheck`（vue-tsc，开发循环可单跑）
  与 `npm run build`；后端载荷类型变更后运行
  `bash da-scripts/gen-ts-types.sh` 再生成 TS 类型（生成物提交入库，
  CI 漂移校验，禁止手改 `frontend/src/types/api/generated/`）。
  审计用独立二进制 `biome check frontend`（CSS+格式）与 `oxlint`（JS 质量）；
  可视化纯函数用 `cd frontend && npm run selfcheck:viz`（geometry/viewmodel 自检）。
- 前端 SFC 一律 `<script setup lang="ts">`；新增前端代码一律 TypeScript，
  禁止 any（用 unknown + 收窄）；API 载荷类型优先取
  `frontend/src/types/api/generated/` 生成物，手写类型只放前端自有契约。
- 开发直接测试：`./da-scripts/pak.sh`；
- 测试用户 admin / admin123。
