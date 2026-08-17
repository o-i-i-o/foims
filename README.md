# IPMA (IP Management Application)

IPMA 是一个基于 Rust 和现代 Web 技术构建的高性能 IP 地址与网络资源管理系统。它旨在为网络管理员提供一个安全、高效且直观的平台，用于管理 IP 地址、交换机、物理资产（机房/机柜/工位）以及网络拓扑可视化。

## ✨ 核心特性

### 🚀 高性能后端
- **Rust & Axum**: 基于 Rust 语言开发，内存安全且性能卓越。
- **HTTP/3 (QUIC)**: 通过nginx代理实现 HTTP/3、HTTP/2 和 HTTPS，提供极速的访问体验。
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
- **语言**: Rust (Edition 2024)
- **Web 框架**: Axum 0.8
- **数据库**: PostgreSQL (via SQLx)
- **协议支持**: Rustls (TLS), async-snmp (SNMP 监测)
- **工具**: Tokio, Serde, Tracing, Lettre (Email)

**前端 (Frontend)**
- **架构**: Vanilla JS (ES Modules) + HTML5 + CSS3
- **特性**: 无构建步骤 (No Build Step)，即改即用。
- **库**: rust-i18n (国际化)，自研 SVG/Canvas 图表

## 📦 安装与部署

### 前置要求
- **Rust**: 最新稳定版 (推荐通过 rustup 安装)
- **PostgreSQL**: 版本 12 或更高
- **OpenSSL**: 开发库 (libssl-dev)
- **系统**: Linux (推荐 Ubuntu 24.04+ 或 Debian 11+)

### 开发环境运行

1.  **克隆仓库**
    ```bash
    git clone https://gitee.com/oi-io0/ipma.git
    cd ipma
    ```

2.  **配置数据库**
    - 创建 PostgreSQL 数据库
    - 编辑 `config.toml` 文件，配置数据库连接信息
    ```toml
    [database]
    url = "postgres://username:password@localhost:5432/ipma"
    ```

3.  **运行项目**
    ```bash
    cargo run
    ```
    访问 `https://localhost` (默认端口，会有自签名证书警告)。

4.  **初始化系统**
    - 首次访问会进入初始化页面
    - 设置管理员账号和数据库连接
    - 完成初始化后即可登录系统

### 生产环境构建

#### 方法一：使用构建脚本

1.  **运行构建脚本**
    ```bash
    ./build-deb.sh
    ```

2.  **安装生成的 Debian 包**
    ```bash
    apt reinstall ./ipma_0.7.2_amd64.deb
    ```

3.  **启动服务**
    ```bash
    systemctl restart ipma
    ```

#### 方法二：手动构建

1.  **编译发布版本**
    ```bash
    cargo build --release
    ```

2.  **运行**
    ```bash
    ./target/release/ipma
    ```

3.  **注册为系统服务**
    - 在系统设置页面点击 "注册为服务" 或手动配置 Systemd
    - 手动配置示例：
    ```bash
    # /etc/systemd/system/ipma.service
    [Unit]
    Description=IP Management Application
    After=network.target postgresql.service
    
    [Service]
    Type=simple
    User=root
    WorkingDirectory=/opt/ipma
    ExecStart=/opt/ipma/ipma
    Restart=always
    
    [Install]
    WantedBy=multi-user.target
    ```

4.  **启用并启动服务**
    ```bash
    systemctl enable ipma
    systemctl start ipma
    ```

## 🔧 配置说明

配置保存在 `config.toml`。

### 服务器配置

```toml
[server]
# 服务器主机地址
host = "0.0.0.0"
# IPv6 地址
host_ipv6 = "::"
# HTTP 端口
http_port = 80
# HTTP 版本 (HTTP/1.1, HTTP/2, HTTP/3)
http_version = "HTTP/3"
# 是否启用 HTTP
http_enabled = true
# 是否自动重定向 HTTP 到 HTTPS
auto_https = true
# 证书类型 (self_signed, imported)
cert_type = "self_signed"
```

### 数据库配置

```toml
[database]
# 数据库连接 URL
url = "postgres://username:password@localhost:5432/ipma"
```

### 安全配置

```toml
[security]
# JWT 密钥
jwt_secret = "your-secret-key"
# 会话超时时间（分钟）
session_timeout = 120
# 页面超时时间（分钟）
page_timeout = 30
```

### 速率限制配置

```toml
[rate_limit]
# 是否启用速率限制
enabled = true
# IP 限制（每窗口秒数）
ip_limit = 100
# 用户限制（每窗口秒数）
user_limit = 60
# 登录限制（每窗口秒数）
login_limit = 10
# 窗口大小（秒）
window_secs = 60
```

### 初始化配置

```toml
[init]
# 是否启用初始化模式
enabled = false
```

### SMTP 配置

```toml
[smtp]
host = "smtp.example.com"
port = 587
username = "admin@example.com"
password = "your-password"
from = "admin@example.com"
tls = true
```

## 📂 项目结构

