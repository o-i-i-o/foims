//! Cron 表达式解析与下次执行时间计算。

use crate::error::{SchedulerError, SchedulerResult};
use chrono::{Datelike, Timelike, Utc};
use foims_common::msg;

/// 计算循环的最大分钟步数（约 366 天）
const MAX_MINUTE_STEPS: usize = 366 * 24 * 60;

/// 根据 cron 表达式计算下次执行时间。
///
/// 语义与 tokio-cron-scheduler 的实际触发对齐：
/// - 秒位按候选秒值逐一遍历（此前对齐到最小命中秒值后按分钟步进，
///   会给出晚于真实触发时刻的 next_run_at）；
/// - 日（day-of-month）与星期（day-of-week）同时受限（均非 `*`）时按
///   OR 语义命中任一即触发（POSIX cron 惯例），此前误用 AND 语义。
pub fn calculate_next_run(cron_expression: &str) -> SchedulerResult<chrono::DateTime<Utc>> {
    let parts: Vec<&str> = cron_expression.split_whitespace().collect();

    if parts.len() != 5 && parts.len() != 6 {
        return Err(SchedulerError::Validation(
            msg("server.task.cron_expression_invalid").with("expression", cron_expression),
        ));
    }

    let cron_parts: Vec<&str> = if parts.len() == 6 {
        parts
    } else {
        // 5 字段表达式按惯例补秒位 "0"
        vec!["0", parts[0], parts[1], parts[2], parts[3], parts[4]]
    };

    // 预展开秒字段命中集合（升序）
    let mut matching_secs: Vec<i32> = Vec::new();
    for sec in 0..=59 {
        if matches_cron_field(cron_parts[0], sec, (0, 59))? {
            matching_secs.push(sec);
        }
    }
    if matching_secs.is_empty() {
        return Err(SchedulerError::Validation(
            msg("server.task.cron_field_invalid").with("field", cron_parts[0]),
        ));
    }

    let now = Utc::now();

    // 从当前分钟起逐分钟尝试：分钟级字段命中后，在该分钟内按候选秒值
    // 升序取第一个晚于 now 的时刻——即真实的下一次执行时刻
    let mut minute_base = now
        .with_second(0)
        .and_then(|t| t.with_nanosecond(0))
        .ok_or_else(|| SchedulerError::Validation(msg("server.task.cron_next_run_calc_failed")))?;

    for _ in 0..MAX_MINUTE_STEPS {
        let (min, hour, day, month, weekday) = (
            minute_base.minute() as i32,
            minute_base.hour() as i32,
            minute_base.day() as i32,
            minute_base.month() as i32,
            // 星期编号与实际触发库 croner 及 POSIX cron 对齐：周日=0
            minute_base.weekday().num_days_from_sunday() as i32,
        );

        if matches_cron_field(cron_parts[1], min, (0, 59))?
            && matches_cron_field(cron_parts[2], hour, (0, 23))?
            && matches_cron_field(cron_parts[4], month, (1, 12))?
            && matches_day_and_weekday(
                cron_parts[3],
                &normalize_weekday_field(cron_parts[5]),
                day,
                weekday,
            )?
        {
            for &sec in &matching_secs {
                let candidate = minute_base
                    .with_second(sec as u32)
                    .and_then(|t| t.with_nanosecond(0))
                    .ok_or_else(|| {
                        SchedulerError::Validation(msg("server.task.cron_next_run_calc_failed"))
                    })?;
                if candidate > now {
                    return Ok(candidate);
                }
            }
        }

        minute_base += chrono::Duration::minutes(1);
    }

    Err(SchedulerError::Validation(msg(
        "server.task.cron_next_run_calc_failed",
    )))
}

/// 日与星期字段的标准 cron 组合判定：
/// 两者同时受限（均非 `*`）时按 AND 语义须同时命中——与实际触发库
/// tokio-cron-scheduler（croner `dom_and_dow(true)`）一致，POSIX 的 OR
/// 惯例仅适用于经典 vixie-cron；
/// 仅一个受限时按该字段判定；两者均通配时恒命中。
fn matches_day_and_weekday(
    day_field: &str,
    weekday_field: &str,
    day: i32,
    weekday: i32,
) -> SchedulerResult<bool> {
    let day_restricted = day_field != "*";
    let weekday_restricted = weekday_field != "*";
    match (day_restricted, weekday_restricted) {
        (true, true) => Ok(matches_cron_field(day_field, day, (1, 31))?
            && matches_cron_field(weekday_field, weekday, (0, 7))?),
        (true, false) => matches_cron_field(day_field, day, (1, 31)),
        (false, true) => matches_cron_field(weekday_field, weekday, (0, 7)),
        (false, false) => Ok(true),
    }
}

