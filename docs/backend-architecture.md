# IPMA 后端架构文档

## 1. 项目概述

**IPMA** (IP/MAC Address Management System) 是一个基于 **Rust 2024 Edition** 和 **Actix-web 4.13.0** 构建的 IP/MAC 地址管理系统，使用 **PostgreSQL** 作为数据库。项目版本为 `0.8.70`，采用 MIT 许可证。

### 1.1 核心技术栈

| 类别 | 技术/库 |
|------|---------|
| Web框架 | Actix-web 4.13.0 (支持 rustls-0_23) |
| 数据库 | PostgreSQL + SQLx 0.8.6 (异步ORM) |
| 认证 | JWT (jsonwebtoken) + bcrypt 密码哈希 + TOTP 双因素认证 |
| SNMP | async-snmp 0.12.0 |
| 定时任务 | tokio-cron-scheduler 0.15.1 |
| 邮件服务 | lettre 0.11.21 (SMTP) |
| 加密 | AES-256-GCM (aes-gcm) + SHA-256 |
| 配置管理 | config 0.15.22 (TOML格式) |
| 国际化 | rust-i18n 4.0.0 (支持中文/英文) |
| HTTP/3 | Quinn 0.11.9 (QUIC协议) |
| 日志 | tracing + tracing-subscriber |

---

## 2. 项目目录结构

```
/root/ipma/
├── Cargo.toml              # 项目配置
├── Cargo.lock              # 依赖锁定
├── config.toml             # 应用配置文件
├── src/
│   ├── main.rs             # 应用入口
│   ├── lib.rs              # 库根模块
│   ├── models.rs           # 数据模型定义
│   ├── config.rs           # 配置管理
│   ├── db.rs               # 数据库连接池
│   ├── error.rs            # 错误处理
│   ├── crypto.rs           # 加密工具
│   ├── auth/               # 认证模块
│   ├── resource/           # 资源管理模块
│   ├── routes/             # 路由定义
│   ├── system/             # 系统管理模块
│   ├── log/                # 日志模块
│   ├── utils/              # 工具模块
│   ├── init/               # 初始化模块
│   └── i18n/               # 国际化文件
├── web/                    # 前端静态资源
├── web-ts/                 # TypeScript前端源码
├── docs/                   # 文档
├── scripts/                # 脚本工具
└── test/                   # 测试脚本
```

---

## 3. 核心模块详解

### 3.1 入口与启动逻辑 ([main.rs](file:///root/ipma/src/main.rs))

**核心功能**:
- **TLS密码学初始化**: 使用 `rustls::crypto::ring::default_provider()` 初始化
- **日志系统**: 调用 `setup_logging()` 初始化双语日志
- **配置加载**: 从 TOML 文件和环境变量加载配置
- **数据库连接池**: 条件创建，支持初始化模式跳过
- **数据库迁移**: 自动运行 `run_migrations_only()`
- **Cron调度器**: 启动定时任务调度器
- **健康检查**: 每30秒检查数据库连接池状态
- **速率限制**: 创建并配置速率限制中间件
- **多服务器启动**:
  - HTTP服务器 (IPv4/IPv6双栈)
  - HTTPS服务器 (支持HTTP/1.1和HTTP/2)
  - HTTP/3服务器 (基于QUIC)
- **CORS配置**: 支持 localhost 开发环境
- **应用服务配置**: 区分初始化模式和正常模式

**关键代码**:
```rust
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 初始化 rustls 密码学提供者
    rustls::crypto::ring::default_provider().install_default()?;
    
    // 加载配置
    let config = Config::load().expect("Failed to load config");
    
    // 创建数据库连接池
    let pool = DbPool::new(&config.database).await?;
    
    // 启动cron调度器
    start_scheduler(Arc::new(pool.clone())).await?;
    
    // 启动HTTP/HTTPS/HTTP3服务器...
}
```

---

### 3.2 配置模块 ([config.rs](file:///root/ipma/src/config.rs))

