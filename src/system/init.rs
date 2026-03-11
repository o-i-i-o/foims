use actix_multipart::Multipart;
use actix_web::{HttpResponse, Result, web};
use bcrypt::hash;
use futures_util::TryStreamExt;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use sqlx;
use sqlx::PgPool;
use sqlx::Row;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;
use uuid::Uuid;
use validator::Validate;

use crate::config::Config;
use crate::models::ApiResponse;

// ==================== 常量定义 ====================

const VERIFICATION_CODE_EXPIRY_SECS: u64 = 15 * 60;
const BCRYPT_COST: u32 = 12;

// ==================== 类型定义 ====================

#[derive(Debug, Clone)]
struct VerificationCode {
    code: String,
    created_at: u64,
}

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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDatabaseRequest {
    pub verification: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportDatabaseRequest {
    pub verification: String,
}

#[derive(Debug, Serialize)]
pub struct CreateDatabaseResponse {
    pub backup_file: Option<String>,
    pub message: String,
}

lazy_static! {
    static ref VERIFICATION_CODE: Mutex<VerificationCode> = Mutex::new(VerificationCode {
        code: generate_verification_code(),
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    });
}

// ==================== 验证码函数 ====================

fn generate_verification_code() -> String {
    use rand::RngExt;
    let chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut code = String::with_capacity(16);
    let mut rng = rand::rng();

    for _ in 0..16 {
        let idx = rng.random_range(0..chars.len());
        code.push(chars.chars().nth(idx).unwrap());
    }

    code
}

fn generate_and_print_verification_code() -> VerificationCode {
    let code = generate_verification_code();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

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

fn verify_code(provided_code: &str) -> Result<(), String> {
    let stored_code = VERIFICATION_CODE.lock()
        .map_err(|_| "无法访问验证码".to_string())?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if now - stored_code.created_at > VERIFICATION_CODE_EXPIRY_SECS {
        return Err("验证码已过期，请重新生成验证码".to_string());
    }

    if stored_code.code != provided_code {
        return Err("验证码无效".to_string());
    }

    Ok(())
}

pub async fn get_verification_code(config: web::Data<Config>) -> Result<HttpResponse> {
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    let verification_code = generate_and_print_verification_code();

    if let Ok(mut lock) = VERIFICATION_CODE.lock() {
        *lock = verification_code.clone();
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        (),
        "验证码生成成功，请检查服务器控制台。",
    )))
}

// ==================== 配置函数 ====================

fn get_backup_dir() -> String {
    if let Some(home) = std::env::var_os("HOME") {
        format!("{}/ipma_backups", home.to_string_lossy())
    } else {
        "/opt/ipma/backups".to_string()
    }
}

fn update_config_enabled(enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = crate::config::get_config_file_path();
    let content = std::fs::read_to_string(&config_path)?;
    let mut value: toml::Value = toml::from_str(&content)?;
    if let Some(init) = value.get_mut("init")
        && let Some(table) = init.as_table_mut()
    {
        table.insert("enabled".to_string(), toml::Value::Boolean(enabled));
    }
    let new_content = toml::to_string(&value)?;
    std::fs::write(&config_path, new_content)?;
    Ok(())
}

// ==================== 数据库检查函数 ====================

fn get_required_tables() -> Vec<&'static str> {
    vec![
        "users",
        "network_cidrs",
        "network_regions",
        "rooms",
        "room_networks",
        "cabinets",
        "workstations",
        "workstation_ports",
        "positions",
        "position_ports",
        "switches",
        "switch_ports",
        "switch_macs",
        "switch_lldps",
        "ip_managers",
        "operation_logs",
        "task_logs",
        "login_logs",
        "revoked_tokens",
        "token_usage",
        "notifications",
        "system_configs",
        "svg_layouts",
    ]
}

fn get_table_columns() -> HashMap<&'static str, Vec<&'static str>> {
    let mut columns: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
    
    columns.insert("users", vec!["id", "username", "password_hash", "email", "role", "status", "created_at", "updated_at"]);
    columns.insert("network_regions", vec!["id", "name", "description", "created_at", "updated_at"]);
    columns.insert("network_cidrs", vec!["id", "name", "network_region_id", "ipv4_cidr", "ipv6_cidr", "ipv4_gateway", "ipv6_gateway", "created_at", "updated_at"]);
    columns.insert("rooms", vec!["id", "name", "room_type", "description", "created_at", "updated_at"]);
    columns.insert("room_networks", vec!["id", "room_id", "network_id", "created_at", "updated_at"]);
    columns.insert("cabinets", vec!["id", "name", "room_id", "capacity", "network_id", "description", "created_at", "updated_at"]);
    columns.insert("workstations", vec!["id", "name", "room_id", "manager", "description", "created_at", "updated_at"]);
    columns.insert("workstation_ports", vec!["id", "workstation_id", "switch_port_id", "created_at", "updated_at"]);
    columns.insert("positions", vec!["id", "name", "cabinet_id", "start_u", "end_u", "network_id", "description", "created_at", "updated_at"]);
    columns.insert("position_ports", vec!["id", "position_id", "switch_port_id", "created_at", "updated_at"]);
    columns.insert("switches", vec!["id", "name", "network_region_id", "network_id", "model", "vendor", "location", "snmp_version", "snmp_community", "parent_switch_id", "parent_port_id", "description", "created_at", "updated_at"]);
    columns.insert("switch_ports", vec!["id", "switch_id", "port_number", "port_name", "port_type", "vlan_id", "status", "speed", "description", "created_at", "updated_at"]);
    columns.insert("switch_macs", vec!["id", "switch_id", "ip_address", "mac_address", "interface", "vlan_id", "created_at", "updated_at"]);
    columns.insert("switch_lldps", vec!["id", "switch_id", "local_port", "neighbor_chassis_id", "neighbor_port_id", "neighbor_port_desc", "neighbor_sys_name", "neighbor_sys_desc", "created_at", "updated_at"]);
    columns.insert("ip_managers", vec!["id", "workstation_id", "position_id", "switch_id", "switch_port_id", "device_type", "network_id", "ip_address", "ip_version", "mac_address", "hostname", "status", "last_seen", "created_at", "updated_at"]);
    columns.insert("operation_logs", vec!["id", "user_id", "action", "resource_type", "resource_id", "details", "result", "ip_address", "created_at"]);
    columns.insert("task_logs", vec!["id", "task_name", "status", "details", "start_time", "end_time", "duration"]);
    columns.insert("login_logs", vec!["id", "username", "ip_address", "user_agent", "success", "error_message", "created_at"]);
    columns.insert("revoked_tokens", vec!["id", "token_hash", "user_id", "revoked_at", "expiry"]);
    columns.insert("token_usage", vec!["id", "token_hash", "user_id", "ip_address", "user_agent", "request_path", "created_at"]);
    columns.insert("notifications", vec!["id", "user_id", "title", "content", "notification_type", "read", "created_at"]);
    columns.insert("system_configs", vec!["id", "config_type", "key", "value", "created_at", "updated_at"]);
    columns.insert("svg_layouts", vec!["id", "layout_type", "room_id", "network_region_id", "element_id", "element_type", "x", "y", "width", "height", "rotation", "created_at", "updated_at"]);
    
    columns
}

