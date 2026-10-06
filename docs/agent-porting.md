# foims-agent 采集器移植清单

node_exporter（Go）→ foims-agent（Rust）采集器移植对照表。方案权威文档见
[agent-design.md](agent-design.md)，本文只负责移植进度跟踪。

## 移植规则

- 指标名/标签/help/类型与 Go 源逐字对齐（`node_` 命名空间），便于与原版输出对照验证；
- 只读 /proc、/sys 与少量 syscall（经 rustix 安全封装，crate 内零 unsafe）；
- 每个采集器支持 `with_root` 路径注入，测试复用 node_exporter 的 fixture
  （`tests/fixtures/`，与原版 `collector/fixtures/` 同构；proc 全量复制、
  sys 由 sys.ttar 合并所需子树、textfile 整目录复制，mounts/net/dev/selinux
  等按测试断言自建）；
- 采集器文件位于 `crates/foims-agent/src/collectors/<name>.rs`，注册进
  `collectors/mod.rs` 的 `build_defaults()`（按字母序）；
- Go 的 build tag（`!no<collector>`）在 Rust 中对应「是否编入 build_defaults」；
- 采集器的布尔开关参数（如 `--collector.stat.softirq`）随 flag 语义裁剪，
  未开放 CLI 开关的一律按默认行为实现。

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

## 批次 1：核心采集器（已完成）

| 采集器 | 默认 | Go 源 | Rust 状态 | 备注 |
|--------|------|-------|-----------|------|
| boottime | 启用 | boot_time_linux（procfs stat btime） | 已移植 | /proc/stat btime |
| cpu | 启用 | cpu_linux.go | 已移植 | /proc/stat 部分；cgroup 节流指标（cpu.go）未移植 |
| cpufreq | 启用 | cpufreq_linux.go | 已移植 | 含 scaling_* 全套 |
| diskstats | 启用 | diskstats_linux.go | 已移植 | 17 指标族；udev 扩展未移植 |
| filesystem | 启用 | filesystem_linux.go + common | 已移植 | statfs 走 rustix；mount_info/purgeable 未移植 |
| hwmon | 启用 | hwmon_linux.go | 已移植 | include/exclude 过滤参数未移植 |
| loadavg | 启用 | loadavg.go + linux | 已移植 | 仅 node_load1/5/15 |
| meminfo | 启用 | meminfo.go + linux | 已移植 | 泛化解析全部字段 |
| netdev | 启用 | netdev_common.go + linux | 已移植 | /proc/net/dev 路径；netlink 路径未移植 |
| os | 启用 | os_release.go | 已移植 | node_os_info/node_os_version/support_end |
| thermal_zone | 启用 | thermal_zone_linux.go | 已移植 | cooling_device 未移植 |
| time | 启用 | time.go + time_linux.go | 已移植 | 时区偏移指标未移植（std 无本地时区 API） |
| uname | 启用 | uname.go | 已移植 | rustix::system::uname |

## 批次 2：系统核心（已完成 19 个）

默认启用集合新增（按字母序）：arp、bonding、conntrack、dmi、edac、entropy、
filefd、kernel_hung、netstat、pressure、schedstat、selinux、sockstat、softnet、
stat、textfile、udp_queues、vmstat、watchdog。

