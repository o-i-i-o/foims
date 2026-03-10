#!/bin/bash

set -e

if [ "$(id -u)" -ne 0 ]; then
    echo "错误：请以 root 权限运行此脚本" >&2
    exit 1
fi

SCRIPT_DIR="/root/ipma"
PROJECT_DIR="/root/ipma"
DEBPAK_DIR="$PROJECT_DIR/debpak"
BINARY_NAME="ipma"

# 从 Cargo.toml 读取版本号
VERSION=$(grep -m1 '^version = "' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
ARCH="amd64"
DEB_NAME="ipma_${VERSION}_${ARCH}.deb"

echo "=== IPMA DEB 打包脚本 (Debian 规范) ==="
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
mkdir -p "$DEBPAK_DIR/opt/ipma"
mkdir -p "$DEBPAK_DIR/var/log/ipma"
mkdir -p "$DEBPAK_DIR/etc/ipma"
mkdir -p "$DEBPAK_DIR/usr/lib/systemd/system"
mkdir -p "$DEBPAK_DIR/usr/share/doc/ipma"
mkdir -p "$DEBPAK_DIR/usr/share/man/man1"
mkdir -p "$DEBPAK_DIR/usr/share/lintian/overrides"
mkdir -p "$DEBPAK_DIR/usr/share/ipma/scripts"
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
    cp -r web "$DEBPAK_DIR/opt/ipma/"
    # 删除开发配置文件
    rm -f "$DEBPAK_DIR/opt/ipma/web/.eslintrc.json"
    rm -f "$DEBPAK_DIR/opt/ipma/web/.editorconfig"
    # 设置 web 目录权限
    find "$DEBPAK_DIR/opt/ipma/web" -type d -exec chmod 755 {} \;
    find "$DEBPAK_DIR/opt/ipma/web" -type f -exec chmod 644 {} \;
fi
if [ -d "templates" ]; then
    cp -r templates "$DEBPAK_DIR/opt/ipma/"
    # 设置 templates 目录权限
    find "$DEBPAK_DIR/opt/ipma/templates" -type d -exec chmod 755 {} \;
    find "$DEBPAK_DIR/opt/ipma/templates" -type f -exec chmod 644 {} \;
fi

# 复制配置文件到 /etc/ipma/
if [ -f "config.toml" ]; then
    cp config.toml "$DEBPAK_DIR/etc/ipma/config.toml"
    chmod 640 "$DEBPAK_DIR/etc/ipma/config.toml"
fi

# 复制初始化脚本
if [ -d "scripts" ]; then
    cp scripts/*.sh "$DEBPAK_DIR/usr/share/ipma/scripts/"
    chmod 755 "$DEBPAK_DIR/usr/share/ipma/scripts/"*.sh
fi

echo ""
echo "6. 创建 polkit 规则（允许 ipma 用户重启服务）..."
cat > "$DEBPAK_DIR/usr/share/polkit-1/actions/cc.example.ipma.policy" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC
 "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <vendor>IPMA</vendor>
  <vendor_url>https://github.com/example/ipma</vendor_url>

  <action id="cc.example.ipma.restart">
    <description>Restart IPMA Service</description>
    <message>Authentication is required to restart the IPMA service</message>
    <defaults>
      <allow_any>auth_admin</allow_any>
      <allow_inactive>auth_admin</allow_inactive>
      <allow_active>auth_admin_keep</allow_active>
    </defaults>
    <annotate key="org.freedesktop.policykit.exec.path">/usr/bin/systemctl</annotate>
    <annotate key="org.freedesktop.policykit.exec.argv1">restart</annotate>
    <annotate key="org.freedesktop.policykit.exec.argv2">ipma.service</annotate>
  </action>
</policyconfig>
EOF

cat > "$DEBPAK_DIR/usr/share/polkit-1/rules.d/ipma.rules" << 'EOF'
// Allow ipma user to restart ipma.service without authentication
polkit.addRule(function(action, subject) {
    if (action.id == "org.freedesktop.systemd1.manage-units" &&
        action.lookup("unit") == "ipma.service" &&
        action.lookup("verb") == "restart" &&
        subject.user == "ipma") {
        return polkit.Result.YES;
    }
});
EOF

echo ""
echo "7. 创建 systemd service 文件..."
cat > "$DEBPAK_DIR/usr/lib/systemd/system/ipma.service" << 'EOF'
[Unit]
Description=IP Management Application
Documentation=man:ipma(1)
After=network.target postgresql.service
Wants=postgresql.service

[Service]
Type=simple
User=ipma
Group=ipma
WorkingDirectory=/opt/ipma
ExecStart=/usr/bin/ipma
Restart=always
RestartSec=5s

# 允许绑定低端口 (80, 443)
AmbientCapabilities=CAP_NET_BIND_SERVICE

# 安全加固
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/ipma /var/log/ipma /etc/ipma
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
Package: ipma
Version: $VERSION
Section: net
Priority: optional
Architecture: $ARCH
Maintainer: IPMA Team <admin@example.com>
Installed-Size: $INSTALLED_SIZE
Depends: libc6 (>= 2.31), adduser
Recommends: postgresql
Suggests: nginx
Homepage: https://github.com/example/ipma
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
        if systemctl is-active --quiet ipma 2>/dev/null; then
            deb-systemd-invoke stop ipma.service || true
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
        # 创建 ipma 系统用户
        if ! getent passwd ipma > /dev/null; then
            adduser --system \
                --shell /usr/sbin/nologin \
                --home /opt/ipma \
                --no-create-home \
                --gecos "IPMA Service Account" \
                --group \
                ipma
        fi

        # 创建必要的目录
        install -d -o ipma -g ipma -m 755 /var/log/ipma
        install -d -o ipma -g ipma -m 755 /opt/ipma
        install -d -o ipma -g ipma -m 755 /etc/ipma

        # 设置配置文件权限
        if [ -f /etc/ipma/config.toml ]; then
            chown ipma:ipma /etc/ipma/config.toml
            chmod 640 /etc/ipma/config.toml
        fi

        # 重载并启用 systemd
        if command -v systemctl >/dev/null 2>&1; then
            deb-systemd-helper enable ipma.service >/dev/null || true
            deb-systemd-invoke start ipma.service >/dev/null || true
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
            deb-systemd-invoke stop ipma.service || true
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
            deb-systemd-helper disable ipma.service >/dev/null || true
            deb-systemd-helper purge ipma.service >/dev/null || true
        fi

        # 删除数据目录
        rm -rf /opt/ipma/*

        # 删除用户
        if getent passwd ipma > /dev/null; then
            deluser --system ipma || true
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
/etc/ipma/config.toml
EOF

echo ""
echo "14. 创建文档文件..."
cat > "$DEBPAK_DIR/usr/share/doc/ipma/copyright" << 'EOF'
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: ipma
Upstream-Contact: IPMA Team <admin@example.com>
Source: https://github.com/example/ipma

Files: *
Copyright: 2024 IPMA Team
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
cat > "$DEBPAK_DIR/usr/share/doc/ipma/changelog" << EOF
ipma ($VERSION) stable; urgency=medium

  * Release version $VERSION
  * IP Management Application with web interface

 -- IPMA Team <admin@example.com>  $(date -R)
EOF
gzip -9 -n "$DEBPAK_DIR/usr/share/doc/ipma/changelog"

if [ -f "README.md" ]; then
    cp README.md "$DEBPAK_DIR/usr/share/doc/ipma/README"
    gzip -9 -n "$DEBPAK_DIR/usr/share/doc/ipma/README"
fi

echo ""
echo "15. 创建 man 手册页..."
cat > "$DEBPAK_DIR/usr/share/man/man1/ipma.1" << EOF
.TH IPMA 1 "$(date +'%B %Y')" "ipma $VERSION" "IP Management Application"
.SH NAME
ipma \- IP Management Application
.SH SYNOPSIS
.B ipma
.RI [ options ]
.SH DESCRIPTION
.B ipma
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
.I /etc/ipma/config.toml
Configuration file.
.TP
.I /opt/ipma/
Application resources directory.
.TP
.I /var/log/ipma/
Log files directory.
.SH SERVICE
The application runs as a systemd service:
.PP
.nf
.RS
systemctl start ipma
systemctl stop ipma
systemctl status ipma
.RE
.fi
.SH AUTHOR
IPMA Team <admin@example.com>
.SH "SEE ALSO"
.BR systemd (1),
.BR postgresql (1)
EOF
gzip -9 -n "$DEBPAK_DIR/usr/share/man/man1/ipma.1"

echo ""
echo "16. 创建 lintian 覆盖文件..."
cat > "$DEBPAK_DIR/usr/share/lintian/overrides/ipma" << 'EOF'
# /opt 目录用于第三方软件，符合 FHS 3.0 规范
ipma: dir-or-file-in-opt

# 嵌入的库是静态链接的，无法避免
ipma: embedded-library

# 使用 deb-systemd-helper 而非直接调用 systemctl
ipma: maintainer-script-calls-systemctl

# 配置文件需要受限权限以保护敏感信息
ipma: non-standard-file-perm 0640 != 0644 [etc/ipma/config.toml]
EOF

echo ""
echo "17. 设置文件权限..."
# 精细设置权限，避免递归
chmod 755 "$DEBPAK_DIR/usr/bin/$BINARY_NAME"
chmod 755 "$DEBPAK_DIR/DEBIAN/preinst"
chmod 755 "$DEBPAK_DIR/DEBIAN/postinst"
chmod 755 "$DEBPAK_DIR/DEBIAN/prerm"
chmod 755 "$DEBPAK_DIR/DEBIAN/postrm"
chmod 644 "$DEBPAK_DIR/usr/lib/systemd/system/ipma.service"
chmod 644 "$DEBPAK_DIR/usr/share/doc/ipma/copyright"

# 为特定目录设置权限
chmod 755 "$DEBPAK_DIR/usr/bin"
chmod 755 "$DEBPAK_DIR/opt"
chmod 755 "$DEBPAK_DIR/opt/ipma"
chmod 755 "$DEBPAK_DIR/var"
chmod 755 "$DEBPAK_DIR/var/log"
chmod 755 "$DEBPAK_DIR/var/log/ipma"
chmod 755 "$DEBPAK_DIR/etc"
chmod 755 "$DEBPAK_DIR/etc/ipma"
chmod 755 "$DEBPAK_DIR/usr"
chmod 755 "$DEBPAK_DIR/usr/lib"
chmod 755 "$DEBPAK_DIR/usr/lib/systemd"
chmod 755 "$DEBPAK_DIR/usr/lib/systemd/system"
chmod 755 "$DEBPAK_DIR/usr/share"
chmod 755 "$DEBPAK_DIR/usr/share/doc"
chmod 755 "$DEBPAK_DIR/usr/share/doc/ipma"
chmod 755 "$DEBPAK_DIR/usr/share/man"
chmod 755 "$DEBPAK_DIR/usr/share/man/man1"
chmod 755 "$DEBPAK_DIR/usr/share/lintian"
chmod 755 "$DEBPAK_DIR/usr/share/lintian/overrides"
chmod 755 "$DEBPAK_DIR/usr/share/ipma"
chmod 755 "$DEBPAK_DIR/usr/share/ipma/scripts"
chmod 755 "$DEBPAK_DIR/usr/share/polkit-1"
chmod 755 "$DEBPAK_DIR/usr/share/polkit-1/actions"
chmod 755 "$DEBPAK_DIR/usr/share/polkit-1/rules.d"

# 为特定文件设置权限
chmod 644 "$DEBPAK_DIR/usr/share/polkit-1/actions/cc.example.ipma.policy"
chmod 644 "$DEBPAK_DIR/usr/share/polkit-1/rules.d/ipma.rules"
chmod 644 "$DEBPAK_DIR/usr/share/lintian/overrides/ipma"

# 确保脚本文件可执行
if [ -d "$DEBPAK_DIR/usr/share/ipma/scripts" ]; then
    chmod 755 "$DEBPAK_DIR/usr/share/ipma/scripts"/*.sh
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
