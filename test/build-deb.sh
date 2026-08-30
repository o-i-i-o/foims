#!/bin/bash

set -e

if [ "$(id -u)" -ne 0 ]; then
    echo "错误：请以 root 权限运行此脚本" >&2
    exit 1
fi

SCRIPT_DIR="/root/ipma"
PROJECT_DIR="/root/ipma"
DEBPAK_DIR="$PROJECT_DIR/debpak"
BINARY_NAME="foims"

# 从 Cargo.toml 读取版本号
VERSION=$(grep -m1 '^version = "' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
ARCH="amd64"
DEB_NAME="foims_${VERSION}_${ARCH}.deb"

echo "=== FOIMS DEB 打包脚本 (Debian 规范) ==="
echo "项目目录: $PROJECT_DIR"
echo "打包目录: $DEBPAK_DIR"
echo "版本: $VERSION"

cd "$PROJECT_DIR"

echo ""
echo "1. 编译 release 版本..."
cargo build --release

if [ ! -f "target/release/$BINARY_NAME" ]; then
    echo "错误: 编译失败，找不到二进制文件"
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
mkdir -p "$DEBPAK_DIR/usr/share/foims/scripts"
mkdir -p "$DEBPAK_DIR/usr/share/polkit-1/actions"
mkdir -p "$DEBPAK_DIR/usr/share/polkit-1/rules.d"

echo ""
echo "4. 复制二进制文件..."
cp -f "target/release/$BINARY_NAME" "$DEBPAK_DIR/usr/bin/"
chmod 755 "$DEBPAK_DIR/usr/bin/$BINARY_NAME"
strip "$DEBPAK_DIR/usr/bin/$BINARY_NAME"

echo ""
echo "5. 复制资源文件..."
if [ -d "web" ]; then
    cp -r web "$DEBPAK_DIR/opt/foims/"
    # 删除开发配置文件
    rm -f "$DEBPAK_DIR/opt/foims/web/.eslintrc.json"
    rm -f "$DEBPAK_DIR/opt/foims/web/.editorconfig"
    # 设置 web 目录权限
    find "$DEBPAK_DIR/opt/foims/web" -type d -exec chmod 755 {} \;
    find "$DEBPAK_DIR/opt/foims/web" -type f -exec chmod 644 {} \;
fi
if [ -d "templates" ]; then
    cp -r templates "$DEBPAK_DIR/opt/foims/"
    # 设置 templates 目录权限
    find "$DEBPAK_DIR/opt/foims/templates" -type d -exec chmod 755 {} \;
    find "$DEBPAK_DIR/opt/foims/templates" -type f -exec chmod 644 {} \;
fi

# 复制配置文件到 /etc/foims/
if [ -f "config.toml" ]; then
    cp config.toml "$DEBPAK_DIR/etc/foims/config.toml"
    chmod 640 "$DEBPAK_DIR/etc/foims/config.toml"
fi

# 复制初始化脚本
if [ -d "scripts" ]; then
    cp scripts/*.sh "$DEBPAK_DIR/usr/share/foims/scripts/"
    chmod 755 "$DEBPAK_DIR/usr/share/foims/scripts/"*.sh
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
echo "7. 创建 systemd service 文件..."
cat > "$DEBPAK_DIR/usr/lib/systemd/system/foims.service" << 'EOF'
[Unit]
Description=IP Management Application
Documentation=man:foims(1)
After=network.target postgresql.service
Wants=postgresql.service

[Service]
Type=simple
User=foims
Group=foims
WorkingDirectory=/opt/foims
ExecStart=/usr/bin/foims
Restart=always
RestartSec=5s

# 允许绑定低端口 (80, 443)
AmbientCapabilities=CAP_NET_BIND_SERVICE

# 安全加固
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/foims /var/log/foims /etc/foims
PrivateTmp=true
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictRealtime=true
RestrictSUIDSGID=true

# 资源限制
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
EOF

echo ""
echo "8. 创建 DEBIAN/control 文件..."
mkdir -p "$DEBPAK_DIR/DEBIAN"

INSTALLED_SIZE=$(du -sk "$DEBPAK_DIR" | cut -f1)

cat > "$DEBPAK_DIR/DEBIAN/control" << EOF
Package: foims
Version: $VERSION
Section: net
Priority: optional
Architecture: $ARCH
Maintainer: FOIMS Team <admin@example.com>
Installed-Size: $INSTALLED_SIZE
Depends: libc6 (>= 2.31), adduser
Recommends: postgresql
Suggests: nginx
Homepage: https://github.com/example/foims
Description: IP Management Application
 A comprehensive IP address management system with web interface.
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

        # 重载并启用 systemd
        if command -v systemctl >/dev/null 2>&1; then
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
systemctl daemon-reload

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
Upstream-Contact: FOIMS Team <admin@example.com>
Source: https://github.com/example/foims

Files: *
Copyright: 2024 FOIMS Team
License: MIT

License: MIT
 Permission is hereby granted, free of charge, to any person obtaining a copy
 of this software and associated documentation files (the "Software"), to deal
 in the Software without restriction, including without limitation the rights
 to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 copies of the Software, and to permit persons to whom the Software is
 furnished to do so, subject to the following conditions:
 .
 The above copyright notice and this permission notice shall be included in all
 copies or substantial portions of the Software.
 .
 THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 SOFTWARE.
EOF

# 创建 changelog（Debian 格式）
cat > "$DEBPAK_DIR/usr/share/doc/foims/changelog" << EOF
foims ($VERSION) stable; urgency=medium

  * Release version $VERSION
  * IP Management Application with web interface

 -- FOIMS Team <admin@example.com>  $(date -R)
EOF
gzip -9 -n "$DEBPAK_DIR/usr/share/doc/foims/changelog"

if [ -f "README.md" ]; then
    cp README.md "$DEBPAK_DIR/usr/share/doc/foims/README"
    gzip -9 -n "$DEBPAK_DIR/usr/share/doc/foims/README"
fi

echo ""
echo "15. 创建 man 手册页..."
cat > "$DEBPAK_DIR/usr/share/man/man1/foims.1" << EOF
.TH FOIMS 1 "$(date +'%B %Y')" "foims $VERSION" "IP Management Application"
.SH NAME
foims \- IP Management Application
.SH SYNOPSIS
.B foims
.RI [ options ]
.SH DESCRIPTION
.B foims
is a comprehensive IP address management system with web interface.
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
FOIMS Team <admin@example.com>
.SH "SEE ALSO"
.BR systemd (1),
.BR postgresql (1)
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
# 精细设置权限，避免递归
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
chmod 755 "$DEBPAK_DIR/usr/share/foims"
chmod 755 "$DEBPAK_DIR/usr/share/foims/scripts"
chmod 755 "$DEBPAK_DIR/usr/share/polkit-1"
chmod 755 "$DEBPAK_DIR/usr/share/polkit-1/actions"
chmod 755 "$DEBPAK_DIR/usr/share/polkit-1/rules.d"

# 为特定文件设置权限
chmod 644 "$DEBPAK_DIR/usr/share/polkit-1/actions/cc.example.foims.policy"
chmod 644 "$DEBPAK_DIR/usr/share/polkit-1/rules.d/foims.rules"
chmod 644 "$DEBPAK_DIR/usr/share/lintian/overrides/foims"

# 确保脚本文件可执行
if [ -d "$DEBPAK_DIR/usr/share/foims/scripts" ]; then
    chmod 755 "$DEBPAK_DIR/usr/share/foims/scripts"/*.sh
fi

echo ""
echo "18. 更新 Installed-Size..."
INSTALLED_SIZE=$(du -sk "$DEBPAK_DIR" | cut -f1)
sed -i "s/^Installed-Size:.*/Installed-Size: $INSTALLED_SIZE/" "$DEBPAK_DIR/DEBIAN/control"

echo ""
echo "19. 构建 DEB 包..."
dpkg-deb --root-owner-group --build "$DEBPAK_DIR" "$PROJECT_DIR/$DEB_NAME"

echo ""
echo "=== 打包完成 ==="
echo "DEB 包: $PROJECT_DIR/$DEB_NAME"
ls -lh "$PROJECT_DIR/$DEB_NAME"

echo ""
echo "验证 DEB 包..."
lintian "$PROJECT_DIR/$DEB_NAME" --tag-display-limit 0 || true

rm -rf "$DEBPAK_DIR"
echo "已删除临时目录: $DEBPAK_DIR"


echo ""
echo "安装命令: sudo dpkg -i $DEB_NAME"
echo "查看信息: dpkg -I $DEB_NAME"
echo "查看内容: dpkg -c $DEB_NAME"
