//! 模块化 CSV 数据导出与导入模板下载。
//!
//! 每张业务表导出为一个 CSV 文件（`表名.csv`），打包为 ZIP（全部模块
//! 或指定模块）。CSV 中外键一律写为业务名称（列定义见 [`crate::spec`]），
//! 不含 UUID 与时间戳列；导入端按业务键匹配资源、UUID 由数据库生成。
//! 设备表中的 SNMP 凭据字段在库内为密文，导出时解密为明文，
//! 以便跨实例导入（导入端会重新用本实例密钥加密）。
//! 文件带 UTF-8 BOM，便于 Excel 直接打开中文内容。

use crate::modules::{MODULES, ModuleDef};
use crate::spec::{Col, TABLE_SPECS, Target, headers};
use crate::types::{DataError, DataProvider, DataResult};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use foims_common::{log_warn, msg};
use serde_json::Value;
use sqlx::{AssertSqlSafe, Connection, PgConnection};
use std::collections::HashMap;
use std::io::{Cursor, Write};
use zip::{ZipWriter, write::FileOptions};

/// devices 表中密文存储的凭据列：导出时一律置空（redact），
/// 不以明文或密文形式离开本实例——密文跨实例密钥不同无法解密，
/// 明文导出则把 SNMP 口令暴露在 ZIP 文件里。列头保留以维持表结构
/// 匹配，导入端空值落 NULL（如需迁移凭据请在新实例手工补录）。
const DEVICE_SECRET_COLUMNS: &[&str] =
    &["snmp_community", "snmp_auth_password", "snmp_priv_password"];

/// 将多个文件打包为 ZIP 字节流。
pub(crate) fn zip_files(files: Vec<(String, Vec<u8>)>) -> DataResult<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let options = FileOptions::<'_, ()>::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut zip = ZipWriter::new(&mut buf);
    for (filename, data) in files {
        zip.start_file(filename.as_str(), options).map_err(|e| {
            DataError::Internal(msg("server.import_export.zip_create_failed").with("error", e))
        })?;
        zip.write_all(&data).map_err(|e| {
            DataError::Internal(msg("server.import_export.zip_write_failed").with("error", e))
        })?;
    }
    zip.finish().map_err(|e| {
        DataError::Internal(msg("server.import_export.zip_finish_failed").with("error", e))
    })?;

    Ok(buf.into_inner())
}

/// ZIP 下载响应。
fn zip_response(data: Vec<u8>, filename: String) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        data,
    )
        .into_response()
}

/// 解析导出类型参数为模块集合。
fn resolve_modules(export_type: &str) -> DataResult<Vec<&'static ModuleDef>> {
    if export_type == "all" {
        Ok(MODULES.iter().collect())
    } else {
        Ok(vec![crate::modules::find_module(export_type).ok_or_else(
            || {
                DataError::Validation(
                    msg("server.import_export.unknown_export_type").with("type", export_type),
                )
            },
        )?])
    }
}

/// 名称上下文：导出时 UUID → 业务名称的映射集合。
struct NameContext {
    /// 单列名称表：rooms / net_outlets / org_templates / device_templates / network_regions
    simple: HashMap<&'static str, HashMap<String, String>>,
    /// 组织节点：id → (parent_id, name)
    orgs: HashMap<String, (Option<String>, String)>,
    /// network_cidrs：id → (区域名, 网段名)
    cidrs: HashMap<String, (String, String)>,
    /// 房间内名称表：workstations / cabinets：id → "房间/名称"
    room_scoped: HashMap<&'static str, HashMap<String, String>>,
    /// positions：id → "房间/机柜/机位名"
    positions: HashMap<String, String>,
    /// patch_panels：id → "房间/机柜/配线架名"
    patch_panels: HashMap<String, String>,
    /// devices：id → "房间/设备名"
    devices: HashMap<String, String>,
    /// device_nics：id → 网卡名
    nics: HashMap<String, String>,
    /// device_interfaces：id → (设备 id, 接口名)
    interfaces: HashMap<String, (String, String)>,
    /// topology_connections：id → (源设备 id, 目标设备 id, 类型)
    connections: HashMap<String, (String, String, String)>,
}

