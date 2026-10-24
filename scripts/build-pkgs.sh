#!/bin/bash
# FOIMS 主程序多格式打包脚本（deb / rpm / zip）
#
# 用法：bash scripts/build-pkgs.sh
# 产物：
#   target/foims_<版本>_<架构>.deb    # Debian 系安装包（dpkg-deb）
#   target/foims-<版本>-1.<架构>.rpm  # RedHat 系安装包（rpmbuild，与 deb 同内容）
#   target/foims_<版本>_<架构>.zip    # 裸二进制压缩包（无包管理器环境手动部署）
#
# 设计约束：
#   - 仅打包不安装：dpkg-deb --root-owner-group 归一化属主、rpmbuild 用户态
#     _topdir/_dbpath，全程无需 root，可在 CI 与普通用户环境直接运行
#   - rpm 复用 deb 的 staging 目录打包（--exclude DEBIAN 与 lintian 覆盖），
#     两格式内容天然一致；rpm 脚本语义与 deb 维护脚本逐一对齐
#     （%pre 运行环境强检 / %post 建用户与证书自签 / %preun 停启用 /
#     %postun 对齐 purge 清理）
#   - systemd 单元唯一来源为 deploy/services/foims.service，本脚本不再内嵌副本
#   - 前端为 Vue 3 + Vite 工程（frontend/），打包前现场构建（npm ci +
#     vite build），产物 dist/ 内容直接铺开到 /usr/share/foims/web（入口页与
#     assets/、icons/、favicon.ico 同级，无 static/ 中间层）；
#     旧版原生 JS 前端已存档于 web-archive/，不进包
#   - 配置文件以 config.toml.example 副本为基础打包（去除 .example 后缀）：
#     仓库/开发机的本地 config.toml 可能含真实数据库连接信息，不进包；
#     数据库连接要素由安装后的初始化向导在页面填写并回写配置文件；
#     JWT 密钥由本脚本生成随机 32 字符替换示例占位符；
#     配置唯一落点为 /etc/foims/config.toml
#   - 数据库备份统一落在 /usr/share/foims/backups（postinst 建目录并
#     归属 foims:foims 0750，systemd 单元 ReadWritePaths 放行；purge/erase
#     时清理）
#   - nginx 生产配置安装到 /etc/nginx/conf.d/foims.conf（conffile）；
#     nginx/PostgreSQL 不做包级强制依赖（兼容源码编译安装，无 dpkg 包
#     记录的场景），改为 preinst 强制检查其 systemd 服务单元或运行中
#     进程，二者均未检出即中止安装；建议 nginx >= 1.28.1（h2c 上游与
#     HTTP/3）；
#     TLS 证书由 postinst 用 openssl 在安装时自签生成（不预置于包内，
#     避免证书随时间过期；已存在且未过期则跳过）；
#     nginx worker 运行用户由 postinst 检测并自动对齐 uds_group 与
#     SupplementaryGroups（drop-in），兼容 www-data / nginx 等差异
#   - 初始化脚本 init-pgsql.sh 安装到 /usr/share/foims/scripts/（建库移交
#     脚本后，向导第 2 步依赖它；白名单复制，打包工具自身不进包）
#   - Agent 二进制不在本包内：由 agent 独立工作空间 foims-agent/ 的 CI 直出
#     各操作系统安装包发布到 Release 页；服务端 web 页仅现场打包配置包
#     （agent.toml + 证书），不再打包二进制

set -euo pipefail

# 路径推导：脚本位于 scripts/，项目根为其上一级目录（不再硬编码绝对路径）
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
mkdir -p "$DEBPAK_DIR/var/log/foims"
mkdir -p "$DEBPAK_DIR/etc/foims"
mkdir -p "$DEBPAK_DIR/etc/nginx/conf.d"
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
echo "5. 构建并复制前端资源..."
# 前端为 Vue 3 + Vite 工程：打包前现场构建，产物部署到 /usr/share/foims/web
# （Linux 标准 share 布局，nginx $web_dir 直接指向该目录）。Node 工具链缺失即中止，
# 避免打出缺前端资源的安装包
if ! command -v npm >/dev/null 2>&1; then
    echo "错误：未找到 npm，前端构建需要 Node.js 工具链" >&2
    exit 1
