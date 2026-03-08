# IPMA 配置文档

## 目录结构

### 安装后文件路径

| 文件类型 | 路径 | 说明 |
|---------|------|------|
| 二进制文件 | `/usr/bin/ipma` | 主程序可执行文件 |
| 配置文件 | `/etc/ipma/config.toml` | 主配置文件 |
| Web 资源 | `/opt/ipma/web/` | 前端静态资源 |
| 模板文件 | `/opt/ipma/templates/` | 邮件等模板 |
| 日志目录 | `/var/log/ipma/` | 日志文件存储 |
| 证书目录 | `/etc/ipma/certs/` | SSL 证书存储 |
| 服务文件 | `/usr/lib/systemd/system/ipma.service` | systemd 服务单元 |
| 初始化脚本 | `/usr/share/ipma/scripts/` | 辅助脚本 |
| Polkit 规则 | `/usr/share/polkit-1/rules.d/ipma.rules` | 权限规则 |

### 配置文件搜索路径

程序按以下优先级搜索配置文件：

1. `./config.toml` - 当前目录（开发环境）
2. `/etc/ipma/config.toml` - 系统配置目录（生产环境）
3. `/opt/ipma/config.toml` - 应用目录（备用）

### Web 资源搜索路径

程序按以下优先级搜索 Web 资源：

1. `./web` - 当前目录（开发环境）
2. `/opt/ipma/web` - 应用目录（生产环境）
3. `/usr/share/ipma/web` - 系统目录（备用）

---

## 配置文件 (config.toml)

```toml
# ============================================================
# IPMA 配置文件
# ============================================================

# ------------------------------------------------------------
# 数据库配置
# ------------------------------------------------------------
[database]
# 数据库名称
database = "ipma"

# 数据库主机地址
# - 本地: "localhost" 或 "127.0.0.1"
# - 远程: 数据库服务器的 IP 或域名
host = "localhost"

# 数据库端口 (PostgreSQL 默认: 5432)
port = 5432

# 数据库用户名
username = "postgres"

# 数据库密码
# 注意: 生产环境请使用强密码
password = "password"

# 最大数据库连接数
# 建议: CPU核心数 * 2 + 有效磁盘数
max_connections = 20

# 查询超时时间（秒）
query_timeout_secs = 30

# 慢查询阈值（毫秒）
# 超过此时间的查询会被记录
slow_query_threshold_ms = 1000

# ------------------------------------------------------------
# 服务器配置
# ------------------------------------------------------------
[server]
# IPv4 监听地址
# - 空字符串 "" 表示监听所有 IPv4 地址
# - 指定 IP 如 "192.168.1.100" 只监听该地址
host = ""

# IPv6 监听地址
# - "::" 表示监听所有 IPv6 地址
# - 留空则不监听 IPv6
host_ipv6 = "::"

# HTTP 服务配置
http_enabled = true      # 是否启用 HTTP
http_port = 80           # HTTP 端口

# HTTPS 服务配置
https_enabled = true     # 是否启用 HTTPS
https_port = 443         # HTTPS 端口

# 自动 HTTPS 重定向
# 启用后，HTTP 请求会自动重定向到 HTTPS
auto_https = true

# HTTP 版本
# - "http1": HTTP/1.1
# - "http2": HTTP/2
# - "http3": HTTP/3 (QUIC)
http_version = "http3"

# 证书类型
# - "self_signed": 自签名证书（自动生成）
# - "imported": 导入的证书
cert_type = "self_signed"

# 服务器公共 URL
# 用于生成重置密码链接、邮件通知等
public_url = "https://192.168.6.102"

# 会话超时时间（分钟）
# 用户无操作后自动登出
session_timeout = 60

# 页面超时时间（分钟）
# 页面空闲超时
page_timeout = 60

# ------------------------------------------------------------
# JWT 配置
# ------------------------------------------------------------
[jwt]
# JWT 密钥
# 生产环境请使用随机生成的强密钥
# 生成方法: openssl rand -base64 32
secret = "451#*jdhjhdfskdsHASDGJSAD256dgdfgs4dfs45werr2x"

# 访问令牌过期时间
# 格式: 数字 + 单位 (s=秒, m=分钟, h=小时, d=天)
access_token_expiry = "15m"

# 刷新令牌过期时间
refresh_token_expiry = "7d"

# ------------------------------------------------------------
# 初始化配置
# ------------------------------------------------------------
[init]
# 是否启用初始化模式
# - true: 仅初始化路由可用，用于首次安装
# - false: 正常运行模式
enabled = false

# ------------------------------------------------------------
# 国际化配置
# ------------------------------------------------------------
[i18n]
# 默认语言
# - "zh": 中文
# - "en": English
default_language = "zh"

# 支持的语言列表
supported_languages = ["zh", "en"]
```

---

## 服务管理

### systemd 服务命令

```bash
# 启动服务
sudo systemctl start ipma

# 停止服务
sudo systemctl stop ipma

# 重启服务
sudo systemctl restart ipma

# 查看服务状态
sudo systemctl status ipma

# 开机自启
sudo systemctl enable ipma

# 禁用开机自启
sudo systemctl disable ipma

# 查看服务日志
sudo journalctl -u ipma -f

# 查看最近 100 行日志
sudo journalctl -u ipma -n 100
```

### 服务文件内容

```ini
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

# 资源限制
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```

---

