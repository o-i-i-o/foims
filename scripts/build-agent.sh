#!/bin/bash
# FOIMS Agent 多平台构建矩阵脚本
#
# 用法：
#   bash scripts/build-agent.sh                  # 构建全部 tier1/2 目标
#   bash scripts/build-agent.sh --tier3          # 追加 tier3 目标（需 musl 交叉工具链）
#   bash scripts/build-agent.sh --list           # 仅列出目标矩阵
#   bash scripts/build-agent.sh --only x86_64-unknown-linux-musl[,其他]
#
# 产物：
#   dist/agents/<target>/foims-agent             # 每目标一个静态二进制
#   dist/agents/manifest.json                    # 运行期分发的版本事实来源
#   dist/agents/SHA256SUMS                       # 全部二进制的校验和
#
# 设计约束：
#   - 主体为纯 Rust 实现（rustix 走 linux 内联 syscall），musl 目标由
#     rust-lld 自含链接（Rust 1.71+ 默认）；但依赖链中含 C 代码（ring 的
#     密码学实现），交叉编译时需按目标注入 musl 交叉 C 编译器（见
#     cc_for_target：x86_64 用 musl-tools，其余用 musl.cc 工具链）
#   - tier1/2 目标：stable 直编，缺 std 组件时 rustup target add；
#     国内镜像源缺组件（404）时自动回退官方源离线安装（校验 sha256）
#   - tier3 目标：需 nightly + rust-src，用 -Z build-std 现场构建 std
#     （与 node_exporter .promu.yml 的 crossbuild 矩阵对齐，OpenBSD 另行处理）
#   - 版本注入：以主程序版本（根 Cargo.toml）作为 FOIMS_AGENT_VERSION 编译期
#     写入 agent（agent 版本 = 主程序版本，天然满足「不低于 foims」门控），
#     并生成 manifest.json 供服务端下载 API 做版本门控与产物校验
#   - 单个目标失败不中断整体构建，结尾汇总并以非零码退出（便于 CI 感知）

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$PROJECT_DIR/dist/agents"
CRATE="foims-agent"
OFFICIAL_DIST="https://static.rust-lang.org/dist"

# tier1/2：rustup 直接提供 std，stable 直编
TIER12_TARGETS=(
    x86_64-unknown-linux-musl
    aarch64-unknown-linux-musl
    i686-unknown-linux-musl
    arm-unknown-linux-musleabihf
    armv7-unknown-linux-musleabihf
)

# tier3：无官方 std 预编译，nightly -Z build-std 现场构建
TIER3_TARGETS=(
    powerpc64le-unknown-linux-musl
    s390x-unknown-linux-musl
    riscv64gc-unknown-linux-musl
    loongarch64-unknown-linux-musl
    mips-unknown-linux-musl
    mipsel-unknown-linux-musl
    mips64-unknown-linux-muslabi64
    mips64el-unknown-linux-muslabi64
    powerpc-unknown-linux-musl
)

# 解析 --list / --only / --tier3
# tier3 说明：-Z build-std 只从源码构建 std，不产出 musl 的 crt 启动对象
# （crt1/crti/crtn 仅随 tier1/2 官方 rust-std 包分发），因此 tier3 目标
# 需要额外安装对应架构的 musl 交叉工具链（如 musl.cc）才能完成链接，
# 默认跳过，留待实施阶段以容器化交叉环境解决。
ONLY=""
BUILD_TIER3=0
case "${1:-}" in
    --list)
        echo "=== tier1/2（stable 直编） ==="
        printf '%s\n' "${TIER12_TARGETS[@]}"
        echo "=== tier3（nightly -Z build-std，需 musl 交叉工具链，--tier3 启用） ==="
        printf '%s\n' "${TIER3_TARGETS[@]}"
        exit 0
        ;;
    --only)
        [ -n "${2:-}" ] || { echo "错误：--only 缺少参数" >&2; exit 1; }
        ONLY="$2"
        ;;
    --tier3)
        BUILD_TIER3=1
        ;;
    "")
        ;;
    *)
        echo "错误：未知参数 $1（支持 --list / --only <targets> / --tier3）" >&2
        exit 1
        ;;
esac

VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$PROJECT_DIR/Cargo.toml" | head -n1)
[ -n "$VERSION" ] || { echo "错误：无法从 Cargo.toml 解析版本号" >&2; exit 1; }

mkdir -p "$DIST_DIR"

# 按 --only 过滤目标（保持矩阵顺序）
is_selected() {
    [ -z "$ONLY" ] && return 0
    echo ",$ONLY," | grep -q ",$1,"
}

