//! 多语言系统日志基础设施。
//!
//! 设计要点：
//! - 后端各 crate 通过 `log_info!` / `log_warn!` / `log_error!` / `log_debug!`
//!   宏输出带 key 的日志，宏按「当前激活的日志语言集合」为每种语言生成
//!   一条 tracing 事件，事件 target 为 `ipma_log::{lang}`；
//! - 订阅器按 target 过滤：控制台层只放行控制台语言对应的 target，
//!   每个日志文件层只放行自身语言的 target，从而实现
//!   「控制台单语言、每个日志文件单语言」；
//! - 翻译能力通过钩子函数注入（由二进制 crate 在启动时注册，
//!   挂接其 rust_i18n 翻译表），ipma-common 自身不持有任何文案；
//! - 普通的（未走宏的）tracing 事件不受影响，会出现在控制台与全部
//!   日志文件中，作为迁移期的兼容回退。

use std::sync::{Mutex, OnceLock};

/// 翻译钩子：(语言, 消息 key, 动态参数) -> 翻译后的文本。
type TranslateFn = fn(&str, &str, &[(&str, &str)]) -> String;

/// 兜底翻译：未注册钩子时仅回显 key（参数以 k=v 形式附加，便于排查）。
fn fallback_translate(locale: &str, key: &str, params: &[(&str, &str)]) -> String {
    if params.is_empty() {
        return key.to_string();
    }
    let joined = params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{locale}] {key} ({joined})")
}

static TRANSLATE_FN: OnceLock<TranslateFn> = OnceLock::new();
static ACTIVE_LANGS: OnceLock<Vec<String>> = OnceLock::new();

/// 注册翻译钩子（由持有翻译资源的二进制 crate 在启动时调用一次）。
pub fn set_log_translate(f: TranslateFn) {
    let _ = TRANSLATE_FN.set(f);
}

/// 设置激活的日志语言集合（控制台语言 ∪ 各日志文件语言，去重）。
pub fn set_active_log_langs(langs: Vec<String>) {
    let _ = ACTIVE_LANGS.set(langs);
}

/// 默认激活语言（未初始化时使用）。
static DEFAULT_LANGS: OnceLock<Vec<String>> = OnceLock::new();

/// 当前激活的日志语言集合；未初始化时默认英文。
pub fn active_log_langs() -> &'static [String] {
    ACTIVE_LANGS
        .get()
        .unwrap_or_else(|| DEFAULT_LANGS.get_or_init(|| vec!["en".to_string()]))
        .as_slice()
}

/// 按 key 翻译为指定语言的日志文本（内部使用，参数值为已字符串化的值）。
pub fn translate_for(locale: &str, key: &str, params: &[(&str, String)]) -> String {
    let params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
    match TRANSLATE_FN.get() {
        Some(f) => f(locale, key, &params),
        None => fallback_translate(locale, key, &params),
    }
}

/// 哑 callsite：仅为多语言事件的 `message` 字段提供稳定标识符。
///
/// 不注册进全局注册表（不参与 Interest 缓存），字段过滤按事件元数据的
/// target 在订阅器侧动态完成。
struct I18nMessageCallsite;

impl tracing::callsite::Callsite for I18nMessageCallsite {
    fn set_interest(&self, _interest: tracing::subscriber::Interest) {}
    fn metadata(&self) -> &tracing::Metadata<'_> {
        &I18N_MESSAGE_META
    }
}

static I18N_MESSAGE_CS: I18nMessageCallsite = I18nMessageCallsite;

static I18N_MESSAGE_META: tracing::Metadata<'static> = tracing::Metadata::new(
    "ipma_i18n_message",
    "ipma_i18n",
    tracing::Level::INFO,
    None,
    None,
    None,
    tracing::field::FieldSet::new(
        &["message"],
        tracing::callsite::Identifier(&I18N_MESSAGE_CS),
    ),
    tracing::metadata::Kind::EVENT,
);

/// 取（或首次创建）指定 `(语言, 级别)` 组合的事件元数据。
///
/// tracing 宏要求 target 为编译期字面量，而多语言事件的 target
/// （`ipma_log::{lang}`）是运行时值，因此走 `Event::dispatch` 底层 API。
/// 元数据按组合缓存：每个 `(语言, 级别)` 仅分配一次，泄漏量有界
/// （语言数 × 4 个级别）。
fn event_metadata(level: tracing::Level, lang: &str) -> &'static tracing::Metadata<'static> {
    static META_CACHE: OnceLock<
        Mutex<std::collections::HashMap<String, &'static tracing::Metadata<'static>>>,
    > = OnceLock::new();

    let cache_key = format!("{lang}/{}", level.as_str());
    let cache = META_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(map) = cache.lock()
        && let Some(meta) = map.get(&cache_key)
    {
        return meta;
    }

    let target: &'static str = Box::leak(format!("ipma_log::{lang}").into_boxed_str());
    let name: &'static str =
        Box::leak(format!("ipma_i18n_event_{lang}_{}", level.as_str()).into_boxed_str());
    let meta: &'static tracing::Metadata<'static> = Box::leak(Box::new(tracing::Metadata::new(
        name,
        target,
        level,
        None,
        None,
        None,
        tracing::field::FieldSet::new(
            &["message"],
            tracing::callsite::Identifier(&I18N_MESSAGE_CS),
        ),
        tracing::metadata::Kind::EVENT,
    )));
    if let Ok(mut map) = cache.lock() {
        map.insert(cache_key, meta);
    }
    meta
}

