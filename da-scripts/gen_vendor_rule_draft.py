#!/usr/bin/env python3
"""LibreNMS os_detection YAML → FOIMS vendor 注册表 Rust 草稿（半自动转换）。

用法：
    python3 da-scripts/gen_vendor_rule_draft.py <os名>...   # 如 junos vrp comware
    python3 da-scripts/gen_vendor_rule_draft.py --list      # 列出上游全部 os 名
    python3 da-scripts/gen_vendor_rule_draft.py --all [--src DIR] [--out DIR]
        # 全量转换：--src 为本地 sparse clone 的 os_detection 目录（缺省
        # /tmp/librenms/resources/definitions/os_detection，可用
        # git clone --depth 1 --filter=blob:none --sparse + sparse-checkout
        获取，避免逐文件 API 撞 GitHub 未认证 60 次/小时限速）；
        --out 为草稿输出目录（缺省 /tmp/librenms-rule-drafts，**不进仓库**，
        人工核对后按需摘取入库）

设计依据（docs/SNMP厂商MIB识别与绑定方案.md §5.3.5「按需拉取、不整目录拷贝」）：
产出一律为草稿（TODO/⚠ 标记强制人工核对）后才可粘入
crates/foims-snmp/src/vendor/ 注册表——新增厂商 = 新增厂商模块文件 +
registry 注册 + 单测，不动核心逻辑。全量转换仅作为「候选池」供人工筛选，
不自动入库（上游 822 个 OS 定义是 OS 级，本项目 VendorRule 是品牌级，
多个 OS 常须人工归并为一条品牌规则）。

转换规则（机械可转的预填、其余留注释）：
- sysObjectID 前缀 → enterprise_ids：仅取 1.3.6.1.4.1 之下第 7 弧（IANA
  企业号），去重升序；完整前缀保留在注释供核对（前缀深度本项目不使用）
- sysDescr 子串 → descr_keywords 预填（小写原样）：上游是子串匹配、本项目
  是词边界词元匹配（keyword_hit 按非字母数字分词后全词元命中），语义不同，
  含短词元（≤2 字符）或非字母数字字符的条目标 ⚠ 提示人工改写
- sysDescr_regex / sysDescr_regex_except → 不机械转换，正则去 /.../ 定界
  符后输出为注释，由人工改写为词元关键词或排除词
- snmpget 主动探测 → 本项目探测链不做主动 snmpget，输出注释忽略
- id/display_name 取上游 os/text 作建议值，aliases 留空由人工补
- type 为 server/unix 等平台类 OS 时提示：应考虑 platform.rs 而非 vendor
"""

import json
import re
import sys
import urllib.request
from collections import defaultdict
from datetime import date
from pathlib import Path

import yaml

# 上游个别文件用 `op: =`（YAML 1.1 的 = 即 value 隐式标签），PyYAML 默认
# 不识别；注册为普通标量即可（只影响该标签，如 boschcpp.yaml）
yaml.SafeLoader.add_constructor(
    "tag:yaml.org,2002:value", lambda loader, node: loader.construct_scalar(node)
)

API_BASE = "https://api.github.com/repos/librenms/librenms/contents/resources/definitions/os_detection"
# 本地 sparse clone 缺省位置（--all 模式数据源）
DEFAULT_SRC = Path("/tmp/librenms/resources/definitions/os_detection")
# 全量草稿输出目录（缺省仓库外：GPL 上游表达的转换草稿，人工核对后按需摘取）
DEFAULT_OUT = Path("/tmp/librenms-rule-drafts")
# GitHub API 要求自定义 User-Agent
HEADERS = {"User-Agent": "foims-rule-draft", "Accept": "application/vnd.github.raw"}


