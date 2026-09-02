# IPMA 前后端代码组织结构审计报告

> 📌 说明：本报告撰写于项目由 IPMA 更名为 FOIMS 之前，文中的 IPMA/ipma-* 等名称均为当时的历史命名，现对应 FOIMS/foims-*，文件路径类同。报告内容作为时点审计记录原样保留。

- **审计日期**:2026-08-29
- **审计基线**:master @ 9202035(工作区干净)
- **审计范围**:后端根 crate `ipma`(0.15.93)+ `crates/` 下 10 个 workspace 成员(158 个 .rs 文件 / 46,711 行);前端 `web/`(59 个 JS 约 24,161 行、30 个 CSS 8,379 行、41 个模态 HTML、zh/en 各 2,125 个 i18n 键)
- **对照依据**:`AGENTS.md`、`docs/code-style.md`

**总体结论**:这是一套组织纪律相当好的代码库——后端 10 个 crate 依赖方向零倒挂、规范靠 clippy/CI 机制落地而非自觉;前端无构建原生 ESM 却做到零内联脚本、零 alert、零全局污染。真正的债务集中在三处:**两个跨 crate 复制类型(AppJson / DatabaseConfig)直接违反自家"不得复制副本"的硬性规范**;**后端 30 个 API 消息键在前端翻译目录中缺失**;**前端 systemManager.js(1,468 行)与后端已完成的 crate 化拆分不对齐**。

---

## 一、后端(Rust workspace)

### 1.1 结构与依赖

主程序 `ipma`(0.15.93)+ 10 个 crate,依赖方向全部正确,无循环、无底层依赖上层:

```
ipma(主程序) ──→ 全部 10 crate
  ├─ ipma-resource / organization / auth / visualization → common, models, auth*
  ├─ ipma-scheduler → common, data-management
  ├─ ipma-models → common, scheduler ← 唯一别扭的边
  └─ ipma-init / x509-manager / data-management → common
```

| crate | 版本 | 职责 | 文件数 | 行数 |
|---|---|---|---|---|
| ipma-common | 0.3.0 | 响应/错误/配置/加密/连接池/限流 | 15 | 4,190 |
| ipma-models | 0.2.1 | 全业务域请求/响应/行模型(唯一副本) | 17 | 5,243 |
| ipma-auth | 0.1.0 | 登录/JWT/2FA/LDAP/SSO/fail2ban/SMTP/操作日志 | 13 | 6,230 |
| ipma-resource | 0.1.0 | 网络/房间/机柜/工位/设备/IP/链路 | 21 | 11,303 |
| ipma-organization | 0.1.0 | 组织树/员工/组织模板 | 3 | 2,274 |
| ipma-visualization | 0.2.1 | 机房布局与拓扑计算 | 4 | 1,902 |
| ipma-data-management | 0.2.0 | CSV 导入导出/日志清理/数据库备份 | 11 | 3,384 |
| ipma-scheduler | 0.1.0 | tokio-cron-scheduler 调度基础设施 | 7 | 1,080 |
| ipma-init | 0.1.0 | 建库建表/结构校验/备份恢复/初始化向导 | 43 | 4,538 |
| ipma-x509-manager | 0.2.0 | X.509 证书(纯库,无 HTTP) | 7 | 1,855 |

主程序 `src/` 只剩组装层:

| 路径 | 文件 | 行数 | 说明 |
|---|---|---|---|
| `src/main.rs` | 1 | 693 | 入口:配置/CORS/UDS/中间件/优雅退出;161 条路由(init 14 + routes/mod.rs 147) |
| `src/routes/` | 2 | 893 | mod.rs(821 行)为纯路由注册中枢,按公开认证/CA/me/受保护分组注释分节 |
| `src/system/` | 5 | 2,039 | config.rs(1,099)/scheduled_task.rs/certificate.rs/task_executors.rs |
| `src/log/` | 5 | 850 | 多语言日志订阅器 + 转发/操作/通知/登录四类日志 |
| `src/utils/` | 1 | 15 | 纯 re-export 兼容转发层 |
| `src/i18n/` | 2 | 588 | zh.yml/en.yml(rust-i18n,仅后端日志文案) |

system/log 留守主程序是提交 66c06f4 明示的决策("依赖全部业务 crate,继续拆分无收益"),验证属实——它们天然是最上层组装点,非失控。

