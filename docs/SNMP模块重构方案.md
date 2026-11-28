# SNMP 模块重构方案（自维护 Profile 版）

> **本版三条前提**
>
> 1. **接受破坏性变更**：不要求平滑过渡、不保留兼容层，目标是逻辑清晰、结构简洁。
> 2. **不再合并上游配置**：不再同步 LibreNMS 的 YAML / PHP；存量数据一次性导入后，
>    配置由本项目自行维护。
> 3. **能配置化的就配置化，不能的改造吸收**：LibreNMS 中可抽象为声明式规则的，
>    一律做成配置；无法抽象的复杂逻辑，用 Rust 重写吸收，而不是留空或照搬 PHP。
>
> 上一版的「三层合并 + origin 优先级 + 阶段 0 快照平滑过渡」全部作废。
> 本版是**一次性切换**方案。

---

## 一、与上一版的关键差异

| 维度 | 上一版 | 本版 |
|------|--------|------|
| 数据来源 | 上游生成物 + 内置补丁 + 现场覆盖，字段级合并 | 内置自维护文件 + 极简现场覆盖，整条替换 |
| 合并复杂度 | `Origin` 优先级 + 字段级 merge | 同 id 整条替换，无字段合并 |
| 迁移方式 | 阶段 0 适配层 + 快照零差异门禁 | 一次性导入 + 一次性切换，无兼容层 |
| 上游同步 | 保留生成脚本，跟随 LibreNMS 更新 | 删除生成脚本，自维护 |
| 配置化范围 | 先 CPU/内存/温度，传感器/存储留到后期 | 一次覆盖 CPU/内存/温度/存储/传感器/设备信息 |
| 复杂逻辑 | 「保留代码，边界不动」 | 明确吸收清单，逐个用 Rust 重写 |
| 识别与采集 | 一个 `OsProfile`，但保留字符串二次查表过渡 | `identify()` 返回 `Identity`，`profile` 直接进采集 |

---

## 二、现状：待导入的存量数据

改造后这些代码会全部删除，所以这里不做「兼容性分析」，只做「数据盘点」——
确认一次性导入不会丢东西。导入的源是**当前仓库内已有的数据**，
不重新从 LibreNMS 生成。

### 2.1 四套采集来源

| # | 来源 | 位置 | 规模 | 去向 |
|---|------|------|------|------|
| ① | 现场配置 | `/etc/foims/vendor_profiles.toml` | 现场自定 | 转成新格式的现场覆盖文件 |
| ② | 上游生成物 | `vendor/os_discovery.rs` | 404 个 OS | 导入 `profiles/*.toml` 后删除 |
| ③ | 内置默认 | `vendor/profiles_default.toml` | cisco / huawei / h3c | 拆进 `profiles/cisco.toml` 等 |
| ④ | 公共回退 | `live/profile.rs` | 5 类公共 MIB | 变成 `profiles/generic.toml` |

### 2.2 识别侧

`vendor/community/` 共 808 条规则（内置品牌 16 + 社区 OS 792），
来自 LibreNMS 825 个 `os_detection` YAML；`KIND_BY_OS` 29 条映射到 4 个 `VendorKind`。
这些规则全部并入 `profiles/*.toml` 的 `[profile.match]`，`KIND_BY_OS` 删除。

### 2.3 问题清单

| 编号 | 问题 | 本版如何根治 |
|------|------|-------------|
| P1 | 识别只返回字符串，采集再查三次表 | `identify()` 返回 `Identity`，`profile` 直接进采集 |
| P2 | 四套来源优先级写死在 `probe()` 的 if 顺序 | 只剩「内置 + 现场覆盖」两层，规则写进文档 |
| P3 | 同品牌多 OS 无继承，各配一份 | `group` 品牌族，字段级继承 |
| P4 | 秒级与 5 分钟两套 OID 来源 | 两条链路共用同一个解释器 |
| P5 | 型号/版本差异靠「列空就换」 | `when` 显式条件 |
| P6 | `MAX_DISCOVERY_TEMP_RULES = 12` 硬编码 | `max_candidates` 配置项 |
| P7 | 表达能力弱于上游 YAML | 补齐 `skip / divisor / precision / states / limits` |
| P8 | 数据分散在 4 处 + 2 个生成脚本，无法整体审阅 | 单一格式、一品牌一文件 |

---

## 三、LibreNMS 机制：抄什么，不抄什么

### 3.1 它的五层机制

1. **多 OS id**：`ios / iosxe / iosxr / nxos / asa / ciscowlc / ciscosb` 各自一个 OS，
   用 sysObjectID 深前缀区分。