```
ipma/
├── src/                # Rust 后端源码
│   ├── auth/           # 认证与用户模块
│   │   ├── login.rs    # 登录相关逻辑
│   │   └── user.rs     # 用户管理逻辑
│   ├── resource/       # 核心资源逻辑
│   │   ├── network.rs  # 网络管理
│   │   ├── switch.rs   # 交换机管理
│   │   └── mod.rs      # 资源模块入口
│   ├── system/         # 系统配置与服务
│   │   ├── config.rs   # 配置管理
│   │   ├── init.rs     # 系统初始化
│   │   └── data.rs     # 数据导入导出
│   ├── log/            # 日志系统
│   │   ├── operation.rs # 操作日志
│   │   └── notification.rs # 通知系统
│   ├── utils/          # 工具函数
│   ├── routes/         # API 路由定义
│   ├── db.rs           # 数据库连接
│   ├── models.rs       # 数据模型
│   ├── config.rs       # 配置加载
│   └── main.rs         # 应用入口
├── web/                # 前端代码
│   └── static/         # 前端静态资源
│       ├── css/        # 样式文件
│       ├── js/         # ES Module 脚本
│       │   ├── modules/    # 功能模块
│       │   ├── utils/      # 工具函数
│       │   ├── lib/        # 第三方库
│       │   ├── login.js    # 登录页面脚本
│       │   └── app.js      # 主应用脚本
│       ├── locales/     # 国际化文件
│       │   ├── zh-CN/      # 中文
│       │   └── en/         # 英文
│       ├── index.html   # 登录页面
│       ├── init_index.html # 初始化页面
│       └── main.html   # 主页面
├── docs/               # 文档
│   └── api.md          # API 文档
├── .trae/              # 项目规则和配置
│   └── rules/          # 代码风格规则
├── Cargo.toml          # 项目依赖配置
├── config.toml         # 应用配置文件
└── build-deb.sh        # Debian 包构建脚本
```

## 🚀 快速开始

### 1. 系统初始化
- 首次访问系统会进入初始化页面
- 设置管理员账号信息
- 配置数据库连接
- 点击 "初始化系统" 完成设置

### 2. 登录系统
- 使用设置的管理员账号登录
- 首次登录建议修改密码
- 可在个人设置中启用双因素认证

### 3. 配置网络
- 进入 "网络管理" 页面
- 添加网络区域和网络
- 配置 IP 地址范围

### 4. 管理设备
- 进入 "交换机管理" 页面
- 添加交换机并配置 SNMP 信息
- 自动采集交换机端口和 MAC 表

### 5. 管理物理资产
- 进入 "资源管理" 页面
- 添加机房、机柜、工位
- 分配 IP 地址到设备

### 6. 监控与维护
- 查看仪表盘了解系统状态
- 配置通知设置
- 定期备份数据

## 📚 文档

- **API 文档**: [docs/api.md](docs/api.md) - 详细的 API 接口说明
- **前端风格指南**: [web/.trae/rules/frontend-style-guide.md](web/.trae/rules/frontend-style-guide.md) - 前端代码风格规范
- **后端风格指南**: [.trae/rules/backend-style-guide.md](.trae/rules/backend-style-guide.md) - 后端代码风格规范

## 🔍 常见问题

### Q: 如何解决自签名证书警告？
A: 在生产环境中，建议导入有效的 SSL 证书。在开发环境中，可以将自签名证书添加到浏览器的信任列表中。

### Q: 如何备份数据？
A: 可以通过系统设置页面的 "导出数据库" 功能，或使用 PostgreSQL 的 pg_dump 工具进行备份。

### Q: 如何恢复数据？
A: 通过系统设置页面的 "导入数据库" 功能，上传备份文件进行恢复。

### Q: 如何配置邮件通知？
A: 在系统设置页面的 "SMTP 配置" 部分，填写邮件服务器信息并测试连接。

### Q: 如何启用双因素认证？
A: 在个人设置页面，点击 "启用双因素认证"，使用认证器应用扫描二维码并输入验证码。

## 🐛 故障排查

### 数据库连接失败
- 检查 PostgreSQL 服务是否运行
- 验证数据库连接 URL 是否正确
- 确保数据库用户有足够的权限

### 服务启动失败
- 检查系统日志：`journalctl -u ipma`
- 验证端口是否被占用
- 检查配置文件是否正确

### SNMP 采集失败
- 验证交换机 SNMP 配置是否正确
- 检查网络连接是否正常
- 确保 SNMP 版本和社区字符串匹配

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
- **Rustls** - TLS 实现 (Apache-2.0 OR MIT)
- **async-snmp** - SNMP 客户端 (Apache-2.0 OR MIT)

## 👤 Author

**oi-io** - [boss@oi-io.cc](mailto:boss@oi-io.cc)

## 🤝 贡献

欢迎提交 Issue 和 Pull Request 来帮助改进这个项目！

## 📞 支持

如果您在使用过程中遇到问题，请通过以下方式寻求支持：
- 提交 Issue 到 GitHub/Gitee 仓库
- 发送邮件到 boss@oi-io.cc

---

**IPMA - 让 IP 管理更简单！**