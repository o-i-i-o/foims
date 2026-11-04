# SNMP 私有化 OID/MIB 匹配方案

> 状态：设计稿（已按代码评估与业界对照修订，待评审）
> 范围：foims-snmp 厂商/平台识别、私有 OID 库、设备级绑定持久化
> 关联：`docs/监控采集与通知模块架构.md`、外部 MIB 资料池（独立仓库 `o-i-i-o/MIBs-more`，仅开发期查证用，见 §5.4）

## 1. 背景与目标

当前 SNMP 采集以公共 MIB（RFC 系列）为主，厂商私有 OID 以硬编码方式内嵌在实时采集探测中，识别与采集口径分散。本方案目标：

1. **配置了品牌**的设备 → 直接匹配对应品牌的厂商 mib/oid 库，跳过公共识别；
2. **未配置品牌或品牌未命中库** → 通过公共 MIB（sysObjectID + sysDescr）自动识别设备品牌；
3. **维护一套厂商 OID 库**：品牌 → 库内 OID 集 → 采集返回指标；
4. **识别成功后持久化绑定**：绑定后轮询直达私有 OID，不再执行公共匹配；
5. 平台（Linux/Windows/BSD 等）与硬件厂商严格分层，平台不冒充厂商。

## 2. 现状盘点

| 现状 | 位置 | 问题 |
|---|---|---|
| sysDescr 词表识别 17 品牌 | `crates/foims-snmp/src/collect/sysinfo.rs` `identify_vendor()` | 仅识别不落库，无 sysObjectID 证据 |
| sysObjectID 企业号识别 | `crates/foims-snmp/src/live/profile.rs` `detect_vendor()` | 与 sysinfo 词表两套逻辑并存，口径不一致 |
| 私有 OID 硬编码在 probe 内 | `crates/foims-snmp/src/live/profile.rs`（Cisco/Huawei/H3C CPU/内存/温度列） | 混在采集流程中，无法按品牌直连 |
| `devices.brand VARCHAR(50)` 手填 | `crates/foims-init/src/schema/tables/devices.rs` | 无厂商库绑定字段，识别结果与配置品牌不联动 |

## 3. 平台与厂商分层（关键前提）

### 3.1 公共 MIB 匹配不到硬件"品牌"

- **Linux + net-snmp**：sysObjectID 恒在 `1.3.6.1.4.1.8072`（IANA 注册：Net-SNMP 项目）下，如 `8072.3.2.10`（示例）。8072 标识 **agent 软件**，与 Dell/Lenovo 等服务器硬件厂商无关。
- **Windows**：sysObjectID 在 `1.3.6.1.4.1.311`（IANA 注册：Microsoft）下。版本从 sysDescr 解析。
- 服务器硬件厂商在标准 SNMP 中无公共获取路径（服务器监控由独立 IPMI 链路覆盖，不在本方案范围）。

**结论：不把 Net-SNMP 或 Microsoft 标记为硬件厂商；平台层只产出版本信息，vendor 恒为 None。**

### 3.2 Linux 与 Windows 的 SNMP 差异（结构性不同）

OID 能力的边界在 **agent 软件**而不是 OS 内核，平台分支按 `(OS, Agent)` 二元组细分：

| 维度 | Linux（net-snmp） | Windows（Microsoft SNMP 服务） |
|---|---|---|
| sysObjectID | `1.3.6.1.4.1.8072.3.2.*`（示例：.10=linux） | `1.3.6.1.4.1.311.*`（前缀规则，不锁死全表） |
| CPU 利用率 | UCD ssCpuRaw 六列计数器（已实现）、hrProcessorLoad | **标准 MIB 无解**：hrProcessorTable 未实现、无 ssCpuRaw，需第三方 agent（如 SNMP Informant），FOIMS 不虚构 |
| 内存 | UCD 标量（memSysAvail/memAvailReal，total−available 口径，已实现） | 仅 hrStorageTable 的 Physical/Virtual Memory 行，无 UCD |
| 磁盘 | hrStorageTable + tmpfs 四子树过滤（/dev /proc /sys /run，已踩坑） | hrStorageTable 但过滤规则不同（无 tmpfs 问题，含内存伪存储行） |
| 负载/传感器 | UCD laTable、LM-SENSORS-MIB | 均无 |
| sysDescr 格式 | `Linux <host> <内核版本> ...` | 新版 `Microsoft Windows Version 10.0 (Build ...)`；旧版 `Hardware: ... Software: Windows Version 6.3 (Build ...)`——两种解析器都要（示例格式，需实测校准） |

