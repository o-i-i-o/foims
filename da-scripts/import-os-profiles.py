#!/usr/bin/env python3
"""SNMP profile 一次性导入脚本（docs/SNMP模块重构方案.md Step 2）。

把仓库内存量数据转换为 vendor/profiles/*.toml 新格式：
  - crates/foims-snmp/src/vendor/community/*.rs   识别规则（808 条）
  - crates/foims-snmp/src/vendor/os_discovery.rs  采集规则（CPU/内存/温度）
  - crates/foims-snmp/src/vendor/profiles_default.toml  内置三家默认采集库
导入完成后本脚本归档到 da-scripts/archive/，不再维护。

规则映射：
  - KIND_BY_OS（community/mod.rs）→ group 归属（cisco/huawei/h3c）；
  - 识别规则 → [profile.match]（前缀/关键词/正则/probes）；
  - 采集规则 → [profile.collect.*]（cpu/mem/temp 候选）；
  - descr_keywords_except（置信度降级词）随 Confidence 一起删除；
  - 空证据占位规则（无任何可转换证据）整体跳过并计入报告。

用法：python3 da-scripts/import-os-profiles.py [--check]
  --check 仅打印对账报告，不写文件。
"""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SNMP_SRC = ROOT / "crates/foims-snmp/src/vendor"
OUT_DIR = SNMP_SRC / "profiles"

# KIND_BY_OS（community/mod.rs）→ group id
KIND_BY_OS = {
    "acs": "cisco", "acsw": "cisco", "apic": "cisco", "asa": "cisco",
    "cat1900": "cisco", "catos": "cisco", "cimc": "cisco", "cisco": "cisco",
    "ciscoepc": "cisco", "ciscome1200": "cisco", "ciscosce": "cisco",
    "ciscospa": "cisco", "ciscosrp": "cisco", "comware": "h3c", "ftd": "cisco",
    "fxos": "cisco", "h3c": "h3c", "huawei": "huawei", "ios": "cisco",
    "iosxe": "cisco", "iosxr": "cisco", "nxos": "cisco", "ons": "cisco",
    "pixos": "cisco", "sanos": "cisco", "viptela": "cisco", "vrp": "huawei",
    "waas": "cisco", "yunshan": "huawei",
}
GROUP_BRAND = {"cisco": "Cisco", "huawei": "Huawei", "h3c": "H3C"}

# 内置品牌级规则（community/builtin.rs）中非三家的：各自独立文件
BUILTIN_FILE_BRANDS = {
    "juniper", "dell", "hp", "aruba", "netgear", "tp-link", "linksys",
    "ubiquiti", "mikrotik", "extreme", "alcatel", "zyxel", "d-link", "buffalo",
}

# LibreNMS processor_precision → 列单位（内存/存储 precision 乘数）
MEM_PRECISION_UNIT = {1: None, 1024: "kb", 1048576: "mb"}

# Rust regex 不支持、且导入数据可能残留的 PCRE 特性（出现即丢弃该正则）
UNSAFE_REGEX_MARKERS = ("(?=", "(?!", "(?<", "(?>", "(?R", "\\1", "\\2", "*+", "++")


def fmt_str(s: str) -> str:
    """TOML 单行基础字符串转义。"""
    out = s.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{out}"'


def fmt_oid_list(prefixes: list[list[int]]) -> str:
    return "[" + ", ".join(fmt_oid(p) for p in prefixes) + "]"


def fmt_oid(prefix: list[int]) -> str:
    return '"' + ".".join(str(a) for a in prefix) + '"'


class Rule:
    """community VendorRule 条目。"""

    def __init__(self):
        self.id = ""
        self.display_name = ""
        self.aliases: list[str] = []
        self.prefixes: list[list[int]] = []
        self.keywords: list[str] = []
        self.keywords_except: list[str] = []
        self.regexes: list[str] = []
        self.probes: list[tuple[str, str, str]] = []  # (oid, op, value)
        self.source_file = ""

    @property
    def has_evidence(self) -> bool:
        return bool(self.prefixes or self.keywords or self.regexes or self.probes)


