//! CSV 解析、字段清洗与类型校验。
//!
//! 使用 csv crate 标准解析（RFC 4180，严格模式），逐字段清洗首尾空白
//! 与换行，不做任何整体正则替换，避免破坏带引号的 CSV 结构。

use crate::spec::{TableSpec, spec_by_header};
use crate::types::{DataError, DataResult};
use ipma_common::msg;
use sqlx::PgConnection;
use std::collections::BTreeMap;
use std::collections::HashMap;

/// 清洗后的单行数据：列名 → 值（已去除首尾空白/换行）。
#[derive(Clone)]
pub struct CsvRow {
    /// 数据行序号（自 1 起，不含表头），用于错误定位。
    pub row_no: usize,
    pub cells: HashMap<String, String>,
}

/// 一张表的导入数据。
pub struct TableBundle {
    pub spec: &'static TableSpec,
    pub rows: Vec<CsvRow>,
}

/// 数据库列元数据（用于空值处理、类型校验与参数绑定转换）。
pub struct ColumnMeta {
    pub udt: String,
    pub nullable: bool,
    pub has_default: bool,
    pub char_len: Option<i32>,
}

pub struct TableMeta {
    pub columns: BTreeMap<String, ColumnMeta>,
}

/// 绑定参数的类型转换后缀：udt_name → SQL 类型名（数组 _inet → inet[]）。
pub fn udt_cast(udt: &str) -> String {
    if let Some(elem) = udt.strip_prefix('_') {
        format!("{elem}[]")
    } else {
        udt.to_string()
    }
}

/// 读取表元数据（类型、可空、默认值、字符长度）。
pub async fn fetch_table_meta(conn: &mut PgConnection, table: &str) -> DataResult<TableMeta> {
    let rows: Vec<(String, String, String, Option<i32>)> = sqlx::query_as(
        r"SELECT column_name, udt_name, is_nullable, character_maximum_length
           FROM information_schema.columns
           WHERE table_schema = 'public' AND table_name = $1",
    )
    .bind(table)
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

    if rows.is_empty() {
        return Err(DataError::Validation(
            msg("server.import_export.spec_missing").with("table", table),
        ));
    }

    // 默认值存在性单独查询（column_default 可能为 NULL）
    let defaults: Vec<(String, bool)> = sqlx::query_as(
        r"SELECT column_name, column_default IS NOT NULL
           FROM information_schema.columns
           WHERE table_schema = 'public' AND table_name = $1",
    )
    .bind(table)
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

    Ok(TableMeta {
        columns: rows
            .into_iter()
            .map(|(name, udt, nullable, char_len)| {
                let has_default = defaults.iter().any(|(c, d)| c == &name && *d);
                let meta = ColumnMeta {
                    udt,
                    nullable: nullable == "YES",
                    has_default,
                    char_len,
                };
                (name, meta)
            })
            .collect(),
    })
}

/// 对称剥离导出侧的公式注入转义前缀（security-review S-1 往返一致性）：
/// 仅当单元格以 `'` 开头且其后首字符恰为导出侧会转义的前缀（`= + - @`、
/// Tab、CR）时剥掉该撇号（如导出 `'+86…` 还原为 `+86…`）；
/// 其余以 `'` 开头的内容（撇号本身是业务数据）原样保留。
pub fn strip_formula_escape(cell: &str) -> &str {
    let rest = match cell.strip_prefix('\'') {
        Some(rest) => rest,
        None => return cell,
    };
    let escaped_prefix = rest
        .as_bytes()
        .first()
        .is_some_and(|&b| matches!(b, b'=' | b'+' | b'-' | b'@' | b'\t' | b'\r'));
    if escaped_prefix { rest } else { cell }
}