现网后果：Generic 回退链对 Windows 设备每轮都在白走 UCD/lm_sensors 的空 walk。平台能力声明（caps）让 probe 直接跳过已知缺失的 MIB 族，减少无效请求。

### 3.3 BSD 系列细分

| 平台 | 证据 | 说明 |
|---|---|---|
| FreeBSD + bsnmpd（基础系统自带） | 企业号 **12325**（IANA：Begemot/FreeBSD）+ sysDescr `FreeBSD x.y-RELEASE ...` | 能力与 net-snmp 不同，单独 caps（初始保守：仅 hrStorage，其余待实测） |
| FreeBSD + net-snmp（ports 安装） | 企业号 8072 **+ sysDescr "FreeBSD ..."** | 关键案例：**8072 不等于 Linux**，net-snmp 可装在任何 OS；平台判定 sysDescr 优先 |
| OpenBSD snmpd | 企业号 **30155** | 自有 agent，MIB 能力最小化，识别-only |
| NetBSD | 多为 net-snmp（pkgsrc） | 按 FreeBSD+net-snmp 同款处理 |
| pfSense / OPNsense | FreeBSD 衍生，sysDescr 含 `pfSense`/`OPNsense` | freebsd 平台 + 厂商建议词（Netgate/Deciso），无采集库，展示-only 扩展点 |

### 3.4 其他操作系统（识别-only，不建采集能力）

Solaris（企业号 42，SMA agent）、AIX（企业号 2，IBM）、ESXi（企业号 6876，VMware）、macOS（多随 net-snmp）。初始仅注册识别规则用于展示平台名。**caps 口径与「保守为 false」相反**：识别-only 平台的能力未实测，若全部置 false 会砍掉现有 Generic 链的采集（UCD/hrStorage/ENTITY-SENSOR 探测全被跳过），因此 caps = 全能力、沿用现状 best-effort 链——不虚构 OID，也不因识别出平台而降级采集；仅 Windows、FreeBSD+bsnmpd 两类已核实缺失的 MIB 族按 §3.2/§3.3 裁剪。

### 3.5 裁剪原则（避免过度细分）

- 发行版（Ubuntu/RHEL/openEuler）、Windows 具体版本号**不进分支**——不影响 OID 能力，只作为 `os_version` 展示元数据解析；
- 分支边界只画在"OID 能力集合"真正不同的地方（agent 软件差异）；
- 识别优先级：**sysDescr OS 词（强证据）→ agent 企业号（弱证据，仅作佐证）**。

## 4. 总体流程

```
SNMP 探测（sysObjectID + sysDescr 一次读取）
        │
        ▼
已配置品牌且命中厂商库识别规则？
   │是                          │否
   ▼                            ▼
绑定厂商库(manual)            公共识别
   │                            │
   │                     平台分类（sysDescr OS 词优先）
   │                       │              │
   │              Linux/Windows/BSD    网络设备匹配
   │              仅平台与版本，        企业号 + 描述词 双证据
   │              不判硬件厂商              │
   │                                       ▼
   └──────────────► 绑定厂商库 ◄───────────┘
                  （双证据自动 / 单证据确认）
                        │
                        ▼
              轮询直达私有 OID（不再执行公共匹配）
```

绿色语义（绑定/直达 = 写库）与灰色语义（识别/分类 = 只读）的分离是本方案核心取舍。

## 5. 模块设计（模块化 + 可扩展）

新建 `crates/foims-snmp/src/vendor/`，采集逻辑（probe/discover_*）留在原处，vendor 域只放"规则与 OID 数据 + 识别纯函数"。识别（OS 级 LibreNMS 模型）与采集库（品牌级）分层：

