pub mod login;
pub mod notification;
pub mod operation;

pub use login::get_login_logs;
pub use operation::get_operation_logs;

use std::fs;
use std::path::Path;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::LocalTime;
use tracing_subscriber::prelude::*;

pub fn setup_logging() -> String {
    let timer = LocalTime::new(
        time::format_description::parse(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6]",
        )
        .unwrap(),
    );

    let app_name = env!("CARGO_PKG_NAME");
    let log_dir = format!("/var/log/{}", app_name);

    if !Path::new(&log_dir).exists() {
        fs::create_dir_all(&log_dir).unwrap_or_else(|e| {
            eprintln!("创建日志目录失败: {}", e);
        });
    }

    let today = chrono::Local::now().format("%Y-%m-%d-%H-%M").to_string();
    let log_file_path = format!("{}/{}.log", log_dir, today);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .with_timer(timer.clone()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::fs::File::create(&log_file_path).unwrap_or_else(|e| {
                    eprintln!("创建日志文件失败: {}", e);
                    std::fs::File::create("ipma.log").unwrap()
                }))
                .with_timer(timer)
                .with_ansi(false),
        )
        .with(
            tracing_subscriber::filter::Targets::new()
                .with_target("ipma", LevelFilter::INFO)
                .with_target("actix_web", LevelFilter::WARN)
                .with_default(LevelFilter::WARN),
        )
        .init();

    log_file_path
}
