use actix_web::{HttpResponse, Result, web};
use bcrypt::hash;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use sqlx;
use sqlx::PgPool;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;
use uuid::Uuid;
use validator::Validate;

use crate::config::Config;
use crate::db::DbPool;
use crate::models::ApiResponse;

// 验证码结构
#[derive(Debug, Clone)]
struct VerificationCode {
    code: String,
    created_at: u64, // 时间戳
}

// 全局验证码存储
lazy_static! {
    static ref VERIFICATION_CODE: Mutex<VerificationCode> = Mutex::new(VerificationCode {
        code: generate_verification_code(),
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    });
}

// 生成随机验证码
fn generate_verification_code() -> String {
    let chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut code = String::with_capacity(16);
    let mut rng = rand::rng();

    for _ in 0..16 {
        let idx = rand::Rng::random_range(&mut rng, 0..chars.len());
        code.push(chars.chars().nth(idx).unwrap());
    }

    code
}

// 生成并输出新的验证码到控制台
fn generate_and_print_verification_code() -> VerificationCode {
    let code = generate_verification_code();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 输出到控制台
    info!("\n======================================================================");
    info!("                         系统初始化验证码                           ");
    info!("======================================================================");
    info!("  验证码: {}", code);
    info!("  有效期: 15分钟");
    info!("  请在初始化页面输入此验证码以完成系统初始化");
    info!("======================================================================\n");

    VerificationCode {
        code,
        created_at: now,
    }
}

// 更新配置文件中的init.enabled
fn update_config_enabled(enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string("config.toml")?;
    let mut value: toml::Value = toml::from_str(&content)?;
    if let Some(init) = value.get_mut("init")
        && let Some(table) = init.as_table_mut()
    {
        table.insert("enabled".to_string(), toml::Value::Boolean(enabled));
    }
    let new_content = toml::to_string(&value)?;
    std::fs::write("config.toml", new_content)?;
    Ok(())
}

// 获取验证码的API端点 - 仅在控制台输出，不返回给前端
pub async fn get_verification_code(config: web::Data<Config>) -> Result<HttpResponse> {
    // 检查初始化开关是否开启
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    // 生成并输出新的验证码
    let verification_code = generate_and_print_verification_code();

    // 更新全局验证码
    if let Ok(mut lock) = VERIFICATION_CODE.lock() {
        *lock = verification_code.clone();
    }

    // 不返回验证码给前端，只返回成功消息
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        (),
        "验证码生成成功，请检查服务器控制台。",
    )))
}

// 检查必要的表是否存在
async fn check_required_tables_exist(pool: &sqlx::PgPool) -> bool {
    // 必要的表列表
    let required_tables = [
        "users",
        "network_cidrs",
        "network_regions",
        "regions",
        "region_networks",
        "rooms",
        "room_networks",
        "cabinets",
        "workstations",
        "positions",
        "workstation_ports",
        "position_ports",
        "switches",
        "switch_ports",
        "ip_managers",
        "operation_logs",
        "task_logs",
        "login_logs",
        "revoked_tokens",
        "token_usage",
        "notifications",
        "system_configs",
    ];

    // 检查每个表是否存在
    for table in &required_tables {
        let exists = match sqlx::query_scalar::<_, bool>(
            &format!("SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = '{}')", table)
        )
        .fetch_one(pool)
        .await
        {
            Ok(exists) => exists,
            Err(_) => return false,
        };

        if !exists {
            return false;
        }
    }

    true
}

// 检查数据库是否有数据
async fn check_has_data(pool: &sqlx::PgPool) -> bool {
    // 检查users表是否有记录
    match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
    {
        Ok(count) => count > 0,
        Err(_) => false,
    }
}

// 检查数据库状态
pub async fn check_db_status(config: web::Data<Config>) -> Result<HttpResponse> {
    let pool = match DbPool::new(&config.database).await {
        Ok(p) => p,
        Err(_) => {
            return Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "data": {
                    "connected": false,
                    "has_tables": false,
                    "required_tables_exist": false,
                    "has_data": false
                }
            })));
        }
    };

    // 检查是否有表
    let has_tables = match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public'",
    )
    .fetch_one(pool.get_conn())
    .await
    {
        Ok(count) => count > 0,
        Err(_) => false,
    };

    // 检查必要的表是否存在
    let required_tables_exist = check_required_tables_exist(pool.get_conn()).await;

    // 检查是否有数据
    let has_data = if required_tables_exist {
        check_has_data(pool.get_conn()).await
    } else {
        false
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "data": {
            "connected": true,
            "has_tables": has_tables,
            "required_tables_exist": required_tables_exist,
            "has_data": has_data
        }
    })))
}

