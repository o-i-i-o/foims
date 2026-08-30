#!/bin/sh

# FOIMS 发布前检查脚本

ERROR_COUNT=0
TOTAL_CHECKS=0

echo "========================================"
echo "   FOIMS 发布前检查"
echo "========================================"
echo ""

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 检查函数
run_check() {
    CHECK_NAME="$1"
    CHECK_CMD="$2"
    TOTAL_CHECKS=$((TOTAL_CHECKS + 1))

    echo "[$TOTAL_CHECKS] 正在执行: ${CHECK_NAME}..."
    eval "$CHECK_CMD" > /dev/null 2>&1
    if [ $? -ne 0 ]; then
        echo "  ${RED}❌ 失败${NC}: ${CHECK_NAME}"
        ERROR_COUNT=$((ERROR_COUNT + 1))
        return 1
    else
        echo "  ${GREEN}✅ 通过${NC}: ${CHECK_NAME}"
        return 0
    fi
    echo ""
}

# 主程序
main() {
    # 获取脚本所在目录
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
    cd "$SCRIPT_DIR" || exit 1

    echo "────────────────────────────────────"
    echo "   项目: foims (IP/MAC 地址管理系统)"
    echo "────────────────────────────────────"

    if [ ! -f "Cargo.toml" ]; then
        echo "  ${RED}❌ 错误${NC}: 未找到 Cargo.toml，当前目录: $(pwd)"
        ERROR_COUNT=$((ERROR_COUNT + 1))
        TOTAL_CHECKS=$((TOTAL_CHECKS + 1))
        summary
        exit 1
    fi

    # 先格式化代码
    echo "[准备] 正在格式化代码..."
    cargo fmt > /dev/null 2>&1
    if [ $? -ne 0 ]; then
        echo "  ${RED}❌ 代码格式化失败${NC}"
        exit 1
    else
        echo "  ${GREEN}✅ 代码格式化完成${NC}"
    fi
    echo ""

    run_check "代码格式检查" "cargo fmt --check"
    run_check "Clippy 静态分析" "cargo clippy --release -- -D warnings"
    run_check "编译检查" "cargo check --release"
    run_check "Release 构建" "cargo build --release"

    summary
}

# 总结函数
summary() {
    echo ""
    echo "========================================"
    echo "   检查结果汇总"
    echo "========================================"
    echo "   总检查数: ${TOTAL_CHECKS}"
    echo "   通过: $((TOTAL_CHECKS - ERROR_COUNT))"
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

# 执行主程序
main
