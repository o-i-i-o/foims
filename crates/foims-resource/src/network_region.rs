//! 网络区域（network_regions）资源：区域 CRUD 与区域 CIDR 数组校验。
//!
//! 由 network.rs 拆分而来（纯移动）：networks 与 network_regions 是
//! 两个独立资源域，共用网段写入 advisory lock。

//! 网络区域与网段管理。

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use foims_auth::meta::{RequestMeta, log_op_best_effort};
use foims_common::AppError;
use foims_common::AppJson;
use foims_common::DbProvider;
use foims_common::pagination::{Pagination, paged_response};
use foims_common::{log_error, msg};
use foims_models::{NetworkRegion, NetworkRegionCreate, NetworkRegionUpdate};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

use super::network::acquire_network_cidr_write_lock;

pub async fn get_network_regions<P: DbProvider>(
    State(state): State<Arc<P>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();

    let sort_by = query.get("sort_by").cloned().unwrap_or_default();
    let sort_order = query.get("sort_order").cloned().unwrap_or_default();

    // ORDER BY 白名单，未匹配时回落默认序，避免注入。
    // CIDR 列取数组首元素参与排序，空数组（NULL）固定排末尾
    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY name DESC",
        ("name", _) => "ORDER BY name ASC",
        ("ipv4_cidrs", "desc") => "ORDER BY ipv4_cidrs[1] DESC NULLS LAST",
        ("ipv4_cidrs", _) => "ORDER BY ipv4_cidrs[1] ASC NULLS LAST",
        ("ipv6_cidrs", "desc") => "ORDER BY ipv6_cidrs[1] DESC NULLS LAST",
        ("ipv6_cidrs", _) => "ORDER BY ipv6_cidrs[1] ASC NULLS LAST",
        ("created_at", "asc") => "ORDER BY created_at ASC",
        _ => "ORDER BY created_at DESC",
    };

    let total: i64 = if search.is_empty() {
        sqlx::query_scalar("SELECT COUNT(*) FROM network_regions")
            .fetch_one(&state.pool()?.get_conn())
            .await?
    } else {
        let pattern = foims_common::net::escape_like(&search);
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM network_regions WHERE name ILIKE $1 OR description ILIKE $1",
        )
        .bind(&pattern)
        .fetch_one(&state.pool()?.get_conn())
        .await?
    };

    let base_select = "SELECT id, name, description,
                    (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv4_cidrs) AS d) as ipv4_cidrs,
                    (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv6_cidrs) AS d) as ipv6_cidrs,
                    created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions";

    let network_regions = if search.is_empty() {
        let sql = format!("{base_select} {order_clause} LIMIT $1 OFFSET $2");
        sqlx::query_as::<_, NetworkRegion>(sqlx::AssertSqlSafe(sql))
            .bind(page_size)
            .bind(offset)
            .fetch_all(&state.pool()?.get_conn())
            .await?
    } else {
        let pattern = foims_common::net::escape_like(&search);
        let sql = format!(
            "{base_select} WHERE name ILIKE $1 OR description ILIKE $1 {order_clause} LIMIT $2 OFFSET $3"
        );
        sqlx::query_as::<_, NetworkRegion>(sqlx::AssertSqlSafe(sql))
            .bind(&pattern)
            .bind(page_size)
            .bind(offset)
            .fetch_all(&state.pool()?.get_conn())
            .await?
    };

    Ok(foims_common::ok_json(
        paged_response(network_regions, total, &pagination),
        "server.network.region_fetched",
    ))
}

/// 区域 CIDR 数组入库前校验：逐条 [`foims_common::net::validate_cidr`]
/// 格式检查（非法直接 422，而非绑定后撞 PG 22P02 报 500），
/// 并拒绝数组内重复条目（重复项对包含语义无增益，只会污染展示）。
fn validate_region_cidr_array(
    cidrs: Option<&Vec<String>>,
    param: &'static str,
) -> Result<(), AppError> {
    let Some(list) = cidrs else {
        return Ok(());
    };
    let mut seen = std::collections::HashSet::with_capacity(list.len());
    for cidr in list {
        if cidr.trim().is_empty() || !foims_common::net::validate_cidr(cidr) {
            return Err(AppError::Validation(
                msg("server.common.invalid_param").with("param", param),
            ));
        }
        if !seen.insert(cidr.trim()) {
            return Err(AppError::Validation(
                msg("server.common.invalid_param").with("param", param),
            ));
        }
    }
    Ok(())
}

