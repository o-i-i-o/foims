//! CSV 数据导出与导入模板下载。
//!
//! 每类资源一个导出函数：查询（列名别名访问）→ 组装行 → 经
//! `build_csv` 统一写出（csv crate 处理引号转义，带 UTF-8 BOM 便于
//! Excel 直接打开中文），最后由 `zip_csv_files` 统一打包下载。

use crate::types::{DataError, DataProvider, DataResult};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use sqlx::Row;
use std::collections::HashMap;
use std::io::{Cursor, Write};
use zip::{ZipWriter, write::FileOptions};

/// UTF-8 BOM：让 Excel 以 UTF-8 打开 CSV 中的中文。
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// 写出 CSV：表头 + 数据行，返回带 UTF-8 BOM 的字节流。
///
/// csv crate 的默认引号策略（仅在必要时转义）与原手写
/// `escape_csv_field` 语义一致，且覆盖 `\r` 等更多控制字符。
fn build_csv(header: &[&str], rows: Vec<Vec<String>>) -> DataResult<Vec<u8>> {
    let write_err = |e: csv::Error| DataError::Internal(format!("生成CSV失败: {e}"));
    let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
    writer.write_record(header).map_err(write_err)?;
    for row in rows {
        writer.write_record(row).map_err(write_err)?;
    }
    let data = writer
        .into_inner()
        .map_err(|e| DataError::Internal(format!("生成CSV失败: {e}")))?;

    let mut out = UTF8_BOM.to_vec();
    out.extend_from_slice(&data);
    Ok(out)
}

/// 将多个文件打包为 ZIP 字节流。
fn zip_files(files: Vec<(&str, Vec<u8>)>) -> DataResult<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let options = FileOptions::<'_, ()>::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut zip = ZipWriter::new(&mut buf);
    for (filename, data) in files {
        zip.start_file(filename, options)
            .map_err(|e| DataError::Internal(format!("创建ZIP文件失败: {e}")))?;
        zip.write_all(&data)
            .map_err(|e| DataError::Internal(format!("写入ZIP文件失败: {e}")))?;
    }
    zip.finish()
        .map_err(|e| DataError::Internal(format!("完成ZIP文件失败: {e}")))?;

    Ok(buf.into_inner())
}

/// 按类型导出业务数据为 CSV（多文件时打包为 ZIP 下载）。
///
/// `type` 缺省为 `all`，导出全部七类资源；ZIP 压缩在阻塞线程池执行。
pub async fn export_csv<P: DataProvider>(
    provider: P,
    type_param: Query<HashMap<String, String>>,
) -> DataResult<Response> {
    let pool = provider.pool()?;
    let mut conn = pool.acquire().await.map_err(DataError::from)?;

    let export_type = type_param
        .get("type")
        .cloned()
        .unwrap_or_else(|| "all".to_string());
    let wanted = |name: &str| export_type == "all" || export_type == name;

    let mut csv_data: Vec<(&str, Vec<u8>)> = Vec::new();
    if wanted("network_regions") {
        csv_data.push((
            "network_regions.csv",
            export_network_regions(&mut conn).await?,
        ));
    }
    if wanted("networks") {
        csv_data.push(("networks.csv", export_networks(&mut conn).await?));
    }
    if wanted("rooms") {
        csv_data.push(("rooms.csv", export_rooms(&mut conn).await?));
    }
    if wanted("workstations") {
        csv_data.push(("workstations.csv", export_workstations(&mut conn).await?));
    }
    if wanted("cabinets") {
        csv_data.push(("cabinets.csv", export_cabinets(&mut conn).await?));
    }
    if wanted("positions") {
        csv_data.push(("positions.csv", export_positions(&mut conn).await?));
    }
    if wanted("switches") {
        csv_data.push(("switches.csv", export_switches(&mut conn, &provider).await?));
    }
    if wanted("ips") {
        csv_data.push(("ip_managers.csv", export_ip_managers(&mut conn).await?));
    }

    let buf = tokio::task::spawn_blocking(move || zip_files(csv_data))
        .await
        .map_err(|e| DataError::Internal(format!("ZIP压缩任务失败: {e}")))??;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=ipma_export_{}_{}.zip",
                    export_type,
                    chrono::Utc::now().format("%Y%m%d_%H%M%S")
                ),
            ),
        ],
        buf,
    )
        .into_response())
}

