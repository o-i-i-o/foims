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