pub async fn create_network_region<P: DbProvider>(
    State(state): State<Arc<P>>,
    meta: RequestMeta,
    AppJson(req): AppJson<NetworkRegionCreate>,
) -> Result<Response, AppError> {
    req.validate()?;
    // CIDR 数组前置校验：非法格式/数组内重复在绑定 CIDR[] 前拦截
    validate_region_cidr_array(req.ipv4_cidrs.as_ref(), "ipv4_cidrs")?;
    validate_region_cidr_array(req.ipv6_cidrs.as_ref(), "ipv6_cidrs")?;

    // 重名预检与写入放同一事务，避免 TOCTOU
    let mut tx = state.pool()?.get_conn().begin().await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_regions WHERE name = $1")
            .bind(&req.name)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_some() {
        return Err(AppError::Conflict(msg("server.network.region_name_exists")));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO network_regions (id, name, description, ipv4_cidrs, ipv6_cidrs, created_at, updated_at)
         VALUES ($1, $2, $3, $4::CIDR[], $5::CIDR[], $6, $7)",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.ipv4_cidrs)
    .bind(&req.ipv6_cidrs)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // 并发写入竞态兜底：network_regions.name 唯一冲突映射为 409
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict(msg("server.network.region_name_exists"));
        }
        AppError::from(e)
    })?;

    tx.commit().await?;

    let network_region = NetworkRegion {
        id,
        name: req.name.clone(),
        description: req.description.clone(),
        ipv4_cidrs: req.ipv4_cidrs.clone(),
        ipv6_cidrs: req.ipv6_cidrs.clone(),
        created_at: now,
        updated_at: now,
    };

    let details = serde_json::json!({
        "name": network_region.name,
        "description": network_region.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "create",
        "network_region",
        Some(&id),
        &details,
    )
    .await;

    Ok(foims_common::ok_json(
        network_region,
        "server.network.region_created",
    ))
}

pub async fn get_network_region<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let network_region = sqlx::query_as::<_, NetworkRegion>(
        "SELECT id, name, description,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv4_cidrs) AS d) as ipv4_cidrs,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv6_cidrs) AS d) as ipv6_cidrs,
                created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions WHERE id = $1"
    ).bind(id)
    .fetch_optional(&state.pool()?.get_conn()).await?
    .ok_or_else(|| AppError::NotFound(msg("server.network.region_not_found")))?;

    Ok(foims_common::ok_json(
        network_region,
        "server.network.region_fetched",
    ))
}

pub async fn update_network_region<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<NetworkRegionUpdate>,
) -> Result<Response, AppError> {
    req.validate()?;
    // CIDR 数组前置校验（含数组内重复检查）：收缩区域等更新场景
    // 在绑定 CIDR[] 前拦截非法值
    validate_region_cidr_array(req.ipv4_cidrs.as_ref(), "ipv4_cidrs")?;
    validate_region_cidr_array(req.ipv6_cidrs.as_ref(), "ipv6_cidrs")?;

    // 预检、写入与更新后复查放同一事务，任一失败整体回滚
    let mut tx = state.pool()?.get_conn().begin().await?;
    // 与网段写互斥：并发「收缩区域 + 在该区域建网段」会基于各自快照
    // 同时通过检查，产生落在区域范围外的网段
    acquire_network_cidr_write_lock(&mut tx).await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_regions WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_none() {
        return Err(AppError::NotFound(msg("server.network.region_not_found")));
    }

    if let Some(name) = &req.name {
        let duplicate: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM network_regions WHERE name = $1 AND id != $2",
        )
        .bind(name)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

        if duplicate.is_some() {
            return Err(AppError::Conflict(msg("server.network.region_name_exists")));
        }
    }

    let now = Utc::now();

    sqlx::query(
        "UPDATE network_regions SET
         name = COALESCE($1, name),
         description = COALESCE($2, description),
         ipv4_cidrs = COALESCE($3::CIDR[], ipv4_cidrs),
         ipv6_cidrs = COALESCE($4::CIDR[], ipv6_cidrs),
         updated_at = $5
         WHERE id = $6",
    )
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.ipv4_cidrs)
    .bind(&req.ipv6_cidrs)
    .bind(now)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // 并发写入竞态兜底：network_regions.name 唯一冲突映射为 409
        if let sqlx::Error::Database(ref db_err) = e
            && db_err.is_unique_violation()
        {
            return AppError::Conflict(msg("server.network.region_name_exists"));
        }
        AppError::from(e)
    })?;

    // 区域 CIDR 更新后复查：其下所有网段的 CIDR 仍须被新区域范围包含，
    // 越界则整体回滚（422），避免收缩区域后下辖网段悬空
    if req.ipv4_cidrs.is_some() || req.ipv6_cidrs.is_some() {
        let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT ipv4_cidr::TEXT, ipv6_cidr::TEXT FROM network_cidrs WHERE network_region_id = $1",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        for (ipv4, ipv6) in &rows {
            if let Some(cidr) = ipv4
                && !foims_common::net::cidr_belongs_to_region(
                    cidr,
                    req.ipv4_cidrs.as_deref().unwrap_or_default(),
                )
            {
                if let Err(e) = tx.rollback().await {
                    log_error!("log.network.region_update_rollback_failed", error = e);
                }
                return Err(AppError::Validation(
                    msg("server.common.invalid_param").with("param", "ipv4_cidrs"),
                ));
            }
            if let Some(cidr) = ipv6
                && !foims_common::net::cidr_belongs_to_region(
                    cidr,
                    req.ipv6_cidrs.as_deref().unwrap_or_default(),
                )
            {
                if let Err(e) = tx.rollback().await {
                    log_error!("log.network.region_update_rollback_failed", error = e);
                }
                return Err(AppError::Validation(
                    msg("server.common.invalid_param").with("param", "ipv6_cidrs"),
                ));
            }
        }
    }

    tx.commit().await?;

    let network_region = sqlx::query_as::<_, NetworkRegion>(
        "SELECT id, name, description,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv4_cidrs) AS d) as ipv4_cidrs,
                (SELECT COALESCE(json_agg(text(d)), '[]') FROM unnest(ipv6_cidrs) AS d) as ipv6_cidrs,
                created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions WHERE id = $1"
    ).bind(id)
    .fetch_one(&state.pool()?.get_conn()).await?;

    let details = serde_json::json!({
        "name": network_region.name,
        "description": network_region.description
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "update",
        "network_region",
        Some(&id),
        &details,
    )
    .await;

    Ok(foims_common::ok_json(
        network_region,
        "server.network.region_updated",
    ))
}