async fn check_required_tables_exist(pool: &sqlx::PgPool) -> bool {
    let required_tables = get_required_tables();
    
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

async fn validate_table_columns(pool: &sqlx::PgPool) -> Result<(), String> {
    let required_columns = get_table_columns();
    
    for (table, columns) in required_columns {
        let table_exists: bool = match sqlx::query_scalar(
            &format!("SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = '{}')", table)
        )
        .fetch_one(pool)
        .await
        {
            Ok(exists) => exists,
            Err(e) => return Err(format!("检查表 {} 是否存在时出错: {}", table, e)),
        };
        
        if !table_exists {
            return Err(format!("表 {} 不存在", table));
        }
        
        for column in columns {
            let column_exists: bool = match sqlx::query_scalar(
                &format!(
                    "SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema = 'public' AND table_name = '{}' AND column_name = '{}')",
                    table, column
                )
            )
            .fetch_one(pool)
            .await
            {
                Ok(exists) => exists,
                Err(e) => return Err(format!("检查列 {}.{} 是否存在时出错: {}", table, column, e)),
            };
            
            if !column_exists {
                return Err(format!("表 {} 缺少必需的列: {}", table, column));
            }
        }
    }
    
    Ok(())
}

async fn check_has_data(pool: &sqlx::PgPool) -> bool {
    match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
    {
        Ok(count) => count > 0,
        Err(_) => false,
    }
}

// ==================== 数据库连接函数 ====================

async fn ensure_database_and_schema(config: &crate::config::DatabaseConfig) -> Result<PgPool, String> {
    let postgres_url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        config.username, config.password, config.host, config.port
    );

    let postgres_pool = PgPool::connect(&postgres_url)
        .await
        .map_err(|e| format!("连接PostgreSQL失败: {}", e))?;

    let db_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)"
    )
    .bind(&config.database)
    .fetch_one(&postgres_pool)
    .await
    .unwrap_or(false);

    if !db_exists {
        info!("数据库 {} 不存在，正在创建...", config.database);
        sqlx::query(&format!("CREATE DATABASE \"{}\"", config.database))
            .execute(&postgres_pool)
            .await
            .map_err(|e| format!("创建数据库失败: {}", e))?;
        info!("数据库 {} 创建成功", config.database);
    }

    drop(postgres_pool);

    let db_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        config.username, config.password, config.host, config.port, config.database
    );

    let pool = PgPool::connect(&db_url)
        .await
        .map_err(|e| format!("连接数据库失败: {}", e))?;

    let schema_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = 'public')"
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(false);

    if !schema_exists {
        info!("public schema 不存在，正在创建...");
        sqlx::query("CREATE SCHEMA IF NOT EXISTS public")
            .execute(&pool)
            .await
            .map_err(|e| format!("创建schema失败: {}", e))?;
        sqlx::query("GRANT ALL ON SCHEMA public TO postgres")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("GRANT ALL ON SCHEMA public TO public")
            .execute(&pool)
            .await
            .ok();
        info!("public schema 创建成功");
    }

    Ok(pool)
}

// ==================== 数据库操作函数 ====================

async fn backup_database(config: &crate::config::DatabaseConfig) -> Result<String, String> {
    let backup_dir = get_backup_dir();
    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("创建备份目录失败: {}", e))?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let backup_file = format!("{}/ipma_backup_{}.sql", backup_dir, timestamp);

    let output = std::process::Command::new("pg_dump")
        .arg("-h")
        .arg(&config.host)
        .arg("-p")
        .arg(config.port.to_string())
        .arg("-U")
        .arg(&config.username)
        .arg("-d")
        .arg(&config.database)
        .arg("-f")
        .arg(&backup_file)
        .env("PGPASSWORD", &config.password)
        .output()
        .map_err(|e| format!("执行pg_dump失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("备份失败: {}", stderr));
    }

    info!("数据库备份成功: {}", backup_file);
    Ok(backup_file)
}

async fn drop_database(config: &crate::config::DatabaseConfig) -> Result<(), String> {
    let postgres_url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        config.username, config.password, config.host, config.port
    );

    let postgres_pool = PgPool::connect(&postgres_url)
        .await
        .map_err(|e| format!("连接PostgreSQL失败: {}", e))?;

    let terminate_query = format!(
        r#"SELECT pg_terminate_backend(pg_stat_activity.pid)
           FROM pg_stat_activity
           WHERE pg_stat_activity.datname = '{}'
           AND pid <> pg_backend_pid()"#,
        config.database
    );
    
    sqlx::query(&terminate_query)
        .execute(&postgres_pool)
        .await
        .map_err(|e| format!("断开数据库连接失败: {}", e))?;

    info!("已断开所有到数据库 {} 的连接", config.database);

    sqlx::query(&format!("DROP DATABASE IF EXISTS \"{}\"", config.database))
        .execute(&postgres_pool)
        .await
        .map_err(|e| format!("删除数据库失败: {}", e))?;

    info!("数据库 {} 已删除", config.database);
    Ok(())
}

async fn create_database(config: &crate::config::DatabaseConfig) -> Result<(), String> {
    let postgres_url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        config.username, config.password, config.host, config.port
    );

    let postgres_pool = PgPool::connect(&postgres_url)
        .await
        .map_err(|e| format!("连接PostgreSQL失败: {}", e))?;

    let db_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)"
    )
    .bind(&config.database)
    .fetch_one(&postgres_pool)
    .await
    .map_err(|e| format!("检查数据库是否存在失败: {}", e))?;

    if db_exists {
        info!("数据库 {} 已存在，跳过创建", config.database);
        return Ok(());
    }

    sqlx::query(&format!("CREATE DATABASE \"{}\" CONNECTION LIMIT = -1", config.database))
        .execute(&postgres_pool)
        .await
        .map_err(|e| format!("创建数据库失败: {}", e))?;

    info!("数据库 {} 创建成功", config.database);
    Ok(())
}

