# FOIMS — Organization IT Information Management System

FOIMS（组织 IT 信息管理系统）是一个基于 Rust 和现代 Web 技术构建的高性能组织 IT 资产管理平台，为网络管理员提供安全、高效且直观的一站式管理界面：IP 地址、交换机、物理资产（机房/机柜/工位）、组织与人员、网络拓扑可视化等。

## ✨ 核心特性

### 🚀 高性能后端
- **Rust Workspace 多 crate 架构**: 主程序 + 10 个职责单一的子 crate，按领域拆分、边界清晰。
- **Axum 0.8 + UDS/h2c**: 后端监听 Unix Domain Socket 并以 h2c (HTTP/2 cleartext) 通信，nginx 反代多路复用，消除握手开销。
- **HTTP/1.1 / HTTP/2 / HTTP/3**: TLS 与静态资源由 nginx 终结，nginx ≥ 1.25.1 可选开启 QUIC (HTTP/3)。
- **PostgreSQL + SQLx**: 异步数据库访问，编译期校验的连接池配置与完备的建表校验。

### 🛡️ 安全优先
- **多层认证**: 本地账号 + **LDAP** + **OIDC 单点登录 (SSO)**，均支持 2FA (TOTP) 双因素认证。
- **双令牌机制**: 短效 Access Token + 长效 Refresh Token，前端自动静默续期。
- **防爆破**: 应用层 fail2ban（IP/用户/登录/邮箱多维度限流封禁），并提供 OS 层 fail2ban 配置样例。
- **密码策略**: 密码有效期、复杂度校验与过期强制修改。
- **审计与日志**: 操作日志、登录日志与通知全链路记录，支持双语输出。

### 🌐 资源与 IP 管理
- **IP 生命周期**: IPv4/IPv6 CIDR 管理，自动分配与回收。
- **网络层级**: 灵活的网络区域 (Region) 与子网 (Subnet) 划分。
- **交换机集成**: SNMP (v1/v2c/v3) 自动采集交换机信息、端口状态、MAC 表与 LLDP 邻居。
- **物理资产**: 完整的机房、机柜、U位、工位管理模型，设备与线缆链路 (Cable Link) 全量纳管。

### 👥 组织管理
- **组织树**: 多级组织架构维护、员工台账与组织模板。

### 📊 可视化与监控
- **交互式布局**: 基于 SVG 的机房/机柜/工位可视化布局，支持拖拽调整与自动绘图。
- **仪表盘**: 实时统计网络利用率、设备分布与最近活动。

### 🛠️ 系统管理
- **数据便携**: 按模块的 CSV 导入导出与数据库全量备份/恢复。
- **定时任务**: 内置 cron 调度（数据库备份、日志清理、MAC 同步、令牌清理等）。
- **通知系统**: 站内通知与邮件通知（SMTP），及时告警 IP/MAC 变更。
- **证书管理**: 自签名证书/CA 生成、导入、清点与下载，配合 nginx TLS 部署。
- **服务管理**: 网页端一键注册/重启 systemd 服务。
- **国际化**: 前后端均支持中英双语切换。

## 🏗️ 技术栈

**后端 (Backend)**
- **语言**: Rust (Edition 2024)，工作区拆分为 11 个成员 crate
- **Web 框架**: Axum 0.8 + Tower HTTP
- **数据库**: PostgreSQL (via SQLx)
- **认证**: JSON Web Token、bcrypt、TOTP、LDAP (ldap3)、OIDC (openidconnect)
- **协议采集**: async-snmp (SNMP v1/v2c/v3)
- **证书**: rcgen / x509-parser
- **工具**: Tokio, Serde, Tracing, Lettre (Email), rust-i18n

**前端 (Frontend)**
- **架构**: 原生 ES Modules + HTML5 + CSS3，无构建步骤，即改即用
- **模块化**: 按业务域拆分 JS 模块，模态页 (modals) 独立 HTML
- **国际化**: `web/static/i18n/` 下 zh/en 双语言包
- **质量工具链**: ESLint + Stylelint + Prettier + htmlhint + Jest + Lighthouse（仅用于开发期校验，见 [web/package.json](web/package.json)）

## 🧭 架构概览

```
浏览器 ──HTTP/1.1 · HTTP/2 · HTTP/3(TLS 由 nginx 终结)──► nginx
                                                        │  静态资源 (web/static/)
                                                        │  UDS + h2c 反代
                                                        ▼
                                              axum (Unix Domain Socket)
                                                        │
                                                        ▼
                                                   PostgreSQL
```