// 扩展UserCreate结构体，添加验证字段
#[derive(Debug, Serialize, Deserialize, validator::Validate)]
pub struct InitRequest {
    #[validate(length(min = 3, max = 50, message = "用户名长度必须在3到50个字符之间"))]
    pub username: String,
    #[validate(length(min = 8, message = "密码长度必须至少8个字符"))]
    pub password: String,
    #[validate(email(message = "请输入有效的邮箱地址"))]
    pub email: String,
    #[validate(length(min = 1, max = 20, message = "角色长度必须在1到20个字符之间"))]
    pub role: String,
    #[validate(length(min = 16, max = 16, message = "验证码长度必须为16个字符"))]
    pub verification: String,
    pub clear_db: Option<bool>, // 是否清空数据库
}

#[derive(Debug, Serialize, Deserialize, validator::Validate)]
pub struct ResetPasswordRequest {
    #[validate(length(min = 1))]
    pub username: String,
    #[validate(length(min = 6))]
    pub new_password: String,
    pub verification: String,
}

pub async fn init_system(
    config: web::Data<Config>,
    req: web::Json<InitRequest>,
) -> Result<HttpResponse> {
    // 0. 验证输入数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "Validation error: {:?}",
                e
            ))),
        );
    }

    // 1. 检查初始化开关是否开启
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    // 2. 验证验证码
    let stored_code_copy = {
        let stored_code = VERIFICATION_CODE.lock().map_err(|_| {
            actix_web::error::ErrorInternalServerError("Failed to access verification code")
        })?;
        stored_code.clone()
    };

    // 检查验证码是否过期（15分钟）
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    if now - stored_code_copy.created_at > 15 * 60 {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error("验证码已过期，请重新生成验证码")));
    }

    if stored_code_copy.code != req.verification {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("验证码无效")));
    }

    // 3. 创建数据库连接
    let pool = DbPool::new(&config.database).await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("创建数据库失败: {}", e))
    })?;

    // 4. 检查是否已经初始化
    let count: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool.get_conn())
        .await
    {
        Ok(count) => count,
        Err(e) => {
            // 检查是否是表不存在的错误
            if let Some(db_err) = e.as_database_error() {
                let message = db_err.to_string();
                if message.contains("UndefinedTable") {
                    // 表不存在，说明尚未初始化
                    0
                } else {
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error(format!("数据库查询错误: {}", e))));
                }
            } else {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("数据库查询错误: {}", e))));
            }
        }
    };

    // 5. 处理非空数据库情况
    if count > 0 {
        if req.clear_db.unwrap_or(false) {
            // 清空数据库
            tracing::info!("正在清空数据库...");
            match drop_all_tables(pool.get_conn()).await {
                Ok(_) => tracing::info!("数据库清空成功"),
                Err(e) => {
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error(format!("清空数据库失败: {}", e))));
                }
            }
            // 重新创建表
            match create_tables(pool.get_conn()).await {
                Ok(_) => (),
                Err(e) => {
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error(format!("创建表失败: {}", e))));
                }
            }
        } else {
            // 数据库不为空且未选择清空，返回错误
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                "数据库不为空，请先设置clear_db=true来清空数据库。",
            )));
        }
    } else {
        // 创建数据库表
        match create_tables(pool.get_conn()).await {
            Ok(_) => (),
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("创建表失败: {}", e))));
            }
        }
    }

    // 6. 创建管理员用户，降低bcrypt成本因子以提高性能
    const BCRYPT_COST: u32 = 12; 
    let password_hash = match hash(&req.password, BCRYPT_COST) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!("密码哈希错误: {:?}", e);
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("密码哈希错误: {}", e))));
        }
    };

    let user_id = Uuid::new_v4();
    let status = true;

    tracing::info!(
        "正在创建管理员用户，ID: {}, 用户名: {}, 邮箱: {}, 角色: {}",
        user_id,
        req.username,
        req.email,
        req.role
    );

    // 7. 不传递created_at::TIMESTAMP WITH TIME ZONE as created_at和updated_at::TIMESTAMP WITH TIME ZONE as updated_at，让数据库使用默认值NOW()
    match sqlx::query(
        r#"INSERT INTO users (id, username, password_hash, email, role, status) 
               VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(user_id)
    .bind(&req.username)
    .bind(&password_hash)
    .bind(&req.email)
    .bind(&req.role)
    .bind(status)
    .execute(pool.get_conn())
    .await
    {
        Ok(_) => (),
        Err(e) => {
            tracing::error!("创建管理员用户失败: {:?}", e);
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "创建管理员用户失败: {}",
                    e
                ))),
            );
        }
    };

    // 8. 更新配置，关闭初始化模式
    match update_config_enabled(false) {
        Ok(_) => (),
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("更新配置失败: {}", e))));
        }
    }

    info!(
        "系统初始化成功，管理员用户已创建: {}, 初始化模式已禁用",
        req.username
    );

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "系统初始化成功")))
}