2. **`group` 品牌族**：`os_detection/iosxe.yaml` 写 `group: cisco`，
   采集类继承 `Shared\Cisco`，公共逻辑只写一遍。
3. **每 OS 一份 `os_discovery` YAML**：声明 OID、`skip_values`、`divisor`、`precision`。
4. **Shared PHP 回退链**：Cisco 内存 `CEM → CMP → CPM`，CPU `5minRev → 5min → OLD-CISCO-CPU-MIB`；
   Huawei MB/字节双列优先 MB；H3C 按 entity class 过滤。
5. **运行期按「有没有数据」选**，配合 HC/非 HC 修正。

### 3.2 取舍

| 机制 | 是否采用 | 说明 |
|------|----------|------|
| OS 粒度 + 深前缀 | ✅ 采用 | 已在做，保留 |
| `group` 品牌族继承 | ✅ 采用 | 本方案核心之一 |
| 有序候选 + 行过滤 | ✅ 采用 | 升级为 `candidate + when + skip` |
| 代码逃生口 | ✅ 采用 | `custom = "fn"`，编译期注册 |
| YAML 目录作为运行时数据源 | ❌ 不采用 | 改为自有 TOML，单一格式 |
| PHP 类继承体系 | ❌ 不采用 | 用 Rust 模块 + 配置继承替代 |
| 跟随上游同步 | ❌ 不采用 | 一次性导入后自维护 |
| 上游全量 OS 覆盖 | ❌ 不追求 | 只维护实际用到的品牌，未知设备走发现流程 |

**一句话：抄它的分层思想，不抄它的数据管道。**

---

## 四、配置化边界：执行清单与吸收清单

### 4.1 判据

一条能力能不能配置化，问三个问题：

1. 单表或单列能否取到？
2. 需不需要跨表 join？
3. 需不需要跨轮次状态？

三条全过 → 配置化；任一不过 → 吸收进代码。

### 4.2 配置化执行清单（本版全部落地）

| 能力 | 配置表达能力 | 说明 |
|------|-------------|------|
| CPU | 多候选 + 多列 + 取最大值 | 覆盖 Cisco/Huawei/H3C/Juniper 等 |
| 内存 | pool（used/free/name）/ entity（usage+size）/ percent | 单位：B/KB/MB/块 |
| 温度 | 单列/多列 + 单位 + 精度 | 摄氏度/毫度/华氏 |
| 存储 | pool / hrStorage | 与内存同构 |
| 传感器 | 温度/电压/电流/功率/风扇/状态/百分比/频率/负载 | 一次做全，不限于温度 |
| 设备信息 | 型号/序列号/版本 OID 来源 + 版本正则 | 替代 LibreNMS `os` 模块的声明部分 |
| 行过滤 | `skip`（字段 + 比较符 + 值） | 跳过 I/O 池、哨兵值 |
| 阈值 | `limits`（low/warn/high） | 供告警评估 |
| 状态映射 | `states`（数值 → 语义） | 风扇/电源/温度状态 |
| 索引策略 | 单索引 / 复合索引 / 探测期缓存 | 覆盖 entity 表 |
| 条件 | `when`（型号/版本/前缀/前置 OID） | 解决型号与年份差异 |

### 4.3 代码吸收清单（无法配置化的部分）

「吸收」的定义：**用 Rust 重写该能力的判定规则，保留语义，不保留 PHP 的代码结构。**
每个吸收模块必须有独立单测 + 至少一台真实设备验证。

| 模块 | 上游规模 | 吸收范围 | 不吸收 |
|------|----------|----------|--------|
| entity-physical | 28 个 PHP 类 | 标准 ENTITY-MIB + Cisco/Huawei/H3C 的 class/name 映射 | 其余厂商的私有扩展 |
| transceivers | 28 个 PHP 类 | 标准 ENTITY-SENSOR + Cisco + Huawei 三套 | 其余厂商 |
| 设备状态 | 30 个 `pollOS` | 重启、电源、风扇、HA/堆叠状态 | 厂商专有业务状态 |
| VLAN | 9 个 PHP 类 | 现有 `vlan.rs` 补厂商差异 | — |
| 端口/流量 | 核心代码 | 现有 `ports.rs` 保留 | — |
| LLDP / CDP | 核心代码 | 现有 `lldp.rs` / `cdp.rs` 保留 | — |
| ARP / FDB | 核心代码 | 现有 `arp.rs` 保留 | — |
| PoE | 核心代码 | 现有 `poe.rs` 保留 | 非标准私有功率表 |
| LAG | 核心代码 | 现有 `lag.rs` 保留（含华为变体列） | — |

### 4.4 明确不做的清单

以下能力**不吸收、不配置**，避免范围失控：

