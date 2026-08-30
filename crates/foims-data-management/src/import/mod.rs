//! 模块化 CSV 数据导入。
//!
//! 导入流程：读取上传文件（ZIP 内多个 `表名.csv` 或单个 CSV）→
//! 校验真实类型（magic bytes + 扩展名 + UTF-8）→ 严格解析并逐字段清洗 →
//! 按固定模块顺序（组织→房间→网络区域→机柜→设备→线路→可视化）在
//! 同一事务内逐表处理：名称引用解析为 UUID、业务键匹配（存在则更新、
//! 不存在则插入且 UUID 由数据库生成）、专项校验（IP 归属房间网段等）。
//! 任一步失败则整批回滚，避免半成品数据。

pub mod org_validate;
pub mod resolve;
pub mod rows;

use crate::modules::MODULES;
use crate::spec::{self, COMPOSITE_NAME_TABLES, Col, TableSpec};
use crate::types::{DataError, DataProvider, DataResult, ok_json};
use axum::extract::Multipart;
use axum::response::Response;
use foims_common::msg;
use resolve::{ResolvedValues, Resolver, key_tuple, upsert_row};
use rows::{
    CsvRow, TableBundle, TableMeta, detect_by_header, ensure_header_matches, fetch_table_meta,
    parse_csv, validate_value,
};
use serde_json::json;
use sqlx::{Connection, PgConnection};
use std::collections::{HashMap, HashSet};
use std::io::Read;

/// devices 表中明文传入、落库前需加密的凭据列（与导出端对应）。
pub(crate) const DEVICE_SECRET_COLUMNS: &[&str] =
    &["snmp_community", "snmp_auth_password", "snmp_priv_password"];

pub async fn import_csv<P: DataProvider>(
    provider: P,
    mut payload: Multipart,
) -> DataResult<Response> {
    let (filename, file_data) = read_upload(&mut payload).await?;
    let bundles = parse_bundles(&filename, &file_data)?;

    let pool = provider.pool()?;
    let mut conn = pool.acquire().await.map_err(DataError::from)?;
    let mut tx = conn.begin().await.map_err(DataError::from)?;

    let mut summary: Vec<serde_json::Value> = Vec::new();
    // 固定模块顺序：ZIP 内文件顺序不影响导入结果
    for module in MODULES {
        for table in module.tables {
            let Some(spec) = spec::find_spec(table) else {
                continue;
            };
            let Some(bundle) = bundles.iter().find(|b| b.spec.table == *table) else {
                continue;
            };
            if bundle.rows.is_empty() {
                continue;
            }

            let meta = fetch_table_meta(&mut tx, table).await?;
            let mut resolver = Resolver::new();
            // 组织模板 levels 全集（懒加载）：organizations.type_path 的
            // 可解析性校验需要在模板表导入完成后读取全部模板 levels
            let mut template_levels: Option<Vec<serde_json::Value>> = None;
            let mut inserted = 0u64;
            let mut updated = 0u64;
            // 组织树按父路径深度排序，保证父节点先入库：
            // 根节点 parent_path 为空串（split 计数恒为 1，须特判为 0），
            // 一级子节点为 1、孙节点为 2，依此类推；同深度保持 CSV 原顺序
            //（稳定排序）。否则根节点与一级子节点深度相同、顺序随导出
            // ORDER BY id（UUID 随机）漂移，子节点先入库会因父缺失整批回滚
            let mut ordered = bundle.rows.clone();
            if *table == "organizations" {
                ordered.sort_by_key(|r| {
                    r.cells
                        .get("parent_path")
                        .map(|p| {
                            if p.is_empty() {
                                0
                            } else {
                                p.split('/').count()
                            }
                        })
                        .unwrap_or(0)
                });
            }

            let mut seen_keys: HashSet<Vec<Option<String>>> = HashSet::new();
            let mut room_cidr_cache: HashMap<String, Vec<Option<String>>> = HashMap::new();
            for row in &ordered {
                let values =
                    resolve_row(&mut tx, &mut resolver, &provider, spec, &meta, row).await?;
                // 组织结构校验（与 API 端口径对齐，防止导入病态数据使
                // 组织相关接口持续报错）：模板 levels 结构、组织 type_path 可解析性
                match *table {
                    "org_templates" => validate_template_levels(&values, row)?,
                    "organizations" => {
                        validate_org_type_path(&mut tx, &mut template_levels, &values, row).await?
                    }
                    _ => {}
                }
                if !seen_keys.insert(key_tuple(spec, &values)) {
                    return Err(DataError::Validation(
                        msg("server.import_export.duplicate_key")
                            .with("table", table)
                            .with("row", row.row_no),
                    ));
                }
                if *table == "ips" {
                    validate_ip_in_room(&mut tx, &mut room_cidr_cache, &values, row).await?;
                }
                if upsert_row(&mut tx, spec, &meta, &values).await? {
                    updated += 1;
                } else {
                    inserted += 1;
                }
            }
            summary.push(json!({
                "table": table,
                "inserted": inserted,
                "updated": updated,
            }));
        }
    }

    tx.commit().await.map_err(DataError::from)?;

    Ok(ok_json(
        json!({"results": summary}),
        "server.import_export.import_success",
    ))
}

