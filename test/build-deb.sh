#!/bin/bash
# FOIMS DEB 打包脚本（Debian 规范）
#
# 用法：bash test/build-deb.sh
# 产物：target/foims_<版本>_<架构>.deb
#
# 设计约束：
#   - 仅打包不安装：dpkg-deb --root-owner-group 归一化属主，全程无需 root，
#     可在 CI 与普通用户环境直接运行
#   - systemd 单元唯一来源为 deploy/services/foims.service，本脚本不再内嵌副本
#   - web 目录仅打包 static/（package.json、tests、eslint 等开发工具链文件不进包）
#   - 配置文件以 config.toml.example 副本为基础打包（去除 .example 后缀）：
#     仓库/开发机的本地 config.toml 可能含真实数据库连接信息，不进包；
#     数据库连接要素由安装后的初始化向导在页面填写并回写配置文件；
#     JWT 密钥由本脚本生成随机 32 字符替换示例占位符

set -euo pipefail

# 路径推导：脚本位于 test/，项目根为其上一级目录（不再硬编码绝对路径）
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DEBPAK_DIR="$PROJECT_DIR/target/debpak"
BINARY_NAME="foims"
SERVICE_SRC="$PROJECT_DIR/deploy/services/foims.service"

# 前置检查
for cmd in cargo dpkg-deb; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "错误：缺少必要命令 $cmd" >&2
        exit 1
    fi
done
if [ ! -f "$SERVICE_SRC" ]; then
    echo "错误：缺少服务文件 $SERVICE_SRC" >&2
    exit 1
fi

# 从根 Cargo.toml 读取版本号（首个 version 赋值行即主程序版本）
VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$PROJECT_DIR/Cargo.toml" | head -n1)
if [ -z "$VERSION" ]; then
    echo "错误：无法从 Cargo.toml 解析版本号" >&2
    exit 1
fi
ARCH="$(dpkg --print-architecture)"
DEB_NAME="foims_${VERSION}_${ARCH}.deb"

echo "=== FOIMS DEB 打包脚本 (Debian 规范) ==="
echo "项目目录: $PROJECT_DIR"
echo "打包目录: $DEBPAK_DIR"
echo "版本: $VERSION  架构: $ARCH"

cd "$PROJECT_DIR"

echo ""
echo "1. 编译 release 版本..."
cargo build --release

if [ ! -f "target/release/$BINARY_NAME" ]; then
    echo "错误: 编译失败，找不到二进制文件" >&2
    exit 1
fi

echo ""
echo "2. 清理旧的打包文件..."
rm -rf "$DEBPAK_DIR"

echo ""
echo "3. 创建目录结构..."
mkdir -p "$DEBPAK_DIR/usr/bin"
mkdir -p "$DEBPAK_DIR/opt/foims"
mkdir -p "$DEBPAK_DIR/var/log/foims"
mkdir -p "$DEBPAK_DIR/etc/foims"
mkdir -p "$DEBPAK_DIR/usr/lib/systemd/system"
mkdir -p "$DEBPAK_DIR/usr/share/doc/foims"
mkdir -p "$DEBPAK_DIR/usr/share/man/man1"
mkdir -p "$DEBPAK_DIR/usr/share/lintian/overrides"
mkdir -p "$DEBPAK_DIR/usr/share/polkit-1/actions"
mkdir -p "$DEBPAK_DIR/usr/share/polkit-1/rules.d"

echo ""
echo "4. 复制二进制文件..."
cp -f "target/release/$BINARY_NAME" "$DEBPAK_DIR/usr/bin/"
chmod 755 "$DEBPAK_DIR/usr/bin/$BINARY_NAME"
# strip 可选（最小化环境可能未装 binutils）
if command -v strip >/dev/null 2>&1; then
    strip "$DEBPAK_DIR/usr/bin/$BINARY_NAME"
fi

