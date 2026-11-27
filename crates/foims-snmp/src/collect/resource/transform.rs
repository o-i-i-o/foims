//! 数值变换原语（docs/SNMP模块重构方案.md §5.6 Transform）：
//! 单位换算、精度/除数、行过滤、状态映射。解释器与厂商形态之间的
//! 唯一换算层——单位统一换算到基准单位（字节 / 摄氏度），不分散在采集模块。

use async_snmp::Value;

use crate::vendor::model;
use model::{SkipField, SkipOp, SkipRule, Unit};

/// LibreNMS `processor_precision` 语义（正值 = raw/precision；负值 = 上游
/// 上报空闲率 100 + raw/precision）。结果必须落在 0–100。
pub fn cpu_pct_value(value: &Value, precision: i32) -> Option<f64> {
    let raw = value.as_u64()? as f64;
    let precision = if precision == 0 { 1 } else { precision };
    let pct = if precision < 0 {
        100.0 + raw / precision as f64
    } else {
        raw / precision as f64
    };
    (0.0..=100.0).contains(&pct).then_some(pct)
}

/// 字节列换算：raw × unit 系数（kb/mb）× precision 乘数。
/// unit 与 precision 同配时二者相乘（precision 面向 unit 无法表达的倍数）。
pub fn to_bytes(raw: u64, unit: Option<Unit>, precision: Option<i32>) -> u64 {
    let factor = unit.map_or(1, Unit::byte_factor);
    let precision = precision.unwrap_or(1).max(1) as u64;
    raw.saturating_mul(factor).saturating_mul(precision)
}

/// 温度列换算：raw × multiplier / divisor → 单位 → 摄氏度。
/// 有效业务区间 (0, 200]℃（0 常为占位、越界为哨兵值）。
pub fn to_celsius(raw: f64, unit: Option<Unit>, divisor: u64, multiplier: u64) -> Option<f64> {
    if divisor == 0 {
        return None;
    }
    let scaled = raw * multiplier as f64 / divisor as f64;
    let celsius = match unit {
        None | Some(Unit::Celsius | Unit::Bytes | Unit::Kb | Unit::Mb | Unit::Blocks) => scaled,
        Some(Unit::Milli) => scaled / 1000.0,
        Some(Unit::Micro) => scaled / 1_000_000.0,
        Some(Unit::Fahrenheit) => (scaled - 32.0) * 5.0 / 9.0,
    };
    (celsius.is_finite() && celsius > 0.0 && celsius <= 200.0).then_some(celsius)
}

/// 传感器值换算：unit 口径归一（温度类换算到摄氏度，其余原值透传）。
pub fn sensor_value(raw: f64, unit: Option<Unit>) -> Option<f64> {
    let value = match unit {
        None => raw,
        Some(Unit::Milli) => raw / 1000.0,
        Some(Unit::Micro) => raw / 1_000_000.0,
        Some(Unit::Fahrenheit) => (raw - 32.0) * 5.0 / 9.0,
        Some(_) => raw,
    };
    value.is_finite().then_some(value)
}

/// ENTITY-SENSOR-MIB entSensorScale（1=yocto … 9=units … 17=yotta）指数因子。
/// 枚举按 3 的步长递增：units(9)=10^0、milli(8)=10^-3、kilo(10)=10^3。
pub fn entity_scale_factor(scale: i32) -> Option<f64> {
    let exponent = 3 * (scale - 9);
    (-24..=24).contains(&exponent).then(|| 10f64.powi(exponent))
}

/// 行过滤（skip 原语）：name/value 文本比对。
pub fn skip_hit(rules: &[SkipRule], name: Option<&str>, raw_text: Option<&str>) -> bool {
    rules.iter().any(|rule| {
        let text = match rule.field {
            SkipField::Name => name,
            SkipField::Value => raw_text,
        };
        let Some(text) = text else {
            return false;
        };
        match rule.op {
            SkipOp::Eq => text == rule.value,
            SkipOp::Ne => text != rule.value,
            SkipOp::Has => text.contains(&rule.value),
        }
    })
}

/// 状态映射（states 原语）：数值 → 语义文本；未命中返回原值十进制串。
pub fn map_state(states: &std::collections::BTreeMap<i64, String>, raw: i64) -> String {
    states.get(&raw).cloned().unwrap_or_else(|| raw.to_string())
}

/// 数值提取：i32（温度可能为负）优先，其次 u64。
pub fn numeric(value: &Value) -> Option<f64> {
    value
        .as_i32()
        .map(f64::from)
        .or_else(|| value.as_u64().map(|v| v as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu精度换算_正负precision() {
        // precision=100：上游上报百分比 ×100
        assert_eq!(cpu_pct_value(&Value::Integer(1500), 100), Some(15.0));
        // precision=-1：上游上报空闲率，100 - raw
        assert_eq!(cpu_pct_value(&Value::Integer(20), -1), Some(80.0));
        // 过滤越界值，避免把负载/计数当成 CPU
        assert_eq!(cpu_pct_value(&Value::Integer(120), 1), None);
        assert_eq!(cpu_pct_value(&Value::Integer(0), 0), Some(0.0));
    }

    #[test]
    fn 字节换算_unit与precision相乘() {
        assert_eq!(to_bytes(10, Some(Unit::Kb), None), 10_240);
        assert_eq!(to_bytes(10, Some(Unit::Mb), None), 10 * 1024 * 1024);
        assert_eq!(to_bytes(10, None, Some(1024)), 10_240);
        assert_eq!(to_bytes(10, None, None), 10);
    }

    #[test]
    fn 温度换算_除数与单位() {
        assert_eq!(to_celsius(45000.0, Some(Unit::Milli), 1, 1), Some(45.0));
        assert_eq!(to_celsius(450.0, None, 10, 1), Some(45.0));
        // 0 与越界视为无效（哨兵值过滤）
        assert_eq!(to_celsius(0.0, None, 1, 1), None);
        assert_eq!(to_celsius(500.0, None, 1, 1), None);
        assert_eq!(
            to_celsius(80.0, Some(Unit::Fahrenheit), 1, 1),
            Some(26.666666666666668)
        );
        assert_eq!(to_celsius(45.0, None, 0, 1), None);
    }

    #[test]
    fn skip_行过滤语义() {
        let rules = vec![SkipRule {
            field: SkipField::Name,
            op: SkipOp::Eq,
            value: "I/O".to_string(),
        }];
        assert!(skip_hit(&rules, Some("I/O"), None));
        assert!(!skip_hit(&rules, Some("Processor"), None));
        assert!(!skip_hit(&rules, None, None));
        let has = vec![SkipRule {
            field: SkipField::Value,
            op: SkipOp::Has,
            value: "ok".to_string(),
        }];
        assert!(skip_hit(&has, None, Some("status: ok")));
    }

    #[test]
    fn 状态映射_命中与未命中() {
        let mut states = std::collections::BTreeMap::new();
        states.insert(1, "ok".to_string());
        states.insert(2, "warning".to_string());
        assert_eq!(map_state(&states, 1), "ok");
        assert_eq!(map_state(&states, 9), "9");
    }
}