# ───────────────────────── Rust 数据解析 ─────────────────────────


def tokenize_rust_array(text: str, start: int) -> tuple[str, int]:
    """从 start（指向 '['）起取出平衡括号内容，返回（内容, 结束下标）。"""
    depth = 0
    i = start
    in_str = False
    while i < len(text):
        c = text[i]
        if in_str:
            if c == '"' and text[i - 1] != "\\":
                in_str = False
        elif c == '"':
            in_str = True
        elif c in "[{(":
            depth += 1
        elif c in "]})":
            depth -= 1
            if depth == 0:
                return text[start + 1 : i], i + 1
        i += 1
    raise ValueError("括号不平衡")


def split_top_items(body: str) -> list[str]:
    """按顶层逗号切分（忽略字符串与嵌套括号内的逗号）。"""
    items, buf, depth, in_str, prev = [], [], 0, False, ""
    for c in body:
        if in_str:
            buf.append(c)
            if c == '"' and prev != "\\":
                in_str = False
        elif c == '"':
            buf.append(c)
            in_str = True
        elif c in "[{(":
            depth += 1
            buf.append(c)
        elif c in "]})":
            depth -= 1
            buf.append(c)
        elif c == "," and depth == 0:
            items.append("".join(buf).strip())
            buf = []
        else:
            buf.append(c)
        prev = c
    tail = "".join(buf).strip()
    if tail:
        items.append(tail)
    return items


def parse_string_array(body: str) -> list[str]:
    return re.findall(r'"((?:[^"\\]|\\.)*)"', body)


def parse_prefix_array(body: str) -> list[list[int]]:
    """`&[1, 3, ...], &[...]` → 弧序列列表。"""
    out = []
    for m in re.finditer(r"&\[([0-9,\s]*)\]", body):
        arcs = [int(a) for a in re.split(r"[,\s]+", m.group(1)) if a.strip()]
        if arcs:
            out.append(arcs)
    return out


def parse_probe_checks(body: str) -> list[tuple[str, str, str]]:
    out = []
    for m in re.finditer(
        r"ProbeCheck\s*\{\s*oid:\s*\"([^\"]+)\",\s*op:\s*ProbeOp::(\w+),\s*value:\s*\"((?:[^\"\\]|\\.)*)\"",
        body,
    ):
        out.append((m.group(1), m.group(2), m.group(3)))
    return out


def parse_community_rules(path: Path) -> list[Rule]:
    """解析 community/*.rs 中的全部 VendorRule 字面量。"""
    text = path.read_text(encoding="utf-8")
    rules: list[Rule] = []
    for m in re.finditer(r"VendorRule\s*\{", text):
        body, _end = tokenize_rust_array(text, m.end() - 1)
        rule = Rule()
        rule.source_file = path.name
        for item in split_top_items(body):
            key, _, value = item.partition(":")
            key = key.strip()
            value = value.strip()
            if key == "id":
                rule.id = value.strip('"')
            elif key == "display_name":
                rule.display_name = value.strip('"')
            elif key == "aliases":
                rule.aliases = parse_string_array(value)
            elif key == "sysobjectid_prefixes":
                rule.prefixes = parse_prefix_array(value)
            elif key == "descr_keywords":
                rule.keywords = parse_string_array(value)
            elif key == "descr_keywords_except":
                rule.keywords_except = parse_string_array(value)
            elif key == "descr_regexes":
                rule.regexes = parse_string_array(value)
            elif key == "probe_checks":
                rule.probes = parse_probe_checks(value)
        if rule.id:
            rules.append(rule)
    return rules


# os_discovery.rs 采集规则解析

