#!/usr/bin/env python3
"""从 LibreNMS os_discovery 生成 foims-snmp 的采集规则表。

与 `gen_vendor_community.py`（只取 os_detection/识别域）互补：本脚本读取
`resources/definitions/os_discovery/*.yaml`，把 CPU、内存、温度三类采集规则
固化为 `crates/foims-snmp/src/vendor/os_discovery.rs`。

OID 来源分两种：
- processors / sensors.temperature 自带 `num_oid`，直接取数字弧；
- mempools 只有 MIB 名（`MIB::object`），需借助 net-snmp 的
  `snmptranslate` + LibreNMS `mibs/` 目录在生成期解析。解析失败的条目跳过，
  不影响 CPU/温度数据生成。

用法（开发期，外部资料池不进仓库）：
    git clone --depth 1 --filter=blob:none --sparse https://github.com/librenms/librenms /tmp/librenms
    cd /tmp/librenms
    git sparse-checkout set resources/definitions/os_discovery mibs

    python3 da-scripts/gen_vendor_discovery.py \
        --src /tmp/librenms/resources/definitions/os_discovery \
        --mibs /tmp/librenms/mibs

依赖：PyYAML；解析内存 MIB 时需要 net-snmp（snmptranslate）。缺依赖时会
跳过 mempools 并继续生成 CPU/温度规则。
"""

from __future__ import annotations

import argparse
import collections
import glob
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

import yaml

# os_discovery 里模板化数值字段的固定写法
INDEX_RE = re.compile(r"\{\{\s*\$index\s*\}\}")
NOINDEX_RE = re.compile(r"\{\{\s*\$noindex\s*\}\}")
MIB_NAME_RE = re.compile(r"^[A-Za-z0-9_-]+::[A-Za-z0-9_.-]+$")


def load_yaml(path: Path) -> dict:
    # LibreNMS YAML 里有裸 `=`（YAML value 标记），PyYAML 默认会报错
    yaml.SafeLoader.add_constructor(
        "tag:yaml.org,2002:value",
        lambda loader, node: loader.construct_scalar(node),
    )
    with path.open(encoding="utf-8") as fh:
        return yaml.safe_load(fh) or {}


def module_data(doc: dict, *keys: str) -> list:
    node = doc.get("modules") or {}
    for key in keys:
        node = (node or {}).get(key) or {}
    return node.get("data") or []


def parse_num_oid(raw) -> tuple[list[int], bool] | None:
    """把 `.1.3...{{ $index }}` 拆成（数字弧, 是否表列）。

    `{{ $noindex }}` 是标量占位，生成期补 `.0`；只接受占位符在末尾且
    没有复合索引的模板，其余条目跳过。
    """
    text = str(raw or "").strip()
    if not text.startswith("."):
        return None

    match = INDEX_RE.search(text)
    if match:
        if text[match.end() :].strip():
            return None
        suffix = text[: match.start()].rstrip()
        arcs = _arcs(suffix)
        return (arcs, True) if arcs else None

    match = NOINDEX_RE.search(text)
    if match:
        if text[match.end() :].strip():
            return None
        suffix = text[: match.start()].rstrip()
        arcs = _arcs(suffix)
        return (arcs + [0], False) if arcs else None

    return None


def _arcs(text: str) -> list[int] | None:
    body = text.strip().strip(".")
    if not body:
        return None
    try:
        return [int(part) for part in body.split(".") if part]
    except ValueError:
        return None


def as_int(value, default: int = 1) -> int:
    try:
        return int(str(value))
    except (TypeError, ValueError):
        return default


