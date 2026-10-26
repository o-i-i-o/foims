# FOIMS 古法编程全局审计报告

> 审计日期：2026-10-26
> 审计原则：全局一致性优先 / 可维护性优先 / 业务逻辑优先 / 可解释性优先
> 审计对象：全仓库（主工作区 `src/` + `crates/`、`frontend/`、`foims-agent/`、`foims-portal/`、部署与流程）
> 边界：`node_exporter/`、`windows_exporter/`（第三方）、`web-archive/`（只读存档）不审计
> 前轮基线：[code-audit-2026-10-22.md](./code-audit-2026-10-22.md)（其问题项修复核验见 §4）

## 1. 审计范围与方法

| 维度 | 内容 |
|---|---|
| 后端 | Rust 2024 + axum 0.8 + sqlx 0.9，212 文件 / 70,311 行（13 个 crate + 主程序） |
| 前端 | Vue 3 + Vite + TypeScript，312 文件 / 33,887 行（`frontend/`） |
| Agent / Portal | 独立工作空间 19,467 行（foims-agent 4 crate + foims-portal） |
| 方法 | 硬性护栏全量检索 + 跨 crate 同名/镜像实现比对 + 注释自认同步义务检索 + 旧审计逐项核验 + 术语抽样 |

四类一致性检索入口：

1. 跨 crate 公开函数名去重统计（`pub fn` 同名 ≥2 逐个语义比对）；
2. 注释中自认"复刻 / 语义对齐 / 必须同步修改"的镜像代码；
3. 前端裸 `fetch`、手写分页、工具重复、样式约定偏离；
4. 前轮审计 B1–B3 / F1–F7 / R1–R4 逐项回归核验。

## 2. 合规项（本轮确认，无需整改）

- **硬性护栏零违规**：非测试代码 0 处 `unwrap/expect`（52 处命中全部位于
  `#[cfg(test)]` 边界内）、0 处 `println!/dbg!`、0 处 `#[allow]`；前端 0 处
  `any`、0 处 `alert()`。
- **分页响应统一**：49 处全部走 `paged_response`，无手写五键旁例。
- **visualization 双层结构非过度抽象**：`http.rs`（`P: DbProvider` handler 层）
  对 `topology.rs`（业务 + SQL）是薄委托，与 resource/organization 的 crate
  结构完全一致，属全局统一而非重复实现，保留。
- **`test_smtp_connection` 双定义非重复**：`src/system/config.rs:1000` 是路由
  handler（读库 + 审计），核心逻辑单点委托 `foims_auth::smtp::test_smtp_connection`，
  分层合理，保留。
- **术语统一达标**：SNMP community 全链路统一"团体名"（zh.json 三处文案、
  en.json "Community String"），无"团体字/社区标签"残留；前端 `console.error`
  带中文上下文符合 code-style.md §214 的明示要求。
- **受控双副本无漂移**：`foims-common/src/report.rs` 与
  `foims-agent/crates/foims-protocol/src/report.rs` 去注释后逐字 diff 为空；
  `x509.rs` 副本同样无漂移（独立 workspace 无法共享 path 依赖，属已声明的受控例外）。

## 3. 发现问题

### [高] R1. 组织 levels 校验簇三份复刻实现（同一功能多实现）

违反古法原则 1（全局一致性）与 AGENTS.md"跨 crate 共享类型放 foims-common，
不得复制副本"。模块注释自认债务："两边规则如需调整必须同步修改"
（[org_validate.rs:8](../crates/foims-data-management/src/import/org_validate.rs)）。

重复清单（5 个符号、6 处实现）：

| 概念 | 位置 A | 位置 B | 位置 C |
|---|---|---|---|
| `validate_levels_mapping` | `foims-organization/org_template.rs:84`（返回根类型 String） | `foims-data-management/import/org_validate.rs:24`（返回 `()`，逐字复刻） | — |
| `is_valid_type_char` | `org_template.rs:78` | `org_validate.rs:17` | — |
| `MAX_ORG_DEPTH` | `org_template.rs:18` | `org_validate.rs:13` | — |
| 根节点查找 | `organization.rs:126 find_root_in_levels`（返回 Result） | `org_template.rs:392 find_root`（返回 Option） | `org_validate.rs:175`（同 B 语义复刻） |
| type_path 导航 | `organization.rs:32 resolve_type_name`（解析出类型名） | `org_validate.rs:195 type_path_resolvable`（仅判可解析，导航循环逐字相同） | — |

业务语义核心：levels 是"类型 → 子类型列表"的树模板，校验规则（对象/数量/
命名/子级已定义/唯一根/无环/深度 ≤ 10）是同一份业务契约，API 端与 CSV 导入端
必须永远一致——目前靠人工双写维持，任一侧漏改即产生"模板能建、导入拒收"
（或反之）的行为分裂。

**整改方案**：新增 `foims-common/src/org_levels.rs` 单点实现（common 已依赖
serde_json；错误返回 `AppMessage`，调用方按各自语义包装 `AppError::Validation/Internal`），
三侧改为引用；`resolve_type_name` 的导航循与 `type_path_resolvable` 收敛为
共享的"路径解析返回类型名"函数，可解析性判定用 `is_ok()`。删除
`org_validate.rs` 整个复刻模块（其测试迁移为 common 测试）。
风险：低（纯重构，规则逐字取现实现，无行为变化）。