/// 星期字段归一化：值 7 为周日的传统别名（croner 将其归一为 0），
/// 直接匹配会因实际 weekday 取值 0..=6 而永不命中。
/// 支持 "7"、"a-7"、"7/s"、"a-7/s" 及逗号列表各段的等价改写。
fn normalize_weekday_field(field: &str) -> String {
    field
        .split(',')
        .map(|part| {
            if let Some((base, step)) = part.split_once('/') {
                return format!("{}/{}", normalize_weekday_base(base), step);
            }
            normalize_weekday_base(part)
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn normalize_weekday_base(base: &str) -> String {
    if let Some((start, end)) = base.split_once('-') {
        if end.trim() == "7" {
            return format!("{start}-6");
        }
        return base.to_string();
    }
    if base.trim() == "7" {
        return "0".to_string();
    }
    base.to_string()
}

/// 单个 cron 字段匹配。`bounds` 为该字段的合法取值区间
/// （秒/分 0..=59、时 0..=23、日 1..=31、月 1..=12、星期 0..=7）：
/// 值、范围端点、步进基点均按边界校验，非法值当场报错，
/// 不再留到整年扫描后才以"永不命中"的形式暴露。
/// 支持 `*`、`n`、`a-b`、`a-b/s`、`n/s`、`*/s`、逗号列表的组合。
fn matches_cron_field(field: &str, value: i32, bounds: (i32, i32)) -> SchedulerResult<bool> {
    let (lo, hi) = bounds;
    let in_bounds = |v: i32| v >= lo && v <= hi;
    if field == "*" {
        return Ok(true);
    }

    if field.contains(',') {
        for part in field.split(',') {
            if matches_cron_field(part, value, bounds)? {
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
        // 步长必须 ≥ 1：步长 0 会导致取模运算整数除零 panic（security-review 第六节）
        if step < 1 {
            return Err(SchedulerError::Validation(
                msg("server.task.cron_step_invalid").with("value", parts[1]),
            ));
        }
        let base_field = parts[0];

        if base_field == "*" {
            return Ok(value % step == 0);
        }
        // 范围步进（a-b/s）：命中区间内自 a 起每隔 s 的取值（标准 cron 语法）
        if base_field.contains('-') {
            let range: Vec<&str> = base_field.split('-').collect();
            if range.len() != 2 {
                return Err(SchedulerError::Validation(
                    msg("server.task.cron_field_invalid").with("field", field),
                ));
            }
            let start: i32 = range[0].parse().map_err(|_| {
                SchedulerError::Validation(
                    msg("server.task.cron_range_start_invalid").with("value", range[0]),
                )
            })?;
            let end: i32 = range[1].parse().map_err(|_| {
                SchedulerError::Validation(
                    msg("server.task.cron_range_end_invalid").with("value", range[1]),
                )
            })?;
            if !in_bounds(start) || !in_bounds(end) || start > end {
                return Err(SchedulerError::Validation(
                    msg("server.task.cron_field_invalid").with("field", field),
                ));
            }
            return Ok(value >= start && value <= end && (value - start) % step == 0);
        }
        let base: i32 = base_field.parse().map_err(|_| {
            SchedulerError::Validation(
                msg("server.task.cron_base_invalid").with("value", base_field),
            )
        })?;
        if !in_bounds(base) {
            return Err(SchedulerError::Validation(
                msg("server.task.cron_base_invalid").with("value", base_field),
            ));
        }
        return Ok((value - base) % step == 0 && value >= base);
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
        if !in_bounds(start) || !in_bounds(end) || start > end {
            return Err(SchedulerError::Validation(
                msg("server.task.cron_field_invalid").with("field", field),
            ));
        }
        return Ok(value >= start && value <= end);
    }

    let field_value: i32 = field.parse().map_err(|_| {
        SchedulerError::Validation(msg("server.task.cron_field_value_invalid").with("field", field))
    })?;
    if !in_bounds(field_value) {
        return Err(SchedulerError::Validation(
            msg("server.task.cron_field_value_invalid").with("field", field),
        ));
    }
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
        // 步长 0 会引发整数除零 panic，修复后按非法步长拒绝
        assert_eq!(
            invalid_key("*/0 * * * * *"),
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
        // 以调用前后的时刻为界，避免「计算完成瞬间跨过整分钟」导致的边界偶发
        let before = Utc::now();
        let next = calculate_next_run("* * * * * *").unwrap_or_else(|e| panic!("应解析成功: {e}"));
        let after = Utc::now();
        assert!(next > before, "下次执行必须晚于调用前时刻");
        assert!(
            next <= after + chrono::Duration::seconds(61),
            "下次执行应在 1 分钟内"
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

    /// 修复后的行为：5 字段表达式秒位对齐到 0，任何时刻调用都能命中
    ///（此前按分钟步进保留当前秒数，整分 0 秒之外调用永远计算失败）
    #[test]
    fn 五字段表达式_秒位对齐后可命中() {
        let next = calculate_next_run("0 12 * * *")
            .unwrap_or_else(|e| panic!("5 字段表达式应解析成功: {e}"));
        assert_eq!(next.second(), 0, "5 字段表达式秒位应为 0");
        assert_eq!((next.minute(), next.hour()), (0, 12));
        assert!(next > Utc::now());

        // 每 15 分钟（5 字段变体）
        let next = calculate_next_run("*/15 * * * *").unwrap_or_else(|e| panic!("应解析成功: {e}"));
        assert_eq!(next.second(), 0);
        assert_eq!(next.minute() % 15, 0);
    }

    /// 6 字段表达式带非零秒位：秒位对齐到秒字段最小命中值后同样可命中
    #[test]
    fn 六字段表达式_非零秒位可命中() {
        let next = calculate_next_run("30 */5 * * * *")
            .unwrap_or_else(|e| panic!("带非零秒位的表达式应解析成功: {e}"));
        assert_eq!(next.second(), 30);
        assert_eq!(next.minute() % 5, 0);
    }

    /// 秒位精确：步进秒字段的下一次命中不晚于一个步长窗口
    ///（保证 next_run_at 不晚于调度器实际触发时刻）
    #[test]
    fn 秒位步进_下一次命中不晚于一个步长() {
        let next =
            calculate_next_run("*/15 * * * * *").unwrap_or_else(|e| panic!("应解析成功: {e}"));
        let now = Utc::now();
        assert!(next > now, "下次执行必须晚于当前时刻");
        assert_eq!(
            next.second() % 15,
            0,
            "秒位应命中步长集合: {}",
            next.second()
        );
        assert!(
            next - now <= chrono::Duration::seconds(15),
            "下一次触发不应晚于 15 秒后: 差值 {:?}",
            next - now
        );
    }

    /// 日与星期同时受限按 AND 语义（与触发库 croner dom_and_dow(true) 一致）
    #[test]
    fn 日与星期同时受限_按且语义判定() {
        // 1 日且周一：day=1/weekday=1（周日=0 体系）同时满足才命中
        assert!(matches_day_and_weekday("1", "1", 1, 1).unwrap_or(false));
        // day 命中但 weekday 不命中 → 不触发
        assert!(!matches_day_and_weekday("1", "1", 1, 2).unwrap_or(true));
        // 反之亦然
        assert!(!matches_day_and_weekday("1", "1", 2, 1).unwrap_or(true));
        // 仅一个受限时按该字段
        assert!(matches_day_and_weekday("*", "1", 15, 1).unwrap_or(false));
        assert!(matches_day_and_weekday("15", "*", 15, 3).unwrap_or(false));
        // 均通配恒命中
        assert!(matches_day_and_weekday("*", "*", 15, 3).unwrap_or(false));
    }

    /// 星期 7 归一为 0（周日别名，与 croner 一致）
    #[test]
    fn 星期字段_七归一为周日() {
        assert_eq!(normalize_weekday_field("7"), "0");
        assert_eq!(normalize_weekday_field("5-7"), "5-6");
        assert_eq!(normalize_weekday_field("7/2"), "0/2");
        assert_eq!(normalize_weekday_field("0,7"), "0,0");
        assert_eq!(normalize_weekday_field("1-5"), "1-5");
        // 0 0 0 * * 7 应能算出下一次（周日）
        let next = calculate_next_run("0 0 0 * * 7")
            .unwrap_or_else(|e| panic!("周日表达式应解析成功: {e}"));
        assert_eq!(next.weekday().num_days_from_sunday(), 0);
    }
}
