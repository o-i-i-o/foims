#!/bin/bash
# 静态资源版本号维护脚本（时间戳方案）
#
# 项目约定（docs/code-style.md §3.5）：改动 JS/CSS 后需同步 bump 三个入口页的
# ?v= 与 resourceLoader.js 的 MODULE_VERSION（必须同号，由
# cargo test --test frontend_consistency 强制校验）。本脚本用一次调用
# 替代手工四处替换，并自带与 Rust 测试相同的一致性复核。
#
# 用法：
#   bash test-scripts/bump_static_version.sh              # 版本号 ← 当前 Unix 时间戳（秒）
#   bash test-scripts/bump_static_version.sh 1761234567   # 写入指定版本号（纯数字）
#   bash test-scripts/bump_static_version.sh --show       # 仅显示当前版本号
#   bash test-scripts/bump_static_version.sh --check      # 仅校验四处版本号是否一致
#
# 设计说明：
#   - 版本号为 Unix 时间戳（秒）：递增、唯一、与提交内容解耦，重复 bump 不会冲突
#   - 单调保护：生成的探版本号若不大于当前版本（同一秒内连按两次、时钟回拨），
#     自动取 当前版本+1，保证任何一次 bump 都能击穿浏览器/nginx 缓存
#   - 写入采用"归一化替换"（页面内所有 ?v= 一律改为目标值），即使个别文件
#     此前漏改也能一次对齐
#   - 本脚本只改版本号，不负责提交；功能性变更另按 0.x.yy 规则 bump Cargo.toml

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# 版本号的四处落点：三个入口页 + ES 模块加载器
ENTRY_PAGES=(
    "web/static/main.html"
    "web/static/index.html"
    "web/static/init_index.html"
)
MODULE_LOADER="web/static/js/utils/resourceLoader.js"

for f in "${ENTRY_PAGES[@]}" "$MODULE_LOADER"; do
    if [[ ! -f "$PROJECT_DIR/$f" ]]; then
        echo "错误：找不到 $PROJECT_DIR/$f（请在 foims 仓库内运行）" >&2
        exit 1
    fi
done

LOADER_PATH="$PROJECT_DIR/$MODULE_LOADER"

# 读取 MODULE_VERSION 作为当前版本（resourceLoader.js 是版本号的权威源）
current_version() {
    local v
    v=$(sed -nE 's/.*MODULE_VERSION = "([0-9]+)".*/\1/p' "$LOADER_PATH")
    if [[ -z "$v" ]]; then
        echo "错误：$MODULE_LOADER 中未找到数字格式的 MODULE_VERSION" >&2
        exit 1
    fi
    echo "$v"
}

# 收集单个文件内全部 ?v= 的值（每行一个）；无匹配时输出为空
collect_page_versions() {
    grep -oE '\?v=[0-9]+' "$1" | sed -E 's/.*=//' || true
}

# 一致性复核：与 tests/frontend_consistency.rs 的 asset_versions_must_be_uniform
# 同一套断言——每个入口页至少一个 ?v=、页内全部同号、且等于 MODULE_VERSION
check_uniform() {
    local expected
    expected=$(current_version)
    local f v page_first
    for f in "${ENTRY_PAGES[@]}"; do
        page_first=""
        while IFS= read -r v; do
            if [[ -z "$page_first" ]]; then
                page_first="$v"
            elif [[ "$v" != "$page_first" ]]; then
                echo "不一致：$f 内存在 ?v=$page_first 与 ?v=$v 并存" >&2
                return 1
            fi
        done < <(collect_page_versions "$PROJECT_DIR/$f")
        if [[ -z "$page_first" ]]; then
            echo "不一致：$f 内未找到任何 ?v= 版本号引用" >&2
            return 1
        fi
        if [[ "$page_first" != "$expected" ]]; then
            echo "不一致：$f 的 ?v=$page_first 与 MODULE_VERSION($expected) 不同" >&2
            return 1
        fi
    done
    echo "版本号一致：$expected"
    return 0
}

# 归一化写入：入口页全部 ?v= 与 MODULE_VERSION 统一改为目标值
apply_version() {
    local new="$1" f
    for f in "${ENTRY_PAGES[@]}"; do
        sed -i -E "s/\?v=[0-9]+/?v=${new}/g" "$PROJECT_DIR/$f"
    done
    sed -i -E "s/(MODULE_VERSION = \")[0-9]+(\")/\1${new}\2/" "$LOADER_PATH"
}

# 计算新版本号：默认取当前时间戳；不大于当前版本时取 当前+1（单调保护）
next_version() {
    local cur="$1" now new
    cur=$((10#$cur))
    now=$(date +%s)
    if ((now <= cur)); then
        new=$((cur + 1))
    else
        new=$now
    fi
    echo "$new"
}

usage() {
    sed -n '2,14p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

main() {
    local cur new
    case "${1:-}" in
        --show)
            echo "当前版本号：$(current_version)"
            ;;
        --check)
            check_uniform
            ;;
        "")
            cur=$(current_version)
            new=$(next_version "$cur")
            apply_version "$new"
            check_uniform
            echo "已 bump：$cur → $new（Unix 时间戳，秒）"
            echo "提示：功能性变更请另按 0.x.yy 规则 bump Cargo.toml"
            ;;
        *)
            if [[ ! "$1" =~ ^[0-9]+$ ]]; then
                echo "错误：版本号只能是纯数字（Unix 时间戳），收到 '$1'" >&2
                usage >&2
                exit 1
            fi
            cur=$(current_version)
            if ((10#$1 < 10#$cur)); then
                echo "警告：指定版本号 $1 小于当前版本 $cur，仅在你明确需要回退时使用" >&2
            fi
            apply_version "$1"
            check_uniform
            echo "已写入版本号：$1"
            ;;
    esac
}

main "$@"