async fn drop_all_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let tables: Vec<String> = sqlx::query_scalar::<_, String>(
        r#"
        SELECT table_name FROM information_schema.tables 
        WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
    "#,
    )
    .fetch_all(pool)
    .await?;

    if !tables.is_empty() {
        sqlx::query("SET session_replication_role = 'replica'")
            .execute(pool)
            .await?;

        for table in tables {
            sqlx::query(&format!("DROP TABLE IF EXISTS {} CASCADE", table))
                .execute(pool)
                .await?;
        }

        sqlx::query("SET session_replication_role = 'origin'")
            .execute(pool)
            .await?;
    }

    Ok(())
}

// ==================== 数据库表结构创建函数 ====================

async fn create_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\"")
        .execute(pool)
        .await?;

    sqlx::query("CREATE EXTENSION IF NOT EXISTS \"pgcrypto\"")
        .execute(pool)
        .await?;

    create_users_table(pool).await?;
    create_network_tables(pool).await?;
    create_room_tables(pool).await?;
    create_switch_tables(pool).await?;
    create_cabinet_tables(pool).await?;
    create_workstation_tables(pool).await?;
    create_ip_managers_table(pool).await?;
    create_log_tables(pool).await?;
    create_token_tables(pool).await?;
    create_notification_tables(pool).await?;
    create_system_tables(pool).await?;

    create_indexes(pool).await?;
    create_views(pool).await?;
    create_crypto_functions(pool).await?;
    create_triggers(pool).await?;

    Ok(())
}

async fn create_users_table(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
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
            two_factor_email_code VARCHAR(10),
            two_factor_email_code_expiry TIMESTAMP WITH TIME ZONE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_network_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
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

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS network_cidrs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            network_region_id UUID NOT NULL REFERENCES network_regions(id),
            ipv4_cidr CIDR,
            ipv6_cidr CIDR,
            ipv4_gateway INET,
            ipv6_gateway INET,
            ipv4_dns INET[],
            ipv6_dns INET[],
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_room_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS rooms (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_type VARCHAR(20) NOT NULL DEFAULT 'OFFICE',
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_room_type CHECK (room_type IN ('OFFICE', 'DATA_CENTER'))
        )"#,
    )
    .execute(pool)
    .await?;

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

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS svg_layouts (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            layout_type VARCHAR(20) NOT NULL,
            room_id UUID REFERENCES rooms(id) ON DELETE CASCADE,
            network_region_id UUID REFERENCES network_regions(id) ON DELETE CASCADE,
            element_id UUID NOT NULL,
            element_type VARCHAR(20) NOT NULL,
            x INTEGER NOT NULL DEFAULT 0,
            y INTEGER NOT NULL DEFAULT 0,
            width INTEGER NOT NULL DEFAULT 160,
            height INTEGER NOT NULL DEFAULT 160,
            rotation INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE NULLS NOT DISTINCT(layout_type, room_id, network_region_id, element_id)
        )"#,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_cabinet_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cabinets (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_id UUID REFERENCES rooms(id) ON DELETE SET NULL,
            capacity INTEGER NOT NULL DEFAULT 42,
            network_id UUID REFERENCES network_cidrs(id),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS positions (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            cabinet_id UUID NOT NULL REFERENCES cabinets(id) ON DELETE CASCADE,
            start_u INTEGER NOT NULL DEFAULT 1,
            end_u INTEGER NOT NULL DEFAULT 1,
            network_id UUID REFERENCES network_cidrs(id),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

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
    .await
    .map(|_| ())
}

async fn create_workstation_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS workstations (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_id UUID NOT NULL REFERENCES rooms(id),
            manager VARCHAR(50),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

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
    .await
    .map(|_| ())
}

async fn create_switch_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS switches (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            network_region_id UUID NOT NULL REFERENCES network_regions(id),
            network_id UUID NOT NULL REFERENCES network_cidrs(id),
            model VARCHAR(100),
            vendor VARCHAR(50),
            location VARCHAR(100),
            snmp_version VARCHAR(3) DEFAULT 'v2c',
            snmp_community VARCHAR(64),
            snmp_username VARCHAR(22),
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

    let _ = sqlx::query(
        "ALTER TABLE switches ADD CONSTRAINT fk_parent_port_id FOREIGN KEY (parent_port_id) REFERENCES switch_ports(id) ON DELETE SET NULL"
    ).execute(pool).await;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS switch_macs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            switch_id UUID NOT NULL REFERENCES switches(id) ON DELETE CASCADE,
            ip_address VARCHAR(45) NOT NULL,
            mac_address VARCHAR(20) NOT NULL,
            interface VARCHAR(50),
            vlan_id INTEGER,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(switch_id, ip_address)
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS switch_lldps (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            switch_id UUID NOT NULL REFERENCES switches(id) ON DELETE CASCADE,
            local_port VARCHAR(50) NOT NULL,
            neighbor_chassis_id VARCHAR(100),
            neighbor_port_id VARCHAR(100),
            neighbor_port_desc VARCHAR(255),
            neighbor_sys_name VARCHAR(255),
            neighbor_sys_desc TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(switch_id, local_port)
        )"#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_ip_managers_table(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(r#"CREATE TABLE IF NOT EXISTS ip_managers (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            workstation_id UUID REFERENCES workstations(id) ON DELETE SET NULL,
            position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
            switch_id UUID REFERENCES switches(id) ON DELETE SET NULL,
            switch_port_id UUID REFERENCES switch_ports(id) ON DELETE SET NULL,
            device_type VARCHAR(20) NOT NULL,
            network_id UUID NOT NULL REFERENCES network_cidrs(id),
            ip_address INET NOT NULL,
            ip_version SMALLINT NOT NULL DEFAULT 4,
            mac_address VARCHAR(20),
            hostname VARCHAR(100),
            status VARCHAR(20) NOT NULL DEFAULT 'active',
            last_seen TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_device_type CHECK (device_type IN ('workstation', 'cabinet_position', 'switch', 'unknown')),
            CONSTRAINT chk_device_consistency CHECK (
                (device_type = 'switch' AND switch_id IS NOT NULL AND workstation_id IS NULL) OR
                (device_type = 'workstation' AND workstation_id IS NOT NULL AND position_id IS NULL AND switch_id IS NULL) OR
                (device_type = 'cabinet_position' AND position_id IS NOT NULL AND workstation_id IS NULL AND switch_id IS NULL) OR
                (device_type = 'unknown')
            )
        )"#)
        .execute(pool).await?;
    Ok(())
}

