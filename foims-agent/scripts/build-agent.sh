#!/bin/bash
# FOIMS Agent 多平台构建矩阵脚本（编译 + OS 系统包组装）
#
# 用法：
#   bash scripts/build-agent.sh                  # 构建全部 tier1/2 目标并组装系统包
#   bash scripts/build-agent.sh --tier3          # 追加 tier3 目标（需 musl 交叉工具链）
#   bash scripts/build-agent.sh --list           # 仅列出目标矩阵
#   bash scripts/build-agent.sh --only x86_64-unknown-linux-musl[,其他]
#   AGENT_BACKEND=zigbuild bash scripts/build-agent.sh   # zig 交叉后端（CI 用）
#
# 产物（全部自持于本工作空间 dist/）：
#   dist/bin/<target>/foims-agent[.exe]          # 每目标一个静态二进制（组装原料）
#   dist/packages/foims-agent_<版本>_<deb架构>.deb   # Linux 程序包（systemd）
#   dist/packages/foims-agent-<版本>-1.<rpm架构>.rpm # Linux 程序包（systemd）
#   dist/packages/foims-agent-<版本>-windows-amd64.exe # Windows 程序包（免解压直用）
#   dist/packages/SHA256SUMS                     # 全部系统包校验和
#
# 分发模型：系统包由 CI 上传至发布页（Release）供用户按需下载；服务端
# web 页只现场组「配置包」（agent.toml + mTLS 证书），不再打包二进制。
# 程序包只含二进制与 systemd unit（Windows 为裸 exe），配置由
# 服务端下发的配置包装入 /etc/foims-agent/（两包解耦：程序包不依赖
# 配置包，先装谁都可以；配置包 postinst 会拉起已装的 agent）。
#
# 设计约束：
#   - 主体为纯 Rust 实现（rustix 走 linux 内联 syscall），musl 目标由
#     rust-lld 自含链接（Rust 1.71+ 默认）；但依赖链中含 C 代码（ring 的
#     密码学实现）。gcc 后端（默认）交叉编译时按目标注入 musl 交叉 C 编译器
#     （见 cc_for_target：x86_64 用 musl-tools，其余用 musl.cc 工具链）；
#     zig 后端（AGENT_BACKEND=zigbuild，CI 用）改由 zig cc 兼任交叉 C 编译器
#     与链接器，免装 musl 交叉工具链（musl.cc 下载多次间歇不可达后弃用），
#     仅影响 tier1/2
#   - tier1/2 目标：stable 直编，缺 std 组件时 rustup target add；
#     国内镜像源缺组件（404）时自动回退官方源离线安装（校验 sha256）
#   - windows 目标：x86_64-pc-windows-gnu（rustup 官方 std，stable 直编）；
#     msvc 目标需 MSVC 工具链（仅 Windows 宿主），不在本脚本矩阵；
#     gcc 后端需 mingw-w64 交叉编译器（ring 需要），zig 后端免注入
#   - tier3 目标：需 nightly + rust-src，用 -Z build-std 现场构建 std
#     （与 node_exporter .promu.yml 的 crossbuild 矩阵对齐，OpenBSD 另行处理）
#   - 版本策略：agent 版本独立自管理（版本号取本工作空间根 Cargo.toml 的
#     [workspace.package] version，即 agent 家族版本），体现在系统包文件名
#     与包内 Version 字段
#   - 单个目标失败不中断整体构建，结尾汇总并以非零码退出（便于 CI 感知）
#   - 组包工具缺失即失败退出（不带缺口发版）：Linux 需要 dpkg-deb 与
#     rpmbuild（Debian 系：apt install rpm；RHEL 系自带 rpm-build）

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="$PROJECT_DIR/dist/bin"
PKG_DIR="$PROJECT_DIR/dist/packages"
CRATE="foims-agent"
CRATE_WINDOWS="foims-agent-windows"
# systemd unit：包内物料的唯一来源
UNIT_SRC="$PROJECT_DIR/deploy/agent/foims-agent.service"
OFFICIAL_DIST="https://static.rust-lang.org/dist"

