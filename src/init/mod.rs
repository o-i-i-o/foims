pub mod types;
pub mod verification;
pub mod config;
pub mod check;
pub mod connection;
pub mod operations;
pub mod schema;
pub mod handlers;

pub use types::{
    BCRYPT_COST,
    VERIFICATION_CODE_EXPIRY_SECS,
    VerificationCode,
    InitRequest,
    CreateDatabaseRequest,
    ImportDatabaseRequest,
    CreateDatabaseResponse,
};

pub use verification::{
    VERIFICATION_CODE,
    verify_code,
    get_verification_code,
};

pub use config::{
    get_backup_dir,
    update_config_enabled,
};

pub use check::{
    get_required_tables,
    get_table_columns,
    check_required_tables_exist,
    validate_table_columns,
    check_has_data,
};

pub use connection::ensure_database_and_schema;

pub use operations::{
    backup_database,
    drop_database,
    create_database,
    drop_all_tables,
};

pub use schema::create_tables;

pub use handlers::{
    check_db_status,
    create_database_api,
    import_database_api,
    import_database_from_file,
    init_system,
    init_db,
    clear_database,
    check_init_status,
    restart_program,
    check_pgsql,
};