is_tier12() {
    local t
    for t in "${TIER12_TARGETS[@]}"; do
        [ "$t" = "$1" ] && return 0
    done
    return 1
}

# 当前 toolchain 的 sysroot（即 toolchain 根目录）
sysroot_of() {
    rustc --print sysroot
}

# 判断目标 std 组件是否已就位（以 rustlib 目录为准，比 rustup 清单更可靠）
target_std_installed() {
    [ -d "$(sysroot_of)/lib/rustlib/$1/lib" ]
}

# 镜像源缺组件时的离线兜底：从官方源下载 rust-std 并校验 sha256 后落位。
# 前提：dist 日期与组件哈希取自本地 toolchain 缓存的 channel manifest，
# 与已安装的 rustc 版本严格匹配。
install_target_offline() {
    local target="$1"
    local sysroot dist_date rustver manifest_url comp_hash tmpdir
    sysroot="$(sysroot_of)"
    dist_date="$(awk -F'"' '/^date/{print $2; exit}' "$sysroot/lib/rustlib/multirust-channel-manifest.toml")"
    rustver="$(rustc --version | awk '{print $2}')"
    manifest_url="$OFFICIAL_DIST/$dist_date/channel-rust-stable.toml"
    comp_hash="$(curl -fsSL "$manifest_url" | awk -v t="$target" '
        index($0, "[pkg.rust-std.target." t "]") { f=1; next }
        f && /^\[/ { exit }
        f && /^xz_hash/ { gsub(/"/, ""); split($0, a, "="); gsub(/ /, "", a[2]); print a[2] }
    ')"
    if [ -z "$comp_hash" ]; then
        echo "错误：官方 manifest 中未找到 $target 的 rust-std 哈希" >&2
        return 1
    fi
    tmpdir="$(mktemp -d)"
    echo "-- 镜像缺组件，改从官方源离线安装 rust-std-$rustver-$target"
    curl -fsSL -o "$tmpdir/std.tar.xz" "$OFFICIAL_DIST/$dist_date/rust-std-$rustver-$target.tar.xz"
    echo "$comp_hash  $tmpdir/std.tar.xz" | sha256sum -c - >/dev/null
    tar -xf "$tmpdir/std.tar.xz" -C "$tmpdir"
    (cd "$tmpdir"/rust-std-*/ && ./install.sh --prefix="$sysroot" >/dev/null)
    rm -rf "$tmpdir"
}

# 各 musl 目标对应的交叉 C 编译器：依赖链中含 C 代码（如 ring）时，
# cc-rs 按目标前缀查找编译器且不回退宿主 cc，必须显式注入 CC_<target>。
# x86_64 用发行版 musl-tools 的 musl-gcc；其余用 musl.cc 交叉工具链。
cc_for_target() {
    case "$1" in
        x86_64-unknown-linux-musl)    command -v musl-gcc || true ;;
        aarch64-unknown-linux-musl)   echo "/opt/musl-cross/aarch64-linux-musl-cross/bin/aarch64-linux-musl-gcc" ;;
        i686-unknown-linux-musl)      echo "/opt/musl-cross/i686-linux-musl-cross/bin/i686-linux-musl-gcc" ;;
        arm-unknown-linux-musleabihf) echo "/opt/musl-cross/arm-linux-musleabihf-cross/bin/arm-linux-musleabihf-gcc" ;;
        armv7-unknown-linux-musleabihf)
            echo "/opt/musl-cross/armv7l-linux-musleabihf-cross/bin/armv7l-linux-musleabihf-gcc" ;;
        *) : ;;
    esac
}

# 构建 tier1/2：stable 直编。
# 链接器统一用 rustc 自带的 rust-lld（除 x86_64 musl 已默认外，其余目标
# 默认调用 cc，交叉场景下宿主 gcc 无法链接异构 musl 目标）；
# C 依赖（ring）按目标注入交叉编译器，未就位时警告并继续（仅纯 Rust 可过）。
build_tier12() {
    local target="$1"
    local linker_var cc_var cc
    linker_var="CARGO_TARGET_$(echo "$target" | tr 'a-z-' 'A-Z_')_LINKER"
    # cc-rs 认小写形式（如 CC_aarch64_unknown_linux_musl），大写不生效
    cc_var="CC_$(echo "$target" | tr '.' '_')"
    if ! target_std_installed "$target"; then
        rustup target add "$target" || install_target_offline "$target"
    fi
    cc="$(cc_for_target "$target")"
    if [ -n "$cc" ] && [ -x "$cc" ]; then
        (cd "$PROJECT_DIR" && env "$linker_var=rust-lld" "$cc_var=$cc" \
            "FOIMS_AGENT_VERSION=$VERSION" \
            cargo build --release -p "$CRATE" --target "$target")
    else
        echo "警告：[$target] 未找到交叉 C 编译器（ring 等依赖需要），仅以 rust-lld 继续" >&2
        (cd "$PROJECT_DIR" && env "$linker_var=rust-lld" "FOIMS_AGENT_VERSION=$VERSION" \
            cargo build --release -p "$CRATE" --target "$target")
    fi
}

