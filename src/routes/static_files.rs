use actix_web::{Error, HttpResponse, get, web, HttpRequest};
use actix_web::error::{ErrorBadRequest, JsonPayloadError};
use std::path::PathBuf;

// 前端资源目录搜索路径（按优先级）
pub const WEB_DIR_PATHS: [&str; 2] = [
    "/opt/ipma/web",        // 应用目录（生产环境）
    "/usr/share/ipma/web",  // 系统目录（备用）
];

// 获取 web 目录的绝对路径
pub fn get_web_dir() -> &'static str {
    for path in &WEB_DIR_PATHS {
        if std::path::Path::new(path).exists() {
            return path;
        }
    }
    // 默认返回当前目录
    "web"
}

pub fn get_static_path(sub_path: &str) -> String {
    let web_dir = get_web_dir();
    format!("{}/static/{}", web_dir, sub_path)
}

// 处理国际化文件的 API 端点
#[get("/static/js/i18n/{file}")]
pub async fn serve_i18n_file(path: web::Path<String>) -> Result<HttpResponse, Error> {
    let file = path.into_inner();
    let web_dir = get_web_dir();
    let file_path = format!("{}/static/js/i18n/{}", web_dir, file);
    let path_buf = PathBuf::from(&file_path);

    // 检查文件是否存在
    if !path_buf.exists() {
        return Ok(HttpResponse::NotFound().finish());
    }

    // 读取文件内容
    let content = std::fs::read_to_string(&file_path)
        .map_err(|_| actix_web::error::ErrorInternalServerError("无法读取文件"))?;

    // 设置正确的 Content-Type
    Ok(HttpResponse::Ok()
        .content_type("application/json; charset=utf-8")
        .body(content))
}

// 定义 JSON 文件处理器
pub async fn serve_json(req: HttpRequest) -> Result<HttpResponse, Error> {
    // 获取请求的路径
    let path = req.path();
    // 移除开头的 /static
    let file_path = path.trim_start_matches("/static/");
    let web_dir = get_web_dir();
    let full_path = format!("{}/static/{}", web_dir, file_path);

    // 读取文件
    let content = match std::fs::read_to_string(&full_path) {
        Ok(content) => content,
        Err(_) => return Ok(HttpResponse::NotFound().finish()),
    };

    // 返回带有正确 Content-Type 的响应
    Ok(HttpResponse::Ok()
        .content_type("application/json; charset=utf-8")
        .body(content))
}

// 自动HTTPS重定向处理函数
pub async fn https_redirect_handler(req: HttpRequest) -> HttpResponse {
    let connection_info = req.connection_info();
    let host = connection_info.host();
    let path = req.uri().path();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();

    // 从主机中提取域名，移除端口
    let domain = host.split(':').next().unwrap_or("localhost");

    HttpResponse::Found()
        .insert_header((
            actix_web::http::header::LOCATION,
            format!("https://{}:443{}{}", domain, path, query),
        ))
        .finish()
}

// 定义JSON错误处理器
pub fn json_error_handler(err: JsonPayloadError, _req: &HttpRequest) -> Error {
    // 解析错误信息，返回更友好的错误提示
    let err_str = err.to_string();
    let friendly_message = if err_str.contains("missing field") {
        let field_name = err_str.split("`").nth(1).unwrap_or("");
        format!("缺少必填字段: {}", field_name)
    } else if err_str.contains("invalid type") {
        let field_info = err_str.split(": ").nth(1).unwrap_or("");
        format!("字段类型错误: {}", field_info)
    } else {
        format!("JSON格式错误: {}", err_str)
    };

    ErrorBadRequest(
        serde_json::json!({"success": false, "message": friendly_message, "data": null}),
    )
}
