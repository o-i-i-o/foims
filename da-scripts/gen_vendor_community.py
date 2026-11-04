#!/usr/bin/env python3
"""LibreNMS os_detection 全量 → vendor/community/ 识别规则目录（OS 级入库）。

用法：
    python3 da-scripts/gen_vendor_community.py [--src DIR]

决策依据（docs/SNMP厂商MIB识别与绑定方案.md §5.3.5）：
按 LibreNMS 的 OS 级分组方式**直接入库**（社区验证数据），识别模型为
sysObjectID 前缀最长匹配（LibreNMS 语义）。**全部识别规则数据出自本脚本**：
- 16 条内置品牌级规则（数据源为脚本内 BUILTIN_BRANDS——原 vendor/mod.rs
  手工词表迁入，人工核实的 IANA 企业号/别名/关键词）；
- 社区 OS 级规则（LibreNMS os_detection 全量转换）；与内置品牌同 id 的
  上游 OS（如 netgear.yaml）**证据合并进内置条目**（前缀/关键词/正则/探测
  并集），display_name/aliases 保持内置（手填品牌匹配的 UI 语义人工核实）；
- vendor_kind() 的 OS→采集库映射（KIND_BY_OS）由脚本按上游 group 生成，
  不再人工同步。

输出（识别/采集分层，采集库 mibs.rs 为手工核实文件，不在本脚本范围）：
    vendor/community/mod.rs        聚合 + only() + KIND_BY_OS + 头注释
    vendor/community/builtin.rs    16 条内置品牌规则（三家关联采集库）
    vendor/community/os_*.rs       社区 OS 规则（按 id 首字母四桶）

数据源：本地 sparse clone 的 os_detection 目录（约 300KB，不进仓库）：
    git clone --depth 1 --filter=blob:none --sparse https://github.com/librenms/librenms /tmp/librenms
    cd /tmp/librenms && git sparse-checkout set resources/definitions/os_detection

转换与排除规则（全部记录在生成文件头注释中）：
- 排除 type=server/unix 平台类（平台识别属 vendor/platform.rs 域）
- sysObjectID 前缀 → sysobjectid_prefixes；前缀第 7 弧为平台 agent 号
  （8072/311/12325/30155/42/2/6876）的丢弃（§3.1 不作厂商证据）
- sysDescr 子串 → descr_keywords（小写去重）；单词元 ≤2 字符的丢弃
  （词元匹配下误命中面过大）；多词元关键词保留（全词元命中，更严格、
  漏报方向安全，企业号前缀兜底）
- **关键词仅从纯 sysDescr 块收集**（无 sysObjectID、无 snmpget 的块）：
  上游 AND 块（前缀+关键词+主动探测组合）中前缀证据已独立成立，其关键
  词常是跨设备通用词（如 airos-af-ltu/airos-af 的 "linux"），作为独立
  证据只会误命中，一律丢弃
- sysDescr_regex → descr_regexes（仅收纯 sysDescr 块的正向正则，Rust
  regex 语法；空定界符丢弃，编译失败运行时跳过）；except 排除正则对
  无其他证据的 OS 无识别价值，仅计数记录
- 无可转换证据的 OS 生成空规则占位（识别永不命中，保留名录完整性），
  不跳过——上游 822 个定义全部在产物中有对应条目
"""

import re
import sys
from collections import Counter
from datetime import date
from pathlib import Path

import yaml

# 上游个别文件用 `op: =`（YAML 1.1 的 = 即 value 隐式标签），PyYAML 默认
# 不识别；注册为普通标量即可（只影响该标签，如 boschcpp.yaml）
yaml.SafeLoader.add_constructor(
    "tag:yaml.org,2002:value", lambda loader, node: loader.construct_scalar(node)
)

DEFAULT_SRC = Path("/tmp/librenms/resources/definitions/os_detection")
# 生成目标目录（入库文件，勿手改——重跑脚本再生成）
OUT_DIR = (
    Path(__file__).resolve().parent.parent
    / "crates/foims-snmp/src/vendor/community"
)
BUCKETS = ("a_g", "h_n", "o_t", "u_z")  # OS 规则按 id 首字母四桶