```
crates/foims-snmp/src/vendor/
├── mod.rs        # VendorRule / ProbeCheck / VendorOidSet / SnmpIdentity /
│                 # registry() + BUILTIN 16 条内置品牌级规则（含三家识别规则）
├── community.rs  # 社区 OS 级规则 738 条（gen_vendor_community.py 生成，勿手改）
├── mibs.rs       # 三家私有采集 OID 库（CISCO/HUAWEI/H3C，绑定直连目标）
├── platform.rs   # PlatformRule / PlatformCaps + platform registry() + 版本解析
├── identify.rs   # 证据聚合（前缀/关键词/正则）→ best_guess + confidence（纯函数）
└── probe.rs      # 主动探测识别（probe_checks 全未命中静态证据时的一轮 GET 比对）
```

### 5.1 核心类型

```rust
/// 厂商库条目：识别规则 + 可选私有采集 OID 集
pub struct VendorMib {
    pub rule: VendorRule,
    /// None = 仅识别规则，无可绑定采集库（识别得出但不提供绑定）
    pub oid_set: Option<VendorOidSet>,
}

pub struct VendorRule {
    pub id: &'static str,                 // "cisco"（品牌级）或 "ios"（社区 OS 级）
    pub display_name: &'static str,       // "Cisco" / "Cisco IOS"
    pub aliases: &'static [&'static str], // 品牌别名（手填匹配 + sysDescr 识别共用一张表）
    /// sysObjectID 前缀（至少 7 弧=企业号级；社区 OS 级规则带更深弧区分
    /// 同企业号产品线）。识别按**最长前缀胜出**（LibreNMS 语义），同长度
    /// 前缀内置品牌级优先；前缀第 7 弧为平台 agent 号（8072 等）的不作
    /// 厂商证据（§3.1）。企业号级取值来源：IANA enterprise-numbers / SNMP::Info
    pub sysobjectid_prefixes: &'static [&'static [u32]],
    /// sysDescr 词边界关键词（小写），来源：设备实测 sysDescr 样例 +
    /// 社区纯 sysDescr 块（上游 AND 块的关键词一律不收，见 §5.3.5）
    pub descr_keywords: &'static [&'static str],
    /// sysDescr 排除关键词（小写）：命中即证据降级为 Low（OEM/固件上报他人
    /// 企业号的场景，如华为 USG 防火墙上报 net-snmp sysObjectID 8072.3.2.10）。
    /// 对应 LibreNMS sysDescr_except 实践，来源标注同上
    pub descr_keywords_except: &'static [&'static str],
    /// sysDescr 正则关键词（Rust regex 语法——上游 PCRE 的无 look-around
    /// 子集，定界符剥除、/i 转 (?i)；编译失败的丢弃并记录于 community.rs
    /// 生成头）。命中与关键词同级记 sysDescr 证据；仅收纯 sysDescr 块的
    /// 正向正则（except 排除正则对无其他证据的 OS 无识别价值，不转换）
    pub descr_regexes: &'static [&'static str],
    /// 主动探测检查（LibreNMS snmpget 语义，多条 AND）：前缀/关键词/正则
    /// 全未命中时经 probe 链路 GET 比对（识别期 IO 收敛于 profile 探测
    /// 链路，见 vendor/probe.rs）。上游 op=!= 的负条件不转换（几乎所有
    /// 值都满足，无识别价值）；探测 OID 仅收点分数字（MIB 名格式运行时
    /// 零 MIB 依赖无法解析，生成端丢弃并计数）
    pub probe_checks: &'static [ProbeCheck],
}

/// 主动探测单条检查：GET `oid` 后按 `op` 比对响应值与 `value`。
pub struct ProbeCheck {
    pub oid: &'static str, // 点分数字 OID
    pub op: ProbeOp,       // Exists / Contains / Starts / Equals / Regex
    pub value: &'static str,
}
```