class MibResolver:
    """按 MIB 模块批量调用 snmptranslate，解析 `MIB::object` → 数字弧。"""

    def __init__(self, mib_root: Path | None):
        self.enabled = bool(mib_root and mib_root.is_dir() and shutil.which("snmptranslate"))
        self.cache: dict[str, list[int] | None] = {}
        self.mib_dirs = ""
        if self.enabled:
            dirs = {str(mib_root)}
            dirs.update(str(p.parent) for p in mib_root.rglob("*") if p.is_file())
            self.mib_dirs = ":".join(sorted(dirs))

    def resolve(self, name: str) -> list[int] | None:
        if name in self.cache:
            return self.cache[name]
        if not self.enabled or not MIB_NAME_RE.match(name):
            self.cache[name] = None
            return None

        module, obj = name.split("::", 1)
        cmd = [
            "snmptranslate",
            "-M",
            self.mib_dirs,
            "-On",
            "-m",
            f"+{module}",
            f"{module}::{obj}",
        ]
        try:
            proc = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        except (OSError, subprocess.TimeoutExpired):
            self.cache[name] = None
            return None

        arcs = None
        for line in proc.stdout.splitlines():
            line = line.strip()
            if line.startswith("."):
                arcs = _arcs(line)
                break
        self.cache[name] = arcs
        return arcs

    def resolve_names(self, names: set[str]) -> dict[str, list[int]]:
        """预解析一批名字；返回值只包含成功的项。"""
        resolved: dict[str, list[int]] = {}
        by_module: dict[str, list[str]] = collections.defaultdict(list)
        for name in names:
            if not self.enabled or not MIB_NAME_RE.match(name):
                continue
            module, obj = name.split("::", 1)
            by_module[module].append(obj)

        for module, objects in sorted(by_module.items()):
            objects = sorted(set(objects))
            cmd = [
                "snmptranslate",
                "-M",
                self.mib_dirs,
                "-On",
                "-m",
                f"+{module}",
            ] + [f"{module}::{obj}" for obj in objects]
            try:
                proc = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
                lines = [line.strip() for line in proc.stdout.splitlines() if line.strip()]
            except (OSError, subprocess.TimeoutExpired):
                lines = []

            if len(lines) == len(objects) and all(line.startswith(".") for line in lines):
                for obj, line in zip(objects, lines):
                    arcs = _arcs(line)
                    if arcs:
                        resolved[f"{module}::{obj}"] = arcs
            else:
                # 批量输出与输入无法对齐时逐个回退，避免错配
                for obj in objects:
                    arcs = self.resolve(f"{module}::{obj}")
                    if arcs:
                        resolved[f"{module}::{obj}"] = arcs
        return resolved


def column(arcs: list[int], indexed: bool) -> str:
    body = ", ".join(str(arc) for arc in arcs)
    return f"OidColumn {{ base: &[{body}], indexed: {'true' if indexed else 'false'} }}"


def cpu_rules(doc: dict) -> list[tuple[list[int], bool, int]]:
    rules: list[tuple[list[int], bool, int]] = []
    for entry in module_data(doc, "processors"):
        parsed = parse_num_oid(entry.get("num_oid"))
        if not parsed:
            continue
        arcs, indexed = parsed
        precision = as_int(entry.get("precision"), 1) or 1
        rule = (arcs, indexed, precision)
        if rule not in rules:
            rules.append(rule)
    return rules


def temp_rules(doc: dict) -> list[tuple[list[int], bool, int, int]]:
    rules: list[tuple[list[int], bool, int, int]] = []
    for entry in module_data(doc, "sensors", "temperature"):
        # user_func 需要执行 PHP 回调，无法静态翻译
        if entry.get("user_func"):
            continue
        parsed = parse_num_oid(entry.get("num_oid"))
        if not parsed:
            continue
        arcs, indexed = parsed
        divisor = as_int(entry.get("divisor"), 1)
        multiplier = as_int(entry.get("multiplier"), 1)
        if divisor <= 0 or multiplier <= 0:
            continue
        rule = (arcs, indexed, divisor, multiplier)
        if rule not in rules:
            rules.append(rule)
    return rules


def mem_entries(doc: dict) -> list[dict]:
    return list(module_data(doc, "mempools"))


def mem_rule_shape(entry: dict) -> str | None:
    has_total = "total" in entry
    has_used = "used" in entry
    has_free = "free" in entry
    has_pct = "percent_used" in entry
    if has_pct and has_total:
        return "PercentTotal"
    if has_used and has_free:
        return "Pool"
    if has_pct:
        return "Percent"
    if has_total and has_free:
        return "TotalFree"
    if has_total and has_used:
        return "UsedTotal"
    return None


def mem_index(entry: dict) -> tuple[bool, int] | None:
    """返回（是否表列, 标量实例号）；复合索引返回 None 表示跳过。"""
    if entry.get("oid"):
        return True, 0
    index = entry.get("index")
    if index is None:
        return False, 0
    if isinstance(index, int):
        return False, index
    if isinstance(index, str):
        text = index.strip()
        if text in ("{{ $index }}", ".{{ $index }}"):
            return True, 0
        if text.isdigit():
            return False, int(text)
    return None


def build_mem_rules(doc: dict, resolved: dict[str, list[int]]) -> list[str]:
    rules: list[str] = []
    for entry in mem_entries(doc):
        shape = mem_rule_shape(entry)
        if shape is None:
            continue
        indexing = mem_index(entry)
        if indexing is None:
            continue
        indexed, scalar_index = indexing
        precision = as_int(entry.get("precision"), 1)
        if precision <= 0:
            precision = 1

        def col(field: str) -> str | None:
            name = entry.get(field)
            arcs = resolved.get(name) if isinstance(name, str) else None
            if not arcs:
                return None
            return column(arcs if indexed else arcs + [scalar_index], indexed)

        if shape == "Pool":
            used, free = col("used"), col("free")
            if used and free:
                rules.append(
                    f"MemRule::Pool {{ used: {used}, free: {free}, precision: {precision} }}"
                )
        elif shape == "PercentTotal":
            percent, total = col("percent_used"), col("total")
            if percent and total:
                rules.append(
                    f"MemRule::PercentTotal {{ percent: {percent}, total: {total}, "
                    f"precision: {precision} }}"
                )
        elif shape == "Percent":
            percent = col("percent_used")
            if percent:
                rules.append(f"MemRule::Percent {{ percent: {percent} }}")
        elif shape == "TotalFree":
            total, free = col("total"), col("free")
            if total and free:
                rules.append(
                    f"MemRule::TotalFree {{ total: {total}, free: {free}, "
                    f"precision: {precision} }}"
                )
        elif shape == "UsedTotal":
            total, used = col("total"), col("used")
            if total and used:
                rules.append(
                    f"MemRule::UsedTotal {{ total: {total}, used: {used}, "
                    f"precision: {precision} }}"
                )
    # 同一 OS 可能有重复条目（不同型号共用），去重但保持 YAML 顺序
    return list(dict.fromkeys(rules))


