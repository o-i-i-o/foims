//! 模块化 JSON 数据导入。
//!
//! 导入流程：解析上传文件（ZIP 内多个模块 JSON 或单个模块 JSON）→
//! 校验模块/表归属 → 按固定模块顺序（外键拓扑序：组织→网络→房间→机柜→
//! 设备→线路→可视化）在同一事务内逐表导入。每张表写入前先做外键存在性
//! 校验（引用目标须已存在于库中，或本事务先前步骤已导入），缺失则整批
//! 报错回滚，避免半成品数据。行级写入为按主键的 UPSERT，重复导入同一
//! 份数据幂等。表结构与类型转换基于 information_schema 元数据驱动，
//! 无需为每张表手写插入语句。

use crate::modules::{MODULES, find_module};
use crate::types::{DataError, DataProvider, DataResult, ok_json};
use axum::extract::Multipart;
use axum::response::Response;
use ipma_common::msg;
use serde_json::{Value, json};
use sqlx::query_builder::QueryBuilder;
use sqlx::{AssertSqlSafe, Connection, PgConnection, Postgres, Row};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use uuid::Uuid;

/// 单个模块文件解析结果。
struct ModuleBundle {
    module: &'static crate::modules::ModuleDef,
    /// 表名 → 行数组
    tables: HashMap<String, Vec<Value>>,
}

/// 表元数据：列名 → PG 类型（udt_name），用于绑定参数的类型转换。
struct TableMeta {
    columns: BTreeMap<String, String>,
    pk: String,
}

/// 外键关系（单列外键；本项目相关表均满足）。
struct ForeignKey {
    column: String,
    ref_table: String,
}

/// devices 表中明文传入、落库前需加密的凭据列（与导出端对应）。
const DEVICE_SECRET_COLUMNS: &[&str] =
    &["snmp_community", "snmp_auth_password", "snmp_priv_password"];

/// cable_links 端点类型 → 端点表（多态端点无数据库外键，需入库前校验）。
const CABLE_ENDPOINT_TABLES: &[(&str, &str)] = &[
    ("device_port", "device_ports"),
    ("net_outlet", "net_outlets"),
    ("device_interface", "device_interfaces"),
    ("patch_panel", "patch_panels"),
];

pub async fn import_json<P: DataProvider>(
    provider: P,
    mut payload: Multipart,
) -> DataResult<Response> {
    let file_data = read_upload(&mut payload).await?;
    let bundles = parse_bundles(&file_data)?;

    let pool = provider.pool()?;
    let mut conn = pool.acquire().await.map_err(DataError::from)?;
    let mut tx = conn.begin().await.map_err(DataError::from)?;

    let mut summary: Vec<Value> = Vec::new();
    // 固定模块顺序：ZIP 内文件顺序不影响导入结果
    for module in MODULES {
        let Some(bundle) = bundles.iter().find(|b| b.module.name == module.name) else {
            continue;
        };

        let mut module_rows: u64 = 0;
        for table in module.tables {
            let Some(rows) = bundle.tables.get(*table) else {
                continue;
            };
            if rows.is_empty() {
                continue;
            };
            let mut rows = rows.clone();

            let meta = fetch_table_meta(&mut tx, table).await?;
            let fks = fetch_table_foreign_keys(&mut tx, table).await?;

            normalize_rows(table, &meta, &mut rows)?;
            if let Some(fk) = fks.iter().find(|fk| fk.ref_table == *table) {
                sort_self_referencing(&mut rows, &meta.pk, &fk.column);
            }
            validate_foreign_keys(&mut tx, table, &fks, &rows).await?;
            if *table == "cable_links" {
                validate_cable_endpoints(&mut tx, &rows).await?;
            }
            if *table == "devices" {
                for row in &mut rows {
                    encrypt_device_secrets(&provider, row).await?;
                }
            }
            upsert_rows(&mut tx, table, &meta, &rows).await?;
            module_rows += rows.len() as u64;
        }
        if module_rows > 0 {
            summary.push(json!({"module": module.name, "rows": module_rows}));
        }
    }

    tx.commit().await.map_err(DataError::from)?;

    Ok(ok_json(
        json!({"results": summary}),
        "server.import_export.import_success",
    ))
}