impl NameContext {
    /// 组织全路径（自根起 "/" 连接）。
    fn org_path(&self, id: &str) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        let mut current = id.to_string();
        for _ in 0..1000 {
            let (parent, name) = self.orgs.get(&current)?;
            parts.push(name.clone());
            match parent {
                Some(p) => current = p.clone(),
                None => {
                    parts.reverse();
                    return Some(parts.join("/"));
                }
            }
        }
        // 深度超限视为脏数据（成环），按未找到处理
        None
    }

    /// 端口/接口统一编码为 "房间/设备名:端口名"（device_interfaces）。
    fn port_path(&self, id: &str) -> Option<String> {
        self.interface_path(id)
    }

    /// 接口编码为 "房间/设备名:接口名"。
    fn interface_path(&self, id: &str) -> Option<String> {
        let (device_id, name) = self.interfaces.get(id)?;
        Some(format!("{}:{}", self.devices.get(device_id)?, name))
    }
}

/// 加载全部名称映射（各表数据量为运维规模，一次性载入内存）。
async fn load_name_context(conn: &mut PgConnection) -> DataResult<NameContext> {
    let mut simple: HashMap<&'static str, HashMap<String, String>> = HashMap::new();
    for table in [
        "rooms",
        "net_outlets",
        "org_templates",
        "device_templates",
        "network_regions",
    ] {
        // 表名来自静态白名单，已人工审计无注入风险
        let sql = format!("SELECT id::text, name FROM {table}");
        let rows: Vec<(String, String)> = sqlx::query_as(AssertSqlSafe(sql))
            .fetch_all(&mut *conn)
            .await?;
        simple.insert(table, rows.into_iter().collect());
    }

    let orgs: Vec<(String, Option<String>, String)> =
        sqlx::query_as("SELECT id::text, parent_id::text, name FROM organizations")
            .fetch_all(&mut *conn)
            .await?;

    let cidrs: Vec<(String, String, String)> = sqlx::query_as(
        r"SELECT c.id::text, r.name, c.name
           FROM network_cidrs c JOIN network_regions r ON r.id = c.network_region_id",
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut room_scoped: HashMap<&'static str, HashMap<String, String>> = HashMap::new();
    for table in ["workstations", "cabinets"] {
        // 表名来自静态白名单，已人工审计无注入风险
        let sql = format!(
            r"SELECT t.id::text, r.name || '/' || t.name
               FROM {table} t JOIN rooms r ON r.id = t.room_id"
        );
        let rows: Vec<(String, String)> = sqlx::query_as(AssertSqlSafe(sql))
            .fetch_all(&mut *conn)
            .await?;
        room_scoped.insert(table, rows.into_iter().collect());
    }

    let positions: Vec<(String, String)> = sqlx::query_as(
        r"SELECT p.id::text, r.name || '/' || c.name || '/' || p.name
           FROM positions p
           JOIN cabinets c ON c.id = p.cabinet_id
           JOIN rooms r ON r.id = c.room_id
           WHERE p.cabinet_id IS NOT NULL",
    )
    .fetch_all(&mut *conn)
    .await?;

    let patch_panels: Vec<(String, String)> = sqlx::query_as(
        r"SELECT pp.id::text, r.name || '/' || c.name || '/' || pp.name
           FROM patch_panels pp
           JOIN cabinets c ON c.id = pp.cabinet_id
           JOIN rooms r ON r.id = c.room_id",
    )
    .fetch_all(&mut *conn)
    .await?;

    let devices: Vec<(String, String)> = sqlx::query_as(
        r"SELECT d.id::text, r.name || '/' || d.name
           FROM devices d JOIN rooms r ON r.id = d.room_id",
    )
    .fetch_all(&mut *conn)
    .await?;

    let nics: Vec<(String, String)> = sqlx::query_as("SELECT id::text, name FROM device_nics")
        .fetch_all(&mut *conn)
        .await?;

    let interfaces: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id::text, device_id::text, name FROM device_interfaces")
            .fetch_all(&mut *conn)
            .await?;

    let connections: Vec<(String, String, String, String)> = sqlx::query_as(
        r"SELECT id::text, source_device_id::text, target_device_id::text, connection_type
           FROM topology_connections",
    )
    .fetch_all(&mut *conn)
    .await?;

    Ok(NameContext {
        simple,
        orgs: orgs.into_iter().map(|(i, p, n)| (i, (p, n))).collect(),
        cidrs: cidrs.into_iter().map(|(i, r, n)| (i, (r, n))).collect(),
        room_scoped,
        positions: positions.into_iter().collect(),
        patch_panels: patch_panels.into_iter().collect(),
        devices: devices.into_iter().collect(),
        nics: nics.into_iter().collect(),
        interfaces: interfaces
            .into_iter()
            .map(|(i, d, n)| (i, (d, n)))
            .collect(),
        connections: connections
            .into_iter()
            .map(|(i, s, t, ty)| (i, (s, t, ty)))
            .collect(),
    })
}

/// 数据库 JSON 值 → CSV 单元格文本。
fn value_to_csv(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            // PG 数组字面量 '{e1,e2}'；本项目数组列（cidr[]/inet[]）不含特殊字符
            let elements: Vec<String> = items
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.replace('\\', "\\\\").replace('"', "\\\""),
                    other => other.to_string().replace('\\', "\\\\").replace('"', "\\\""),
                })
                .collect();
            format!("{{{}}}", elements.join(","))
        }
        Value::Object(_) => value.to_string(),
    }
}