**核心配置结构**:
- `DatabaseConfig`: 数据库连接配置 (host, port, database, username, password, max_connections, query_timeout, slow_query_threshold)
- `ServerConfig`: 服务器配置 (IPv4/IPv6地址、HTTP/HTTPS端口、auto_https、HTTP版本、证书类型、公共URL、会话/页面超时)
- `JwtConfig`: JWT配置 (secret, access_token_expiry, refresh_token_expiry)
- `InitConfig`: 初始化模式开关
- `I18nConfig`: 国际化配置 (默认语言、支持语言列表)
- `RateLimitConfig`: 速率限制配置 (IP限制、用户限制、登录限制、时间窗口)

**配置加载优先级**:
1. `/etc/ipma/config.toml` (生产环境)
2. `/opt/ipma/config.toml` (备用)
3. `./config.toml` (开发环境)
4. 环境变量 (IPMA_* 前缀)

---

### 3.3 数据库模块 ([db.rs](file:///root/ipma/src/db.rs))

**核心功能**:
- **连接池包装器** (`DbPool`): 封装 SQLx 的 `PgPool`
- **性能指标监控**: 活跃连接数、空闲连接数、等待请求数、总请求数、失败请求数、平均等待时间
- **连接获取监控**: 记录获取连接的等待时间和成功率
- **健康检查**: 定期执行 `SELECT 1` 检查连接
- **自动缩放**: 根据负载动态调整连接池大小 (高负载扩容、低负载缩容)
- **查询超时**: 支持带超时的查询执行
- **慢查询检测**: 超过阈值记录警告日志

---

### 3.4 认证模块 (`auth/`)

#### 3.4.1 认证中间件与登录 ([auth/login.rs](file:///root/ipma/src/auth/login.rs))

**核心功能**:
- **JWT认证中间件** (`auth_middleware`): 验证请求中的Token，支持设备指纹绑定
- **用户登录** (`login`): 用户名/密码验证，bcrypt密码比对
- **双因素认证** (`login_with_two_factor`): TOTP验证码验证
- **邮箱验证码登录** (`login_with_email_code`): SMTP发送验证码
- **Token刷新** (`refresh_token`): 刷新Access Token
- **密码重置** (`forgot_password`, `reset_password`): 通过邮箱重置密码
- **登出** (`logout`): 撤销Token并记录日志

#### 3.4.2 JWT工具 ([auth/utils.rs](file:///root/ipma/src/auth/utils.rs))

**核心功能**:
- **Token生成**: 支持Access Token和Refresh Token
- **Token验证**: 验证签名、过期时间、设备指纹
- **Token撤销**: 记录到数据库的 `revoked_tokens` 表
- **Token使用记录**: 防滥用检测 (1分钟30次/5分钟100次限制)

**JWT Claims结构**:
```rust
pub struct JwtClaims {
    pub sub: String,           // 用户ID
    pub username: String,
    pub role: String,
    pub exp: usize,            // 过期时间
    pub iat: usize,            // 签发时间
    pub iss: String,           // 签发者
    pub jti: String,           // Token唯一标识
    pub aud: String,           // 受众
    pub device_fingerprint: Option<String>,
    pub ip_address: Option<String>,
}
```

#### 3.4.3 用户管理 ([auth/user.rs](file:///root/ipma/src/auth/user.rs))

**核心功能**:
- 用户CRUD操作
- 密码加密存储 (bcrypt)
- 用户状态管理 (启用/禁用)
- 角色管理

---

### 3.5 资源管理模块 (`resource/`)

#### 3.5.1 网络管理 ([resource/network.rs](file:///root/ipma/src/resource/network.rs))

**核心功能**:
- **网络区域管理**: 创建、查询、更新、删除网络区域
- **网络管理**: 支持IPv4/IPv6双栈
  - CIDR格式验证
  - 网关配置
  - DNS服务器配置 (最多5个)