pub async fn init_db(config: web::Data<Config>) -> Result<HttpResponse> {
    // 创建数据库连接
    let pool = match DbPool::new(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("创建数据库失败: {}", e))));
        }
    };

    // 检查必要的表是否存在
    let required_tables_exist = check_required_tables_exist(pool.get_conn()).await;

    // 如果表不存在或不完整，创建或补全表结构
    if !required_tables_exist {
        match create_tables(pool.get_conn()).await {
            Ok(_) => info!("数据库表结构初始化成功"),
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("创建表失败: {}", e))));
            }
        };
    } else {
        info!("数据库表结构已存在，跳过初始化");
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "数据库初始化成功")))
}

// 清空数据库API
pub async fn clear_database(
    config: web::Data<Config>,
    verification: web::Query<serde_json::Value>,
) -> Result<HttpResponse> {
    // 检查初始化开关是否开启
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    // 验证验证码
    let verification_code = match verification.get("code") {
        Some(code) => code.as_str().unwrap_or(""),
        None => "",
    };

    let stored_code_copy = {
        let stored_code = VERIFICATION_CODE.lock().map_err(|_| {
            actix_web::error::ErrorInternalServerError("Failed to access verification code")
        })?;
        stored_code.clone()
    };

    // 检查验证码是否过期（15分钟）
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    if now - stored_code_copy.created_at > 15 * 60 {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error("验证码已过期，请重新生成验证码")));
    }

    if stored_code_copy.code != verification_code {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "验证码无效，请检查输入的验证码是否正确",
        )));
    }

    // 创建数据库连接
    let pool = DbPool::new(&config.database).await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("创建数据库失败: {}", e))
    })?;

    // 清空数据库
    tracing::info!("正在通过API清空数据库...");
    drop_all_tables(pool.get_conn())
        .await
        .map_err(|_e| actix_web::error::ErrorInternalServerError("清空数据库失败"))?;

    tracing::info!("通过API清空数据库成功");
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "数据库清空成功")))
}