async fn create_log_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
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

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS mac_history (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            mac_address VARCHAR(20) NOT NULL,
            ip_address INET NOT NULL,
            ip_manager_id UUID NOT NULL REFERENCES ip_managers(id) ON DELETE CASCADE,
            device_type VARCHAR(20) NOT NULL,
            workstation_id UUID REFERENCES workstations(id) ON DELETE SET NULL,
            position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
            switch_id UUID REFERENCES switches(id) ON DELETE SET NULL,
            network_id UUID NOT NULL REFERENCES network_cidrs(id),
            change_type VARCHAR(20) NOT NULL DEFAULT 'update',
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_mac_history_mac_address ON mac_history(mac_address)"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_mac_history_ip_address ON mac_history(ip_address)"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_mac_history_ip_manager_id ON mac_history(ip_manager_id)"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_mac_history_created_at ON mac_history(created_at)"
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_token_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
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
    .await
    .map(|_| ())
}

async fn create_notification_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
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
    .await
    .map(|_| ())
}

async fn create_system_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS system_configs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            config_type VARCHAR(50) NOT NULL,
            key VARCHAR(100) NOT NULL,
            value TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(config_type, key)
        )"#,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

async fn create_indexes(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let indexes = [
        "CREATE INDEX IF NOT EXISTS idx_users_username ON users(username)",
        "CREATE INDEX IF NOT EXISTS idx_users_email ON users(email)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_workstation_id ON ip_managers(workstation_id)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_switch_id ON ip_managers(switch_id) WHERE device_type = 'switch'",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_ip_address ON ip_managers(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_mac_address ON ip_managers(mac_address)",
        "CREATE INDEX IF NOT EXISTS idx_operation_logs_user_id ON operation_logs(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_operation_logs_created_at ON operation_logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_login_logs_username ON login_logs(username)",
        "CREATE INDEX IF NOT EXISTS idx_login_logs_created_at ON login_logs(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_revoked_tokens_token_hash ON revoked_tokens(token_hash)",
        "CREATE INDEX IF NOT EXISTS idx_revoked_tokens_user_id ON revoked_tokens(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_revoked_tokens_expiry ON revoked_tokens(expiry)",
        "CREATE INDEX IF NOT EXISTS idx_token_usage_token_hash ON token_usage(token_hash)",
        "CREATE INDEX IF NOT EXISTS idx_token_usage_user_id ON token_usage(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_token_usage_ip_address ON token_usage(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_token_usage_created_at ON token_usage(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_notifications_user_id ON notifications(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_notifications_created_at ON notifications(created_at)",
        "CREATE INDEX IF NOT EXISTS idx_switches_parent_switch_id ON switches(parent_switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_ports_switch_id ON switch_ports(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_switch_id ON switch_macs(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_ip_address ON switch_macs(ip_address)",
        "CREATE INDEX IF NOT EXISTS idx_switch_macs_mac_address ON switch_macs(mac_address)",
        "CREATE INDEX IF NOT EXISTS idx_switch_lldps_switch_id ON switch_lldps(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_switch_id ON ip_managers(switch_id)",
        "CREATE INDEX IF NOT EXISTS idx_ip_managers_switch_device ON ip_managers(switch_id, device_type)",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_ip_managers_ip_unique ON ip_managers(ip_address)",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_ip_managers_mac_unique ON ip_managers(mac_address) WHERE mac_address IS NOT NULL AND mac_address != ''",
    ];

    for idx in &indexes {
        sqlx::query(idx).execute(pool).await?;
    }

    migrate_ip_managers_constraint(pool).await?;
    migrate_switch_position_fields(pool).await?;
    migrate_mac_history(pool).await?;

    Ok(())
}

async fn migrate_ip_managers_constraint(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM pg_constraint WHERE conname = 'chk_device_consistency'"
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);
    
    if count > 0 {
        sqlx::query("ALTER TABLE ip_managers DROP CONSTRAINT IF EXISTS chk_device_consistency")
            .execute(pool)
            .await?;
        
        sqlx::query("ALTER TABLE ip_managers DROP CONSTRAINT IF EXISTS chk_device_type")
            .execute(pool)
            .await?;
        
        sqlx::query(
            r#"ALTER TABLE ip_managers ADD CONSTRAINT chk_device_type CHECK (device_type IN ('workstation', 'cabinet_position', 'switch', 'unknown'))"#
        )
        .execute(pool)
        .await?;
        
        sqlx::query(
            r#"ALTER TABLE ip_managers ADD CONSTRAINT chk_device_consistency CHECK (
                (device_type = 'switch' AND switch_id IS NOT NULL AND workstation_id IS NULL) OR
                (device_type = 'workstation' AND workstation_id IS NOT NULL AND position_id IS NULL AND switch_id IS NULL) OR
                (device_type = 'cabinet_position' AND position_id IS NOT NULL AND workstation_id IS NULL AND switch_id IS NULL) OR
                (device_type = 'unknown')
            )"#
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_switch_position_fields(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM information_schema.columns 
         WHERE table_name = 'switches' AND column_name = 'cabinet_id'"
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);
    
    if count == 0 {
        sqlx::query(
            "ALTER TABLE switches 
             ADD COLUMN cabinet_id UUID REFERENCES cabinets(id) ON DELETE SET NULL,
             ADD COLUMN start_u INTEGER,
             ADD COLUMN end_u INTEGER"
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r#"UPDATE switches s 
               SET cabinet_id = p.cabinet_id, 
                   start_u = p.start_u, 
                   end_u = p.end_u
               FROM positions p
               JOIN ip_managers im ON im.position_id = p.id 
               WHERE im.switch_id = s.id
               AND s.cabinet_id IS NULL"#
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_mac_history(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM information_schema.tables WHERE table_name = 'mac_history'"
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);
    
    if count == 0 {
        sqlx::query(
            r#"CREATE TABLE mac_history (
                id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                mac_address VARCHAR(20) NOT NULL,
                ip_address INET NOT NULL,
                ip_manager_id UUID NOT NULL REFERENCES ip_managers(id) ON DELETE CASCADE,
                device_type VARCHAR(20) NOT NULL,
                workstation_id UUID REFERENCES workstations(id) ON DELETE SET NULL,
                position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
                switch_id UUID REFERENCES switches(id) ON DELETE SET NULL,
                network_id UUID NOT NULL REFERENCES network_cidrs(id),
                change_type VARCHAR(20) NOT NULL DEFAULT 'update',
                created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
            )"#
        )
        .execute(pool)
        .await?;

        sqlx::query("CREATE INDEX idx_mac_history_mac_address ON mac_history(mac_address)")
            .execute(pool)
            .await?;
        
        sqlx::query("CREATE INDEX idx_mac_history_ip_address ON mac_history(ip_address)")
            .execute(pool)
            .await?;
        
        sqlx::query("CREATE INDEX idx_mac_history_ip_manager_id ON mac_history(ip_manager_id)")
            .execute(pool)
            .await?;
        
        sqlx::query("CREATE INDEX idx_mac_history_created_at ON mac_history(created_at)")
            .execute(pool)
            .await?;
    }

    let trigger_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_trigger WHERE tgname = 'trg_log_mac_address_change')"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    if !trigger_exists {
        sqlx::query(r#"
            CREATE OR REPLACE FUNCTION log_mac_address_change() RETURNS TRIGGER AS $$
            BEGIN
                IF TG_OP = 'INSERT' THEN
                    IF NEW.mac_address IS NOT NULL AND NEW.mac_address != '' THEN
                        INSERT INTO mac_history (
                            mac_address, ip_address, ip_manager_id, device_type,
                            workstation_id, position_id, switch_id, network_id, change_type
                        ) VALUES (
                            NEW.mac_address, NEW.ip_address, NEW.id, NEW.device_type,
                            NEW.workstation_id, NEW.position_id, NEW.switch_id, NEW.network_id, 'create'
                        );
                    END IF;
                ELSIF TG_OP = 'UPDATE' THEN
                    IF OLD.mac_address IS DISTINCT FROM NEW.mac_address THEN
                        IF NEW.mac_address IS NOT NULL AND NEW.mac_address != '' THEN
                            INSERT INTO mac_history (
                                mac_address, ip_address, ip_manager_id, device_type,
                                workstation_id, position_id, switch_id, network_id, change_type
                            ) VALUES (
                                NEW.mac_address, NEW.ip_address, NEW.id, NEW.device_type,
                                NEW.workstation_id, NEW.position_id, NEW.switch_id, NEW.network_id, 'update'
                            );
                        END IF;
                    END IF;
                END IF;
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        "#)
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TRIGGER trg_log_mac_address_change AFTER INSERT OR UPDATE OF mac_address ON ip_managers FOR EACH ROW EXECUTE FUNCTION log_mac_address_change()"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn create_views(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("DROP VIEW IF EXISTS ip_managers_with_details CASCADE")
        .execute(pool)
        .await?;

    sqlx::query(r#"
        CREATE VIEW ip_managers_with_details AS
        SELECT 
            imm.id,
            imm.workstation_id,
            imm.position_id,
            imm.switch_id,
            imm.switch_port_id,
            imm.device_type,
            CASE
                WHEN imm.device_type = 'switch' AND s.id IS NOT NULL THEN s.name::text
                WHEN w.id IS NOT NULL THEN w.name::text
                WHEN cp.id IS NOT NULL THEN cp.name::text
                ELSE '未知设备'
            END AS device_name,
            imm.network_id,
            CASE
                WHEN w.id IS NOT NULL THEN w.name::text
                ELSE NULL
            END AS workstation_name,
            CASE
                WHEN cp.id IS NOT NULL THEN cp.name::text
                ELSE NULL
            END AS cabinet_position_name,
            CASE
                WHEN s.id IS NOT NULL THEN s.name::text
                ELSE NULL
            END AS switch_name,
            sp.port_number::text AS switch_port_number,
            COALESCE(n.name, '未知')::text AS network_name,
            COALESCE(nt.name, '未知')::text AS network_region,
            host(imm.ip_address) as ip_address,
            imm.ip_version,
            imm.mac_address,
            imm.hostname,
            imm.status,
            imm.last_seen,
            imm.created_at,
            imm.updated_at
        FROM ip_managers imm
        LEFT JOIN workstations w ON imm.workstation_id = w.id
        LEFT JOIN rooms r ON w.room_id = r.id
        LEFT JOIN positions cp ON imm.position_id = cp.id
        LEFT JOIN cabinets c ON cp.cabinet_id = c.id
        LEFT JOIN switches s ON imm.switch_id = s.id
        LEFT JOIN switch_ports sp ON imm.switch_port_id = sp.id
        LEFT JOIN network_cidrs n ON imm.network_id = n.id
        LEFT JOIN network_regions nt ON n.network_region_id = nt.id
    "#)
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_crypto_functions(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS encryption_keys (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            key_name VARCHAR(50) UNIQUE NOT NULL,
            encryption_key TEXT NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )
    "#)
    .execute(pool)
    .await?;

    let key_path = "/etc/ipma/encryption.key";
    let key_base64 = if std::path::Path::new(key_path).exists() {
        if let Ok(key) = std::fs::read(key_path) {
            if key.len() == 32 {
                use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
                BASE64.encode(&key)
            } else {
                tracing::warn!("加密密钥长度不正确，将使用默认密钥");
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string()
            }
        } else {
            tracing::warn!("无法读取加密密钥文件，将使用默认密钥");
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string()
        }
    } else {
        tracing::warn!("加密密钥文件不存在，将使用默认密钥");
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string()
    };

    sqlx::query(r#"
        INSERT INTO encryption_keys (key_name, encryption_key) 
        VALUES ('system_configs_key', $1)
        ON CONFLICT (key_name) DO UPDATE SET encryption_key = EXCLUDED.encryption_key, updated_at = NOW()
    "#)
    .bind(&key_base64)
    .execute(pool)
    .await?;

    sqlx::query(r#"
        CREATE OR REPLACE FUNCTION encrypt_password(p_password TEXT)
        RETURNS TEXT AS $$
        DECLARE
            v_key BYTEA;
            v_padded BYTEA;
            v_encrypted BYTEA;
            v_padding_len INTEGER;
        BEGIN
            IF p_password IS NULL OR p_password = '' THEN
                RETURN p_password;
            END IF;
            
            SELECT decode(encryption_key, 'base64') INTO v_key 
            FROM encryption_keys 
            WHERE key_name = 'system_configs_key';
            
            IF v_key IS NULL THEN
                RAISE EXCEPTION 'Encryption key not found';
            END IF;
            
            v_padding_len := 16 - (length(p_password::BYTEA) % 16);
            v_padded := p_password::BYTEA || repeat(chr(v_padding_len), v_padding_len)::BYTEA;
            
            v_encrypted := encrypt(v_padded, v_key, 'aes-ecb/pad:none');
            
            RETURN encode(v_encrypted, 'base64');
        END;
        $$ LANGUAGE plpgsql STRICT IMMUTABLE;
    "#)
    .execute(pool)
    .await?;

    sqlx::query(r#"
        CREATE OR REPLACE FUNCTION decrypt_password(p_encrypted TEXT)
        RETURNS TEXT AS $$
        DECLARE
            v_key BYTEA;
            v_ciphertext BYTEA;
            v_decrypted BYTEA;
            v_padding_val INTEGER;
        BEGIN
            IF p_encrypted IS NULL OR p_encrypted = '' THEN
                RETURN p_encrypted;
            END IF;
            
            SELECT decode(encryption_key, 'base64') INTO v_key 
            FROM encryption_keys 
            WHERE key_name = 'system_configs_key';
            
            IF v_key IS NULL THEN
                RAISE EXCEPTION 'Encryption key not found';
            END IF;
            
            v_ciphertext := decode(p_encrypted, 'base64');
            
            v_decrypted := decrypt(v_ciphertext, v_key, 'aes-ecb/pad:none');
            
            v_padding_val := get_byte(v_decrypted, length(v_decrypted) - 1);
            
            IF v_padding_val > 0 AND v_padding_val <= 16 THEN
                v_decrypted := substring(v_decrypted, 1, length(v_decrypted) - v_padding_val);
            END IF;
            
            RETURN convert_from(v_decrypted, 'UTF8');
        END;
        $$ LANGUAGE plpgsql STRICT IMMUTABLE;
    "#)
    .execute(pool)
    .await?;

    sqlx::query(r#"
        DROP VIEW IF EXISTS switches_with_details;
        CREATE VIEW switches_with_details AS
        SELECT 
            s.id, s.name, s.network_region_id, s.network_id, s.model, s.vendor,
            s.location, s.snmp_version, 
            s.snmp_community,
            s.snmp_username, s.snmp_auth_protocol, 
            s.snmp_auth_password,
            s.snmp_priv_protocol, 
            s.snmp_priv_password,
            s.snmp_port,
            s.parent_switch_id, ps.name as parent_switch_name,
            s.parent_port_id, pp.port_number as parent_port_number,
            s.cabinet_id, c.name as cabinet_name,
            s.start_u, s.end_u,
            s.description,
            'switch' as device_type,
            host(im.ip_address) as ip_address,
            im.mac_address,
            s.created_at, s.updated_at
        FROM switches s
        LEFT JOIN switches ps ON s.parent_switch_id = ps.id
        LEFT JOIN switch_ports pp ON s.parent_port_id = pp.id
        LEFT JOIN cabinets c ON s.cabinet_id = c.id
        LEFT JOIN LATERAL (
            SELECT ip_address, mac_address 
            FROM ip_managers 
            WHERE switch_id = s.id AND device_type = 'switch' 
            LIMIT 1
        ) im ON true
    "#)
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_triggers(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(r#"
        CREATE OR REPLACE FUNCTION check_position_overlap() RETURNS TRIGGER AS $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM positions 
                WHERE cabinet_id = NEW.cabinet_id 
                AND id != NEW.id
                AND (
                    (NEW.start_u BETWEEN start_u AND end_u)
                    OR (NEW.end_u BETWEEN start_u AND end_u)
                    OR (start_u BETWEEN NEW.start_u AND NEW.end_u)
                    OR (end_u BETWEEN NEW.start_u AND NEW.end_u)
                )
            ) THEN
                RAISE EXCEPTION '机位U位范围重叠';
            END IF;
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
    "#)
    .execute(pool)
    .await?;

    let _ = sqlx::query(
        "DROP TRIGGER IF EXISTS trg_check_position_overlap ON positions"
    ).execute(pool).await;

    sqlx::query(
        "CREATE TRIGGER trg_check_position_overlap BEFORE INSERT OR UPDATE ON positions FOR EACH ROW EXECUTE FUNCTION check_position_overlap()"
    )
    .execute(pool)
    .await?;

    sqlx::query(r#"
        CREATE OR REPLACE FUNCTION check_switch_circular_dependency() RETURNS TRIGGER AS $$
        BEGIN
            IF NEW.parent_switch_id = NEW.id THEN
                RAISE EXCEPTION '交换机不能以自身为上级交换机';
            END IF;
            
            IF NEW.parent_switch_id IS NOT NULL THEN
                IF EXISTS (
                    WITH RECURSIVE switch_tree AS (
                        SELECT id, parent_switch_id FROM switches WHERE id = NEW.parent_switch_id
                        UNION ALL
                        SELECT s.id, s.parent_switch_id FROM switches s
                        JOIN switch_tree st ON s.id = st.parent_switch_id
                    )
                    SELECT 1 FROM switch_tree WHERE id = NEW.id
                ) THEN
                    RAISE EXCEPTION '交换机层级关系存在循环依赖';
                END IF;
            END IF;
            
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
    "#)
    .execute(pool)
    .await?;

    let _ = sqlx::query(
        "DROP TRIGGER IF EXISTS trg_check_switch_circular_dependency ON switches"
    ).execute(pool).await;

    sqlx::query(
        "CREATE TRIGGER trg_check_switch_circular_dependency BEFORE INSERT OR UPDATE OF parent_switch_id ON switches FOR EACH ROW EXECUTE FUNCTION check_switch_circular_dependency()"
    )
    .execute(pool)
    .await?;

    sqlx::query(r#"
        CREATE OR REPLACE FUNCTION log_mac_address_change() RETURNS TRIGGER AS $$
        BEGIN
            IF TG_OP = 'INSERT' THEN
                IF NEW.mac_address IS NOT NULL AND NEW.mac_address != '' THEN
                    INSERT INTO mac_history (
                        mac_address, ip_address, ip_manager_id, device_type,
                        workstation_id, position_id, switch_id, network_id, change_type
                    ) VALUES (
                        NEW.mac_address, NEW.ip_address, NEW.id, NEW.device_type,
                        NEW.workstation_id, NEW.position_id, NEW.switch_id, NEW.network_id, 'create'
                    );
                END IF;
            ELSIF TG_OP = 'UPDATE' THEN
                IF OLD.mac_address IS DISTINCT FROM NEW.mac_address THEN
                    IF NEW.mac_address IS NOT NULL AND NEW.mac_address != '' THEN
                        INSERT INTO mac_history (
                            mac_address, ip_address, ip_manager_id, device_type,
                            workstation_id, position_id, switch_id, network_id, change_type
                        ) VALUES (
                            NEW.mac_address, NEW.ip_address, NEW.id, NEW.device_type,
                            NEW.workstation_id, NEW.position_id, NEW.switch_id, NEW.network_id, 'update'
                        );
                    END IF;
                END IF;
            END IF;
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
    "#)
    .execute(pool)
    .await?;

    let _ = sqlx::query(
        "DROP TRIGGER IF EXISTS trg_log_mac_address_change ON ip_managers"
    ).execute(pool).await;

    sqlx::query(
        "CREATE TRIGGER trg_log_mac_address_change AFTER INSERT OR UPDATE OF mac_address ON ip_managers FOR EACH ROW EXECUTE FUNCTION log_mac_address_change()"
    )
    .execute(pool)
    .await?;

    Ok(())
}

// ==================== API 处理函数 ====================

pub async fn check_db_status(config: web::Data<Config>) -> Result<HttpResponse> {
    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "data": {
                    "connected": false,
                    "has_tables": false,
                    "required_tables_exist": false,
                    "has_data": false,
                    "error": e
                }
            })));
        }
    };

    let has_tables = match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public'",
    )
    .fetch_one(&pool)
    .await
    {
        Ok(count) => count > 0,
        Err(_) => false,
    };

    let required_tables_exist = check_required_tables_exist(&pool).await;

    let has_data = if required_tables_exist {
        check_has_data(&pool).await
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

pub async fn create_database_api(
    config: web::Data<Config>,
    req: web::Json<CreateDatabaseRequest>,
) -> Result<HttpResponse> {
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    if let Err(e) = verify_code(&req.verification) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e)));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(e)));
        }
    };

    let has_data = check_has_data(&pool).await;
    let mut backup_file: Option<String> = None;

    if has_data {
        info!("数据库不为空，正在备份...");
        match backup_database(&config.database).await {
            Ok(file) => {
                backup_file = Some(file);
                info!("备份完成，正在删除旧数据库...");
            }
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("备份失败: {}", e))));
            }
        }

        drop(pool);

        if let Err(e) = drop_database(&config.database).await {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("删除数据库失败: {}", e))));
        }
    } else {
        drop(pool);
        
        let postgres_url = format!(
            "postgres://{}:{}@{}:{}/postgres",
            config.database.username, config.database.password, config.database.host, config.database.port
        );
        let postgres_pool = match PgPool::connect(&postgres_url).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("连接PostgreSQL失败: {}", e))));
            }
        };
        
        let db_exists: bool = match sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)"
        )
        .bind(&config.database.database)
        .fetch_one(&postgres_pool)
        .await
        {
            Ok(exists) => exists,
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("检查数据库失败: {}", e))));
            }
        };
        
        if db_exists {
            info!("数据库存在但无数据，正在删除重建...");
            if let Err(e) = drop_database(&config.database).await {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("删除数据库失败: {}", e))));
            }
        }
    }

    if let Err(e) = create_database(&config.database).await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建数据库失败: {}", e))));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(e)));
        }
    };

    if let Err(e) = create_tables(&pool).await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建表失败: {}", e))));
    }

    info!("数据库创建成功");
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        CreateDatabaseResponse {
            backup_file,
            message: "数据库创建成功".to_string(),
        },
        "数据库创建成功",
    )))
}