- **网络与房间关联**

#### 3.5.2 IP地址管理 ([resource/ip.rs](file:///root/ipma/src/resource/ip.rs))

**核心功能**:
- **IP地址CRUD**: 创建、查询、更新、删除IP记录
- **自动分配IP**: 从指定网络的CIDR中自动分配可用IP
- **批量创建**: 支持批量创建IP记录
- **MAC地址同步**: 从交换机SNMP获取MAC地址表并同步
- **MAC变更检测**: 检测MAC地址变化并发送通知 (站内通知+邮件)
- **IP归属查询**: 按工位、机位、交换机查询关联IP

#### 3.5.3 房间管理 ([resource/room.rs](file:///root/ipma/src/resource/room.rs))

**核心功能**:
- 房间CRUD (支持 office/data_center 类型)
- 房间与网络关联
- 房间统计信息 (工位数量)

#### 3.5.4 机柜管理 ([resource/cabinets.rs](file:///root/ipma/src/resource/cabinets.rs))

**核心功能**:
- 机柜CRUD
- 机柜容量管理 (1-48U)
- 机柜与网络关联
- 机柜统计信息 (机位数量)

#### 3.5.5 机位管理 ([resource/cabinet.rs](file:///root/ipma/src/resource/cabinet.rs))

**核心功能**:
- 机位CRUD (U位范围 1-48)
- 机位与交换机端口关联
- 机位IP管理

#### 3.5.6 工位管理 ([resource/workstation.rs](file:///root/ipma/src/resource/workstation.rs))

**核心功能**:
- 工位CRUD
- 工位与交换机端口关联
- 工位IP管理

#### 3.5.7 交换机管理 (`resource/switch/`)

**文件列表**:
- `mod.rs`: 模块导出
- `device.rs`: 交换机设备CRUD
- `port.rs`: 交换机端口管理
- `snmp.rs`: SNMP连接和测试
- `mac.rs`: MAC地址表获取
- `lldp.rs`: LLDP邻居发现

**核心功能**:
- **交换机CRUD**: 支持SNMP v1/v2c/v3配置
- **SNMP连接测试**: 验证SNMP连通性
- **端口同步**: 从交换机SNMP同步端口信息
- **MAC地址表**: 获取交换机MAC地址表并存储到数据库
- **LLDP邻居**: 发现交换机LLDP邻居信息
- **SNMP v3支持**: 认证协议、隐私协议配置

---

### 3.6 路由定义 ([routes/mod.rs](file:///root/ipma/src/routes/mod.rs))

**路由结构** (完整API列表):

| 路由前缀 | 功能模块 | 主要端点 |
|---------|---------|---------|
| `/api/auth` | 认证 | login, logout, refresh, forgot-password, reset-password, me |
| `/api/auth/login/*` | 多因素认证 | email, two-factor, send-code, send-2fa-code |
| `/health` | 健康检查 | status |
| `/api/users` | 用户管理 | CRUD |
| `/api/two-factor` | 2FA管理 | init, enable, disable |
| `/api/resources/networks` | 网络管理 | CRUD |
| `/api/resources/network-regions` | 网络区域 | CRUD |
| `/api/resources/rooms` | 房间管理 | CRUD |
| `/api/resources/cabinets` | 机柜管理 | CRUD |
| `/api/resources/workstations` | 工位管理 | CRUD |
| `/api/resources/positions` | 机位管理 | CRUD |
| `/api/resources/ip` | IP管理 | CRUD, auto-assign, batch, pull |
| `/api/resources/layouts` | 布局管理 | save, get, delete |
| `/api/switches` | 交换机管理 | CRUD, ports, macs, lldp, snmp |
| `/api/logs` | 日志查询 | operation, login |
| `/api/notifications` | 通知管理 | list, mark-read |
| `/api/system` | 系统管理 | info, dashboard, restart, config |
| `/api/system/smtp` | SMTP配置 | config, test, send |
| `/api/system/certificate` | 证书管理 | status, generate, import, download |
| `/api/system/import-export` | 数据导入导出 | import/csv, export/csv, export/database |
| `/api/system/scheduled-tasks` | 定时任务 | CRUD, toggle, run, logs |