/// 读取上传文件字段：返回（原始文件名, 内容），限制 50MB。
async fn read_upload(payload: &mut Multipart) -> DataResult<(String, Vec<u8>)> {
    const MAX_UPLOAD_SIZE: usize = 50 * 1024 * 1024;
    while let Some(mut field) = payload.next_field().await.map_err(|e| {
        DataError::Internal(msg("server.import_export.file_read_failed").with("error", e))
    })? {
        if field.name() == Some("file") {
            let filename = field.file_name().unwrap_or_default().to_string();
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
            if filename.is_empty() || data.is_empty() {
                return Err(DataError::Validation(msg(
                    "server.import_export.no_file_selected",
                )));
            }
            return Ok((filename, data));
        }
    }
    Err(DataError::Validation(msg(
        "server.import_export.no_file_selected",
    )))
}

/// 解析上传内容：ZIP（多个 `表名.csv`，按文件名匹配表）或单个 CSV
/// （只校验后缀，不比对文件名，按表头识别表）。两处均做真实类型校验，
/// 防止伪装扩展名的文件上传漏洞。
fn parse_bundles(filename: &str, data: &[u8]) -> DataResult<Vec<TableBundle>> {
    let is_zip = data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06");
    if is_zip {
        if !filename.to_lowercase().ends_with(".zip") {
            return Err(DataError::Validation(msg(
                "server.import_export.invalid_file_type",
            )));
        }
        let entries = read_zip_entries(data)?;
        let mut bundles: Vec<TableBundle> = Vec::new();
        let mut seen: HashSet<&'static str> = HashSet::new();
        for (name, content) in entries {
            let stem = name
                .strip_suffix(".csv")
                .ok_or_else(|| {
                    DataError::Validation(
                        msg("server.import_export.invalid_zip_entry").with("name", &name),
                    )
                })?
                .to_string();
            let spec = spec::find_spec(&stem).ok_or_else(|| {
                DataError::Validation(
                    msg("server.import_export.invalid_zip_entry").with("name", &name),
                )
            })?;
            if !seen.insert(spec.table) {
                return Err(DataError::Validation(
                    msg("server.import_export.duplicate_table").with("table", spec.table),
                ));
            }
            // 解压后同样校验 CSV 格式与表头
            if std::str::from_utf8(&content).is_err() || content.contains(&0) {
                return Err(DataError::Validation(
                    msg("server.import_export.invalid_zip_entry").with("name", &name),
                ));
            }
            let (headers, csv_rows) = parse_csv(&content)?;
            ensure_header_matches(&headers, spec)?;
            bundles.push(TableBundle {
                spec,
                rows: csv_rows,
            });
        }
        Ok(bundles)
    } else {
        if !filename.to_lowercase().ends_with(".csv") {
            return Err(DataError::Validation(msg(
                "server.import_export.invalid_file_type",
            )));
        }
        // CSV 必须是纯 UTF-8 文本且不含 NUL（防伪装成文本的其他内容）
        if std::str::from_utf8(data).is_err() || data.contains(&0) {
            return Err(DataError::Validation(msg(
                "server.import_export.invalid_file_type",
            )));
        }
        let (headers, csv_rows) = parse_csv(data)?;
        let spec = detect_by_header(&headers)?;
        if csv_rows.is_empty() {
            return Err(DataError::Validation(msg(
                "server.import_export.no_data_rows",
            )));
        }
        Ok(vec![TableBundle {
            spec,
            rows: csv_rows,
        }])
    }
}

/// 读取 ZIP 内全部条目（仅接受扁平的 `.csv` 文件，含 zip bomb 防护）。
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
            return Err(DataError::Validation(
                msg("server.import_export.invalid_zip_entry").with("name", &name),
            ));
        }
        if !name.ends_with(".csv") {
            return Err(DataError::Validation(
                msg("server.import_export.invalid_zip_entry").with("name", &name),
            ));
        }
        let mut limited = (&mut file).take(MAX_DECOMPRESSED_SIZE);
        let mut content = Vec::new();
        Read::read_to_end(&mut limited, &mut content).map_err(|e| {
            DataError::Internal(msg("server.import_export.zip_entry_read_failed").with("error", e))
        })?;
        // 读取长度达到单条目上限即视为超限：take(MAX) 会静默截断超限内容，
        // 截断点落在记录/UTF-8 边界时数据无声丢失，必须显式拒绝
        if content.len() as u64 >= MAX_DECOMPRESSED_SIZE {
            return Err(DataError::Validation(msg(
                "server.import_export.file_too_large",
            )));
        }
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
            "server.import_export.no_csv_files",
        )));
    }
    Ok(entries)
}

