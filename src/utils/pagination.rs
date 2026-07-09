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
        let offset = (page - 1) * page_size;
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