echo ""
echo "5. 复制资源文件..."
# 仅打包 web/static：后端与 nginx 均只消费该目录，
# 其余（package.json/tests/lint 配置/.trae）为开发工具链文件
if [ -d "web/static" ]; then
    mkdir -p "$DEBPAK_DIR/opt/foims/web"
    cp -r web/static "$DEBPAK_DIR/opt/foims/web/"
    find "$DEBPAK_DIR/opt/foims/web" -type d -exec chmod 755 {} \;
    find "$DEBPAK_DIR/opt/foims/web" -type f -exec chmod 644 {} \;
fi

# 配置文件打包（conffile，升级时 dpkg 保留本地修改）：
# 以 config.toml.example 副本去除示例后缀进包——本地 config.toml 可能
# 含真实数据库连接信息，绝不直接打包；JWT 密钥由本脚本生成随机
# 32 字符（纯字母数字，TOML 与 sed 分隔符安全）替换示例占位符
if [ ! -f "config.toml.example" ]; then
    echo "错误：缺少 config.toml.example" >&2
    exit 1
fi
cp config.toml.example "$DEBPAK_DIR/etc/foims/config.toml"
JWT_SECRET="$(LC_ALL=C tr -dc 'A-Za-z0-9' </dev/urandom | head -c 32)"
if [ "${#JWT_SECRET}" -ne 32 ]; then
    echo "错误：生成 JWT 随机密钥失败" >&2
    exit 1
fi
sed -i "s|^secret = \"CHANGE_ME_TO_RANDOM_32_PLUS_CHARS\"|secret = \"$JWT_SECRET\"|" \
    "$DEBPAK_DIR/etc/foims/config.toml"
# 占位符未被替换说明示例文件结构变化（占位符行改名/删除），必须失败退出，
# 否则包内配置会带着公开示例密钥进入生产
if grep -q "CHANGE_ME_TO_RANDOM_32_PLUS_CHARS" "$DEBPAK_DIR/etc/foims/config.toml"; then
    echo "错误：JWT 密钥占位符替换失败，请检查 config.toml.example 的 [jwt] secret 行" >&2
    exit 1
fi
chmod 640 "$DEBPAK_DIR/etc/foims/config.toml"

# 复制部署说明（nginx/fail2ban 配置与服务文件供运维参考）
if [ -d "deploy" ]; then
    mkdir -p "$DEBPAK_DIR/usr/share/foims/deploy"
    cp -r deploy/nginx deploy/services deploy/fail2ban "$DEBPAK_DIR/usr/share/foims/deploy/"
    find "$DEBPAK_DIR/usr/share/foims/deploy" -type f -exec chmod 644 {} \;
fi

echo ""
echo "6. 创建 polkit 规则（允许 foims 用户重启服务）..."
cat > "$DEBPAK_DIR/usr/share/polkit-1/actions/cc.example.foims.policy" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC
 "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <vendor>FOIMS</vendor>
  <vendor_url>https://github.com/example/foims</vendor_url>

  <action id="cc.example.foims.restart">
    <description>Restart FOIMS Service</description>
    <message>Authentication is required to restart the FOIMS service</message>
    <defaults>
      <allow_any>auth_admin</allow_any>
      <allow_inactive>auth_admin</allow_inactive>
      <allow_active>auth_admin_keep</allow_active>
    </defaults>
    <annotate key="org.freedesktop.policykit.exec.path">/usr/bin/systemctl</annotate>
    <annotate key="org.freedesktop.policykit.exec.argv1">restart</annotate>
    <annotate key="org.freedesktop.policykit.exec.argv2">foims.service</annotate>
  </action>
</policyconfig>
EOF

cat > "$DEBPAK_DIR/usr/share/polkit-1/rules.d/foims.rules" << 'EOF'
// Allow foims user to restart foims.service without authentication
polkit.addRule(function(action, subject) {
    if (action.id == "org.freedesktop.systemd1.manage-units" &&
        action.lookup("unit") == "foims.service" &&
        action.lookup("verb") == "restart" &&
        subject.user == "foims") {
        return polkit.Result.YES;
    }
});
EOF

echo ""
echo "7. 安装 systemd service 文件（来源 deploy/services/foims.service）..."
cp "$SERVICE_SRC" "$DEBPAK_DIR/usr/lib/systemd/system/foims.service"