pub async fn import_database_api(
    config: web::Data<Config>,
    req: web::Json<ImportDatabaseRequest>,
) -> Result<HttpResponse> {
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    if let Err(e) = verify_code(&req.verification) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e)));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(e)));
        }
    };

    let has_data = check_has_data(&pool).await;
    let mut backup_file: Option<String> = None;

    if has_data {
        info!("数据库不为空，正在备份...");
        match backup_database(&config.database).await {
            Ok(file) => {
                backup_file = Some(file);
                info!("备份完成，正在删除旧数据库...");
            }
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("备份失败: {}", e))));
            }
        }

        drop(pool);

        if let Err(e) = drop_database(&config.database).await {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("删除数据库失败: {}", e))));
        }
    } else {
        drop(pool);
        
        let postgres_url = format!(
            "postgres://{}:{}@{}:{}/postgres",
            config.database.username, config.database.password, config.database.host, config.database.port
        );
        let postgres_pool = match PgPool::connect(&postgres_url).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("连接PostgreSQL失败: {}", e))));
            }
        };
        
        let db_exists: bool = match sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)"
        )
        .bind(&config.database.database)
        .fetch_one(&postgres_pool)
        .await
        {
            Ok(exists) => exists,
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("检查数据库失败: {}", e))));
            }
        };
        
        if db_exists {
            info!("数据库存在但无数据，正在删除重建...");
            if let Err(e) = drop_database(&config.database).await {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("删除数据库失败: {}", e))));
            }
        }
    }

    if let Err(e) = create_database(&config.database).await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建数据库失败: {}", e))));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(e)));
        }
    };

    if let Err(e) = create_tables(&pool).await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建表失败: {}", e))));
    }

    if let Err(e) = validate_table_columns(&pool).await {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("数据库字段完整性校验失败: {}", e))));
    }

    info!("数据库导入成功");
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        CreateDatabaseResponse {
            backup_file,
            message: "数据库导入成功".to_string(),
        },
        "数据库导入成功",
    )))
}