/// 解析 CSV 字节流：剥离 BOM、严格解析、逐字段清洗。
/// 返回（表头, 数据行）。
pub fn parse_csv(data: &[u8]) -> DataResult<(Vec<String>, Vec<CsvRow>)> {
    let start = if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &data[3..]
    } else {
        data
    };

    let mut reader = csv::ReaderBuilder::new().flexible(false).from_reader(start);
    let headers = reader
        .headers()
        .map_err(|e| {
            DataError::Validation(
                msg("server.import_export.csv_parse_failed").with("error", e.to_string()),
            )
        })?
        .iter()
        .map(|h| h.trim().to_string())
        .collect::<Vec<String>>();

    let mut rows = Vec::new();
    for (idx, record) in reader.records().enumerate() {
        let record = record.map_err(|e| {
            DataError::Validation(
                msg("server.import_export.csv_parse_failed")
                    .with("line", idx + 2) // 含表头行的物理行号
                    .with("error", e.to_string()),
            )
        })?;
        let mut cells = HashMap::with_capacity(headers.len());
        for (i, header) in headers.iter().enumerate() {
            // 逐字段清洗首尾空白（含换行、全角空格等 Unicode 空白），
            // 再剥离导出侧公式转义前缀，保证导出→导入往返一致
            let value = strip_formula_escape(record.get(i).unwrap_or_default().trim()).to_string();
            cells.insert(header.clone(), value);
        }
        rows.push(CsvRow {
            row_no: idx + 1,
            cells,
        });
    }
    Ok((headers, rows))
}

/// 按表头集合识别表（单文件导入不比对文件名）。
pub fn detect_by_header(headers: &[String]) -> DataResult<&'static TableSpec> {
    spec_by_header(headers)
        .ok_or_else(|| DataError::Validation(msg("server.import_export.unrecognized_header")))
}

/// 校验表头与指定规格一致（ZIP 内文件名已确定表）。
pub fn ensure_header_matches(headers: &[String], spec: &TableSpec) -> DataResult<()> {
    let expected: std::collections::HashSet<&str> = spec
        .columns
        .iter()
        .map(crate::spec::Col::csv_name)
        .collect();
    let got: std::collections::HashSet<&str> = headers.iter().map(String::as_str).collect();
    if expected == got {
        Ok(())
    } else {
        Err(DataError::Validation(
            msg("server.import_export.header_mismatch").with("table", spec.table),
        ))
    }
}

