//! 日志模块（登录/操作/通知查询）与多语言日志初始化。
//!
//! 多语言日志架构：
//! - 控制台同时只输出一种语言，由 `[i18n].log_language` 控制（缺省 `en`）；
//! - 日志文件按语言分文件，每个文件只含一种语言，语言集合由
//!   `[i18n].logfiles_i18n_out` 控制（缺省跟随 `log_language`）；
//! - `logfiles_i18n_out` 必须是 `supported_languages` 的子集（且
//!   `log_language` 必须在 `supported_languages` 内），否则视为配置错误，
//!   控制台与日志文件整体回退为仅输出英文（en）日志；
//! - 各业务 crate 通过 `ipma_common::log_info!` 等宏输出带 key 的日志，
//!   宏为每种激活语言生成一条 target 为 `ipma_log::{lang}` 的事件，
//!   本模块按 target 将事件路由到对应语言的控制台/文件输出层。

pub mod login;
pub mod notification;
pub mod operation;

pub use login::get_login_logs;
pub use operation::get_operation_logs;

use std::fs;
use std::path::Path;

use ipma_common::{log_error, log_warn};
use time::format_description::BorrowedFormatItem;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt::time::LocalTime;
use tracing_subscriber::prelude::*;

use crate::config::I18nConfig;

/// 时间格式候选，按优先级排列，解析失败时逐级降级
const TIME_FORMATS: [&str; 3] = [
    "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6]",
    "[year]-[month]-[day] [hour]:[minute]:[second]",
    "[hour]:[minute]:[second]",
];

/// 日志系统解析后的语言配置。
#[derive(Debug, Clone)]
pub struct LogI18nConfig {
    /// 控制台日志语言（单值）
    pub console_lang: String,
    /// 日志文件语言集合（每个语言一个文件）
    pub file_langs: Vec<String>,
    /// 配置错误说明（英文，用于回退模式下提示），正常为 None
    pub config_error: Option<String>,
}

/// 解析并校验日志语言配置。
///
/// 校验失败（语言不在 supported_languages 内、文件语言集合为空等）时
/// 回退为「控制台 en + 文件 en」并记录错误原因。
pub fn resolve_log_i18n_config(i18n: Option<&I18nConfig>) -> LogI18nConfig {
    let fallback = || LogI18nConfig {
        console_lang: "en".to_string(),
        file_langs: vec!["en".to_string()],
        config_error: None,
    };

    let Some(cfg) = i18n else {
        // 整个 [i18n] 段缺失：默认英文控制台 + 英文日志文件
        return fallback();
    };

    let supported = &cfg.supported_languages;
    let console = cfg.log_language.trim().to_lowercase();

    let fail = |reason: String| LogI18nConfig {
        console_lang: "en".to_string(),
        file_langs: vec!["en".to_string()],
        config_error: Some(reason),
    };

    if supported.is_empty() {
        return fail("supported_languages is empty".to_string());
    }

    if !supported.iter().any(|s| s.eq_ignore_ascii_case(&console)) {
        return fail(format!(
            "log_language '{console}' is not in supported_languages {supported:?}"
        ));
    }

    // 文件语言缺省跟随控制台语言
    let requested = cfg
        .logfiles_i18n_out
        .clone()
        .unwrap_or_else(|| vec![console.clone()]);

    if requested.is_empty() {
        return fail("logfiles_i18n_out is empty".to_string());
    }

    if let Some(bad) = requested
        .iter()
        .find(|l| !supported.iter().any(|s| s.eq_ignore_ascii_case(l.trim())))
    {
        return fail(format!(
            "logfiles_i18n_out contains '{bad}' which is not in supported_languages {supported:?}"
        ));
    }

    // 去重并归一为小写，保持配置顺序
    let mut file_langs: Vec<String> = Vec::new();
    for lang in requested {
        let lang = lang.trim().to_lowercase();
        if !lang.is_empty() && !file_langs.contains(&lang) {
            file_langs.push(lang);
        }
    }

    LogI18nConfig {
        console_lang: console.to_lowercase(),
        file_langs,
        config_error: None,
    }
}

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
fn open_log_file(log_dir: &str, lang: &str) -> (Option<fs::File>, String, Vec<String>) {
    let today = chrono::Local::now().format("%Y-%m-%d-%H-%M");
    // 每个语言一个文件，文件内只含该语言的日志
    let primary = format!("{log_dir}/{today}.{lang}.log");
    let mut errors = Vec::new();

    match fs::File::create(&primary) {
        Ok(file) => return (Some(file), primary, errors),
        Err(e) => errors.push(format!("创建日志文件 {primary} 失败: {e}")),
    }

    let degraded = format!("ipma.{lang}.log");
    match fs::File::create(&degraded) {
        Ok(file) => (Some(file), degraded, errors),
        Err(e) => {
            errors.push(format!(
                "创建降级日志文件 {degraded} 也失败: {e}，该语言日志将只输出到控制台"
            ));
            (None, degraded, errors)
        }
    }
}