RE_OS_BLOCK = re.compile(r'\(\s*"([^"]+)"\s*,\s*DiscoveryProfile\s*\{')
RE_CPU = re.compile(
    r"CpuRule\s*\{\s*column:\s*OidColumn\s*\{\s*base:\s*&\[([0-9,\s]*)\]\s*,\s*indexed:\s*(true|false)\s*\}\s*,\s*precision:\s*(-?\d+)\s*\}"
)
RE_MEM_POOL = re.compile(
    r"MemRule::Pool\s*\{\s*used:\s*OidColumn\s*\{\s*base:\s*&\[([0-9,\s]*)\]\s*,\s*indexed:\s*(true|false)\s*\}\s*,\s*free:\s*OidColumn\s*\{\s*base:\s*&\[([0-9,\s]*)\]\s*,\s*indexed:\s*(true|false)\s*\}\s*,\s*precision:\s*(\d+)\s*\}"
)
RE_MEM_TOTAL_FREE = re.compile(
    r"MemRule::TotalFree\s*\{\s*total:\s*OidColumn\s*\{\s*base:\s*&\[([0-9,\s]*)\]\s*,\s*indexed:\s*(true|false)\s*\}\s*,\s*free:\s*OidColumn\s*\{\s*base:\s*&\[([0-9,\s]*)\]\s*,\s*indexed:\s*(true|false)\s*\}\s*,\s*precision:\s*(\d+)\s*\}"
)
RE_MEM_USED_TOTAL = re.compile(
    r"MemRule::UsedTotal\s*\{\s*total:\s*OidColumn\s*\{\s*base:\s*&\[([0-9,\s]*)\]\s*,\s*indexed:\s*(true|false)\s*\}\s*,\s*used:\s*OidColumn\s*\{\s*base:\s*&\[([0-9,\s]*)\]\s*,\s*indexed:\s*(true|false)\s*\}\s*,\s*precision:\s*(\d+)\s*\}"
)
RE_MEM_PERCENT_TOTAL = re.compile(
    r"MemRule::PercentTotal\s*\{\s*percent:\s*OidColumn\s*\{\s*base:\s*&\[([0-9,\s]*)\]\s*,\s*indexed:\s*(true|false)\s*\}\s*,\s*total:\s*OidColumn\s*\{\s*base:\s*&\[([0-9,\s]*)\]\s*,\s*indexed:\s*(true|false)\s*\}\s*,\s*precision:\s*(\d+)\s*\}"
)
RE_MEM_PERCENT = re.compile(
    r"MemRule::Percent\s*\{\s*percent:\s*OidColumn\s*\{\s*base:\s*&\[([0-9,\s]*)\]\s*,\s*indexed:\s*(true|false)\s*\}\s*\}"
)
RE_TEMP = re.compile(
    r"TempRule\s*\{\s*column:\s*OidColumn\s*\{\s*base:\s*&\[([0-9,\s]*)\]\s*,\s*indexed:\s*(true|false)\s*\}\s*,\s*divisor:\s*(\d+)\s*,\s*multiplier:\s*(\d+)\s*\}"
)


def arcs(s: str) -> str:
    return ".".join(a for a in re.split(r"[,\s]+", s.strip()) if a)


class CollectPlan:
    """os_discovery 单 OS 采集规则（保序）。"""

    def __init__(self):
        self.cpu: list[tuple[str, bool, int]] = []
        self.mem: list[dict] = []
        self.temp: list[tuple[str, bool, int, int]] = []


