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
- **子网管理**: 灵活的网络区域 (Region) 与子网 (Subnet) 划分。
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
- **服务管理**: 网页端一键重启 systemd 服务。
- **国际化**: 前后端多语言支持，目前实现了中文与英文。

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
- **Rust**: 1.87 或更高（推荐通过 rust官方命令安装
- **PostgreSQL**: 版本 16 或更高
- **OpenSSL**: 开发库 (libssl-dev，邮件组件 native-tls 依赖)
- **nginx**: 版本 ≥ 1.28.1（h2c 上游代理；如需 HTTP/3 还需编译 QUIC 支持）
- **系统**: Linux（推荐 Debian 11+ 或 Ubuntu 24.04+）

### 1. 获取代码与配置

```bash
git clone https://github.com/o-i-i-o/foims.git
cd foims
cp config.toml.example config.toml   # 开发环境用当前目录；生产环境放 /etc/foims/config.toml
```

配置文件按优先级搜索：`/etc/foims/config.toml` → `/opt/foims/config.toml` → `./config.toml`，详见 [🔧 配置说明](#-配置说明)。

### 2. 准备数据库

保持 `config.toml` 中 `[init] enabled = true`，首次启动后访问初始化向导：
完成 PostgreSQL 检查后在「数据库配置」页填写连接信息，可点击「创建数据库」
自动建库（要求该用户已存在且具有 CREATEDB 权限），再通过「连接测试」进入
后续流程。

也可跳过页面建库，手动在 PostgreSQL 中预先创建（此后连接测试直接通过）：

```sql
CREATE USER foims WITH PASSWORD 'your_password' CREATEDB;
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

- 首次访问需要修改配置文件 `config.toml` 中的 `init.enabled` 为 `true`，会进入初始化向导页面，按提示完成建表并创建管理员账号
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
   - 通过安装deb包或手动将 `foims.service` 到 `/etc/systemd/system/foims.service`注册为服务运行


4. **启用并启动**
   ```bash
   systemctl enable --now foims
   ```

5. **（可选）启用 OS 层 fail2ban**
   - 样例见 [deploy/fail2ban/](deploy/fail2ban/)（filter 与 jail 配置）

## 🔧 配置说明

配置文件为 TOML 格式（搜索路径见上文），完整字段与注释见 [config.toml.example](config.toml.example)。

## 🚀 快速开始

1. **系统初始化**: 首次访问进入初始化向导，完成建表与管理员账号创建
2. **登录系统**: 使用初始化的管理员账号登录，启用双因素认证
3. **配置子网**: 在「资源管理」中添加网络区域与子网，划分 IP 范围
4. **管理设备**: 在「交换机管理」中添加交换机并配置 SNMP，自动采集端口、MAC 表与 LLDP
5. **管理物理资产**: 录入机房、机柜、工位，纳管设备与线缆链路
6. **维护组织**: 维护组织树与员工台账，关联资产归属
7. **监控运维**: 查看仪表盘、配置通知与定时备份任务

## 📚 文档

- **代码风格规范**: [docs/code-style.md](docs/code-style.md)（前后端唯一权威来源）
- **协作与构建说明**: [AGENTS.md](AGENTS.md)
- **部署样例**: [deploy/](deploy/)（nginx、fail2ban）

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
- 提交 Issue 到 Github 仓库
- QQ 群：1107983881
- 发送邮件到 boss@oi-io.cc
- 如果此程序对您有帮助，请考虑捐赠支持
---

**FOIMS - 让组织 IT 管理更简单！**