/// 单行解析：普通列清洗校验、引用列解析为 UUID、设备凭据加密。
/// 返回 数据库列 → 值（None 显式 NULL；缺省列不在映射中）。
async fn resolve_row<P: DataProvider>(
    conn: &mut PgConnection,
    resolver: &mut Resolver,
    provider: &P,
    spec: &TableSpec,
    meta: &TableMeta,
    row: &CsvRow,
) -> DataResult<ResolvedValues> {
    let mut values: ResolvedValues = ResolvedValues::new();

    for col in spec.columns {
        match col {
            Col::Info(_) => {}
            Col::Plain(db_col) => {
                let raw = row.cells.get(*db_col).cloned().unwrap_or_default();
                let column_meta = meta.columns.get(*db_col).ok_or_else(|| {
                    DataError::Internal(
                        msg("server.import_export.spec_missing")
                            .with("table", spec.table)
                            .with("column", db_col),
                    )
                })?;
                if raw.is_empty() {
                    if column_meta.nullable {
                        values.insert(db_col.to_string(), None);
                    } else if !column_meta.has_default {
                        return Err(DataError::Validation(
                            msg("server.import_export.required_field_missing")
                                .with("table", spec.table)
                                .with("row", row.row_no)
                                .with("column", db_col),
                        ));
                    }
                    // 有默认值的列缺省，交给数据库填充
                    continue;
                }
                // 参与复合引用（"/" 路径）的名称列禁止包含分隔符
                if COMPOSITE_NAME_TABLES.contains(&(spec.table, *db_col)) && raw.contains('/') {
                    return Err(DataError::Validation(
                        msg("server.import_export.name_contains_separator")
                            .with("table", spec.table)
                            .with("row", row.row_no)
                            .with("column", db_col),
                    ));
                }
                if let Err(reason) = validate_value(column_meta, &raw) {
                    return Err(DataError::Validation(
                        msg("server.import_export.field_invalid")
                            .with("table", spec.table)
                            .with("row", row.row_no)
                            .with("column", db_col)
                            .with("value", &raw)
                            .with("reason", reason),
                    ));
                }
                values.insert(db_col.to_string(), Some(raw));
            }
            Col::Ref { csv, db, target } => {
                let raw = row.cells.get(*csv).cloned().unwrap_or_default();
                let nullable = meta.columns.get(*db).is_some_and(|m| m.nullable);
                if raw.is_empty() {
                    if nullable {
                        values.insert(db.to_string(), None);
                    } else {
                        return Err(DataError::Validation(
                            msg("server.import_export.required_field_missing")
                                .with("table", spec.table)
                                .with("row", row.row_no)
                                .with("column", csv),
                        ));
                    }
                    continue;
                }
                let id = resolver
                    .resolve(conn, target, &row.cells, csv, spec.table, row.row_no)
                    .await?;
                values.insert(db.to_string(), Some(id));
            }
        }
    }

    // 设备凭据列明文 → 本实例密文
    if spec.table == "devices" {
        for col in DEVICE_SECRET_COLUMNS {
            if let Some(Some(plain)) = values.get(*col) {
                if plain.is_empty() {
                    continue;
                }
                let encrypted = provider.encrypt_password(plain).await?;
                // CSV 按“明文长度 ≤ 列宽”校验，而凭据以密文落库（+28 字节
                // 再 base64 膨胀），明文贴着列宽上限时密文可能溢出；
                // 提前给出明确的校验错误，而非等到写入期 value too long
                if let Some(len) = meta.columns.get(*col).and_then(|m| m.char_len)
                    && encrypted.chars().count() > len as usize
                {
                    return Err(DataError::Validation(
                        msg("server.import_export.field_invalid")
                            .with("table", spec.table)
                            .with("row", row.row_no)
                            .with("column", *col)
                            .with(
                                "reason",
                                format!("凭据加密后超过列宽 {len}，请缩短明文长度"),
                            ),
                    ));
                }
                values.insert(col.to_string(), Some(encrypted));
            }
        }
    }

    Ok(values)
}