| 采集器 | Go 源 | Rust 状态 | 备注 |
|--------|-------|-----------|------|
| arp | arp_linux.go | 已移植 | /proc/net/arp 路径（即原版 netlink=false 行为）；设备正则过滤未移植 |
| bonding | bonding_linux.go | 已移植 | sysfs 路径；lower_/slave_ 前缀回退 |
| conntrack | conntrack_linux.go | 已移植 | count/max + stat/nf_conntrack 逐 CPU 聚合 |
| dmi | dmi.go | 已移植 | 20 属性；标签顺序固定（原版 map 迭代随机）；system_vendor 读 sys_vendor 文件 |
| edac | edac_linux.go | 已移植 | mc/csrow/ch 层级 + dimm_label 转换；Glob 字典序 |
| entropy | entropy_linux.go | 已移植 | entropy_avail/poolsize |
| filefd | filefd_linux.go | 已移植 | file-nr 三列取 1/3 列 |
| kernel_hung | kernel_hung_linux.go | 已移植 | hung_task_detect_count |
| netstat | netstat_linux.go + descs | 已移植 | help 为统一模式「Statistic + 去下划线 key」，无需移植 311 条 descs 大表；过滤正则以字符串逻辑等价实现 |
| pressure | pressure_linux.go | 已移植 | cpu/io/memory/irq；行缺失语义对齐 Go（NoData） |
| schedstat | schedstat_linux.go | 已移植 | 兼容内核 6.2+ 十字段与旧版四字段两种行格式 |
| selinux | selinux_linux.go | 已移植 | config mode 按 go-selinux 编码（1/2/3），缺省 permissive |
| sockstat | sockstat_linux.go | 已移植 | 含 mem_bytes（rustix page_size）；未知键忽略 |
| softnet | softnet_linux.go | 已移植 | 列偏移 0/1/2/4/5/10/12；CPU 序号取行序号（对齐 procfs） |
| stat | stat_linux.go | 已移植 | node_boot_time_seconds 由 boottime 输出（避免重复族）；per-vector softirq（默认关）未移植 |
| textfile | textfile.go | 已移植 | 内置 Prometheus 文本解析器；标签并集补齐/help 合成/冲突置错语义对齐；summary/histogram 跳过并置错（偏离） |
| udp_queues | udp_queues_linux.go | 已移植 | udp/udp6 求和；udp6 缺失跳过 |
| vmstat | vmstat_linux.go | 已移植 | 默认过滤正则以字符串前缀逻辑等价实现 |
| watchdog | watchdog.go | 已移植 | 数值文件缺失/非数值跳过；info 恒输出 |

## 批次 3：剩余（待移植）

| 采集器 | 默认 | 说明 |
|--------|------|------|
| bcache | 启用 | sysfs，fixture 已随 sys.ttar 就位（272 项） |
| nfs / nfsd | nfs 默认关 / nfsd 启用 | /proc/net/rpc，指标族较多 |
| xfs | 启用 | /proc/fs/xfs/stat；Go fixture 缺失，需自建测试数据 |
| netclass | 启用 | Go 走 rtnetlink，需 sysfs 等价路径或 netlink 决策 |
| powersupplyclass | 启用 | /sys/class/power_supply |
| nvme / nvmesubsystem | 关 | /sys/class/nvme |
| dmmultipath | 关 | Go 依赖 dmsetup 子进程，需先评估 exec 策略 |
| timex | 启用 | 阻塞于 rustix 无 adjtimex 封装（crate 禁 unsafe） |

## 批次 4：进程/服务/硬件扩展（默认关闭，待移植）

processes、systemd、logind、network_route、pcidevice、qdisc、slabinfo、tcpstat、
wifi、zoneinfo、ethtool、drm、lnstat、ntp、interrupts、softirqs、buddyinfo、
sysctl、swap、xfrm、supervisord、runit

## 非 Linux 采集器（不移植）

netisr（FreeBSD）、exec（BSD）、partition/netinterface（AIX）、devstat
（Dragonfly/BSD 系）；diskstats/loadavg 等 Go 文件的多平台变体以 Linux 版为准。

## 已知偏离汇总

- 正则类过滤（netdev 设备过滤、filesystem 排除规则、hwmon include/exclude、
  netstat/vmstat 字段过滤、arp 设备过滤）以字符串/前缀匹配等价实现，
  未引入 regex 依赖；
- cpu 采集器未移植 cgroup 节流指标与 idle 回跳缓存（无状态实现）；
- netdev/filesystem 未移植并发抓取与卡死超时机制；
- time 未输出 `node_time_zone_offset_seconds`（待 chrono/tz 依赖决策）；
- textfile 仅支持 counter/gauge/untyped，summary/histogram 跳过并置
  `node_textfile_scrape_error` 1；目录为单目录注入（原版支持多目录与 glob）；
- stat 的 `node_boot_time_seconds` 由 boottime 采集器输出，避免重复指标族
  （原版两采集器均输出）；
- arp 默认走 /proc/net/arp（原版默认 netlink），统计范围限于 IPv4 邻居；
- selinux config mode 缺省值取 go-selinux 的 permissive(2) 兜底；
- 测试为各采集器文件内的 `#[cfg(test)]` 模块，fixture 数据位于
  `crates/foims-agent/tests/fixtures/`，`cargo test -p foims-agent` 全绿。