- 无线（客户端数、射频、AP 统计）——上游 50 处 `discoverWireless*`
- BGP / OSPF / VRF / 路由表
- 应用监控（applications）、服务检查（services）
- SLA / NAC / QoS
- 告警平台对接类模块

这些属于「监控平台」范畴，不属于「SNMP 设备台账与资源采集」。
如果未来要做，单独立项，不塞进本模块。

### 4.5 为什么不做 PHP 逐行翻译

- 302 个 PHP 类约 42,587 行，逐行翻译的产出是「能跑的垃圾」，无法维护；
- 大量逻辑是历史兼容（旧型号、已停产设备），FOIMS 没有对应设备就不该背；
- 吸收的正确粒度是**能力**（如「读思科光模块四元组」），不是**类**。

---

## 五、目标架构

### 5.1 分层与依赖方向

```
live/            调度、快照、告警喂料（不含任何 OID 知识）
  │
collect/         采集实现
  ├─ resource/   配置驱动：解释 CollectPlan（cpu/mem/temp/storage/sensors/device）
  └─ topology/   代码驱动：ports/vlan/lldp/cdp/arp/poe/entity/transceiver/lag
  │
vendor/          纯数据：模型 + 注册表 + 匹配（无 IO、无 async）
```

依赖只能向下，`vendor` 不依赖 `collect`。这条约束保证「配置」和「采集实现」不会互相渗透。

### 5.2 数据模型

```rust
pub struct OsProfile {
    pub id: &'static str,
    pub kind: ProfileKind,          // Group | Os
    pub group: Option<&'static str>, // 仅 kind=Os
    pub brand: &'static str,        // cisco / huawei / h3c / generic …
    pub display_name: &'static str,
    pub matcher: Matcher,           // kind=Group 时为空
    pub collect: CollectPlan,
    pub source: &'static str,       // 仅日志：来自哪个文件
}

pub struct Matcher {
    pub sysobjectid: Vec<Vec<u32>>, // 前缀弧序列
    pub descr_keywords: Vec<String>,
    pub descr_regex: Vec<Regex>,    // 启动期编译，失败即启动失败
    pub probes: Vec<Probe>,         // { oid, op, value }
}

pub struct CollectPlan {
    pub cpu: Option<CpuPlan>,
    pub mem: Option<MemPlan>,
    pub temp: Option<TempPlan>,
    pub storage: Option<StoragePlan>,
    pub sensors: Option<SensorPlan>,
    pub device: Option<DevicePlan>,
}

/// 识别结果：采集入口 + 展示/平台信息。
/// 不再携带裸字符串 id；`confidence` / `evidence` 当前无消费方，随重构删除。
pub struct Identity {
    pub profile: Option<&'static OsProfile>, // None = 走 generic
    pub brand: &'static str,
    pub platform: Option<&'static str>,
    pub os_version: Option<String>,
    pub caps: PlatformCaps,                  // 公共回退链的能力声明
}
```

`CollectPlan` 的每个族都是 `Option`：`None` 表示「本层不表态」，继续向 `group` 找。
继承因此是类型系统的一部分，不需要一堆 `if let`。

`Identity` 里 `profile` 直接进采集；`brand` / `platform` / `os_version` 用于展示与日志；
`caps` 只在回落 generic 时用来跳过已知缺失的 MIB 族。

`ProfileKind` 只有两个值：`Group`（品牌族，无 matcher）和 `Os`（可识别）。
不再有 `Origin` 枚举参与合并决策，`source` 只用于日志。

### 5.3 注册表加载

```
启动
 ├─ 读内置 vendor/profiles/*.toml        （编译期 include_str!，完整数据）
 ├─ 读 /etc/foims/snmp-profiles.d/*.toml （可选，按文件名排序）
 │    同一 id → 整条替换；enabled=false → 删除；新 id → 追加
 ├─ 校验：id 唯一、group 存在且无环、前缀冲突、正则可编译、OID 合法
 └─ 冻结为 LazyLock<Registry>（只读）
```

三条硬规则：

1. **不做字段级合并。** 现场要改一条 profile，就整条给它，或者直接 `enabled = false`。
   可预测性优先于便利性。
2. **配置错误 = 启动失败。** 打印文件名、行号、字段路径。
   不再「解析失败静默回退内置」——那会让错误配置长期潜伏。
3. **不缓存到磁盘。** 每次启动重新解析，量级毫秒，无状态。

### 5.4 匹配语义

```
1. 所有 kind=os 且 sysobjectid 前缀命中的 profile，取前缀最长者
2. 同长度多命中 → descr_keywords 命中词数多者
3. 仍并列 → 有 descr_regex 命中者
4. 仍并列 → id 字典序（保证确定性）
5. 静态证据全空 → probes 逐个 GET，首个命中者
6. 仍无 → None（按 generic 组采集）
```