# tier1/2：rustup 直接提供 std，stable 直编
TIER12_TARGETS=(
    x86_64-unknown-linux-musl
    aarch64-unknown-linux-musl
    i686-unknown-linux-musl
    arm-unknown-linux-musleabihf
    armv7-unknown-linux-musleabihf
)

# windows：rustup 官方 std，stable 直编（gnu 目标；msvc 需 Windows 宿主）
WINDOWS_TARGETS=(
    x86_64-pc-windows-gnu
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
        echo "=== windows（stable 直编，gnu 目标） ==="
        printf '%s\n' "${WINDOWS_TARGETS[@]}"
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

# 交叉后端选择：AGENT_BACKEND=zigbuild 时 tier1/2 改用 cargo zigbuild
# （zig cc 兼任交叉 C 编译器与链接器，无需注入 CC_*/LINKER）；默认 gcc 后端。
# 用环境变量而非命令行开关，便于 CI 透传（与 AGENT_TARGETS 惯例一致）
ZIGBUILD=0
if [ "${AGENT_BACKEND:-}" = "zigbuild" ]; then
    ZIGBUILD=1
fi

# 包内物料唯一来源检查：缺失即失败退出，不带缺口发版
if [ ! -f "$UNIT_SRC" ]; then
    echo "错误：缺少包内物料 $UNIT_SRC" >&2
    exit 1
fi

# 目标的 deb 包架构名（Debian 官方架构表）；未适配返回空串
deb_arch_of() {
    case "$1" in
        x86_64-unknown-linux-musl)      echo amd64 ;;
        aarch64-unknown-linux-musl)     echo arm64 ;;
        i686-unknown-linux-musl)        echo i386 ;;
        arm-unknown-linux-musleabihf | armv7-unknown-linux-musleabihf) echo armhf ;;
        powerpc64le-unknown-linux-musl) echo ppc64el ;;
        s390x-unknown-linux-musl)       echo s390x ;;
        riscv64gc-unknown-linux-musl)   echo riscv64 ;;
        loongarch64-unknown-linux-musl) echo loongarch64 ;;
        mips-unknown-linux-musl)        echo mips ;;
        mipsel-unknown-linux-musl)      echo mipsel ;;
        mips64-unknown-linux-muslabi64) echo mips64 ;;
        mips64el-unknown-linux-muslabi64) echo mips64el ;;
        powerpc-unknown-linux-musl)     echo powerpc ;;
        *) echo "" ;;
    esac
}

# 目标的 rpm 包架构名（rpm 官方架构表）；未适配返回空串
rpm_arch_of() {
    case "$1" in
        x86_64-unknown-linux-musl)      echo x86_64 ;;
        aarch64-unknown-linux-musl)     echo aarch64 ;;
        i686-unknown-linux-musl)        echo i686 ;;
        arm-unknown-linux-musleabihf)   echo armhfp ;;
        armv7-unknown-linux-musleabihf) echo armv7hl ;;
        powerpc64le-unknown-linux-musl) echo ppc64le ;;
        s390x-unknown-linux-musl)       echo s390x ;;
        riscv64gc-unknown-linux-musl)   echo riscv64 ;;
        loongarch64-unknown-linux-musl) echo loongarch64 ;;
        mips-unknown-linux-musl)        echo mips ;;
        mipsel-unknown-linux-musl)      echo mipsel ;;
        mips64-unknown-linux-muslabi64) echo mips64 ;;
        mips64el-unknown-linux-muslabi64) echo mips64el ;;
        powerpc-unknown-linux-musl)     echo ppc ;;
        *) echo "" ;;
    esac
}

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

