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

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- Pagination::new 边界 ----------

    #[test]
    fn test_new_normal_values() {
        // 常规取值：page/page_size 原样保留，offset = (page-1)*page_size
        let p = Pagination::new(2, 20);
        assert_eq!(p.page, 2);
        assert_eq!(p.page_size, 20);
        assert_eq!(p.offset, 20);
    }

    #[test]
    fn test_new_page_below_one_clamped() {
        // page <= 1 一律回退为 1，offset 为 0
        for page in [1, 0, -5] {
            let p = Pagination::new(page, 20);
            assert_eq!(p.page, 1, "page={page} 应被钳制为 1");
            assert_eq!(p.offset, 0);
        }
    }

    #[test]
    fn test_new_page_size_clamped_to_range() {
        // page_size 低于下限钳为 1，高于上限钳为 MAX_PAGE_SIZE
        let too_small = Pagination::new(3, 0);
        assert_eq!(too_small.page_size, 1);
        assert_eq!(too_small.offset, 2);

        let negative = Pagination::new(3, -100);
        assert_eq!(negative.page_size, 1);

        let too_big = Pagination::new(3, MAX_PAGE_SIZE + 1);
        assert_eq!(too_big.page_size, MAX_PAGE_SIZE);
        assert_eq!(too_big.offset, 2 * MAX_PAGE_SIZE);

        // 恰好等于上限时应原样保留
        let boundary = Pagination::new(3, MAX_PAGE_SIZE);
        assert_eq!(boundary.page_size, MAX_PAGE_SIZE);
    }

    #[test]
    fn test_new_offset_saturating_on_overflow() {
        // 极端入参下 offset 饱和于 i64::MAX 而不溢出
        let p = Pagination::new(i64::MAX, i64::MAX);
        assert_eq!(p.page, i64::MAX);
        assert_eq!(p.page_size, MAX_PAGE_SIZE);
        assert_eq!(p.offset, i64::MAX);
    }

    // ---------- from_query ----------

    #[test]
    fn test_from_query_empty_map_uses_defaults() {
        // 空查询参数回退默认值 page=1 / page_size=20
        let query = HashMap::new();
        let p = Pagination::from_query(&query);
        assert_eq!(p.page, DEFAULT_PAGE);
        assert_eq!(p.page_size, DEFAULT_PAGE_SIZE);
        assert_eq!(p.offset, 0);
    }

    #[test]
    fn test_from_query_invalid_values_fall_back() {
        // 非数字 / 数字过大超出 i64 均回退默认值
        let mut query = HashMap::new();
        query.insert("page".to_string(), "abc".to_string());
        query.insert("page_size".to_string(), "1x".to_string());
        let p = Pagination::from_query(&query);
        assert_eq!(p.page, DEFAULT_PAGE);
        assert_eq!(p.page_size, DEFAULT_PAGE_SIZE);

        // 数字过大超出 i64 同样回退
        let mut overflow = HashMap::new();
        overflow.insert("page".to_string(), "99999999999999999999999".to_string());
        let p2 = Pagination::from_query(&overflow);
        assert_eq!(p2.page, DEFAULT_PAGE);
    }

    #[test]
    fn test_from_query_negative_values_clamped_not_defaulted() {
        // 负数是合法 i64：可解析后被 new 钳制（page→1、page_size→1），而非回退默认值
        let mut query = HashMap::new();
        query.insert("page".to_string(), "-5".to_string());
        query.insert("page_size".to_string(), "-3".to_string());
        let p = Pagination::from_query(&query);
        assert_eq!(p.page, 1);
        assert_eq!(p.page_size, 1);
        assert_eq!(p.offset, 0);
    }

    #[test]
    fn test_from_query_valid_values_used() {
        // 合法输入被采纳：page=3、page_size=50 → offset=100
        let mut query = HashMap::new();
        query.insert("page".to_string(), "3".to_string());
        query.insert("page_size".to_string(), "50".to_string());
        let p = Pagination::from_query(&query);
        assert_eq!(p.page, 3);
        assert_eq!(p.page_size, 50);
        assert_eq!(p.offset, 100);
    }

    #[test]
    fn test_from_query_out_of_range_normalized() {
        // 合法数字但越界（0）由 new 归一化，不回退默认而是钳制
        let mut query = HashMap::new();
        query.insert("page".to_string(), "0".to_string());
        query.insert("page_size".to_string(), "5000".to_string());
        let p = Pagination::from_query(&query);
        assert_eq!(p.page, 1);
        assert_eq!(p.page_size, MAX_PAGE_SIZE);
    }

    // ---------- total_pages ----------

    #[test]
    fn test_total_pages_exact_division() {
        // 整除：100 / 20 = 5
        let p = Pagination::new(1, 20);
        assert_eq!(p.total_pages(100), 5);
        assert_eq!(p.total_pages(20), 1);
    }

    #[test]
    fn test_total_pages_rounds_up() {
        // 非整除向上取整：101 / 20 → 6；1 / 20 → 1
        let p = Pagination::new(1, 20);
        assert_eq!(p.total_pages(101), 6);
        assert_eq!(p.total_pages(1), 1);
        assert_eq!(p.total_pages(21), 2);
    }

    #[test]
    fn test_total_pages_zero_total() {
        // total 为 0 时总页数为 0
        let p = Pagination::new(1, 20);
        assert_eq!(p.total_pages(0), 0);
    }

    // ---------- paged_response ----------

    #[test]
    fn test_paged_response_keys_and_values() {
        // 响应体键固定为 items/total/page/page_size/total_pages，共 5 个
        let p = Pagination::new(2, 10);
        let body = paged_response(vec!["a", "b", "c"], 25, &p);

        let obj = body
            .as_object()
            .unwrap_or_else(|| panic!("响应体应为 JSON 对象"));
        assert_eq!(obj.len(), 5, "键集合应恰好为固定五键");
        assert_eq!(obj["items"], serde_json::json!(["a", "b", "c"]));
        assert_eq!(obj["items"].as_array().map(Vec::len), Some(3));
        assert_eq!(obj["total"], serde_json::json!(25));
        assert_eq!(obj["page"], serde_json::json!(2));
        assert_eq!(obj["page_size"], serde_json::json!(10));
        assert_eq!(obj["total_pages"], serde_json::json!(3));
    }

    #[test]
    fn test_paged_response_empty_items() {
        // 空列表：items 为空数组、total_pages 为 0
        let p = Pagination::new(1, 20);
        let body = paged_response(Vec::<i32>::new(), 0, &p);
        assert_eq!(body["items"], serde_json::json!([]));
        assert_eq!(body["total"], serde_json::json!(0));
        assert_eq!(body["total_pages"], serde_json::json!(0));
    }

    #[test]
    fn test_paged_response_serializable_struct_items() {
        // 泛型项支持任意可序列化结构体
        #[derive(serde::Serialize)]
        struct Row {
            id: i32,
        }
        let p = Pagination::new(1, 2);
        let body = paged_response(vec![Row { id: 1 }, Row { id: 2 }], 2, &p);
        assert_eq!(body["items"], serde_json::json!([{ "id": 1 }, { "id": 2 }]));
        assert_eq!(body["total_pages"], serde_json::json!(1));
    }
}
