# Actix-web 实现备份文档

> 本文档记录 IPMA 项目在 actix-web 4.14.0 上的实现细节，作为 axum 重构前的参考基线。
> 创建日期：2026-08-07
> 项目版本：0.14.1

## 目录

1. [依赖清单](#1-依赖清单)
2. [服务器启动与生命周期](#2-服务器启动与生命周期)
3. [应用配置与中间件栈](#3-应用配置与中间件栈)
4. [路由组织](#4-路由组织)
5. [自定义 Extractor](#5-自定义-extractor)
6. [中间件实现](#6-中间件实现)
7. [错误处理体系](#7-错误处理体系)
8. [静态文件服务](#8-静态文件服务)
9. [子 Crate 耦合点](#9-子-crate-耦合点)
10. [Multipart 文件上传](#10-multipart-文件上传)
11. [重构映射表](#11-重构映射表)

---

## 1. 依赖清单

### workspace 顶层 `Cargo.toml`

```toml
[workspace.dependencies]
actix-web = { version = "4.14.0" }
actix-multipart = "0.8.0"
```

### 主 crate `[dependencies]`

```toml
actix-web = { workspace = true }
actix-multipart = { workspace = true }
actix-files = "0.6.10"
actix-cors = "0.7.1"

[dev-dependencies]
actix-rt = "2.11.0"
```

### 子 crate 依赖

| crate | 依赖 |
|---|---|
| `ipma-visualization` | `actix-web = "4.14.0"` |
| `ipma-init` | `actix-web = "4.14.0"`, `actix-multipart = "0.8.0"` |
| `ipma-data-manager` | `actix-web = { workspace = true }`, `actix-multipart = { workspace = true }` |
| `ipma-scheduler` | （**无 actix-web 依赖，完全框架无关**） |

---

## 2. 服务器启动与生命周期

文件：[`src/main.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/main.rs)

### 入口点

```rust
#[actix_web::main]
async fn main() -> std::io::Result<()>
```

### 关键设计

1. **`tokio::task::LocalSet` 包裹**：因 actix-web 的 future 默认 `!Send`，必须用 `LocalSet` + `spawn_local` 运行 `HttpServer`。
2. **UDS 监听**：使用 `create_uds_listener()` 自定义创建 `UnixListener`，设置 socket 文件权限 0666（允许 nginx 等访问）。
3. **多 ServerHandle 协调关闭**：当前仅 UDS 一个监听器，但已抽象为 `Vec<ServerHandle>` + `Vec<JoinHandle>` 双列表，支持扩展。
4. **优雅关闭流程**（5 步）：
   - 第 1 步：`handle.stop(true)` + 10 秒超时，停止接收新连接
   - 第 2 步：3 秒超时等待服务器任务结束
   - 第 3 步：`shutdown.request_shutdown()` 通知后台任务
   - 第 4 步：5 秒超时关闭调度器
   - 第 5 步：5 秒超时关闭数据库连接池
5. **强制退出兜底**：注册二次 Ctrl-C 处理，若优雅关闭超时则 `std::process::exit(1)`。

### `HttpServer` 配置

```rust
HttpServer::new(create_app)
    .workers(std::cmp::max(2, num_cpus::get()))
    .keep_alive(std::time::Duration::from_secs(5))
    .disable_signals()    // 由自定义 ShutdownSignal 管理
    .listen_uds(uds_listener)?
    .run();
```

### `create_app` 闭包

```rust
let create_app = move || {
    App::new()
        .wrap(Compress::default())
        .wrap(actix_web::middleware::Logger::default())
        .wrap(build_cors_middleware(&app_state_for_app.config))
        .wrap(RateLimitMiddleware::new(app_rate_limiter.clone(), app_rate_limit_enabled))
        .wrap(actix_middleware::from_fn(static_cache_control_middleware))
        .wrap(actix_middleware::from_fn(security_headers_middleware))
        .configure(|cfg| configure_app_services(cfg, &app_state_for_app, serve_static))
};
```

> **注意中间件顺序**：actix 中 `.wrap()` 后添加的先执行。执行顺序为：
> `security_headers → static_cache_control → RateLimit → Cors → Logger → Compress → handler`

---

## 3. 应用配置与中间件栈

文件：[`src/main.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/main.rs#L538-L669) `configure_app_services()`

### `app_data` 注册

```rust
cfg.app_data(app_state.clone());        // web::Data<AppState>
cfg.app_data(web::JsonConfig::default()
    .limit(10 * 1024 * 1024)            // 10MB
    .error_handler(json_error_handler));
cfg.app_data(web::FormConfig::default().limit(50 * 1024 * 1024));   // 50MB
cfg.app_data(web::PayloadConfig::new(50 * 1024 * 1024));            // 50MB
```

### 三种运行模式（基于 `config.init.enabled` 和 `serve_static`）

1. **初始化模式**（`init.enabled = true`）：
   - 注册 `/api/init/*` 路由（带 `localhost_only_middleware`）
   - 若 `serve_static`：托管 `/static` + `/init_index.html`，根路径重定向到 `/init_index.html`

2. **开发模式**（`init.enabled = false` + `serve_static = true`）：
   - 托管 `/static` 文件
   - 注册 `/static/locales/{lang}/{file:.*\.json}` 和 `/static/{path:.*\.json}` 走 `serve_json`
   - 注册业务 API 路由（`init_routes`）
   - `/main.html` 和 `/` 提供入口 HTML

3. **生产模式**（`init.enabled = false` + `serve_static = false`）：
   - 仅注册业务 API 路由，静态文件由 nginx 托管

### `build_cors_middleware`

文件：[`src/main.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/main.rs#L100-L148)

- 允许方法：GET/POST/PUT/DELETE/PATCH/OPTIONS
- `allow_any_header()` + `supports_credentials()` + `max_age(3600)`
- `allowed_origin_fn` 动态判定：
  - 比对 `config.server.cors_allowed_origins`
  - 自动加入 `public_url` 的 http/https 形式
  - 允许匹配末尾 `:80`/`:443` 的 origin
  - `allow_localhost_cors` 开启时允许 `localhost`/`127.0.0.1`/`[::1]` 任意端口

### 自定义 `from_fn` 中间件

文件：[`src/main.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/main.rs#L68-L98)

```rust
async fn static_cache_control_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error>
// 对 /static/ 路径设置 Cache-Control: no-cache, must-revalidate

async fn security_headers_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error>
// 设置 Content-Security-Policy: frame-ancestors 'none'
```

---

## 4. 路由组织

文件：[`src/routes/mod.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/routes/mod.rs#L120-L480)

### 路由风格

- **不使用** `#[get]/#[post]` 属性宏
- 全部使用 `web::scope().route("/path", web::get().to(handler))` 命令式风格
- 路径参数用 `/{id}` 形式

### 路由层级

```
/health                                    [无认证]
/api/auth/*                                [部分无认证]
    /login, /login/email, /login/send-code
    /login/two-factor, /login/send-2fa-code
    /logout, /refresh, /forgot-password, /reset-password
    /me  [auth_middleware]
/api/*                                     [auth_middleware]
    /users, /two-factor
    /resources/*
        /networks, /network-regions
        /rooms, /cabinets, /workstations, /positions
        /ip, /layouts, /topology
        /organizations, /org-templates
        /net-outlets, /cable-links
        /device-templates, /devices/*
    /logs
    /notifications
    /system/*
        /import-export/*                  [AdminUser]
        /logs/clear, /logs/stats          [AdminUser]
        /scheduled-tasks, /fail2ban
/api/init/*                                [localhost_only_middleware]
```

### 路由统计

- `.route()` / `.resource()` / `web::scope` / `web::service` 调用总计：**222 处**（主 crate）

### 数据导出/导入 handler 包装

```rust
async fn data_import_csv(
    _admin: crate::auth::extractor::AdminUser,    // 通过 extractor 强制管理员
    state: web::Data<AppState>,
    payload: actix_multipart::Multipart,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> { ... }
```

---

## 5. 自定义 Extractor

文件：[`src/auth/extractor.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/auth/extractor.rs)

### `AuthUser`

```rust
pub struct AuthUser {
    pub sub: String,
    pub username: String,
    pub role: String,
    pub device_fingerprint: Option<String>,
    pub ip_address: Option<String>,
}

impl FromRequest for AuthUser {
    type Error = AppError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        // 从 req.extensions() 读取 JwtClaims（由 auth_middleware 注入）
        match req.extensions().get::<JwtClaims>() {
            Some(c) => ready(Ok(AuthUser { ... })),
            None => ready(Err(AppError::Unauthorized("未授权访问".to_string()))),
        }
    }
}
```

### `AdminUser`

```rust
pub struct AdminUser { pub sub: String, pub username: String }

impl FromRequest for AdminUser {
    // 同上，额外校验 c.role == "admin"
}
```

### 关键点

- 两个 extractor 都**同步**（`Ready` future）
- 从 `req.extensions()` 读取 `JwtClaims`，由 `auth_middleware` 在中间件层注入
- 不解析请求体，可直接对应 axum 的 `FromRequestParts`

---

## 6. 中间件实现

### 6.1 `auth_middleware`

文件：[`src/auth/login.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/auth/login.rs#L60-L148)

```rust
pub async fn auth_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error>
```

**逻辑流程**：

1. `extract_token_from_service_request(&req)` 提取 Bearer token，缺失返回 401
2. `req.app_data::<web::Data<AppState>>()` 取应用状态，缺失返回 500
3. `state.jwt_utils.validate_token(&token)` 校验 JWT：
   - `ExpiredSignature` → `api.token_expired`
   - 其他 → `api.invalid_token`
4. 校验 `claims.token_type == "access"`（防 token 类型混淆）
5. `is_token_revoked()` 检查黑名单
6. `get_client_info_from_service_request(&req)` 获取 IP + User-Agent
7. `JwtUtils::generate_device_fingerprint()` 比对设备指纹
8. **`req.extensions_mut().insert(claims)`** 注入到 request extensions 供下游 extractor 使用
9. `next.call(req).await?` 调用下游，`res.map_into_left_body()`

**i18n**：错误消息通过 `detect_user_language(req.request())` + `ApiResponse::error_i18n()` 本地化

### 6.2 `localhost_only_middleware`

文件：[`src/auth/login.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/auth/login.rs#L150-L173)

```rust
pub async fn localhost_only_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error>
```

**逻辑**：`req.peer_addr().map(|a| a.ip().is_loopback()).unwrap_or(false)`，非本地返回 403

### 6.3 `RateLimitMiddleware`（最复杂）

文件：[`src/utils/rate_limit.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/utils/rate_limit.rs#L270-L399)

**双重实现**：

- `RateLimitMiddleware`：实现 `Transform<S, ServiceRequest>`
- `RateLimitMiddlewareService<S>`：实现 `Service<ServiceRequest>`

**类型签名**：

```rust
impl<S, B> Transform<S, ServiceRequest> for RateLimitMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = actix_web::Error;
    type Transform = RateLimitMiddlewareService<S>;
    type InitError = ();
    type Future = LocalBoxFuture<'static, Result<Self::Transform, Self::InitError>>;
    // ...
}
```

**关键设计**：

- `EitherBody<B>`：early-reject 时返回 `EitherBody::Right`（错误响应），正常通过返回 `EitherBody::Left`
- `LocalBoxFuture<'static, ...>`：pin 在当前线程
- 通过 `forward_ready!(service)` 转发 `poll_ready`

**业务逻辑**：

- 严格路径列表（`is_strict_path`）：登录/重置密码相关 7 条路径，仅 IP 限流
- 邮件路径列表（`is_email_path`）：3 条路径，额外限邮件发送频率
- `extract_user_id_from_token`：解码 JWT payload 取 `sub`（不做完整验证）
- `direct_ip = req.connection_info().realip_remote_addr()`
- 若 `limiter.is_trusted_proxy(&direct_ip)`，调 `get_real_ip_from_request(req.request())` 取真实 IP
- 否则 `normalize_ipv4_address(&direct_ip)`

**`RateLimitError`**：

```rust
impl ResponseError for RateLimitError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::TooManyRequests()
            .insert_header(("Retry-After", self.retry_after.to_string()))
            .json(json!({
                "success": false,
                "message": &self.message,
                "error_type": "rate_limit_exceeded",
                "retry_after": self.retry_after
            }))
    }
}
```

**`RateLimiter`（业务核心）**：

- 滑动窗口加权算法：`weighted_count = previous_count * prev_weight + current_count * current_weight`
- 三层限流：`ip_limits`、`user_limits`、`email_limits`（均 `DashMap<String, RateLimitEntry>`）
- 后台清理任务：每 60 秒 `cleanup_expired()`，监听 shutdown 信号

### 6.4 `static_cache_control_middleware` 和 `security_headers_middleware`

见 [§3](#3-应用配置与中间件栈)

---

## 7. 错误处理体系

### 7.1 主 crate `AppError`

文件：[`src/error.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/error.rs)

```rust
#[derive(Error, Debug)]
pub enum AppError {
    Database(String),
    NotFound(String),
    Validation(String),
    Unauthorized(String),
    Forbidden(String),
    Conflict(String),
    Internal(String),
    Snmp(String),
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode { /* 8 个分支 */ }
    fn error_response(&self) -> HttpResponse {
        match self {
            AppError::Internal(msg) => {
                error!("内部错误详情: {}", msg);
                HttpResponse::build(self.status_code())
                    .json(ApiResponse::<()>::error("服务器内部错误，请稍后重试".to_string()))
            }
            _ => HttpResponse::build(self.status_code())
                .json(ApiResponse::<()>::error(self.to_string())),
        }
    }
}
```

**`From` 转换链**：

- `From<sqlx::Error>`：按 PostgreSQL 错误码映射（23505/23503/23514/22P02/22023/08006/...）
- `From<validator::ValidationErrors>`
- `From<ipma_data_manager::DataError>`
- `From<ipma_visualization::VisualizationError>`
- `From<ipma_scheduler::SchedulerError>`

### 7.2 子 crate 错误类型

| crate | 文件 | 类型 | 状态码分支 |
|---|---|---|---|
| `ipma-visualization` | [`layout.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/crates/ipma-visualization/src/layout.rs#L9-L41) | `VisualizationError` | 5 |
| `ipma-data-manager` | [`types.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/crates/ipma-data-manager/src/types.rs#L34-L72) | `DataError` | 5 |
| `ipma-init` | [`error.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/crates/ipma-init/src/error.rs) | `InitError` | 7 |
| 主 crate | [`error.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/error.rs) | `AppError` | 8 |
| 主 crate | [`rate_limit.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/utils/rate_limit.rs#L67-L78) | `RateLimitError` | 429 固定 |

**总计 5 个 `ResponseError` 实现**，每个都有独立的 `From<sqlx::Error>` 实现（重复代码）。

### 7.3 子 crate `ApiResponse<T>` 类型

`ipma-data-manager` 在 `types.rs` 定义了 `ApiResponse<T>`，主 crate `models` 中也定义了同名类型。两者结构相同但分别定义。

---

## 8. 静态文件服务

文件：[`src/routes/static_files.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/src/routes/static_files.rs)

### `get_web_dir()`

```rust
static WEB_DIR: OnceLock<&'static str> = OnceLock::new();

pub fn get_web_dir() -> &'static str {
    WEB_DIR.get_or_init(|| {
        // 1. 环境变量 IPMA_WEB_DIR 优先
        // 2. /opt/ipma/web, /usr/share/ipma/web
        // 3. fallback "web"
    })
}
```

### `serve_json`

```rust
pub async fn serve_json(req: HttpRequest) -> Result<HttpResponse, Error>
```

- 路径前缀 `/static/`，校验 `..` 防穿越
- `tokio::fs::canonicalize` 校验解析路径必须 `starts_with(web_dir)`
- Content-Type: `application/json; charset=utf-8`

### `json_error_handler`

```rust
pub fn json_error_handler(err: JsonPayloadError, _req: &HttpRequest) -> Error
```

处理三类错误消息：

- `missing field` → "缺少必填字段: xxx"
- `invalid type` → "字段类型错误: xxx"
- 其他 → "JSON格式错误: xxx"

### `actix-files::Files` 配置

```rust
Files::new("/static", &static_path)
    .prefer_utf8(true)
    .use_etag(true)
    .use_last_modified(true)
```

---

## 9. 子 Crate 耦合点

### 9.1 `ipma-visualization`

**使用方式**：

- 返回 `Result<HttpResponse, VisualizationError>` 给主 crate
- 实现 `actix_web::ResponseError`
- 使用 `web::Path<Uuid>` 提取路径参数

**耦合文件**：

- [`src/topology.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/crates/ipma-visualization/src/topology.rs)：14 处 `HttpResponse::Ok().json(...)`
- [`src/layout.rs`](file:///media/oi-io/AA709DF48A7AD5C2/ipma/crates/ipma-visualization/src/layout.rs)：`ResponseError` 实现 + 多处 `HttpResponse`

### 9.2 `ipma-data-manager`

**使用方式**：

- 定义 `DataError` + `ResponseError`
- 定义 `DataProvider` trait（`#[async_trait]`，含 `decrypt_password`）
- 定义 `ApiResponse<T>` 类型
- `export.rs` / `backup.rs` 设置 `CONTENT_DISPOSITION` header 返回文件下载
- `import/mod.rs` 使用 `actix_multipart::Multipart`

**关键 handler 签名**：

```rust
pub async fn import_csv<P: DataProvider>(
    provider: P,
    mut payload: actix_multipart::Multipart,
    query: web::Query<HashMap<String, String>>,
) -> DataResult<HttpResponse>

pub async fn export_csv<P: DataProvider>(
    provider: P,
    type_param: web::Query<HashMap<String, String>>,
) -> DataResult<HttpResponse>
```

### 9.3 `ipma-init`

**使用方式**：

- 定义 `InitError` + `ResponseError`
- handlers 通过 `web::Data<InitContext>` 获取初始化上下文
- 使用 `web::Json<T>` 接收请求体
- `database_ops.rs` 使用 `actix_multipart::Multipart`

**关键 handler 签名**：

```rust
pub async fn init_system(
    ctx: web::Data<InitContext>,
    req: web::Json<InitRequest>,
) -> Result<HttpResponse, InitError>

pub async fn import_database_from_file(
    ctx: web::Data<InitContext>,
    payload: actix_multipart::Multipart,
) -> Result<HttpResponse, InitError>
```

### 9.4 `ipma-scheduler`

**完全框架无关**，仅依赖 `sqlx`/`tokio`/`tokio-cron-scheduler`，重构无需改动。

---

## 10. Multipart 文件上传

### `ipma-data-manager/src/import/mod.rs`

```rust
const MAX_UPLOAD_SIZE: usize = 50 * 1024 * 1024;

while let Some(mut field) = payload.try_next().await? {
    if field.name() == Some("file") {
        filename = field.content_disposition()
            .and_then(|cd| cd.get_filename().map(ToString::to_string));
        let mut data = Vec::new();
        while let Some(chunk) = field.try_next().await? {
            data.extend_from_slice(&chunk);
            if data.len() > MAX_UPLOAD_SIZE {
                return Err(DataError::Validation("文件大小超过50MB限制".to_string()));
            }
        }
        file_data = Some(data);
        break;
    }
}
```

### `ipma-init/src/handlers/database_ops.rs`

类似逻辑，导入数据库备份文件。

### `PayloadConfig` / `FormConfig`

```rust
cfg.app_data(web::FormConfig::default().limit(50 * 1024 * 1024));
cfg.app_data(web::PayloadConfig::new(50 * 1024 * 1024));
```

---

## 11. 重构映射表

### 依赖替换

| actix-web 系 | axum 系 |
|---|---|
| `actix-web = "4.14.0"` | `axum = "0.7"` |
| `actix-multipart = "0.8.0"` | `axum::extract::Multipart`（内置） |
| `actix-files = "0.6.10"` | `tower-http = { version = "0.6", features = ["fs"] }` |
| `actix-cors = "0.7.1"` | `tower-http::cors` |
| `actix-rt = "2.11.0"` | `tokio` (已存在) |
| - | `tower = "0.5"` |
| - | `tower-http` features: `["cors", "compression-full", "trace", "fs", "set-header"]` |

### API 映射

| actix-web | axum |
|---|---|
| `App::new()` | `Router::new()` |
| `web::scope("/api")` | `Router::new().nest("/api", ...)` 或直接合并 |
| `web::resource("/x").route(web::get().to(h))` | `Router::new().route("/x", get(h))` |
| `web::Json<T>` | `axum::Json<T>` |
| `web::Path<T>` | `axum::extract::Path<T>` |
| `web::Query<T>` | `axum::extract::Query<T>` |
| `web::Form<T>` | `axum::extract::Form<T>` |
| `web::Data<S>` | `axum::extract::State<S>` |
| `HttpRequest` | `axum::extract::Request` 或拆分为 `HeaderMap`/`ConnectInfo` 等 |
| `HttpResponse::Ok().json(x)` | `(StatusCode::OK, axum::Json(x)).into_response()` 或 `Response::builder()` |
| `FromRequest` | `FromRequest` / `FromRequestParts` |
| `ResponseError` | `IntoResponse` |
| `middleware::from_fn` | `axum::middleware::from_fn` / `from_fn_with_state` |
| `Transform` trait | `tower::Layer` |
| `Service` trait | `tower::Service` |
| `ServiceRequest` / `ServiceResponse` | `Request` / `Response` |
| `req.extensions_mut().insert(x)` | 相同 |
| `req.app_data::<Data<S>>()` | `State<S>::from_request_parts(...)` |
| `req.peer_addr()` | `ConnectInfo<SocketAddr>` extractor |
| `req.connection_info().realip_remote_addr()` | 自定义 header 解析（X-Forwarded-For） |
| `req.connection_info().scheme() == "https"` | 自定义 header 或 `Secure` 配置 |
| `HttpServer::new(...).workers(N).listen_uds(l)` | `axum::serve(listener, app).into_make_service()` |
| `actix_files::Files::new(...)` | `tower_http::services::ServeDir::new(...)` |
| `actix_cors::Cors` | `tower_http::cors::CorsLayer` |
| `Compress::default()` | `tower_http::compression::CompressionLayer` |
| `Logger::default()` | `tower_http::trace::TraceLayer` |
| `EitherBody<B>` | `Response<Body>` 直接返回 |
| `actix_multipart::Multipart` | `axum::extract::Multipart` |
| `#[actix_web::main]` | `#[tokio::main]` |
| `LocalSet` + `spawn_local` | （删除，axum future 是 Send） |
| `JsonConfig::default().error_handler(...)` | 自定义 `FromRequest` 包装 `Json` 或 `axum::extract::DefaultBodyLimit` + `JsonRejection` 映射 |

### 错误处理迁移模式

```rust
// actix-web
impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode { ... }
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(ApiResponse::<()>::error(self.to_string()))
    }
}

// axum
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        (status, axum::Json(ApiResponse::<()>::error(self.to_string()))).into_response()
    }
}
```

### Extractor 迁移模式

```rust
// actix-web
impl FromRequest for AuthUser {
    type Error = AppError;
    type Future = Ready<Result<Self, Self::Error>>;
    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future { ... }
}

// axum
#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        // 从 parts.extensions 取 JwtClaims
    }
}
```

### Middleware 迁移模式

```rust
// actix-web (from_fn style)
pub async fn auth_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> { ... }

// axum
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> { ... }
// 注册: .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
```

### RateLimitMiddleware 迁移方案

axum 推荐用 `axum::middleware::from_fn_with_state` 重写，early-reject 直接返回 `Response`：

```rust
pub async fn rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    req: Request,
    next: Next,
) -> Response {
    if let Err(e) = limiter.check_rate_limit(...) {
        return (StatusCode::TOO_MANY_REQUESTS,
                [("Retry-After", e.retry_after.to_string())],
                axum::Json(json!({...})))
            .into_response();
    }
    next.run(req).await
}
```

---

## 12. 关键约束（迁移时必须保留）

1. **UDS 监听** + socket 文件权限 0666
2. **优雅关闭** 5 步流程及超时
3. **二次 Ctrl-C 强制退出**兜底
4. **CORS 动态 origin 函数**逻辑（含 localhost、public_url 自动加入、:80/:443 后缀匹配）
5. **`frame-ancestors 'none'` 必须通过 HTTP 头**设置（不能 meta）
6. **`/static/` Cache-Control: no-cache, must-revalidate**
7. **JsonConfig 10MB / FormConfig 50MB / PayloadConfig 50MB** 限制
8. **JWT claims 注入 request extensions**（供 `AuthUser`/`AdminUser` 读取）
9. **token_type == "access" 校验**（防 token 类型混淆）
10. **设备指纹校验**
11. **Token 黑名单检查**
12. **i18n 错误消息**（`detect_user_language` + `error_i18n`）
13. **速率限制严格路径列表**（7 条）和邮件路径列表（3 条）
14. **三层限流**（IP/user/email）+ 滑动窗口加权算法
15. **`/api/init/*` 仅 localhost 访问**
16. **AdminUser 强制管理员**（数据导出/导入/日志清理）
17. **路径穿越防护**（`..` 检查 + `canonicalize` + `starts_with` 校验）
18. **JSON 错误消息本地化**（missing field / invalid type / 格式错误）
19. **生产模式不托管静态文件**（由 nginx 处理）
20. **后台任务关闭信号**（`broadcast::Receiver<()>`）

---

文档版本：1.0.0
最后更新：2026-08-07
