# FOIMS Agent 版本记录

版本号规则：见主仓 docs/dev_version.md（格式一致）；agent 家族版本独立自
管理，取本工作空间根 `Cargo.toml` 的 `[workspace.package].version`，两个
平台 crate 以 `version.workspace = true` 共享。

# v0.2.0

# 2026101518480701
# 2026年10月15日18点48分07秒 第1次修改

1.从 foims 主仓拆出为独立工作空间（与主程序仅网络通信，版本解除绑定）：
  - `foims-agent-core`：平台无关层——指标模型（MetricFamily）、采集器框架
    （Collector trait/超时冷却/scrape + ScrapeMetrics 四字段注入，解决
    node_/windows_exporter_ 两套抓取指标名与 help 文案差异）、上报
    transport 全套（h3 mTLS 交换/退避/断网缓存/证书续期检查）与
    `Platform` trait（version/machine_id/collect_frame/persist_renewed_material
    四钩子，Reporter<P: Platform> 泛型复用全部上报调度）；
  - `foims-protocol`：上报协议类型与 x509 解析（与主仓 foims-common 的
    report.rs/x509.rs 为受控双副本，修改必须双侧同步）；
  - `foims-agent`：Linux 平台 33 个 node_exporter 对齐采集器 + LinuxPlatform
    （/etc/machine-id、proc 统计、传感器、续期落盘 0644/0600 原子替换）；
  - `foims-agent-windows`：windows_exporter 默认启用 8 采集器（cpu/logical_
    disk/memory/net/os/physical_disk/service/system）Go→Rust 移植，指标名/
    help/标签逐字对齐（windows_exporter_collector_duration_seconds 等）；
    FFI 集中 ffi.rs（PDH/注册表/SCM/系统信息/卷信息/进程时间，模块级
    allow(unsafe_code) 唯一豁免点），os 采集器走注册表 + GetTickCount64 +
    GlobalMemoryStatusEx 避免 WMI，service 走 SCM API + 进程创建时刻；
  - workspace lints：`unsafe_code = "deny"`（可被 ffi 模块局部 allow 覆盖）
    + clippy 护栏集。
2.交叉编译矩阵：tier1/2 五 musl 目标（gcc 后端注入 musl 交叉 C 编译器 /
  AGENT_BACKEND=zigbuild 走 zig cc）+ 新增 x86_64-pc-windows-gnu（mingw-w64
  交叉 C 编译器或 zigbuild），产物统一命名 foims-agent（windows 目标
  cargo 自动追加 .exe，经 [[bin]] name = "foims-agent"）；windows 0.62
  适配要点：BOOL 移至 windows-core、QueryServiceConfigW/GetProcessTimes
  等 BOOL 型 API 返回 Result<()>、ENUM_SERVICE_STATUS_PROCESSW/
  QUERY_SERVICE_CONFIGW 带 W 后缀、dwCurrentState/dwStartType 为新类型
  需 .0 取值。manifest.json version 语义 = agent 版本（版本门控已移除）。
3.验证：cargo test --workspace 173 测试全绿、clippy --release -D warnings
  与 fmt --check 通过；x86_64-pc-windows-gnu 交叉编译实测产出 PE32+ 二进制
  （11.6 MiB，--only 与全量构建均验证归集与 SHA256SUMS/manifest 生成）。
