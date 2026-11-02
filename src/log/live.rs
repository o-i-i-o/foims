//! 运行日志：文件读取 + SSE 新日志通知。
//!
//! 架构：`setup_logging` 为每种文件语言打开「本轮」日志文件（文件名含启动
//! 时间，每次启动新建），路径登记到进程级注册表；`LiveLogLayer` 挂在订阅
//! 器链上，捕获放行事件后仅向 broadcast 通道发送一个「有新日志」信号
//! （不携带日志内容）。日志内容以文件为权威数据源：
//!
//! - `GET /api/logs/run?lang=xx` 返回本轮日志文件尾部（行对齐截尾），
//!   因此页面刷新、连接建立之前的启动日志都不会丢失；
//! - SSE `GET /api/logs/live` 只推送轻量 `notify` 事件，前端收到后防抖
//!   拉取 `/api/logs/run` 自动刷新。
//!
//! 无订阅者时广播发送立即失败返回，日志路径零额外开销；慢消费者落后超过
//! 通道容量时同样映射为 `notify`（文件是权威源，下一条新日志会再次通知，
//! 不存在静默丢数据）。

use std::collections::HashMap;
use std::convert::Infallible;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
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
use tracing_subscriber::filter::{LevelFilter, Targets};

use crate::app_state::AppState;
use foims_auth::AdminOrAuditorUser;
use foims_common::{AppError, msg, ok_json};

/// 广播通道容量：落后超过该条数的连接收到 Lagged，同样映射为 notify。
const LIVE_LOG_CAPACITY: usize = 512;

/// 新日志通知广播通道发送端（`setup_logging` 时创建，进程级单例）。
static LIVE_LOG_TX: OnceLock<broadcast::Sender<()>> = OnceLock::new();

/// 本轮各语言日志文件路径注册表（`setup_logging` 时登记，语言 → 路径）。
static RUN_LOG_FILES: OnceLock<HashMap<String, String>> = OnceLock::new();

/// 登记本轮各语言日志文件路径（仅 `setup_logging` 调用一次）。
pub fn set_run_log_files(files: HashMap<String, String>) {
    let _ = RUN_LOG_FILES.set(files);
}

/// 订阅新日志通知广播；日志系统尚未初始化时返回 None。
fn subscribe_live_logs() -> Option<broadcast::Receiver<()>> {
    LIVE_LOG_TX.get().map(|tx| tx.subscribe())
}

/// 运行日志捕获层：把经过过滤链放行的事件折叠为「有新日志」信号。
#[derive(Clone, Copy, Default)]
pub struct LiveLogLayer;

impl<S> tracing_subscriber::Layer<S> for LiveLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        _event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // 仅作通知信号，日志内容以文件为准；无订阅者时发送立即失败返回
        if let Some(tx) = LIVE_LOG_TX.get() {
            let _ = tx.send(());
        }
    }
}

/// 创建运行日志层并初始化通知通道（仅 `setup_logging` 调用一次）。
pub fn live_log_layer() -> LiveLogLayer {
    let (tx, _) = broadcast::channel(LIVE_LOG_CAPACITY);
    let _ = LIVE_LOG_TX.set(tx);
    LiveLogLayer
}

/// 运行日志层过滤规则：所有激活语言的多语言事件都放行（语言由读取接口
/// 的 lang 参数选择），普通事件与依赖库事件的口径与控制台层一致。
pub fn build_live_filter(all_langs: &[String], i18n_level: LevelFilter) -> Targets {
    let mut targets = Targets::new()
        .with_target("foims", LevelFilter::INFO)
        .with_target("axum", LevelFilter::WARN);
    for lang in all_langs {
        targets = targets.with_target(format!("foims_log::{lang}"), i18n_level);
    }
    targets.with_default(LevelFilter::WARN)
}