pub async fn import_database_from_file(
    config: web::Data<Config>,
    mut payload: Multipart,
) -> Result<HttpResponse> {
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    let mut verification_code: Option<String> = None;
    let mut sql_file_path: Option<PathBuf> = None;

    std::fs::create_dir_all("/tmp/ipma_import")
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("创建临时目录失败: {}", e)))?;

    while let Some(mut field) = payload.try_next().await.map_err(actix_web::error::ErrorBadRequest)? {
        let content_disposition = field.content_disposition();
        let field_name = content_disposition.map(|cd| cd.get_name().unwrap_or("").to_string()).unwrap_or_default();

        if field_name == "verification" {
            let data = field.bytes(10 * 1024 * 1024).await
                .map_err(actix_web::error::ErrorBadRequest)?
                .map_err(actix_web::error::ErrorBadRequest)?;
            verification_code = Some(String::from_utf8_lossy(&data).to_string());
        } else if field_name == "sql_file" {
            let filename = content_disposition
                .and_then(|cd| cd.get_filename().map(|s| s.to_string()))
                .unwrap_or_else(|| "import.sql".to_string());
            let filepath = PathBuf::from(format!("/tmp/ipma_import/{}", filename));
            let mut f = std::fs::File::create(&filepath)
                .map_err(|e| actix_web::error::ErrorInternalServerError(format!("创建文件失败: {}", e)))?;
            
            let data = field.bytes(100 * 1024 * 1024).await
                .map_err(actix_web::error::ErrorBadRequest)?
                .map_err(actix_web::error::ErrorBadRequest)?;
            f.write_all(&data)
                .map_err(|e| actix_web::error::ErrorInternalServerError(format!("写入文件失败: {}", e)))?;
            sql_file_path = Some(filepath);
        }
    }

    let verification = match verification_code {
        Some(code) => code,
        None => return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("缺少验证码"))),
    };

    let sql_path = match sql_file_path {
        Some(path) => path,
        None => return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("缺少SQL文件"))),
    };

    if let Err(e) = verify_code(&verification) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e)));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(e)));
        }
    };

    let has_data = check_has_data(&pool).await;
    let mut backup_file: Option<String> = None;

    if has_data {
        info!("数据库不为空，正在备份...");
        match backup_database(&config.database).await {
            Ok(file) => {
                backup_file = Some(file);
                info!("备份完成，正在删除旧数据库...");
            }
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("备份失败: {}", e))));
            }
        }

        drop(pool);

        if let Err(e) = drop_database(&config.database).await {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("删除数据库失败: {}", e))));
        }
    } else {
        drop(pool);
        
        let postgres_url = format!(
            "postgres://{}:{}@{}:{}/postgres",
            config.database.username, config.database.password, config.database.host, config.database.port
        );
        let postgres_pool = match PgPool::connect(&postgres_url).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("连接PostgreSQL失败: {}", e))));
            }
        };
        
        let db_exists: bool = match sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)"
        )
        .bind(&config.database.database)
        .fetch_one(&postgres_pool)
        .await
        {
            Ok(exists) => exists,
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("检查数据库失败: {}", e))));
            }
        };
        
        if db_exists {
            info!("数据库存在但无数据，正在删除重建...");
            if let Err(e) = drop_database(&config.database).await {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("删除数据库失败: {}", e))));
            }
        }
    }

    if let Err(e) = create_database(&config.database).await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建数据库失败: {}", e))));
    }

    let output = std::process::Command::new("psql")
        .arg("-h")
        .arg(&config.database.host)
        .arg("-p")
        .arg(config.database.port.to_string())
        .arg("-U")
        .arg(&config.database.username)
        .arg("-d")
        .arg(&config.database.database)
        .arg("-f")
        .arg(&sql_path)
        .env("PGPASSWORD", &config.database.password)
        .output()
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("执行psql失败: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("导入SQL文件失败: {}", stderr))));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(e)));
        }
    };

    if let Err(e) = validate_table_columns(&pool).await {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("数据库字段完整性校验失败: {}", e))));
    }

    let _ = std::fs::remove_file(&sql_path);

    info!("数据库从文件导入成功");
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        CreateDatabaseResponse {
            backup_file,
            message: "数据库导入成功".to_string(),
        },
        "数据库导入成功",
    )))
}

