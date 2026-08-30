//! 静态资源服务。
//!
//! 统一 JSON 提取器（AppJson）已下沉至 foims-common，调用方直接
//! `use foims_common::AppJson`，本模块不再保留副本。

use std::sync::OnceLock;

pub const WEB_DIR_PATHS: [&str; 2] = ["/opt/foims/web", "/usr/share/foims/web"];

static WEB_DIR: OnceLock<&'static str> = OnceLock::new();

#[must_use]
pub fn get_web_dir() -> &'static str {
    WEB_DIR.get_or_init(|| {
        // 优先使用环境变量 FOIMS_WEB_DIR（开发和生产环境通用）
        if let Ok(custom_dir) = std::env::var("FOIMS_WEB_DIR")
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
