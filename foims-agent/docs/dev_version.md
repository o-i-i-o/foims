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

# v0.2.1

# 2026101523493401
# 2026年10月15日23点49分34秒 第1次修改

1.清理 HTTP/3 上报可行性 demo 残留：删除 examples/h3_demo.rs（server/client
  双子命令验证性代码，假设已由 foims-agent-core transport 与主仓
  foims-agent-service report_server 正式实现承载）及仅服务该 demo 的
  dev-dependencies（bytes/h3/h3-quinn/http/quinn/rustls/tokio，正式链路依赖
  均在 foims-agent-core [dependencies]），同步修正 reporter.rs 与
  report_smoke.rs 中对 h3_demo / gen-h3-demo-certs.sh 的注释引用；
  report_smoke 冒烟示例（走正式 Reporter）保留。

# v0.3.0

# 2026101601004501
# 2026年10月16日01点00分45秒 第1次修改

1.采集/上报解耦 + 批量上报（大规模主机部署负载优化，模块级 x+1）：
  - 采集与上报解耦：新增 collect_interval_secs 采集间隔（默认 5s，
    clamp 1-60），ConfigFile/ReporterConfig 扩展该字段（build 四参签名
    不变、缺省 5）；独立采集任务周期采帧入内存队列，主循环仍按
    report_interval 批量出队上报，采集频率不再受上报间隔拖累；
  - FrameQueue 内存帧队列（上限 120 帧 / 8MiB，超限弃最旧，dropped
    计数随响应回执观测）；上报失败整批放回并按时间序对半拆分逐级
    降批量（最终退化为单帧），兼容旧服务端与 413 容量拒绝；
  - H3Transport QUIC 连接复用：空闲 ≥120s 自动关闭重建，复用失败重建
    重试一次；证书续期成功后立即关闭连接使新证书生效；删除断网磁盘
    缓存重放（PENDING_CACHE_CAP/cache_push/replay_pending）；
  - report_once（--once）保持单帧单报不变；
2.foims-protocol 受控双副本同步：AgentReportBatch{frames, dropped}
  批量帧协议（主仓 foims-common report.rs 同步，roundtrip 测试双侧覆盖）；
3.验证：cargo test --workspace 178 测试全绿，clippy --release
  -D warnings 与 fmt 通过。

# v0.4.0

# 2026101608304901
# 2026年10月16日08点30分49秒 第1次修改

1.分发模型重构（构建链，agent 程序代码零改动）：build-agent.sh 新增 OS
  系统包组装——Linux 每目标 deb（dpkg-deb）+ rpm（rpmbuild，_dbpath 指向
  临时库免系统 rpmdb 依赖），Windows zip（exe + README.txt），产物归集
  dist/packages/ 并生成 SHA256SUMS；systemd unit（foims-agent.service）与
  Windows 说明为 deploy/agent/ 包内物料唯一来源；删 manifest.json 与
  install.sh 归集（服务端动态组二进制链路已移除，无消费者）；产物结构
  dist/agents/ → dist/bin/（二进制原料）+ dist/packages/（系统包）；
2.程序包/配置包解耦语义：程序包只含二进制 + systemd unit + 空配置目录
  /etc/foims-agent/（postinst 检测服务端下发的配置包已就位即 enable +
  try-restart，prerm 卸载时 stop/disable）；配置由主程序 /api/agents/
  config-package 现场组包安装到同目录；
3.CI：ci.yml build 任务独立跑 build-agent.sh（AGENT_BACKEND=zigbuild），
  release 任务改下载 build 任务 artifact 直传发布页，不再重复编译。