pub async fn check_init_status(config: web::Data<Config>) -> Result<HttpResponse> {
    let pool = match DbPool::new(&config.database).await {
        Ok(p) => p,
        Err(_) => {
            return Ok(HttpResponse::Ok().json(serde_json::json!({
                "initialized": false,
                "version": env!("CARGO_PKG_VERSION"),
            })));
        }
    };

    let initialized = match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(pool.get_conn())
        .await
    {
        Ok(count) => count > 0,
        Err(_) => false,
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "initialized": initialized,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

// 清空数据库，删除所有表
async fn drop_all_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    // 获取所有表名
    let tables: Vec<String> = sqlx::query_scalar::<_, String>(
        r#"
        SELECT table_name FROM information_schema.tables 
        WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
    "#,
    )
    .fetch_all(pool)
    .await?;

    if !tables.is_empty() {
        // 禁用外键约束
        sqlx::query("SET session_replication_role = 'replica'")
            .execute(pool)
            .await?;

        // 删除所有表
        for table in tables {
            sqlx::query(&format!("DROP TABLE IF EXISTS {} CASCADE", table))
                .execute(pool)
                .await?;
        }

        // 启用外键约束
        sqlx::query("SET session_replication_role = 'origin'")
            .execute(pool)
            .await?;
    }

    Ok(())
}

async fn create_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    // 创建扩展
    sqlx::query("CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\"")
        .execute(pool)
        .await?;

    // 用户表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            username VARCHAR(50) UNIQUE NOT NULL,
            password_hash VARCHAR(255) NOT NULL,
            email VARCHAR(100) UNIQUE NOT NULL,
            role VARCHAR(20) NOT NULL,
            status BOOLEAN NOT NULL DEFAULT TRUE,
            reset_token VARCHAR(255),
            reset_token_expiry TIMESTAMP WITH TIME ZONE,
            two_factor_secret VARCHAR(255),
            two_factor_enabled BOOLEAN NOT NULL DEFAULT FALSE,
            two_factor_verified BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    // 网络区域表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS network_regions (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(20) NOT NULL UNIQUE,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    // 网络表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS network_cidrs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            network_region_id UUID NOT NULL REFERENCES network_regions(id),
            ipv4_cidr CIDR,
            ipv6_cidr CIDR,
            ipv4_gateway INET,
            ipv6_gateway INET,
            ipv4_dns INET,
            ipv6_dns INET,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    // 区域表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS regions (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    // 区域-网络关联表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS region_networks (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            region_id UUID NOT NULL REFERENCES regions(id) ON DELETE CASCADE,
            network_id UUID NOT NULL REFERENCES network_cidrs(id),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(region_id, network_id)
        )"#,
    )
    .execute(pool)
    .await?;

    // 房间表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS rooms (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    // 房间网络关联表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS room_networks (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
            network_id UUID NOT NULL REFERENCES network_cidrs(id),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(room_id, network_id)
        )"#,
    )
    .execute(pool)
    .await?;

    // 机柜表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cabinets (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            capacity INTEGER NOT NULL DEFAULT 42,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    // 机柜网络关联表
    sqlx::query(
        // cabinet_networks 表已移除，机柜通过房间间接关联网络
        "",
    )
    .execute(pool)
    .await?;

    // 工位表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS workstations (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_id UUID NOT NULL REFERENCES rooms(id),
            manager VARCHAR(50),
            description TEXT,
            x INTEGER NOT NULL DEFAULT 0,
            y INTEGER NOT NULL DEFAULT 0,
            width INTEGER NOT NULL DEFAULT 160,
            height INTEGER NOT NULL DEFAULT 160,
            rotation INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    // 机位表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS positions (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            cabinet_id UUID NOT NULL REFERENCES cabinets(id) ON DELETE CASCADE,
            start_u INTEGER NOT NULL DEFAULT 1,
            end_u INTEGER NOT NULL DEFAULT 1,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    // SVG布局表 - 支持同时存储工位和机位可视化布局信息
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS svg_layouts (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            layout_type VARCHAR(20) NOT NULL, -- workstation, network_region
            room_id UUID REFERENCES rooms(id) ON DELETE CASCADE,
            network_region_id UUID REFERENCES network_regions(id) ON DELETE CASCADE,
            element_id UUID NOT NULL,
            element_type VARCHAR(20) NOT NULL, -- workstation, door, network_device
            x INTEGER NOT NULL DEFAULT 0,
            y INTEGER NOT NULL DEFAULT 0,
            width INTEGER NOT NULL DEFAULT 160,
            height INTEGER NOT NULL DEFAULT 160,
            rotation INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(layout_type, room_id, network_region_id, element_id)
        )"#,
    )
    .execute(pool)
    .await?;

    // 尝试添加列（如果表已存在但列不存在）
    let _ = sqlx::query(
        "ALTER TABLE positions ADD COLUMN IF NOT EXISTS start_u INTEGER NOT NULL DEFAULT 1",
    )
    .execute(pool)
    .await;
    let _ = sqlx::query(
        "ALTER TABLE positions ADD COLUMN IF NOT EXISTS end_u INTEGER NOT NULL DEFAULT 1",
    )
    .execute(pool)
    .await;

    // 交换机表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS switches (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            network_region_id UUID NOT NULL REFERENCES network_regions(id),
            ip_address VARCHAR(50) NOT NULL UNIQUE,
            mac_address VARCHAR(20),
            model VARCHAR(100),
            vendor VARCHAR(50),
            management_ip VARCHAR(50),
            location VARCHAR(100),
            snmp_version VARCHAR(10) DEFAULT 'v2c',
            snmp_community VARCHAR(100),
            snmp_username VARCHAR(50),
            snmp_auth_protocol VARCHAR(10),
            snmp_auth_password VARCHAR(100),
            snmp_priv_protocol VARCHAR(10),
            snmp_priv_password VARCHAR(100),
            snmp_port INTEGER DEFAULT 161,
            parent_switch_id UUID REFERENCES switches(id) ON DELETE SET NULL,
            parent_port_id UUID,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    // 交换机端口表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS switch_ports (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            switch_id UUID NOT NULL REFERENCES switches(id) ON DELETE CASCADE,
            port_number VARCHAR(30) NOT NULL,
            port_name VARCHAR(50),
            port_type VARCHAR(20) DEFAULT 'access',
            vlan_id INTEGER,
            status VARCHAR(20) DEFAULT 'up',
            speed VARCHAR(20),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(switch_id, port_number)
        )"#,
    )
    .execute(pool)
    .await?;

    // 工位-交换机端口关联表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS workstation_ports (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            workstation_id UUID NOT NULL REFERENCES workstations(id) ON DELETE CASCADE,
            switch_port_id UUID NOT NULL REFERENCES switch_ports(id) ON DELETE CASCADE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(workstation_id, switch_port_id)
        )"#,
    )
    .execute(pool)
    .await?;

    // 机位-交换机端口关联表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS position_ports (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            position_id UUID NOT NULL REFERENCES positions(id) ON DELETE CASCADE,
            switch_port_id UUID NOT NULL REFERENCES switch_ports(id) ON DELETE CASCADE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(position_id, switch_port_id)
        )"#,
    )
    .execute(pool)
    .await?;

    // 迁移：为现有的workstation_ports表添加switch_port_id列（如果不存在）
    let _ = sqlx::query("ALTER TABLE workstation_ports ADD COLUMN IF NOT EXISTS switch_port_id UUID REFERENCES switch_ports(id) ON DELETE CASCADE")
        .execute(pool).await;
    // 迁移：移除旧的network_id和port列（如果存在）
    let _ = sqlx::query("ALTER TABLE workstation_ports DROP COLUMN IF EXISTS network_id")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE workstation_ports DROP COLUMN IF EXISTS port")
        .execute(pool)
        .await;

    // 迁移：为现有的position_ports表添加switch_port_id列（如果不存在）
    let _ = sqlx::query("ALTER TABLE position_ports ADD COLUMN IF NOT EXISTS switch_port_id UUID REFERENCES switch_ports(id) ON DELETE CASCADE")
        .execute(pool).await;
    // 迁移：移除旧的network_id和port列（如果存在）
    let _ = sqlx::query("ALTER TABLE position_ports DROP COLUMN IF EXISTS network_id")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE position_ports DROP COLUMN IF EXISTS port")
        .execute(pool)
        .await;

    // IP 管理表
    sqlx::query(r#"CREATE TABLE IF NOT EXISTS ip_managers (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            workstation_id UUID REFERENCES workstations(id) ON DELETE SET NULL,
            position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
            switch_id UUID REFERENCES switches(id) ON DELETE SET NULL,
            switch_port_id UUID REFERENCES switch_ports(id) ON DELETE SET NULL,
            device_type VARCHAR(20) NOT NULL,
            network_id UUID NOT NULL REFERENCES network_cidrs(id),
            ip_address VARCHAR(50) NOT NULL,
            ip_version VARCHAR(10) NOT NULL DEFAULT 'IPv4',
            mac_address VARCHAR(20),
            hostname VARCHAR(100),
            status VARCHAR(20) NOT NULL DEFAULT 'active',
            last_seen TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_device_type CHECK (device_type IN ('workstation', 'cabinet_position', 'switch')),
            CONSTRAINT chk_single_device CHECK (
                (device_type = 'workstation' AND workstation_id IS NOT NULL AND position_id IS NULL AND switch_id IS NULL) OR
                (device_type = 'cabinet_position' AND workstation_id IS NULL AND position_id IS NOT NULL AND switch_id IS NULL) OR
                (device_type = 'switch' AND workstation_id IS NULL AND position_id IS NULL AND switch_id IS NOT NULL)
            )
        )"#)
        .execute(pool).await?;

    // 尝试为用户表添加密码重置相关字段
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN IF NOT EXISTS reset_token VARCHAR(255)")
        .execute(pool).await;
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN IF NOT EXISTS reset_token_expiry TIMESTAMP WITH TIME ZONE")
        .execute(pool).await;
    // 尝试为用户表添加2FA相关字段
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN IF NOT EXISTS two_factor_secret VARCHAR(255)")
        .execute(pool).await;
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN IF NOT EXISTS two_factor_enabled BOOLEAN NOT NULL DEFAULT FALSE")
        .execute(pool).await;
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN IF NOT EXISTS two_factor_verified BOOLEAN NOT NULL DEFAULT FALSE")
        .execute(pool).await;

    // 尝试添加邮箱验证码相关字段
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN IF NOT EXISTS two_factor_email_code VARCHAR(10)")
        .execute(pool).await;
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN IF NOT EXISTS two_factor_email_code_expiry TIMESTAMP WITH TIME ZONE")
        .execute(pool).await;
    
    // 确保 password_hash 长度足够（修复可能的截断问题）
    let _ = sqlx::query("ALTER TABLE users ALTER COLUMN password_hash TYPE VARCHAR(255)")
        .execute(pool).await;

    // 尝试添加 position_id 列（如果表已存在但列不存在）
    let _ = sqlx::query("ALTER TABLE ip_managers ADD COLUMN IF NOT EXISTS position_id UUID REFERENCES positions(id) ON DELETE SET NULL")
        .execute(pool).await;

    // 尝试添加 switch_id 列（如果表已存在但列不存在）
    let _ = sqlx::query("ALTER TABLE ip_managers ADD COLUMN IF NOT EXISTS switch_id UUID REFERENCES switches(id) ON DELETE SET NULL")
        .execute(pool).await;

    // 尝试添加 switch_port_id 列（如果表已存在但列不存在）
    let _ = sqlx::query("ALTER TABLE ip_managers ADD COLUMN IF NOT EXISTS switch_port_id UUID REFERENCES switch_ports(id) ON DELETE SET NULL")
        .execute(pool).await;

    // 尝试添加 device_type 列（如果表已存在但列不存在）
    let _ = sqlx::query("ALTER TABLE ip_managers ADD COLUMN IF NOT EXISTS device_type VARCHAR(20) NOT NULL DEFAULT 'workstation'")
        .execute(pool).await;

    // 修改 workstation_id 为可选（允许 NULL）
    let _ = sqlx::query("ALTER TABLE ip_managers ALTER COLUMN workstation_id DROP NOT NULL")
        .execute(pool)
        .await;

    // 操作日志表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS operation_logs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            user_id UUID NOT NULL REFERENCES users(id),
            action VARCHAR(100) NOT NULL,
            resource_type VARCHAR(50) NOT NULL,
            resource_id UUID NOT NULL,
            details JSONB NOT NULL DEFAULT '{}',
            result BOOLEAN NOT NULL,
            ip_address VARCHAR(50) NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    // 任务执行日志表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS task_logs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            task_name VARCHAR(100) NOT NULL,
            status VARCHAR(20) NOT NULL,
            details JSONB NOT NULL DEFAULT '{}',
            start_time TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            end_time TIMESTAMP WITH TIME ZONE,
            duration INTEGER
        )"#,
    )
    .execute(pool)
    .await?;

    // 登录日志表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS login_logs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            username VARCHAR(50) NOT NULL,
            ip_address VARCHAR(50) NOT NULL,
            user_agent VARCHAR(255),
            success BOOLEAN NOT NULL,
            error_message VARCHAR(255),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    // 令牌撤销表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS revoked_tokens (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            token_hash VARCHAR(255) NOT NULL,
            user_id UUID REFERENCES users(id),
            revoked_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            expiry TIMESTAMP WITH TIME ZONE NOT NULL
        )"#,
    )
    .execute(pool)
    .await?;

    // 令牌使用频率表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS token_usage (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            token_hash VARCHAR(255) NOT NULL,
            user_id UUID REFERENCES users(id),
            ip_address VARCHAR(50) NOT NULL,
            user_agent VARCHAR(255),
            request_path VARCHAR(255) NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    // 通知表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS notifications (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            user_id UUID REFERENCES users(id),
            title VARCHAR(100) NOT NULL,
            content TEXT NOT NULL,
            notification_type VARCHAR(20) NOT NULL,
            read BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    // 系统配置表
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS system_configs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            config_type VARCHAR(50) NOT NULL,
            key VARCHAR(50) NOT NULL,
            value VARCHAR(255),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(config_type, key)
        )"#,
    )
    .execute(pool)
    .await?;

    // 创建索引
    sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_users_username ON users(username)"#)
        .execute(pool)
        .await?;

    sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_users_email ON users(email)"#)
        .execute(pool)
        .await?;

    sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_ip_managers_workstation_id ON ip_managers(workstation_id)"#)
        .execute(pool).await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_ip_managers_ip_address ON ip_managers(ip_address)"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_ip_managers_mac_address ON ip_managers(mac_address)"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_operation_logs_user_id ON operation_logs(user_id)"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_operation_logs_created_at ON operation_logs(created_at)"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_login_logs_username ON login_logs(username)"#)
        .execute(pool)
        .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_login_logs_created_at ON login_logs(created_at)"#,
    )
    .execute(pool)
    .await?;

    // 为令牌撤销表添加索引
    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_revoked_tokens_token_hash ON revoked_tokens(token_hash)"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_revoked_tokens_user_id ON revoked_tokens(user_id)"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_revoked_tokens_expiry ON revoked_tokens(expiry)"#,
    )
    .execute(pool)
    .await?;

    // 为令牌使用频率表添加索引
    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_token_usage_token_hash ON token_usage(token_hash)"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_token_usage_user_id ON token_usage(user_id)"#)
        .execute(pool)
        .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_token_usage_ip_address ON token_usage(ip_address)"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_token_usage_created_at ON token_usage(created_at)"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_notifications_user_id ON notifications(user_id)"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_notifications_created_at ON notifications(created_at)"#,
    )
    .execute(pool)
    .await?;

    // 交换机索引
    sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_switches_ip_address ON switches(ip_address)"#)
        .execute(pool)
        .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_switches_parent_switch_id ON switches(parent_switch_id)"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE INDEX IF NOT EXISTS idx_switch_ports_switch_id ON switch_ports(switch_id)"#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