---

### 3.7 系统管理模块 (`system/`)

#### 3.7.1 系统配置 ([system/config.rs](file:///root/ipma/src/system/config.rs))

**核心功能**:
- **系统信息**: 获取版本、运行时间、数据库状态
- **仪表盘统计**: 用户、网络、房间、机柜、交换机、IP等统计数据
- **系统配置更新**: 更新并保存TOML配置
- **服务管理**: 注册systemd服务、检查服务状态、重启服务
- **SMTP配置**: 从数据库获取/保存SMTP配置
- **证书管理**: 生成自签名证书、导入证书、下载证书
- **语言设置**: 支持中文/英文切换
- **会话/页面超时**: 配置超时时间
- **通知设置**: 邮件收件人配置

#### 3.7.2 证书管理 ([system/cert.rs](file:///root/ipma/src/system/cert.rs))

**核心功能**:
- **自签名证书生成**: 使用 rcgen 生成 ECDSA P-256 证书
- **证书导入**: 支持上传证书和私钥文件
- **证书选择**: 自动选择最新导入或自签名证书
- **TLS配置加载**: 加载 rustls ServerConfig (支持ALPN h2/http1.1)

#### 3.7.3 定时任务 ([system/cron.rs](file:///root/ipma/src/system/cron.rs))

**核心功能**:
- **数据备份**: 每日自动执行 pg_dump 备份数据库
- **Token清理**: 每小时清理过期撤销Token
- **使用记录清理**: 每日清理30天前的Token使用记录
- **用户任务同步**: 每5分钟同步数据库中的定时任务
- **Cron表达式解析**: 支持标准cron格式
- **任务执行**: MAC同步、Token清理、备份、日志清理

#### 3.7.4 定时任务管理 ([system/scheduled_task.rs](file:///root/ipma/src/system/scheduled_task.rs))

**核心功能**:
- 定时任务CRUD
- 任务状态切换 (启用/禁用)
- 手动执行任务
- 任务执行日志查询

#### 3.7.5 SMTP服务 ([system/smtp.rs](file:///root/ipma/src/system/smtp.rs))

**核心功能**:
- **SMTP配置管理**: 从数据库存取配置 (密码AES加密)
- **连接测试**: 发送测试邮件
- **邮件发送**: 批量发送邮件给指定用户
- **MAC变更通知**: 自动发送MAC地址变更邮件

#### 3.7.6 数据导入导出 ([system/data.rs](file:///root/ipma/src/system/data.rs))

**核心功能**:
- **CSV导出**: 支持网络区域、网络、房间、工位、机柜、机位、交换机、IP的CSV导出
- **CSV导入**: 支持批量导入 (跳过/覆盖模式)
- **ZIP打包**: 导出文件自动打包为ZIP
- **数据库导出**: 使用 pg_dump 导出完整数据库
- **日志清理**: 按类型和天数清理日志
- **模板下载**: 提供导入模板

#### 3.7.7 HTTP/3支持 ([system/http3.rs](file:///root/ipma/src/system/http3.rs))

**核心功能**:
- **QUIC端点**: 使用 Quinn 创建HTTP/3服务器
- **请求处理**: 支持健康检查、用户信息、静态文件、API路由
- **JWT认证**: 在HTTP/3中验证Token
- **缓冲区池**: 使用对象池优化内存分配
- **并发控制**: 信号量限制并发连接数

---

### 3.8 日志模块 (`log/`)

#### 3.8.1 日志配置 ([log/mod.rs](file:///root/ipma/src/log/mod.rs))

**核心功能**:
- **日志初始化**: 使用 tracing-subscriber
- **双输出**: 同时输出到stdout和文件
- **日志文件**: 按时间命名存储在 `/var/log/ipma/`
- **日志级别**: ipma=INFO, actix_web=WARN, 其他=WARN