/// 读取上传文件字段，限制 50MB。
async fn read_upload(payload: &mut Multipart) -> DataResult<Vec<u8>> {
    const MAX_UPLOAD_SIZE: usize = 50 * 1024 * 1024;
    while let Some(mut field) = payload.next_field().await.map_err(|e| {
        DataError::Internal(msg("server.import_export.file_read_failed").with("error", e))
    })? {
        if field.name() == Some("file") {
            let mut data = Vec::new();
            while let Some(chunk) = field.chunk().await.map_err(|e| {
                DataError::Internal(msg("server.import_export.chunk_read_failed").with("error", e))
            })? {
                data.extend_from_slice(&chunk);
                if data.len() > MAX_UPLOAD_SIZE {
                    return Err(DataError::Validation(
                        msg("server.import_export.file_too_large")
                            .with("limit", MAX_UPLOAD_SIZE / 1024 / 1024),
                    ));
                }
            }
            return Ok(data);
        }
    }
    Err(DataError::Validation(msg(
        "server.import_export.no_file_selected",
    )))
}

/// 解析上传内容：ZIP（多个模块 JSON）或单个模块 JSON。
fn parse_bundles(file_data: &[u8]) -> DataResult<Vec<ModuleBundle>> {
    let module_files: Vec<(String, Vec<u8>)> =
        if file_data.starts_with(b"PK\x03\x04") || file_data.starts_with(b"PK\x05\x06") {
            read_zip_entries(file_data)?
        } else {
            vec![("upload.json".to_string(), file_data.to_vec())]
        };

    let mut bundles: Vec<ModuleBundle> = Vec::new();
    for (filename, content) in module_files {
        let stem = filename.trim_end_matches(".json");
        let parsed: Value = serde_json::from_slice(&content).map_err(|e| {
            DataError::Validation(
                msg("server.import_export.json_parse_failed")
                    .with("file", &filename)
                    .with("error", e),
            )
        })?;

        // 模块名优先取文件内声明的 module 字段，缺省回退到文件名
        let module_name = parsed
            .get("module")
            .and_then(|v| v.as_str())
            .unwrap_or(stem);
        let module = find_module(module_name).ok_or_else(|| {
            DataError::Validation(
                msg("server.import_export.unknown_module").with("module", module_name),
            )
        })?;

        let mut tables: HashMap<String, Vec<Value>> = HashMap::new();
        if let Some(tables_obj) = parsed.get("tables").and_then(|v| v.as_object()) {
            for (table, rows) in tables_obj {
                if !module.tables.contains(&table.as_str()) {
                    return Err(DataError::Validation(
                        msg("server.import_export.table_not_in_module")
                            .with("table", table)
                            .with("module", module_name),
                    ));
                }
                let rows: Vec<Value> = rows.as_array().cloned().unwrap_or_default();
                tables.insert(table.clone(), rows);
            }
        }
        bundles.push(ModuleBundle { module, tables });
    }
    Ok(bundles)
}

/// 读取 ZIP 内全部 .json 条目（含 zip bomb 防护）。
fn read_zip_entries(file_data: &[u8]) -> DataResult<Vec<(String, Vec<u8>)>> {
    const MAX_DECOMPRESSED_SIZE: u64 = 100 * 1024 * 1024;
    const MAX_TOTAL_DECOMPRESSED: u64 = 500 * 1024 * 1024;
    let mut total: u64 = 0;
    let mut entries = Vec::new();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(file_data.to_vec())).map_err(|e| {
        DataError::Validation(msg("server.import_export.zip_entry_read_failed").with("error", e))
    })?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| {
            DataError::Internal(msg("server.import_export.zip_entry_read_failed").with("error", e))
        })?;
        let name = file.name().to_string();
        if name.contains("..") || name.contains('/') || name.contains('\\') {
            continue;
        }
        if !name.ends_with(".json") {
            continue;
        }
        let mut limited = (&mut file).take(MAX_DECOMPRESSED_SIZE);
        let mut content = Vec::new();
        std::io::Read::read_to_end(&mut limited, &mut content).map_err(|e| {
            DataError::Internal(msg("server.import_export.zip_entry_read_failed").with("error", e))
        })?;
        total += content.len() as u64;
        if total > MAX_TOTAL_DECOMPRESSED {
            return Err(DataError::Validation(msg(
                "server.import_export.file_too_large",
            )));
        }
        entries.push((name, content));
    }
    if entries.is_empty() {
        return Err(DataError::Validation(msg(
            "server.import_export.no_module_files",
        )));
    }
    Ok(entries)
}