### [中] R2. `start_cleanup_task` 同名不同义（同一概念多命名之反例）

`foims-common/rate_limit.rs:341`（限流桶内存清理）与
`foims-auth/app_fail2ban.rs:110`（fail2ban 记录清理 + 配置加载 + 邮件频控回收）
是两个完全不同的后台任务，共用同一函数名。跨 crate 检索、日志排查与调用点
阅读时极易混淆，违反命名全局统一。

**整改方案**：fail2ban 侧更名 `start_fail2ban_cleanup_task`（其注释表明还承担
"启动加载配置"职责，名字也应体现）。风险：极低（改调用点即可）。

### [中] R3. 双副本同步义务无自动校验（流程缺口）

report.rs / x509.rs 双副本目前"人工同步 + 注释声明"，本轮核验无漂移，但
CI 与 `da-scripts/release_check.sh` 均无任何漂移拦截——下次改协议字段漏同步
只能靠人眼。对比：ts-rs 生成物已有 CI 漂移校验，同类义务应同等待遇。

**整改方案**：在 `foims-common` 增加一个测试（或 release_check.sh 检查项），
去注释逐字 diff 两侧副本文件，不一致即失败。风险：无（只增校验不改行为）。

### [低] F1. AgentsPage 手写分页，未复用 `usePagedList`（旧审计 F2 未整改）

`AgentsPage.vue:57-95` 手工维护 `page ref + URLSearchParams + totalPages` 与
越界回退；`usePagedList`（含越界回退）已被 6 个页面/组件复用。同一功能两套
实现，且手写版无表头排序能力，后续演进易再分叉。

**整改方案**：AgentsPage 迁移到 `usePagedList`（extraKey 承载 status/keyword
筛选）。风险：低（行为等价迁移，页面自测即可）。

### [低] F2. 可视化 SVG 内联裸色值（旧审计 F4 残留）

`WorkstationCanvas.vue:349`、`CabinetCanvas.vue:400` 的 `stroke="#3b82f6"`
为 variables.css 体系外裸 hex，且两文件各写一遍同一颜色。

**整改方案**：提取为 `utils/visualization` 共享常量（SVG presentation
attribute 不吃 CSS 变量，常量是正确落点）。风险：无。

### [低] F3. `usePageTimeout` 默认超时 30 分钟双写

`usePageTimeout.ts:18`（函数内 `|| 30`）与 `:31`（初始值 `30`）同一兜底值两处
字面量。属重复防御性处理：兜底口径若调整需改两处。

**整改方案**：提炼 `const DEFAULT_TIMEOUT_MINUTES = 30` 单点引用。风险：无。

### [信息] I1. 旧审计全部问题已修复（回归核验通过）

| 项 | 核验结果 |
|---|---|
| B1 persistent(false) | 77 处 QueryBuilder 块全部带 `persistent(false)`，脚本复检缺失 0 |
| B2 鉴权每请求 2 次 DB 往返 | `middleware.rs` 现仅 1 次 `query_as` |
| B3 配置备份权限 | 备份载荷全量掩码（password/secret/enrollment_token/trap 均 `***`），敏感面已消除 |
| F1 skipAuthCheck 失效 | 前端已无任何 `skipAuthCheck` 残留 |
| F3 泛型标注 | 无泛型裸调用从 67 降至 22 处，且集中在 `apiDelete`（响应载荷本为 `()`，可豁免） |
| F6 mousemove 节流 | 旧方案已重构为 `usePageTimeout` 统一监听（仅余 F3 双写小点） |
| R1 CI 分支覆盖 | `branches: [ "**" ]` 全分支触发 |
| R2 upgrade 目录 | 每个中版本目录仅保留一份 SQL |

### [信息] I2. 不整改项及理由（业务逻辑优先，避免为改而改）

- **超大 SFC**（VisualizationPage 1584 / LoginPage 1465 / ResourcesPage 1260 行，
  旧审计 F7）：拆分属结构重构，当前无并行开发冲突，收益低于回归风险，
  建议随可视化五阶段重构计划顺带处理，不单独立项。
- **InitWizard.vue 裸 `fetch("/api/auth/init-status")`**：初始化向导运行于
  鉴权就绪之前，client.ts 的 401 拦截/刷新逻辑对其是干扰项，豁免合理；
  建议补一行豁免注释使其"受控例外"显性化（随 F 项整改顺带加）。
- **report.rs/x509.rs 双副本本身**：独立 workspace 的架构约束，已声明同步
  义务且无漂移，仅补校验（R3），不做工程合并。

## 4. 结论与优先级

| 优先级 | 项 | 动作 |
|---|---|---|
| 高 | R1 组织校验簇三份复刻 | 收敛至 `foims-common::org_levels` 单点 |
| 中 | R2 同名不同义函数 | fail2ban 侧更名 |
| 中 | R3 副本漂移无校验 | 增加逐字 diff 校验 |
| 低（默认同修） | F1 / F2 / F3 | 各自单点小改 |

R1 为本轮唯一结构性整改；完成后"规则需两侧同步修改"的技术债注释即删除，
组织层级校验在全仓库只剩一份权威实现。
