//! 分页参数解析与统一分页响应构造。

use std::collections::HashMap;

pub const DEFAULT_PAGE: i64 = 1;
pub const DEFAULT_PAGE_SIZE: i64 = 20;
pub const MAX_PAGE_SIZE: i64 = 1000;

#[derive(Debug, Clone)]
pub struct Pagination {
    pub page: i64,
    pub page_size: i64,
    pub offset: i64,
}

impl Pagination {
    #[must_use]
    pub fn new(page: i64, page_size: i64) -> Self {
        let page = page.max(1);
        let page_size = page_size.clamp(1, MAX_PAGE_SIZE);
        let offset = page.saturating_sub(1).saturating_mul(page_size);
        Self {
            page,
            page_size,
            offset,
        }
    }

    #[must_use]
    pub fn from_query(query: &HashMap<String, String>) -> Self {
        let page = query
            .get("page")
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_PAGE);
        let page_size = query
            .get("page_size")
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_PAGE_SIZE);
        Self::new(page, page_size)
    }

    #[must_use]
    pub const fn total_pages(&self, total: i64) -> i64 {
        (total + self.page_size - 1) / self.page_size
    }
}

/// 构造统一的分页列表响应体。
///
/// 所有列表接口的分页数据统一使用 `items` 键（历史上部分接口用过
/// `data`，已废弃），配合 [`Pagination::total_pages`] 计算 `total_pages`。
pub fn paged_response<T: serde::Serialize>(
    items: Vec<T>,
    total: i64,
    pagination: &Pagination,
) -> serde_json::Value {
    serde_json::json!({
        "items": items,
        "total": total,
        "page": pagination.page,
        "page_size": pagination.page_size,
        "total_pages": pagination.total_pages(total)
    })
}
