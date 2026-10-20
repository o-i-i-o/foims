#!/bin/sh

# FOIMS 发布前检查脚本
#
# 流程对齐 .github/workflows/ci.yml：
#   frontend job —— npm ci / vite build（SFC 编译与 import 解析阻断）/
#                   Biome（CSS + 格式）/ Oxlint（JS 质量）
#   build job    —— cargo fmt --check / clippy / test / build + build-pkgs.sh 打包
#                   （deb/rpm/zip；agent 交叉编译在 CI 独立 agent-build job，
#                   本地由下方 Agent 构建检查覆盖）
#
# 与 CI 的差异：
#   - 本地执行前先 cargo fmt 自动修复（CI 仅 --check）
#   - 依赖 node_modules 已存在时跳过 npm ci（CI 每次全新安装）
#   - cargo test 已知环境性失败降级为警告不阻断：/etc/foims/encryption.key
#     为 root 0600，非 root 用户下 crypto::tests 固定权限拒绝（CI 无此文件，
#     与代码无关），判定标准见 is_known_env_failure

ERROR_COUNT=0
TOTAL_CHECKS=0
WARN_COUNT=0
LOG_DIR=""
CHECK_LOG=""

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# ---- 前端检查函数（对齐 CI frontend job，子 shell 隔离 cwd）----

# 前端依赖：node_modules 已存在则跳过，否则按 CI 方式全新安装
fe_lint_deps() {
    (
        cd frontend || exit 1
        if [ -d node_modules ]; then
            echo "node_modules 已存在，跳过 npm ci"
            exit 0
        fi
        npm ci --no-audit --no-fund
    )
}

# 前端构建：Vue SFC 编译 + 模板/导入解析在构建期集中暴露
fe_build() {
    (
        cd frontend || exit 1
        npm run build
    )
}

# Biome：CSS 审计 + 格式检查（配置见根 biome.json，存档目录已排除）
fe_biome() { biome check frontend; }

# Oxlint：JS 代码质量（配置见根 .oxlintrc.json，存档目录已排除）
fe_oxlint() { oxlint; }

# ---- 检查执行框架 ----

# 展示当前检查日志的末尾（失败诊断用）
show_log_tail() {
    echo "  ── 输出末尾（完整日志: ${CHECK_LOG}）──"
    tail -n 30 "$CHECK_LOG" 2>/dev/null | sed 's/^/  │ /'
}

# 执行检查（不计数）：输出写入日志文件，透传命令退出码
execute_check() {
    CHECK_LOG="$LOG_DIR/$TOTAL_CHECKS.log"
    eval "$1" > "$CHECK_LOG" 2>&1
    return $?
}

# 标准检查：失败计数并展示日志尾部
run_check() {
    CHECK_NAME="$1"
    TOTAL_CHECKS=$((TOTAL_CHECKS + 1))
    echo "[$TOTAL_CHECKS] 正在执行: ${CHECK_NAME}..."
    if execute_check "$2"; then
        echo "  ${GREEN}✅ 通过${NC}: ${CHECK_NAME}"
    else
        echo "  ${RED}❌ 失败${NC}: ${CHECK_NAME}"
        show_log_tail
        ERROR_COUNT=$((ERROR_COUNT + 1))
    fi
}

# 判定 cargo test 失败是否为已知环境性失败：
# 全部失败测试集中于 crypto::tests 且输出含权限拒绝（判定依据见文件头注释）
is_known_env_failure() {
    FAILED_TESTS=$(grep -E '^test [^ ]+ \.\.\. FAILED$' "$1" 2>/dev/null \
        | sed 's/^test //; s/ \.\.\. FAILED$//')
    [ -n "$FAILED_TESTS" ] || return 1
    printf '%s\n' "$FAILED_TESTS" | grep -qv '^crypto::tests' && return 1
    grep -q 'Permission denied' "$1"
}

# cargo test 专用检查：已知环境性失败降级为警告不阻断
run_cargo_test() {
    TOTAL_CHECKS=$((TOTAL_CHECKS + 1))
    echo "[$TOTAL_CHECKS] 正在执行: 运行测试..."
    if execute_check "cargo test --release"; then
        echo "  ${GREEN}✅ 通过${NC}: 运行测试"
    elif is_known_env_failure "$CHECK_LOG"; then
        WARN_COUNT=$((WARN_COUNT + 1))
        echo "  ${YELLOW}⚠️  已知环境性失败（不阻断）${NC}: 运行测试"
        echo "  原因: /etc/foims/encryption.key 为 root 0600，非 root 用户下 crypto::tests 读取失败"
        echo "  （CI 环境无该文件，与代码无关；失败项: $(grep -cE '^test [^ ]+ \.\.\. FAILED$' "$CHECK_LOG") 个，均属 crypto::tests）"
    else
        echo "  ${RED}❌ 失败${NC}: 运行测试"
        show_log_tail
        ERROR_COUNT=$((ERROR_COUNT + 1))
    fi
}