/// 以 target `ipma_log::{lang}` 派发一条携带单 `message` 字段的事件
/// （日志宏内部使用；订阅器按 target 路由到对应语言的控制台/日志文件层）。
///
/// 不能用 `Event::dispatch`：它跳过 `enabled` 预检，而 Filtered 层依赖
/// 该预检设置的线程本地门禁状态做逐层过滤。此处复刻 `event!` 宏的
/// 完整派发流程（enabled 预检 → 构造事件 → 派发）。
fn dispatch_event(level: tracing::Level, lang: &str, text: &str) {
    let meta = event_metadata(level, lang);
    let Some(field) = meta.fields().field("message") else {
        return;
    };
    let values = [(&field, Some(&text as &dyn tracing::Value))];
    let value_set = meta.fields().value_set(&values);
    tracing::dispatcher::get_default(|dispatch| {
        if dispatch.enabled(meta) {
            let event = tracing::event::Event::new(meta, &value_set);
            dispatch.event(&event);
        }
    });
}

/// 派发一条多语言 info 事件（`log_info!` 宏的内部实现）。
pub fn emit_info(lang: &str, text: &str) {
    dispatch_event(tracing::Level::INFO, lang, text);
}

/// 派发一条多语言 warn 事件（`log_warn!` 宏的内部实现）。
pub fn emit_warn(lang: &str, text: &str) {
    dispatch_event(tracing::Level::WARN, lang, text);
}

/// 派发一条多语言 error 事件（`log_error!` 宏的内部实现）。
pub fn emit_error(lang: &str, text: &str) {
    dispatch_event(tracing::Level::ERROR, lang, text);
}

/// 派发一条多语言 debug 事件（`log_debug!` 宏的内部实现）。
pub fn emit_debug(lang: &str, text: &str) {
    dispatch_event(tracing::Level::DEBUG, lang, text);
}

/// 输出一条多语言 info 日志。
///
/// 用法：`log_info!("log.key")` 或 `log_info!("log.key", name = value, count = n)`，
/// 消息模板中的 `{{name}}` 占位符由各语言的翻译资源插值。
#[macro_export]
macro_rules! log_info {
    ($key:expr $(, $pname:ident = $pval:expr)* $(,)?) => {{
        let params: Vec<(&str, String)> = vec![$((stringify!($pname), ($pval).to_string())),*];
        let key: &str = &$key;
        for lang in $crate::active_log_langs() {
            let text = $crate::translate_for(lang, key, &params);
            $crate::emit_info(lang, &text);
        }
    }};
}

/// 输出一条多语言 warn 日志（用法同 [`log_info!`]）。
#[macro_export]
macro_rules! log_warn {
    ($key:expr $(, $pname:ident = $pval:expr)* $(,)?) => {{
        let params: Vec<(&str, String)> = vec![$((stringify!($pname), ($pval).to_string())),*];
        let key: &str = &$key;
        for lang in $crate::active_log_langs() {
            let text = $crate::translate_for(lang, key, &params);
            $crate::emit_warn(lang, &text);
        }
    }};
}

/// 输出一条多语言 error 日志（用法同 [`log_info!`]）。
#[macro_export]
macro_rules! log_error {
    ($key:expr $(, $pname:ident = $pval:expr)* $(,)?) => {{
        let params: Vec<(&str, String)> = vec![$((stringify!($pname), ($pval).to_string())),*];
        let key: &str = &$key;
        for lang in $crate::active_log_langs() {
            let text = $crate::translate_for(lang, key, &params);
            $crate::emit_error(lang, &text);
        }
    }};
}

/// 输出一条多语言 debug 日志（用法同 [`log_info!`]）。
#[macro_export]
macro_rules! log_debug {
    ($key:expr $(, $pname:ident = $pval:expr)* $(,)?) => {{
        let params: Vec<(&str, String)> = vec![$((stringify!($pname), ($pval).to_string())),*];
        let key: &str = &$key;
        for lang in $crate::active_log_langs() {
            let text = $crate::translate_for(lang, key, &params);
            $crate::emit_debug(lang, &text);
        }
    }};
}