fi
(
    cd frontend || exit 1
    npm ci --no-audit --no-fund || exit 1
    npm run build || exit 1
) || exit 1
if [ ! -d "frontend/dist" ]; then
    echo "错误：frontend/dist 不存在，前端构建疑似失败" >&2
    exit 1
fi
mkdir -p "$DEBPAK_DIR/usr/share/foims/web"
# 用 dist/. 镜像内容（不留 web/dist 嵌套，隐藏文件一并拷入）
cp -r frontend/dist/. "$DEBPAK_DIR/usr/share/foims/web/"
find "$DEBPAK_DIR/usr/share/foims/web" -type d -exec chmod 755 {} \;
find "$DEBPAK_DIR/usr/share/foims/web" -type f -exec chmod 644 {} \;

# 配置文件打包（conffile，升级时 dpkg 保留本地修改）：
# 以 config.toml.example 副本去除示例后缀进包——本地 config.toml 可能
# 含真实数据库连接信息，绝不直接打包；JWT 密钥由本脚本生成随机
# 32 字符（纯字母数字前缀 + 字母数字，TOML 与 sed 分隔符安全）替换示例占位符
if [ ! -f "config.toml.example" ]; then
    echo "错误：缺少 config.toml.example" >&2
    exit 1
fi
cp config.toml.example "$DEBPAK_DIR/etc/foims/config.toml"
# 随机串管道各段均消费至 EOF（head 定长读文件后正常退出，base64/tr/cut
# 读完上游才退出）：不存在提前退出的消费者，set -o pipefail 下不会触发
# SIGPIPE。此前 "tr | head -c" 组合中 head 拿满即退出，tr 继续写入收到
# Broken pipe，竞态性地使整条管道失败（CI 实测复现）
RAW_SECRET="$(head -c 64 /dev/urandom | base64 | LC_ALL=C tr -dc 'A-Za-z0-9' | cut -c1-28)"
# 固定前缀补齐大小写/数字/特殊字符四类，避免首启即触发弱密钥告警；
# 28 位随机字母数字（约 166 位熵）承载全部强度
JWT_SECRET="A1a!${RAW_SECRET}"
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

# nginx 配置安装到 /etc/nginx/conf.d/（conffile，升级时 dpkg 保留本地修改）。
# TLS 证书路径指向 /etc/ssl/foims-certs/default_cert.*，由 postinst 用
# openssl 在安装时自签生成（安装时生成而非打包时预置，避免包内证书
# 随时间过期）
if [ ! -f "deploy/nginx/foims.conf" ]; then
    echo "错误：缺少 deploy/nginx/foims.conf" >&2
    exit 1
fi
cp deploy/nginx/foims.conf "$DEBPAK_DIR/etc/nginx/conf.d/foims.conf"
chmod 644 "$DEBPAK_DIR/etc/nginx/conf.d/foims.conf"

# 复制部署说明（nginx/fail2ban 配置与服务文件供运维参考）
if [ -d "deploy" ]; then
    mkdir -p "$DEBPAK_DIR/usr/share/foims/deploy"
    cp -r deploy/nginx deploy/services deploy/fail2ban "$DEBPAK_DIR/usr/share/foims/deploy/"
    find "$DEBPAK_DIR/usr/share/foims/deploy" -type f -exec chmod 644 {} \;
fi

# 复制初始化脚本（白名单）：向导第 2 步数据库配置页引导用户运行该脚本
# 建角色与库（v0.20.24 曾因向导内置建库功能移除打包，v0.20.37 建库移交
# 脚本后恢复）；缺失即失败退出，不带缺口发版
if [ ! -f "scripts/init-pgsql.sh" ]; then
    echo "错误：缺少 scripts/init-pgsql.sh（初始化向导依赖）" >&2
    exit 1
fi
mkdir -p "$DEBPAK_DIR/usr/share/foims/scripts"
cp scripts/init-pgsql.sh "$DEBPAK_DIR/usr/share/foims/scripts/"
chmod 755 "$DEBPAK_DIR/usr/share/foims/scripts/init-pgsql.sh"

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
Recommends: fail2ban
Suggests: nginx (>= 1.28.1), postgresql (>= 15)
Homepage: https://github.com/example/foims
Description: Organization Infrastructure Management System
 FOIMS is a high-performance organization infrastructure management
 platform based on Rust Axum and PostgreSQL, with a web interface.
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