#### 3.8.2 操作日志 ([log/operation.rs](file:///root/ipma/src/log/operation.rs))

**核心功能**:
- 查询操作日志 (支持资源类型、资源ID、用户ID、操作类型过滤)
- 分页查询
- 关联用户表获取用户名

#### 3.8.3 登录日志 ([log/login.rs](file:///root/ipma/src/log/login.rs))

**核心功能**:
- 查询登录日志
- 支持搜索 (用户名、IP地址、User-Agent)
- 分页查询

#### 3.8.4 通知 ([log/notification.rs](file:///root/ipma/src/log/notification.rs))

**核心功能**:
- 获取通知列表 (支持已读/未读筛选)
- 标记通知为已读
- 创建通知 (站内通知)

---

### 3.9 工具模块 (`utils/`)

#### 3.9.1 通用工具 ([utils/common.rs](file:///root/ipma/src/utils/common.rs))

**核心功能**:
- **IP/MAC验证**: 使用 ipnetwork 和 macaddr crate
- **CIDR验证**: IPv4/IPv6 CIDR格式正则验证
- **Token管理**: SHA-256哈希、撤销、使用记录、清理
- **操作日志**: 记录用户操作到数据库
- **MAC地址获取**: 从ARP缓存读取、批量ping探测
- **双语日志**: 同时输出中英文日志
- **网络查询**: 预定义SQL查询和结果解析

#### 3.9.2 分页 ([utils/pagination.rs](file:///root/ipma/src/utils/pagination.rs))

**核心功能**:
- 分页参数解析
- 分页结果封装
- 总页数计算

#### 3.9.3 速率限制 ([utils/rate_limit.rs](file:///root/ipma/src/utils/rate_limit.rs))

**核心功能**:
- **IP级别限制**: 默认100请求/60秒
- **用户级别限制**: 默认200请求/60秒
- **登录限制**: 默认5请求/60秒
- **滑动窗口**: 基于时间窗口的计数
- **自动清理**: 每分钟清理过期记录
- **Actix中间件**: 集成到请求处理流程

#### 3.9.4 缓冲区池 ([utils/buffer_pool.rs](file:///root/ipma/src/utils/buffer_pool.rs))

**核心功能**:
- 对象池模式复用 Vec<u8> 缓冲区
- 减少HTTP/3处理中的内存分配

---

### 3.10 初始化模块 (`init/`)

**核心功能**:
- **系统初始化**: 首次安装时的引导流程
- **数据库创建**: 创建数据库和表结构
- **数据导入**: 从SQL文件导入数据
- **配置检查**: 验证数据库连接和表结构
- **验证码**: 初始化操作的安全验证
- **重启程序**: 初始化完成后重启服务

---

### 3.11 数据模型 ([models.rs](file:///root/ipma/src/models.rs))

**核心模型** (共定义了30+个结构体):

| 类别 | 模型 |
|------|------|
| 响应模型 | `ApiResponse<T>`, `PaginatedResponse<T>` |
| 布局 | `Position`, `LayoutSaveRequest`, `LayoutItem` |
| 用户 | `User`, `UserCreate`, `UserUpdate`, `UserLogin` |
| 2FA | `TwoFactorConfigResponse`, `TwoFactorLoginRequest` |
| 网络 | `NetworkRegion`, `Network`, `NetworkCreate`, `NetworkUpdate` |
| 房间 | `Room`, `RoomCreate`, `RoomUpdate`, `RoomWithNetworks` |
| 机柜 | `Cabinet`, `CabinetWithNetworks`, `CabinetCreate` |
| 机位 | `CabinetPosition`, `CabinetPositionWithDetails`, `CabinetPositionCreate` |
| 工位 | `Workstation`, `WorkstationWithDetails`, `WorkstationCreate` |
| IP管理 | `IpManager`, `IpManagerWithNames`, `IpManagerCreate`, `AutoAssignIpRequest` |
| 交换机 | `Switch`, `SwitchWithParent`, `SwitchCreate`, `SwitchUpdate` |
| 端口 | `SwitchPort`, `SwitchPortWithSwitch`, `SwitchPortCreate` |
| SNMP | `SnmpTestRequest`, `ArpEntry`, `LldpNeighbor`, `SwitchMac`, `SwitchLldp` |
| 日志 | `OperationLog`, `LoginLog`, `TaskLog` |
| 定时任务 | `ScheduledTask`, `ScheduledTaskCreate`, `ScheduledTaskUpdate` |
| 通知 | `Notification` |

