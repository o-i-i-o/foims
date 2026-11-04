#!/usr/bin/env python3
"""LibreNMS os_detection 全量 → vendor/community.rs 社区规则（OS 级入库）。

用法：
    python3 da-scripts/gen_vendor_community.py [--src DIR]

决策依据（docs/SNMP厂商MIB识别与绑定方案.md §5.3.5）：
按 LibreNMS 的 OS 级分组方式**直接入库**（社区验证数据），不再按品牌级
人工归并——本项目识别模型已升级为 sysObjectID 前缀最长匹配（LibreNMS
语义），OS 级规则与内置品牌级规则（同长度前缀内置优先）自然共存：
- 内置 16 品牌 + 三家采集库保留（人工核实的品牌归并，id/display 语义不变）；
- 社区 OS 级规则提供更深前缀细分（如 9=Cisco 下 ios/iosxe）与更广覆盖，
  命中后经 vendor_kind() 映射回采集库归属。

数据源：本地 sparse clone 的 os_detection 目录（约 300KB，不进仓库）：
    git clone --depth 1 --filter=blob:none --sparse https://github.com/librenms/librenms /tmp/librenms
    cd /tmp/librenms && git sparse-checkout set resources/definitions/os_detection

转换与排除规则（全部记录在生成文件头注释中）：
- 排除 type=server/unix 平台类（平台识别属 vendor/platform.rs 域）
- 排除 os 名与内置品牌 id 重复的 OS（内置规则优先）
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
- 无有效前缀/关键词/正则/探测的 OS 跳过（本项目识别模型表达不了）
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
# 生成目标（入库文件，勿手改——重跑脚本再生成）
OUT = Path(__file__).resolve().parent.parent / "crates/foims-snmp/src/vendor/community.rs"

# 内置品牌 id（vendor/mod.rs BUILTIN）：同名 OS 排除，内置规则优先
BUILTIN_IDS = {
    "cisco", "huawei", "h3c", "juniper", "dell", "hp", "aruba", "netgear",
    "tp-link", "linksys", "ubiquiti", "mikrotik", "extreme", "alcatel",
    "zyxel", "d-link",
}
# 平台 agent 企业号（vendor/platform.rs 注册表 + net-snmp）：
# 前缀第 7 弧为这些号的不作厂商证据（§3.1）
PLATFORM_AGENTS = {8072, 311, 12325, 30155, 42, 2, 6876}
# snmpget op → Rust ProbeOp 变体（vendor/mod.rs）；不在表内的 op（含 != 负
# 条件——几乎所有值都满足，无识别价值）丢弃
PROBE_OPS = {
    "exists": "Exists", "contains": "Contains", "starts": "Starts",
    "=": "Equals", "regex": "Regex",
}


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


def main() -> None:
    src = Path(sys.argv[sys.argv.index("--src") + 1]) if "--src" in sys.argv else DEFAULT_SRC
    if not src.is_dir():
        sys.exit(f"错误：数据源不存在：{src}（先 sparse clone，见脚本头注释）")

    rules = []           # (os, text, prefixes, keywords)
    kind_groups = {}     # vendor_kind 校准用：group → os 列表
    stats = Counter()
    skipped = []

    for path in sorted(src.glob("*.yaml")):
        stats["upstream"] += 1
        doc = yaml.safe_load(path.read_text(encoding="utf-8"))
        if not isinstance(doc, dict):
            stats["skipped"] += 1
            skipped.append(f"{path.stem}（结构非映射）")
            continue
        if str(doc.get("type", "")) in ("server", "unix"):
            stats["platform"] += 1
            continue
        if path.stem in BUILTIN_IDS:
            stats["builtin_dup"] += 1
            skipped.append(f"{path.stem}（与内置品牌 id 重复）")
            continue
        group = str(doc.get("group") or "")
        if group:
            kind_groups.setdefault(group, []).append(path.stem)

        prefixes, keywords = [], set()
        regexes, probes = set(), set()
        dropped_agent = 0
        regex_cnt = 0
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
                    block_prefixes.append(arcs)
            prefixes.extend(block_prefixes)
            # 关键词/正则仅从「纯 sysDescr 块」收集（无 sysObjectID、无
            # snmpget）：上游 AND 块中前缀已独立成立，子串/正则转换后
            # 只有跨设备通用词误命中风险、无识别增益（airos-af-ltu 教训）
            if not as_list(rule.get("sysObjectID")) and "snmpget" not in rule:
                for k in as_list(rule.get("sysDescr")):
                    k = str(k).lower()
                    if usable_keyword(k):
                        keywords.add(k)
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
                        regexes.add(rx)
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
                probes.add((oid_s, PROBE_OPS[op], str(g.get("value", ""))))
            if "snmpget" in rule:
                stats["snmpget"] += 1
        stats["regex"] += regex_cnt
        # 前缀去重（同前缀多 discovery 条目）并按弧序排序（生成输出稳定）
        prefixes = sorted({tuple(a) for a in prefixes})

        if not prefixes and not keywords and not regexes and not probes:
            stats["no_evidence"] += 1
            skipped.append(f"{path.stem}（无有效前缀/关键词/正则/探测）")
            continue
        stats["dropped_agent_prefix"] += dropped_agent
        rules.append((path.stem, str(doc.get("text") or path.stem), prefixes,
                      sorted(keywords), sorted(regexes), sorted(probes)))

    # —— 生成 Rust ——
    out = ["//! 社区 OS 级识别规则：LibreNMS os_detection 全量转换（GPL 上游，",
           "//! 只提取识别事实：sysObjectID 前缀 + sysDescr 关键词/正则 + 主动探测）。",
           f"//! 由 da-scripts/gen_vendor_community.py 于 {date.today()} 生成，**勿手改**：",
           "//! 更新上游后重跑脚本再生成；新增规则结构变化须同步 vendor/mod.rs。",
           "//!",
           f"//! 上游 {stats['upstream']} 个定义 → 本表 {len(rules)} 条 OS 规则。转换口径：",
           f"//! - 平台类（type=server/unix，走 platform.rs）{stats['platform']} 个；"
           f"与内置品牌 id 重复 {stats['builtin_dup']} 个（内置优先）",
           f"//! - 无有效证据（前缀/关键词/正则/探测全空）跳过 {stats['no_evidence']} 个",
           f"//! - 丢弃平台 agent 号前缀 {stats['dropped_agent_prefix']} 条"
           f"（8072 net-snmp 等，§3.1 不作厂商证据）",
           f"//! - 丢弃高风险关键词（单词元 ≤2 字符）{stats['kw_dropped']} 条、"
           f"孤儿关键词（AND 伙伴失效，块内前缀为平台号/含主动探测）{stats['kw_orphan_dropped']} 条",
           f"//! - 正则：sysDescr_regex/except 原始 {stats['regex']} 条，仅收纯 sysDescr 块正向正则"
           f"{sum(len(r[4]) for r in rules)} 条（PCRE 定界符剥除，/i 转 (?i)，Rust regex"
           f" 编译失败运行时跳过）；空定界丢弃 {stats.get('regex_dropped', 0)} 条",
           f"//! - 主动探测：snmpget 块 {stats['snmpget']} 个，转换 {sum(len(p) for *_, p in rules)} 项检查、"
           f"丢弃（op=!=/未知/缺 OID）{stats['probe_dropped']} 条、"
           f"MIB 名格式 OID（MIB::obj，零 MIB 依赖无法解析）{stats['probe_mibname_dropped']} 条",
           "//! - 多词元关键词保留（本项目全词元命中，比上游子串严格，漏报方向安全）",
           "",
           "use super::{ProbeCheck, ProbeOp, VendorMib, VendorRule, only};",
           "",
           "pub(super) static COMMUNITY: &[VendorMib] = &["]
    for os, text, prefixes, keywords, regexes, probes in rules:
        out.append(f"    // LibreNMS os_detection/{os}.yaml")
        pres = ", ".join("&[" + ", ".join(map(str, p)) + "]" for p in prefixes)
        out.append(f"    only(&VendorRule {{ id: {rust_str(os)}, display_name: {rust_str(text)}, aliases: &[], sysobjectid_prefixes: &[{pres}], descr_keywords: &[")
        for kw in keywords:
            out.append(f"        {rust_str(kw)},")
        out.append("        ], descr_keywords_except: &[], descr_regexes: &[")
        for rx in regexes:
            out.append(f"        {rust_str(rx)},")
        out.append("        ], probe_checks: &[")
        for oid_s, op, val in probes:
            out.append(
                f"        ProbeCheck {{ oid: {rust_str(oid_s)}, op: ProbeOp::{op}, "
                f"value: {rust_str(val)} }},"
            )
        out.append("        ], }),")
    out.append("];")
    OUT.write_text("\n".join(out) + "\n", encoding="utf-8")

    # —— vendor_kind 校准清单（stdout，人工核对 mod.rs 映射表）——
    print(f"已生成 {OUT}（{len(rules)} 条 OS 规则）")
    for g in ("cisco", "huawei", "h3c"):
        if g in kind_groups:
            print(f"vendor_kind 校准 [{g}]: {' '.join(sorted(kind_groups[g]))}")
    for extra in ("comware", "vrp"):
        if extra in kind_groups:
            print(f"vendor_kind 校准 [{extra}]: {' '.join(sorted(kind_groups[extra]))}")
    if skipped:
        print(f"跳过 {len(skipped)} 个：", "、".join(s.split("（")[0] for s in skipped[:30]),
              "..." if len(skipped) > 30 else "")


if __name__ == "__main__":
    main()