echo ""
echo "8. 创建 DEBIAN/control 文件..."
mkdir -p "$DEBPAK_DIR/DEBIAN"

cat > "$DEBPAK_DIR/DEBIAN/control" << EOF
Package: foims
Version: $VERSION
Section: net
Priority: optional
Architecture: $ARCH
Maintainer: oi-io <boss@oi-io.cc>
Installed-Size: 0
Depends: libc6 (>= 2.31), adduser
Recommends: postgresql
Suggests: nginx
Homepage: https://github.com/example/foims
Description: Organization IT Information Management System
 FOIMS - Organization IT Information Management System based on Rust
 Axum and PostgreSQL, with web interface.
 Features include:
  - IP allocation and tracking
  - Switch management with SNMP support
  - Network topology visualization
  - Cabinet and workstation management
EOF

echo ""
echo "9. 创建 preinst 脚本..."
cat > "$DEBPAK_DIR/DEBIAN/preinst" << 'EOF'
#!/bin/sh
set -e

# 升级时停止旧服务
if [ "$1" = "upgrade" ]; then
    if command -v systemctl >/dev/null 2>&1; then
        if systemctl is-active --quiet foims 2>/dev/null; then
            deb-systemd-invoke stop foims.service || true
        fi
    fi
fi

exit 0
EOF
chmod 755 "$DEBPAK_DIR/DEBIAN/preinst"

echo ""
echo "10. 创建 postinst 脚本..."
cat > "$DEBPAK_DIR/DEBIAN/postinst" << 'EOF'
#!/bin/sh
set -e

case "$1" in
    configure)
        # 创建 foims 系统用户
        if ! getent passwd foims > /dev/null; then
            adduser --system \
                --shell /usr/sbin/nologin \
                --home /opt/foims \
                --no-create-home \
                --gecos "FOIMS Service Account" \
                --group \
                foims
        fi

        # 创建必要的目录
        install -d -o foims -g foims -m 755 /var/log/foims
        install -d -o foims -g foims -m 755 /opt/foims
        install -d -o foims -g foims -m 755 /etc/foims

        # 设置配置文件权限
        if [ -f /etc/foims/config.toml ]; then
            chown foims:foims /etc/foims/config.toml
            chmod 640 /etc/foims/config.toml
        fi

        # 重载 systemd 并启用服务
        if command -v systemctl >/dev/null 2>&1; then
            systemctl daemon-reload
            deb-systemd-helper enable foims.service >/dev/null || true
            deb-systemd-invoke start foims.service >/dev/null || true
        fi
        ;;

    abort-upgrade|abort-remove|abort-deconfigure)
        ;;

    *)
        echo "postinst called with unknown argument \`$1'" >&2
        ;;
esac

exit 0
EOF
chmod 755 "$DEBPAK_DIR/DEBIAN/postinst"

echo ""
echo "11. 创建 prerm 脚本..."
cat > "$DEBPAK_DIR/DEBIAN/prerm" << 'EOF'
#!/bin/sh
set -e

case "$1" in
    remove|upgrade|deconfigure)
        if command -v systemctl >/dev/null 2>&1; then
            deb-systemd-invoke stop foims.service || true
        fi
        ;;

    failed-upgrade)
        ;;

    *)
        echo "prerm called with unknown argument \`$1'" >&2
        ;;
esac

exit 0
EOF
chmod 755 "$DEBPAK_DIR/DEBIAN/prerm"

echo ""
echo "12. 创建 postrm 脚本..."
cat > "$DEBPAK_DIR/DEBIAN/postrm" << 'EOF'
#!/bin/sh
set -e