匹配结果封装为 `Identity`，其中 `profile` 直接进采集；`brand` 用于
`devices.brand`、日志、前端展示。字符串 id 不再作为采集索引键。

### 5.5 三级解析

```
resolve("iosxe")
  ├─ iosxe.collect.cpu = Some  → 用它
  ├─ None → cisco.collect.cpu  → 用它
  └─ None → generic.collect.cpu → 公共链
```

每个族支持三种模式：

| 模式 | 语义 |
|------|------|
| `inherit`（默认） | 本层未声明则向上继承 |
| `replace` | 本层声明，整体替换父族，不拼接候选 |
| `disable` | 本层显式关闭该族，不再向上找 |

`disable` 是现场最常用的：某型号内存表返回垃圾值，只关内存，不动 CPU/温度。

### 5.6 配置驱动引擎

一个解释器，两种调用方（秒级实时、5 分钟趋势）共用：

```rust
pub struct ResourceReader<'a, R: SnmpReader> {
    reader: &'a R,
    plan: &'a CollectPlan,
    // 探测期缓存的索引（entity 表等）
    index_cache: &'a IndexCache,
}

impl<R: SnmpReader> ResourceReader<'_, R> {
    pub async fn cpu(&self) -> Vec<CpuReading>;
    pub async fn mem(&self) -> Option<MemReading>;
    pub async fn temp(&self) -> Vec<TempReading>;
    pub async fn storage(&self) -> Vec<StorageReading>;
    pub async fn sensors(&self) -> Vec<SensorReading>;
}
```

解释器只认 6 个原语：

| 原语 | 含义 |
|------|------|
| `Column { oid }` | walk 单列，按索引聚合 |
| `Table { index, columns }` | walk 多列，按索引对齐成行 |
| `Scalar { oid }` | GET 单值 |
| `Indexed { oid, index_from }` | 用探测期缓存索引做定向 GET |
| `Filter` | 行级过滤（字段 + 比较符） |
| `Transform` | 单位/精度/状态映射 |

超出这 6 个原语的，一律 `custom = "fn_name"`，不允许在配置里写表达式。

### 5.7 目录结构

```
crates/foims-snmp/src/
  vendor/
    model.rs          # OsProfile / Matcher / CollectPlan / 各 Plan
    registry.rs       # 加载、覆盖、校验、冻结
    identify.rs       # 匹配纯函数
    profiles/
      mod.rs          # include_str! 清单（一品牌一行）
      generic.toml
      cisco.toml
      huawei.toml
      h3c.toml
      juniper.toml
      ...
  collect/
    mod.rs
    walk.rs           # 共享 walk 原语（超时、GETBULK 降级）
    sysinfo.rs        # 系统组，识别输入
    resource/
      mod.rs          # ResourceReader
      cpu.rs mem.rs temp.rs storage.rs sensors.rs device.rs
      transform.rs    # 单位/精度/状态映射
    topology/         # 原 collect/*.rs 平移，逻辑不变
      ports.rs vlan.rs lldp.rs cdp.rs arp.rs poe.rs entity.rs
      transceiver.rs lag.rs traffic.rs
  live/
    mod.rs            # 快照类型
    collector.rs      # 调度，调用 collect::resource / topology
```

`vendor/profiles/mod.rs` 的清单是显式的：

```rust
pub(super) const FILES: &[(&str, &str)] = &[
    ("generic", include_str!("generic.toml")),
    ("cisco",   include_str!("cisco.toml")),
    ("huawei",  include_str!("huawei.toml")),
    // 新增品牌：加一行 + 一个文件
];
```

不用 `build.rs`、不用目录扫描、不用宏。加品牌就是加一行，读代码的人一眼能看完。

---

## 六、配置文件规范

### 6.1 组织方式

- 一品牌一文件：`cisco.toml` 同时放 `group cisco` 和它旗下所有 OS。
- 文件内顺序：先 `group`，后各 OS。
- 单文件超过 1500 行就拆（如 `cisco-ios.toml` / `cisco-nxos.toml`），
  `mod.rs` 里多一行。
- `generic.toml` 只放公共回退链，不参与识别。

### 6.2 schema 示例

