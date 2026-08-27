//! 名称引用解析与按业务键的 UPSERT 写入。
//!
//! 解析在事务内进行：目标资源要么已在库中，要么是本事务先前步骤
//! 刚导入的行（对同一事务可见）。解析不到即报校验错误并整体回滚，
//! 避免半成品数据。写入按业务键匹配：已存在则 UPDATE，否则 INSERT
//! （主键 UUID 由数据库生成）。绑定为文本并附 `::类型` 显式转换，
//! 与库内 inet/cidr/jsonb 等列类型精确对齐。

use crate::import::rows::{TableMeta, udt_cast};
use crate::names;
use crate::spec::{TableSpec, Target};
use crate::types::{DataError, DataResult};
use ipma_common::msg;
use sqlx::{AssertSqlSafe, PgConnection};
use std::collections::{BTreeMap, HashMap};

/// 名称引用解析缓存（与连接分离，跨行复用；连接由调用方传入）。
pub struct Resolver {
    cache: HashMap<String, String>,
}

/// 引用解析失败：区分"格式错误"与"目标不存在"，均带表/行/列上下文。
fn ref_error(
    kind: &'static str,
    table: &str,
    row_no: usize,
    column: &str,
    value: &str,
) -> DataError {
    DataError::Validation(
        msg(kind)
            .with("table", table)
            .with("row", row_no)
            .with("column", column)
            .with("value", value),
    )
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// 执行单值查询（返回 id 文本）。SQL 由静态规格拼接，参数全部绑定。
    async fn query_id(
        conn: &mut PgConnection,
        sql: String,
        binds: Vec<String>,
    ) -> DataResult<Option<String>> {
        let mut query = sqlx::query_scalar::<_, String>(AssertSqlSafe(sql));
        for value in binds {
            query = query.bind(value);
        }
        query.fetch_optional(conn).await.map_err(DataError::from)
    }

    /// 带缓存的查询：命中直接返回；未命中查库，目标不存在返回 None。
    async fn cached_lookup(
        &mut self,
        conn: &mut PgConnection,
        key: String,
        sql: String,
        binds: Vec<String>,
    ) -> DataResult<Option<String>> {
        if let Some(hit) = self.cache.get(&key) {
            return Ok(Some(hit.clone()));
        }
        let Some(id) = Self::query_id(conn, sql, binds).await? else {
            return Ok(None);
        };
        self.cache.insert(key, id.clone());
        Ok(Some(id))
    }

    /// 解析引用列：返回目标 id。空值处理（可空列写 NULL）由调用方完成。
    pub async fn resolve(
        &mut self,
        conn: &mut PgConnection,
        target: &Target,
        cells: &HashMap<String, String>,
        own_col: &str,
        table: &str,
        row_no: usize,
    ) -> DataResult<String> {
        let value = cells.get(own_col).cloned().unwrap_or_default();
        let miss = |v: &str| {
            ref_error(
                "server.import_export.ref_not_found",
                table,
                row_no,
                own_col,
                v,
            )
        };
        let bad = |v: &str| {
            ref_error(
                "server.import_export.invalid_ref_format",
                table,
                row_no,
                own_col,
                v,
            )
        };
        let display = value.clone();

        let id = match target {
            Target::ByName {
                table: ref_table,
                name_col,
            } => {
                // 表名来自静态规格白名单
                let sql = format!("SELECT id::text FROM {ref_table} WHERE {name_col} = $1");
                self.cached_lookup(
                    conn,
                    format!("byname|{ref_table}|{value}"),
                    sql,
                    vec![value],
                )
                .await?
            }
            Target::OrgPath => {
                return self.resolve_org_path(conn, &value, table, row_no).await;
            }
            Target::Cidr => {
                let region = cells.get("region_name").cloned().unwrap_or_default();
                if region.is_empty() {
                    return Err(bad(&value));
                }
                self.cached_lookup(
                    conn,
                    format!("cidr|{region}|{value}"),
                    r"SELECT c.id::text
                       FROM network_cidrs c
                       JOIN network_regions r ON r.id = c.network_region_id
                       WHERE r.name = $1 AND c.name = $2"
                        .to_string(),
                    vec![region, value],
                )
                .await?
            }
            Target::RoomScoped { table: ref_table } => {
                let Some((room, name)) = names::split_room_scoped(&value) else {
                    return Err(bad(&value));
                };
                self.cached_lookup(
                    conn,
                    format!("roomscoped|{ref_table}|{room}|{name}"),
                    // 表名来自静态规格白名单
                    format!(
                        r"SELECT t.id::text
                           FROM {ref_table} t JOIN rooms r ON r.id = t.room_id
                           WHERE r.name = $1 AND t.name = $2"
                    ),
                    vec![room.to_string(), name.to_string()],
                )
                .await?
            }
            Target::Position => {
                let Some((room, cabinet, position)) = names::split_cabinet_scoped(&value) else {
                    return Err(bad(&value));
                };
                self.cached_lookup(
                    conn,
                    format!("position|{room}|{cabinet}|{position}"),
                    r"SELECT p.id::text
                       FROM positions p
                       JOIN cabinets c ON c.id = p.cabinet_id
                       JOIN rooms r ON r.id = c.room_id
                       WHERE r.name = $1 AND c.name = $2 AND p.name = $3"
                        .to_string(),
                    vec![room.to_string(), cabinet.to_string(), position.to_string()],
                )
                .await?
            }
            Target::Device => {
                return self
                    .resolve_device(conn, &value)
                    .await
                    .map_err(|_| miss(&value));
            }
            Target::DeviceNic => {
                let device = cells.get("device").cloned().unwrap_or_default();
                let device_id = self
                    .resolve_device(conn, &device)
                    .await
                    .map_err(|_| bad(&device))?;
                self.cached_lookup(
                    conn,
                    format!("nic|{device_id}|{value}"),
                    r"SELECT id::text FROM device_nics
                       WHERE device_id = $1 AND name = $2"
                        .to_string(),
                    vec![device_id, value],
                )
                .await?
            }
            Target::DeviceInterface => {
                let device = cells.get("device").cloned().unwrap_or_default();
                let device_id = self
                    .resolve_device(conn, &device)
                    .await
                    .map_err(|_| bad(&device))?;
                self.cached_lookup(
                    conn,
                    format!("iface|{device_id}|{value}"),
                    r"SELECT id::text FROM device_interfaces
                       WHERE device_id = $1 AND name = $2"
                        .to_string(),
                    vec![device_id, value],
                )
                .await?
            }
            Target::DevicePort => {
                let Some((room, device, port)) = names::split_device_scoped(&value) else {
                    return Err(bad(&value));
                };
                let device_id = self
                    .resolve_device(conn, &format!("{room}/{device}"))
                    .await
                    .map_err(|_| miss(&value))?;
                self.cached_lookup(
                    conn,
                    format!("port|{device_id}|{port}"),
                    r"SELECT id::text FROM device_ports
                       WHERE device_id = $1 AND port_number = $2"
                        .to_string(),
                    vec![device_id, port.to_string()],
                )
                .await?
            }
            Target::Endpoint { type_col } => {
                let kind = cells.get(*type_col).cloned().unwrap_or_default();
                return self
                    .resolve_endpoint(conn, &kind, &value)
                    .await
                    .map_err(|_| miss(&value));
            }
            Target::TopologyConnection => {
                let source = cells.get("source_device").cloned().unwrap_or_default();
                let target_dev = cells.get("target_device").cloned().unwrap_or_default();
                let conn_type = cells.get("connection_type").cloned().unwrap_or_default();
                let source_id = self.resolve_device(conn, &source).await.map_err(|_| {
                    ref_error(
                        "server.import_export.ref_not_found",
                        table,
                        row_no,
                        "source_device",
                        &source,
                    )
                })?;
                let target_id = self.resolve_device(conn, &target_dev).await.map_err(|_| {
                    ref_error(
                        "server.import_export.ref_not_found",
                        table,
                        row_no,
                        "target_device",
                        &target_dev,
                    )
                })?;
                // 物理连接无唯一约束，取最早一条，尽力而为
                Self::query_id(
                    conn,
                    r"SELECT id::text FROM topology_connections
                       WHERE source_device_id = $1 AND target_device_id = $2
                         AND connection_type = $3
                       ORDER BY created_at LIMIT 1"
                        .to_string(),
                    vec![source_id, target_id, conn_type],
                )
                .await?
            }
        };
        id.ok_or_else(|| miss(&display))
    }

    /// 组织路径解析：逐段下钻（根段 parent 为 NULL）。
    async fn resolve_org_path(
        &mut self,
        conn: &mut PgConnection,
        path: &str,
        table: &str,
        row_no: usize,
    ) -> DataResult<String> {
        let bad = || {
            ref_error(
                "server.import_export.invalid_ref_format",
                table,
                row_no,
                "parent_path",
                path,
            )
        };
        let miss = || {
            ref_error(
                "server.import_export.ref_not_found",
                table,
                row_no,
                "parent_path",
                path,
            )
        };
        if path.is_empty() {
            return Err(bad());
        }
        if let Some(hit) = self.cache.get(&format!("orgpath|{path}")) {
            return Ok(hit.clone());
        }
        let mut parent: Option<String> = None;
        for segment in path.split('/') {
            if segment.is_empty() {
                return Err(bad());
            }
            let id = match &parent {
                None => {
                    let sql =
                        "SELECT id::text FROM organizations WHERE parent_id IS NULL AND name = $1";
                    Self::query_id(conn, sql.to_string(), vec![segment.to_string()]).await?
                }
                Some(p) => {
                    let sql =
                        "SELECT id::text FROM organizations WHERE parent_id = $1 AND name = $2";
                    Self::query_id(conn, sql.to_string(), vec![p.clone(), segment.to_string()])
                        .await?
                }
            }
            .ok_or_else(miss)?;
            parent = Some(id);
        }
        let Some(result) = parent else {
            return Err(bad());
        };
        self.cache.insert(format!("orgpath|{path}"), result.clone());
        Ok(result)
    }

    /// 设备引用 "房间/设备名" → id。
    async fn resolve_device(&mut self, conn: &mut PgConnection, value: &str) -> DataResult<String> {
        let Some((room, device)) = names::split_room_scoped(value) else {
            return Err(DataError::Validation(msg(
                "server.import_export.invalid_ref_format",
            )));
        };
        self.cached_lookup(
            conn,
            format!("device|{room}|{device}"),
            r"SELECT d.id::text
               FROM devices d JOIN rooms r ON r.id = d.room_id
               WHERE r.name = $1 AND d.name = $2"
                .to_string(),
            vec![room.to_string(), device.to_string()],
        )
        .await?
        .ok_or_else(|| DataError::Validation(msg("server.import_export.ref_not_found")))
    }

    /// 跳接线路多态端点解析。
    async fn resolve_endpoint(
        &mut self,
        conn: &mut PgConnection,
        kind: &str,
        value: &str,
    ) -> DataResult<String> {
        let not_found = || DataError::Validation(msg("server.import_export.ref_not_found"));
        match kind {
            "device_port" | "device_interface" => {
                let Some((room, device, ident)) = names::split_device_scoped(value) else {
                    return Err(not_found());
                };
                let device_id = self
                    .resolve_device(conn, &format!("{room}/{device}"))
                    .await?;
                let (table, ident_col) = if kind == "device_port" {
                    ("device_ports", "port_number")
                } else {
                    ("device_interfaces", "name")
                };
                // 表/列名来自静态映射
                let sql = format!(
                    "SELECT id::text FROM {table} WHERE device_id = $1 AND {ident_col} = $2"
                );
                Self::query_id(conn, sql, vec![device_id, ident.to_string()])
                    .await?
                    .ok_or_else(not_found)
            }
            "net_outlet" => {
                let sql = "SELECT id::text FROM net_outlets WHERE name = $1";
                Self::query_id(conn, sql.to_string(), vec![value.to_string()])
                    .await?
                    .ok_or_else(not_found)
            }
            "patch_panel" => {
                let Some((room, cabinet, panel)) = names::split_cabinet_scoped(value) else {
                    return Err(not_found());
                };
                let sql = r"SELECT pp.id::text
                           FROM patch_panels pp
                           JOIN cabinets c ON c.id = pp.cabinet_id
                           JOIN rooms r ON r.id = c.room_id
                           WHERE r.name = $1 AND c.name = $2 AND pp.name = $3";
                Self::query_id(
                    conn,
                    sql.to_string(),
                    vec![room.to_string(), cabinet.to_string(), panel.to_string()],
                )
                .await?
                .ok_or_else(not_found)
            }
            _ => Err(DataError::Validation(
                msg("server.import_export.unknown_endpoint_type").with("type", kind),
            )),
        }
    }
}