/// 读取表元数据（列与类型、主键）。
async fn fetch_table_meta(conn: &mut PgConnection, table: &str) -> DataResult<TableMeta> {
    let columns: Vec<(String, String)> = sqlx::query_as(
        r"SELECT column_name, udt_name
           FROM information_schema.columns
           WHERE table_schema = 'public' AND table_name = $1",
    )
    .bind(table)
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

    if columns.is_empty() {
        return Err(DataError::Validation(
            msg("server.import_export.table_not_in_module").with("table", table),
        ));
    }

    // table 来自静态白名单，字符串字面量转 regclass 无注入风险
    let pk_sql = format!(
        r"SELECT a.attname
           FROM pg_index i
           JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
           WHERE i.indrelid = '{table}'::regclass AND i.indisprimary
           ORDER BY a.attnum LIMIT 1"
    );
    // 表名来自静态白名单，已人工审计无注入风险
    let pk: String = sqlx::query_scalar(AssertSqlSafe(pk_sql))
        .fetch_one(&mut *conn)
        .await
        .map_err(DataError::from)?;

    Ok(TableMeta {
        columns: columns.into_iter().collect(),
        pk,
    })
}

/// 读取表的外键关系（单列外键，本模块涉及表均满足）。
async fn fetch_table_foreign_keys(
    conn: &mut PgConnection,
    table: &str,
) -> DataResult<Vec<ForeignKey>> {
    let rows = sqlx::query(
        r"SELECT kcu.column_name, ccu.table_name
           FROM information_schema.table_constraints tc
           JOIN information_schema.key_column_usage kcu
             ON tc.constraint_name = kcu.constraint_name AND tc.table_schema = kcu.table_schema
           JOIN information_schema.constraint_column_usage ccu
             ON ccu.constraint_name = tc.constraint_name AND ccu.table_schema = tc.table_schema
           WHERE tc.constraint_type = 'FOREIGN KEY'
             AND tc.table_schema = 'public' AND tc.table_name = $1",
    )
    .bind(table)
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

    Ok(rows
        .iter()
        .filter_map(|r| {
            let column = r.try_get::<String, _>("column_name").ok()?;
            let ref_table = r.try_get::<String, _>("table_name").ok()?;
            Some(ForeignKey { column, ref_table })
        })
        .filter(|fk| !fk.column.is_empty() && !fk.ref_table.is_empty())
        .collect())
}

/// 行规范化：丢弃未知列、校验主键存在且为合法 UUID。
fn normalize_rows(table: &str, meta: &TableMeta, rows: &mut [Value]) -> DataResult<()> {
    for row in rows.iter_mut() {
        let Some(obj) = row.as_object_mut() else {
            return Err(DataError::Validation(
                msg("server.import_export.row_not_object").with("table", table),
            ));
        };
        obj.retain(|k, _| meta.columns.contains_key(k));
        match obj.get(&meta.pk) {
            Some(Value::String(id)) => {
                if id.parse::<Uuid>().is_err() {
                    return Err(DataError::Validation(
                        msg("server.import_export.invalid_row_id")
                            .with("table", table)
                            .with("id", id),
                    ));
                }
            }
            _ => {
                return Err(DataError::Validation(
                    msg("server.import_export.missing_row_id").with("table", table),
                ));
            }
        }
    }
    Ok(())
}