case "$1" in
    purge)
        # 禁用服务
        if command -v systemctl >/dev/null 2>&1; then
            deb-systemd-helper disable foims.service >/dev/null || true
            deb-systemd-helper purge foims.service >/dev/null || true
        fi

        # 删除数据目录
        rm -rf /opt/foims/*

        # 删除用户
        if getent passwd foims > /dev/null; then
            deluser --system foims || true
        fi
        ;;

    remove|upgrade|failed-upgrade|abort-install|abort-upgrade|disappear)
        ;;

    *)
        echo "postrm called with unknown argument \`$1'" >&2
        ;;
esac

exit 0
EOF
chmod 755 "$DEBPAK_DIR/DEBIAN/postrm"

echo ""
echo "13. 创建 conffiles 文件..."
cat > "$DEBPAK_DIR/DEBIAN/conffiles" << 'EOF'
/etc/foims/config.toml
EOF

echo ""
echo "14. 创建文档文件..."
cat > "$DEBPAK_DIR/usr/share/doc/foims/copyright" << 'EOF'
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: foims
Upstream-Contact: oi-io <boss@oi-io.cc>
Source: https://github.com/example/foims

Files: *
Copyright: 2024-2026 oi-io
License: GPL-3.0-or-later

License: GPL-3.0-or-later
 This program is free software: you can redistribute it and/or modify
 it under the terms of the GNU General Public License as published by
 the Free Software Foundation, either version 3 of the License, or
 (at your option) any later version.
 .
 This program is distributed in the hope that it will be useful,
 but WITHOUT ANY WARRANTY; without even the implied warranty of
 MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 GNU General Public License for more details.
 .
 You should have received a copy of the GNU General Public License
 along with this program. If not, see <https://www.gnu.org/licenses/>.
EOF

# 创建 changelog（Debian 格式）
cat > "$DEBPAK_DIR/usr/share/doc/foims/changelog" << EOF
foims ($VERSION) stable; urgency=medium

  * Release version $VERSION
  * Organization IT Information Management System with web interface

 -- oi-io <boss@oi-io.cc>  $(date -R)
EOF
gzip -9 -n "$DEBPAK_DIR/usr/share/doc/foims/changelog"

if [ -f "README.md" ]; then
    cp README.md "$DEBPAK_DIR/usr/share/doc/foims/README"
    gzip -9 -n "$DEBPAK_DIR/usr/share/doc/foims/README"
fi

echo ""
echo "15. 创建 man 手册页..."
cat > "$DEBPAK_DIR/usr/share/man/man1/foims.1" << EOF
.TH FOIMS 1 "$(date +'%B %Y')" "foims $VERSION" "Organization IT Information Management System"
.SH NAME
foims \- Organization IT Information Management System
.SH SYNOPSIS
.B foims
.RI [ options ]
.SH DESCRIPTION
.B foims
is an organization IT information management system based on Rust
Axum and PostgreSQL, with a web interface.
.PP
Features include:
.IP \(bu 2
IP allocation and tracking
.IP \(bu 2
Switch management with SNMP support
.IP \(bu 2
Network topology visualization
.IP \(bu 2
Cabinet and workstation management
.SH OPTIONS
.TP
.B \-h, \-\-help
Show help message and exit.
.TP
.B \-V, \-\-version
Show version information and exit.
.SH FILES
.TP
.I /etc/foims/config.toml
Configuration file.
.TP
.I /opt/foims/
Application resources directory.
.TP
.I /var/log/foims/
Log files directory.
.TP
.I /usr/share/foims/deploy/
Reference nginx/systemd/fail2ban deployment configurations.
.SH SERVICE
The application runs as a systemd service:
.PP
.nf
.RS
systemctl start foims
systemctl stop foims
systemctl status foims
.RE
.fi
.SH AUTHOR
oi-io <boss@oi-io.cc>
.SH "SEE ALSO"
.BR systemd (1)
EOF
gzip -9 -n "$DEBPAK_DIR/usr/share/man/man1/foims.1"

echo ""
echo "16. 创建 lintian 覆盖文件..."
cat > "$DEBPAK_DIR/usr/share/lintian/overrides/foims" << 'EOF'
# /opt 目录用于第三方软件，符合 FHS 3.0 规范
foims: dir-or-file-in-opt

# 嵌入的库是静态链接的，无法避免
foims: embedded-library

# 使用 deb-systemd-helper 而非直接调用 systemctl
foims: maintainer-script-calls-systemctl

# 配置文件需要受限权限以保护敏感信息
foims: non-standard-file-perm 0640 != 0644 [etc/foims/config.toml]
EOF

echo ""
echo "17. 设置文件权限..."
chmod 755 "$DEBPAK_DIR/usr/bin/$BINARY_NAME"
chmod 755 "$DEBPAK_DIR/DEBIAN/preinst"
chmod 755 "$DEBPAK_DIR/DEBIAN/postinst"
chmod 755 "$DEBPAK_DIR/DEBIAN/prerm"
chmod 755 "$DEBPAK_DIR/DEBIAN/postrm"
chmod 644 "$DEBPAK_DIR/usr/lib/systemd/system/foims.service"
chmod 644 "$DEBPAK_DIR/usr/share/doc/foims/copyright"

# 为特定目录设置权限
chmod 755 "$DEBPAK_DIR/usr/bin"
chmod 755 "$DEBPAK_DIR/opt"
chmod 755 "$DEBPAK_DIR/opt/foims"
chmod 755 "$DEBPAK_DIR/var"
chmod 755 "$DEBPAK_DIR/var/log"
chmod 755 "$DEBPAK_DIR/var/log/foims"
chmod 755 "$DEBPAK_DIR/etc"
chmod 755 "$DEBPAK_DIR/etc/foims"
chmod 755 "$DEBPAK_DIR/usr"
chmod 755 "$DEBPAK_DIR/usr/lib"
chmod 755 "$DEBPAK_DIR/usr/lib/systemd"
chmod 755 "$DEBPAK_DIR/usr/lib/systemd/system"
chmod 755 "$DEBPAK_DIR/usr/share"
chmod 755 "$DEBPAK_DIR/usr/share/doc"
chmod 755 "$DEBPAK_DIR/usr/share/doc/foims"
chmod 755 "$DEBPAK_DIR/usr/share/man"
chmod 755 "$DEBPAK_DIR/usr/share/man/man1"
chmod 755 "$DEBPAK_DIR/usr/share/lintian"
chmod 755 "$DEBPAK_DIR/usr/share/lintian/overrides"
chmod 755 "$DEBPAK_DIR/usr/share/polkit-1"
chmod 755 "$DEBPAK_DIR/usr/share/polkit-1/actions"
chmod 755 "$DEBPAK_DIR/usr/share/polkit-1/rules.d"

# 为特定文件设置权限
chmod 644 "$DEBPAK_DIR/usr/share/polkit-1/actions/cc.example.foims.policy"
chmod 644 "$DEBPAK_DIR/usr/share/polkit-1/rules.d/foims.rules"
chmod 644 "$DEBPAK_DIR/usr/share/lintian/overrides/foims"

echo ""
echo "18. 更新 Installed-Size..."
INSTALLED_SIZE=$(du -sk "$DEBPAK_DIR" | cut -f1)
sed -i "s/^Installed-Size:.*/Installed-Size: $INSTALLED_SIZE/" "$DEBPAK_DIR/DEBIAN/control"

echo ""
echo "19. 构建 DEB 包..."
dpkg-deb --root-owner-group --build "$DEBPAK_DIR" "$PROJECT_DIR/target/$DEB_NAME"

echo ""
echo "=== 打包完成 ==="
echo "DEB 包: $PROJECT_DIR/target/$DEB_NAME"
ls -lh "$PROJECT_DIR/target/$DEB_NAME"

echo ""
echo "验证 DEB 包..."
dpkg-deb --info "$PROJECT_DIR/target/$DEB_NAME"
if command -v lintian >/dev/null 2>&1; then
    lintian "$PROJECT_DIR/target/$DEB_NAME" --tag-display-limit 0 || true
else
    echo "提示：未安装 lintian，跳过静态检查"
fi

rm -rf "$DEBPAK_DIR"
echo "已删除临时目录: $DEBPAK_DIR"

echo ""
echo "安装命令: sudo dpkg -i target/$DEB_NAME"
echo "查看内容: dpkg -c target/$DEB_NAME"