/// 专项校验：IP 地址必须属于设备所在房间绑定的网段
/// （房间未绑定任何网段时不做归属限制；device_macs 为 SNMP 发现数据，
/// 可能含外来地址，不做此校验）。
async fn validate_ip_in_room(
    conn: &mut PgConnection,
    room_cidr_cache: &mut HashMap<String, Vec<Option<String>>>,
    values: &ResolvedValues,
    row: &CsvRow,
) -> DataResult<()> {
    let ip_str = values
        .get("ip_address")
        .cloned()
        .flatten()
        .unwrap_or_default();
    let interface_id = values
        .get("device_interface_id")
        .cloned()
        .flatten()
        .unwrap_or_default();
    let Some(ip) = crate::names::parse_inet(&ip_str) else {
        return Err(DataError::Validation(
            msg("server.import_export.field_invalid")
                .with("table", "ips")
                .with("row", row.row_no)
                .with("column", "ip_address")
                .with("value", &ip_str)
                .with("reason", "不是有效的 IP 地址"),
        ));
    };

    let Some((_, room_id)) = resolve::interface_owner(conn, &interface_id).await? else {
        return Err(DataError::Validation(
            msg("server.import_export.ref_not_found")
                .with("table", "ips")
                .with("row", row.row_no)
                .with("column", "interface_name")
                .with(
                    "value",
                    row.cells.get("interface_name").cloned().unwrap_or_default(),
                ),
        ));
    };

    let cidrs = if let Some(hit) = room_cidr_cache.get(&room_id) {
        hit.clone()
    } else {
        let loaded = resolve::room_cidrs(conn, &room_id).await?;
        room_cidr_cache.insert(room_id.clone(), loaded.clone());
        loaded
    };
    if cidrs.is_empty() {
        // 房间未绑定网段：无法限定归属，跳过
        return Ok(());
    }

    let matched = cidrs
        .iter()
        .flatten()
        .filter_map(|c| crate::names::parse_cidr(c))
        .any(|(net, prefix)| crate::names::ip_in_cidr(ip, net, prefix));
    if !matched {
        let room_name = resolve::room_name_of(conn, &room_id).await;
        return Err(DataError::Validation(
            msg("server.import_export.ip_not_in_room_subnet")
                .with("table", "ips")
                .with("row", row.row_no)
                .with("ip", &ip_str)
                .with("room", room_name),
        ));
    }
    Ok(())
}

/// 组织模板行结构校验：levels 走与 API 端相同的映射校验
///（根唯一/子级类型已定义/环检测/深度上限），不合格整批回滚。
/// 错误消息沿用 `server.org_template.validation.*`，与 API 校验提示一致。
fn validate_template_levels(values: &ResolvedValues, row: &CsvRow) -> DataResult<()> {
    let Some(Some(levels_text)) = values.get("levels") else {
        // 缺省交由数据库 NOT NULL 约束报告
        return Ok(());
    };
    let levels: serde_json::Value = serde_json::from_str(levels_text).map_err(|e| {
        DataError::Validation(
            msg("server.data-management.field_invalid")
                .with("table", "org_templates")
                .with("row", row.row_no)
                .with("column", "levels")
                .with("value", levels_text)
                .with("reason", format!("不是有效的 JSON: {e}")),
        )
    })?;
    org_validate::validate_levels_mapping(&levels).map_err(DataError::Validation)
}

/// 组织行 type_path 校验：在模板表导入完成后（同一事务内可见），
/// type_path 必须能被某个模板的 levels 解析（根锚点 0 + 逐级索引导航），
/// 不可解析拒绝该行，避免存量节点在组织编辑接口持续报错。
async fn validate_org_type_path(
    conn: &mut PgConnection,
    cache: &mut Option<Vec<serde_json::Value>>,
    values: &ResolvedValues,
    row: &CsvRow,
) -> DataResult<()> {
    let Some(Some(type_path)) = values.get("type_path") else {
        return Ok(());
    };
    if cache.is_none() {
        let levels: Vec<serde_json::Value> = sqlx::query_scalar("SELECT levels FROM org_templates")
            .fetch_all(&mut *conn)
            .await
            .map_err(DataError::from)?;
        *cache = Some(levels);
    }
    let resolvable = cache.as_ref().is_some_and(|levels| {
        levels
            .iter()
            .any(|l| org_validate::type_path_resolvable(l, type_path))
    });
    if !resolvable {
        return Err(DataError::Validation(
            msg("server.data-management.field_invalid")
                .with("table", "organizations")
                .with("row", row.row_no)
                .with("column", "type_path")
                .with("value", type_path)
                .with("reason", "无法被任何组织模板解析"),
        ));
    }
    Ok(())
}