/// 自引用表（如 organizations.parent_id）按依赖排序：父行先于子行入库。
fn sort_self_referencing(rows: &mut Vec<Value>, pk: &str, fk: &str) {
    let mut remaining = std::mem::take(rows);
    let mut placed: HashSet<String> = HashSet::new();
    let mut result: Vec<Value> = Vec::with_capacity(remaining.len());
    let mut progressed = true;
    while !remaining.is_empty() && progressed {
        progressed = false;
        let mut i = 0;
        while i < remaining.len() {
            let parent_ready = match remaining[i].get(fk).and_then(|v| v.as_str()) {
                None | Some("") => true,
                Some(parent) => placed.contains(parent),
            };
            if parent_ready {
                let row = remaining.remove(i);
                if let Some(id) = row.get(pk).and_then(|v| v.as_str()) {
                    placed.insert(id.to_string());
                }
                result.push(row);
                progressed = true;
            } else {
                i += 1;
            }
        }
    }
    // 环形引用按原顺序追加，交给数据库外键约束报错
    result.extend(remaining);
    *rows = result;
}

/// 外键存在性校验：引用目标须已在库中存在（事务内先前导入的模块可见）。
/// 自引用外键额外把本批行的主键也视作可用目标。
async fn validate_foreign_keys(
    conn: &mut PgConnection,
    table: &str,
    fks: &[ForeignKey],
    rows: &[Value],
) -> DataResult<()> {
    for fk in fks {
        let mut ids: Vec<String> = rows
            .iter()
            .filter_map(|r| r.get(&fk.column).and_then(|v| v.as_str()))
            .filter(|v| !v.is_empty())
            .map(String::from)
            .collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            continue;
        }
        let uuids = parse_uuids(table, &fk.column, &ids)?;

        let sql = format!("SELECT COUNT(*) FROM {} WHERE id = ANY($1)", fk.ref_table);
        let count: i64 = sqlx::query_scalar(AssertSqlSafe(sql))
            .bind(&uuids)
            .fetch_one(&mut *conn)
            .await
            .map_err(DataError::from)?;

        let mut available = count as usize;
        if fk.ref_table == table {
            let batch_ids: HashSet<&str> = rows
                .iter()
                .filter_map(|r| r.get("id").and_then(|v| v.as_str()))
                .collect();
            available += ids
                .iter()
                .filter(|id| batch_ids.contains(id.as_str()))
                .count();
        }

        if available < uuids.len() {
            return Err(DataError::Validation(
                msg("server.import_export.fk_target_missing")
                    .with("table", table)
                    .with("column", &fk.column)
                    .with("ref_table", &fk.ref_table),
            ));
        }
    }
    Ok(())
}

/// 校验跳接线路端点存在（多态端点无外键约束，对应触发器的入库前预检）。
async fn validate_cable_endpoints(conn: &mut PgConnection, rows: &[Value]) -> DataResult<()> {
    for (endpoint_col, id_col) in [
        ("a_endpoint_type", "a_endpoint_id"),
        ("b_endpoint_type", "b_endpoint_id"),
    ] {
        let mut by_type: HashMap<&str, Vec<String>> = HashMap::new();
        for row in rows {
            let Some(kind) = row.get(endpoint_col).and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(id) = row.get(id_col).and_then(|v| v.as_str()) else {
                continue;
            };
            by_type.entry(kind).or_default().push(id.to_string());
        }
        for (kind, ids) in by_type {
            let Some((_, ref_table)) = CABLE_ENDPOINT_TABLES.iter().find(|(k, _)| *k == kind)
            else {
                return Err(DataError::Validation(
                    msg("server.import_export.unknown_endpoint_type").with("type", kind),
                ));
            };
            let uuids = parse_uuids("cable_links", endpoint_col, &ids)?;
            let sql = format!("SELECT COUNT(*) FROM {ref_table} WHERE id = ANY($1)");
            let count: i64 = sqlx::query_scalar(AssertSqlSafe(sql))
                .bind(&uuids)
                .fetch_one(&mut *conn)
                .await
                .map_err(DataError::from)?;
            if count as usize != uuids.len() {
                return Err(DataError::Validation(
                    msg("server.import_export.endpoint_missing").with("type", kind),
                ));
            }
        }
    }
    Ok(())
}