// 导入数据库API
pub async fn import_database(config: web::Data<Config>) -> Result<HttpResponse> {
    // 检查初始化开关是否开启
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    // 创建数据库连接
    let pool = DbPool::new(&config.database).await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("创建数据库失败: {}", e))
    })?;

    // 清空现有表结构
    drop_all_tables(pool.get_conn()).await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("清空数据库失败: {}", e))
    })?;

    // 重新创建表结构
    create_tables(pool.get_conn())
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("创建表失败: {}", e)))?;

    info!("数据库导入成功");

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "数据库导入成功")))
}

// 重启程序API
pub async fn restart_program() -> Result<HttpResponse> {
    info!("收到重启程序请求，正在准备重启...");

    // 启动一个新的进程来重启程序
    std::thread::spawn(|| {
        // 延时1秒后执行重启
        std::thread::sleep(std::time::Duration::from_secs(1));

        // 执行重启命令
        let _ = std::process::Command::new("systemctl")
            .arg("restart")
            .arg("ipma.service")
            .status();
    });

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
        (),
        "程序重启请求已接收，系统正在重启...",
    )))
}

pub async fn check_pgsql(config: web::Data<Config>) -> Result<HttpResponse> {
    // 检查 PostgreSQL 是否安装
    let installed = std::process::Command::new("which")
        .arg("psql")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !installed {
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "installed": false,
            "running": false,
            "error": "PostgreSQL is not installed. Please install PostgreSQL first."
        })));
    }

    // 尝试连接到 PostgreSQL 的 template1 数据库（总是存在的模板数据库）
    let url = format!(
        "postgres://{}:{}@{}:{}/template1",
        config.database.username,
        config.database.password,
        config.database.host,
        config.database.port
    );

    match PgPool::connect(&url).await {
        Ok(_) => {
            // PostgreSQL 运行，只返回连接状态
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "installed": true,
                "running": true,
                "message": "PostgreSQL is running and connection is successful."
            })))
        }
        Err(e) => {
            let error_str = e.to_string();
            if error_str.contains("connect")
                || error_str.contains("timeout")
                || error_str.contains("refused")
            {
                Ok(HttpResponse::Ok().json(serde_json::json!({
                    "installed": true,
                    "running": false,
                    "error": format!("PostgreSQL is installed but not running: {}", error_str)
                })))
            } else {
                // 可能是认证错误或其他配置问题，但PostgreSQL运行
                Ok(HttpResponse::Ok().json(serde_json::json!({
                    "installed": true,
                    "running": true,
                    "auto_initialized": false,
                    "error": format!("PostgreSQL connection error (may be authentication or configuration issue): {}", error_str)
                })))
            }
        }
    }
}