# 强制运行依赖检查：FOIMS 运行依赖 nginx（反代/TLS/静态资源）与
# PostgreSQL（数据库）。不检查 dpkg 包记录，只认「systemd 服务单元
# 或运行中进程」，兼容源码编译安装（无包记录、服务单元名各异）的
# 场景；二者均未检出则中止安装，避免装完即不可用。
check_runtime_component() {
    name="$1"
    proc_name="$2"
    unit_pattern="$3"

    # 1) systemd 服务单元（任意状态：已注册即认可，含未启动的自建服务）
    if command -v systemctl >/dev/null 2>&1; then
        if systemctl list-unit-files --type=service 2>/dev/null | grep -q "$unit_pattern"; then
            echo "已检测到 $name（systemd 服务单元）"
            return 0
        fi
    fi

    # 2) 运行中的进程（编译安装且未注册 systemd 服务的场景）
    if command -v pgrep >/dev/null 2>&1; then
        if pgrep -x "$proc_name" >/dev/null 2>&1; then
            echo "已检测到 $name（运行中的 $proc_name 进程）"
            return 0
        fi
    elif ps -eo comm= 2>/dev/null | grep -qx "$proc_name"; then
        echo "已检测到 $name（运行中的 $proc_name 进程）"
        return 0
    fi

    echo "错误：未检测到 $name 运行环境（既无 systemd 服务单元，也无运行中的 $proc_name 进程）" >&2
    echo "      FOIMS 需要 $name：请完成安装（发行版包或源码编译）并确保其已注册服务或正在运行后重试" >&2
    return 1
}

check_runtime_component "nginx" "nginx" "^nginx\.service" || exit 1
check_runtime_component "PostgreSQL" "postgres" "postgresql.*\.service" || exit 1

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
                --home /usr/share/foims \
                --no-create-home \
                --gecos "FOIMS Service Account" \
                --group \
                foims
        fi

        # 创建必要的目录
        install -d -o foims -g foims -m 755 /var/log/foims
        install -d -o foims -g foims -m 755 /etc/foims
        # 数据库备份目录（初始化向导与定时备份任务的唯一落点）
        install -d -o foims -g foims -m 750 /usr/share/foims/backups

        # 旧版静态资源目录（/opt/foims）已废弃：升级时清理残留，避免
        # 旧版前端文件与旧配置副本被误用；目录清空后一并删除
        rm -rf /opt/foims/web /opt/foims/config.toml
        rmdir /opt/foims 2>/dev/null || true

        # 证书目录：服务用户 foims 需在运行期（证书管理页）写入生成，
        # 750 仅属组可见；nginx master 以 root 读取私钥不受影响
        install -d -o foims -g foims -m 750 /etc/ssl/foims-ca
        install -d -o foims -g foims -m 750 /etc/ssl/foims-certs

        # 设置配置文件权限
        if [ -f /etc/foims/config.toml ]; then
            chown foims:foims /etc/foims/config.toml
            chmod 640 /etc/foims/config.toml
        fi

        # nginx worker 运行用户自动对齐：nginx worker 进程需能连接 UDS
        # socket（属组由 config.toml uds_group 决定，服务进程改属组又依赖
        # SupplementaryGroups）。Debian/Ubuntu 发行版 nginx 为 www-data，
        # nginx.org 官方包/部分环境为 nginx，硬编码任一值都会在另一场景
        # 下 502（connect 13: Permission denied）。此处解析 nginx.conf 的
        # user 指令，同步 config.toml 的 uds_group 与 drop-in 覆盖的
        # SupplementaryGroups，安装即用
        NGINX_USER=""
        if [ -f /etc/nginx/nginx.conf ]; then
            NGINX_USER=$(sed -n 's/^[[:space:]]*user[[:space:]]\{1,\}\([^[:space:];]\{1,\}\).*/\1/p' /etc/nginx/nginx.conf | head -n1)
        fi
        # user 指令缺失或对应用户不存在时回退 Debian 默认
        if [ -z "$NGINX_USER" ] || ! getent passwd "$NGINX_USER" >/dev/null 2>&1; then
            NGINX_USER=www-data
        fi
        # SupplementaryGroups 需要真实存在的组：取该用户主组
        NGINX_GROUP=$(id -gn "$NGINX_USER" 2>/dev/null || echo "$NGINX_USER")

        if [ -f /etc/foims/config.toml ] && grep -q '^uds_group' /etc/foims/config.toml; then
            sed -i "s|^uds_group[[:space:]]*=.*|uds_group = \"$NGINX_USER\"|" /etc/foims/config.toml
        fi

        # drop-in 覆盖服务单元的 SupplementaryGroups（/etc 优先于 /usr/lib，
        # 同名单指令覆盖语义），使非 root 的 foims 进程有权改 socket 属组；
        # 写入在下方 daemon-reload 之前，重启/首启即生效
        install -d -m 755 /etc/systemd/system/foims.service.d
        cat > /etc/systemd/system/foims.service.d/uds-group.conf <<FOIMS_EOF
