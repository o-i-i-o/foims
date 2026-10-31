//! 运行日志实时推送（SSE）。
//!
//! 架构与多语言日志一致：`LiveLogLayer` 挂在订阅器链上，捕获所有放行到
//! 该层的事件（多语言事件按 target `foims_log::{lang}` 携带语言标记，
//! 普通 tracing 事件不带语言、对所有语言可见），序列化后投入进程级
//! broadcast 通道；SSE 端点为每条连接订阅通道并按请求语言过滤下发。
//!
//! 无订阅者时广播发送立即失败返回，日志路径零额外开销；慢消费者落后
//! 超过通道容量时由 `RecvError::Lagged` 感知，向前端下发跳过提示而非
//! 断流静默丢数据。

use std::convert::Infallible;
use std::fmt;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{HeaderName, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tracing::field::{Field, Visit};
use tracing_subscriber::filter::{LevelFilter, Targets};

use crate::app_state::AppState;
use foims_auth::AdminOrAuditorUser;
use foims_common::{AppError, msg};

/// 单条运行日志事件（SSE `log` 事件载荷）。
///
/// `lang` 为多语言事件的语言标记；普通 tracing 事件为 None（对所有
/// 语言连接可见）。`skipped` 非 None 时表示该事件为「接收端落后跳过」
/// 占位通知，前端按条数渲染提示文案。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct LiveLogEvent {
    pub ts: String,
    pub level: String,
    pub target: String,
    pub lang: Option<String>,
    pub message: String,
    pub skipped: Option<u64>,
}

/// 广播通道容量：落后超过该条数的连接收到「跳过 N 条」提示（不重放历史）。
const LIVE_LOG_CAPACITY: usize = 512;

/// 运行日志广播通道发送端（`setup_logging` 时创建，进程级单例）。
static LIVE_LOG_TX: OnceLock<broadcast::Sender<Arc<LiveLogEvent>>> = OnceLock::new();

/// 订阅运行日志广播；日志系统尚未初始化时返回 None。
pub fn subscribe_live_logs() -> Option<broadcast::Receiver<Arc<LiveLogEvent>>> {
    LIVE_LOG_TX.get().map(|tx| tx.subscribe())
}

/// 当前时间戳（本地时区，毫秒精度）。
fn now_ts() -> String {
    chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S%.3f")
        .to_string()
}

/// 事件字段收集器：`message` 字段为主文案，其余字段以 `k=v` 追加。
#[derive(Default)]
struct FieldCollector {
    message: Option<String>,
    fields: Vec<String>,
}

impl Visit for FieldCollector {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        } else {
            self.fields.push(format!("{}={value:?}", field.name()));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }
}

/// 运行日志捕获层：把经过过滤链放行的事件转为 [`LiveLogEvent`] 广播。
#[derive(Clone, Copy, Default)]
pub struct LiveLogLayer;

impl<S> tracing_subscriber::Layer<S> for LiveLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut collector = FieldCollector::default();
        event.record(&mut collector);

        let message = match collector.message {
            Some(message) if collector.fields.is_empty() => message,
            Some(message) => format!("{} {}", message, collector.fields.join(" ")),
            None => collector.fields.join(" "),
        };

        let event = LiveLogEvent {
            ts: now_ts(),
            level: event.metadata().level().as_str().to_string(),
            target: event.metadata().target().to_string(),
            lang: event
                .metadata()
                .target()
                .strip_prefix("foims_log::")
                .map(str::to_string),
            message,
            skipped: None,
        };

        if let Some(tx) = LIVE_LOG_TX.get() {
            let _ = tx.send(Arc::new(event));
        }
    }
}

/// 创建运行日志层并初始化广播通道（仅 `setup_logging` 调用一次）。
pub fn live_log_layer() -> LiveLogLayer {
    let (tx, _) = broadcast::channel(LIVE_LOG_CAPACITY);
    let _ = LIVE_LOG_TX.set(tx);
    LiveLogLayer
}

/// 运行日志层过滤规则：所有激活语言的多语言事件都放行（语言过滤推迟到
/// SSE 连接侧），普通业务事件与依赖库事件的口径与控制台层一致。
pub fn build_live_filter(all_langs: &[String], i18n_level: LevelFilter) -> Targets {
    let mut targets = Targets::new()
        .with_target("foims", LevelFilter::INFO)
        .with_target("axum", LevelFilter::WARN);
    for lang in all_langs {
        targets = targets.with_target(format!("foims_log::{lang}"), i18n_level);
    }
    targets.with_default(LevelFilter::WARN)
}

/// 解析 SSE 连接的日志语言：仅接受激活语言集合内的取值，缺省/非法时
/// 回落为集合首项（即控制台语言），保证 EventSource 不因参数问题断开。
fn resolve_stream_lang(query: &Query<std::collections::HashMap<String, String>>) -> String {
    let active = foims_common::active_log_langs();
    let requested = query.get("lang").map(|lang| lang.trim().to_lowercase());
    match requested {
        Some(lang) if active.iter().any(|item| item == &lang) => lang,
        _ => active.first().cloned().unwrap_or_else(|| "en".to_string()),
    }
}

/// 广播项转 SSE 事件：语言不匹配的返回 None 过滤掉。
fn sse_event(
    item: Result<Arc<LiveLogEvent>, BroadcastStreamRecvError>,
    lang: &str,
) -> Option<Result<Event, Infallible>> {
    match item {
        Ok(event) => {
            if event
                .lang
                .as_deref()
                .is_none_or(|event_lang| event_lang == lang)
                && let Ok(data) = serde_json::to_string(event.as_ref())
            {
                return Some(Ok(Event::default().event("log").data(data)));
            }
            None
        }
        // 慢消费者：以占位事件告知跳过条数，保持连接继续
        Err(BroadcastStreamRecvError::Lagged(n)) => {
            let event = LiveLogEvent {
                ts: now_ts(),
                level: "WARN".to_string(),
                target: "foims_live".to_string(),
                lang: None,
                message: String::new(),
                skipped: Some(n),
            };
            serde_json::to_string(&event)
                .ok()
                .map(|data| Ok(Event::default().event("log").data(data)))
        }
    }
}

/// GET /api/logs/live?lang=zh：SSE 实时推送运行日志。
///
/// 角色由 admin_guard_middleware 的 /api/logs/** 矩阵约束（admin 或
/// auditor），提取器二次校验；连接只接收建立之后产生的新日志。
pub async fn get_live_logs(
    State(_state): State<Arc<AppState>>,
    _viewer: AdminOrAuditorUser,
    query: Query<std::collections::HashMap<String, String>>,
) -> Result<Response, AppError> {
    let lang = resolve_stream_lang(&query);
    let Some(rx) = subscribe_live_logs() else {
        return Err(AppError::Internal(msg("server.logs.live_unavailable")));
    };

    let stream = BroadcastStream::new(rx).filter_map(move |item| sse_event(item, &lang));

    let sse = Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    );
    // X-Accel-Buffering: nginx 关闭该响应的缓冲（SSE 必须逐条下发）
    Ok((
        [
            (header::CACHE_CONTROL, "no-cache"),
            (HeaderName::from_static("x-accel-buffering"), "no"),
        ],
        sse,
    )
        .into_response())
}
