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

pub async fn https_redirect_handler(req: HttpRequest) -> HttpResponse {
    let connection_info = req.connection_info();
    let host = connection_info.host();
    let path = req.uri().path();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();

    let requested_domain = host.split(':').next().unwrap_or("localhost");
    if requested_domain.contains('/') || requested_domain.contains('@') {
        return HttpResponse::BadRequest().finish();
    }

    let app_state = req.app_data::<actix_web::web::Data<crate::app_state::AppState>>();
    let https_port = app_state
        .and_then(|s| s.config.server.https_port)
        .unwrap_or(443);

    let trusted_domain = app_state
        .map(|s| {
            let mut allowed: Vec<&str> = vec![s.config.server.host.as_str()];
            if let Some(ipv6) = &s.config.server.host_ipv6 {
                allowed.push(ipv6.as_str());
            }
            if !s.config.server.public_url.is_empty() {
                let url = s.config.server.public_url.as_str();
                let after_scheme = url.split("://").nth(1).unwrap_or(url);
                let host_part = after_scheme.split('/').next().unwrap_or(after_scheme);
                let host_only = host_part.split(':').next().unwrap_or(host_part);
                if !host_only.is_empty() {
                    allowed.push(host_only);
                }
            }
            if allowed.contains(&requested_domain) {
                requested_domain.to_string()
            } else {
                s.config.server.host.clone()
            }
        })
        .unwrap_or_else(|| requested_domain.to_string());

    let port_suffix = if https_port == 443 {
        String::new()
    } else {
        format!(":{https_port}")
    };

    HttpResponse::Found()
        .insert_header((
            actix_web::http::header::LOCATION,
            format!("https://{trusted_domain}{port_suffix}{path}{query}"),
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