```toml
# ══════════════════════ 品牌族：共享采集方案，不参与识别 ══════════════════════
[[profile]]
id           = "cisco"
kind         = "group"
brand        = "cisco"
display_name = "Cisco"

# ── CPU：有序候选，首个「有值且满足 when」的生效 ──
[profile.collect.cpu]
max_candidates = 4
pick           = "max"          # 多行取最大值

[[profile.collect.cpu.candidate]]
[[profile.collect.cpu.candidate.column]]
oid       = "1.3.6.1.4.1.9.9.109.1.1.1.1.8"   # cpmCPUTotal5minRev
value     = "percent"
precision = 1

[[profile.collect.cpu.candidate]]
[[profile.collect.cpu.candidate.column]]
oid       = "1.3.6.1.4.1.9.9.109.1.1.1.1.7"   # cpmCPUTotal5min
value     = "percent"
precision = 1

# ── 内存：CEM → CMP → CPM 三段回退，全部声明式 ──
[[profile.collect.mem.candidate]]
kind = "pool"
[[profile.collect.mem.candidate.field]]
role = "name"
oid  = "1.3.6.1.4.1.9.9.48.1.1.1.2"
[[profile.collect.mem.candidate.field]]
role = "used"
oid  = "1.3.6.1.4.1.9.9.48.1.1.1.5"
unit = "bytes"
[[profile.collect.mem.candidate.field]]
role = "free"
oid  = "1.3.6.1.4.1.9.9.48.1.1.1.6"
unit = "bytes"
skip = [{ field = "name", op = "eq", value = "I/O" }]

# ── 温度 ──
[[profile.collect.temp.candidate]]
[[profile.collect.temp.candidate.column]]
oid  = "1.3.6.1.4.1.9.9.13.1.3.1.3"
unit = "celsius"

# ══════════════════════ OS：只写差异 ══════════════════════
[[profile]]
id           = "iosxe"
kind         = "os"
group        = "cisco"
brand        = "cisco"
display_name = "Cisco IOS-XE"

[profile.match]
sysobjectid    = ["1.3.6.1.4.1.9.1.1045"]
descr_keywords = ["IOS-XE", "IOS XE"]

# 继承 cisco 的 CPU/内存，只覆盖温度
[profile.collect.temp]
mode = "replace"

[[profile.collect.temp.candidate]]
[[profile.collect.temp.candidate.column]]
oid  = "1.3.6.1.4.1.9.9.13.1.3.1.3"
unit = "celsius"

# ══════════════════════ 型号差异：显式 when ══════════════════════
[[profile]]
id           = "ios"
kind         = "os"
group        = "cisco"
brand        = "cisco"
display_name = "Cisco IOS"

[profile.match]
sysobjectid = ["1.3.6.1.4.1.9.1.1"]

# 老 IOS 优先用 5min，不支持 Rev 列
[[profile.collect.cpu.candidate]]
when = { max_version = "12.0" }
[[profile.collect.cpu.candidate.column]]
oid       = "1.3.6.1.4.1.9.9.109.1.1.1.1.7"
value     = "percent"
precision = 1

```

现场覆盖是**独立文件**，不与内置文件混写：

```toml
# /etc/foims/snmp-profiles.d/10-iosxe.toml
[[profile]]
id      = "iosxe"
kind    = "os"
group   = "cisco"
brand   = "cisco"
enabled = false        # 整条停用，回落到 generic
```

### 6.3 字段语义

| 字段 | 取值 | 说明 |
|------|------|------|
| `kind` | `group` / `os` | 族不参与识别 |
| `group` | 已存在的 group id | 仅 `kind=os` |
| `brand` | 字符串 | 展示、统计、前端字段 |
| `enabled` | `true`（默认）/ `false` | `false` 在加载后删除该条 |
| `match.sysobjectid` | 弧序列数组 | 最长前缀胜出 |
| `match.descr_keywords` | 字符串数组 | 词元精确匹配 |
| `match.descr_regex` | 正则数组 | Rust `regex` 语法 |
| `match.probes` | `{ oid, op, value }` | `op` 支持 `eq` / `ne` / `has` |
| `collect.*.mode` | `inherit` / `replace` / `disable` | 默认 `inherit` |
| `collect.*.max_candidates` | 整数 | 替代原 `MAX_DISCOVERY_TEMP_RULES` |
| `collect.*.pick` | `max` / `first` / `sum` | 多行聚合策略 |
| `candidate.when` | 见下表 | 不满足则跳过该候选 |
| `candidate.column.value` | `percent` / `bytes` / `celsius` / `count` | 语义标记 |
| `candidate.column.unit` | `bytes` / `kb` / `mb` / `blocks` / `celsius` / `milli` / `micro` / `fahrenheit` | 单位 |
| `candidate.column.divisor` | 整数 | 除法换算 |
| `candidate.column.multiplier` | 整数 | 乘法换算 |
| `candidate.column.precision` | 整数 | 小数位 |
| `candidate.skip` | `{ field, op, value }` | 行过滤 |
| `candidate.limits` | `{ low, warn, high }` | 阈值 |
| `candidate.states` | 数值 → 字符串 | 状态映射 |
| `candidate.custom` | 函数名 | 逃生口 |

