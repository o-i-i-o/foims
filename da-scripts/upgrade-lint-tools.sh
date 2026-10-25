#!/usr/bin/env bash
# lint 工具链（Biome / Oxlint）版本升级与一致性校验。
#
# 为什么固定版本而不用 @latest：CI 的 lint 步骤是阻断门，必须可复现——同一份
# 代码今天绿、明天也该绿；@latest 会把"上游发版"直接变成"构建中断"，且
# biome.json 的 $schema 只能写具体版本，无法跟随 latest。真正的痛点是版本散落
# 在多处、手工升级容易漏改，因此用本脚本一次改全并验证。
#
# 引用点（升级会全部改写，--check 会全部校验）：
#   1. .github/workflows/ci.yml   npx --yes <包>@<版本>（版本权威来源）
#   2. package.json               devDependencies 精确版本（本机二进制）
#   3. biome.json                 $schema 版本
#   4. docs/code-style.md         文档命令示例
#
# 用法：
#   bash da-scripts/upgrade-lint-tools.sh           # 升到 npm latest 并验证
#   bash da-scripts/upgrade-lint-tools.sh --check   # 只校验一致性与本机二进制
set -euo pipefail
cd "$(dirname "$0")/.."

CI_FILE=".github/workflows/ci.yml"
DOC_FILE="docs/code-style.md"
BIOME_JSON="biome.json"
PKG_JSON="package.json"
PROBE_FILE="frontend/src/styles/__upgrade_probe.css"
# 安装走官方源：npmmirror 对 Biome/Oxlint 的平台二进制有同步延迟，缺包时
# 会报 Cannot find module '@biomejs/cli-<平台>/biome'
INSTALL_REGISTRY="${INSTALL_REGISTRY:-https://registry.npmjs.org}"

info() { echo "→ $*"; }
warn() { echo "警告：$*" >&2; }
fail() {
    echo "错误：$*" >&2
    exit 1
}
cleanup() { rm -f "$PROBE_FILE"; }
trap cleanup EXIT

# 从 CI 文件提取固定版本：grep 能保证 anchor 唯一，避免误匹配文档里的 <version>
ci_biome() { grep -o '@biomejs/biome@[0-9][0-9.]*' "$CI_FILE" | head -n1 | sed 's/.*@//'; }
ci_oxlint() { grep -o 'oxlint@[0-9][0-9.]*' "$CI_FILE" | head -n1 | sed 's/.*@//'; }

# package.json 里的版本（本机二进制安装依据）
pkg_biome() { node -p "require('./$PKG_JSON').devDependencies['@biomejs/biome']"; }
pkg_oxlint() { node -p "require('./$PKG_JSON').devDependencies['oxlint']"; }

# biome.json 的 $schema 版本
schema_biome() { grep -o 'schemas/[0-9][0-9.]*/schema.json' "$BIOME_JSON" | head -n1 | cut -d/ -f2; }

# 文档命令示例里的版本
doc_biome() { grep -o '@biomejs/biome@[0-9][0-9.]*' "$DOC_FILE" | head -n1 | sed 's/.*@//'; }
doc_oxlint() { grep -o 'oxlint@[0-9][0-9.]*' "$DOC_FILE" | head -n1 | sed 's/.*@//'; }

# 本机已安装二进制版本（未安装返回空串）
local_version() {
    local bin="./node_modules/.bin/$1"
    [ -x "$bin" ] && "$bin" --version 2>/dev/null | sed 's/^Version: *//' || true
}

# 当前平台对应的 Biome 二进制包名（补装用）
biome_cli_pkg() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os-$arch" in
        Linux-x86_64) echo "linux-x64" ;;
        Linux-aarch64 | Linux-arm64) echo "linux-arm64" ;;
        Darwin-arm64) echo "darwin-arm64" ;;
        Darwin-x86_64) echo "darwin-x64" ;;
        *) echo "" ;;
    esac
}

# ---- 一致性校验 ----

check_consistency() {
    local a b c d e f ok=0
    a="$(ci_biome)"
    b="$(ci_oxlint)"
    c="$(pkg_biome)"
    d="$(pkg_oxlint)"
    e="$(schema_biome)"
    f="$(doc_oxlint)"
    info "引用点版本：ci.yml biome@${a} oxlint@${b}；package.json biome@${c} oxlint@${d}；biome.json schema@${e}；docs biome@$(doc_biome) oxlint@${f}"
    [ "$a" = "$c" ] && [ "$a" = "$e" ] && [ "$a" = "$(doc_biome)" ] || ok=1
    [ "$b" = "$d" ] && [ "$b" = "$f" ] || ok=1
    if [ "$ok" -ne 0 ]; then
        fail "四处引用点版本不一致，请跑 bash da-scripts/upgrade-lint-tools.sh 统一"
    fi
    info "四处引用点版本一致"
}

report_local() {
    local lb lo
    lb="$(local_version biome)"
    lo="$(local_version oxlint)"
    echo "  本机二进制：biome ${lb:-未安装} / oxlint ${lo:-未安装}"
    if [ -n "$lb" ] && [ "$lb" != "$(ci_biome)" ]; then
        warn "本机 biome ${lb} 与固定版本 $(ci_biome) 不一致，重跑本脚本可修复"
    fi
    if [ -n "$lo" ] && [ "$lo" != "$(ci_oxlint)" ]; then
        warn "本机 oxlint ${lo} 与固定版本 $(ci_oxlint) 不一致，重跑本脚本可修复"
    fi
}