def render(profiles: list[tuple[str, list, list[str], list]]) -> str:
    lines = [
        "//! LibreNMS os_discovery 采集规则（CPU/内存/温度）。",
        "//!",
        "//! 由 `da-scripts/gen_vendor_discovery.py` 生成，**勿手改**。",
        "//! 运行时零 MIB 依赖：所有 OID 已固化为数字弧。",
        "",
        "use super::discovery::{CpuRule, DiscoveryProfile, MemRule, OidColumn, TempRule};",
        "",
        "/// 按 OS id 升序排列（运行时二分查找）。",
        "#[rustfmt::skip]",
        "pub(super) static PROFILES: &[(&str, DiscoveryProfile)] = &[",
    ]
    for os_id, cpu, mem, temp in profiles:
        lines.append("    (")
        lines.append(f'        "{os_id}",')
        lines.append("        DiscoveryProfile {")
        lines.append("            cpu: &[")
        for arcs, indexed, precision in cpu:
            lines.append(
                f"                CpuRule {{ column: {column(arcs, indexed)}, "
                f"precision: {precision} }},"
            )
        lines.append("            ],")
        lines.append("            mem: &[")
        for rule in mem:
            lines.append(f"                {rule},")
        lines.append("            ],")
        lines.append("            temp: &[")
        for arcs, indexed, divisor, multiplier in temp:
            lines.append(
                f"                TempRule {{ column: {column(arcs, indexed)}, "
                f"divisor: {divisor}, multiplier: {multiplier} }},"
            )
        lines.append("            ],")
        lines.append("        },")
        lines.append("    ),")
    lines.extend(["];", ""])
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--src",
        default="/tmp/librenms/resources/definitions/os_discovery",
        help="os_discovery YAML 目录",
    )
    parser.add_argument(
        "--mibs",
        default="/tmp/librenms/mibs",
        help="LibreNMS mibs 目录（用于内存 MIB 名解析；缺失则跳过内存规则）",
    )
    parser.add_argument(
        "--out",
        default="crates/foims-snmp/src/vendor/os_discovery.rs",
        help="生成的 Rust 文件",
    )
    args = parser.parse_args()

    src = Path(args.src)
    if not src.is_dir():
        print(f"os_discovery 目录不存在: {src}", file=sys.stderr)
        return 1

    docs = {}
    for path in sorted(glob.glob(str(src / "*.yaml"))):
        os_id = Path(path).stem
        if os_id.startswith("."):
            continue
        try:
            docs[os_id] = load_yaml(Path(path))
        except Exception as exc:  # noqa: BLE001 - 生成脚本需跳过坏文件并提示
            print(f"跳过 {path}: {exc}", file=sys.stderr)

    # 先收集内存 MIB 名，批量翻译
    mib_names: set[str] = set()
    for doc in docs.values():
        for entry in mem_entries(doc):
            for field in ("total", "used", "free", "percent_used"):
                value = entry.get(field)
                if isinstance(value, str) and "::" in value:
                    mib_names.add(value)

    resolver = MibResolver(Path(args.mibs) if args.mibs else None)
    if not resolver.enabled:
        print("未找到 snmptranslate 或 mibs 目录，跳过 mempools 生成", file=sys.stderr)
        resolved: dict[str, list[int]] = {}
    else:
        resolved = resolver.resolve_names(mib_names)

    profiles = []
    cpu_count = mem_count = temp_count = 0
    for os_id in sorted(docs):
        doc = docs[os_id]
        cpu = cpu_rules(doc)
        mem = build_mem_rules(doc, resolved)
        temp = temp_rules(doc)
        if not (cpu or mem or temp):
            continue
        cpu_count += len(cpu)
        mem_count += len(mem)
        temp_count += len(temp)
        profiles.append((os_id, cpu, mem, temp))

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(render(profiles), encoding="utf-8")
    print(
        f"生成 {out}: {len(profiles)} 个 OS, CPU {cpu_count} 条, "
        f"内存 {mem_count} 条, 温度 {temp_count} 条"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