`when` 支持的条件（封闭集合）：

| 条件 | 含义 |
|------|------|
| `min_version` / `max_version` | 按识别出的固件版本比较 |
| `model_regex` | 按设备型号匹配 |
| `sysobjectid_prefix` | 按 sysObjectID 前缀匹配 |
| `require_oid` | 该 OID 必须存在 |
| `require_value` | 该 OID 必须等于指定值 |

### 6.4 校验规则（启动期，失败即退出）

| 规则 | 级别 |
|------|------|
| `id` 全局唯一（覆盖后） | 错误 |
| `kind=os` 必须有 `group` 或 `brand` | 错误 |
| `group` 必须指向存在的 `kind=group` | 错误 |
| `group` 引用无环 | 错误 |
| `sysobjectid` 前缀完全重复 | 错误 |
| 前缀存在包含关系 | 告警（合法但需确认） |
| 正则编译失败 | 错误 |
| OID 非点分数字 | 错误 |
| `custom` 函数名未注册 | 错误 |
| 单 profile 候选数 > 16 | 告警 |

### 6.5 维护流程

| 场景 | 操作 |
|------|------|
| 新增品牌 | 加 `profiles/<brand>.toml` + `mod.rs` 一行 |
| 新增 OS | 在同品牌文件加一条 `[[profile]]` |
| 修正 OID | 改对应 `candidate`，跑 `cargo test -p foims-snmp` |
| 型号差异 | 加 `when` 条件，不要改默认候选 |
| 现场例外 | `/etc/foims/snmp-profiles.d/<name>.toml` 整条替换或 `enabled=false` |
| 退役型号 | 删除该 `[[profile]]`，同步删测试样本 |

**未知设备发现**：识别未命中时，把 `sysObjectID + sysDescr` upsert 到
`snmp_unknown_devices` 表（首见/末见/次数）。运维定期看这张表决定补哪些品牌，
替代「跟随上游自动获得覆盖」的旧模式。

---

## 七、破坏性变更清单

| 变更 | 类型 | 影响 |
|------|------|------|
| `/etc/foims/vendor_profiles.toml` → `/etc/foims/snmp-profiles.d/*.toml` | 路径 + 格式 | 现场配置需一次性重写 |
| 配置文件从「字段合并」改为「整条替换」 | 语义 | 覆盖更可预测，但需复制整条 |
| 配置错误从「回退内置」改为「启动失败」 | 行为 | 错误配置不再潜伏 |
| `identify()` 返回 `Identity`（含 `profile` 引用），不再返回裸字符串 id | API | 调用方全部改 |
| `VendorKind` 删除，改用 `brand: &'static str` | API | 载荷字段值不变，类型变 |
| `vendor/community/` 删除 | 代码 | 808 条规则并入 TOML |
| `vendor/os_discovery.rs` 删除 | 代码 | 404 条规则并入 TOML |
| `vendor/discovery.rs` / `profiles.rs` / `profiles_default.toml` 删除 | 代码 | 由 `model.rs` / `registry.rs` / `profiles/` 替代 |
| `KIND_BY_OS` 删除 | 代码 | 由 `group` 替代 |
| `vendor/probe.rs` 合并进 `identify.rs` | 代码 | 探测 OID 由 `match.probes` 提供 |
| `gen_vendor_community.py` / `gen_vendor_discovery.py` 删除 | 工具 | 不再同步上游 |
| `MAX_DISCOVERY_TEMP_RULES` 删除 | 常量 | 改 `max_candidates` |
| `live/profile.rs` 硬编码公共 MIB 删除 | 代码 | 归入 `generic.toml` |
| `collect/metrics.rs` 拆解 | 代码 | 资源部分进 `collect/resource/`，流量部分进 `collect/topology/traffic.rs` |

---

## 八、实施步骤（一次性切换，无兼容层）

每一步是一个可独立 review 的提交；第 3、4 步必须在同一个发布里完成，
中间状态不可发布。

### Step 1：模型与注册表骨架

**产出**
- `vendor/model.rs`、`vendor/registry.rs`、`vendor/identify.rs`（重写）
- `vendor/profiles/generic.toml`（手写，作为 schema 样板）
- schema 校验器 + 单测

**退出条件**
- `generic.toml` 能加载、校验、冻结
- 构造 8 类非法配置，全部在启动期报错且信息可定位

### Step 2：一次性数据导入

