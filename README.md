# IPMA (IP Management Application)

IPMA 是一个基于 Rust 和现代 Web 技术构建的高性能 IP 地址与网络资源管理系统。它旨在为网络管理员提供一个安全、高效且直观的平台，用于管理 IP 地址、交换机、物理资产（机房/机柜/工位）以及网络拓扑可视化。

## ✨ 核心特性

### 🚀 高性能后端
- **Rust & Actix-web**: 基于 Rust 语言开发，内存安全且性能卓越。
- **HTTP/3 (QUIC)**: 原生支持 HTTP/3、HTTP/2 和 HTTPS，提供极速的访问体验。
- **PostgreSQL**: 使用 SQLx 进行异步数据库交互，确保数据的一致性与高并发处理能力。

### 🛡️ 安全优先
- **全面加密**: 支持 TLS 1.3，集成自签名证书生成与证书导入功能。
- **多重认证**: 内置用户认证系统，支持 2FA (TOTP) 双因素认证。
- **审计日志**: 详尽的操作日志与登录日志，记录每一次关键变更。

### 🌐 资源与 IP 管理
- **IP 生命周期**: 支持 IPv4/IPv6 CIDR 管理，自动分配与回收 IP。
- **网络层级**: 灵活的网络区域 (Region) 与子网 (Subnet) 划分。
- **交换机集成**: 支持 SNMP (v1/v2c/v3) 协议，自动采集交换机信息、端口状态及 ARP 表。
- **物理资产**: 完整的机房、机柜、U位、工位管理模型。

### 📊 可视化与监控
- **交互式拓扑**: 基于 SVG 的工位与机柜可视化布局，支持拖通过拽调整与自动绘图。
- **仪表盘**: 实时统计网络利用率、设备分布与最近活动。
- **通知系统**: 内置站内通知与邮件通知（SMTP），及时告警 IP/MAC 变更。

### 🛠️ 系统管理
- **数据便携**: 支持 CSV 和数据库全量备份的导入导出。
- **服务管理**: 内置 Systemd 服务注册与管理功能。
- **国际化**: 前端支持多语言切换（目前支持中文与英文）。

## 🏗️ 技术栈

**后端 (Backend)**
- **语言**: Rust (Edition 2021)
- **Web 框架**: Actix-web 4
- **数据库**: PostgreSQL (via SQLx)
- **协议支持**: Rustls (TLS), Quinn (HTTP/3), async-snmp
- **工具**: Tokio, Serde, Tracing, Lettre (Email)

**前端 (Frontend)**
- **架构**: Vanilla JS (ES Modules) + HTML5 + CSS3
- **特性**: 无构建步骤 (No Build Step)，即改即用。
- **库**: i18next (国际化), Plotters (图表后端生成)

## 📦 安装与部署

### 前置要求
- **Rust**: 最新稳定版 (推荐通过 rustup 安装)
- **PostgreSQL**: 版本 12 或更高
- **OpenSSL**: 开发库 (libssl-dev)

### 开发环境运行

1.  **克隆仓库**
    ```bash
    git clone https://github.com/your-repo/ipma.git
    cd ipma
    ```

2.  **配置数据库**
    创建数据库并设置环境变量：
    ```bash
    # .env 文件
    DATABASE_URL=postgres://user:password@localhost/ipma_db
    ```

3.  **运行项目**
    ```bash
    cargo run
    ```
    访问 `https://localhost` (默认端口，会有自签名证书警告)。

### 生产环境构建

1.  **编译发布版本**
    ```bash
    cargo build --release
    ```

2.  **运行**
    ```bash
    ./target/release/ipma
    ```

3.  **注册为系统服务**
    在系统设置页面点击 "注册为服务" 或手动配置 Systemd。

## 🔧 配置说明

首次运行时，系统会生成默认配置文件 `config.toml`。

```toml
[server]
host = "0.0.0.0"
port = 8080
https_port = 8443
# 开启 HTTP/3 支持
enable_http3 = true

[database]
url = "postgres://..."

[security]
jwt_secret = "..."
# 自动重定向 HTTP 到 HTTPS
auto_https = true
```

## 📂 项目结构

```
ipma/
├── src/                # Rust 后端源码
│   ├── auth/           # 认证与用户模块
│   ├── resource/       # 核心资源逻辑 (IP, Network, Switch...)
│   ├── system/         # 系统配置与服务
│   ├── log/            # 日志系统
│   └── routes/         # API 路由定义
├── web/
│   └── static/         # 前端静态资源
│       ├── css/        # 样式文件
│       ├── js/         # ES Module 脚本
│       └── main.html   # 单页应用入口
└── Cargo.toml          # 项目依赖配置
```

## 📝 License

本项目采用 MIT 许可证。