```rust
/// 平台识别规则：agent 企业号是弱证据（net-snmp 跨 OS），
/// sysDescr OS 关键词是强证据，两者组合定 (os, agent)
pub struct PlatformRule {
    pub id: &'static str,          // "linux" / "windows" / "freebsd"
    pub display_name: &'static str,
    /// agent 软件企业号（弱证据），来源：IANA enterprise-numbers
    pub agent_enterprise_ids: &'static [u32],
    /// sysDescr OS 关键词（强证据，示例：首词匹配 "linux"/"freebsd"/"microsoft windows"）
    pub descr_keywords: &'static [&'static str],
    /// 采集能力声明：probe 按此跳过已知缺失的 MIB 族
    pub caps: PlatformCaps,
    /// sysDescr → 版本展示串（Linux 内核版本 / Windows 版本，示例正则）
    pub version_parser: fn(&str) -> Option<String>,
}

pub struct PlatformCaps {
    pub ucd_mem: bool,       // UCD 内存标量
    pub ucd_cpu: bool,       // ssCpuRaw / laTable
    pub hr_storage: bool,
    pub hr_processor: bool,
    pub lm_sensors: bool,
    pub entity_sensors: bool, // ENTITY-SENSOR-MIB 温度（Windows 亦未实现，需裁剪）
}
```

```rust
/// 识别结果（read-only，每次探测可产出）
pub struct SnmpIdentity {
    pub platform: Option<&'static str>,      // 平台规则 id，Unknown 为 None
    pub os_version: Option<String>,          // 内核版本 / Windows 版本（展示用）
    pub vendor_guess: Option<&'static str>,  // 厂商库 id
    pub confidence: Confidence,              // High / Low
    pub caps: platform::PlatformCaps,        // 采集能力声明（probe 门控用）
    /// 证据说明（UI 展示 + 调试）：冲突/降级等场景运行时拼接，故为 String
    pub evidence: Vec<String>,
}
```

初始平台注册表（8 条）：`linux`（全能力，即现状 net-snmp 口径；agent 企业号留空，8072 由常量单独处理）、`windows`（仅 hr_storage）、`freebsd`（bsnmpd 保守仅 hr_storage，net-snmp 经 effective_caps 升级）、`openbsd` / `solaris` / `aix` / `esxi` / `macos`（识别-only，caps=全能力走现状 best-effort 链）；未命中 → Unknown 平台全能力，走现有 Generic 链（现状行为等价）。

### 5.2 采集 OID 集与 VendorKind

- `VendorOidSet` 承载现有 profile.rs 内嵌的厂商 OID（CPU/内存/温度列、内存计划形态 Pool/EntityPercent 参数）；
- `VendorKind` 枚举保留（采集计划 switch 的区分键），registry 提供 `vendor_id → VendorKind` 映射；
- **收编两套旧逻辑**：删除 `sysinfo.rs::identify_vendor` 与 `profile.rs::detect_vendor`，识别统一走 `vendor::identify`（全局一致性，不留兼容 shim）；
- **收编兼容口径**：`identify_vendor` 历史返回展示串（"Cisco"/"HP/HPE"/"Unknown"），`VendorRule.display_name` 必须覆盖这些历史取值、aliases 覆盖 hp/hpe/procurve 等手填词，保证 snmp-info API 与前端品牌展示口径不漂移；
- `VendorOidSet` 预留「列族变体」扩展位（未来 NX-OS/Comware 大版本出现 OID 差异时在厂商文件内拆变体，不动核心），初始不实现。

### 5.3 厂商 OID 库维护规则（来源标注）