/// 行中外键 UUID → 名称。
fn ref_to_csv(ctx: &NameContext, target: &Target, row: &Value, db_col: &str) -> DataResult<String> {
    let id = row.get(db_col).and_then(Value::as_str).unwrap_or_default();
    if id.is_empty() {
        return Ok(String::new());
    }
    let resolved = match target {
        Target::ByName { table, .. } => ctx
            .simple
            .get(*table)
            .and_then(|m| m.get(id))
            .cloned()
            .ok_or_else(|| {
                DataError::Internal(msg("server.import_export.name_resolve_failed").with("id", id))
            })?,
        Target::OrgPath => ctx.org_path(id).ok_or_else(|| {
            DataError::Internal(msg("server.import_export.name_resolve_failed").with("id", id))
        })?,
        Target::Cidr => ctx.cidrs.get(id).map(|(_, n)| n.clone()).ok_or_else(|| {
            DataError::Internal(msg("server.import_export.name_resolve_failed").with("id", id))
        })?,
        Target::RoomScoped { table } => ctx
            .room_scoped
            .get(*table)
            .and_then(|m| m.get(id))
            .cloned()
            .ok_or_else(|| {
                DataError::Internal(msg("server.import_export.name_resolve_failed").with("id", id))
            })?,
        Target::Position => ctx.positions.get(id).cloned().ok_or_else(|| {
            DataError::Internal(msg("server.import_export.name_resolve_failed").with("id", id))
        })?,
        Target::Device => ctx.devices.get(id).cloned().ok_or_else(|| {
            DataError::Internal(msg("server.import_export.name_resolve_failed").with("id", id))
        })?,
        Target::DeviceNic => ctx.nics.get(id).cloned().ok_or_else(|| {
            DataError::Internal(msg("server.import_export.name_resolve_failed").with("id", id))
        })?,
        Target::DeviceInterface => {
            ctx.interfaces
                .get(id)
                .map(|(_, n)| n.clone())
                .ok_or_else(|| {
                    DataError::Internal(
                        msg("server.import_export.name_resolve_failed").with("id", id),
                    )
                })?
        }
        Target::DevicePort => ctx.port_path(id).ok_or_else(|| {
            DataError::Internal(msg("server.import_export.name_resolve_failed").with("id", id))
        })?,
        Target::Endpoint { .. } => endpoint_to_csv(ctx, row, db_col)?,
        Target::TopologyConnection => {
            // 值由伴随列表达，本列输出可读回显
            ctx.connections
                .get(id)
                .map(|(s, t, _)| {
                    // 关键字段（设备名）缺失时告警而非静默占位 "?"，
                    // 便于发现导出数据不完整的行
                    let name_of = |sid: &str| match ctx.devices.get(sid) {
                        Some(n) => n.clone(),
                        None => {
                            log_warn!("log.import_export.export_ref_missing", id = sid);
                            "?".to_string()
                        }
                    };
                    format!("{} -> {}", name_of(s), name_of(t))
                })
                .unwrap_or_default()
        }
    };
    Ok(resolved)
}