### 1.2 问题清单

#### [严重] `AppJson` 主程序完整功能副本

`src/routes/static_files.rs:57-92` 重新定义了 `AppJson<T>` 提取器与 `map_json_rejection`,而 `crates/ipma-common/src/json.rs:28` 已有同名同功能实现并对外导出(lib.rs 还有 `pub use json::AppJson`)。主程序 5 个文件全部 import 副本而非 common 的实现:

- `src/routes/mod.rs:25`
- `src/system/config.rs:17`
- `src/system/certificate.rs:28`
- `src/system/scheduled_task.rs:18`
- `src/log/forwarding.rs:17`

顺带:该类型放在名为 "static_files" 的文件里,模块名与内容不符。违反 docs/code-style.md:37-38"不得在多个 crate 各存一份副本"。

#### [严重] `DatabaseConfig` 三份定义,init 版逐字段复制

三个 crate 各有一个 `pub struct DatabaseConfig`:

- `crates/ipma-common/src/config.rs:33-50`(13 字段,含连接池参数)
- `crates/ipma-init/src/types.rs:68-85`(**13 字段与 common 逐字段完全相同,连 `default_max_connections` 等 7 个默认值函数也整段复制**,types.rs:87 起)
- `crates/ipma-data-management/src/types.rs:97-104`(5 字段子集)

连带代价:`src/main.rs:274-288` 手工逐字段搬运 common→init 的 13 个字段;`src/main.rs:454-460` 与 `src/app_state.rs:72-80` 再各搬运一次到 data-management 的 5 字段版。任何字段演进需改三处。违反"不得复制副本"规范。

(注:`ApiResponse`、`LayoutSaveRequest` 等其余共享类型已正确收敛——ApiResponse 由 ipma-models 再导出而非复制,LayoutSaveRequest 副本已在提交 2128a14 消除。)

#### [中等] models → scheduler 依赖倒挂 + scheduler 无谓拖入 data-management

- `crates/ipma-models/src/models/log.rs:28` 仅为一个 `pub use ipma_scheduler::{ScheduledTask, TaskLog};` 让领域模型底座依赖调度基础设施,形成 models → scheduler → data-management 长链。
- `crates/ipma-scheduler/src/models.rs:34` 又仅为 `pub use ipma_data_management::DatabaseConfig;` 传递依赖整个 CSV 导入导出/备份代码。

#### [中等] "动态 SQL 用 QueryBuilder"规范与实践大面积不符

QueryBuilder 仅 9 个文件使用;`format!` + `sqlx::AssertSqlSafe` 拼 SQL 共 **54 处、分布 22 个文件**,典型:

- `crates/ipma-resource/src/network.rs:85-267`(手工维护 `${param_count}` 占位符序号)
- `crates/ipma-resource/src/device/crud.rs:66-110`(`format!("WHERE {}", where_parts.join(" AND "))`)
- `crates/ipma-resource/src/cabinets.rs:62,77,96,115`(ORDER BY 白名单后插值)
- `src/log/notification.rs:52-71`(LIMIT/OFFSET 直接内插)
- 另有 `src/log/login.rs`、`src/system/scheduled_task.rs`、`crates/ipma-auth/src/user.rs` 等。

抽查确认排序/表名均为白名单或内部常量、值仍走 `.bind()`,**无注入风险**,但与 AGENTS.md 硬性规范直接冲突(规范未给 AssertSqlSafe 留例外)。ipma-init 的 DDL 拼接与 data-management 的表名拼接属标识符场景,合理。

#### [中等] crate 内分层三种风格并存

1. handler + 业务/数据层分离:仅 ipma-visualization(`http.rs` 面向 `P: DbProvider` 泛型,业务与 SQL 在 layout.rs/topology.rs)。
2. 纯库 + 主程序 handler:仅 ipma-x509-manager(HTTP 与操作日志由主程序 `src/system/certificate.rs` 负责)。
3. handler 内联 SQL 无数据层:大多数——ipma-resource、ipma-organization、ipma-auth、`src/log/*`(如 `crates/ipma-organization/src/lib.rs` 1,168 行 handler、树构建、SQL 同文件)。

docs/code-style.md 只约定了 trait 依赖倒置,未约定 crate 内分层模板,导致风格漂移。

#### [中等] 测试组织:后端 API 零自动化断言

