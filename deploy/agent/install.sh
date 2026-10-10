#!/bin/sh
# FOIMS Agent 安装脚本（zip 场景，设计 docs/agent-design.md §6.4）
#
# 用法：解压下载的 zip 后，在产物目录执行：
#   sudo sh install.sh
# 包内为按当前平台定向组包的单一二进制（服务端动态打包，无需架构探测），
# 本脚本将其与配置落位并注册系统服务（systemd 优先，sysvinit 兜底）。
set -eu

# 安装需要写 /usr/local/bin、/etc/foims-agent 并注册系统服务，必须 root
if [ "$(id -u)" -ne 0 ]; then
    echo "错误：请使用 sudo 运行本脚本（需要写 /usr/local/bin 与 /etc/foims-agent 并注册系统服务）" >&2
    exit 1
fi

SRC_DIR="$(cd "$(dirname "$0")" && pwd)"

# zip 内文件互相配套（agent.toml 含一次性 token，client.key 为 mTLS 客户端私钥），
# 拆散后安装会失败
for f in foims-agent agent.toml ca.pem client.pem client.key; do
    if [ ! -f "$SRC_DIR/$f" ]; then
        echo "错误：缺少 $f（请勿删改 zip 内文件后安装）" >&2
        exit 1
    fi
done

# 二进制与配置落位：二进制 0755；agent.toml 含 token 与 client.key 私钥均限 0600
install -m 755 "$SRC_DIR/foims-agent" /usr/local/bin/foims-agent
install -d -m 755 /etc/foims-agent
install -m 600 "$SRC_DIR/agent.toml" /etc/foims-agent/agent.toml
install -m 644 "$SRC_DIR/ca.pem" /etc/foims-agent/ca.pem
install -m 644 "$SRC_DIR/client.pem" /etc/foims-agent/client.pem
install -m 600 "$SRC_DIR/client.key" /etc/foims-agent/client.key

# 启动前自检：架构不匹配（exec format error）或二进制损坏在此暴露，
# 避免注册服务后进入无意义的重启循环
if ! /usr/local/bin/foims-agent --version >/dev/null 2>&1; then
    echo "错误：foims-agent 二进制无法执行（架构不匹配或文件损坏），请确认下载平台与主机架构一致" >&2
    exit 1
fi

if command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
    cat > /etc/systemd/system/foims-agent.service << 'UNIT_EOF'
[Unit]
Description=FOIMS Agent host metrics collector
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/foims-agent --config /etc/foims-agent/agent.toml
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
UNIT_EOF
    systemctl daemon-reload
    systemctl enable foims-agent.service
    systemctl restart foims-agent.service
    echo "FOIMS Agent 已安装并启动（systemd）：systemctl status foims-agent"
else
    # sysvinit 兜底：无 systemd 的环境（老发行版/容器）注册 init.d 脚本
    cat > /etc/init.d/foims-agent << 'INIT_EOF'
#!/bin/sh
### BEGIN INIT INFO
# Provides:          foims-agent
# Required-Start:    $network
# Default-Start:     2 3 4 5
# Default-Stop:      0 1 6
# Short-Description: FOIMS Agent host metrics collector
### END INIT INFO
DAEMON=/usr/local/bin/foims-agent
case "$1" in
    start)
        pgrep -x foims-agent >/dev/null 2>&1 || "$DAEMON" --config /etc/foims-agent/agent.toml &
        ;;
    stop)
        pkill -x foims-agent 2>/dev/null || true
        ;;
    restart)
        "$0" stop
        sleep 1
        "$0" start
        ;;
    status)
        if pgrep -x foims-agent >/dev/null 2>&1; then
            echo "foims-agent is running"
        else
            echo "foims-agent is stopped"
        fi
        ;;
    *)
        echo "用法: $0 {start|stop|restart|status}" >&2
        exit 1
        ;;
esac
INIT_EOF
    chmod 755 /etc/init.d/foims-agent
    if command -v update-rc.d >/dev/null 2>&1; then
        update-rc.d foims-agent defaults
    elif command -v chkconfig >/dev/null 2>&1; then
        chkconfig --add foims-agent
    fi
    /etc/init.d/foims-agent restart
    echo "FOIMS Agent 已安装并启动（sysvinit）"
fi