# 由 foims 包 postinst 按检测到的 nginx worker 用户自动生成（勿手工编辑）：
# 使 foims 进程能将 UDS socket 属组改为反代进程的组
[Service]
SupplementaryGroups=$NGINX_GROUP
FOIMS_EOF

        # nginx 默认证书：已存在且剩余 ≥1 天（checkend 86400 秒）则幂等跳过，
        # 否则安装时自签生成（安装时生成而非打包预置，避免包内证书随时间
        # 过期）。openssl 失败时 set -e 使安装失败：HTTPS 栈无证书不可用，
        # 静默降级会掩盖问题
        FOIMS_CERT=/etc/ssl/foims-certs/default_cert.pem
        FOIMS_KEY=/etc/ssl/foims-certs/default_cert.key
        if [ -f "$FOIMS_CERT" ] && [ -f "$FOIMS_KEY" ] \
            && openssl x509 -checkend 86400 -noout -in "$FOIMS_CERT" >/dev/null 2>&1; then
            echo "nginx 默认证书已存在且有效，跳过生成"
        else
            openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
                -keyout "$FOIMS_KEY" -out "$FOIMS_CERT" -days 3650 \
                -subj "/O=FOIMS/CN=$(hostname)" \
                -addext "subjectAltName=DNS:$(hostname),DNS:localhost,IP:127.0.0.1,IP:::1"
            chmod 644 "$FOIMS_CERT"
            chmod 600 "$FOIMS_KEY"
        fi

        # nginx 配置校验并重载：配置错误同样使安装失败（conffile 已由 dpkg
        # 就位；本地管理员删除该文件时跳过校验，尊重现场裁剪）
        if [ -f /etc/nginx/conf.d/foims.conf ] && command -v nginx >/dev/null 2>&1; then
            nginx -t || {
                echo "错误：nginx 配置校验失败（/etc/nginx/conf.d/foims.conf）" >&2
                exit 1
            }
            if command -v systemctl >/dev/null 2>&1 && systemctl is-active --quiet nginx; then
                systemctl reload nginx || true
            fi
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

        # 删除旧版数据目录（0.29 及以前版本的静态资源目录）
        rm -rf /opt/foims
        # 删除数据库备份（purge 语义为彻底清除）
        rm -rf /usr/share/foims/backups

        # 删除证书物料（站点根 CA 与全部生成/导入证书，purge 语义为彻底清除）
        rm -rf /etc/ssl/foims-ca /etc/ssl/foims-certs /etc/ssl/foims-import-certs /etc/ssl/foims-import-cas

        # 删除 postinst 生成的 UDS 属组 drop-in（purge 语义为彻底清除）
        rm -f /etc/systemd/system/foims.service.d/uds-group.conf
        rmdir --ignore-fail-on-non-empty /etc/systemd/system/foims.service.d 2>/dev/null || true

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
/etc/nginx/conf.d/foims.conf
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
  * Organization Infrastructure Management System with web interface

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
.TH FOIMS 1 "$(date +'%B %Y')" "foims $VERSION" "Organization Infrastructure Management System"
.SH NAME
foims \- Organization Infrastructure Management System
.SH SYNOPSIS
.B foims
.RI [ options ]
.SH DESCRIPTION
.B foims
is an organization infrastructure management system based on Rust
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
.I /etc/nginx/conf.d/foims.conf
nginx reverse proxy configuration (installed as a conffile).
.TP
.I /etc/ssl/foims-certs/
TLS certificates managed by the built-in certificate manager
(default_cert.pem/.key is generated at install time by the postinst).
.TP
.I /usr/share/foims/web/
Frontend static resources served by nginx (document root).
.TP
.I /var/log/foims/
Log files directory.
.TP
.I /usr/share/foims/backups/
Database backups created by the setup wizard and scheduled backup task.
.TP
.I /usr/share/foims/deploy/
Reference nginx/systemd/fail2ban deployment configurations.
.TP
.I /usr/share/foims/scripts/init-pgsql.sh
PostgreSQL initialization script (creates the role and database
required by the setup wizard; run it before step 2 of the wizard).
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
.SH NETWORK
Apart from the Unix domain socket served behind nginx, the process
listens on two UDP ports directly (not proxied); allow them through
the firewall:
.TP
.I udp/9100
FOIMS Agent metric reporting (HTTP/3 over QUIC).
.TP
.I udp/162
SNMP Trap/Inform reception (configurable in /etc/foims/config.toml).
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
chmod 755 "$DEBPAK_DIR/var"
chmod 755 "$DEBPAK_DIR/var/log"
chmod 755 "$DEBPAK_DIR/var/log/foims"
chmod 755 "$DEBPAK_DIR/etc"
chmod 755 "$DEBPAK_DIR/etc/foims"
chmod 755 "$DEBPAK_DIR/etc/nginx"
chmod 755 "$DEBPAK_DIR/etc/nginx/conf.d"
chmod 755 "$DEBPAK_DIR/usr"
chmod 755 "$DEBPAK_DIR/usr/lib"
chmod 755 "$DEBPAK_DIR/usr/lib/systemd"
chmod 755 "$DEBPAK_DIR/usr/lib/systemd/system"
chmod 755 "$DEBPAK_DIR/usr/share"
chmod 755 "$DEBPAK_DIR/usr/share/foims"
chmod 755 "$DEBPAK_DIR/usr/share/foims/web"
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
echo "=== DEB 完成 ==="
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