- 后端以 h2c 监听 UDS（默认 `/run/foims/api.sock`，调试为 `/tmp/foims-dev.sock`），不直接暴露 TCP 端口。
- TLS 证书、安全响应头、静态资源缓存均在 nginx 层完成，样例见 [deploy/nginx/](deploy/nginx/)。
- 也可将 `[server.listen].serve_static` 设为 `true` 由 axum 同时托管静态文件（不推荐生产使用）。

## 📦 安装与部署

### 前置要求
- **Rust**: 最新稳定版（推荐通过 rustup 安装）
- **PostgreSQL**: 版本 12 或更高
- **OpenSSL**: 开发库 (libssl-dev，邮件组件 native-tls 依赖)
- **nginx**: 版本 ≥ 1.25.1（h2c 上游代理；如需 HTTP/3 还需编译 QUIC 支持）
- **系统**: Linux（推荐 Debian 11+ 或 Ubuntu 24.04+）

### 1. 获取代码与配置

```bash
git clone https://gitee.com/oi-io0/foims.git
cd foims
cp config.toml.example config.toml   # 开发环境用当前目录；生产环境放 /etc/foims/config.toml
```

配置文件按优先级搜索：`/etc/foims/config.toml` → `/opt/foims/config.toml` → `./config.toml`，详见 [🔧 配置说明](#-配置说明)。

### 2. 准备数据库

使用初始化脚本（自动创建用户与数据库）：

```bash
PG_PASSWORD=your_password ./scripts/init-pgsql.sh
```

或手动执行：

```sql
CREATE USER foims WITH PASSWORD 'your_password';
CREATE DATABASE foims OWNER foims;
```

### 3. 构建与运行（开发调试）

```bash
cargo build --release
sudo ./target/release/foims          # UDS 绑定与属组设置需要 root
```

- 调试时后端默认监听 `/tmp/foims-dev.sock`，需配合 nginx 访问：
  将 [deploy/nginx/foims-dev.conf](deploy/nginx/foims-dev.conf) 安装到 nginx（HTTP only，修改 `web_dir` 路径），`nginx -t && systemctl reload nginx` 后访问 `http://localhost`。
- 前端为原生 ESM，无需构建；改完 `web/` 下文件刷新即可生效。

### 4. 初始化系统

- 首次访问会进入初始化向导页面，按提示完成建表并创建管理员账号
- 完成初始化后使用该账号登录，建议立即修改密码并在个人设置中启用 2FA

### 5. 生产环境部署

1. **编译发布版本**
   ```bash
   cargo build --release
   ```

2. **安装 nginx 生产配置**
   - 参考 [deploy/nginx/foims.conf](deploy/nginx/foims.conf)：配置 `server_name`、静态资源目录、TLS 证书路径
   - TLS 证书可使用 certbot 自动申请，或在本系统「系统设置 → 证书管理」中生成自签名证书并下载部署
   - 后端 UDS 路径需与 nginx upstream 一致（默认 `/run/foims/api.sock`）

3. **注册 systemd 服务**
   - 推荐在 Web 界面「系统设置」中点击注册服务（自动写入 `/etc/systemd/system/foims.service`）
   - 也可手动编写服务单元，示例：
     ```ini
     # /etc/systemd/system/foims.service
     [Unit]
     Description=FOIMS - Organization IT Information Management System
     After=network.target postgresql.service

     [Service]
     Type=simple
     User=root
     WorkingDirectory=/opt/foims
     ExecStart=/opt/foims/foims
     Restart=always

     [Install]
     WantedBy=multi-user.target
     ```

4. **启用并启动**
   ```bash
   systemctl enable --now foims
   ```

5. **（可选）启用 OS 层 fail2ban**
   - 样例见 [deploy/fail2ban/](deploy/fail2ban/)（filter 与 jail 配置）

## 🔧 配置说明

配置文件为 TOML 格式（搜索路径见上文），完整字段与注释见 [config.toml.example](config.toml.example)。

### 数据库配置

```toml
[database]
host = "localhost"
port = 5432
database = "foims"
username = "username"
password = "password"
max_connections = 20            # 以下连接池参数均有合理默认值，可按需覆盖
```

### 服务器配置

```toml
[server]
public_url = "localhost/"       # 用于拼接外部链接
page_timeout = 30               # 页面无操作超时（分钟），应大于 access_token_expiry
cors_allowed_origins = []       # 允许的 CORS 来源

[server.listen]
uds_path = "/tmp/foims-dev.sock" # UDS socket 路径（生产用 /run/foims/api.sock）
uds_group = "www-data"           # socket 属组，须与 nginx worker 属组一致
serve_static = false             # 是否由 axum 托管静态文件（推荐 false，由 nginx 托管）
```

### 认证配置 (JWT)

```toml
[jwt]
secret = "CHANGE_ME_TO_RANDOM_32_PLUS_CHARS"  # openssl rand -base64 48 生成
access_token_expiry = "15m"                   # 短效令牌
refresh_token_expiry = "7d"                   # 长效续期令牌
```

### 速率限制配置

```toml
[rate_limit]
enabled = true
ip_limit = 1000                 # 每 window_secs 单 IP 请求数
user_limit = 200                # 每 window_secs 单用户请求数
login_limit = 5                 # 每 window_secs 登录尝试数
window_secs = 60
email_limit = 5                 # 每 email_window_secs 邮件发送数
email_window_secs = 3600
```

### 国际化配置

```toml
[i18n]
log_language = "en"             # 控制台日志语言（zh / en）
supported_languages = ["zh", "en"]
```

### SNMP 采集配置

```toml
[snmp]
timeout_secs = 5
retries = 3
lldp_timeout_secs = 30
mac_scan_timeout_secs = 10
```

> SMTP 邮件、通知、密码策略等运行时设置在 Web 界面「系统设置」中配置，无需写入配置文件。

## 📂 项目结构

```
foims/
├── src/                          # 主程序（二进制 foims）
│   ├── routes/                   # API 路由与静态文件服务
│   ├── system/                   # 系统配置、证书管理、定时任务执行器
│   ├── log/                      # 操作/登录日志、通知、日志转发
│   ├── i18n/                     # 后端日志语言资源 (zh.yml / en.yml)
│   ├── utils/                    # 限流等中间件工具
│   ├── app_state.rs              # 全局应用状态
│   ├── shutdown.rs               # 优雅退出
│   └── main.rs                   # 入口：UDS 监听与启动流程
├── crates/                       # 工作区子 crate（按领域拆分）
│   ├── foims-common/             # 响应/错误/配置/加密/连接池/限流等共享设施
│   ├── foims-models/             # 全业务域请求/响应/行模型（唯一定义）
│   ├── foims-auth/               # 登录/JWT/2FA/LDAP/SSO/fail2ban/SMTP/操作日志
│   ├── foims-resource/           # 网络/机房/机柜/工位/设备/IP/线缆链路
│   ├── foims-organization/       # 组织树/员工/组织模板
│   ├── foims-visualization/      # 机房布局与拓扑计算
│   ├── foims-data-management/    # CSV 导入导出/数据库备份
│   ├── foims-scheduler/          # cron 调度基础设施
│   ├── foims-init/               # 建库建表/结构校验/备份恢复/初始化向导
│   └── foims-x509-management/    # X.509 证书管理（纯库）
├── web/                          # 前端（原生 ESM，无构建步骤）
│   └── static/
│       ├── js/                   # ES Module 脚本
│       │   ├── modules/          # 按业务域拆分的功能模块
│       │   ├── utils/            # 工具函数（apiClient、resourceLoader 等）
│       │   ├── app.js            # 主应用脚本
│       │   └── login.js          # 登录页脚本
│       ├── css/                  # 样式（base/components/layouts/modals/pages）
│       ├── i18n/                 # 前端语言包 (zh.json / en.json)
│       ├── modals/               # 各域模态页 HTML
│       ├── index.html            # 登录页
│       ├── init_index.html       # 初始化向导页
│       └── main.html             # 主应用页
├── deploy/                       # 部署样例
│   ├── nginx/                    # nginx 生产/调试配置
│   └── fail2ban/                 # OS 层 fail2ban 配置
├── scripts/                      # 运维脚本（init-pgsql.sh、SQL 增量脚本）
├── docs/                         # 文档（code-style.md、审计报告）
├── test/                         # API/构建 Shell 脚本
├── tests/                        # Rust 集成测试（前端一致性校验等）
├── .trae/ web/.trae/             # 前后端代码风格指南
├── Cargo.toml                    # 工作区与主 crate 定义
├── config.toml.example           # 配置样例
└── LICENSE / NOTICE              # GPL-3.0 许可证与第三方组件清单
```

## 🚀 快速开始

1. **系统初始化**: 首次访问进入初始化向导，完成建表与管理员账号创建
2. **登录系统**: 使用初始化的管理员账号登录，启用双因素认证
3. **配置网络**: 在「网络管理」中添加网络区域与子网，划分 IP 范围
4. **管理设备**: 在「交换机管理」中添加交换机并配置 SNMP，自动采集端口、MAC 表与 LLDP
5. **管理物理资产**: 录入机房、机柜、工位，纳管设备与线缆链路
6. **维护组织**: 维护组织树与员工台账，关联资产归属
7. **监控运维**: 查看仪表盘、配置通知与定时备份任务

## 📚 文档

- **代码风格规范**: [docs/code-style.md](docs/code-style.md)（前后端唯一权威来源）
- **协作与构建说明**: [AGENTS.md](AGENTS.md)
- **前端风格指南**: [web/.trae/rules/frontend-style-guide.md](web/.trae/rules/frontend-style-guide.md)
- **后端风格指南**: [.trae/rules/backend-style-guide.md](.trae/rules/backend-style-guide.md)
- **部署样例**: [deploy/](deploy/)（nginx、fail2ban）

## 🔍 常见问题

### Q: 如何解决自签名证书警告？
A: 推荐生产环境使用 certbot 申请有效证书；也可在「系统设置 → 证书管理」生成/导入证书后部署到 nginx（证书由 nginx 加载，更换后 `systemctl reload nginx`）。开发环境可将自签名证书加入浏览器信任列表。

### Q: 如何备份数据？
A: 通过「系统设置」的数据库备份功能，或使用 PostgreSQL 的 `pg_dump` 工具；也可配置定时任务自动备份。

### Q: 如何恢复数据？
A: 通过「系统设置」的备份恢复功能，或初始化向导中的备份恢复入口。

### Q: 如何配置邮件通知？
A: 在「系统设置」的 SMTP 配置中填写邮件服务器信息并测试连接。

### Q: 如何接入企业已有账号体系？
A: 在「系统设置」中配置 LDAP 服务器或 OIDC 单点登录 (SSO)；外部账号登录同样支持强制 2FA 策略。

### Q: 如何启用双因素认证？
A: 在个人设置页面启用，使用认证器应用扫描二维码并输入验证码。

## 🐛 故障排查

### 页面无法访问
- 确认 nginx 已启动且配置中的 `web_dir`、UDS 路径正确：`nginx -t`
- 检查 UDS socket 是否存在且属组与 nginx worker 一致（`uds_group`）
- 后端日志：`journalctl -u foims`（服务方式）或控制台输出（手动运行）

### 数据库连接失败
- 检查 PostgreSQL 服务是否运行
- 验证 `[database]` 连接信息是否正确
- 确保数据库用户有足够的权限

### SNMP 采集失败
- 验证交换机 SNMP 配置（版本、社区字符串/v3 用户凭据）是否正确
- 检查网络连通性与 `[snmp]` 超时设置

### 服务重启异常
- 检查 UDS 是否被残留占用（后端启动时会自动检测并清理陈旧 socket）
- 查看 `journalctl -u foims` 与 `systemctl status foims`

## 📝 License

本项目采用 [GPL-3.0-or-later 许可证](LICENSE)。

Copyright (c) 2025-2026 oi-io <boss@oi-io.cc>

本项目为自由软件，可依据自由软件基金会发布的 GNU GPL v3（或更新版本）
许可证重新发布或修改，详见 [LICENSE](LICENSE) 文件。

## 📦 Third-Party Components

本项目使用了以下开源组件，详细信息请参阅 [NOTICE](NOTICE) 文件，
许可证全文见 [third-party-licenses/](third-party-licenses/) 目录：

- **Axum** - Rust Web 框架 (MIT)
- **SQLx** - 异步 PostgreSQL 驱动 (Apache-2.0 OR MIT)
- **Tokio** - 异步运行时 (MIT)
- **ldap3 / openidconnect** - LDAP 与 OIDC 单点登录 (MIT OR Apache-2.0 / MIT)
- **async-snmp** - SNMP 客户端 (Apache-2.0 OR MIT)
- **rcgen / x509-parser** - X.509 证书生成与解析 (MIT OR Apache-2.0)

## 👤 Author

**oi-io** - [boss@oi-io.cc](mailto:boss@oi-io.cc)

## 🤝 贡献

欢迎提交 Issue 和 Pull Request 来帮助改进这个项目！提交前请阅读 [docs/code-style.md](docs/code-style.md) 并确保 `cargo fmt`、`cargo clippy --release -- -D warnings` 与前端 lint 通过。

## 📞 支持

如果您在使用过程中遇到问题，请通过以下方式寻求支持：
- 提交 Issue 到 Gitee 仓库
- 发送邮件到 boss@oi-io.cc

---

**FOIMS - 让组织 IT 管理更简单！**