/// 跳接线路端点 UUID → 按端点类型的名称路径。
fn endpoint_to_csv(ctx: &NameContext, row: &Value, id_col: &str) -> DataResult<String> {
    let id = row.get(id_col).and_then(Value::as_str).unwrap_or_default();
    let endpoint_type = row
        .get(id_col.replace("_id", "_type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let resolved = match endpoint_type {
        "device_interface" => ctx.interface_path(id),
        "net_outlet" => ctx
            .simple
            .get("net_outlets")
            .and_then(|m| m.get(id))
            .cloned(),
        "patch_panel" => ctx.patch_panels.get(id).cloned(),
        _ => None,
    };
    resolved.ok_or_else(|| {
        DataError::Internal(
            msg("server.import_export.name_resolve_failed")
                .with("id", id)
                .with("type", endpoint_type),
        )
    })
}

/// 伴随列取值（Info 列不写库，导出时由其他映射推导）。
/// 引用 ID 非空但名称解析失败时记告警，避免行数据静默不完整。
fn info_to_csv(ctx: &NameContext, table: &str, csv_col: &str, row: &Value) -> DataResult<String> {
    let id_of = |col: &str| -> String {
        row.get(col)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    // 引用 id 非空却解析不到名称 → 输出空串并告警
    let resolve_or_warn = |id: &str, name: Option<String>| -> String {
        if !id.is_empty() && name.is_none() {
            log_warn!("log.import_export.export_ref_missing", id = id);
        }
        name.unwrap_or_default()
    };
    match (table, csv_col) {
        ("room_networks", "region_name") => Ok(resolve_or_warn(
            &id_of("subnet_id"),
            ctx.cidrs.get(&id_of("subnet_id")).map(|(r, _)| r.clone()),
        )),
        ("ips", "device") => {
            let iface = id_of("device_interface_id");
            let device = ctx
                .interfaces
                .get(&iface)
                .and_then(|(d, _)| ctx.devices.get(d).cloned());
            Ok(resolve_or_warn(&iface, device))
        }
        ("ips", "region_name") => Ok(resolve_or_warn(
            &id_of("subnet_id"),
            ctx.cidrs.get(&id_of("subnet_id")).map(|(r, _)| r.clone()),
        )),
        ("topology_connection_members", "source_device") => Ok(resolve_or_warn(
            &id_of("connection_id"),
            ctx.connections
                .get(&id_of("connection_id"))
                .and_then(|(s, _, _)| ctx.devices.get(s).cloned()),
        )),
        ("topology_connection_members", "target_device") => Ok(resolve_or_warn(
            &id_of("connection_id"),
            ctx.connections
                .get(&id_of("connection_id"))
                .and_then(|(_, t, _)| ctx.devices.get(t).cloned()),
        )),
        ("topology_connection_members", "connection_type") => Ok(resolve_or_warn(
            &id_of("connection_id"),
            ctx.connections
                .get(&id_of("connection_id"))
                .map(|(_, _, ty)| ty.clone()),
        )),
        _ => Ok(String::new()),
    }
}

/// 读取一张表的全部行（JSON 对象数组），按主键排序保证导出稳定。
async fn fetch_table_rows(conn: &mut PgConnection, table: &str) -> DataResult<Vec<Value>> {
    // table 来自规格静态白名单；AssertSqlSafe 表示该拼接已经人工审计无注入风险
    let sql = format!("SELECT to_jsonb(t) FROM {table} t ORDER BY id");
    let rows: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(AssertSqlSafe(sql))
        .fetch_all(conn)
        .await
        .map_err(DataError::from)?;
    Ok(rows.into_iter().map(|j| j.0).collect())
}

/// CSV 公式注入防护：以 = + - @（及 Tab/CR）开头的单元格前置单引号，
/// 防止用户可控字段（名称/描述等）在 Excel/WPS 中被当作公式/DDE 执行
/// （security-review S-1）
fn escape_csv_formula(cell: &str) -> String {
    let dangerous = cell.starts_with('=')
        || cell.starts_with('+')
        || cell.starts_with('-')
        || cell.starts_with('@')
        || cell.starts_with('\t')
        || cell.starts_with('\r');
    if dangerous {
        format!("'{cell}")
    } else {
        cell.to_string()
    }
}

/// 将一张表写成 CSV 字节流（UTF-8 BOM + 表头 + 数据行）。
fn write_table_csv(
    spec: &crate::spec::TableSpec,
    rows: &[Value],
    ctx: &NameContext,
) -> DataResult<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    out.write_all(&[0xEF, 0xBB, 0xBF]).map_err(|e| {
        DataError::Internal(msg("server.import_export.csv_build_failed").with("error", e))
    })?;
    let mut writer = csv::Writer::from_writer(out);

    writer.write_record(headers(spec)).map_err(|e| {
        DataError::Internal(msg("server.import_export.csv_build_failed").with("error", e))
    })?;

    for row in rows {
        let mut record: Vec<String> = Vec::with_capacity(spec.columns.len());
        for col in spec.columns {
            let value = match col {
                Col::Plain(db_col) => value_to_csv(row.get(*db_col).unwrap_or(&Value::Null)),
                Col::Ref { csv: _, db, target } => ref_to_csv(ctx, target, row, db)?,
                Col::Info(csv_col) => info_to_csv(ctx, spec.table, csv_col, row)?,
            };
            record.push(escape_csv_formula(&value));
        }
        writer.write_record(&record).map_err(|e| {
            DataError::Internal(msg("server.import_export.csv_build_failed").with("error", e))
        })?;
    }

    let bytes = writer
        .into_inner()
        .map_err(|e| {
            DataError::Internal(
                msg("server.import_export.csv_build_failed").with("error", e.to_string()),
            )
        })?
        .into_inner();
    Ok(bytes)
}

/// 置空设备行中的 SNMP 凭据字段（导出脱敏，空值跳过）。
async fn redact_device_secrets(row: &mut Value) {
    let Some(obj) = row.as_object_mut() else {
        return;
    };
    for col in DEVICE_SECRET_COLUMNS {
        if obj.contains_key(*col) {
            obj.insert((*col).to_string(), Value::String(String::new()));
        }
    }
}

/// 按模块导出业务数据为 CSV（ZIP 打包）。
///
/// `type` 缺省为 `all`（全部模块）；传入模块名时仅导出该模块的表。
pub async fn export_csv<P: DataProvider>(
    provider: P,
    type_param: Query<HashMap<String, String>>,
) -> DataResult<Response> {
    let export_type = type_param
        .get("type")
        .cloned()
        .unwrap_or_else(|| "all".to_string());
    let selected = resolve_modules(&export_type)?;

    let pool = provider.pool()?;
    let mut conn = pool.acquire().await.map_err(DataError::from)?;
    // 全程单事务（REPEATABLE READ 只读）：名称上下文与各表行数据取自
    // 同一快照，避免导出期间并发写入造成跨表引用错位
    let mut tx = conn.begin().await.map_err(DataError::from)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(DataError::from)?;
    let ctx = load_name_context(&mut tx).await?;

    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for module in selected {
        for table in module.tables {
            let spec = crate::spec::find_spec(table).ok_or_else(|| {
                DataError::Internal(msg("server.import_export.spec_missing").with("table", table))
            })?;
            let mut rows = fetch_table_rows(&mut tx, table).await?;
            // 设备凭据列脱敏置空：凭据不随导出文件离开本实例
            if *table == "devices" {
                for row in &mut rows {
                    redact_device_secrets(row).await;
                }
            }
            let csv_data = write_table_csv(spec, &rows, &ctx)?;
            files.push((format!("{table}.csv"), csv_data));
        }
    }

    tx.commit().await.map_err(DataError::from)?;

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let zip_data = tokio::task::spawn_blocking(move || zip_files(files))
        .await
        .map_err(|e| {
            DataError::Internal(msg("server.import_export.zip_create_failed").with("error", e))
        })??;
    Ok(zip_response(
        zip_data,
        format!("foims_export_{export_type}_{timestamp}.zip"),
    ))
}

/// 下载导入模板：所选模块各表仅含表头的空 CSV，打包为 ZIP。
/// 不访问数据库，与导出结构完全一致，便于手工构造或对照修改。
pub async fn download_template(type_param: Query<HashMap<String, String>>) -> DataResult<Response> {
    let export_type = type_param
        .get("type")
        .cloned()
        .unwrap_or_else(|| "all".to_string());
    let selected = resolve_modules(&export_type)?;

    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for module in selected.iter() {
        for table in module.tables {
            let Some(spec) = TABLE_SPECS.iter().find(|s| s.table == *table) else {
                // 规格缺失属模块/表清单与 TABLE_SPECS 漂移，留痕排查
                foims_common::log_warn!("log.import_export.template_spec_missing", table = table);
                continue;
            };
            let mut out = Cursor::new(Vec::new());
            // BOM 写入失败按无模板处理（内存写入不会失败）
            if out.write_all(&[0xEF, 0xBB, 0xBF]).is_err() {
                continue;
            }
            let mut writer = csv::Writer::from_writer(out);
            if let Err(e) = writer.write_record(headers(spec)) {
                foims_common::log_warn!(
                    "log.import_export.template_write_failed",
                    table = table,
                    error = e
                );
                continue;
            }
            match writer.into_inner() {
                Ok(inner) => files.push((format!("{table}.csv"), inner.into_inner())),
                Err(e) => {
                    foims_common::log_warn!(
                        "log.import_export.template_write_failed",
                        table = table,
                        error = e
                    );
                }
            }
        }
    }

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let zip_data = zip_files(files)?;
    Ok(zip_response(
        zip_data,
        format!("foims_import_template_{timestamp}.zip"),
    ))
}