echo ""
echo "20. 构建 RPM 包（复用 DEB staging，内容一致；rpmbuild 用户态无需 root）..."
if ! command -v rpmbuild >/dev/null 2>&1; then
    echo "错误：缺少 rpmbuild（Debian/Ubuntu 安装：sudo apt-get install rpm）" >&2
    exit 1
fi
# deb 架构 → rpm 架构映射（同机编译，架构与宿主一致）
case "$ARCH" in
    amd64) RPM_ARCH=x86_64 ;;
    arm64) RPM_ARCH=aarch64 ;;
    armhf) RPM_ARCH=armv7hl ;;
    i386)  RPM_ARCH=i686 ;;
    *)     RPM_ARCH="$ARCH" ;;
esac
RPM_NAME="foims-${VERSION}-1.${RPM_ARCH}.rpm"
RPMTOP="$PROJECT_DIR/target/rpmtop"
rm -rf "$RPMTOP"
mkdir -p "$RPMTOP/BUILD" "$RPMTOP/RPMS" "$RPMTOP/SOURCES" "$RPMTOP/SPECS" "$RPMTOP/SRPMS" "$RPMTOP/rpmdb"
# staging 整体作为 Source（排除 DEBIAN 控制目录与 lintian 覆盖等 deb 专有内容）
tar -czf "$RPMTOP/SOURCES/pak.tar.gz" -C "$DEBPAK_DIR" \
    --exclude=./DEBIAN --exclude=./usr/share/lintian .
cat > "$RPMTOP/SPECS/foims.spec" << SPEC_EOF
Name:           foims
Version:        $VERSION
Release:        1
Summary:        Organization Infrastructure Management System
License:        GPL-3.0-or-later
URL:            https://github.com/example/foims
Source0:        pak.tar.gz
Requires:       systemd openssl
Recommends:     fail2ban
Suggests:       nginx >= 1.28.1 postgresql-server >= 15

%description
FOIMS is a high-performance organization infrastructure management
platform based on Rust Axum and PostgreSQL, with a web interface.
Features include IP
allocation and tracking, switch management with SNMP support,
network topology visualization, and cabinet/workstation management.

%install
mkdir -p "%{buildroot}"
tar -xzf "%{SOURCE0}" -C "%{buildroot}"