**产出**
- `da-scripts/import-os-profiles.py`（**一次性脚本，导入后归档，不维护**）
- 从 `os_discovery.rs` + `community/` + `profiles_default.toml` 生成
  `profiles/cisco.toml`、`huawei.toml`、`h3c.toml`、其余品牌文件
- 导入报告：条目数、丢弃项、空证据项、无法转换项

**退出条件**
- 404 条采集规则、808 条识别规则全部有归属或显式记录丢弃原因
- 抽取 20 台真实设备的 sysObjectID 做识别抽查，结果与旧 `identify()` 一致
- 导入脚本归档到 `da-scripts/archive/`，README 标注「不再维护」

### Step 3：识别切换

**产出**
- `identify()` 改为返回 `Identity`（`profile` 字段即采集入口）
- 删除 `vendor/community/`、`vendor/os_discovery.rs`、`vendor/discovery.rs`、
  `vendor/profiles.rs`、`vendor/profiles_default.toml`、`KIND_BY_OS`
- `live/profile.rs` 改为调用 `registry()`
- `sysinfo.rs` 的 `brand` 改取 `profile.brand`

**退出条件**
- 全仓 grep 不到旧类型与旧文件
- `cargo clippy --release --workspace --all-targets -- -D warnings` 通过

### Step 4：采集切换

**产出**
- `collect/resource/` 解释器接管 cpu / mem / temp / storage / sensors / device
- `live/collector.rs` 与 `snmp_poll.rs` 共用 `ResourceReader`
- 删除 `live/profile.rs` 的硬编码探测函数
- 5 分钟趋势路径接入资源组

**退出条件**
- 同一设备在同一时间窗口，实时与趋势的 CPU/内存/温度差异在允许误差内
- 三台真实设备（Cisco/Huawei/H3C）趋势库 CPU/内存非空

### Step 5：复杂逻辑吸收

**产出**
- `collect/topology/entity.rs`、`transceiver.rs`、`device_state.rs`
- 原 `collect/*.rs` 平移到 `collect/topology/`

**退出条件**
- 每个吸收模块有独立单测 + 一台设备验证
- 不吸收清单中的能力确认未引入

### Step 6：清理与文档

**产出**
- 删除 `VendorKind`，统一 `brand` 字符串
- 更新 `docs/监控采集与通知模块架构.md`
- 运维手册：新增品牌 / 修正 OID / 现场覆盖 / 回滚
- `snmp_unknown_devices` 表 + 初始化 SQL + `foims-init` 同步

**退出条件**
- 仓库中不存在第二套采集 OID 数据
- 新同事按手册能在 30 分钟内新增一个 OS

---

## 九、测试

### 9.1 配置层

| 测试 | 内容 |
|------|------|
| 解析 | 每个字段的最小/最大/非法值 |
| 校验 | 重复 id、group 缺失、group 环、前缀冲突、正则失败、OID 非法、custom 未注册 |
| 覆盖 | 同 id 整条替换、`enabled=false`、新 id 追加 |
| 继承 | `inherit` / `replace` / `disable` 三态 |
| 匹配 | 最长前缀、同长并列、关键词计数、正则、probe、全不命中 |

### 9.2 设备金标

新增 `crates/foims-snmp/tests/fixtures/devices/*.json`：

```json
{
  "name": "cisco-c9300",
  "sys_object_id": "1.3.6.1.4.1.9.1.2494",
  "sys_descr": "Cisco IOS Software, C9300 Software ...",
  "expected": { "brand": "cisco", "profile": "iosxe" },
  "oids": { "1.3.6.1.4.1.9.9.109.1.1.1.1.8.1": 17 }
}
```

用现有 `SnmpReader` 的内存实现跑解释器，断言输出。
覆盖：Cisco IOS / IOS-XE、Huawei VRP、H3C Comware、Fortigate、Juniper、Ruijie。

### 9.3 一次性导入校验

导入脚本运行后，用旧代码对新旧结果做一次对比（脚本内完成，不进入 CI）。
报告存档到 `docs/` 或 PR 描述，**只作为迁移期证据，不作为架构约束**。

### 9.4 回归

- `cargo fmt`
- `cargo clippy --release --workspace --all-targets -- -D warnings`
- `cargo test -p foims-snmp`
- 若载荷变更：`bash da-scripts/gen-ts-types.sh` + `cd frontend && npm run typecheck`

---

## 十、风险与对策