## 构建脚本 (build-deb.sh)

### 用途

构建 Debian 软件包 (.deb)，用于在 Debian/Ubuntu 系统上安装。

### 使用方法

```bash
# 进入项目目录
cd /root/ipma

# 执行构建脚本
./build-deb.sh
```

### 构建流程

1. 编译 release 版本
2. 清理旧的打包文件
3. 创建目录结构
4. 复制二进制文件
5. 复制资源文件 (web, templates)
6. 创建 polkit 规则
7. 创建 systemd 服务文件
8. 创建 DEBIAN 控制文件
9. 创建安装/卸载脚本
10. 创建文档和手册页
11. 构建 DEB 包

### 输出文件

构建完成后，在项目目录生成：
```
ipma_<版本号>_amd64.deb
```

### 安装 DEB 包

```bash
# 安装
sudo dpkg -i ipma_0.7.2_amd64.deb

# 如果有依赖问题，执行：
sudo apt-get install -f
```

### 版本号

版本号自动从 `Cargo.toml` 中的 `version` 字段读取。

---

## PostgreSQL 初始化脚本 (init-pgsql.sh)

### 用途

初始化 PostgreSQL 数据库，创建用户和数据库，并更新配置文件。

### 使用方法

```bash
# 基本用法（使用默认值）
sudo PG_PASSWORD=your_password /usr/share/ipma/scripts/init-pgsql.sh

# 自定义配置
sudo PG_USER=ipma \
     PG_PASSWORD=your_password \
     PG_DATABASE=ipma \
     PG_HOST=localhost \
     PG_PORT=5432 \
     /usr/share/ipma/scripts/init-pgsql.sh
```

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `PG_USER` | ipma | 数据库用户名 |
| `PG_PASSWORD` | (必需) | 数据库密码 |
| `PG_DATABASE` | ipma | 数据库名称 |
| `PG_HOST` | localhost | 数据库主机 |
| `PG_PORT` | 5432 | 数据库端口 |

### 执行内容

1. 检查 PostgreSQL 是否安装
2. 启动 PostgreSQL 服务
3. 创建数据库用户（带 CREATEDB 权限）
4. 创建数据库
5. 授予所有必要权限
6. 更新 `/etc/ipma/config.toml` 配置

### 完整安装流程

```bash
# 1. 安装 deb 包
sudo dpkg -i ipma_0.7.2_amd64.deb

# 2. 初始化 PostgreSQL
sudo PG_PASSWORD=secure_password /usr/share/ipma/scripts/init-pgsql.sh

# 3. 启动服务
sudo systemctl restart ipma

# 4. 查看状态
sudo systemctl status ipma

# 5. 查看日志
sudo journalctl -u ipma -f
```

---

## 权限说明

### ipma 用户

安装后自动创建 `ipma` 系统用户：

```bash
# 用户属性
用户名: ipma
类型: 系统用户
Shell: /usr/sbin/nologin (禁止登录)
主目录: /opt/ipma
```

### Polkit 权限

`ipma` 用户通过 polkit 规则获得以下权限：

- 重启 `ipma.service` 服务（无需密码）

规则文件位置：`/usr/share/polkit-1/rules.d/ipma.rules`

```javascript
polkit.addRule(function(action, subject) {
    if (action.id == "org.freedesktop.systemd1.manage-units" &&
        action.lookup("unit") == "ipma.service" &&
        action.lookup("verb") == "restart" &&
        subject.user == "ipma") {
        return polkit.Result.YES;
    }
});
```

### 目录权限

```bash
# 配置文件
/etc/ipma/config.toml    # root:ipma 640

# 日志目录
/var/log/ipma/           # ipma:ipma 755

# 应用目录
/opt/ipma/               # ipma:ipma 755

# 证书目录
/etc/ipma/certs/         # ipma:ipma 755
```

---

## 日志

### 日志位置

```
/var/log/ipma/ipma.log
```

### 日志级别

- `INFO`: 一般信息
- `WARN`: 警告
- `ERROR`: 错误
- `DEBUG`: 调试信息

### 查看日志

```bash
# 实时查看
tail -f /var/log/ipma/ipma.log

# 或通过 journalctl
journalctl -u ipma -f
```

---

## 故障排查

### 服务无法启动

```bash
# 检查服务状态
systemctl status ipma

# 查看详细日志
journalctl -u ipma -n 100

# 检查配置文件
cat /etc/ipma/config.toml

# 检查权限
ls -la /etc/ipma/
ls -la /var/log/ipma/
ls -la /opt/ipma/
```

### 数据库连接失败

```bash
# 检查 PostgreSQL 状态
systemctl status postgresql

# 测试数据库连接
PGPASSWORD=your_password psql -U ipma -h localhost -d ipma

# 检查配置
grep -A5 "\[database\]" /etc/ipma/config.toml
```

### 端口被占用

```bash
# 检查端口占用
sudo netstat -tlnp | grep -E ":(80|443)"

# 或使用 ss
sudo ss -tlnp | grep -E ":(80|443)"
```

### 权限问题

```bash
# 修复权限
sudo chown -R ipma:ipma /opt/ipma
sudo chown -R ipma:ipma /var/log/ipma
sudo chown -R ipma:ipma /etc/ipma/certs
sudo chown ipma:ipma /etc/ipma/config.toml
sudo chmod 640 /etc/ipma/config.toml
```
