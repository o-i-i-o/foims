//! 静态资源服务与统一 JSON 提取器（AppJson）。

use std::sync::OnceLock;

use axum::Json;
use axum::extract::{FromRequest, Request};
use serde::de::DeserializeOwned;

use crate::error::AppError;

pub const WEB_DIR_PATHS: [&str; 2] = ["/opt/ipma/web", "/usr/share/ipma/web"];

static WEB_DIR: OnceLock<&'static str> = OnceLock::new();

#[must_use]
pub fn get_web_dir() -> &'static str {
    WEB_DIR.get_or_init(|| {
        // 优先使用环境变量 IPMA_WEB_DIR（开发和生产环境通用）
        if let Ok(custom_dir) = std::env::var("IPMA_WEB_DIR")
            && !custom_dir.is_empty()
            && std::path::Path::new(&custom_dir).exists()
        {
            return Box::leak(custom_dir.into_boxed_str());
        }
        // 然后搜索默认部署路径
        for path in &WEB_DIR_PATHS {
            if std::path::Path::new(path).exists() {
                return path;
            }
        }
        "web"
    })
}

// ==================== JSON 提取器包装器 ====================

/// 将 axum `JsonRejection` 转换为带 i18n key 的 `AppError`
fn map_json_rejection(rejection: axum::extract::rejection::JsonRejection) -> AppError {
    let err_str = rejection.to_string();
    let message = if err_str.contains("missing field") {
        let field_name = err_str.split('`').nth(1).unwrap_or("");
        ipma_common::msg("server.common.missing_field").with("field", field_name)
    } else if err_str.contains("invalid type") {
        let field_info = err_str.split(": ").nth(1).unwrap_or("");
        ipma_common::msg("server.common.invalid_type").with("info", field_info)
    } else {
        ipma_common::msg("server.common.json_error").with("error", err_str)
    };

    AppError::Validation(message)
}

/// JSON 请求体提取器（带 i18n key 错误提示）
///
/// 替代 actix-web 中的 `web::JsonConfig::error_handler` 配置。
/// 所有需要解析 JSON 请求体的 handler 应使用 `AppJson<T>` 而不是 `Json<T>`。
pub struct AppJson<T>(pub T);

impl<T, S> FromRequest<S> for AppJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let json = Json::<T>::from_request(req, state)
            .await
            .map_err(map_json_rejection)?;
        Ok(AppJson(json.0))
    }
}