pub async fn delete_network_region<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
) -> Result<Response, AppError> {
    // 存在性/占用检查与 DELETE 包进同一事务，避免检查后被并发创建
    // 的网段绕过（并发下以 FK 500 收场的 TOCTOU）
    let mut tx = state.pool()?.get_conn().begin().await?;
    acquire_network_cidr_write_lock(&mut tx).await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM network_regions WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_none() {
        return Err(AppError::NotFound(msg("server.network.region_not_found")));
    }

    let network_count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM network_cidrs WHERE network_region_id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;

    if network_count > 0 {
        return Err(AppError::Validation(msg("server.network.region_in_use")));
    }

    sqlx::query("DELETE FROM network_regions WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "network_region_id": id.to_string()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "delete",
        "network_region",
        Some(&id),
        &details,
    )
    .await;

    Ok(foims_common::ok_json((), "server.network.region_deleted"))
}

#[cfg(test)]
mod tests {
    //! 本模块覆盖 network.rs 处理器所依赖的纯校验链
    //!（CIDR 格式/类型、网关归属、区域包含），不触及数据库。

    use foims_common::AppError;
    use foims_common::net::{
        cidr_belongs_to_region, get_cidr_type, validate_cidr, validate_gateway_in_cidr,
    };

    // ==================== CIDR 格式校验（IPv4） ====================

    #[test]
    fn test_validate_cidr_ipv4_valid() {
        // 主机位全 0 的合法网段（含前缀边界 0 与 32）
        for cidr in [
            "10.0.0.0/8",
            "192.168.1.0/24",
            "172.16.0.0/12",
            "0.0.0.0/0",
            "10.1.2.3/32",
            // 无掩码的裸地址按 /32 处理（与 PostgreSQL CIDR 语义一致）
            "10.2.3.4",
        ] {
            assert!(validate_cidr(cidr), "合法 IPv4 CIDR {cidr} 应通过");
        }
    }

    #[test]
    fn test_validate_cidr_ipv4_invalid() {
        // 主机位非 0、前缀越界、格式错误均应拒绝
        for cidr in [
            "192.168.1.1/24", // 主机位非 0
            "10.0.0.0/33",    // 前缀超过 32
            "10.0.0.0/-1",    // 负前缀
            "10.0.0.0/",      // 掩码为空
            "300.0.0.0/8",    // 非法地址段
            "",               // 空串
            "abc/24",         // 非数字地址
        ] {
            assert!(!validate_cidr(cidr), "非法 IPv4 CIDR {cidr:?} 应被拒绝");
        }
    }