1. 识别规则初始 16 家词表（收编现状：cisco/huawei/h3c + 13 识别-only）+ 已验证企业号（9=Cisco / 2011=Huawei / 25506=HH3C / 11=HPE）；企业号以 **IANA 官方 enterprise-numbers 文件为唯一权威来源**（可随时下载最新版），其他资料仅作旁证；
2. 采集库初始仅 cisco/huawei/h3c，OID 常量从 profile.rs 迁入，**每条常量上方注释必须注明来源**：RFC 号 / 厂商公开 MIB 文件名 / IANA 企业号。迁移时顺带修正粗糙口径：华为实体表 `hwEntityCpuUsageThreshold=0` 占位行应跳过（借鉴 LibreNMS skip_values），不再只按 0..=100 过滤；
3. 采集库 ⊂ 识别规则：识别得出但无采集库的品牌（如 Juniper）→ 展示识别结果，不提供绑定；
4. 新增厂商 = mod.rs BUILTIN 追加规则（绑定直连另在 mibs.rs 加采集库）+ 单测，不动核心逻辑；社区 OS 级随上游更新重跑生成脚本；
5. 规则数据的批量来源（GPL 上游只提取识别事实：sysObjectID 前缀 + sysDescr 关键词/正则 + 主动探测检查，不拷贝采集配置表达；优先级从高到低）：
   - **LibreNMS `resources/definitions/os_detection/*.yaml`**（822 个 OS 识别规则、持续更新的社区验证数据）：**已按 LibreNMS 的 OS 级分组方式直接全量入库**（项目决策：社区验证数据可信，不按品牌级人工归并）——识别模型随之升级为 sysObjectID 前缀**最长匹配**（LibreNMS 语义），OS 级深前缀（如 9.1.2330=ios 特定型号）优先于品牌级 7 弧前缀，同长度内置优先；命中 OS 级 id 后经 `vendor_kind()` 按 LibreNMS group 归属采集库（cisco 组全产品线→Cisco；华为仅 vrp/yunshan→Huawei，oceanstor/smartax 等存储接入线走 Generic；comware→H3c）。入库落地为 **`da-scripts/gen_vendor_community.py`**（本地 sparse clone 全量转换 → `vendor/community.rs`，勿手改），转换口径：排除平台类（type=server/unix，29 个，属 platform.rs 域）与无有效证据 OS（前缀/关键词/正则/探测全空，54 个）；**关键词仅从纯 sysDescr 块收集**——上游 AND 块（前缀+关键词+snmpget 组合，如 airos-af-ltu=8072+"Linux"+UBNT MIB 探测）的关键词是跨设备通用词，作为独立证据只会误命中；单词元 ≤2 字符关键词丢弃；**sysDescr_regex → descr_regexes**（仅收纯 sysDescr 块的正向正则，PCRE 定界符剥除、`/i` 标志转 Rust regex 内联 `(?i)`，编译失败运行时跳过；except 排除正则对无其他证据的 OS 无识别价值，不转换）；**snmpget → probe_checks**（探测 OID 多为厂商私有 MIB、设备自证，GET+值比对独立于块内伙伴成立；仅收点分数字 OID——`MIB::obj` 格式运行时零 MIB 依赖无法解析，生成端丢弃并计数；op=`!=` 负条件几乎全满足，丢弃）。各项丢弃计数均记录于生成文件头。现状 822 → 738 条 OS 规则 + 16 内置品牌级规则共存。8072(net-snmp) 下聚合 68 个互不相干 OS，印证「8072 不作厂商证据」的设计；
   - **单 OS 草稿工具 `da-scripts/gen_vendor_rule_draft.py`**（`<os名>` 按需生成单条草稿 / `--list` 列全部 os 名 / `--all` 生成仓库外候选池草稿与 `_SUMMARY.md` 企业号聚合对照）：用于核对单个 OS 的转换细节或排查识别差异，不入库流程；
   - **SNMP::Info 企业号映射表**（`lib/SNMP/Info.pm` 的 `device_type()`）：单表核对；
   - **外部 MIB 资料池（独立仓库 `MIBs-more`，2019 年快照）**：仅作离线 OID 字典，查证「OID 数值 ↔ 含义/单位」；它**不进 foims 仓库、不进程序与部署包**——FOIMS 采集是数字 OID 定向 GET，运行时零 MIB 依赖（与 Prometheus snmp_exporter 同路线，区别于 LibreNMS 运行时 snmptranslate）；
   - 产出一律为 Rust 源码常量（编译期静态，无运行时配置热载），不引入 MIB 运行时解析。

### 5.4 业界对照（关键取舍的依据）