# —— 内置品牌级规则（原 vendor/mod.rs BUILTIN 手工词表迁入，数据不变）——
# (id, display_name, aliases, 前缀弧组, 关键词, 采集库, 来源注释)
BUILTIN_BRANDS = [
    ("cisco", "Cisco", ["cisco"], [[1, 3, 6, 1, 4, 1, 9]], ["cisco"], "CISCO",
     "IANA enterprise-numbers：9 = Cisco Systems"),
    ("huawei", "Huawei", ["huawei"], [[1, 3, 6, 1, 4, 1, 2011]],
     ["huawei", "versatile routing platform"], "HUAWEI",
     "IANA enterprise-numbers：2011 = Huawei Technologies；"
     "「versatile routing platform」为华为 VRP 系统多词关键词"),
    ("h3c", "H3C", ["h3c", "3com"], [[1, 3, 6, 1, 4, 1, 25506]],
     ["h3c", "comware", "3com"], "H3C",
     "IANA enterprise-numbers：25506 = H3C（原 3Com 归并）"),
    ("juniper", "Juniper", ["juniper"], [[1, 3, 6, 1, 4, 1, 2636]],
     ["juniper", "junos"], None, "识别-only（无采集库）"),
    ("dell", "Dell", ["dell"], [[1, 3, 6, 1, 4, 1, 674]], ["dell"], None,
     "识别-only"),
    ("hp", "HP/HPE",
     ["hp", "hpe", "procurve", "hewlett packard", "hewlett-packard"],
     [[1, 3, 6, 1, 4, 1, 11]],
     ["hp", "hpe", "procurve", "hewlett packard"], None,
     "企业号 11 同时覆盖 Comware（→h3c 冲突，描述词胜出）与"
     " ProVision/Aruba（本条），冲突组合一律 Low + 人工确认（§6.1）"),
    ("aruba", "Aruba", ["aruba"], [[1, 3, 6, 1, 4, 1, 14823]],
     ["aruba", "arubaos"], None, "识别-only"),
    ("netgear", "Netgear", ["netgear"], [[1, 3, 6, 1, 4, 1, 4526]],
     ["netgear", "prosafe"], None, "识别-only"),
    ("tp-link", "TP-Link", ["tp-link", "tplink", "tp link"], [],
     ["tp-link", "tplink"], None,
     "企业号未核实，留空不虚构（§5.3.1）；识别-only"),
    ("linksys", "Linksys", ["linksys"], [], ["linksys"], None,
     "企业号未核实，留空不虚构；识别-only"),
    ("ubiquiti", "Ubiquiti", ["ubiquiti", "ubnt", "unifi"],
     [[1, 3, 6, 1, 4, 1, 4413], [1, 3, 6, 1, 4, 1, 10002],
      [1, 3, 6, 1, 4, 1, 41112]],
     ["ubiquiti", "ubnt", "unifi"], None, "识别-only"),
    ("mikrotik", "MikroTik", ["mikrotik", "routeros"],
     [[1, 3, 6, 1, 4, 1, 14988]], ["mikrotik", "routeros"], None, "识别-only"),
    ("extreme", "Extreme Networks",
     ["extreme", "extremenetworks", "extreme networks"],
     [[1, 3, 6, 1, 4, 1, 1916]], ["extreme", "extremexos"], None, "识别-only"),
    ("alcatel", "Alcatel", ["alcatel", "alcatel-lucent"],
     [[1, 3, 6, 1, 4, 1, 6486]], ["alcatel"], None, "识别-only"),
    ("zyxel", "ZyXEL", ["zyxel"], [], ["zyxel"], None,
     "企业号未核实，留空不虚构；识别-only"),
    ("d-link", "D-Link", ["d-link", "dlink"], [[1, 3, 6, 1, 4, 1, 171]],
     ["d-link", "dlink"], None, "识别-only"),
]
BUILTIN_IDS = {b[0] for b in BUILTIN_BRANDS}

# 平台 agent 企业号（vendor/platform.rs 注册表 + net-snmp）：
# 前缀第 7 弧为这些号的不作厂商证据（§3.1）
PLATFORM_AGENTS = {8072, 311, 12325, 30155, 42, 2, 6876}
# snmpget op → Rust ProbeOp 变体（vendor/mod.rs）；不在表内的 op（含 != 负
# 条件——几乎所有值都满足，无识别价值）丢弃
PROBE_OPS = {
    "exists": "Exists", "contains": "Contains", "starts": "Starts",
    "=": "Equals", "regex": "Regex",
}
# vendor_kind（OS → 采集库归属）生成口径：与原 mod.rs 硬编码语义一致——
# cisco 组全产品线绑定 Cisco；华为仅 VRP 网络设备（yunshan=CloudEngine
# VRP8 实体表同构）；新华三 Comware。其余（含 huawei 组的 oceanstor/
# smartax 等存储接入线）走 Generic 公共回退链