/// 一行数据解析后的数据库值：列 → 值（None 表示显式 NULL，
/// 不在映射中的列表示缺省、由数据库默认值填充）。
pub type ResolvedValues = BTreeMap<String, Option<String>>;

/// 业务键的元组表示（用于批内去重）。
pub fn key_tuple(spec: &TableSpec, values: &ResolvedValues) -> Vec<Option<String>> {
    spec.key
        .iter()
        .map(|k| values.get(*k).cloned().flatten())
        .collect()
}

/// 按业务键匹配已有行并写入：返回 true 表示更新了已有行，false 表示新插入。
pub async fn upsert_row(
    conn: &mut PgConnection,
    spec: &TableSpec,
    meta: &TableMeta,
    values: &ResolvedValues,
) -> DataResult<bool> {
    // 业务键匹配（可空键用 IS NOT DISTINCT FROM，与 NULL 精确匹配；
    // 多行同键时取最早一条，与库内历史数据行为一致）
    let mut where_sql = String::new();
    for (i, col) in spec.key.iter().enumerate() {
        if i > 0 {
            where_sql.push_str(" AND ");
        }
        where_sql.push_str(&format!("{col} IS NOT DISTINCT FROM ${}", i + 1));
    }
    let select_sql = format!(
        "SELECT id::text FROM {} WHERE {where_sql} ORDER BY created_at LIMIT 1",
        spec.table
    );
    let mut query = sqlx::query_scalar::<_, String>(AssertSqlSafe(select_sql));
    for col in spec.key {
        query = query.bind(values.get(*col).cloned().flatten());
    }
    let existing: Option<String> = query
        .fetch_optional(&mut *conn)
        .await
        .map_err(DataError::from)?;

    let Some(existing_id) = existing else {
        // 新插入：全部列入库，UUID 由数据库生成；文本绑定附显式类型转换
        insert_row(conn, spec, meta, values).await?;
        return Ok(false);
    };

    // 更新已有行：除业务键外的全部列
    let update_cols: Vec<&str> = values
        .keys()
        .map(String::as_str)
        .filter(|c| !spec.key.contains(c))
        .collect();
    if update_cols.is_empty() {
        return Ok(true);
    }
    let set_sql: Vec<String> = update_cols
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let cast = meta
                .columns
                .get(*col)
                .map(|m| udt_cast(&m.udt))
                .unwrap_or_else(|| "text".to_string());
            format!("{} = ${}::{}", col, i + 1, cast)
        })
        .collect();
    let sql = format!(
        "UPDATE {} SET {} WHERE id = ${}::uuid",
        spec.table,
        set_sql.join(", "),
        update_cols.len() + 1
    );
    let mut query = sqlx::query(AssertSqlSafe(sql));
    for col in &update_cols {
        query = query.bind(values.get(*col).cloned().flatten());
    }
    query = query.bind(existing_id);
    query.execute(&mut *conn).await.map_err(DataError::from)?;
    Ok(true)
}

