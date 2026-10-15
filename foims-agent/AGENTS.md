# FOIMS Agent 工作空间规范

本目录为 FOIMS Agent 的独立工作空间（位于 foims 主仓根目录下，从
crates/foims-agent 拆出），负责 Linux/Windows 平台主机指标采集与 HTTP/3
mTLS 上报。**独立 Cargo workspace，不进主仓 workspace members**（主程序
编译不受影响）；与主程序仅通过网络通信，版本独立自管理。

## 工程结构

```
crates/foims-agent-core     平台无关：指标模型、采集器框架（trait/超时冷却/scrape）、上报 transport
crates/foims-protocol       上报协议类型（AgentReport 等）与证书剩余寿命解析（与主仓 foims-common 人工同步）
crates/foims-agent          Linux/Unix 平台：33 个 node_exporter 对齐采集器 + LinuxPlatform
crates/foims-agent-windows  Windows 平台：windows_exporter 对齐采集器 + WindowsPlatform
```

- 平台 crate 只实现 `foims_agent_core::reporter::Platform`（机器标识/采集一帧/
  续期落盘），上报循环、退避、断网缓存、证书续期逻辑全部复用 core。
- `crates/foims-protocol` 与主仓 `foims-common` 的 `report.rs`/`x509.rs` 为
  双副本关系：**修改协议字段必须双侧同步**，并在两边 dev_version 记录。

## 硬性规则

- Rust 2024 Edition；编码 UTF-8；注释统一中文。
- 禁止 `unwrap()`/`expect()`/`unreachable!()`/`println!`（测试代码除外）；
  禁止用 `_` 忽略参数。
- unsafe 政策：workspace 层 `unsafe_code = "deny"`；**仅**
  `foims-agent-windows` 的 `ffi` 模块可局部 `#[allow(unsafe_code)]`，
  其余代码零 unsafe（Linux crate 继承 deny，永不写 unsafe）。
- 后端日志统一 `tracing`；动态 SQL 不适用（agent 不直连数据库）。
- 指标移植纪律见 `docs/agent-porting.md`：指标名/help/标签与原版
  （node_exporter / windows_exporter）逐字对齐。

## 版本与验证

- agent 版本取根 `Cargo.toml` 的 `[workspace.package].version`，两个平台
  crate 以 `version.workspace = true` 共享；构建产物清单 manifest.json 的
  version 语义为「本批产物的 agent 版本」（与服务端版本无绑定）。
- 每次更新后：`cargo fmt && cargo clippy --release -- -D warnings`；
  测试：`cargo test --workspace`（Linux 宿主可跑全部纯逻辑单测，
  windows crate 的 FFI 调用路径由 `cfg(target_os = "windows")` 门控）。
- 多平台构建：`bash scripts/build-agent.sh`（tier1/2 musl 五目标 +
  windows-gnu 目标；`--list` 查看矩阵）。
- 与主程序打包联动：主仓 `scripts/build-deb.sh` 会级联本工作空间的
  `scripts/build-agent.sh` 编译 agent 物料并装包（工作空间定位
  `AGENT_WORKSPACE` 环境变量可覆盖，默认主仓根下 `foims-agent/`）；
  dist/agents 产物归集在本工作空间，`AGENT_TARGETS=none` 可让主程序打包
  跳过 agent 物料。
