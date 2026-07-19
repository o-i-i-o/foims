use std::sync::OnceLock;

use actix_web::error::{ErrorBadRequest, JsonPayloadError};
use actix_web::{Error, HttpRequest, HttpResponse};

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

async fn validate_static_path(file_path: &str, base_dir: &str) -> Option<std::path::PathBuf> {
    let resolved = std::path::PathBuf::from(file_path);
    let canonical = match tokio::fs::canonicalize(&resolved).await {
        Ok(c) => c,
        Err(_) => return None,
    };
    let canonical_base = match tokio::fs::canonicalize(base_dir).await {
        Ok(c) => c,
        Err(_) => return None,
    };
    if canonical.starts_with(&canonical_base) {
        Some(canonical)
    } else {
        None
    }
}

pub async fn serve_json(req: HttpRequest) -> Result<HttpResponse, Error> {
    let path = req.path();
    let file_path = path.trim_start_matches("/static/");
    if file_path.contains("..") {
        return Ok(HttpResponse::NotFound().finish());
    }
    let web_dir = get_web_dir();
    let full_path = format!("{web_dir}/static/{file_path}");
    let validated = match validate_static_path(&full_path, web_dir).await {
        Some(p) => p,
        None => return Ok(HttpResponse::NotFound().finish()),
    };

    let content = match tokio::fs::read_to_string(&validated).await {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(HttpResponse::NotFound().finish());
        }
        Err(e) => {
            tracing::error!("读取静态文件失败: {} - {}", full_path, e);
            return Err(actix_web::error::ErrorInternalServerError("无法读取文件"));
        }
    };

    Ok(HttpResponse::Ok()
        .content_type("application/json; charset=utf-8")
        .body(content))
}

pub fn json_error_handler(err: JsonPayloadError, _req: &HttpRequest) -> Error {
    let err_str = err.to_string();
    let friendly_message = if err_str.contains("missing field") {
        let field_name = err_str.split('`').nth(1).unwrap_or("");
        format!("缺少必填字段: {field_name}")
    } else if err_str.contains("invalid type") {
        let field_info = err_str.split(": ").nth(1).unwrap_or("");
        format!("字段类型错误: {field_info}")
    } else {
        format!("JSON格式错误: {err_str}")
    };

    ErrorBadRequest(
        serde_json::json!({"success": false, "message": friendly_message, "data": null}),
    )
}