def parse_os_discovery(path: Path) -> dict[str, CollectPlan]:
    text = path.read_text(encoding="utf-8")
    plans: dict[str, CollectPlan] = {}
    marks = list(RE_OS_BLOCK.finditer(text))
    for i, m in enumerate(marks):
        os_id = m.group(1)
        end = marks[i + 1].start() if i + 1 < len(marks) else len(text)
        body = text[m.end() : end]
        plan = CollectPlan()
        for c in RE_CPU.finditer(body):
            plan.cpu.append((arcs(c.group(1)), c.group(2) == "true", int(c.group(3))))
        for c in RE_MEM_POOL.finditer(body):
            plan.mem.append({
                "kind": "pool", "roles": [("used", arcs(c.group(1)), c.group(2) == "true"),
                                          ("free", arcs(c.group(3)), c.group(4) == "true")],
                "precision": int(c.group(5)),
            })
        for c in RE_MEM_TOTAL_FREE.finditer(body):
            plan.mem.append({
                "kind": "pool", "roles": [("total", arcs(c.group(1)), c.group(2) == "true"),
                                          ("free", arcs(c.group(3)), c.group(4) == "true")],
                "precision": int(c.group(5)),
            })
        for c in RE_MEM_USED_TOTAL.finditer(body):
            plan.mem.append({
                "kind": "pool", "roles": [("total", arcs(c.group(1)), c.group(2) == "true"),
                                          ("used", arcs(c.group(3)), c.group(4) == "true")],
                "precision": int(c.group(5)),
            })
        for c in RE_MEM_PERCENT_TOTAL.finditer(body):
            plan.mem.append({
                "kind": "percent",
                "roles": [("percent", arcs(c.group(1)), c.group(2) == "true"),
                          ("total", arcs(c.group(3)), c.group(4) == "true")],
                "precision": int(c.group(5)),
            })
        for c in RE_MEM_PERCENT.finditer(body):
            plan.mem.append({
                "kind": "percent",
                "roles": [("percent", arcs(c.group(1)), c.group(2) == "true")],
                "precision": 1,
            })
        for c in RE_TEMP.finditer(body):
            plan.temp.append((arcs(c.group(1)), c.group(2) == "true", int(c.group(3)), int(c.group(4))))
        plans[os_id] = plan
    return plans


def parse_default_profiles(path: Path) -> dict[str, dict]:
    """profiles_default.toml → 品牌级采集库（cisco/huawei/h3c）。"""
    with path.open("rb") as f:
        data = tomllib.load(f)
    out: dict[str, dict] = {}
    for entry in data.get("profile", []):
        out[entry["id"]] = entry
    return out


# ───────────────────────── TOML 生成 ─────────────────────────


def column_block(table_role: str, role_name: str, oid: str, indexed: bool, unit: str | None,
                 indent: str, value: str | None = None,
                 precision: int | None = None,
                 divisor: int | None = None, multiplier: int | None = None) -> list[str]:
    """生成一个 column/field 子表块（field 带语义 role 键）。"""
    header = f"{indent}[[profile.collect.{{FAMILY}}.candidate.{table_role}]]"
    body = []
    if table_role == "field":
        body.append(f"{indent}role = {fmt_str(role_name)}")
    body.append(f"{indent}oid = {fmt_str(oid)}")
    if not indexed:
        body.append(f"{indent}indexed = false")
    if value:
        body.append(f"{indent}value = {fmt_str(value)}")
    if unit:
        body.append(f"{indent}unit = {fmt_str(unit)}")
    if precision is not None and precision != 1:
        body.append(f"{indent}precision = {precision}")
    if divisor is not None and divisor != 1:
        body.append(f"{indent}divisor = {divisor}")
    if multiplier is not None and multiplier != 1:
        body.append(f"{indent}multiplier = {multiplier}")
    return [header] + body


