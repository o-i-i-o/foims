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
        time::format_description::parse_borrowed::<2>(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6]",
        )
        .unwrap_or_else(|e| {
            eprintln!("警告: 无法解析时间格式，使用默认格式: {}", e);
            time::format_description::parse_borrowed::<2>(
                "[year]-[month]-[day] [hour]:[minute]:[second]",
            )
            .unwrap_or_else(|e2| {
                eprintln!("警告: 默认时间格式也无法解析: {}，使用最简格式", e2);
                time::format_description::parse_borrowed::<2>("[hour]:[minute]:[second]")
                    .unwrap_or_else(|e3| {
                        eprintln!("错误: 所有时间格式都无法解析: {}", e3);
                        time::format_description::parse_borrowed::<2>("[hour][minute][second]")
                            .unwrap_or_default()
                    })
            })
        }),
    );

    let app_name = env!("CARGO_PKG_NAME");
    let log_dir = format!("/var/log/{app_name}");

    if !Path::new(&log_dir).exists()
        && let Err(e) = fs::create_dir_all(&log_dir)
    {
        eprintln!("创建日志目录失败: {e}，将使用当前目录");
    }

    let today = chrono::Local::now().format("%Y-%m-%d-%H-%M").to_string();
    let log_file_path = format!("{log_dir}/{today}.log");

    let file_appender = match std::fs::File::create(&log_file_path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("创建日志文件失败: {e}，将使用默认日志文件");
            match std::fs::File::create("ipma.log") {
                Ok(file) => file,
                Err(e2) => {
                    eprintln!("创建默认日志文件也失败: {e2}，将只输出到控制台");
                    return String::from("ipma.log");
                }
            }
        }
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .with_timer(timer.clone()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_appender)
                .with_timer(timer)
                .with_ansi(false),
        )
        .with(
            tracing_subscriber::filter::Targets::new()
                .with_target("ipma", LevelFilter::INFO)
                .with_target("axum", LevelFilter::WARN)
                .with_default(LevelFilter::WARN),
        )
        .init();

    log_file_path
}
