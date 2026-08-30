//! FOIMS 数据库初始化与管理模块。
//!
//! 负责数据库结构创建、必需表/列校验、备份恢复与初始化向导 API。
//! 本项目不使用迁移框架：结构变更时直接执行 SQL 修改数据库，
//! 并同步完善本模块的结构创建代码（见 AGENTS.md）。

pub mod check;
pub mod config;
pub mod connection;
pub mod context;
pub mod error;
pub mod handlers;
pub mod operations;
pub mod schema;
pub mod types;
pub mod utils;
pub mod verification;

pub use context::InitContext;
pub use error::InitError;

pub use types::{
    BCRYPT_COST, CreateDatabaseRequest, CreateDatabaseResponse, DatabaseConfig, InitRequest,
    VERIFICATION_CODE_EXPIRY_SECS, VerificationCode,
};

pub use verification::{get_verification_code, verify_code};

pub use config::{get_backup_dir, update_config_enabled};

pub use check::{
    check_has_data, check_required_tables_exist, get_required_tables, get_table_columns,
    validate_table_columns,
};

pub use connection::ensure_database_and_schema;

pub use operations::{backup_database, create_database, drop_all_tables, drop_database};

pub use schema::create_tables;

pub use handlers::{
    check_db_status, check_init_status, check_pgsql, clear_database, create_database_api,
    import_database_from_file, init_db, init_system, restart_program,
};

/// 统一 API 响应结构（由 foims-common 提供，保持原有路径兼容）。
pub use foims_common::ApiResponse;