pub async fn init_system(
    config: web::Data<Config>,
    req: web::Json<InitRequest>,
) -> Result<HttpResponse> {
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "验证错误: {:?}",
                e
            ))),
        );
    }

    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    if let Err(e) = verify_code(&req.verification) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e)));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(e)));
        }
    };

    let count: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
    {
        Ok(count) => count,
        Err(e) => {
            if let Some(db_err) = e.as_database_error() {
                if db_err.to_string().contains("UndefinedTable") {
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

    if count > 0 {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "数据库已有用户数据，请先通过新建或导入功能初始化数据库。",
        )));
    }

    if !check_required_tables_exist(&pool).await
        && let Err(e) = create_tables(&pool).await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建表失败: {}", e))));
    }

    let password_hash = match hash(&req.password, BCRYPT_COST) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!("密码哈希错误: {:?}", e);
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("密码哈希错误: {}", e))));
        }
    };

    let user_id = Uuid::new_v4();

    tracing::info!(
        "正在创建管理员用户，ID: {}, 用户名: {}, 邮箱: {}, 角色: {}",
        user_id,
        req.username,
        req.email,
        req.role
    );

    if let Err(e) = sqlx::query(
        r#"INSERT INTO users (id, username, password_hash, email, role, status) 
               VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(user_id)
    .bind(&req.username)
    .bind(&password_hash)
    .bind(&req.email)
    .bind(&req.role)
    .bind(true)
    .execute(&pool)
    .await
    {
        tracing::error!("创建管理员用户失败: {:?}", e);
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "创建管理员用户失败: {}",
                e
            ))),
        );
    }

    if let Err(e) = update_config_enabled(false) {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("更新配置失败: {}", e))));
    }

    info!(
        "系统初始化成功，管理员用户已创建: {}, 初始化模式已禁用",
        req.username
    );

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "系统初始化成功")))
}