# windows 目标判定（triple 含 windows 即可，无需逐个比对）
is_windows() {
    case "$1" in
        *windows*) return 0 ;;
        *) return 1 ;;
    esac
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
# windows-gnu 用 mingw-w64 交叉编译器（ring 需要）。
cc_for_target() {
    case "$1" in
        x86_64-unknown-linux-musl)    command -v musl-gcc || true ;;
        aarch64-unknown-linux-musl)   echo "/opt/musl-cross/aarch64-linux-musl-cross/bin/aarch64-linux-musl-gcc" ;;
        i686-unknown-linux-musl)      echo "/opt/musl-cross/i686-linux-musl-cross/bin/i686-linux-musl-gcc" ;;
        arm-unknown-linux-musleabihf) echo "/opt/musl-cross/arm-linux-musleabihf-cross/bin/arm-linux-musleabihf-gcc" ;;
        armv7-unknown-linux-musleabihf)
            echo "/opt/musl-cross/armv7l-linux-musleabihf-cross/bin/armv7l-linux-musleabihf-gcc" ;;
        x86_64-pc-windows-gnu)        command -v x86_64-w64-mingw32-gcc || true ;;
        *) : ;;
    esac
}

# 目标的归集产物名：windows 目标经 [[bin]] name = "foims-agent" 统一为
# foims-agent（cargo 对 windows 目标自动追加 .exe），与系统包组包逻辑一致
product_name_of() {
    case "$1" in
        *windows*) echo "$CRATE.exe" ;;
        *) echo "$CRATE" ;;
    esac
}

