use std::sync::OnceLock;

use actix_web::error::{ErrorBadRequest, JsonPayloadError};
use actix_web::{Error, HttpRequest, HttpResponse, get, web};

pub const WEB_DIR_PATHS: [&str; 2] = ["/opt/ipma/web", "/usr/share/ipma/web"];

static WEB_DIR: OnceLock<&'static str> = OnceLock::new();

#[must_use]
pub fn get_web_dir() -> &'static str {
    WEB_DIR.get_or_init(|| {
        for path in &WEB_DIR_PATHS {
            if std::path::Path::new(path).exists() {
                return path;
            }
        }
        "web"
    })
}

#[must_use]
pub fn get_static_path(sub_path: &str) -> String {
    let web_dir = get_web_dir();
    format!("{web_dir}/static/{sub_path}")
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

#[get("/static/js/i18n/{file}")]
pub async fn serve_i18n_file(path: web::Path<String>) -> Result<HttpResponse, Error> {
    let file = path.into_inner();
    if file.contains("..") || file.contains('/') || file.contains('\\') {
        return Ok(HttpResponse::NotFound().finish());
    }
    let web_dir = get_web_dir();
    let file_path = format!("{web_dir}/static/js/i18n/{file}");
    let validated = match validate_static_path(&file_path, web_dir).await {
        Some(p) => p,
        None => return Ok(HttpResponse::NotFound().finish()),
    };

    let content = match tokio::fs::read_to_string(&validated).await {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(HttpResponse::NotFound().finish());
        }
        Err(_) => {
            return Err(actix_web::error::ErrorInternalServerError("无法读取文件"));
        }
    };

    Ok(HttpResponse::Ok()
        .content_type("application/json; charset=utf-8")
        .body(content))
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

pub async fn https_redirect_handler(req: HttpRequest) -> HttpResponse {
    let connection_info = req.connection_info();
    let host = connection_info.host();
    let path = req.uri().path();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();

    let domain = host.split(':').next().unwrap_or("localhost");
    if domain.contains('/') || domain.contains('@') {
        return HttpResponse::BadRequest().finish();
    }

    let https_port = req
        .app_data::<actix_web::web::Data<crate::app_state::AppState>>()
        .and_then(|s| s.config.server.https_port)
        .unwrap_or(443);

    let port_suffix = if https_port == 443 {
        String::new()
    } else {
        format!(":{https_port}")
    };

    HttpResponse::Found()
        .insert_header((
            actix_web::http::header::LOCATION,
            format!("https://{domain}{port_suffix}{path}{query}"),
        ))
        .finish()
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