# 总结函数
summary() {
    echo ""
    echo "========================================"
    echo "   检查结果汇总"
    echo "========================================"
    echo "   总检查数: ${TOTAL_CHECKS}"
    echo "   通过: $((TOTAL_CHECKS - ERROR_COUNT))（另 ${WARN_COUNT} 项环境性警告）"
    echo "   失败: ${ERROR_COUNT}"
    echo ""

    if [ $ERROR_COUNT -eq 0 ]; then
        echo "  ${GREEN}🎉 所有检查通过！可以发布。${NC}"
        echo "========================================"
        exit 0
    else
        echo "  ${YELLOW}⚠️  发现 ${ERROR_COUNT} 个失败项，请修复后重试。${NC}"
        echo "========================================"
        exit 1
    fi
}

# 主程序
main() {
    # 切换到项目根（脚本位于 test-scripts/）
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
    cd "$SCRIPT_DIR/.." || exit 1

    echo "========================================"
    echo "   FOIMS 发布前检查（流程对齐 CI 工作流）"
    echo "========================================"
    echo "────────────────────────────────────"
    echo "   项目: foims (IP/MAC 地址管理系统)"
    echo "────────────────────────────────────"

    if [ ! -f "Cargo.toml" ]; then
        echo "  ${RED}❌ 错误${NC}: 未找到 Cargo.toml，当前目录: $(pwd)"
        exit 1
    fi
    if [ ! -d "frontend" ]; then
        echo "  ${RED}❌ 错误${NC}: 未找到 frontend/ 目录，当前目录: $(pwd)"
        exit 1
    fi

    # 检查日志目录（脚本退出时清理，失败详情已在屏幕展示）
    LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/foims-release-check.XXXXXX")"
    trap 'rm -rf "$LOG_DIR"' EXIT HUP INT TERM

    # Node.js 前置：缺失时前端检查整体无法执行，明确报错并跳过该段
    SKIP_FRONTEND=0
    if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
        echo "  ${RED}❌ 错误${NC}: 未检测到 node/npm，前端检查无法执行（CI 前端 job 使用 Node 24）"
        TOTAL_CHECKS=$((TOTAL_CHECKS + 1))
        ERROR_COUNT=$((ERROR_COUNT + 1))
        SKIP_FRONTEND=1
    fi

    # 准备：先自动格式化（CI 仅 --check，本地先修复再校验）
    echo "[准备] 正在格式化代码（cargo fmt）..."
    if cargo fmt > "$LOG_DIR/fmt.log" 2>&1; then
        echo "  ${GREEN}✅ 代码格式化完成${NC}"
    else
        echo "  ${RED}❌ 代码格式化失败${NC}"
        CHECK_LOG="$LOG_DIR/fmt.log"
        show_log_tail
        exit 1
    fi
    echo ""

    # 前端检查（对齐 CI frontend job）
    if [ "$SKIP_FRONTEND" -eq 0 ]; then
        echo "──────── 前端检查（CI frontend job）────────"
        run_check "前端依赖安装" fe_lint_deps
        run_check "前端构建（vite build）" fe_build
        run_check "Biome（CSS + 格式）" fe_biome
        run_check "Oxlint（JS 质量）" fe_oxlint
        echo ""
    fi

    # 后端检查（对齐 CI build job）
    echo "──────── 后端检查（CI build job）────────"
    run_check "代码格式检查" "cargo fmt --check"
    run_check "Clippy 静态分析" "cargo clippy --release -- -D warnings"
    run_cargo_test
    run_check "Release 构建" "cargo build --release"
    echo ""

    # 打包检查（对齐 CI build 与 agent-build 任务）
    echo "──────── 打包检查（CI build / agent-build 步骤）────────"
    run_check "主程序打包（deb/rpm/zip）" "bash scripts/build-pkgs.sh"
    run_check "Agent 多平台安装包（deb/rpm/zip）" "bash foims-agent/scripts/build-agent.sh"

    summary
}

# 执行主程序
main