- 亮点:59 个源文件带 `#[cfg(test)] mod tests`(models 全部子模块、common 11 个模块均有)。
- 根 `tests/` 仅 1 个文件 `frontend_consistency.rs`(333 行),内容是**前端资源一致性校验**——意味着后端 HTTP/API 层没有任何可 CI 化的 Rust 集成测试。
- `test/` 下 7 个手写 shell 脚本(test_all_api.sh 等),与 AGENTS.md 手工流程一致,不可 CI 化。
- 各 crate 无 `tests/` 集成测试目录。

#### [中等] 子 crate 版本号失去意义

主程序每次提交 bump(0.15.93);子 crate 停在 0.1.0~0.3.0 长期不动(ipma-auth 自拆分后从未变过)。"每次更新 bump 版本号"规范未明确是否覆盖子 crate,实际执行已双轨。

#### [轻微]

- `PgPassFile` 双实现已互相漂移:`crates/ipma-data-management/src/backup.rs:8` vs `crates/ipma-init/src/utils.rs:21`(同为临时 .pgpass 创建 + Drop 清理,错误类型已分叉)。
- `src/system/config.rs` 1,099 行混装 29 个 handler、十来个子域(系统信息/systemd 重启/配置备份/会话超时/语言/通知/SMTP 测试/密码策略/仪表盘统计),且与 `crates/ipma-common/src/config.rs`(配置加载)同名不同义。
- `JwtConfig` 同名异构:common 版为 TOML 形态(expiry 字符串"15m"),auth 版为运行时形态(u64 秒+算法+issuer),易混淆。
- `README.md:221-258` 项目结构段仍描述拆分前的 `src/auth/`、`src/resource/`、`src/db.rs` 等旧结构;权威结构图在 docs/code-style.md 2.1 与 `src/lib.rs:5-17`。
- `_secadmin: SecAdminUser` 例外(`src/log/forwarding.rs:173` 等)未写入 AGENTS.md 的例外条款;`let _ =` 丢弃返回值 20 处亦不在成文例外内。
- 8 个文件绕过 log_i18n 直接硬编码中文 tracing 日志(`crates/ipma-resource/src/device/mac.rs:46,83`、`snmp.rs:13`、`lldp.rs:9`、`crates/ipma-init/src/verification.rs:11`、`crates/ipma-common/src/rate_limit.rs:40,47` 等)。
- glob 再导出泛化(`pub use xxx::*`,models/resource/data-management),visualization 已因同名冲突被迫走模块路径访问。
- ipma-init lib.rs:1372-1401 扁平再导出约 25 个符号,兼容层思维(utils/、init/lib.rs)在拆分后普遍保留,长期是认知负担。

### 1.3 做得好的

1. **依赖倒置纪律严明**:业务 crate 零反向依赖,状态访问一律经 `DbProvider`/`AuthProvider`/`DataProvider` trait,由 `src/app_state.rs` 统一实现。
2. **规范落地有机制保障**:workspace clippy 护栏(根 Cargo.toml:10-19)+ fmt + CI `-D warnings`,unwrap/expect/println/`#[allow]` **全库零残留**(命中均为测试代码)。
3. **拆分过程有完整决策记录**:7 个"拆分 N/6"提交逐层下沉,system/log 留守有明示理由;`src/lib.rs` 与 docs/code-style.md 保留结构图与契约说明。
4. **文件尺寸控制好**:46,711 行无一个超 2,000 行文件,最大 login.rs 1,991 行;`_` 忽略参数严格遵守例外条款。
5. **i18n 三层体系**(前端 locales / 后端日志 yml / API 消息 key 化)边界清晰、有架构注释;日志多语言按 target 路由分文件。
6. **统一响应/分页/错误契约**:`ApiResponse`/`ok_json`/`AppError`/`paged_response`/`classify_db_error` 集中于 ipma-common 且高频一致使用(分页 21 处调用零手写 total_pages)。
7. 模块声明风格统一:10 个 mod.rs,全库不存在 `foo.rs` + `foo/` 混用。

---

## 二、前端(web/)

### 2.1 形态与规模

纯静态 HTML + 原生 ES Modules,**无构建链**;web/package.json(0.15.74)仅承载 lint/test 工具链(eslint 9/stylelint 17/prettier 3/htmlhint/jest 30/@lhci/cli),无 runtime 依赖、无 build script。nginx 缓存分层:`/static/js/` no-cache(ESM 相对导入不带版本号),其余 `?v=` immutable,入口 HTML no-store。