/// 解析请求的日志语言：仅接受激活语言集合内的取值，缺省/非法时回落为
/// 集合首项（即控制台语言），保证接口不因参数问题失败。
fn resolve_log_lang(query: &Query<std::collections::HashMap<String, String>>) -> String {
    let active = foims_common::active_log_langs();
    let requested = query.get("lang").map(|lang| lang.trim().to_lowercase());
    match requested {
        Some(lang) if active.iter().any(|item| item == &lang) => lang,
        _ => active.first().cloned().unwrap_or_else(|| "en".to_string()),
    }
}

/// 读取本轮日志文件的尾部行（最多 [`RUN_LOG_TAIL_BYTES`] 字节，起点对齐
/// 到行首防止截断出半行）。文件未登记（该语言降级到控制台）或尚未产生
/// 内容时返回空列表，属正常状态；读错误由调用方转为接口错误。
fn read_run_log_tail(path: &str) -> Vec<String> {
    const RUN_LOG_TAIL_BYTES: u64 = 256 * 1024;

    let Ok(mut file) = fs::File::open(path) else {
        return Vec::new();
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut bytes = Vec::new();
    if len > RUN_LOG_TAIL_BYTES
        && file
            .seek(SeekFrom::Start(len - RUN_LOG_TAIL_BYTES))
            .is_err()
    {
        return Vec::new();
    }
    if file.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }

    // 从文件中间截断时丢弃首个不完整行（对齐到下一个换行符）
    let start = if len > RUN_LOG_TAIL_BYTES {
        bytes
            .iter()
            .position(|&b| b == b'\n')
            .map_or(bytes.len(), |pos| pos + 1)
    } else {
        0
    };
    let text = String::from_utf8_lossy(&bytes[start..]);
    text.lines().map(str::to_string).collect()
}

/// GET /api/logs/run?lang=zh：读取本轮运行日志文件尾部。
///
/// 角色由 admin_guard_middleware 的 /api/logs/** 矩阵约束（admin 或
/// auditor），提取器二次校验；文件不存在视为无日志（空列表）而非错误。
pub async fn get_run_logs(
    State(_state): State<Arc<AppState>>,
    _viewer: AdminOrAuditorUser,
    query: Query<std::collections::HashMap<String, String>>,
) -> Result<Response, AppError> {
    let lang = resolve_log_lang(&query);
    let lines = RUN_LOG_FILES
        .get()
        .and_then(|files| files.get(&lang))
        .map(|path| read_run_log_tail(path))
        .unwrap_or_default();

    Ok(ok_json(
        serde_json::json!({ "lines": lines }),
        "server.logs.run_retrieved",
    ))
}

/// 广播项转 SSE 事件：新日志通知统一折叠为轻量 `notify` 事件。
fn notify_event(item: Result<(), BroadcastStreamRecvError>) -> Option<Result<Event, Infallible>> {
    // Lagged（消费过慢）同样映射为 notify：文件是权威源，刷新即补齐。
    // data 必须携带非空载荷：SSE 规范规定 data 缓冲为空时事件整体丢弃
    // 不派发，而 axum 对空字符串不写 data 字段，`data("")` 会导致前端
    // EventSource 永远收不到 notify、日志页不自动刷新。
    let _ = item;
    Some(Ok(Event::default().event("notify").data("1")))
}

/// GET /api/logs/live：SSE 推送「有新日志」通知。
///
/// 角色由 admin_guard_middleware 的 /api/logs/** 矩阵约束（admin 或
/// auditor），提取器二次校验；连接只通知建立之后产生的新日志，日志内容
/// 由前端拉取 /api/logs/run 获得（语言由读取接口的 lang 参数决定）。
pub async fn get_live_logs(
    State(_state): State<Arc<AppState>>,
    _viewer: AdminOrAuditorUser,
) -> Result<Response, AppError> {
    let Some(rx) = subscribe_live_logs() else {
        return Err(AppError::Internal(msg("server.logs.live_unavailable")));
    };

    let stream = BroadcastStream::new(rx).filter_map(notify_event);

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