def fetch(url: str) -> bytes:
    """拉取 URL 内容，非 200 时终止并给出可读错误。"""
    req = urllib.request.Request(url, headers=HEADERS)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return resp.read()
    except urllib.error.HTTPError as e:
        if e.code == 404:
            sys.exit(f"错误：上游不存在该 os 文件（{url}）。可用 --list 查看全部 os 名")
        sys.exit(f"错误：拉取失败 HTTP {e.code}（{url}）")
    except urllib.error.URLError as e:
        sys.exit(f"错误：网络不可达（{url}）：{e.reason}")


def list_os_names() -> list[str]:
    """列出上游全部 os 名（目录内 .yaml 文件名，去扩展名）。"""
    req = urllib.request.Request(API_BASE, headers={"User-Agent": "foims-rule-draft"})
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            items = json.load(resp)
    except (urllib.error.URLError, json.JSONDecodeError) as e:
        sys.exit(f"错误：列目录失败：{e}")
    return sorted(item["name"][:-5] for item in items if item["name"].endswith(".yaml"))


def enterprise_of(prefix: str) -> int | None:
    """sysObjectID 前缀 → IANA 企业号（1.3.6.1.4.1 下第 7 弧），非企业分支返回 None。"""
    arcs = prefix.strip().strip(".").split(".")
    if len(arcs) >= 7 and arcs[:6] == ["1", "3", "6", "1", "4", "1"]:
        return int(arcs[6])
    return None


def as_list(value) -> list:
    """YAML 字段兼容标量/列表两种写法（sysObjectID 上游两种都出现过）。"""
    if value is None:
        return []
    return value if isinstance(value, list) else [value]


def rust_str(s: str) -> str:
    """转义为 Rust 字符串字面量。"""
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def keyword_risk(keyword: str) -> str:
    """词元匹配风险评估：分词后含 ≤2 字符短词元（如 'VRP (R) Software' 的 R）
    或含非字母数字字符（括号/正则元字符会被本项目分词吃掉，语义变化）时标 ⚠。"""
    tokens = [t for t in re.split(r"[^0-9a-zA-Z]+", keyword.lower()) if t]
    if not tokens:
        return " ⚠ 空关键词，须人工改写"
    if any(len(t) <= 2 for t in tokens):
        return " ⚠ 含短词元，词元匹配易误命中，须人工改写"
    if re.search(r"[^0-9a-zA-Z\s-]", keyword):
        return " ⚠ 含特殊字符，本项目分词口径与上游子串不同，须人工核对"
    return ""


def extract_facts(doc: dict) -> dict:
    """从 os_detection 文档提取识别事实（gen_draft 与全量统计共用）。"""
    facts = {
        "text": str(doc.get("text", "")),
        "type": str(doc.get("type", "")),
        "oid_prefixes": [],
        "descr_items": [],
        "regex_items": [],
        "except_items": [],
        "snmpget_items": [],
    }
    for rule in doc.get("discovery", []) or []:
        facts["oid_prefixes"].extend(str(p) for p in as_list(rule.get("sysObjectID")))
        facts["descr_items"].extend(str(s) for s in as_list(rule.get("sysDescr")))
        facts["regex_items"].extend(str(r) for r in as_list(rule.get("sysDescr_regex")))
        facts["except_items"].extend(str(r) for r in as_list(rule.get("sysDescr_regex_except")))
        facts["except_items"].extend(str(r) for r in as_list(rule.get("sysDescr_except")))
        if "snmpget" in rule:
            facts["snmpget_items"].append(json.dumps(rule["snmpget"], ensure_ascii=False))
    facts["enterprises"] = sorted(
        {e for p in facts["oid_prefixes"] if (e := enterprise_of(p)) is not None}
    )
    return facts