| 编号 | 风险 | 影响 | 对策 |
|------|------|------|------|
| R1 | 一次性导入遗漏字段 | 高 | 导入报告逐项对账；20 台设备识别抽查；空证据条目显式标记而非静默丢弃 |
| R2 | 失去上游同步，新设备无人支持 | 高 | `snmp_unknown_devices` 表 + 定期汇总；新增品牌成本压到「一个 TOML 文件」 |
| R3 | 现场配置格式不兼容 | 中 | 提供一次性转换脚本；发布说明明确写「不兼容」；保留上一个 release tag 可回滚 |
| R4 | 一次性切换无灰度 | 高 | 3、4 步同发布；发布前用设备金标全量跑；保留旧二进制与旧配置备份 |
| R5 | 解释器逐渐变成表达式语言 | 中 | 6 个原语 + 封闭 `when` 条件；新增能力优先 `custom` |
| R6 | 吸收模块工作量超预期 | 中 | 吸收清单已限定 4 个模块；不吸收清单明确排除无线/BGP |
| R7 | 配置体积导致启动变慢 | 低 | 404 条量级实测；超 200ms 再考虑 `build.rs`，接口不变 |
| R8 | 整条替换导致现场配置冗长 | 低 | 现场覆盖只用于例外，不是常规扩展手段；文档明确优先级 |

---

## 十一、验收标准

1. **单一数据源**：仓库中只有 `vendor/profiles/*.toml` 一处采集 OID 数据，
   不存在生成物、不存在第二套识别表。
2. **新增 OS 零改码**：新增一个已知品牌的 OS，只加 TOML。
3. **同源采集**：秒级实时与 5 分钟趋势走同一个 `ResourceReader`。
4. **配置可诊断**：任何配置错误在启动期失败，输出文件、行号、字段路径。
5. **未知设备可观测**：识别未命中有记录、可查询、可统计。
6. **删除清单清零**：第七章列出的文件、类型、脚本全部不存在。
7. **工程质量**：`fmt` / `clippy -D warnings` / 单测 / 类型生成全部通过。

---

## 附录 A：LibreNMS 模块对照全表

| LibreNMS 能力 | 上游实现 | FOIMS 现状 | 本方案 |
|---------------|----------|-----------|--------|
| os 基础信息 | PHP `discoverOS`（93 处）+ YAML | `sysinfo.rs` | 配置（OID 来源 + 版本正则） |
| processors（CPU） | YAML + Shared PHP 回退 | `os_discovery` + `profiles` | 配置 |
| mempools | YAML + Shared PHP 回退 | 同上 | 配置 |
| storage | YAML | 无 | 配置 |
| sensors | YAML + `pollSensors`（17 处） | 仅温度 | 配置（全类型） |
| entity-physical | PHP（28 处） | `entity.rs` | 代码吸收 |
| transceivers | PHP（28 处） | 无 | 代码吸收（3 套） |
| 端口/流量 | 核心代码 | `ports.rs` | 代码（保留） |
| VLAN | PHP（9 处） | `vlan.rs` | 代码（保留） |
| LLDP / CDP | 核心代码 | `lldp.rs` / `cdp.rs` | 代码（保留） |
| ARP / FDB | 核心代码 | `arp.rs` | 代码（保留） |
| PoE | 核心代码 | `poe.rs` | 代码（保留） |
| LAG | 核心代码 | `lag.rs` | 代码（保留） |
| 设备状态 | `pollOS`（30 处） | 部分 | 代码吸收（4 类） |
| BGP / OSPF / VRF | PHP | 无 | **不做** |
| 无线 | PHP（50 处） | 无 | **不做** |
| applications / services | PHP | 无 | **不做** |
| SLA / NAC / QoS | PHP | 无 | **不做** |

## 附录 B：为什么「整条替换」比「字段合并」好

字段合并看起来方便，实际带来三个问题：

1. **优先级不可见**：现场改了内存，内置改了 CPU，最终生效的是哪个文件的哪个字段，
   只能靠读代码推断。
2. **校验困难**：合并后的结果不在任何单个文件里，报错无法定位到行。
3. **升级冲突**：内置结构变化时，现场的部分字段可能语义已经变了，但合并仍会成功，
   产生「能启动但采错数」的隐性故障。

整条替换把「现场到底覆盖了什么」变成可枚举的事实：
`ls /etc/foims/snmp-profiles.d/` 就是全部答案。
代价是现场配置更长，但现场覆盖本来就应该是例外，不是常态。

## 附录 C：术语对照

| 本文 | 对应 |
|------|------|
| Profile | 一条 `[[profile]]`，即一个 OS 或品牌族的完整画像 |
| 品牌族 | `kind = "group"`，对应 LibreNMS 的 `group` |
| 候选 | `candidate`，对应 YAML 的 `data` 数组项 |
| 生效条件 | `when`，对应 PHP 里的 if 判断 |
| 逃生口 | `custom`，对应 LibreNMS `user_func` |
| 吸收 | 用 Rust 重写能力，不翻译 PHP 代码 |
