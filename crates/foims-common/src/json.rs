//! 统一 JSON 请求体提取器（AppJson）。

use axum::Json;
use axum::extract::{FromRequest, Request};
use serde::de::DeserializeOwned;

use crate::error::AppError;

/// 将 axum `JsonRejection` 转换为带 i18n key 的 `AppError`
fn map_json_rejection(rejection: axum::extract::rejection::JsonRejection) -> AppError {
    let err_str = rejection.to_string();
    let message = if err_str.contains("missing field") {
        let field_name = err_str.split('`').nth(1).unwrap_or("");
        crate::msg("server.common.missing_field").with("field", field_name)
    } else if err_str.contains("invalid type") {
        let field_info = err_str.split(": ").nth(1).unwrap_or("");
        crate::msg("server.common.invalid_type").with("info", field_info)
    } else {
        crate::msg("server.common.json_error").with("error", err_str)
    };

    AppError::Validation(message)
}

/// JSON 请求体提取器（带 i18n key 错误提示）
///
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