| 项目 | 做法 | 对本方案的支撑 |
|---|---|---|
| LibreNMS | 每 OS 一个识别 YAML（sysObjectID 前缀 + sysDescr 子串 + except + snmpget 探测），822 个文件；采集 OID 在 `os_discovery/*.yaml`（含 skip_values） | registry + 每厂商一文件的组织方式在数量级上可行；except 字段是实践必需——其 linux.yaml 用 `sysDescr_regex_except: /^USG/` 排除上报 net-snmp sysObjectID 的华为 USG 防火墙 |
| SNMP::Info（Netdisco） | `device_type()`：sysDescr 正则特例优先 → 企业号 map 兜底；8072/311/12325/6876 均归「设备类别」而非硬件厂商；另处理 Teltonika 上报通用 net-snmp sysObjectID 特例 | 印证 §3 平台/厂商分层与「8072 ≠ Linux」的证据优先级 |
| Prometheus snmp_exporter | MIB 由 generator 编译期生成采集配置，无运行时厂商识别 | 印证编译期固化 OID 的轻量路线；FOIMS 的半自动提取产物即 .rs 常量 |
| Zabbix | OID 全在模板、主机手动挂载，无自动识别 | 印证 manual 绑定语义；本方案额外的 detected 双证据自动绑定（High 才写、Low 人工确认）对 CMDB 场景更保守、更安全 |

## 6. 识别与绑定策略

### 6.1 双证据与置信度

识别输出双结果：`best_guess`（驱动采集计划，保持现有能力）+ `confidence`（驱动持久化绑定）：

| 证据组合 | best_guess | confidence | 绑定动作 |
|---|---|---|---|
| 企业号命中 + 描述词命中同厂商，且未命中 except 词 | 该厂商 | High | **自动绑定**（source=detected） |
| 命中任一 except 词（OEM/固件上报他人企业号，如 USG 报 8072.3.2.10） | 按 sysDescr 判定 | Low | 不绑定，UI 提示确认（防误自动绑定——绑定后不回退，错误会持续到人工发现） |
| 仅企业号命中（如 9/2011/25506） | 该厂商 | Low | 不绑定，UI 提示确认 |
| 仅描述词命中（关键词或 descr_regexes 正则） | 该厂商 | Low | 不绑定，UI 提示确认 |
| 静态证据全未命中 + 主动探测 probe_checks 全命中（多条 AND，一轮 GET） | 该厂商 | Low | 不绑定，UI 提示确认（探测 op 多为宽松子串/存在性检查，不足以驱动自动绑定） |
| 企业号 11(HPE) + 描述词 comware（冲突，描述胜出，沿用现语义） | H3C | Low | 不绑定，UI 提示确认 |
| 企业号 11(HPE) + 描述词 aruba/procurve（同企业号下的 ProVision 系） | Aruba（无采集库则展示-only） | Low | 不绑定，UI 提示确认；11 下的所有冲突组合一律人工确认 |
| 企业号 8072 / 311 / 12325 / 30155 | None（platform=linux/windows/freebsd/openbsd） | — | 不绑定，记录 OS 版本 |
| 均未命中 | Generic | — | 不绑定，走公共回退链 |

### 6.2 绑定持久化

设备表加两列（均空时每次识别探测零额外请求——probe 本就 GET 系统组）：

```sql
ALTER TABLE devices ADD COLUMN snmp_vendor_id VARCHAR(32);
ALTER TABLE devices ADD COLUMN snmp_vendor_source VARCHAR(16)
    CHECK (snmp_vendor_source IN ('manual', 'detected'));
-- 两列同空同非空，防「有 id 无 source」脏态（foims-init 建表含同款约束）
ALTER TABLE devices ADD CONSTRAINT chk_devices_snmp_vendor
    CHECK ((snmp_vendor_id IS NULL) = (snmp_vendor_source IS NULL));
```

