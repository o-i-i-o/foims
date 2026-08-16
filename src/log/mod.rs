//! 日志模块（登录/操作/通知）。

pub mod login;
pub mod notification;
pub mod operation;

pub use login::get_login_logs;
pub use operation::get_operation_logs;

use std::fs;
use std::path::Path;
use time::format_description::BorrowedFormatItem;
use tracing::warn;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::LocalTime;
use tracing_subscriber::prelude::*;

/// 时间格式候选，按优先级排列，解析失败时逐级降级
const TIME_FORMATS: [&str; 3] = [
    "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6]",
    "[year]-[month]-[day] [hour]:[minute]:[second]",
    "[hour]:[minute]:[second]",
];

/// 构建本地时间格式化器，返回使用的格式化器与各级降级原因
fn build_timer() -> (LocalTime<Vec<BorrowedFormatItem<'static>>>, Vec<String>) {
    let mut errors = Vec::new();
    for fmt in TIME_FORMATS {
        match time::format_description::parse_borrowed::<2>(fmt) {
            Ok(parsed) => return (LocalTime::new(parsed), errors),
            Err(e) => errors.push(format!("时间格式 {fmt} 解析失败: {e}")),
        }
    }
    // 全部候选失败：退化为最简拼接格式，保证日志仍可输出
    let parsed =
        time::format_description::parse_borrowed::<2>("[hour][minute][second]").unwrap_or_default();
    errors.push(String::from("所有时间格式候选均解析失败，已退化为最简格式"));
    (LocalTime::new(parsed), errors)
}

/// 依次尝试主日志文件与当前目录降级文件，返回 (文件句柄, 路径, 降级原因)
fn open_log_file(log_dir: &str) -> (Option<fs::File>, String, Vec<String>) {
    let today = chrono::Local::now().format("%Y-%m-%d-%H-%M");
    let primary = format!("{log_dir}/{today}.log");
    let mut errors = Vec::new();

    match fs::File::create(&primary) {
        Ok(file) => return (Some(file), primary, errors),
        Err(e) => errors.push(format!("创建日志文件 {primary} 失败: {e}")),
    }

    match fs::File::create("ipma.log") {
        Ok(file) => (Some(file), String::from("ipma.log"), errors),
        Err(e) => {
            errors.push(format!(
                "创建降级日志文件 ipma.log 也失败: {e}，将只输出到控制台"
            ));
            (None, String::from("ipma.log"), errors)
        }
    }
}

pub fn setup_logging() -> String {
    let (timer, mut warnings) = build_timer();

    let app_name = env!("CARGO_PKG_NAME");
    let log_dir = format!("/var/log/{app_name}");

    if !Path::new(&log_dir).exists()
        && let Err(e) = fs::create_dir_all(&log_dir)
    {
        warnings.push(format!("创建日志目录 {log_dir} 失败: {e}，将尝试当前目录"));
    }

    let (file_appender, log_file_path, file_warnings) = open_log_file(&log_dir);
    warnings.extend(file_warnings);

    let filter = tracing_subscriber::filter::Targets::new()
        .with_target("ipma", LevelFilter::INFO)
        .with_target("axum", LevelFilter::WARN)
        .with_default(LevelFilter::WARN);

    match file_appender {
        Some(file) => {
            tracing_subscriber::registry()
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(std::io::stdout)
                        .with_timer(timer.clone()),
                )
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(file)
                        .with_timer(timer)
                        .with_ansi(false),
                )
                .with(filter)
                .init();
        }
        None => {
            tracing_subscriber::registry()
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(std::io::stdout)
                        .with_timer(timer),
                )
                .with(filter)
                .init();
        }
    }

    // 订阅器就绪后，统一通过 tracing 记录引导阶段的降级情况
    for warning in warnings {
        warn!("{warning}");
    }

    log_file_path
}