def render_collect(plan: CollectPlan, default: dict | None, family_indent: str) -> str:
    """生成 [profile.collect.*] 段（os_discovery 规则在前，内置默认在后）。"""
    out: list[str] = []

    def cand(family: str, head: list[str], columns: list[list[str]], kind: str | None = None):
        base = f"{family_indent}[[profile.collect.{family}.candidate]]"
        out.append(base)
        for h in head:
            out.append(f"{family_indent}{h}")
        if kind:
            out.append(f"{family_indent}kind = {fmt_str(kind)}")
        for col in columns:
            out.extend(l.replace("{FAMILY}", family) for l in col)

    # CPU：LibreNMS 规则 + 内置默认列，逐列一条候选（首个有值生效）
    cpu_columns: list[tuple[str, bool, int]] = list(plan.cpu)
    if default:
        for c in default.get("cpu", []):
            cpu_columns.append((c["oid"], True, 1))
    if cpu_columns:
        out.append(f"{family_indent}[profile.collect.cpu]")
        out.append(f"{family_indent}pick = {fmt_str('max')}")
        for oid, indexed, precision in cpu_columns:
            cand("cpu", [], [column_block("column", "", oid, indexed, None, family_indent,
                                          value="percent", precision=precision)])

    # 内存：规则 + 内置默认（profiles_default 的 mem 字段）
    mem_items: list[dict] = list(plan.mem)
    if default and "mem" in default:
        d = default["mem"]
        roles = []
        for role in ("name", "used", "free", "usage", "size_mega", "size_bytes"):
            if role in d:
                roles.append((role, d[role], True))
        if roles:
            mem_items.append({"kind": d.get("kind", "pool"), "roles": roles, "precision": 1})
    if mem_items:
        out.append(f"{family_indent}[profile.collect.mem]")
        for item in mem_items:
            unit = MEM_PRECISION_UNIT.get(item["precision"])
            extra_precision = None if item["precision"] in MEM_PRECISION_UNIT else item["precision"]
            columns = []
            for role, oid, indexed in item["roles"]:
                col_unit = unit if role in ("used", "free", "total") else None
                columns.append(column_block(
                    "field", role, oid, indexed, col_unit, family_indent,
                    precision=extra_precision if role in ("used", "free", "total") else None))
            cand("mem", [], columns, kind=item["kind"])

    # 温度：规则 + 内置默认列（摄氏度）
    temp_columns: list[tuple[str, bool, int, int]] = list(plan.temp)
    if default:
        for c in default.get("temp", []):
            temp_columns.append((c["oid"], True, 1, 1))
    if temp_columns:
        out.append(f"{family_indent}[profile.collect.temp]")
        for oid, indexed, divisor, multiplier in temp_columns:
            cand("temp", [], [column_block("column", "", oid, indexed, None, family_indent,
                                           divisor=divisor, multiplier=multiplier)])
    return "\n".join(out)


PROBE_OP_MAP = {"Equals": "eq", "Contains": "has", "Starts": "starts",
                "Exists": "exists", "Regex": "regex"}


def render_match(rule: Rule, indent: str) -> str:
    lines = [f"{indent}[profile.match]"]
    if rule.prefixes:
        lines.append(f"{indent}sysobjectid = {fmt_oid_list(rule.prefixes)}")
    if rule.keywords:
        lines.append(f"{indent}descr_keywords = [" + ", ".join(fmt_str(k) for k in rule.keywords) + "]")
    kept_regexes = [r for r in rule.regexes if not any(m in r for m in UNSAFE_REGEX_MARKERS)]
    if kept_regexes:
        lines.append(f"{indent}descr_regex = [" + ", ".join(fmt_str(r) for r in kept_regexes) + "]")
    for oid, op, value in rule.probes:
        lines.append("")
        lines.append(f"{indent}[[profile.match.probes]]")
        lines.append(f"{indent}oid = {fmt_str(oid)}")
        lines.append(f"{indent}op = {fmt_str(PROBE_OP_MAP.get(op, 'exists'))}")
        lines.append(f"{indent}value = {fmt_str(value)}")
    return "\n".join(lines)


def render_profile(rule: Rule, plan: CollectPlan | None) -> str:
    group = KIND_BY_OS.get(rule.id)
    brand = GROUP_BRAND.get(group) if group else rule.display_name
    # group 与品牌级 OS 规则同 id 时（cisco/huawei/h3c），OS 规则以 -brand 后缀入册
    os_id = f"{rule.id}-brand" if group and rule.id == group else rule.id
    lines = ["[[profile]]", f"id = {fmt_str(os_id)}", 'kind = "os"']
    if group:
        lines.append(f"group = {fmt_str(group)}")
    lines.append(f"brand = {fmt_str(brand)}")
    lines.append(f"display_name = {fmt_str(rule.display_name)}")
    if rule.has_evidence:
        lines.append("")
        lines.append(render_match(rule, ""))
    collect = render_collect(plan or CollectPlan(), None, "")
    if collect:
        lines.append("")
        lines.append(collect)
    return "\n".join(lines) + "\n"