---

### 3.12 错误处理 ([error.rs](file:///root/ipma/src/error.rs))

**核心功能**:
- **自定义错误类型** (`AppError`): Database, NotFound, Validation, Unauthorized, Forbidden, Conflict, Internal, Snmp
- **HTTP状态码映射**: 每个错误类型对应正确的HTTP状态码
- **SQLx错误转换**: 自动识别重复键、外键约束、连接超时等
- **验证错误转换**: validator::ValidationErrors 自动转换

---

### 3.13 加密模块 ([crypto.rs](file:///root/ipma/src/crypto.rs))

**核心功能**:
- **AES-256-GCM加密**: 用于SNMP密码等敏感数据加密
- **密钥管理**: 自动生成和保存32字节密钥到 `/etc/ipma/encryption.key`
- **Base64编码**: 密文使用Base64编码存储

---

## 4. 已实现功能模块总结

### 4.1 认证与授权
- [x] 用户名/密码登录 (bcrypt验证)
- [x] JWT Token管理 (Access/Refresh Token)
- [x] 双因素认证 (TOTP，基于RFC 4226)
- [x] 邮箱验证码登录
- [x] 密码重置 (通过邮箱)
- [x] 设备指纹绑定
- [x] Token撤销和使用追踪
- [x] 用户角色管理

### 4.2 网络资源管理
- [x] 网络区域管理
- [x] IPv4/IPv6双栈网络管理 (CIDR支持)
- [x] 网关和DNS配置
- [x] 房间管理 (办公室/数据中心)
- [x] 机柜管理 (U位容量)
- [x] 机位管理 (U位范围)
- [x] 工位管理
- [x] 网络与资源关联

### 4.3 IP地址管理
- [x] IP地址CRUD
- [x] 自动IP分配 (从CIDR)
- [x] 批量创建IP
- [x] MAC地址管理
- [x] MAC变更检测和通知
- [x] IP与工位/机位/交换机关联

### 4.4 交换机管理
- [x] 交换机CRUD
- [x] SNMP v1/v2c/v3支持
- [x] SNMP连接测试
- [x] 端口信息同步
- [x] MAC地址表获取
- [x] LLDP邻居发现
- [x] 交换机与机柜U位关联

### 4.5 系统管理
- [x] 系统信息和仪表盘统计
- [x] 配置管理 (TOML文件)
- [x] 服务注册 (systemd)
- [x] 应用重启/操作系统重启
- [x] SMTP邮件服务配置
- [x] 证书管理 (自签名/导入)
- [x] 语言切换 (中文/英文)
- [x] 会话/页面超时配置

### 4.6 数据导入导出
- [x] CSV导出 (ZIP打包)
- [x] CSV导入 (支持跳过/覆盖模式)
- [x] 数据库导出 (pg_dump)
- [x] 导入模板下载
- [x] 日志清理

### 4.7 定时任务
- [x] Cron表达式定时任务
- [x] 数据库自动备份
- [x] Token自动清理
- [x] 日志自动清理
- [x] MAC同步任务
- [x] 任务执行日志

### 4.8 日志与监控
- [x] 操作日志记录
- [x] 登录日志记录
- [x] 通知系统 (站内通知)
- [x] 速率限制 (IP/用户/登录)
- [x] 数据库连接池监控