# 构建 tier3：nightly -Z build-std（链接器同 tier1/2，统一 rust-lld）
build_tier3() {
    local target="$1"
    local linker_var
    linker_var="CARGO_TARGET_$(echo "$target" | tr 'a-z-' 'A-Z_')_LINKER"
    rustup toolchain install nightly --profile minimal >/dev/null 2>&1 \
        || rustup toolchain install nightly --profile minimal
    rustup component add rust-src --toolchain nightly
    (cd "$PROJECT_DIR" && env "$linker_var=rust-lld" "FOIMS_AGENT_VERSION=$VERSION" \
        cargo +nightly build --release -p "$CRATE" \
        -Z build-std=std,panic_abort --target "$target")
}

echo "=== FOIMS Agent 多平台构建 (v$VERSION) ==="
FAILED=()
BUILT=()
for target in "${TIER12_TARGETS[@]}" "${TIER3_TARGETS[@]}"; do
    is_selected "$target" || continue
    if ! is_tier12 "$target" && [ "$BUILD_TIER3" -ne 1 ]; then
        continue
    fi
    echo "-- [$target] 构建开始"
    if is_tier12 "$target"; then
        if build_tier12 "$target"; then
            BUILT+=("$target")
        else
            echo "错误：[$target] 构建失败" >&2
            FAILED+=("$target")
        fi
    else
        if build_tier3 "$target"; then
            BUILT+=("$target")
        else
            echo "错误：[$target] 构建失败" >&2
            FAILED+=("$target")
        fi
    fi
done

# 归集产物 + 校验和 + manifest.json（运行期分发的版本事实来源）
: > "$DIST_DIR/SHA256SUMS"
MANIFEST_TMP="$(mktemp)"
# 收集阶段先落 JSON 片段，全部成功后一次性写 manifest.json（防半写清单）
: > "$MANIFEST_TMP"
for target in "${BUILT[@]:-}"; do
    [ -n "$target" ] || continue
    src="$PROJECT_DIR/target/$target/release/$CRATE"
    dst_dir="$DIST_DIR/$target"
    if [ ! -f "$src" ]; then
        echo "警告：[$target] 产物缺失 $src" >&2
        continue
    fi
    mkdir -p "$dst_dir"
    cp "$src" "$dst_dir/$CRATE"
    (cd "$DIST_DIR" && sha256sum "$target/$CRATE") >> "$DIST_DIR/SHA256SUMS"
    sha="$(sha256sum "$dst_dir/$CRATE" | awk '{print $1}')"
    size="$(stat -c %s "$dst_dir/$CRATE")"
    printf ',\n    "%s": { "sha256": "%s", "size": %s }' "$target" "$sha" "$size" >> "$MANIFEST_TMP"
    echo "-- [$target] 归集完成"
done

if [ ! -s "$MANIFEST_TMP" ]; then
    echo "错误：无任何构建产物，未生成 manifest.json" >&2
    rm -f "$MANIFEST_TMP"
    exit 1
fi

# 组装清单骨架：片段首字符为分隔逗号，首行替换为缩进并补齐骨架字段
{
    printf '{\n  "version": "%s",\n  "generated_at": "%s",\n  "targets": {' \
        "$VERSION" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    sed '1s/^,/\n    /' "$MANIFEST_TMP"
    printf '\n  }\n}\n'
} > "$DIST_DIR/manifest.json"
rm -f "$MANIFEST_TMP"
# 语法自检：清单必须可被 python 解析（存在 python3 的环境即校验，CI 必有）
if command -v python3 >/dev/null 2>&1; then
    python3 -c "import json,sys; json.load(open('$DIST_DIR/manifest.json'))" \
        || { echo "错误：manifest.json 语法非法" >&2; exit 1; }
fi
echo "-- manifest.json 已生成（version=$VERSION, targets=$(grep -c '"sha256"' "$DIST_DIR/manifest.json")）"

echo "=== 构建汇总：成功 ${#BUILT[@]} / 失败 ${#FAILED[@]} ==="
if [ "${#FAILED[@]}" -gt 0 ]; then
    echo "失败目标: ${FAILED[*]}" >&2
    exit 1
fi
echo "产物目录: $DIST_DIR"