/// 插入新行（文本绑定 + 显式类型转换）。
async fn insert_row(
    conn: &mut PgConnection,
    spec: &TableSpec,
    meta: &TableMeta,
    values: &ResolvedValues,
) -> DataResult<()> {
    let cols: Vec<&str> = values.keys().map(String::as_str).collect();
    let placeholders: Vec<String> = cols
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let cast = meta
                .columns
                .get(*col)
                .map(|m| udt_cast(&m.udt))
                .unwrap_or_else(|| "text".to_string());
            format!("${}::{}", i + 1, cast)
        })
        .collect();
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        spec.table,
        cols.join(", "),
        placeholders.join(", ")
    );
    let mut query = sqlx::query(AssertSqlSafe(sql));
    for value in values.values() {
        query = query.bind(value.clone());
    }
    query.execute(&mut *conn).await.map_err(DataError::from)?;
    Ok(())
}

/// 接口 → (设备 id, 设备房间 id)。供 IP 网段归属校验使用。
pub async fn interface_owner(
    conn: &mut PgConnection,
    interface_id: &str,
) -> DataResult<Option<(String, String)>> {
    let row: Option<(String, String)> = sqlx::query_as(
        r"SELECT d.id::text, d.room_id::text
           FROM device_interfaces di JOIN devices d ON d.id = di.device_id
           WHERE di.id = $1::uuid",
    )
    .bind(interface_id)
    .fetch_optional(conn)
    .await
    .map_err(DataError::from)?;
    Ok(row)
}

/// 房间名查询（错误信息上下文用，查不到返回空串）。
pub async fn room_name_of(conn: &mut PgConnection, room_id: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT name FROM rooms WHERE id = $1::uuid")
        .bind(room_id)
        .fetch_optional(conn)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// 房间绑定的全部网段文本（ipv4_cidr/ipv6_cidr 交替列表）。
pub async fn room_cidrs(conn: &mut PgConnection, room_id: &str) -> DataResult<Vec<Option<String>>> {
    let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        r"SELECT n.ipv4_cidr::text, n.ipv6_cidr::text
           FROM room_networks rn JOIN network_cidrs n ON n.id = rn.network_id
           WHERE rn.room_id = $1::uuid",
    )
    .bind(room_id)
    .fetch_all(conn)
    .await
    .map_err(DataError::from)?;
    let mut result = Vec::new();
    for (v4, v6) in rows {
        result.push(v4);
        result.push(v6);
    }
    Ok(result)
}