### 4.9 网络协议支持
- [x] HTTP/1.1
- [x] HTTP/2 (通过TLS ALPN)
- [x] HTTP/3 (QUIC)
- [x] IPv4/IPv6双栈
- [x] HTTPS (TLS 1.3)
- [x] CORS支持

### 4.10 安全特性
- [x] JWT认证
- [x] 密码加密 (bcrypt)
- [x] 敏感数据加密 (AES-256-GCM)
- [x] 速率限制
- [x] Token撤销
- [x] 设备指纹验证
- [x] HSTS中间件

---

## 5. 整体架构说明

### 5.1 架构模式

项目采用 **分层架构** 设计:

```
┌─────────────────────────────────────┐
│           前端 (web/web-ts)          │
│    (静态HTML + JavaScript/TypeScript) │
└─────────────────────────────────────┘
                  ↓
┌─────────────────────────────────────┐
│         Actix-web HTTP服务器         │
│  (HTTP/1.1 + HTTP/2 + HTTP/3/QUIC)  │
└─────────────────────────────────────┘
                  ↓
┌─────────────────────────────────────┐
│           中间件层                   │
│  (CORS + Logger + RateLimit + HSTS) │
└─────────────────────────────────────┘
                  ↓
┌─────────────────────────────────────┐
│           路由层 (routes)            │
│    (URL路由分发 → 处理器函数)         │
└─────────────────────────────────────┘
                  ↓
┌─────────────────────────────────────┐
│           业务逻辑层                 │
│  (auth + resource + system + log)   │
└─────────────────────────────────────┘
                  ↓
┌─────────────────────────────────────┐
│           数据访问层                 │
│      (SQLx + PostgreSQL)            │
└─────────────────────────────────────┘
```

### 5.2 请求处理流程

1. **请求接收**: Actix-web 接收 HTTP/HTTPS/HTTP3 请求
2. **中间件处理**: CORS → Logger → RateLimit → HSTS (HTTPS)
3. **路由匹配**: 根据URL路径匹配到对应处理器
4. **认证检查**: `auth_middleware` 验证JWT Token (除公开路由)
5. **参数验证**: 使用 `validator` crate 验证请求数据
6. **业务处理**: 执行CRUD操作或业务逻辑
7. **数据库操作**: 通过 `DbPool` 执行SQL查询
8. **响应返回**: 返回 `ApiResponse<T>` JSON响应

### 5.3 关键设计特点

1. **异步全栈**: 所有IO操作均为异步 (async/await)
2. **类型安全**: 大量使用Rust类型系统防止运行时错误
3. **错误处理**: 统一的 `AppError` 类型和 `IntoResponse` trait
4. **配置驱动**: 支持配置文件和环境变量双重配置
5. **IPv6友好**: 所有网络相关代码均支持IPv6
6. **安全优先**: 密码加密、Token管理、速率限制、HSTS
7. **可观测性**: 详细的日志记录、性能指标、健康检查
8. **国际化**: 支持中英文双语界面和日志

---

## 6. 总结

IPMA项目是一个功能完整、架构清晰的IP/MAC地址管理系统。它充分利用了Rust的语言特性（内存安全、零成本抽象、强大的类型系统）和Actix-web的高性能特性，构建了一个企业级的网络资源管理平台。

项目的主要亮点包括:
- **完整的网络资源管理**: 从网络区域到具体IP地址的全链路管理
- **强大的交换机集成**: SNMP协议支持，自动发现网络拓扑
- **多因素认证**: JWT + TOTP + 邮箱验证码，确保安全性
- **现代Web协议**: 同时支持HTTP/1.1、HTTP/2和HTTP/3
- **自动化运维**: 定时备份、自动清理、服务管理
- **数据可移植性**: CSV导入导出、数据库备份恢复

项目代码质量高，遵循Rust最佳实践，具有良好的可维护性和扩展性。