# 安装前：运行环境强检（与 DEB preinst 同语义，不做包记录检查，
# 只认 systemd 服务单元或运行中进程）＋升级时（\$1=2）停旧服务
%pre
check_runtime_component() {
    name="\$1"
    proc_name="\$2"
    unit_pattern="\$3"
    if command -v systemctl >/dev/null 2>&1; then
        if systemctl list-unit-files --type=service 2>/dev/null | grep -q "\$unit_pattern"; then
            echo "已检测到 \$name（systemd 服务单元）"
            return 0
        fi
    fi
    if command -v pgrep >/dev/null 2>&1; then
        if pgrep -x "\$proc_name" >/dev/null 2>&1; then
            echo "已检测到 \$name（运行中的 \$proc_name 进程）"
            return 0
        fi
    elif ps -eo comm= 2>/dev/null | grep -qx "\$proc_name"; then
        echo "已检测到 \$name（运行中的 \$proc_name 进程）"
        return 0
    fi
    echo "错误：未检测到 \$name 运行环境（既无 systemd 服务单元，也无运行中的 \$proc_name 进程）" >&2
    echo "      FOIMS 需要 \$name：请完成安装并确保其已注册服务或正在运行后重试" >&2
    return 1
}
check_runtime_component "nginx" "nginx" "^nginx\\.service" || exit 1
check_runtime_component "PostgreSQL" "postgres" "postgresql.*\\.service" || exit 1
if [ "\$1" -eq 2 ] && command -v systemctl >/dev/null 2>&1; then
    systemctl stop foims.service 2>/dev/null || true
fi

# 安装后：建用户/目录/权限、nginx worker 属组对齐、自签默认证书、
# nginx 校验重载、启用并拉起服务（与 DEB postinst 同语义）
%post
NOLOGIN=/usr/sbin/nologin
[ -x "\$NOLOGIN" ] || NOLOGIN=/sbin/nologin
getent group foims >/dev/null 2>&1 || groupadd -r foims
id foims >/dev/null 2>&1 || useradd -r -g foims -s "\$NOLOGIN" -d /usr/share/foims -M -c "FOIMS Service Account" foims
install -d -o foims -g foims -m 755 /var/log/foims /etc/foims
install -d -o foims -g foims -m 750 /usr/share/foims/backups
# 旧版静态资源目录（/opt/foims）已废弃：升级时清理残留
rm -rf /opt/foims/web /opt/foims/config.toml
rmdir /opt/foims 2>/dev/null || true
install -d -o foims -g foims -m 750 /etc/ssl/foims-ca /etc/ssl/foims-certs
if [ -f /etc/foims/config.toml ]; then
    chown foims:foims /etc/foims/config.toml
    chmod 640 /etc/foims/config.toml
fi
NGINX_USER=""
if [ -f /etc/nginx/nginx.conf ]; then
    NGINX_USER=\$(sed -n 's/^[[:space:]]*user[[:space:]]\{1,\}\([^[:space:];]\{1,\}\).*/\1/p' /etc/nginx/nginx.conf | head -n1)
fi
if [ -z "\$NGINX_USER" ] || ! getent passwd "\$NGINX_USER" >/dev/null 2>&1; then
    NGINX_USER=www-data
fi
NGINX_GROUP=\$(id -gn "\$NGINX_USER" 2>/dev/null || echo "\$NGINX_USER")
if [ -f /etc/foims/config.toml ] && grep -q '^uds_group' /etc/foims/config.toml; then
    sed -i "s|^uds_group[[:space:]]*=.*|uds_group = \"\$NGINX_USER\"|" /etc/foims/config.toml
fi
install -d -m 755 /etc/systemd/system/foims.service.d
cat > /etc/systemd/system/foims.service.d/uds-group.conf <<FOIMS_EOF
# 由 foims 包 post 脚本按检测到的 nginx worker 用户自动生成（勿手工编辑）：
# 使 foims 进程能将 UDS socket 属组改为反代进程的组
[Service]
SupplementaryGroups=\$NGINX_GROUP
FOIMS_EOF
FOIMS_CERT=/etc/ssl/foims-certs/default_cert.pem
FOIMS_KEY=/etc/ssl/foims-certs/default_cert.key
if [ -f "\$FOIMS_CERT" ] && [ -f "\$FOIMS_KEY" ] \\
    && openssl x509 -checkend 86400 -noout -in "\$FOIMS_CERT" >/dev/null 2>&1; then
    echo "nginx 默认证书已存在且有效，跳过生成"
else
    openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \\
        -keyout "\$FOIMS_KEY" -out "\$FOIMS_CERT" -days 3650 \\
        -subj "/O=FOIMS/CN=\$(hostname)" \\
        -addext "subjectAltName=DNS:\$(hostname),DNS:localhost,IP:127.0.0.1,IP:::1"
    chmod 644 "\$FOIMS_CERT"
    chmod 600 "\$FOIMS_KEY"
