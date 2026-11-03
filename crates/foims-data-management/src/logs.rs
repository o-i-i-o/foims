//! 日志清理与统计。

use crate::types::{ClearLogsRequest, DataError, DataProvider, DataResult, ok_json};
use axum::response::Response;
use foims_common::msg;
use serde_json::json;
use sqlx::{PgConnection, PgPool, QueryBuilder};
use validator::Validate;

/// 清理目标清单：(log_type 取值, 表名)。单类型与 all 分支共用同一白名单，
/// 替代原先每张表两份 days 分支共 6 段重复 DELETE SQL。
const LOG_TARGETS: &[(&str, &str)] = &[
    ("operation", "operation_logs"),
    ("login", "login_logs"),
    ("notification", "notifications"),
];

/// 保留天数上限（约 10 年）：手动清理允许 0..=MAX_KEEP_DAYS（0=删除全部），
/// 定时任务下界为 1（见 scheduled_task.rs 的 DAYS_CONFIG_RANGE），共用同一上界。
const MAX_KEEP_DAYS: i32 = 3650;

/// log_type → 目标表清单：all 展开全部，单类型按白名单查表，未命中拒绝。
fn resolve_log_targets(log_type: &str) -> DataResult<Vec<(&'static str, &'static str)>> {
    match log_type {
        "all" => Ok(LOG_TARGETS.to_vec()),
        _ => LOG_TARGETS
            .iter()
            .find(|(key, _)| *key == log_type)
            .map(|(key, table)| vec![(*key, *table)])
            .ok_or(DataError::Validation(msg("server.logs.type_invalid"))),
    }
}

/// 手动清理与预览共用的入参解析：载荷校验、days 缺省拒绝（0 语义为
/// “删除全部”，属危险操作不得静默回退）、范围校验（0..=MAX_KEEP_DAYS）。
fn parse_clear_request(req: &ClearLogsRequest) -> DataResult<i32> {
    req.validate()?;
    let Some(days) = req.days else {
        return Err(DataError::Validation(msg("server.logs.days_required")));
    };
    if !(0..=MAX_KEEP_DAYS).contains(&days) {
        return Err(DataError::Validation(msg("server.logs.days_invalid")));
    }
    Ok(days)
}

/// 追加保留期过滤：days=0 不追加（针对全表），否则限定 created_at 早于
/// days 天前。表名来自 LOG_TARGETS 编译期白名单，天数阈值参数化绑定。
fn push_retention_clause(qb: &mut QueryBuilder<sqlx::Postgres>, days: i32) {
    if days != 0 {
        qb.push(" WHERE created_at < NOW() - INTERVAL '1 day' * ")
            .push_bind(days);
    }
}

/// 单表清理：days=0 全表删除，否则删除早于 days 天的记录，返回删除行数。
async fn clear_table(conn: &mut PgConnection, table: &str, days: i32) -> DataResult<u64> {
    let mut qb = QueryBuilder::<sqlx::Postgres>::new("DELETE FROM ");
    qb.push(table);
    push_retention_clause(&mut qb, days);
    let result = qb.build().execute(conn).await.map_err(DataError::from)?;
    Ok(result.rows_affected())
}

/// 统计单表中将被清理的记录数（与 clear_table 完全同一过滤口径）。
async fn count_clearable_rows(pool: &PgPool, table: &str, days: i32) -> DataResult<i64> {
    let mut qb = QueryBuilder::<sqlx::Postgres>::new("SELECT COUNT(*) FROM ");
    qb.push(table);
    push_retention_clause(&mut qb, days);
    let count = qb
        .build_query_scalar()
        .fetch_one(pool)
        .await
        .map_err(DataError::from)?;
    Ok(count)
}

/// 核心日志清理逻辑，返回删除的行数。
///
/// 范围（0..=MAX_KEEP_DAYS）在此统一把关：手动接口与定时任务执行器
/// 共用本入口，任何一方的入参越界都在此处拒绝。
pub async fn clear_logs_core(pool: &PgPool, days: i32, log_type: &str) -> DataResult<u64> {
    if !(0..=MAX_KEEP_DAYS).contains(&days) {
        return Err(DataError::Validation(msg("server.logs.days_invalid")));
    }
    let targets = resolve_log_targets(log_type)?;

    let mut tx = pool.begin().await.map_err(DataError::from)?;
    let mut deleted = 0u64;
    for (_, table) in &targets {
        deleted += clear_table(&mut tx, table, days).await?;
    }
    tx.commit().await.map_err(DataError::from)?;
    Ok(deleted)
}