/// 字段值类型校验（入库前的用户输入校验，失败返回原因描述）。
/// 返回 Err(原因) 时调用方补充表/行/列上下文生成校验错误。
pub fn validate_value(meta: &ColumnMeta, value: &str) -> Result<(), String> {
    let udt = meta.udt.as_str();
    match udt {
        "int2" => value
            .parse::<i16>()
            .map(|_| ())
            .map_err(|_| "不是有效的小整数".to_string()),
        "int4" => value
            .parse::<i32>()
            .map(|_| ())
            .map_err(|_| "不是有效的整数".to_string()),
        "int8" => value
            .parse::<i64>()
            .map(|_| ())
            .map_err(|_| "不是有效的整数".to_string()),
        "float4" | "float8" | "numeric" => value
            .parse::<f64>()
            .map(|_| ())
            .map_err(|_| "不是有效的数值".to_string()),
        "bool" => match value.to_ascii_lowercase().as_str() {
            "true" | "false" => Ok(()),
            _ => Err("应为 true 或 false".to_string()),
        },
        "inet" => crate::names::parse_inet(value)
            .map(|_| ())
            .ok_or_else(|| "不是有效的 IP 地址".to_string()),
        "cidr" => crate::names::parse_cidr(value)
            .map(|_| ())
            .ok_or_else(|| "不是有效的网段（如 10.0.0.0/24）".to_string()),
        "_inet" | "_cidr" => {
            let elements =
                parse_pg_array(value).ok_or_else(|| "不是有效的数组（形如 {a,b}）".to_string())?;
            let elem_udt = &udt[1..];
            for element in elements {
                let elem_meta = ColumnMeta {
                    udt: elem_udt.to_string(),
                    nullable: true,
                    has_default: false,
                    char_len: None,
                };
                validate_value(&elem_meta, &element)?;
            }
            Ok(())
        }
        "json" | "jsonb" => serde_json::from_str::<serde_json::Value>(value)
            .map(|_| ())
            .map_err(|_| "不是有效的 JSON".to_string()),
        "varchar" | "bpchar" => {
            if let Some(len) = meta.char_len
                && value.chars().count() > len as usize
            {
                return Err(format!("超过 {len} 字符上限"));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// 解析 PG 数组字面量 "{a,b}" 为元素列表（仅支持本项目无特殊字符的
/// inet[]/cidr[] 列，元素不含逗号/引号/反斜杠）。
pub fn parse_pg_array(value: &str) -> Option<Vec<String>> {
    let inner = value.strip_prefix('{')?.strip_suffix('}')?;
    if inner.is_empty() {
        return Some(Vec::new());
    }
    Some(
        inner
            .split(',')
            .map(|e| e.trim().trim_matches('"').to_string())
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(udt: &str, char_len: Option<i32>) -> ColumnMeta {
        ColumnMeta {
            udt: udt.to_string(),
            nullable: true,
            has_default: false,
            char_len,
        }
    }

    #[test]
    fn 解析并清洗csv字段() {
        let data = "name,room_name\r\n 交换机1 , 机房A \r\n".as_bytes();
        let (headers, rows) = parse_csv(data).unwrap();
        assert_eq!(headers, vec!["name", "room_name"]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cells["name"], "交换机1");
        assert_eq!(rows[0].cells["room_name"], "机房A");
        assert_eq!(rows[0].row_no, 1);
    }

    #[test]
    fn 剥离bom() {
        let mut data = vec![0xEF, 0xBB, 0xBF];
        data.extend_from_slice("name\n值\n".as_bytes());
        let (headers, rows) = parse_csv(&data).unwrap();
        assert_eq!(headers, vec!["name"]);
        assert_eq!(rows[0].cells["name"], "值");
    }

    /// 导出侧加撇号转义的单元格在导入时对称还原；撇号本身是业务数据的
    /// 单元格（后随字符不属于导出侧转义前缀集合）必须原样保留
    #[test]
    fn 公式转义前缀对称剥离() {
        // 导出侧会转义的前缀组合：撇号被剥掉
        assert_eq!(
            strip_formula_escape("'+86 138-0013-8000"),
            "+86 138-0013-8000"
        );
        assert_eq!(strip_formula_escape("'=SUM(A1)"), "=SUM(A1)");
        assert_eq!(strip_formula_escape("'-1"), "-1");
        assert_eq!(strip_formula_escape("'@cmd"), "@cmd");
        assert_eq!(strip_formula_escape("'\tTAB"), "\tTAB");
        assert_eq!(strip_formula_escape("'\rCR"), "\rCR");

        // 其余以撇号开头的内容原样保留
        assert_eq!(strip_formula_escape("'abc"), "'abc");
        assert_eq!(strip_formula_escape("'"), "'");
        assert_eq!(strip_formula_escape("'"), "'");
        // 不以撇号开头的内容不受影响
        assert_eq!(strip_formula_escape("=abc"), "=abc");
        assert_eq!(strip_formula_escape("+1"), "+1");
    }

    #[test]
    fn 字段类型校验() {
        assert!(validate_value(&meta("int4", None), "42").is_ok());
        assert!(validate_value(&meta("int4", None), "abc").is_err());
        assert!(validate_value(&meta("bool", None), "TRUE").is_ok());
        assert!(validate_value(&meta("bool", None), "yes").is_err());
        assert!(validate_value(&meta("inet", None), "192.168.1.1").is_ok());
        assert!(validate_value(&meta("inet", None), "999.1.1.1").is_err());
        assert!(validate_value(&meta("cidr", None), "10.0.0.0/24").is_ok());
        assert!(validate_value(&meta("cidr", None), "10.0.0.0").is_err());
        assert!(validate_value(&meta("varchar", Some(20)), "网络区域").is_ok());
        let too_long = "a".repeat(21);
        assert!(validate_value(&meta("varchar", Some(20)), &too_long).is_err());
    }

    #[test]
    fn 数组字面量校验() {
        assert!(validate_value(&meta("_inet", None), "{10.0.0.1,10.0.0.2}").is_ok());
        assert!(validate_value(&meta("_inet", None), "{}").is_ok());
        assert!(validate_value(&meta("_cidr", None), "{10.0.0.0/24}").is_ok());
        assert!(validate_value(&meta("_inet", None), "{bad}").is_err());
        assert_eq!(
            parse_pg_array("{a, b}").unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
