//! Cron 表达式解析与下次执行时间计算。

use crate::error::{SchedulerError, SchedulerResult};
use chrono::{Datelike, Timelike, Utc};
use ipma_common::msg;

/// 根据 cron 表达式计算下次执行时间
pub fn calculate_next_run(cron_expression: &str) -> SchedulerResult<chrono::DateTime<Utc>> {
    let parts: Vec<&str> = cron_expression.split_whitespace().collect();

    if parts.len() != 5 && parts.len() != 6 {
        return Err(SchedulerError::Validation(
            msg("server.task.cron_expression_invalid").with("expression", cron_expression),
        ));
    }

    let now = Utc::now();
    let mut next = now;

    for _ in 0..366 * 24 * 60 {
        next += chrono::Duration::minutes(1);

        let (sec, min, hour, day, month, weekday) = (
            next.second() as i32,
            next.minute() as i32,
            next.hour() as i32,
            next.day() as i32,
            next.month() as i32,
            next.weekday().num_days_from_monday() as i32,
        );

        let cron_parts = if parts.len() == 6 {
            parts.clone()
        } else {
            vec!["0", parts[0], parts[1], parts[2], parts[3], parts[4]]
        };

        if matches_cron_field(cron_parts[0], sec)?
            && matches_cron_field(cron_parts[1], min)?
            && matches_cron_field(cron_parts[2], hour)?
            && matches_cron_field(cron_parts[3], day)?
            && matches_cron_field(cron_parts[4], month)?
            && matches_cron_field(cron_parts[5], weekday)?
        {
            return Ok(next);
        }
    }

    Err(SchedulerError::Validation(msg(
        "server.task.cron_next_run_calc_failed",
    )))
}