def gen_draft(os_name: str, doc: dict) -> str:
    """单个 os_detection 文档 → VendorRule Rust 草稿。"""
    f = extract_facts(doc)
    text = f["text"] or os_name
    lines = []

    lines.append(f"// ===== LibreNMS os_detection/{os_name}.yaml 草稿（拉取于 {date.today()}，人工核对后使用）=====")
    lines.append(f"// 上游元数据：os={os_name} / text={text} / group={doc.get('group', '-')} / type={f['type'] or '-'}")
    if f["type"] in ("server", "unix"):
        lines.append("// ⚠ 上游 type 为平台/OS 类：优先考虑进 vendor/platform.rs 平台注册表，而非厂商规则")
    if f["oid_prefixes"]:
        lines.append(f"// 完整 sysObjectID 前缀（本项目仅取第 7 弧企业号，深度信息不使用）：{', '.join(f['oid_prefixes'])}")
    else:
        lines.append("// 上游无 sysObjectID 证据（仅 sysDescr/主动探测识别）")
    for s in f["regex_items"]:
        lines.append(f"// sysDescr_regex（正则，须人工改写为词元关键词）：{s.strip('/')}")
    for s in f["except_items"]:
        lines.append(f"// sysDescr_except（正则，对应 descr_keywords_except 须人工改写）：{s.strip('/') if s.startswith('/') else s}")
    for s in f["snmpget_items"]:
        lines.append(f"// snmpget 主动探测（本项目探测链不做，忽略）：{s}")

    lines.append("only(&VendorRule {")
    lines.append(f"    id: {rust_str(os_name)},")
    lines.append(f"    display_name: {rust_str(text)},")
    lines.append("    aliases: &[],")
    if f["enterprises"]:
        ent = ", ".join(str(e) for e in f["enterprises"])
        lines.append(f"    enterprise_ids: &[{ent}],  // TODO 与 IANA enterprise-numbers 核对")
    else:
        lines.append("    enterprise_ids: &[],")
    lines.append("    descr_keywords: &[")
    # 小写后去重（上游常含大小写变体，如 'kernel JUNOS'/'kernel Junos'）
    for kw in sorted({k.lower() for k in f["descr_items"]}):
        lines.append(f"        {rust_str(kw)},{keyword_risk(kw)}")
    if not f["descr_items"]:
        lines.append("        // 上游无 sysDescr 子串证据")
    lines.append("    ],")
    lines.append("    descr_keywords_except: &[],  // TODO 由上方 except 注释人工改写")
    lines.append("}),")
    return "\n".join(lines)


# 现有 vendor 注册表的品牌级企业号（vendor/mod.rs registry），用于汇总对照：
# 标记哪些上游 OS 命中已收录品牌、哪些是新候选
KNOWN_ENTERPRISES = {
    9: "cisco", 2011: "huawei", 25506: "h3c", 2636: "juniper", 674: "dell",
    11: "hp", 14823: "aruba", 4526: "netgear", 4413: "ubiquiti", 10002: "ubiquiti",
    41112: "ubiquiti", 14988: "mikrotik", 1916: "extreme", 6486: "alcatel",
    171: "d-link",
}