    #[test]
    fn test_validate_cidr_ipv6_valid() {
        for cidr in [
            "2001:db8::/32",
            "2001:db8:1::/48",
            "fe80::/10",
            "::/0",
            "2001:db8::1/128",
            // 无掩码的裸地址按 /128 处理
            "2001:db8::2",
        ] {
            assert!(validate_cidr(cidr), "合法 IPv6 CIDR {cidr} 应通过");
        }
    }

    #[test]
    fn test_validate_cidr_ipv6_invalid() {
        for cidr in [
            "2001:db8::1/64", // 主机位非 0
            "::1/0",          // 全地址下主机位非 0
            "::/129",         // 前缀超过 128
            "gg::/16",        // 非法十六进制
        ] {
            assert!(!validate_cidr(cidr), "非法 IPv6 CIDR {cidr:?} 应被拒绝");
        }
    }

    // ==================== CIDR 类型识别 ====================

    #[test]
    fn test_get_cidr_type_v4_v6_and_none() {
        assert_eq!(get_cidr_type("10.0.0.0/8"), Some("ipv4"));
        assert_eq!(get_cidr_type("192.168.1.0/30"), Some("ipv4"));
        assert_eq!(get_cidr_type("2001:db8::/32"), Some("ipv6"));
        assert_eq!(get_cidr_type("fe80::/64"), Some("ipv6"));
        // 非法输入（即便主机位非 0）类型解析只看地址族
        assert_eq!(get_cidr_type("192.168.1.1/24"), Some("ipv4"));
        // 完全非法输入返回 None
        assert_eq!(get_cidr_type("invalid"), None);
        assert_eq!(get_cidr_type(""), None);
        assert_eq!(get_cidr_type("10.0.0.0/33"), None);
    }

    #[test]
    fn test_get_cidr_type_family_mismatch_guard() {
        // create_network 的校验链要求「格式合法 + 地址族匹配」同时成立：
        // IPv4 CIDR 提交到 ipv6 槽位 / 反之均应视为不匹配
        assert_ne!(get_cidr_type("10.0.0.0/8"), Some("ipv6"));
        assert_ne!(get_cidr_type("2001:db8::/32"), Some("ipv4"));
    }

    // ==================== 网关与网段归属校验 ====================

    #[test]
    fn test_gateway_in_cidr_ipv4_ok() {
        // 网关在网段内且地址族一致应通过
        assert!(
            validate_gateway_in_cidr(Some("192.168.1.1"), Some("192.168.1.0/24"), "ipv4").is_ok()
        );
        // 网段边界上的首/末地址同样合法
        assert!(validate_gateway_in_cidr(Some("10.0.0.255"), Some("10.0.0.0/24"), "ipv4").is_ok());
        // 容忍 PostgreSQL INET 文本自带的掩码后缀
        assert!(
            validate_gateway_in_cidr(Some("192.168.1.1/32"), Some("192.168.1.0/24"), "ipv4")
                .is_ok()
        );
        // 空白网关视为未提供，直接通过
        assert!(validate_gateway_in_cidr(Some("  "), Some("192.168.1.0/24"), "ipv4").is_ok());
        assert!(validate_gateway_in_cidr(None, None, "ipv4").is_ok());
    }

    #[test]
    fn test_gateway_in_cidr_ipv4_rejected() {
        // 网段外
        let err = validate_gateway_in_cidr(Some("192.168.2.1"), Some("192.168.1.0/24"), "ipv4")
            .err()
            .unwrap_or_else(|| panic!("网段外网关应被拒绝"));
        assert!(matches!(err, AppError::Validation(_)));
        // 格式非法
        assert!(
            validate_gateway_in_cidr(Some("not-an-ip"), Some("192.168.1.0/24"), "ipv4").is_err()
        );
        // 地址族不匹配：IPv6 网关填入 ipv4 槽位
        assert!(
            validate_gateway_in_cidr(Some("2001:db8::1"), Some("192.168.1.0/24"), "ipv4").is_err(),
            "地址族不匹配应被拒绝"
        );
        // 提供网关但缺少同族 CIDR
        assert!(validate_gateway_in_cidr(Some("192.168.1.1"), None, "ipv4").is_err());
    }