def as_list(value) -> list:
    if value is None:
        return []
    return value if isinstance(value, list) else [value]


def parse_prefix(prefix: str) -> list[int] | None:
    """sysObjectID 前缀字符串 → 弧数组；非 1.3.6.1.4.1 企业分支返回 None。"""
    arcs = [int(a) for a in prefix.strip().strip(".").split(".")]
    if len(arcs) < 7 or arcs[:6] != [1, 3, 6, 1, 4, 1]:
        return None
    return arcs


def tokens(keyword: str) -> list[str]:
    return [t for t in re.split(r"[^0-9a-zA-Z]+", keyword.lower()) if t]


def usable_keyword(kw: str) -> bool:
    """单规则：单词元且 ≤2 字符丢弃（词元匹配误命中面过大）；多词元保留。"""
    tk = tokens(kw)
    return bool(tk) and not (len(tk) == 1 and len(tk[0]) <= 2)


def rust_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def bucket_of(os_name: str) -> str:
    c = os_name[0].lower()
    if c <= "g":
        return "a_g"
    if c <= "n":
        return "h_n"
    if c <= "t":
        return "o_t"
    return "u_z"


def main() -> None:
    src = Path(sys.argv[sys.argv.index("--src") + 1]) if "--src" in sys.argv else DEFAULT_SRC
    if not src.is_dir():
        sys.exit(f"错误：数据源不存在：{src}（先 sparse clone，见脚本头注释）")

    # 证据按 OS 归集：内置品牌条目与上游同 id 条目合并进同一证据集
    # entries[os] = {"display": 内置 display_name 或上游 text, "aliases": 内置,
    #                "mib": 采集库或 None, "note": 来源注释, "builtin": bool,
    #                "prefixes": set, "keywords": set, "regexes": set,
    #                "probes": set}
    entries: dict[str, dict] = {}
    for os_id, display, aliases, prefixes, keywords, mib, note in BUILTIN_BRANDS:
        entries[os_id] = {
            "display": display, "aliases": aliases, "mib": mib, "note": note,
            "builtin": True, "prefixes": {tuple(p) for p in prefixes},
            "keywords": set(keywords), "regexes": set(), "probes": set(),
        }

    kind_groups: dict[str, list[str]] = {}
    stats = Counter()

    for path in sorted(src.glob("*.yaml")):
        stats["upstream"] += 1
        doc = yaml.safe_load(path.read_text(encoding="utf-8"))
        if not isinstance(doc, dict):
            # 结构非映射：既非平台类也无法提取 OS 名，仅计数（上游现状为 0）
            stats["malformed"] += 1
            continue
        if str(doc.get("type", "")) in ("server", "unix"):
            stats["platform"] += 1
            continue
        os_id = path.stem
        builtin_hit = os_id in BUILTIN_IDS
        if builtin_hit:
            # 与内置品牌同 id（如 netgear.yaml）：证据合并进内置条目，
            # display_name/aliases 保持内置（手填品牌匹配的 UI 语义）
            stats["builtin_merged"] += 1
            entry = entries[os_id]
        else:
            group = str(doc.get("group") or "")
            if group:
                kind_groups.setdefault(group, []).append(os_id)
            entry = {
                "display": str(doc.get("text") or os_id), "aliases": [],
                "mib": None, "note": "", "builtin": False,
                "prefixes": set(), "keywords": set(), "regexes": set(),
                "probes": set(),
            }
            entries[os_id] = entry

        regex_cnt = 0
        dropped_agent = 0
        for rule in doc.get("discovery", []) or []:
            # 块内 AND 语义：收集本块前缀，平台号前缀丢弃并计数
            block_prefixes = []
            for p in as_list(rule.get("sysObjectID")):
                arcs = parse_prefix(str(p))
                if arcs is None:
                    continue
                if arcs[6] in PLATFORM_AGENTS:
                    dropped_agent += 1
                else:
                    block_prefixes.append(tuple(arcs))
            entry["prefixes"].update(block_prefixes)
            # 关键词/正则仅从「纯 sysDescr 块」收集（无 sysObjectID、无
            # snmpget）：上游 AND 块中前缀已独立成立，子串/正则转换后
            # 只有跨设备通用词误命中风险、无识别增益（airos-af-ltu 教训）
            if not as_list(rule.get("sysObjectID")) and "snmpget" not in rule:
                for k in as_list(rule.get("sysDescr")):
                    k = str(k).lower()
                    if usable_keyword(k):
                        entry["keywords"].add(k)
                    elif tokens(k):
                        stats["kw_dropped"] += 1
                for rx in as_list(rule.get("sysDescr_regex")):
                    rx = str(rx).strip()
                    # PCRE 定界格式 /pattern/flags（如 /.../i）：剥定界符，
                    # i 标志转 Rust regex 内联 (?i)；其余标志（s/m/x）上游
                    # 罕见且丢弃后匹配更保守（漏报方向安全）
                    delimited = re.fullmatch(r"/(.*)/([a-z]*)", rx)
                    if delimited:
                        rx, pcre_flags = delimited.group(1).strip(), delimited.group(2)
                        if "i" in pcre_flags:
                            rx = f"(?i){rx}"
                    else:
                        rx = rx.strip("/")
                    if rx:
                        entry["regexes"].add(rx)
                    else:
                        stats["regex_dropped"] += 1
            elif as_list(rule.get("sysDescr")):
                stats["kw_orphan_dropped"] += len(as_list(rule.get("sysDescr")))
            regex_cnt += len(as_list(rule.get("sysDescr_regex")))
            regex_cnt += len(as_list(rule.get("sysDescr_regex_except")))
            regex_cnt += len(as_list(rule.get("sysDescr_except")))
            # 主动探测（snmpget）独立成立：探测 OID 多为厂商私有 MIB（设备
            # 自证），GET+值比对不依赖块内伙伴；op=!= 负条件与未知 op 丢弃；
            # MIB 名格式（MIB::obj，上游靠 snmptranslate 解析）本项目运行时
            # 零 MIB 依赖，仅收点分数字 OID
            for g in as_list(rule.get("snmpget")):
                op = str(g.get("op", "exists")).lower()
                oid_s = str(g.get("oid", "")).strip().lstrip(".")
                if not oid_s or op not in PROBE_OPS:
                    stats["probe_dropped"] += 1
                    continue
                if not re.fullmatch(r"[\d.]+", oid_s):
                    stats["probe_mibname_dropped"] += 1
                    continue
                entry["probes"].add((oid_s, PROBE_OPS[op], str(g.get("value", ""))))
            if "snmpget" in rule:
                stats["snmpget"] += 1
        stats["regex"] += regex_cnt
        stats["dropped_agent_prefix"] += dropped_agent

        # 无有效证据的上游 OS：生成空规则占位（识别永不命中、无风险），
        # 保留 OS 名录完整性——上游补充证据后重跑本脚本自动补全；
        # 内置条目已有数据不受影响
        if not (entry["prefixes"] or entry["keywords"] or entry["regexes"] or entry["probes"]):
            stats["empty_evidence"] += 1

    # —— KIND_BY_OS：OS id → 采集库归属（cisco 组 + 华为 VRP 家族 + Comware）——
    # 空证据占位条目不入表（识别永不命中，映射不可达）
    kind_by_os = []
    for os_id in sorted(entries):
        e = entries[os_id]
        if not (e["prefixes"] or e["keywords"] or e["regexes"] or e["probes"]):
            continue
        group = next((g for g, l in kind_groups.items() if os_id in l), "")
        if os_id == "cisco" or group == "cisco":
            kind_by_os.append((os_id, "Cisco"))
        elif os_id in ("huawei", "vrp", "yunshan"):
            kind_by_os.append((os_id, "Huawei"))
        elif os_id in ("h3c", "comware"):
            kind_by_os.append((os_id, "H3c"))
        # 其余（含 huawei 组 oceanstor/smartax 等）→ Generic，不入表

    # —— 生成 Rust：community/ 目录（mod 聚合 + builtin + OS 四桶）——
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    os_rules = [o for o in sorted(entries) if not entries[o]["builtin"]]
    per_bucket = {b: [o for o in os_rules if bucket_of(o) == b] for b in BUCKETS}

    header = [
        "//! LibreNMS os_detection 全量转换 + 内置品牌级规则（识别数据单一来源）。",
        f"//! 由 da-scripts/gen_vendor_community.py 于 {date.today()} 生成，**勿手改**：",
        "//! 更新上游后重跑脚本再生成；规则结构变化须同步 vendor/mod.rs。",
        "//!",
        f"//! 上游 {stats['upstream']} 个定义 = 平台类 {stats['platform']} 个（platform.rs 域）"
        + (f"+ 结构异常 {stats['malformed']} 个 + " if stats["malformed"] else "+ ")
        + f"本表 {len(entries)} 条（内置品牌 {sum(1 for e in entries.values() if e['builtin'])}"
        f" + 社区 OS {len(os_rules)}）。对账：{stats['upstream']} 全覆盖，无跳过。",
        f"//! - 与内置品牌同 id 证据合并 {stats['builtin_merged']} 个"
        "（如 netgear.yaml 并入内置 netgear，display/aliases 保持内置）",
        f"//! - 空证据占位 {stats['empty_evidence']} 个：上游证据为 MIB 名格式探测"
        "（厂商 MIB 不可得，零 MIB 依赖无法解析）/平台号前缀/except 正则/低质"
        "关键词，无可转换证据 → 生成空规则（识别永不命中、无风险），保留 OS 名录"
        "完整性，上游补充证据后重跑自动补全",
        f"//! - 丢弃平台 agent 号前缀 {stats['dropped_agent_prefix']} 条"
        "（8072 net-snmp 等，§3.1 不作厂商证据）",
        f"//! - 丢弃高风险关键词（单词元 ≤2 字符）{stats['kw_dropped']} 条、"
        f"孤儿关键词（AND 伙伴失效，块内前缀为平台号/含主动探测）{stats['kw_orphan_dropped']} 条",
        f"//! - 正则：sysDescr_regex/except 原始 {stats['regex']} 条，仅收纯 sysDescr 块正向正则"
        f"{sum(len(e['regexes']) for e in entries.values())} 条（PCRE 定界符剥除，/i 转 (?i)，"
        "Rust regex 编译失败运行时跳过）；空定界丢弃"
        f" {stats.get('regex_dropped', 0)} 条",
        f"//! - 主动探测：snmpget 块 {stats['snmpget']} 个，转换 "
        f"{sum(len(e['probes']) for e in entries.values())} 项检查、"
        f"丢弃（op=!=/未知/缺 OID）{stats['probe_dropped']} 条、"
        "MIB 名格式 OID（MIB::obj，零 MIB 依赖无法解析）"
        f"{stats['probe_mibname_dropped']} 条",
        "//! - 多词元关键词保留（本项目全词元命中，比上游子串严格，漏报方向安全）",
    ]

    # mod.rs：聚合 + only + KIND_BY_OS
    mod_rs = header + [
        "",
        "use super::{VendorMib, VendorRule};",
        "",
        "/// 识别-only 品牌条目（无采集库）。",
        "pub(super) const fn only(rule: &'static VendorRule) -> VendorMib {",
        "    VendorMib { rule, oid_set: None }",
        "}",
        "",
        "/// 全部规则分桶：内置品牌级在前（词表更精、sysDescr 判定优先），",
        "/// 社区 OS 级按 id 首字母四桶（前缀匹配不受顺序影响，最长前缀胜出）。",
        "pub(super) static COMMUNITY: &[&[VendorMib]] = &[",
        "    builtin::BUILTIN,",
    ]
    for b in BUCKETS:
        mod_rs.append(f"    os_{b}::OS_{b.upper().replace('_', '')},")
    mod_rs += [
        "];",
        "",
        "/// OS id → 采集库归属（vendor_kind() 查表，miss → Generic 公共回退链）。",
        "/// cisco 组全产品线绑定 Cisco MIB 家族（探测失败自动回退公共链，激进",
        "/// 绑定安全）；华为仅 VRP 网络设备（oceanstor/smartax 等产品线走",
        "/// Generic）；新华三 Comware。",
        "pub(super) static KIND_BY_OS: &[(&str, crate::live::VendorKind)] = &[",
    ]
    for os_id, kind in kind_by_os:
        mod_rs.append(f'    ({rust_str(os_id)}, crate::live::VendorKind::{kind}),')
    mod_rs += [
        "];",
        "",
    ]
    for b in BUCKETS:
        mod_rs.append(f"mod os_{b};")
    mod_rs.append("mod builtin;")
    (OUT_DIR / "mod.rs").write_text("\n".join(mod_rs) + "\n", encoding="utf-8")

    # builtin.rs：16 条内置品牌（三家关联采集库；literal 构造不用 only）
    builtin_rs = header + [
        "",
        "use super::super::{VendorMib, VendorRule};",
        "",
        "pub(super) static BUILTIN: &[VendorMib] = &[",
    ]
    for os_id, display, aliases, prefixes, keywords, mib, note in BUILTIN_BRANDS:
        e = entries[os_id]
        pres = ", ".join("&[" + ", ".join(map(str, p)) + "]" for p in sorted(e["prefixes"]))
        oid_set = f"Some(&super::super::mibs::{mib})" if mib else "None"
        if note:
            builtin_rs.append(f"    // {note}")
        builtin_rs.append("    VendorMib {")
        builtin_rs.append(f"        rule: &VendorRule {{ id: {rust_str(os_id)}, display_name: {rust_str(display)}, aliases: &[")
        for a in aliases:
            builtin_rs.append(f"            {rust_str(a)},")
        builtin_rs.append(f"        ], sysobjectid_prefixes: &[{pres}], descr_keywords: &[")
        for kw in sorted(e["keywords"]):
            builtin_rs.append(f"            {rust_str(kw)},")
        builtin_rs.append("        ], descr_keywords_except: &[], descr_regexes: &[], probe_checks: &[], },")
        builtin_rs.append(f"        oid_set: {oid_set},")
        builtin_rs.append("    },")
    builtin_rs.append("];")
    (OUT_DIR / "builtin.rs").write_text("\n".join(builtin_rs) + "\n", encoding="utf-8")

    # os_*.rs：社区 OS 规则四桶（导入按需：ProbeCheck/ProbeOp 仅探测桶需要）
    for b in BUCKETS:
        has_probes = any(entries[o]["probes"] for o in per_bucket[b])
        probe_types = "ProbeCheck, ProbeOp, " if has_probes else ""
        bucket_rs = [
            f"//! 社区 OS 级规则（id 首字母 {b.replace('_', '–')} 桶）。"
            f"统计口径见 mod.rs 头注释，**勿手改**。",
            "",
            f"use super::super::{{{probe_types}VendorMib, VendorRule}};",
            "use super::only;",
            "",
            f"pub(super) static OS_{b.upper().replace('_', '')}: &[VendorMib] = &[",
        ]
        for os_id in per_bucket[b]:
            e = entries[os_id]
            pres = ", ".join("&[" + ", ".join(map(str, p)) + "]" for p in sorted(e["prefixes"]))
            bucket_rs.append(f"    // LibreNMS os_detection/{os_id}.yaml")
            bucket_rs.append(
                f"    only(&VendorRule {{ id: {rust_str(os_id)}, display_name: {rust_str(e['display'])}, aliases: &[], sysobjectid_prefixes: &[{pres}], descr_keywords: &["
            )
            for kw in sorted(e["keywords"]):
                bucket_rs.append(f"        {rust_str(kw)},")
            bucket_rs.append("        ], descr_keywords_except: &[], descr_regexes: &[")
            for rx in sorted(e["regexes"]):
                bucket_rs.append(f"        {rust_str(rx)},")
            bucket_rs.append("        ], probe_checks: &[")
            for oid_s, op, val in sorted(e["probes"]):
                bucket_rs.append(
                    f"        ProbeCheck {{ oid: {rust_str(oid_s)}, op: ProbeOp::{op}, "
                    f"value: {rust_str(val)} }},"
                )
            bucket_rs.append("        ], }),")
        bucket_rs.append("];")
        (OUT_DIR / f"os_{b}.rs").write_text("\n".join(bucket_rs) + "\n", encoding="utf-8")

    print(f"已生成 {OUT_DIR}（内置 {sum(1 for e in entries.values() if e['builtin'])} + "
          f"OS {len(os_rules)} = {len(entries)} 条，KIND_BY_OS {len(kind_by_os)} 条）")
    for g, kind in (("cisco", "Cisco"), ("comware", "H3c")):
        if g in kind_groups:
            print(f"vendor_kind [{g} 组 → {kind}]: {' '.join(sorted(kind_groups[g]))}")
    if stats["malformed"]:
        print(f"结构非映射（未转换）{stats['malformed']} 个")


if __name__ == "__main__":
    main()