pub async fn init_db(config: web::Data<Config>) -> Result<HttpResponse> {
    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(e)));
        }
    };

    let required_tables_exist = check_required_tables_exist(&pool).await;

    if !required_tables_exist {
        if let Err(e) = create_tables(&pool).await {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("创建表失败: {}", e))));
        }
        info!("数据库表结构初始化成功");
    } else {
        info!("数据库表结构已存在，跳过初始化");
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "数据库初始化成功")))
}

pub async fn clear_database(
    config: web::Data<Config>,
    verification: web::Query<serde_json::Value>,
) -> Result<HttpResponse> {
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    let verification_code = match verification.get("code") {
        Some(code) => code.as_str().unwrap_or(""),
        None => "",
    };

    if let Err(e) = verify_code(verification_code) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e)));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(e)));
        }
    };

    tracing::info!("正在通过API清空数据库...");
    if let Err(e) = drop_all_tables(&pool).await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("清空数据库失败: {}", e))));
    }

    tracing::info!("通过API清空数据库成功");
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "数据库清空成功")))
}

pub async fn check_init_status(config: web::Data<Config>) -> Result<HttpResponse> {
    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(_) => {
            return Ok(HttpResponse::Ok().json(serde_json::json!({
                "initialized": false,
                "version": env!("CARGO_PKG_VERSION"),
            })));
        }
    };

    let initialized = match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
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

pub async fn restart_program() -> Result<HttpResponse> {
    info!("收到重启程序请求，正在准备重启...");
    crate::system::config::trigger_service_restart()
}

pub async fn check_pgsql(config: web::Data<Config>) -> Result<HttpResponse> {
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

    let url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        config.database.username,
        config.database.password,
        config.database.host,
        config.database.port
    );

    match PgPool::connect(&url).await {
        Ok(_) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "installed": true,
                "running": true,
                "message": "PostgreSQL is running and connection is successful."
            })))
        }
        Err(e) => {
            let error_str = e.to_string();
            let running = !error_str.contains("connect")
                && !error_str.contains("timeout")
                && !error_str.contains("refused");
            
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "installed": true,
                "running": running,
                "error": error_str
            })))
        }
    }
}
