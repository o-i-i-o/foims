// IPMA Init Module - Database initialization and management
// Copyright (c) 2024-2025 oi-io <boss@oi-io.cc>
// SPDX-License-Identifier: MIT

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
    BCRYPT_COST, CreateDatabaseRequest, CreateDatabaseResponse, DatabaseConfig,
    ImportDatabaseRequest, InitRequest, VERIFICATION_CODE_EXPIRY_SECS, VerificationCode,
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
    import_database_api, import_database_from_file, init_db, init_system, restart_program,
};

/// Simplified API response for the init crate (no i18n dependency).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T, message: &str) -> Self {
        Self {
            success: true,
            message: message.to_string(),
            data: Some(data),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
        }
    }
}
