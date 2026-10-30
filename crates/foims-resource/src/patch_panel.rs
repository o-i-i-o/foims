//! 配线架资源：列表查询（供线路端点选择）与机柜弹窗内的整体同步；
//! 配线架隶属机柜，名称在单机柜范围内唯一。

use axum::extract::{Path, Query, State};
use axum::response::Response;
use chrono::Utc;
use foims_auth::meta::{RequestMeta, log_op_best_effort};
use foims_common::AppJson;
use foims_common::DbProvider;
use foims_common::pagination::{Pagination, paged_response};
use foims_common::{AppError, msg};
use foims_models::{CabinetPatchPanelsSync, PatchPanelWithDetails};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

/// GET /api/resources/patch-panels：分页获取配线架列表（供线路端点选择场景，
/// `search` 匹配名称、`cabinet_id` 过滤，非法 UUID 显式 400 而非静默忽略）。
pub async fn get_patch_panels<P: DbProvider>(
    State(state): State<Arc<P>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let pagination = Pagination::from_query(&query);
    let page_size = pagination.page_size;
    let offset = pagination.offset;
    let search = query.get("search").cloned().unwrap_or_default();
    let cabinet_id = query.get("cabinet_id").cloned();
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "name".to_string());
    let sort_order = query
        .get("sort_order")
        .cloned()
        .unwrap_or_else(|| "asc".to_string());

    let search_pattern = foims_common::net::escape_like(&search);
    let parsed_cabinet_id = cabinet_id
        .as_ref()
        .map(|id| {
            Uuid::parse_str(id).map_err(|_| {
                AppError::Validation(msg("server.common.invalid_param").with("param", "cabinet_id"))
            })
        })
        .transpose()?;

    let order_clause = match (sort_by.as_str(), sort_order.as_str()) {
        ("name", "desc") => "ORDER BY ap.name DESC",
        ("created_at", "desc") => "ORDER BY ap.created_at DESC",
        ("created_at", _) => "ORDER BY ap.created_at ASC",
        _ => "ORDER BY ap.name ASC",
    };

    let has_cabinet_filter = parsed_cabinet_id.is_some();
    let has_search = !search.is_empty();

    // 动态条件全部参数化构建（push_bind 自动编号，不拼接用户输入原文）
    let mut count_qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT COUNT(*) FROM patch_panels_with_details ap WHERE 1=1",
    );
    if has_search {
        count_qb
            .push(" AND ap.name ILIKE ")
            .push_bind(&search_pattern);
    }
    if has_cabinet_filter {
        count_qb
            .push(" AND ap.cabinet_id = ")
            .push_bind(parsed_cabinet_id);
    }
    let total: i64 = count_qb
        .build_query_scalar()
        .persistent(false)
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT ap.id, ap.name, ap.cabinet_id, ap.cabinet_name, ap.room_id, ap.room_name, \
         ap.created_at::TIMESTAMPTZ, ap.updated_at::TIMESTAMPTZ \
         FROM patch_panels_with_details ap WHERE 1=1",
    );
    if has_search {
        qb.push(" AND ap.name ILIKE ").push_bind(&search_pattern);
    }
    if has_cabinet_filter {
        qb.push(" AND ap.cabinet_id = ")
            .push_bind(parsed_cabinet_id);
    }
    // 排序段为白名单常量，经 push 拼接
    qb.push(" ").push(order_clause);
    qb.push(" LIMIT ").push_bind(page_size);
    qb.push(" OFFSET ").push_bind(offset);
    let patch_panels = qb
        .build_query_as::<PatchPanelWithDetails>()
        .persistent(false)
        .fetch_all(&state.pool()?.get_conn())
        .await?;

    Ok(foims_common::ok_json(
        paged_response(patch_panels, total, &pagination),
        "server.patch_panel.list_retrieved",
    ))
}

/// PUT /api/resources/cabinets/{id}/patch-panels：整体同步机柜下的配线架
/// （请求即最终状态：缺失项删除、带 id 项更新、无 id 项新增；机柜存在性检查
/// 在本事务内做，避免事务外快照读与写入之间的 TOCTOU）：机柜不存在 404、
/// 单机柜内重名 409、待删配线架被线路引用 400。
pub async fn sync_cabinet_patch_panels<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<CabinetPatchPanelsSync>,
) -> Result<Response, AppError> {
    req.validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let cabinet_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cabinets WHERE id = $1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if !cabinet_exists {
        return Err(AppError::NotFound(msg("server.cabinet.not_found")));
    }

    let items = &req.patch_panels;
    let now = Utc::now();

    let existing_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM patch_panels WHERE cabinet_id = $1")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;

    let request_ids: Vec<Uuid> = items.iter().filter_map(|i| i.id).collect();

    // 删除请求中不存在的配线架（cable_links 的删除保护触发器会阻止被引用的删除）
    for existing_id in &existing_ids {
        if !request_ids.contains(existing_id) {
            sqlx::query("DELETE FROM patch_panels WHERE id = $1")
                .bind(existing_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    if let sqlx::Error::Database(db_err) = &e {
                        let msg_text = db_err.message();
                        if msg_text.contains("ERR_CABLE_LINK_REFERENCE") {
                            return AppError::Validation(msg("server.patch_panel.in_use"));
                        }
                    }
                    AppError::from(e)
                })?;
        }
    }

    for item in items {
        let dup: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM patch_panels WHERE cabinet_id = $1 AND name = $2 AND ($3::uuid IS NULL OR id != $3)",
        )
        .bind(id)
        .bind(&item.name)
        .bind(item.id)
        .fetch_optional(&mut *tx)
        .await?;
        if dup.is_some() {
            return Err(AppError::Conflict(msg("server.patch_panel.name_exists")));
        }

        if let Some(item_id) = item.id {
            // 归属校验：仅允许更新当前机柜下的配线架，
            // 携带其他机柜的 id 时按未找到处理，避免跨父资源静默搬移
            let updated = sqlx::query(
                "UPDATE patch_panels SET name = $1, cabinet_id = $2, updated_at = $3 WHERE id = $4 AND cabinet_id = $5",
            )
            .bind(&item.name)
            .bind(id)
            .bind(now)
            .bind(item_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() == 0 {
                return Err(AppError::NotFound(msg("server.common.not_found")));
            }
        } else {
            let new_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO patch_panels (id, name, cabinet_id, created_at, updated_at) VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(new_id)
            .bind(&item.name)
            .bind(id)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    let details = serde_json::json!({
        "cabinet_id": id.to_string(),
        "patch_panel_count": items.len()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "sync_patch_panels",
        "cabinet",
        Some(&id),
        &details,
    )
    .await;

    Ok(foims_common::ok_json((), "server.patch_panel.synced"))
}