def render_group_profile(group_id: str, plan: CollectPlan, default: dict | None) -> str:
    """品牌族 profile：共享采集方案，不参与识别。"""
    lines = ["[[profile]]", f'id = "{group_id}"', 'kind = "group"',
             f'brand = "{GROUP_BRAND[group_id]}"',
             f'display_name = "{GROUP_BRAND[group_id]}"']
    collect = render_collect(plan, default, "")
    if collect:
        lines.append("")
        lines.append(collect)
    return "\n".join(lines) + "\n"


# ───────────────────────── 主流程 ─────────────────────────


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="仅对账，不写文件")
    args = parser.parse_args()

    # 1. 识别规则（builtin 在前 = 描述通道优先级，保持原顺序导入）
    rules: dict[str, Rule] = {}
    order: list[str] = []
    duplicates = 0
    for name in ("community/builtin.rs", "community/os_a_g.rs", "community/os_h_n.rs",
                 "community/os_o_t.rs", "community/os_u_z.rs"):
        for rule in parse_community_rules(SNMP_SRC / name):
            if rule.id in rules:
                duplicates += 1
                continue
            rules[rule.id] = rule
            order.append(rule.id)

    # 2. 采集规则
    plans = parse_os_discovery(SNMP_SRC / "os_discovery.rs")
    defaults = parse_default_profiles(SNMP_SRC / "profiles_default.toml")

    report: list[str] = []
    empty_evidence = [r for r in rules.values() if not r.has_evidence]
    unreachable = sorted(set(plans) - set(rules))
    dropped_regexes = sum(
        len([r for r in rule.regexes if any(m in r for m in UNSAFE_REGEX_MARKERS)])
        for rule in rules.values()
    )
    except_keywords = sum(len(rule.keywords_except) for rule in rules.values())

    # 3. 文件分组
    files: dict[str, list[str]] = {}
    for group_id in ("cisco", "huawei", "h3c"):
        group_plan = plans.get(group_id, CollectPlan())
        files[f"{group_id}.toml"] = [render_group_profile(group_id, group_plan, defaults.get(group_id))]
    for os_id in order:
        rule = rules[os_id]
        if not rule.has_evidence:
            continue
        group = KIND_BY_OS.get(os_id)
        if group:
            fname = f"{group}.toml"
        elif os_id in BUILTIN_FILE_BRANDS:
            fname = f"{os_id}.toml"
        else:
            c = os_id[0].lower()
            fname = (f"community-a-c.toml" if c <= "c"
                     else "community-d-f.toml" if c <= "f"
                     else "community-g-i.toml" if c <= "i"
                     else "community-j-l.toml" if c <= "l"
                     else "community-m-o.toml" if c <= "o"
                     else "community-p-r.toml" if c <= "r"
                     else "community-s-u.toml" if c <= "u"
                     else "community-v-z.toml")
        # 组内顺序：group 条目已在文件首（上面已写入），OS 追加
        files.setdefault(fname, [])
        files[fname].append(render_profile(rule, plans.get(os_id)))

    report.append(f"识别规则总数（VendorRule）: {len(rules) + duplicates}（去重 {duplicates}）")
    report.append(f"空证据占位跳过: {len(empty_evidence)} 条: " +
                  ", ".join(sorted(r.id for r in empty_evidence)))
    report.append(f"os_discovery 采集 OS 数: {len(plans)}")
    report.append(f"无识别规则、不可达的采集条目（丢弃）: {len(unreachable)} 条: " +
                  ", ".join(unreachable))
    report.append(f"丢弃 descr_regex（Rust regex 不支持特性）: {dropped_regexes} 条")
    report.append(f"删除 descr_keywords_except（Confidence 随方案删除）: {except_keywords} 个关键词")
    report.append(f"导入 probe_checks: {sum(len(r.probes) for r in rules.values())} 条")
    for fname in sorted(files):
        lines = sum(c.count("\n") for c in files[fname])
        report.append(f"  {fname}: {len(files[fname])} 条 profile，约 {lines} 行")

    print("\n".join(report))
    if args.check:
        return 0

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for fname, parts in sorted(files.items()):
        (OUT_DIR / fname).write_text("\n".join(parts), encoding="utf-8")
    print(f"\n已写入 {OUT_DIR}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