/// 导出网络区域。
async fn export_network_regions(conn: &mut sqlx::PgConnection) -> DataResult<Vec<u8>> {
    let rows = sqlx::query("SELECT name, description FROM network_regions ORDER BY name")
        .fetch_all(&mut *conn)
        .await
        .map_err(DataError::from)?;

    let records = rows
        .into_iter()
        .map(|row| {
            vec![
                row.get::<String, _>("name"),
                row.get::<Option<String>, _>("description")
                    .unwrap_or_default(),
            ]
        })
        .collect();

    build_csv(&["名称", "描述"], records)
}

/// 导出网段（含区域、CIDR、网关、DNS）。
async fn export_networks(conn: &mut sqlx::PgConnection) -> DataResult<Vec<u8>> {
    let rows = sqlx::query(
        r"SELECT n.name, nr.name as region_name,
           n.ipv4_cidr::TEXT as ipv4_cidr, n.ipv6_cidr::TEXT as ipv6_cidr,
           n.ipv4_gateway::TEXT as ipv4_gateway, n.ipv6_gateway::TEXT as ipv6_gateway,
           COALESCE(array_to_string(n.ipv4_dns, ','), '') as ipv4_dns,
           COALESCE(array_to_string(n.ipv6_dns, ','), '') as ipv6_dns,
           n.description
           FROM network_cidrs n
           JOIN network_regions nr ON n.network_region_id = nr.id
           ORDER BY nr.name, n.name",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

    let records = rows
        .into_iter()
        .map(|row| {
            vec![
                row.get::<String, _>("name"),
                row.get::<String, _>("region_name"),
                row.get::<Option<String>, _>("ipv4_cidr")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("ipv6_cidr")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("ipv4_gateway")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("ipv6_gateway")
                    .unwrap_or_default(),
                row.get::<String, _>("ipv4_dns"),
                row.get::<String, _>("ipv6_dns"),
                row.get::<Option<String>, _>("description")
                    .unwrap_or_default(),
            ]
        })
        .collect();

    build_csv(
        &[
            "名称",
            "网络区域",
            "IPv4 CIDR",
            "IPv6 CIDR",
            "IPv4网关",
            "IPv6网关",
            "IPv4 DNS",
            "IPv6 DNS",
            "描述",
        ],
        records,
    )
}

/// 导出房间（动态列：每行网络的“区域/名称”横向展开）。
async fn export_rooms(conn: &mut sqlx::PgConnection) -> DataResult<Vec<u8>> {
    let rooms = sqlx::query("SELECT id, name, room_type FROM rooms ORDER BY name")
        .fetch_all(&mut *conn)
        .await
        .map_err(DataError::from)?;

    let mut room_networks_map: HashMap<uuid::Uuid, Vec<String>> = HashMap::new();
    let mut max_networks = 0;

    for room in &rooms {
        let room_id: uuid::Uuid = room.get("id");
        let networks: Vec<String> = sqlx::query_scalar(
            r"SELECT nr.name || '/' || n.name FROM room_networks rn
               JOIN network_cidrs n ON rn.network_id = n.id
               JOIN network_regions nr ON n.network_region_id = nr.id
               WHERE rn.room_id = $1
               ORDER BY rn.created_at",
        )
        .bind(room_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(DataError::from)?;

        max_networks = max_networks.max(networks.len());
        room_networks_map.insert(room_id, networks);
    }

    let mut header: Vec<String> = vec!["名称".to_string(), "类型".to_string()];
    for i in 1..=max_networks {
        header.push(format!("网络{i}"));
    }

    let records = rooms
        .iter()
        .map(|room| {
            let room_id: uuid::Uuid = room.get("id");
            let name: String = room.get("name");
            let room_type: String = room.get("room_type");
            let networks = room_networks_map.get(&room_id).cloned().unwrap_or_default();

            let room_type_display = match room_type.as_str() {
                "OFFICE" => "办公室",
                "DATA_CENTER" => "数据中心",
                "TELECOM_CLOSET" => "弱电井",
                _ => &room_type,
            };

            let mut record = vec![name, room_type_display.to_string()];
            for i in 0..max_networks {
                record.push(networks.get(i).cloned().unwrap_or_default());
            }
            record
        })
        .collect();

    build_csv(
        &header.iter().map(String::as_str).collect::<Vec<_>>(),
        records,
    )
}

/// 导出工位（含首个绑定 IP）。
async fn export_workstations(conn: &mut sqlx::PgConnection) -> DataResult<Vec<u8>> {
    let rows = sqlx::query(
        r"SELECT w.name, r.name as room_name, w.manager, w.description,
                  (SELECT host(i.ip_address) FROM ips i
                   JOIN device_interfaces di ON i.device_interface_id = di.id
                   JOIN devices d ON di.device_id = d.id
                   WHERE d.workstation_id = w.id LIMIT 1) as ip_address
           FROM workstations w
           JOIN rooms r ON w.room_id = r.id
           ORDER BY r.name, w.name",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

    let records = rows
        .into_iter()
        .map(|row| {
            vec![
                row.get::<String, _>("name"),
                row.get::<String, _>("room_name"),
                row.get::<Option<String>, _>("ip_address")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("manager").unwrap_or_default(),
                row.get::<Option<String>, _>("description")
                    .unwrap_or_default(),
            ]
        })
        .collect();

    build_csv(&["名称", "房间", "IP地址", "负责人", "描述"], records)
}

/// 导出机柜（动态列：机房网络横向展开）。
async fn export_cabinets(conn: &mut sqlx::PgConnection) -> DataResult<Vec<u8>> {
    let cabinets = sqlx::query(
        r"SELECT c.id, c.name, r.name as room_name, c.description
           FROM cabinets c
           JOIN rooms r ON c.room_id = r.id
           ORDER BY r.name, c.name",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

    let mut cabinet_networks_map: HashMap<uuid::Uuid, Vec<String>> = HashMap::new();
    let mut max_networks = 0;

    for cabinet in &cabinets {
        let cabinet_id: uuid::Uuid = cabinet.get("id");
        let networks: Vec<String> = sqlx::query_scalar(
            r"SELECT DISTINCT nr.name || '/' || n.name
               FROM cabinets c
               JOIN room_networks rn ON c.room_id = rn.room_id
               JOIN network_cidrs n ON rn.network_id = n.id
               JOIN network_regions nr ON n.network_region_id = nr.id
               WHERE c.id = $1
               ORDER BY 1",
        )
        .bind(cabinet_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(DataError::from)?;

        max_networks = max_networks.max(networks.len());
        cabinet_networks_map.insert(cabinet_id, networks);
    }

    let mut header: Vec<String> = vec!["名称".to_string(), "房间".to_string()];
    for i in 1..=max_networks {
        header.push(format!("网络{i}"));
    }
    header.push("描述".to_string());

    let records = cabinets
        .iter()
        .map(|cabinet| {
            let cabinet_id: uuid::Uuid = cabinet.get("id");
            let networks = cabinet_networks_map
                .get(&cabinet_id)
                .cloned()
                .unwrap_or_default();

            let mut record = vec![
                cabinet.get::<String, _>("name"),
                cabinet.get::<String, _>("room_name"),
            ];
            for i in 0..max_networks {
                record.push(networks.get(i).cloned().unwrap_or_default());
            }
            record.push(
                cabinet
                    .get::<Option<String>, _>("description")
                    .unwrap_or_default(),
            );
            record
        })
        .collect();

    build_csv(
        &header.iter().map(String::as_str).collect::<Vec<_>>(),
        records,
    )
}

/// 导出机位（含首个绑定 IP）。
async fn export_positions(conn: &mut sqlx::PgConnection) -> DataResult<Vec<u8>> {
    let rows = sqlx::query(
        r"SELECT p.name, c.name as cabinet_name, p.start_u, p.end_u, p.description,
                  (SELECT host(i.ip_address) FROM ips i
                   JOIN device_interfaces di ON i.device_interface_id = di.id
                   JOIN devices d ON di.device_id = d.id
                   WHERE d.position_id = p.id LIMIT 1) as ip_address
           FROM positions p
           JOIN cabinets c ON p.cabinet_id = c.id
           ORDER BY c.name, p.start_u",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

    let records = rows
        .into_iter()
        .map(|row| {
            vec![
                row.get::<String, _>("name"),
                row.get::<String, _>("cabinet_name"),
                row.get::<i32, _>("start_u").to_string(),
                row.get::<i32, _>("end_u").to_string(),
                row.get::<Option<String>, _>("ip_address")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("description")
                    .unwrap_or_default(),
            ]
        })
        .collect();

    build_csv(
        &["名称", "机柜", "起始U", "结束U", "IP地址", "描述"],
        records,
    )
}

/// 导出交换机清单（SNMP Community 解密后明文导出，供运维迁移使用）。
async fn export_switches<P: DataProvider>(
    conn: &mut sqlx::PgConnection,
    provider: &P,
) -> DataResult<Vec<u8>> {
    let rows = sqlx::query(
        r"SELECT d.name, d.model, d.brand, d.location, d.snmp_version, d.snmp_port,
           d.snmp_community, d.snmp_username, d.description,
           (SELECT host(i.ip_address) FROM ips i
            JOIN device_interfaces di ON i.device_interface_id = di.id
            WHERE di.device_id = d.id ORDER BY i.created_at LIMIT 1) as ip_address
           FROM devices d
           WHERE d.device_type = 'switch'
           ORDER BY d.name",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

    let mut records = Vec::with_capacity(rows.len());
    for row in rows {
        // Community 以 AES-GCM 加密存储，导出时解密还原
        let snmp_community: Option<String> = row.get("snmp_community");
        let decrypted_community = if let Some(c) = snmp_community.filter(|c| !c.is_empty()) {
            Some(provider.decrypt_password(&c).await?)
        } else {
            None
        };

        records.push(vec![
            row.get::<String, _>("name"),
            row.get::<Option<String>, _>("ip_address")
                .unwrap_or_default(),
            row.get::<Option<String>, _>("model").unwrap_or_default(),
            row.get::<Option<String>, _>("brand").unwrap_or_default(),
            row.get::<Option<String>, _>("location").unwrap_or_default(),
            row.get::<Option<String>, _>("snmp_version")
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "v2c".to_string()),
            row.get::<Option<i32>, _>("snmp_port")
                .unwrap_or(161)
                .to_string(),
            decrypted_community.unwrap_or_default(),
            row.get::<Option<String>, _>("snmp_username")
                .unwrap_or_default(),
            row.get::<Option<String>, _>("description")
                .unwrap_or_default(),
        ]);
    }

    build_csv(
        &[
            "名称",
            "IP地址",
            "型号",
            "品牌",
            "位置",
            "SNMP版本",
            "SNMP端口",
            "SNMP Community",
            "SNMP用户名",
            "描述",
        ],
        records,
    )
}

/// 导出全部 IP 绑定明细（工位/机位 + 网络 + MAC/状态）。
async fn export_ip_managers(conn: &mut sqlx::PgConnection) -> DataResult<Vec<u8>> {
    let rows = sqlx::query(
        r"SELECT w.name as workstation_name, p.name as position_name,
           nr.name as network_region, n.name as network_name, host(im.ip_address) as ip_address,
           di.mac_address, d.hostname, im.status
           FROM ips im
           JOIN device_interfaces di ON im.device_interface_id = di.id
           JOIN devices d ON di.device_id = d.id
           LEFT JOIN workstations w ON d.workstation_id = w.id
           LEFT JOIN positions p ON d.position_id = p.id
           LEFT JOIN network_cidrs n ON im.network_id = n.id
           LEFT JOIN network_regions nr ON n.network_region_id = nr.id
           ORDER BY im.ip_address",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

    let records = rows
        .into_iter()
        .map(|row| {
            vec![
                row.get::<Option<String>, _>("workstation_name")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("position_name")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("network_region")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("network_name")
                    .unwrap_or_default(),
                row.get::<String, _>("ip_address"),
                row.get::<Option<String>, _>("mac_address")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("hostname").unwrap_or_default(),
                row.get::<String, _>("status"),
            ]
        })
        .collect();

    build_csv(
        &[
            "工位",
            "机位",
            "网络区域",
            "网络",
            "IP地址",
            "MAC地址",
            "主机名",
            "状态",
        ],
        records,
    )
}

/// 下载导入模板（静态示例行，多个模板打包为 ZIP）。
pub async fn download_template(type_param: Query<HashMap<String, String>>) -> DataResult<Response> {
    let template_type = type_param
        .get("type")
        .cloned()
        .unwrap_or_else(|| "all".to_string());
    let wanted = |name: &str| template_type == "all" || template_type == name;

    let simple_template = |content: &str| -> Vec<u8> {
        let mut csv = UTF8_BOM.to_vec();
        csv.extend_from_slice(content.as_bytes());
        csv
    };

    let mut csv_data: Vec<(&str, Vec<u8>)> = Vec::new();
    if wanted("network_regions") {
        csv_data.push((
            "network_regions.csv",
            simple_template("名称,描述\n示例网络区域,示例描述\n"),
        ));
    }
    if wanted("networks") {
        csv_data.push((
            "networks.csv",
            simple_template("名称,网络区域,IPv4 CIDR,IPv6 CIDR,IPv4网关,IPv6网关,IPv4 DNS,IPv6 DNS,描述\n示例网络,示例网络区域,192.168.1.0/24,,192.168.1.1,,8.8.8.8,,示例描述\n"),
        ));
    }
    if wanted("rooms") {
        csv_data.push((
            "rooms.csv",
            simple_template("名称,类型,网络1,网络2\n示例房间,办公室,网络区域/网络名称,\n"),
        ));
    }
    if wanted("workstations") {
        csv_data.push((
            "workstations.csv",
            simple_template(
                "名称,房间,IP地址,负责人,描述\n示例工位,示例房间,192.168.1.100,管理员,示例描述\n",
            ),
        ));
    }
    if wanted("cabinets") {
        csv_data.push((
            "cabinets.csv",
            simple_template(
                "名称,房间,网络1,网络2,描述\n示例机柜,示例房间,网络区域/网络名称,,示例描述\n",
            ),
        ));
    }
    if wanted("positions") {
        csv_data.push((
            "positions.csv",
            simple_template(
                "名称,机柜,起始U,结束U,IP地址,描述\n示例机位,示例机柜,1,2,192.168.1.101,示例描述\n",
            ),
        ));
    }
    if wanted("switches") {
        csv_data.push((
            "switches.csv",
            simple_template("名称,IP地址,型号,厂商,位置,SNMP版本,SNMP端口,SNMP Community,SNMP用户名,描述\n示例交换机,192.168.1.1,H3C S5500,H3C,机房A,v2c,161,public,,示例描述\n"),
        ));
    }

    let buf = zip_files(csv_data)?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=ipma_import_template_{}.zip",
                    chrono::Utc::now().format("%Y%m%d_%H%M%S")
                ),
            ),
        ],
        buf,
    )
        .into_response())
}