/// 设备凭据列明文 → 本实例密文。
async fn encrypt_device_secrets<P: DataProvider>(provider: &P, row: &mut Value) -> DataResult<()> {
    let Some(obj) = row.as_object_mut() else {
        return Ok(());
    };
    for col in DEVICE_SECRET_COLUMNS {
        if let Some(Value::String(plain)) = obj.get(*col).cloned() {
            if plain.is_empty() {
                continue;
            }
            let encrypted = provider.encrypt_password(&plain).await?;
            obj.insert((*col).to_string(), Value::String(encrypted));
        }
    }
    Ok(())
}

/// 按主键 UPSERT 写入行（列白名单与类型转换均来自表元数据）。
async fn upsert_rows(
    conn: &mut PgConnection,
    table: &str,
    meta: &TableMeta,
    rows: &[Value],
) -> DataResult<()> {
    for row in rows {
        let Some(obj) = row.as_object() else {
            continue;
        };
        // 只写入行内出现的列；缺失列走表默认值
        let cols: Vec<&str> = meta
            .columns
            .keys()
            .map(String::as_str)
            .filter(|c| obj.contains_key(*c))
            .collect();

        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("INSERT INTO ");
        qb.push(table);
        qb.push(" (");
        for (i, col) in cols.iter().enumerate() {
            if i > 0 {
                qb.push(", ");
            }
            qb.push(*col);
        }
        qb.push(") VALUES (");
        for (i, col) in cols.iter().enumerate() {
            if i > 0 {
                qb.push(", ");
            }
            qb.push_bind(json_to_text(obj.get(*col).unwrap_or(&Value::Null)));
            qb.push("::");
            qb.push(meta.columns[*col].as_str());
        }
        qb.push(") ON CONFLICT (");
        qb.push(meta.pk.as_str());
        qb.push(") ");
        // 仅剩主键列时无可更新列，退化为 DO NOTHING
        let update_cols: Vec<&str> = cols
            .iter()
            .copied()
            .filter(|c| *c != meta.pk.as_str())
            .collect();
        if update_cols.is_empty() {
            qb.push("DO NOTHING");
        } else {
            qb.push("DO UPDATE SET ");
            for (i, col) in update_cols.iter().enumerate() {
                if i > 0 {
                    qb.push(", ");
                }
                qb.push(*col);
                qb.push(" = EXCLUDED.");
                qb.push(*col);
            }
        }

        qb.build()
            .execute(&mut *conn)
            .await
            .map_err(DataError::from)?;
    }
    Ok(())
}

/// JSON 值 → 文本绑定值（配合 ::udt 显式转换，覆盖本项目全部列类型）。
fn json_to_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Array(items) => {
            // PG 数组字面量：'{e1,e2}'；本项目数组列（cidr[]/inet[]）不含特殊字符
            let elements: Vec<String> = items
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.replace('\\', "\\\\").replace('"', "\\\""),
                    other => other.to_string().replace('\\', "\\\\").replace('"', "\\\""),
                })
                .collect();
            Some(format!("{{{}}}", elements.join(",")))
        }
        Value::Object(o) => Some(Value::Object(o.clone()).to_string()),
    }
}

/// 批量解析 UUID 字符串（非法值报校验错误）。
fn parse_uuids(table: &str, column: &str, ids: &[String]) -> DataResult<Vec<Uuid>> {
    ids.iter()
        .map(|id| {
            Uuid::parse_str(id).map_err(|_| {
                DataError::Validation(
                    msg("server.import_export.invalid_row_id")
                        .with("table", table)
                        .with("column", column)
                        .with("id", id),
                )
            })
        })
        .collect()
}