report_latest() {
    local lb lo
    # --prefer-online：packument 有本地缓存期，不强制刷新会漏掉刚发布的版本，
    # “已是最新”的判断就会失真（缓存滞后是缓存问题，靠刷新解决，与是否
    # 固定版本无关）
    lb="$(npm view --prefer-online @biomejs/biome version 2>/dev/null || true)"
    lo="$(npm view --prefer-online oxlint version 2>/dev/null || true)"
    echo "  registry latest：biome ${lb:-查询失败} / oxlint ${lo:-查询失败}"
}

# ---- 升级 ----

# 四处引用点是否已全部等于给定版本（含“已升到 latest 但某处漏改”的情况）
pins_all_equal() {
    local b="$1" o="$2"
    [ "$(pkg_biome)" = "$b" ] && [ "$(schema_biome)" = "$b" ] && [ "$(doc_biome)" = "$b" ] \
        && [ "$(pkg_oxlint)" = "$o" ] && [ "$(doc_oxlint)" = "$o" ]
}

rewrite_pins() {
    local new_biome="$1" new_oxlint="$2"
    # 正则全量替换：不依赖“当前值是否一致”，因此对半改状态（某处漏改）
    # 也能一键收敛
    sed -i -E "s|schemas/[0-9][0-9.]*/schema.json|schemas/${new_biome}/schema.json|" "$BIOME_JSON"
    sed -i -E "s|@biomejs/biome@[0-9][0-9.]*|@biomejs/biome@${new_biome}|g; s|oxlint@[0-9][0-9.]*|oxlint@${new_oxlint}|g" "$CI_FILE"
    sed -i -E "s|@biomejs/biome@[0-9][0-9.]*|@biomejs/biome@${new_biome}|g; s|oxlint@[0-9][0-9.]*|oxlint@${new_oxlint}|g" "$DOC_FILE"
    npm pkg set "devDependencies.@biomejs/biome=${new_biome}" "devDependencies.oxlint=${new_oxlint}"
}

# 空跑防护：探针必须让本次安装的 biome 报错，否则说明命令没真正执行
verify_tools() {
    printf '.probe{color:red}\n' > "$PROBE_FILE"
    if ./node_modules/.bin/biome check frontend >/dev/null 2>&1; then
        fail "探针未触发报错，biome 可能没有真正执行（检查是否装到了同名旧包）"
    fi
    rm -f "$PROBE_FILE"
    info "探针验证通过（不合格式的 CSS 被检出）"

    ./node_modules/.bin/biome check frontend
    ./node_modules/.bin/oxlint
}

final_summary() {
    echo
    echo "完成："
    echo "  biome      $(ci_biome)  （ci.yml / package.json / biome.json schema / docs）"
    echo "  oxlint     $(ci_oxlint) （ci.yml / package.json / docs）"
    echo "  校验       四处一致 + 探针验证 + biome check + oxlint 全绿"
    echo "  提醒       若新版本调整了配置项，按新版提示跑"
    echo "             npx @biomejs/biome@$(ci_biome) migrate --write"
    echo "             并保持 biome.json 的 files.includes 多行风格"
}

main() {
    local mode="${1:-upgrade}"
    local cur_biome cur_oxlint new_biome new_oxlint cli_pkg
    case "$mode" in
        --check | check)
            check_consistency
            report_local
            report_latest
            ;;
        upgrade)
            # 不先强制一致：升级正是用来收拾“某处漏改”的，基准取 ci.yml
            report_local
            cur_biome="$(ci_biome)"
            cur_oxlint="$(ci_oxlint)"
            new_biome="$(npm view --prefer-online @biomejs/biome version)"
            new_oxlint="$(npm view --prefer-online oxlint version)"
            info "当前基准：biome ${cur_biome} / oxlint ${cur_oxlint}；registry latest：biome ${new_biome} / oxlint ${new_oxlint}"

            if [ "$cur_biome" = "$new_biome" ] && [ "$cur_oxlint" = "$new_oxlint" ] \
                && pins_all_equal "$new_biome" "$new_oxlint"; then
                info "固定版本已是最新且四处一致，跳过文件改写，仅校验本机工具"
            else
                rewrite_pins "$new_biome" "$new_oxlint"
            fi
            check_consistency

            info "安装本机二进制（registry=${INSTALL_REGISTRY}）"
            npm install --no-audit --no-fund --registry="$INSTALL_REGISTRY"

            # 镜像可能漏装 Biome 平台二进制：按需从官方源补装
            if ! ./node_modules/.bin/biome --version >/dev/null 2>&1; then
                cli_pkg="$(biome_cli_pkg)"
                [ -n "$cli_pkg" ] || fail "未识别的平台，无法补装 Biome 平台二进制"
                warn "biome 不可执行，补装 @biomejs/cli-${cli_pkg}@$(ci_biome)"
                npm install --no-save --no-package-lock --no-audit --no-fund \
                    --registry="$INSTALL_REGISTRY" "@biomejs/cli-${cli_pkg}@$(ci_biome)"
            fi

            report_local
            verify_tools
            final_summary
            ;;
        *)
            fail "未知参数 $mode（可用：--check）"
            ;;
    esac
}

main "$@"