/// rust_i18n 翻译钩子：按语言查表并做 `{{name}}` 插值。
fn rust_i18n_translate(locale: &str, key: &str, params: &[(&str, &str)]) -> String {
    let template = rust_i18n::t!(key, locale = locale);
    let mut text = template.to_string();
    for (name, value) in params {
        text = text.replace(&format!("{{{{{name}}}}}"), value);
    }
    text
}

/// 为某个输出层构建过滤规则：
/// - 常规业务事件（target 以 `ipma` 开头）按既有级别放行；
/// - 多语言宏事件（target `ipma_log::{lang}`）只放行本层语言，其余语言全部关闭。
fn build_filter(allow_lang: &str, all_langs: &[String]) -> Targets {
    let mut targets = Targets::new()
        .with_target("ipma", LevelFilter::INFO)
        .with_target("axum", LevelFilter::WARN);
    for lang in all_langs {
        let target = format!("ipma_log::{lang}");
        if lang == allow_lang {
            targets = targets.with_target(target, LevelFilter::INFO);
        } else {
            targets = targets.with_target(target, LevelFilter::OFF);
        }
    }
    targets.with_default(LevelFilter::WARN)
}

/// 初始化多语言日志系统。
///
/// 返回各日志文件路径（含降级说明的记录会通过日志输出）。
/// 必须在加载配置之后、任何多语言日志宏调用之前执行。
pub fn setup_logging(i18n: Option<&I18nConfig>) -> Vec<String> {
    let log_cfg = resolve_log_i18n_config(i18n);

    // 激活语言 = 控制台语言 ∪ 文件语言（去重）
    let mut active: Vec<String> = Vec::new();
    for lang in std::iter::once(&log_cfg.console_lang).chain(log_cfg.file_langs.iter()) {
        if !active.contains(lang) {
            active.push(lang.clone());
        }
    }
    ipma_common::set_active_log_langs(active.clone());
    ipma_common::set_log_translate(rust_i18n_translate);

    let (timer, mut warnings) = build_timer();

    let app_name = env!("CARGO_PKG_NAME");
    let log_dir = format!("/var/log/{app_name}");

    if !Path::new(&log_dir).exists()
        && let Err(e) = fs::create_dir_all(&log_dir)
    {
        warnings.push(format!("创建日志目录 {log_dir} 失败: {e}，将尝试当前目录"));
    }

    // 每种文件语言打开一个独立日志文件：句柄交给订阅层，路径用于启动日志
    let mut appenders: Vec<(Option<fs::File>, String, String)> = Vec::new();
    for lang in &log_cfg.file_langs {
        let (file, path, file_warnings) = open_log_file(&log_dir, lang);
        warnings.extend(file_warnings);
        appenders.push((file, path, lang.clone()));
    }

    let all_langs = active.clone();
    let log_files: Vec<String> = appenders.iter().map(|(_, path, _)| path.clone()).collect();

    // 各语言的文件输出层收集为 Vec（Vec<L> 实现了 Layer），
    // 避免逐个 .with() 导致返回类型不断变化
    let file_layers: Vec<_> = appenders
        .into_iter()
        .filter_map(|(file, _path, lang)| {
            let file = file?;
            Some(
                tracing_subscriber::fmt::layer()
                    .with_writer(MakeWriterAdapter(file))
                    .with_timer(timer.clone())
                    .with_ansi(false)
                    .with_filter(build_filter(&lang, &all_langs)),
            )
        })
        .collect();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .with_timer(timer)
                .with_filter(build_filter(&log_cfg.console_lang, &all_langs)),
        )
        .with(file_layers)
        .init();

    // 订阅器就绪后，统一通过日志记录引导阶段的降级情况
    for warning in warnings {
        log_warn!("log.init_warning", detail = warning);
    }

    if let Some(reason) = &log_cfg.config_error {
        // 配置非法：按约定回退为仅输出英文日志，并明确提示原因
        log_error!("log.i18n_config_error", reason = reason);
    }

    log_files
}

/// 将 `fs::File` 适配为 tracing 的 MakeWriter（每次写入借用同一句柄）。
struct MakeWriterAdapter(fs::File);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for MakeWriterAdapter {
    type Writer = &'a fs::File;

    fn make_writer(&'a self) -> Self::Writer {
        &self.0
    }
}