fn matches_cron_field(field: &str, value: i32) -> SchedulerResult<bool> {
    if field == "*" {
        return Ok(true);
    }

    if field.contains(',') {
        for part in field.split(',') {
            if matches_cron_field(part, value)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }

    if field.contains('/') {
        let parts: Vec<&str> = field.split('/').collect();
        if parts.len() != 2 {
            return Err(SchedulerError::Validation(
                msg("server.task.cron_field_invalid").with("field", field),
            ));
        }
        let step: i32 = parts[1].parse().map_err(|_| {
            SchedulerError::Validation(msg("server.task.cron_step_invalid").with("value", parts[1]))
        })?;
        let base_field = parts[0];

        if base_field == "*" {
            return Ok(value % step == 0);
        } else {
            let base: i32 = base_field.parse().map_err(|_| {
                SchedulerError::Validation(
                    msg("server.task.cron_base_invalid").with("value", base_field),
                )
            })?;
            return Ok((value - base) % step == 0 && value >= base);
        }
    }

    if field.contains('-') {
        let parts: Vec<&str> = field.split('-').collect();
        if parts.len() != 2 {
            return Err(SchedulerError::Validation(
                msg("server.task.cron_field_invalid").with("field", field),
            ));
        }
        let start: i32 = parts[0].parse().map_err(|_| {
            SchedulerError::Validation(
                msg("server.task.cron_range_start_invalid").with("value", parts[0]),
            )
        })?;
        let end: i32 = parts[1].parse().map_err(|_| {
            SchedulerError::Validation(
                msg("server.task.cron_range_end_invalid").with("value", parts[1]),
            )
        })?;
        return Ok(value >= start && value <= end);
    }

    let field_value: i32 = field.parse().map_err(|_| {
        SchedulerError::Validation(msg("server.task.cron_field_value_invalid").with("field", field))
    })?;
    Ok(value == field_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    /// 断言表达式非法并返回 Validation 错误的消息 key
    /// （非法字段放在秒位，保证解析在首轮即失败，不受时序影响）
    fn invalid_key(expr: &str) -> String {
        let Err(SchedulerError::Validation(m)) = calculate_next_run(expr) else {
            panic!("表达式 {expr} 应解析失败");
        };
        m.key().to_string()
    }

    #[test]
    fn 字段数不是5或6_直接拒绝() {
        assert_eq!(invalid_key(""), "server.task.cron_expression_invalid");
        assert_eq!(
            invalid_key("* * * *"),
            "server.task.cron_expression_invalid"
        );
        assert_eq!(
            invalid_key("* * * * * * *"),
            "server.task.cron_expression_invalid"
        );
    }

    #[test]
    fn 非数字字面量_返回字段值非法() {
        assert_eq!(
            invalid_key("abc * * * * *"),
            "server.task.cron_field_value_invalid"
        );
    }

    #[test]
    fn 步长非法_返回步长错误() {
        assert_eq!(
            invalid_key("*/x * * * * *"),
            "server.task.cron_step_invalid"
        );
    }

    #[test]
    fn 步长基数非法_返回基数错误() {
        // 步长合法而基数非法时才命中 base_invalid（步长解析先于基数）
        assert_eq!(
            invalid_key("a/5 * * * * *"),
            "server.task.cron_base_invalid"
        );
    }

    #[test]
    fn 范围与步进段数超限_返回字段非法() {
        // 范围段 a-b-c 共 3 段
        assert_eq!(
            invalid_key("1-2-3 * * * * *"),
            "server.task.cron_field_invalid"
        );
        // 步进段 a/b/c 共 3 段
        assert_eq!(
            invalid_key("1/2/3 * * * * *"),
            "server.task.cron_field_invalid"
        );
    }

    #[test]
    fn 范围端点非法_返回对应错误() {
        assert_eq!(
            invalid_key("a-5 * * * * *"),
            "server.task.cron_range_start_invalid"
        );
        assert_eq!(
            invalid_key("1-b * * * * *"),
            "server.task.cron_range_end_invalid"
        );
    }

    #[test]
    fn 每分钟表达式_一分钟内命中() {
        let next = calculate_next_run("* * * * * *").unwrap_or_else(|e| panic!("应解析成功: {e}"));
        let delta = (next - Utc::now()).num_seconds();
        assert!(
            (0..=60).contains(&delta),
            "下次执行应在 1 分钟内，实际 {delta}s"
        );
    }

    #[test]
    fn 指定时刻表达式_命中该时刻() {
        // 每天 12:00（秒位通配）
        let next = calculate_next_run("* 0 12 * * *").unwrap_or_else(|e| panic!("应解析成功: {e}"));
        assert_eq!((next.minute(), next.hour()), (0, 12));
        assert!(next > Utc::now());
    }

    #[test]
    fn 通配步进字段_按步长命中() {
        // 每 15 分钟
        let next =
            calculate_next_run("* */15 * * * *").unwrap_or_else(|e| panic!("应解析成功: {e}"));
        assert_eq!(next.minute() % 15, 0);
    }

    #[test]
    fn 基数加步进字段_从基数起命中() {
        // 从第 10 分钟起每 5 分钟
        let next =
            calculate_next_run("* 10/5 * * * *").unwrap_or_else(|e| panic!("应解析成功: {e}"));
        let minute = next.minute() as i32;
        assert!(minute >= 10 && (minute - 10) % 5 == 0, "实际分钟 {minute}");
    }

    #[test]
    fn 逗号枚举字段_命中任一值() {
        // 第 0 或 30 分钟
        let next =
            calculate_next_run("* 0,30 * * * *").unwrap_or_else(|e| panic!("应解析成功: {e}"));
        assert_eq!(next.minute() % 30, 0, "实际分钟 {}", next.minute());
    }

    #[test]
    fn 范围字段_命中区间内() {
        // 8-18 点的第 0 分钟
        let next =
            calculate_next_run("* 0 8-18 * * *").unwrap_or_else(|e| panic!("应解析成功: {e}"));
        assert!((8..=18).contains(&next.hour()), "实际小时 {}", next.hour());
        assert_eq!(next.minute(), 0);
    }

    #[test]
    fn 从不匹配的表达式_返回计算失败() {
        // 2 月 30 日不存在：日与月字段永不同时命中
        let key = {
            let Err(SchedulerError::Validation(m)) = calculate_next_run("* 0 0 30 2 *") else {
                panic!("不可满足表达式应返回计算失败");
            };
            m.key().to_string()
        };
        assert_eq!(key, "server.task.cron_next_run_calc_failed");
    }

    /// 记录现状（生产代码缺陷，未在本任务修复）：
    /// 5 字段表达式按惯例秒位补 "0"，但 `calculate_next_run` 从当前时刻起按
    /// 分钟步进，秒数保持为调用时的秒数，因此除恰好在整秒 0 调用外，
    /// 任何 5 字段表达式都遍历完一年仍无法命中，最终返回计算失败。
    #[test]
    fn 五字段表达式_受秒位缺陷影响计算失败() {
        if Utc::now().second() == 0 {
            // 恰逢整秒 0 时该缺陷不显现，跳过断言避免偶发失败
            return;
        }
        let Err(SchedulerError::Validation(m)) = calculate_next_run("0 12 * * *") else {
            panic!("整秒 0 之外的 5 字段表达式当前无法命中（见函数注释）");
        };
        assert_eq!(m.key(), "server.task.cron_next_run_calc_failed");
    }
}
