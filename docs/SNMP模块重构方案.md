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

识别由「规则表 + 聚合器」两层构成，重构时分别处置：

- `vendor/community/` 共 808 条规则（内置品牌 16 + 社区 OS 792），
  来自 LibreNMS 上游 822 个定义（其中 29 条属平台域，见下）；
  全部并入 `profiles/*.toml` 的 `[profile.match]`。
- `vendor/platform.rs` 29 条平台规则（sysDescr 强证据 + 企业号弱证据、
  net-snmp 8072 排除、每平台版本解析器）：词表与版本正则转 `device`
  采集配置；caps 推导保留为识别层吸收代码（caps 是被消费的：
  探测/采集逐族依赖它裁剪公共链，Windows 等不再空走）。
- `identify.rs` 聚合逻辑（双证据聚合、冲突时描述词胜出）：
  吸收进新 `vendor/identify.rs`（语义见 §5.4）。
- `KIND_BY_OS` 29 条映射到 4 个 `VendorKind`：由 `group` 归属替代，
  `KIND_BY_OS` 删除。

### 2.3 问题清单

| 编号 | 问题 | 本版如何根治 |
|------|------|-------------|
| P1 | 识别只返回字符串，采集再查三次表 | `identify()` 返回 `Identity`，`profile` 直接进采集 |
| P2 | 四套来源优先级写死在 `probe()` 的 if 顺序 | 只剩「内置 + 现场覆盖」两层，规则写进文档 |
| P3 | 同品牌多 OS 无继承，各配一份 | `group` 品牌族，字段级继承 |
| P4 | 秒级与 5 分钟两套 OID 来源 | 两条链路共用同一个解释器 |
| P5 | 型号/版本差异靠「列空就换」 | 有序候选 + 运行期有值即生效（机制吸收进解释器，不做条件语言） |
| P6 | `MAX_DISCOVERY_TEMP_RULES = 12` 截断丢传感器 | 取消截断：候选数显式可审 + 启动期告警 |
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
| 有序候选 + 行过滤 | ✅ 采用 | 升级为 `candidate + skip`，运行期回退见 §5.5 |
| 代码逃生口 | ✅ 采用 | `custom = "fn"`，编译期注册 |
| YAML 目录作为运行时数据源 | ❌ 不采用 | 改为自有 TOML，单一格式 |
| PHP 类继承体系 | ❌ 不采用 | 用 Rust 模块 + 配置继承替代 |
| 跟随上游同步 | ❌ 不采用 | 一次性导入后自维护 |
| 上游全量 OS 覆盖 | ✅ 采用 | 存量全量导入不裁剪；新增品牌按需维护，未知设备走发现流程 |

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
`match.probes` 的 GET 执行在采集层（`sysinfo.rs` / live 探测入口）完成，
`vendor` 只声明 probe 清单、消费执行结果——`vendor` 保持无 IO。

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
/// `profile` 非 Option：generic 是常驻注册表的兜底 profile，识别失败直接返回它。
pub struct Identity {
    pub profile: &'static OsProfile,         // 识别失败 = generic 兜底 profile
    pub brand: &'static str,
    pub platform: Option<&'static str>,
    pub os_version: Option<String>,
    pub caps: PlatformCaps,                  // 公共回退链的能力声明（平台域吸收代码推导）
}
```

`CollectPlan` 的每个族都是 `Option`：`None` 表示「本层不表态」，继续向 `group` 找。
继承因此是类型系统的一部分，不需要一堆 `if let`。
运行期消费的是解析后的 `ResolvedCollectPlan`（§5.5），不是本结构。

`Identity` 里 `profile` 直接进采集；`brand` / `platform` / `os_version` 用于展示与日志；
`caps` 只在回落 generic 时用来跳过已知缺失的 MIB 族。
`profile` 非 `Option`：generic 是常驻注册表的兜底 profile，采集层不再有 None 分支。

`ProfileKind` 只有两个值：`Group`（品牌族，无 matcher）和 `Os`（可识别）。
不再有 `Origin` 枚举参与合并决策，`source` 只用于日志。

### 5.3 注册表加载

```
启动
 ├─ 读内置 vendor/profiles/*.toml        （编译期 include_str!，完整数据）
 ├─ 读 /etc/foims/snmp-profiles.d/*.toml （可选，按文件名排序）
 │    同一 id → 整条替换；enabled=false → 删除；新 id → 追加
 ├─ 校验：id 唯一、group 存在（group 不嵌套）、generic 为保留 id、前缀冲突、正则可编译、OID 合法
 └─ 冻结为 LazyLock<Registry>（只读）
```

三条硬规则：

1. **不做字段级合并。** 现场要改一条 profile，就整条给它，或者直接 `enabled = false`。
   可预测性优先于便利性。
2. **配置错误 = 启动失败。** 打印文件名、行号、字段路径。
   不再「解析失败静默回退内置」——那会让错误配置长期潜伏。
3. **不缓存到磁盘。** 每次启动重新解析，量级毫秒，无状态。
4. **加载结果落日志。** 逐条输出 `profile <id>: source=<文件> action=replaced/disabled/added`，
   现场排查覆盖是否生效直接看启动日志。

所有权模型：`Registry` 拥有全部数据（`Vec<OsProfile>` + 索引），整体冻在
`static LazyLock<Registry>`——静态项内部的数据本身长期存活，经 `LazyLock`
取得的引用即 `'static`，不逐条 `Box::leak`。

### 5.4 匹配语义（双通道聚合，吸收现行 identify() 语义）

前缀通道（sysObjectID）与描述通道（sysDescr）独立匹配，再聚合：

```
前缀通道：kind=os 且 sysobjectid 前缀命中
  ├─ 取前缀最长者；同长并列 → descr_keywords 命中词数多者
  ├─ 仍并列 → 有 descr_regex 命中者；仍并列 → id 字典序（确定性）
描述通道：descr_keywords 词元命中或 descr_regex 命中
  ├─ 按注册表顺序首个命中者（顺序 = 内置 FILES 顺序 + 文件内声明顺序；
  │    现场覆盖同 id 保持原位、新 id 按文件名排序追加——写死并测试）
聚合：
  ├─ 双通道一致 → 该 profile（双证据）
  ├─ 冲突 → 描述通道胜出（OEM 设备 sysObjectID 不可信，
  │    沿用现行语义：11 + comware → H3C）
  ├─ 仅其一命中 → 该 profile
  ├─ 双空 → probes 按 `match.probes` 声明顺序逐个 GET，首个命中者
  │    （GET 执行在采集层，vendor 只做给定结果后的裁决）
  └─ 仍无 → generic 兜底 profile（caps 由平台域推导）
```

匹配结果封装为 `Identity`，其中 `profile` 直接进采集；`brand` 用于
`devices.brand`、日志、前端展示。字符串 id 不再作为采集索引键。
`confidence`/`evidence` 字段删除，但冲突、probe 命中、降级等裁决过程
走 `tracing` 日志，保证可诊断。

### 5.5 三级解析（静态声明 + 运行期回退）

族语义分两层，缺一不可：

**静态层**（profile 解析时）：本层未声明的族继承父族声明，再无则到 generic。
解析产物是独立的 `ResolvedCollectPlan`，`ResourceReader` 只消费解析后的链，
不接触原始 `OsProfile` / `CollectPlan`（`replace` / `disable` 在解析期裁剪链）：

```rust
pub struct ResolvedFamily<T> {
    pub mode: CollectMode,
    pub candidates: Vec<ResolvedCandidate<T>>, // 本层在前，父族次之，generic 最后
}

pub struct ResolvedCollectPlan {
    pub cpu: ResolvedFamily<CpuPlan>,
    pub mem: ResolvedFamily<MemPlan>,
    // temp / storage / sensors / device 同构
}
```

`generic` 是特殊的全局回退层：常驻注册表、无 matcher、不可停用、
不参与 `group` 归属（任何 profile 的 `group` 不得指向 generic）。

**运行期层**（采集时）：`inherit`（默认）下本层候选在前、父族候选作运行期
回退——walk 空则继续试下一级，贯穿到 generic 公共链。这是现行「探测为空
不阻断，继续后续层」语义的配置化重述（LibreNMS 叠加发现语义），
`KIND_BY_OS` 绑定 OS 的回退兜底由此保留。

```
resolve("iosxe").cpu
  ├─ inherit：iosxe 候选 → walk 空 → cisco 组候选 → walk 空 → generic 公共链
  ├─ replace：仅 iosxe 候选（generic 公共链仍是最后兜底）
  └─ disable：该族关闭，不采集
```

每个族支持三种模式：

| 模式 | 语义 |
|------|------|
| `inherit`（默认） | 本层候选在前，父族候选作运行期回退，贯穿到 generic |
| `replace` | 仅本层候选，不向父族运行期回退 |
| `disable` | 该族彻底关闭 |

`disable` 是现场最常用的：某型号内存表返回垃圾值，只关内存，不动 CPU/温度。

语义矩阵（Step 1 测试用例来源）：

| 本层声明 | mode | 生效候选链 |
|---|---|---|
| 有 | `inherit`（默认） | 本层 → 父族（若有）→ generic 公共链 |
| 无 | `inherit`（默认） | 父族（若有）→ generic 公共链 |
| 有 | `replace` | 仅本层 → generic 公共链（最后兜底） |
| 无 | `replace` | 启动失败：replace 表达隔离意图，无候选即配置错误 |
| 有/无 | `disable` | 空，generic 也不兜底（该族彻底关闭） |

### 5.6 配置驱动引擎

一个解释器，两种调用方（秒级实时、5 分钟趋势）共用：

```rust
pub struct ResourceReader<'a, R: SnmpReader> {
    reader: &'a R,
    plan: &'a ResolvedCollectPlan,
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

**候选执行结果**（回退判定不只看「有没有数据」，数据有效性与采集成功是两个概念）：

| 结果 | 继续下一候选 |
|------|------|
| `Success` | 否（该候选生效） |
| `Empty`（walk 无行 / GET 无值） | 是 |
| `Invalid`（哨兵值、越界、类型不符、`skip` 命中） | 是 |
| `Unsupported`（OID 无响应 / noSuchName） | 是 |
| `Error`（超时、不可达等传输层故障） | **否**——本轮该族置空并告警 |

`Error` 不是候选不支持：网络故障不得被当成回退信号放大成大量无效探测。

**统一输出模型**（解释器负责把厂商形态标准化，存储/告警/趋势不感知差异）：

```rust
pub struct ResourceReading {
    pub index: String,
    pub name: Option<String>,
    pub used: Option<u64>,      // 内部基准单位：字节
    pub free: Option<u64>,
    pub total: Option<u64>,
    pub usage_percent: Option<f64>,
}
```

单位换算在 `Transform` 原语内统一换算到基准单位，不分散在各采集模块。

**pick 语义**：`pick` 只表达**行聚合**（同一候选内多索引行 → 单读数，
如多核 CPU 取 max）；候选之间的选择固定为「首个 `Success` 候选生效」，
不配置；最终指标的汇总归消费方（告警/趋势），解释器不做第三层聚合。

超出这 6 个原语的，一律 `custom = "fn_name"`，不允许在配置里写表达式。
`custom` 边界（Step 1 固定 trait 签名与 `CollectContext`，静态白名单注册，
无动态加载）：

- 一个 custom 只负责一个明确的采集能力，输出统一族读数；
- 入参仅 `CollectContext`（reader + index_cache + 候选上下文）；
- 禁止写数据库、禁止修改 Registry / Profile；
- 必须有独立单测、超时与请求预算（防止单个 custom 放大 SNMP 请求量）；
- 禁止把厂商分支结构带进解释器（吸收的单向性：代码里的 OID 知识
  只流向配置，解释器语义不固化成厂商分支）。

### 5.7 目录结构

```
crates/foims-snmp/src/
  vendor/
    model.rs          # OsProfile / Matcher / CollectPlan / 各 Plan
    registry.rs       # 加载、覆盖、校验、冻结
    identify.rs       # 匹配纯函数（双通道聚合 + 冲突裁决 + caps 推导）
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

# ── CPU：有序候选，首个「有值」的生效 ──
[profile.collect.cpu]
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

# 继承 cisco 的 CPU/内存，只覆盖温度（声明即本层先生效；
# 默认 inherit 下 cisco 组温度候选仍作运行期回退）
[profile.collect.temp]
[[profile.collect.temp.candidate]]
[[profile.collect.temp.candidate.column]]
oid  = "1.3.6.1.4.1.9.9.13.1.3.1.3"
unit = "celsius"

# ══════════════════════ 型号差异：不做条件语言 ══════════════════════
# 老 IOS 不支持 Rev 列：候选顺序 + 运行期有值即生效天然覆盖——
# Rev 列 walk 不到自动落 5min 列（LibreNMS 运行期选择语义），
# 垃圾值/哨兵值用 skip 行过滤。不引入版本/型号条件字段。

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
| `display_name` | 字符串 | 前端展示；未声明时默认取 `id` |
| `enabled` | `true`（默认）/ `false` | `false` 在加载后删除该条 |
| `match.sysobjectid` | 弧序列数组 | 最长前缀胜出 |
| `match.descr_keywords` | 字符串数组 | 词元精确匹配 |
| `match.descr_regex` | 正则数组 | Rust `regex` 语法 |
| `match.probes` | `{ oid, op, value }` | `op` 支持 `eq` / `ne` / `has` |
| `collect.*.mode` | `inherit` / `replace` / `disable` | 默认 `inherit`：父族候选作运行期回退（§5.5） |
| `collect.*.pick` | `max` / `first` / `sum` | 行聚合：同一候选内多索引行的聚合（§5.6） |
| `candidate.column.value` | `percent` / `bytes` / `celsius` / `count` | 语义标记 |
| `candidate.column.unit` | `bytes` / `kb` / `mb` / `blocks` / `celsius` / `milli` / `micro` / `fahrenheit` | 单位 |
| `candidate.column.divisor` | 整数 | 除法换算 |
| `candidate.column.multiplier` | 整数 | 乘法换算 |
| `candidate.column.precision` | 整数 | 小数位 |
| `candidate.skip` | `{ field, op, value }` | 行过滤 |
| `candidate.limits` | `{ low, warn, high }` | 阈值 |
| `candidate.states` | 数值 → 字符串 | 状态映射 |
| `candidate.custom` | 函数名 | 逃生口（签名见 §5.6） |

### 6.4 校验规则（启动期，失败即退出）

| 规则 | 级别 |
|------|------|
| `id` 全局唯一（覆盖后） | 错误 |
| `kind=os` 必须有 `group` 或 `brand` | 错误 |
| `group` 必须指向存在的 `kind=group` | 错误 |
| `kind=group` 声明了 `group` 字段 | 错误（group 不嵌套） |
| `sysobjectid` 前缀完全重复 | 错误 |
| 前缀存在包含关系 | 信息（正常层级匹配，如企业号 → 产品线） |
| `replace` 族无任何候选 | 错误（隔离意图与空声明矛盾） |
| `group` 指向 generic，或 `id = "generic"` | 错误（generic 为保留的全局回退层） |
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
| 型号差异 | 按数据有无排序候选 / `skip` 过滤哨兵值，不要改默认候选 |
| 现场例外 | `/etc/foims/snmp-profiles.d/<name>.toml` 整条替换或 `enabled=false`；单族 disable 须整条复制后修改（模板取发布包附带的内置 TOML 副本，禁止凭记忆手写） |
| 退役型号 | 删除该 `[[profile]]`，同步删测试样本 |

**未知设备发现**：识别未命中时，把 `sysObjectID + sysDescr` upsert 到
`snmp_unknown_devices` 表（首见/末见/次数）。运维定期看这张表决定补哪些品牌，
替代「跟随上游自动获得覆盖」的旧模式。
记录走后台旁路（best-effort），入库失败仅告警，不阻塞识别与采集。

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
| `vendor/platform.rs` 删除 | 代码 | 词表/版本正则转 `device` 配置，caps 推导吸收进识别代码 |
| `vendor/probe.rs` 删除 | 代码 | probe 清单由 `match.probes` 提供，GET 执行移入采集层，裁决进 `identify.rs` |
| `gen_vendor_community.py` / `gen_vendor_discovery.py` 删除 | 工具 | 不再同步上游 |
| `MAX_DISCOVERY_TEMP_RULES` 删除 | 常量 | 取消截断，候选数显式可审 + 启动期告警 |
| `live/profile.rs` 硬编码公共 MIB 删除 | 代码 | 归入 `generic.toml` |
| `collect/metrics.rs` 拆解 | 代码 | 资源部分进 `collect/resource/`，流量部分进 `collect/topology/traffic.rs` |

---

## 八、实施步骤（一次性切换，无兼容层）

每一步是一个可独立 review 的提交；第 3、4 步必须在同一个发布里完成，
中间状态不可发布。

### Step 1：模型与注册表骨架

**产出**
- `vendor/model.rs`、`vendor/registry.rs`、`vendor/identify.rs`（重写，
  含双通道聚合与 caps 推导吸收）
- `ResolvedCollectPlan` / `ResolvedFamily` 解析模型（§5.5）与候选结果模型（§5.6）
- `vendor/profiles/generic.toml`（手写，作为 schema 样板）
- `custom` trait 与 `CollectContext` 定义（§5.6 边界约束）
- schema 校验器 + 单测

**退出条件**
- `generic.toml` 能加载、校验、冻结
- 构造 8 类非法配置，全部在启动期报错且信息可定位
- §5.5 语义矩阵全组合有测试覆盖
- 候选结果五分类（Success/Empty/Invalid/Unsupported/Error）回退语义有单测

### Step 2：一次性数据导入

**产出**
- `da-scripts/import-os-profiles.py`（**一次性脚本，导入后归档，不维护**）
- 从 `os_discovery.rs` + `community/` + `profiles_default.toml` 生成
  `profiles/cisco.toml`、`huawei.toml`、`h3c.toml`、其余品牌文件
- 导入报告（静态审计）：条目数、丢弃项、空证据项、无法转换项、
  OID 合法性、候选顺序、单位/精度映射

**退出条件**
- 404 条采集规则、808 条识别规则全部有归属或显式记录丢弃原因
- 抽取 20 台真实设备的 sysObjectID 做识别抽查，结果与旧 `identify()` 一致
- 金标设备 OID 快照（≥ 6 台，覆盖 §9.2 品牌清单）采集入库，供 Step 4 验证
- 导入脚本归档到 `da-scripts/archive/`，README 标注「不再维护」

### Step 3：识别切换

**产出**
- `identify()` 改为返回 `Identity`（`profile` 字段即采集入口）
- 删除 `vendor/community/`、`vendor/os_discovery.rs`、`vendor/discovery.rs`、
  `vendor/profiles.rs`、`vendor/profiles_default.toml`、`KIND_BY_OS`
- 删除 `vendor/platform.rs`（词表已转 `device` 配置，caps 推导移入识别代码）
- `live/profile.rs` 改为调用 `registry()`
- `match.probes` 的 GET 执行移入采集层调用方（`sysinfo.rs` / live 探测入口），
  `vendor` 恢复无 IO
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
- §9.2 行为场景清单全通过（多核 / 多 pool / 哨兵值 / 无效池 / 复合索引 /
  候选不支持 / walk 部分成功 / 同 OID 异型返回）

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
| 校验 | 重复 id、group 缺失、group 嵌套、generic 保留 id、replace 空候选、前缀冲突、正则失败、OID 非法、custom 未注册 |
| 覆盖 | 同 id 整条替换、`enabled=false`、新 id 追加 |
| 继承 | §5.5 语义矩阵全组合（inherit / replace / disable × 本层声明有无） |
| 匹配 | 最长前缀、同长并列、关键词计数、正则、probe、全不命中、双通道冲突（描述词胜出） |

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
行为场景（fixtures 须覆盖，与 Step 4 退出条件对应）：

| 场景 | 验证点 |
|------|------|
| 多 CPU 实例 / 多内存 pool | 行聚合与多行输出 |
| 温度哨兵值 / 无效存储池 | `Invalid` → 候选回退、`skip` 生效 |
| ENTITY-MIB 复合索引 | `Indexed` / `Table` 对齐 |
| 候选 OID 不支持 / walk 部分成功 | `Unsupported` / `Empty` → 回退 |
| 同一 OID 异型返回（Counter / Gauge / 空值） | `Transform` 类型处理 |
| 传输层超时 | `Error` → 该族置空，不放大探测 |

品牌覆盖：Cisco IOS / IOS-XE、Huawei VRP、H3C Comware、Fortigate、Juniper、Ruijie
（前提：存量导入有该品牌规则且真实设备快照可得，缺一移出首批清单）。

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
| R5 | 解释器逐渐变成表达式语言 | 中 | 6 个原语 + `custom` 逃生口；不引入条件/表达式字段 |
| R6 | 吸收模块工作量超预期 | 中 | 吸收清单已限定 4 个模块；不吸收清单明确排除无线/BGP |
| R7 | 配置体积导致启动变慢 | 低 | 404 条量级实测；超 200ms 再考虑 `build.rs`，接口不变 |
| R8 | 整条替换导致现场配置冗长 | 中 | 现场覆盖只用于例外；发布包附带内置 TOML 副本作覆盖模板；单族 disable 必须整条复制后改，禁止凭记忆手写 |

---

## 十一、验收标准

1. **单一数据源**：仓库中只有 `vendor/profiles/*.toml` 一处采集 OID 数据，
   不存在生成物、不存在第二套识别表。
2. **新增 OS 零改码**：新增一个已知品牌的 OS，在现有原语与 `custom`
   覆盖范围内只加 TOML（超出覆盖范围才改码）。
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
| 逃生口 | `custom`，对应 LibreNMS `user_func` |
| 吸收 | 用 Rust 重写能力，不翻译 PHP 代码 |