# 构建 tier1/2：stable 直编。
# gcc 后端（默认）：链接器统一用 rustc 自带的 rust-lld（除 x86_64 musl 已
# 默认外，其余目标默认调用 cc，交叉场景下宿主 gcc 无法链接异构 musl 目标）；
# C 依赖（ring）按目标注入交叉编译器，未就位时警告并继续（仅纯 Rust 可过）。
# zig 后端（AGENT_BACKEND=zigbuild）：zig cc 兼任交叉 C 编译器与链接器，
# 无需任何注入；仅影响 tier1/2（tier3 仍走 build-std + rust-lld）。
build_tier12() {
    local target="$1"
    if ! target_std_installed "$target"; then
        rustup target add "$target" || install_target_offline "$target"
    fi
    if [ "$ZIGBUILD" -eq 1 ]; then
        (cd "$PROJECT_DIR" && \
            cargo zigbuild --release -p "$CRATE" --target "$target")
        return
    fi
    local linker_var cc_var cc
    linker_var="CARGO_TARGET_$(echo "$target" | tr 'a-z-' 'A-Z_')_LINKER"
    # cc-rs 认小写形式（如 CC_aarch64_unknown_linux_musl），大写不生效
    cc_var="CC_$(echo "$target" | tr '.' '_')"
    cc="$(cc_for_target "$target")"
    if [ -n "$cc" ] && [ -x "$cc" ]; then
        (cd "$PROJECT_DIR" && env "$linker_var=rust-lld" "$cc_var=$cc" \
            cargo build --release -p "$CRATE" --target "$target")
    else
        echo "警告：[$target] 未找到交叉 C 编译器（ring 等依赖需要），仅以 rust-lld 继续" >&2
        (cd "$PROJECT_DIR" && env "$linker_var=rust-lld" \
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
    (cd "$PROJECT_DIR" && env "$linker_var=rust-lld" \
        cargo +nightly build --release -p "$CRATE" \
        -Z build-std=std,panic_abort --target "$target")
}

# 构建 windows（gnu 目标，stable 直编）：
# gcc 后端：rust-lld 链接 + mingw-w64 交叉 C 编译器（ring 需要）；
# zig 后端：zig cc 兼任 C 编译器与链接器，免注入
build_windows() {
    local target="$1"
    if ! target_std_installed "$target"; then
        rustup target add "$target" || install_target_offline "$target"
    fi
    if [ "$ZIGBUILD" -eq 1 ]; then
        (cd "$PROJECT_DIR" && \
            cargo zigbuild --release -p "$CRATE_WINDOWS" --target "$target")
        return
    fi
    local cc
    cc="$(cc_for_target "$target")"
    if [ -n "$cc" ] && [ -x "$cc" ]; then
        (cd "$PROJECT_DIR" && env "CC_x86_64_pc_windows_gnu=$cc" \
            cargo build --release -p "$CRATE_WINDOWS" --target "$target")
    else
        echo "警告：[$target] 未找到 mingw-w64 交叉 C 编译器（ring 需要），仅以 rust-lld 继续" >&2
        (cd "$PROJECT_DIR" && \
            cargo build --release -p "$CRATE_WINDOWS" --target "$target")
    fi
}

echo "=== FOIMS Agent 多平台构建 (v$VERSION) ==="
FAILED=()
BUILT=()
for target in "${TIER12_TARGETS[@]}" "${WINDOWS_TARGETS[@]}" "${TIER3_TARGETS[@]}"; do
    is_selected "$target" || continue
    if ! is_tier12 "$target" && ! is_windows "$target" && [ "$BUILD_TIER3" -ne 1 ]; then
        continue
    fi
    echo "-- [$target] 构建开始"
    if is_windows "$target"; then
        builder=build_windows
    elif is_tier12 "$target"; then
        builder=build_tier12
    else
        builder=build_tier3
    fi
    if "$builder" "$target"; then
        BUILT+=("$target")
    else
        echo "错误：[$target] 构建失败" >&2
        FAILED+=("$target")
    fi
done

# 归集产物到 dist/bin/<target>/（系统包组装的原料）
for target in "${BUILT[@]:-}"; do
    [ -n "$target" ] || continue
    bin_name="$(product_name_of "$target")"
    src="$PROJECT_DIR/target/$target/release/$bin_name"
    dst_dir="$BIN_DIR/$target"
    if [ ! -f "$src" ]; then
        echo "警告：[$target] 产物缺失 $src" >&2
        continue
    fi
    mkdir -p "$dst_dir"
    cp "$src" "$dst_dir/$bin_name"
    echo "-- [$target] 归集完成"
done

# 组包工具前置检查：只要求本次构建涉及的工具链，缺失即失败退出
NEED_LINUX=0
NEED_WINDOWS=0
for target in "${BUILT[@]:-}"; do
    [ -n "$target" ] || continue
    if is_windows "$target"; then
        NEED_WINDOWS=1
    else
        NEED_LINUX=1
    fi
done
if [ "$NEED_LINUX" -eq 1 ]; then
    for cmd in dpkg-deb rpmbuild; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            echo "错误：缺少 $cmd（Linux 系统包组装需要；Debian 系执行 sudo apt install rpm，RHEL 系安装 rpm-build）" >&2
            exit 1
        fi
    done
fi

# deb postinst：加载 unit；配置包已就位时启用并重启服务，未就位时提示
# 先装配置包（两包解耦：先装程序包还是先装配置包均可闭环）
DEB_POSTINST='#!/bin/sh
# 安装/升级后：加载 unit；服务端下发的配置包已就位时启用并重启服务
set -e
if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload || true
    if [ -f /etc/foims-agent/agent.toml ]; then
        systemctl enable foims-agent.service >/dev/null 2>&1 || true
        systemctl try-restart foims-agent.service || true
    else
        echo "提示：尚未安装 FOIMS 配置包（/etc/foims-agent/agent.toml 缺失），请从服务端下载配置包安装后再启动服务。"
    fi
fi
'

# deb prerm：卸载（remove）时停止并禁用服务；升级（upgrade）不动，
# 由 postinst 负责拉起
DEB_PRERM='#!/bin/sh
# 卸载时停止并禁用服务（升级路径不经过本分支，由 postinst 拉起新进程）
set -e
if [ "$1" = "remove" ] && command -v systemctl >/dev/null 2>&1; then
    systemctl stop foims-agent.service 2>/dev/null || true
    systemctl disable foims-agent.service 2>/dev/null || true
fi
'

# 组装 deb 程序包（dpkg-deb，无需 root）：二进制 + systemd unit + 空配置目录
package_linux_deb() {
    local target="$1"
    local arch src pkgtmp
    arch="$(deb_arch_of "$target")"
    if [ -z "$arch" ]; then
        echo "错误：[$target] 未适配 deb 架构映射" >&2
        return 1
    fi
    src="$BIN_DIR/$target/$CRATE"
    pkgtmp="$(mktemp -d)"
    mkdir -p "$pkgtmp/DEBIAN" \
        "$pkgtmp/usr/bin" \
        "$pkgtmp/usr/lib/systemd/system" \
        "$pkgtmp/etc/foims-agent"
    install -m 755 "$src" "$pkgtmp/usr/bin/$CRATE"
    install -m 644 "$UNIT_SRC" "$pkgtmp/usr/lib/systemd/system/foims-agent.service"
    {
        echo "Package: $CRATE"
        echo "Version: $VERSION"
        echo "Section: net"
        echo "Priority: optional"
        echo "Architecture: $arch"
        echo "Maintainer: oi-io <boss@oi-io.cc>"
        echo "Description: FOIMS Agent host metrics collector"
        echo " Installed on managed hosts; reports metrics to the FOIMS"
        echo " server over HTTP/3 (QUIC mTLS). Configure via the"
        echo " server-issued configuration package (foims-agent-config)."
        echo "Installed-Size: $(du -sk "$pkgtmp/usr" | cut -f1)"
    } > "$pkgtmp/DEBIAN/control"
    printf '%s\n' "$DEB_POSTINST" > "$pkgtmp/DEBIAN/postinst"
    printf '%s\n' "$DEB_PRERM" > "$pkgtmp/DEBIAN/prerm"
    chmod 755 "$pkgtmp/DEBIAN/postinst" "$pkgtmp/DEBIAN/prerm"
    # 目录权限归一 755（mktemp 根目录默认 700、mkdir 随 umask 落 775）
    find "$pkgtmp" -type d -exec chmod 755 {} +
    dpkg-deb --root-owner-group --build "$pkgtmp" \
        "$PKG_DIR/${CRATE}_${VERSION}_${arch}.deb" >/dev/null
    rm -rf "$pkgtmp"
}

# 组装 rpm 程序包（rpmbuild，用户态 _topdir，无需 root）：内容与 deb 等价。
# spec 不写 BuildArch：包架构由 --target 决定；rpm 6 起「可构建架构」收窄
# 到宿主同族（compatible build archs），spec 显式声明异族 BuildArch 会报
# No compatible architectures found for build（实测 Debian rpm 6.0.1）
package_linux_rpm() {
    local target="$1"
    local arch src top
    arch="$(rpm_arch_of "$target")"
    if [ -z "$arch" ]; then
        echo "错误：[$target] 未适配 rpm 架构映射" >&2
        return 1
    fi
    src="$BIN_DIR/$target/$CRATE"
    top="$(mktemp -d)"
    mkdir -p "$top/BUILD" "$top/RPMS" "$top/SOURCES" "$top/SPECS" "$top/SRPMS"
    install -m 755 "$src" "$top/SOURCES/$CRATE"
    install -m 644 "$UNIT_SRC" "$top/SOURCES/foims-agent.service"
    cat > "$top/SPECS/$CRATE.spec" << SPEC_EOF
Name:           $CRATE
Version:        $VERSION
Release:        1
Summary:        FOIMS Agent host metrics collector
License:        GPL-3.0-or-later

Source0:        $CRATE
Source1:        foims-agent.service

%description
Installed on managed hosts; reports metrics to the FOIMS server over
HTTP/3 (QUIC mTLS). Configure via the server-issued configuration
package (foims-agent-config).

%install
install -Dm755 "%{SOURCE0}" "%{buildroot}/usr/bin/$CRATE"
install -Dm644 "%{SOURCE1}" "%{buildroot}/usr/lib/systemd/system/foims-agent.service"
mkdir -p "%{buildroot}/etc/foims-agent"

%post
if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload || true
    if [ -f /etc/foims-agent/agent.toml ]; then
        systemctl enable foims-agent.service >/dev/null 2>&1 || true
        systemctl try-restart foims-agent.service || true
    else
        echo "提示：尚未安装 FOIMS 配置包（/etc/foims-agent/agent.toml 缺失），请从服务端下载配置包安装后再启动服务。"
    fi
fi

%preun
if [ "\$1" -eq 0 ] && command -v systemctl >/dev/null 2>&1; then
    systemctl stop foims-agent.service 2>/dev/null || true
    systemctl disable foims-agent.service 2>/dev/null || true
fi

%files
%attr(755,root,root) /usr/bin/$CRATE
/usr/lib/systemd/system/foims-agent.service
%dir %attr(755,root,root) /etc/foims-agent
SPEC_EOF
    # _dbpath 指向临时目录：组包是纯文件操作，不需要系统 rpm 数据库；
    # Debian 宿主的 /var/lib/rpm 未初始化，缺该定义 rpmbuild 会报 sqlite 错。
    # 失败显式 return 1：函数经 if 条件上下文调用时 set -e 失效，末尾的
    # rm -rf 成功会把失败掩盖成返回 0，导致产物静默丢失（实锤过的坑）
    if ! rpmbuild -bb --quiet --define "_topdir $top" --define "_dbpath $top/rpmdb" \
        --target "$arch" "$top/SPECS/$CRATE.spec" >/dev/null; then
        rm -rf "$top"
        return 1
    fi
    if ! mv "$top/RPMS/$arch/$CRATE-$VERSION-1.$arch.rpm" "$PKG_DIR/"; then
        rm -rf "$top"
        return 1
    fi
    rm -rf "$top"
}

# Windows 程序包：裸 exe 直接落发布目录，免解压下载即用（部署步骤见
# 仓库 foims-agent/deploy/agent/README-windows.txt）
package_windows_exe() {
    install -m 755 "$BIN_DIR/$1/$CRATE.exe" \
        "$PKG_DIR/$CRATE-$VERSION-windows-amd64.exe"
}

mkdir -p "$PKG_DIR"
PACK_FAILED=()
for target in "${BUILT[@]:-}"; do
    [ -n "$target" ] || continue
    bin_name="$(product_name_of "$target")"
    if [ ! -f "$BIN_DIR/$target/$bin_name" ]; then
        echo "警告：[$target] 无归集产物，跳过系统包组装" >&2
        continue
    fi
    echo "-- [$target] 组装系统包"
    if is_windows "$target"; then
        if package_windows_exe "$target"; then
            echo "-- [$target] exe 完成"
        else
            echo "错误：[$target] exe 组装失败" >&2
            PACK_FAILED+=("$target")
        fi
    else
        if package_linux_deb "$target" && package_linux_rpm "$target"; then
            echo "-- [$target] deb/rpm 完成"
        else
            echo "错误：[$target] deb/rpm 组装失败" >&2
            PACK_FAILED+=("$target")
        fi
    fi
done

# 系统包校验和（覆盖全部产物，随发布页一同上传供下载方校验）
: > "$PKG_DIR/SHA256SUMS"
for pkg in "$PKG_DIR"/*.deb "$PKG_DIR"/*.rpm "$PKG_DIR"/*.exe; do
    [ -e "$pkg" ] || continue
    (cd "$PKG_DIR" && sha256sum "$(basename "$pkg")") >> "$PKG_DIR/SHA256SUMS"
done

if [ ! -s "$PKG_DIR/SHA256SUMS" ]; then
    echo "错误：无任何系统包产物" >&2
    exit 1
fi

echo "=== 构建汇总：编译成功 ${#BUILT[@]} / 编译失败 ${#FAILED[@]} / 组包失败 ${#PACK_FAILED[@]} ==="
if [ "${#FAILED[@]}" -gt 0 ]; then
    echo "编译失败目标: ${FAILED[*]}" >&2
fi
if [ "${#PACK_FAILED[@]}" -gt 0 ]; then
    echo "组包失败目标: ${PACK_FAILED[*]}" >&2
fi
if [ "${#FAILED[@]}" -gt 0 ] || [ "${#PACK_FAILED[@]}" -gt 0 ]; then
    exit 1
fi
echo "产物目录: $PKG_DIR"
ls -lh "$PKG_DIR"