```
web/static/
├── main.html (1,382 行)   # 单页外壳:7 个 content-section,27 个 CSS link,唯一 <script> 为 app.js 模块入口
├── index.html (333) / init_index.html (186)
├── css/    30 文件 8,379 行(base/components/layouts/modals/pages/utilities 分层)
├── js/     59 文件约 24,161 行(app.js+login.js 入口;config/ 2;modules/ 23 业务模块;
│           modules/visualization/ 10;modules/ipma_init/ 4;utils/ 18 共享层)
├── modals/ 41 个 HTML 按 13 个功能域分目录
└── i18n/   zh.json + en.json(各 2,125 个展开键,双向差集为 0)
```

目录按功能域与后端 crate 大体一一对应:organization、resource 系列(networks/room/cabinet/position/workstation/cableLink/device)、visualization(10 文件)、auth、init、scheduler(并入 system 域)、data-management 与 x509-manager(并入 systemManager,见问题)。

### 2.2 问题清单

#### [中等·已验证] 后端 30 个 API 消息键缺失于前端翻译目录,用户会看到裸键

全量比对(非抽样):后端 `msg()` 共 472 个键,剔除 `crates/ipma-common/src/msg.rs` 单元测试桩后,前端 zh/en 目录缺失 **30 个**:

- **`server.import_export.*` 全部 28 个**(file_too_large、csv_parse_failed、header_mismatch、zip_write_failed、invalid_zip_entry、duplicate_key 等),来自 `crates/ipma-data-management/src/import/mod.rs:120,125,248` 等处的 DataError,CSV 导入/导出失败场景必现;
- `server.detail.internal` / `server.detail.sensitive`,来自 `crates/ipma-init/src/error.rs:187,203` 与 `crates/ipma-visualization/src/layout.rs:606,618`。

根因是**命名分叉**:前端已有 `import_export.*` 命名空间(14 键,不带 `server.` 前缀,文案内容其实存在),后端却另立 `server.import_export.*`;而 `web/static/js/utils/apiClient.js:19` 的 `translateServerMessage` 直接 `t(data.message)`,`t()` 无前缀回退、缺键返回键本身,用户会看到 `server.import_export.file_too_large` 原文。

**防护盲区**:`tests/frontend_consistency.rs` 的 i18n 校验只检查 (a) zh/en 互相一致、(b) JS `t()`/HTML `data-i18n*` 引用的键存在——不检查"后端 `msg()` 键 ⊆ 前端目录"方向,故此缺口无测试拦截。

#### [中等] systemManager.js 与后端 crate 化不对齐

`web/static/js/modules/systemManager.js`(1,468 行,前端最大文件)聚合至少 10 个子域:系统信息/SMTP/LDAP/SSO/通知/日志统计与清理/CSV 导入导出/数据库备份/密码策略/证书管理(40+ 函数)。后端已拆出 ipma-data-management、ipma-x509-manager、ipma-scheduler 三个 crate,前端却收拢在一个文件里,是前端最大的内聚性债务。类似但轻微:`login.js` 1,311 行承载登录页全部逻辑(密码/邮箱验证码/2FA/忘记密码/会话),与 modules/ 按域拆分风格不一致。

#### [中等] 单元测试覆盖极低

59 个 JS 中仅 3 个纯工具文件有 Jest 单测(helpers/network/sessionManager,共 275 行),约 12,000 行业务模块零覆盖。Lighthouse 以 `python3 -m http.server` 纯静态服务运行,API 全部 404,指标仅反映资源加载层面(web/.lighthouseci/ 为产物,断言 0 失败)。

#### [轻微]