/// POST /api/system/logs/clear：按 `log_type`（operation/login/notification/
/// all）清理早于 `days` 天的日志（0 表示全部，缺省或越界 400）。
/// 返回 (删除行数, 响应)，行数供调用方写审计详情复用。
pub async fn clear_logs<P: DataProvider>(
    provider: P,
    req: ClearLogsRequest,
) -> DataResult<(u64, Response)> {
    let days = parse_clear_request(&req)?;
    let pool = provider.pool()?;

    let deleted = clear_logs_core(&pool, days, &req.log_type).await?;

    let response = ok_json(
        json!({ "deleted": deleted }),
        msg("server.logs.cleared").with("count", deleted),
    );
    Ok((deleted, response))
}

/// POST /api/system/logs/clear/preview：按与清理完全相同的入参口径统计
/// 将被删除的条数（不执行删除），供前端确认弹窗展示。响应载荷
/// `{ items: [{log_type, count}], total }` 仅含本次清理涉及的目标。
pub async fn preview_logs_clear<P: DataProvider>(
    provider: P,
    req: ClearLogsRequest,
) -> DataResult<Response> {
    let days = parse_clear_request(&req)?;
    let targets = resolve_log_targets(&req.log_type)?;
    let pool = provider.pool()?;

    let mut items = Vec::with_capacity(targets.len());
    let mut total = 0i64;
    for (log_type, table) in &targets {
        let count = count_clearable_rows(&pool, table, days).await?;
        total += count;
        items.push(json!({ "log_type": log_type, "count": count }));
    }

    Ok(ok_json(
        json!({ "items": items, "total": total }),
        "server.logs.preview_retrieved",
    ))
}

/// GET /api/system/logs/stats：统计操作日志、登录日志与通知的总数及
/// 最早一条的时间。
pub async fn get_logs_stats<P: DataProvider>(provider: P) -> DataResult<Response> {
    let pool = provider.pool()?;

    let operation_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operation_logs")
        .fetch_one(&pool)
        .await
        .map_err(DataError::from)?;

    let login_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM login_logs")
        .fetch_one(&pool)
        .await
        .map_err(DataError::from)?;

    let notification_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications")
        .fetch_one(&pool)
        .await
        .map_err(DataError::from)?;

    let operation_oldest: Option<String> = sqlx::query_scalar(
        "SELECT created_at::text FROM operation_logs ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(DataError::from)?;

    let login_oldest: Option<String> = sqlx::query_scalar(
        "SELECT created_at::text FROM login_logs ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(DataError::from)?;

    let notification_oldest: Option<String> = sqlx::query_scalar(
        "SELECT created_at::text FROM notifications ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(DataError::from)?;

    Ok(ok_json(
        json!({
            "operation_logs": { "count": operation_count, "oldest": operation_oldest },
            "login_logs": { "count": login_count, "oldest": login_oldest },
            "notifications": { "count": notification_count, "oldest": notification_oldest }
        }),
        "server.logs.stats_retrieved",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 目标解析_all展开全部() {
        assert_eq!(
            resolve_log_targets("all").map_err(|_| ()).as_deref(),
            Ok(&[
                ("operation", "operation_logs"),
                ("login", "login_logs"),
                ("notification", "notifications")
            ][..])
        );
    }

    #[test]
    fn 目标解析_单类型命中白名单() {
        assert_eq!(
            resolve_log_targets("login").map_err(|_| ()).as_deref(),
            Ok(&[("login", "login_logs")][..])
        );
    }

    #[test]
    fn 目标解析_未知类型拒绝() {
        assert!(resolve_log_targets("audit").is_err());
    }

    #[test]
    fn 清理入参_days缺省拒绝() {
        let req = ClearLogsRequest {
            log_type: "all".to_string(),
            days: None,
        };
        assert!(parse_clear_request(&req).is_err());
    }

    #[test]
    fn 清理入参_days区间内合法含零() {
        for days in [0, 1, 3650] {
            let req = ClearLogsRequest {
                log_type: "all".to_string(),
                days: Some(days),
            };
            assert_eq!(parse_clear_request(&req).map_err(|_| ()), Ok(days));
        }
    }

    #[test]
    fn 清理入参_days越界拒绝() {
        for days in [-1, 3651, i32::MAX] {
            let req = ClearLogsRequest {
                log_type: "all".to_string(),
                days: Some(days),
            };
            assert!(parse_clear_request(&req).is_err(), "days={days} 应被拒绝");
        }
    }
}