- **manual**：保存设备时 `brand` 命中注册表（display_name/aliases 大小写不敏感）→ 直接绑定；改为未命中值 → 自动清空绑定（brand 与绑定一致性自动维护）；
- **detected**：仅 High 证据自动写入；Low 只作建议返回，UI「确认绑定」后落库（用户动作记 manual）；
- **绑定后**：probe 按 `snmp_vendor_id` 直连厂商 OID 集，跳过识别逻辑——不再执行公共匹配；
- 「重新识别」= 清空两列后重跑识别探测。

## 7. 回退逻辑评估（逐条）

1. 绑定后厂商 OID 采集全空（品牌填错/固件不实现）→ **不自动回退公共匹配**（否则每轮重复探测且口径漂移，违背绑定语义）；trace warn + debug 面板可见 + 「重新识别」人工出口；
2. 手填品牌不在注册表 → 只存 brand 不绑定，属合法自定义品牌，不报错；
3. 识别出的品牌仅在 `brand` 为空时回填，不覆盖用户手填值（与现有"SNMP 覆盖同步保留原值"语义一致）;
4. Linux/Windows/BSD 平台不因"识别不出硬件厂商"而报错，平台与版本信息照常展示；
5. 识别探测失败（系统组无响应）→ 维持现 `ProbeError` 行为。

## 8. mock 测试策略（不依赖真实设备）

- **识别纯函数**：输入 sysObjectID arcs + sysDescr 字符串 → 断言 platform/vendor/confidence。用例覆盖：
  - 现有 ship/Sharp 子串误命中回归；
  - 8072/311/12325/30155 平台用例（含 8072 + "FreeBSD" 组合）；
  - High/Low 全部证据组合、11+comware / 11+aruba 冲突；
  - except 词降级（8072.3.2.10 + sysDescr 含 "USG" → Low，不自动绑定）；
  - Windows 新旧两种 sysDescr 格式解析。
- **SnmpReader trait**（get_many/walk）抽象采集 IO，probe 泛型化；测试用 `BTreeMap<Oid, Value>` 内存快照构造华为/Cisco/net-snmp/Windows 四类设备 → 断言 probe 产出的 CPU/MEM/TEMP 采集计划类型，probe 全链路可 mock；
- **绑定策略**：High 自动写、Low 不写、manual 覆盖 detected、重新识别清空（DB 层测试）。

## 9. API / 前端

- 扩展 `GET /api/resources/devices/{id}/snmp-info` → 返回完整 `SnmpIdentity`（platform/os_version/vendor_guess/confidence/evidence）+ 现有 brand/model/hostname；
- 新增绑定确认/清空接口（或并入设备更新校验白名单）；「重新识别」入口；
- 设备模态框 `brand` 输入挂厂商库 datalist 建议（GET /api/snmp/vendors）；
- 识别面板展示平台/版本/证据/置信度，Low 结果提供「确认绑定」「重新识别」按钮；绑定来源徽标（manual/detected）；
- i18n 中英同步；`da-scripts/gen-ts-types.sh` 再生成 TS 类型。

## 10. 版本与合规

- DB 变更 → 根 `Cargo.toml` bump **0.34.0**（按 `docs/dev_version.md` 记录版本）；
- `foims-init` 建表代码 + `check.rs` 校验清单同步；
- `upgrade/0.34/` 产出迁移 SQL（基于 0.33.x 数据库）；
- `cargo fmt && cargo clippy --release --workspace --all-targets -- -D warnings` 全过；
- 前端 `npm run typecheck` + `npm run build` 全过，前端版本随 `frontend/package.json` 同步。

## 11. 实施顺序

| 阶段 | 内容 | 依赖 |
|---|---|---|
| 一 | vendor 模块骨架：平台注册表 + 识别纯函数 + 单测；收编两套旧识别；SnmpReader trait + mock 快照 | 无 DB 变更，可先行。**✅ 已实施（v0.82.0）** |
| 二 | 持久化绑定：加列 + 迁移 SQL + 绑定 API + probe 绑定直连 | 阶段一 |
| 三 | 前端：datalist、识别确认面板、重新识别入口 + TS 类型 + i18n | 阶段二 |
| 四（可选） | 示例厂商骨架模块（注释标注「示例」），验证扩展路径 | 阶段一 |