def run_all(src: Path, out: Path) -> None:
    """全量转换本地 os_detection 目录 → 每 OS 一份草稿 + 汇总统计。

    输出目录在仓库外：草稿是 GPL 上游表达的转换产物，仅作人工核对候选池，
    不进仓库、不自动入库。
    """
    if not src.is_dir():
        sys.exit(f"错误：数据源目录不存在：{src}\n先获取上游（约 300KB，不进仓库）：\n"
                 f"  git clone --depth 1 --filter=blob:none --sparse https://github.com/librenms/librenms /tmp/librenms\n"
                 f"  cd /tmp/librenms && git sparse-checkout set resources/definitions/os_detection")
    out.mkdir(parents=True, exist_ok=True)

    stats = {"total": 0, "skipped": 0, "platform_hint": 0, "with_ent": 0,
             "descr_only": 0, "weakest": 0, "risk_kw": 0, "regex": 0, "snmpget": 0}
    by_enterprise = defaultdict(list)   # 企业号 → [os 名]（同企业号多 OS 需人工归并）
    skipped = []

    for path in sorted(src.glob("*.yaml")):
        stats["total"] += 1
        try:
            doc = yaml.safe_load(path.read_text(encoding="utf-8"))
        except yaml.YAMLError as e:
            stats["skipped"] += 1
            skipped.append(f"{path.stem}（YAML 解析失败：{e}）")
            continue
        if not isinstance(doc, dict):
            stats["skipped"] += 1
            skipped.append(f"{path.stem}（顶层结构非映射）")
            continue

        f = extract_facts(doc)
        (out / f"{path.stem}.rs").write_text(gen_draft(path.stem, doc) + "\n", encoding="utf-8")

        if f["type"] in ("server", "unix"):
            stats["platform_hint"] += 1
        if f["enterprises"]:
            stats["with_ent"] += 1
            for e in f["enterprises"]:
                by_enterprise[e].append(path.stem)
        elif f["descr_items"]:
            stats["descr_only"] += 1
        else:
            stats["weakest"] += 1
        stats["risk_kw"] += sum(1 for kw in f["descr_items"] if keyword_risk(kw.lower()))
        stats["regex"] += len(f["regex_items"]) + len(f["except_items"])
        stats["snmpget"] += len(f["snmpget_items"])

    # 汇总报告（Markdown，含与现有注册表的对照 + 归并提示）
    lines = [
        f"# LibreNMS os_detection 全量转换汇总（{date.today()}）",
        "",
        f"- 上游文件：{stats['total']} 个，跳过 {stats['skipped']} 个",
        f"- 有 sysObjectID 企业号证据：{stats['with_ent']}；仅 sysDescr 子串：{stats['descr_only']}；"
        f"两者皆无（仅正则/主动探测，最弱）：{stats['weakest']}",
        f"- 平台类（type=server/unix，走 platform.rs 而非 vendor）：{stats['platform_hint']}",
        f"- ⚠ 风险关键词（短词元/特殊字符，须人工改写）：{stats['risk_kw']} 条",
        f"- 正则证据（须人工改写）：{stats['regex']} 条；主动探测（忽略）：{stats['snmpget']} 条",
        "",
        "## 企业号 → 上游 OS 聚合（本项目 VendorRule 是品牌级，同企业号多 OS 须人工归并为一条规则）",
        "",
        "| 企业号 | 现有注册表 | 上游 OS |",
        "|---|---|---|",
    ]
    for ent in sorted(by_enterprise):
        known = KNOWN_ENTERPRISES.get(ent, "—（新候选）")
        lines.append(f"| {ent} | {known} | {', '.join(by_enterprise[ent])} |")
    if skipped:
        lines += ["", "## 跳过文件", ""] + [f"- {s}" for s in skipped]
    (out / "_SUMMARY.md").write_text("\n".join(lines) + "\n", encoding="utf-8")

    print(f"转换完成：{stats['total'] - stats['skipped']}/{stats['total']} 份草稿 → {out}/")
    print(f"汇总报告：{out}/_SUMMARY.md（企业号聚合 {len(by_enterprise)} 组）")
    print("提示：草稿不进仓库，人工核对 TODO/⚠ 后按需摘取入 vendor/ 注册表")


def main() -> None:
    args = sys.argv[1:]

    if not args:
        sys.exit(__doc__)
    if args == ["--list"]:
        names = list_os_names()
        print(f"上游共 {len(names)} 个 os_detection 定义：")
        print(" ".join(names))
        return
    if args and args[0] == "--all":
        src, out = DEFAULT_SRC, DEFAULT_OUT
        rest = args[1:]
        for i, a in enumerate(rest):
            if a == "--src" and i + 1 < len(rest):
                src = Path(rest[i + 1])
            elif a == "--out" and i + 1 < len(rest):
                out = Path(rest[i + 1])
        run_all(src, out)
        return

    for os_name in args:
        raw = fetch(f"{API_BASE}/{os_name}.yaml")
        doc = yaml.safe_load(raw)
        if not isinstance(doc, dict):
            sys.exit(f"错误：{os_name}.yaml 结构非预期（顶层应为映射），须人工处理")
        print(gen_draft(os_name, doc))
        print()


if __name__ == "__main__":
    main()