- web/package.json 版本 0.15.74 落后 Cargo 0.15.93 约 19 个版本(d0354c6 后未再同步;该文件自述仅工具环境,影响有限)。
- `utils/networkCardManager.js`(752 行,含完整模态 UI 逻辑)放在 utils/ 而非 modules/,归类与目录语义不符。
- CSS 模态样式三处存放(`css/modals/` 仅 4 文件、`css/components/modals.css` 359 行、多数实际在 `css/pages/`)边界模糊;`css/pages/net_area_modal.css`(11 行)名字是 modal 却放 pages/。
- 个别重复实现:`deviceMacLldp.js:117` 6 行局部 `debounceInput` 与 ui.js 的共享 debounce 重复;systemManager.js 6 处内联 `toLocaleDateString()/toISOString()` 绕开 formatter.js。
- 暗色模式未实现(全 CSS 0 处 `prefers-color-scheme` / `[data-theme`),主题仅靠 variables.css 单套变量(108 个 CSS 变量)。
- 约 4 行英文注释与"注释统一中文"不完全一致(如 ui.js、i18n.js:111)。
- `app.js:56-69` 初始化失败兜底用 innerHTML/内联样式(CSP 允许 `style-src 'unsafe-inline'`,属错误兜底路径,可用)。

### 2.3 做得好的

1. **无构建原生 ESM 的工程纪律靠机制保证**:CSP `script-src 'self'`(main.html:9)使内联脚本根本不可用——0 内联 script、0 内联 style、0 内联事件(onclick= 全库 0 处,addEventListener 237 处);alert/confirm/prompt 与 console.log 全量 grep 为 0;全局 `window.` 赋值全库仅 1 处(userManager.js:160)。
2. **用 Rust 集成测试固化前端一致性**(tests/frontend_consistency.rs,5 项:JS 引用的 DOM id 存在、MODAL_REGISTRY/MODULE_REGISTRY 指向的文件存在且键匹配、i18n zh/en 一致且引用键存在、动态 loadModule 名称已注册、资源版本号全站统一),精准补偿无构建前端缺失的编译期校验;`asset_versions_must_be_uniform`(:301)已把 `?v=` ↔ MODULE_VERSION 联动固化为阻断测试(当前同号 01359)。
3. **API 层统一且成熟**:`utils/apiClient.js`(343 行,32 文件引用)覆盖同键并发 GET 去重、token 定时刷新+刷新单飞、401 自动 refresh 重试、429 按 Retry-After 退避、blob 下载提取 filename、`translateServerMessage` 服务端 key 翻译;裸 fetch 全库仅 13 处/6 文件且各有充分理由,业务模块零散写 fetch 不存在;循环依赖(apiClient ↔ resources)已用 `CustomEvent("ipma:data-mutation")` 主动解耦。
4. **懒加载体系完整**:`resourceLoader.js` MODULE_REGISTRY 21 条 + 双 Map 去重 + `when: idle/visible`(requestIdleCallback/IntersectionObserver)+ idle 预热 + 模态 HTML 内存缓存 + 按需 CSS(styleLoader.js);对"动态 import 双实例"深坑有注释与防护(动态 import 不带版本号)。
5. **i18n 质量高**:zh/en 各 2,125 键完全对称零漂移;按需加载(首屏 gzip 约 29KB);data-i18n* 属性 954 处 + JS `t()` 1,000 次;前后端"后端回 key、前端翻译"契约清晰(后端 `src/i18n/*.yml` 仅 256 键日志文案)。
6. **共享层收敛**:toast 唯一实现(toast.js,319 次调用/34 文件)、防抖唯一共享实现(ui.js)、分页消费完全收敛到 pagination.js(消费 total_pages,含页大小选择器与跳页框)。
7. **CI 分工明确**(.github/workflows/ci.yml):eslint(质量)→ stylelint(CSS)→ prettier --check(格式)→ jest(纯函数),阻断级。
8. web/.trae/rules 仅 4 行指向 docs/code-style.md,不维护副本防漂移。

---

## 三、建议的处理优先级(仅排序,未动手)

1. **i18n 缺键**(影响真实用户,改动小):对齐 `server.import_export.*` 命名或补齐 30 键,并把"后端 msg() 键 ⊆ 前端目录"加进 tests/frontend_consistency.rs——现有校验方向恰好漏了这一边。
2. **AppJson / DatabaseConfig 副本**(违反自家硬性规范,DatabaseConfig 涉及三处手工搬运):删副本改用 ipma-common 导出,顺带把 AppJson 移出 static_files.rs。
3. **models→scheduler 依赖倒挂与 systemManager.js 拆分**(各半小时级与半天级的结构收敛)。
4. **规范文本回写**:AssertSqlSafe 例外、`_secadmin` 例外、子 crate 是否 bump 的澄清、README 结构段更新。