    #[test]
    fn test_gateway_in_cidr_ipv6() {
        assert!(
            validate_gateway_in_cidr(Some("2001:db8::1"), Some("2001:db8::/64"), "ipv6").is_ok()
        );
        // 网段外
        assert!(
            validate_gateway_in_cidr(Some("2001:db9::1"), Some("2001:db8::/64"), "ipv6").is_err()
        );
        // IPv4 网关填入 ipv6 槽位
        assert!(
            validate_gateway_in_cidr(Some("192.168.1.1"), Some("2001:db8::/64"), "ipv6").is_err(),
            "地址族不匹配应被拒绝"
        );
        // /128 边界：网关恰好为唯一地址
        assert!(
            validate_gateway_in_cidr(Some("2001:db8::1"), Some("2001:db8::1/128"), "ipv6").is_ok()
        );
    }

    // ==================== 网段与区域 CIDR 包含关系 ====================

    #[test]
    fn test_cidr_belongs_to_region_empty_region_allows_all() {
        // 区域未定义 CIDR 时不做限制
        assert!(cidr_belongs_to_region("192.168.1.0/24", &[]));
        assert!(cidr_belongs_to_region("2001:db8:1::/48", &[]));
    }

    #[test]
    fn test_cidr_belongs_to_region_membership() {
        let region = vec!["10.0.0.0/8".to_string(), "2001:db8::/32".to_string()];
        // 子网（前缀更大）属于任一区域 CIDR 即通过
        assert!(cidr_belongs_to_region("10.1.0.0/16", &region));
        assert!(cidr_belongs_to_region("10.1.2.0/24", &region));
        assert!(cidr_belongs_to_region("2001:db8:1::/48", &region));
        // 与区域 CIDR 完全相同（前缀相等）不算子网
        assert!(
            !cidr_belongs_to_region("10.0.0.0/8", &region),
            "前缀相等不满足 << 语义"
        );
        // 区域外的网段
        assert!(!cidr_belongs_to_region("192.168.1.0/24", &region));
        assert!(!cidr_belongs_to_region("2001:db9::/32", &region));
    }

    #[test]
    fn test_cidr_belongs_to_region_cross_family_rejected() {
        // IPv4 与 IPv6 不能互相包含
        let region_v4 = vec!["10.0.0.0/8".to_string()];
        assert!(!cidr_belongs_to_region("2001:db8:1::/48", &region_v4));
        let region_v6 = vec!["2001:db8::/32".to_string()];
        assert!(!cidr_belongs_to_region("10.1.0.0/16", &region_v6));
        // 超网（前缀更小）同样不属于区域
        assert!(!cidr_belongs_to_region("10.0.0.0/7", &region_v4));
    }

    // ==================== 区域 CIDR 数组前置校验 ====================

    #[test]
    fn test_validate_region_cidr_array_valid() {
        // None（字段缺省）与合法数组均通过
        assert!(super::validate_region_cidr_array(None, "ipv4_cidrs").is_ok());
        let list = vec!["10.0.0.0/8".to_string(), "192.168.0.0/16".to_string()];
        assert!(super::validate_region_cidr_array(Some(&list), "ipv4_cidrs").is_ok());
        let v6 = vec!["2001:db8::/32".to_string()];
        assert!(super::validate_region_cidr_array(Some(&v6), "ipv6_cidrs").is_ok());
        // 空数组合法（清空区域范围）
        assert!(super::validate_region_cidr_array(Some(&Vec::new()), "ipv4_cidrs").is_ok());
    }

    #[test]
    fn test_validate_region_cidr_array_invalid_format() {
        // 非法 CIDR（主机位非 0 / 前缀越界 / 非法文本 / 空串）在绑定前拦截为 422
        for bad in ["192.168.1.1/24", "10.0.0.0/33", "abc", ""] {
            let list = vec!["10.0.0.0/8".to_string(), bad.to_string()];
            let err = super::validate_region_cidr_array(Some(&list), "ipv4_cidrs")
                .err()
                .unwrap_or_else(|| panic!("非法 CIDR {bad:?} 应被拒绝"));
            assert!(
                matches!(err, AppError::Validation(_)),
                "应为 422 校验错误，实际 {err}"
            );
        }
    }

    #[test]
    fn test_validate_region_cidr_array_duplicate_rejected() {
        // 数组内重复条目拒绝（trim 后等价视为同一条）
        let dup = vec![
            "10.0.0.0/8".to_string(),
            "172.16.0.0/12".to_string(),
            "10.0.0.0/8".to_string(),
        ];
        assert!(super::validate_region_cidr_array(Some(&dup), "ipv4_cidrs").is_err());
        // 带首尾空白的条目同样被拒绝（非法格式或 trim 后重复）
        let spaced = vec!["2001:db8::/32".to_string(), " 2001:db8::/32 ".to_string()];
        assert!(super::validate_region_cidr_array(Some(&spaced), "ipv6_cidrs").is_err());
    }
}