fi
if [ -f /etc/nginx/conf.d/foims.conf ] && command -v nginx >/dev/null 2>&1; then
    nginx -t || {
        echo "错误：nginx 配置校验失败（/etc/nginx/conf.d/foims.conf）" >&2
        exit 1
    }
    if systemctl is-active --quiet nginx 2>/dev/null; then
        systemctl reload nginx || true
    fi
fi
if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload
    systemctl enable foims.service >/dev/null 2>&1 || true
    systemctl restart foims.service >/dev/null 2>&1 || true
fi

# 卸载前（erase 卸载 \$1=0 停用并取消自启；升级 \$1=1 不动作，由 %post
# 的 restart 无缝拉起新版）
%preun
if [ "\$1" -eq 0 ] && command -v systemctl >/dev/null 2>&1; then
    systemctl stop foims.service 2>/dev/null || true
    systemctl disable foims.service >/dev/null 2>&1 || true
fi

# 卸载后（erase，对齐 DEB purge 语义：彻底清除数据与运行生成物；
# %config(noreplace) 修改过的配置留 .rpmsave 由运维自行处置）
%postun
if [ "\$1" -eq 0 ]; then
    rm -rf /opt/foims
    rm -rf /usr/share/foims/backups
    rm -rf /etc/ssl/foims-ca /etc/ssl/foims-certs /etc/ssl/foims-import-certs /etc/ssl/foims-import-cas
    rm -f /etc/systemd/system/foims.service.d/uds-group.conf
    rmdir --ignore-fail-on-non-empty /etc/systemd/system/foims.service.d 2>/dev/null || true
    id foims >/dev/null 2>&1 && userdel foims || true
fi

%files
%attr(755,root,root) /usr/bin/foims
/usr/share/foims/web
/var/log/foims
%dir /etc/foims
%config(noreplace) /etc/foims/config.toml
/etc/nginx/conf.d/foims.conf
/usr/lib/systemd/system/foims.service
/usr/share/foims/deploy
/usr/share/foims/scripts
/usr/share/doc/foims
/usr/share/man/man1/foims.1.gz
/usr/share/polkit-1/actions/cc.example.foims.policy
/usr/share/polkit-1/rules.d/foims.rules
SPEC_EOF
# _dbpath 指向临时库：Debian 宿主 /var/lib/rpm 未初始化，缺该定义 rpmbuild
# 会报 sqlite 错（与 agent 侧 rpmbuild 同解法）
rpmbuild -bb --quiet --define "_topdir $RPMTOP" --define "_dbpath $RPMTOP/rpmdb" \
    --target "$RPM_ARCH" "$RPMTOP/SPECS/foims.spec"
mv "$RPMTOP/RPMS/$RPM_ARCH/$RPM_NAME" "$PROJECT_DIR/target/$RPM_NAME"
rm -rf "$RPMTOP"

echo ""
echo "=== RPM 完成 ==="
echo "RPM 包: $PROJECT_DIR/target/$RPM_NAME"
ls -lh "$PROJECT_DIR/target/$RPM_NAME"

echo ""
echo "21. 打包 ZIP（裸二进制，供无包管理器环境手动部署）..."
ZIP_NAME="foims_${VERSION}_${ARCH}.zip"
(cd "$DEBPAK_DIR/usr/bin" && zip -X -q "$PROJECT_DIR/target/$ZIP_NAME" "$BINARY_NAME")

echo ""
echo "=== 打包完成 ==="
echo "DEB 包: $PROJECT_DIR/target/$DEB_NAME"
echo "RPM 包: $PROJECT_DIR/target/$RPM_NAME"
echo "ZIP 包: $PROJECT_DIR/target/$ZIP_NAME"
ls -lh "$PROJECT_DIR/target/$DEB_NAME" "$PROJECT_DIR/target/$RPM_NAME" "$PROJECT_DIR/target/$ZIP_NAME"

echo ""
echo "验证 RPM 包..."
rpm -qpi "$PROJECT_DIR/target/$RPM_NAME" 2>/dev/null || true

rm -rf "$DEBPAK_DIR"
echo "已删除临时目录: $DEBPAK_DIR"

echo ""
echo "安装命令: sudo dpkg -i target/$DEB_NAME  或  sudo rpm -Uvh target/$RPM_NAME"
echo "查看内容: dpkg -c target/$DEB_NAME  或  rpm -qpl target/$RPM_NAME"
