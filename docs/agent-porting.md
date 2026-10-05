# foims-agent 采集器移植清单

node_exporter（Go）→ foims-agent（Rust）采集器移植对照表。方案权威文档见
[agent-design.md](agent-design.md)，本文只负责移植进度跟踪。

## 移植规则

- 指标名/标签/help/类型与 Go 源逐字对齐（`node_` 命名空间），便于与原版输出对照验证；
- 只读 /proc、/sys 与少量 syscall（经 rustix 安全封装，crate 内零 unsafe）；
- 每个采集器支持 `with_root` 路径注入，测试复用 node_exporter 的 fixture
  （`tests/fixtures/`，与原版 `collector/fixtures/` 同构）；
- 采集器文件位于 `crates/foims-agent/src/collectors/<name>.rs`，注册进
  `collectors/mod.rs` 的 `build_defaults()`（按字母序）；
- Go 的 build tag（`!no<collector>`）在 Rust 中对应「是否编入 build_defaults」。

## 平台矩阵（scripts/build-agent.sh）

| 层级 | 目标 | 状态 | 说明 |
|------|------|------|------|
| tier1/2 | x86_64-unknown-linux-musl | 已验证 | rust-lld 自含链接，静态 PIE |
| tier1/2 | aarch64-unknown-linux-musl | 已验证 | 需 `CARGO_TARGET_…_LINKER=rust-lld` |
| tier1/2 | i686-unknown-linux-musl | 已验证 | std 组件走官方源离线兜底安装 |
| tier1/2 | arm-unknown-linux-musleabihf | 已验证 | 同上 |
| tier1/2 | armv7-unknown-linux-musleabihf | 已验证 | 同上 |
| tier3 | powerpc64le/s390x/riscv64gc/loongarch64/mips/mipsel/mips64/mips64el/powerpc musl | 待实施 | `-Z build-std` 只构建 std，musl crt 启动对象（crt1/crti/crtn）需各架构 musl 交叉工具链（musl.cc / 容器化交叉），脚本留 `--tier3` 开关 |
| OpenBSD | openbsd/amd64（.promu.yml 有） | 暂缓 | rustix/std 对 OpenBSD 支持有限，待服务端阶段评估 |

镜像缺组件兜底：`rustup target add` 404 时自动从官方 static.rust-lang.org 下载
对应版本 rust-std（sha256 校验，取自本地 channel manifest 的 dist 日期与哈希），
`install.sh --prefix=<sysroot>` 落位。

## 批次 1：核心采集器（demo，已完成）

| 采集器 | 默认 | Go 源 | Rust 状态 | 备注 |
|--------|------|-------|-----------|------|
| boottime | 启用 | boottime_linux.go | 已移植 | /proc/stat btime |
| cpu | 启用 | cpu_linux.go | 已移植 | /proc/stat 部分；cgroup 节流指标（cpu.go）未移植 |
| cpufreq | 启用 | cpufreq_linux.go | 已移植 | 含 scaling_* 全套 |
| diskstats | 启用 | diskstats_linux.go | 已移植 | 17 指标族；udev 扩展未移植 |
| filesystem | 启用 | filesystem_linux.go + common | 已移植 | statfs 走 rustix；mount_info/purgeable 未移植 |
| hwmon | 启用 | hwmon_linux.go | 已移植 | include/exclude 过滤参数未移植 |
| loadavg | 启用 | loadavg.go + linux | 已移植 | 仅 node_load1/5/15 |
| meminfo | 启用 | meminfo.go + linux | 已移植 | 泛化解析全部字段 |
| netdev | 启用 | netdev_common.go + linux | 已移植 | /proc/net/dev 路径；netlink 路径未移植 |
| os | 启用 | os_release.go | 已移植 | node_os_info/node_os_version/support_end |
| thermal_zone | 启用 | thermal_zone_linux.go | 已移植 | |
| time | 启用 | time.go + time_linux.go | 已移植 | 时区偏移指标未移植（std 无本地时区 API） |
| uname | 启用 | uname.go | 已移植 | rustix::system::uname |

## 批次 2：系统核心（默认启用，待移植）

arp、bcache、bonding、conntrack、edac、entropy、kernel_hung、netclass、netstat、
nfs、nfsd、pressure、schedstat、selinux、softnet、stat（node_procs_* 等）、
udp_queues、vmstat、xfs、textfile、dmi、dmmultipath、nvme、timex、watchdog、
powersupplyclass

## 批次 3：存储与网络扩展（待移植）

| 默认启用 | 默认关闭 |
|----------|----------|
| btrfs、fibrechannel、infiniband、ipvs、mdadm、tapestats、zfs | drbd、mountstats、meminfo_numa、nvmesubsystem |

## 批次 4：进程/服务/硬件扩展（默认关闭，待移植）

processes、systemd、logind、network_route、pcidevice、qdisc、slabinfo、tcpstat、
wifi、zoneinfo、ethtool、drm、lnstat、ntp、interrupts、softirqs、buddyinfo、
sysctl、swap、xfrm、supervisord、runit

## 非 Linux 采集器（不移植）

netisr（FreeBSD）、exec（BSD）、partition/netinterface（AIX）、devstat
（Dragonfly/BSD 系）；diskstats/loadavg 等 Go 文件的多平台变体以 Linux 版为准。

## 已知偏离汇总

- 正则类过滤（netdev 设备过滤、filesystem 排除规则、hwmon include/exclude）
  以字符串/前缀匹配等价实现，未引入 regex 依赖；
- cpu 采集器未移植 cgroup 节流指标与 idle 回跳缓存（无状态实现）；
- netdev/filesystem 未移植并发抓取与卡死超时机制；
- time 未输出 `node_time_zone_offset_seconds`（待 chrono/tz 依赖决策）；
- 测试为各采集器文件内的 `#[cfg(test)]` 模块，fixture 数据位于
  `crates/foims-agent/tests/fixtures/`，`cargo test -p foims-agent` 全绿。
