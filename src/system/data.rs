use actix_web::{HttpResponse, web};
use futures_util::TryStreamExt;
use sqlx::Row;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::io::{Cursor, Read, Write};
use zip::{ZipWriter, write::FileOptions};

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::ApiResponse;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub mode: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub success: bool,
    pub message: String,
    pub details: Vec<String>,
}

fn escape_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn empty_to_none(s: &str) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s.trim().to_string())
    }
}

async fn find_network_id(
    conn: &mut sqlx::PgConnection,
    network_identifier: &str,
) -> Result<Option<uuid::Uuid>, sqlx::Error> {
    if network_identifier.contains('/') {
        let parts: Vec<&str> = network_identifier.splitn(2, '/').collect();
        if parts.len() == 2 {
            let region_name = parts[0].trim();
            let network_name = parts[1].trim();
            sqlx::query_scalar(
                "SELECT n.id FROM network_cidrs n 
                 JOIN network_regions r ON n.network_region_id = r.id 
                 WHERE r.name = $1 AND n.name = $2",
            )
            .bind(region_name)
            .bind(network_name)
            .fetch_optional(&mut *conn)
            .await
        } else {
            Ok(None)
        }
    } else {
        sqlx::query_scalar("SELECT id FROM network_cidrs WHERE name = $1")
            .bind(network_identifier.trim())
            .fetch_optional(&mut *conn)
            .await
    }
}

pub async fn export_csv(
    state: web::Data<AppState>,
    type_param: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let mut conn = state.pool()?.acquire().await?;

    let export_type = type_param.get("type").cloned().unwrap_or_else(|| "all".to_string());
    let mut csv_data: Vec<(&str, Vec<u8>)> = Vec::new();
    let utf8_bom = &[0xEF, 0xBB, 0xBF];

    if export_type == "all" || export_type == "network_regions" {
        csv_data.push(export_network_regions(&mut conn, utf8_bom).await?);
    }

    if export_type == "all" || export_type == "networks" {
        csv_data.push(export_networks(&mut conn, utf8_bom).await?);
    }

    if export_type == "all" || export_type == "rooms" {
        csv_data.push(export_rooms(&mut conn, utf8_bom).await?);
    }

    if export_type == "all" || export_type == "workstations" {
        csv_data.push(export_workstations(&mut conn, utf8_bom).await?);
    }

    if export_type == "all" || export_type == "cabinets" {
        csv_data.push(export_cabinets(&mut conn, utf8_bom).await?);
    }

    if export_type == "all" || export_type == "positions" {
        csv_data.push(export_positions(&mut conn, utf8_bom).await?);
    }

    if export_type == "all" || export_type == "switches" {
        csv_data.push(export_switches(&mut conn, utf8_bom).await?);
    }

    if export_type == "all" || export_type == "ips" {
        csv_data.push(export_ip_managers(&mut conn, utf8_bom).await?);
    }

    let mut buf = Cursor::new(Vec::new());
    let options = FileOptions::<'_, ()>::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    {
        let mut zip = ZipWriter::new(&mut buf);
        for (filename, data) in csv_data {
            zip.start_file(filename, options)
                .map_err(|e| AppError::Internal(format!("创建ZIP文件失败: {e}")))?;
            zip.write_all(&data)
                .map_err(|e| AppError::Internal(format!("写入ZIP文件失败: {e}")))?;
        }
        zip.finish()
            .map_err(|e| AppError::Internal(format!("完成ZIP文件失败: {e}")))?;
    }

    Ok(HttpResponse::Ok()
        .content_type("application/zip")
        .append_header((
            actix_web::http::header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=ipma_export_{}_{}.zip",
                export_type,
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            ),
        ))
        .body(buf.into_inner()))
}

async fn export_network_regions(
    conn: &mut sqlx::PgConnection,
    utf8_bom: &[u8],
) -> Result<(&'static str, Vec<u8>), AppError> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);
    csv.extend_from_slice("名称,描述\n".as_bytes());

    let rows = sqlx::query("SELECT name, description FROM network_regions ORDER BY name")
        .fetch_all(&mut *conn)
        .await?;

    for row in rows {
        let name: String = row.get(0);
        let description: Option<String> = row.get(1);
        let line = format!(
            "{},{}\n",
            escape_csv_field(&name),
            escape_csv_field(&description.unwrap_or_default())
        );
        csv.extend_from_slice(line.as_bytes());
    }

    Ok(("network_regions.csv", csv))
}

async fn export_networks(
    conn: &mut sqlx::PgConnection,
    utf8_bom: &[u8],
) -> Result<(&'static str, Vec<u8>), AppError> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);
    csv.extend_from_slice(
        "名称,网络区域,IPv4 CIDR,IPv6 CIDR,IPv4网关,IPv6网关,IPv4 DNS,IPv6 DNS,描述\n".as_bytes(),
    );

    let rows = sqlx::query(
        r"SELECT n.name, nr.name as region_name, 
           n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, 
           n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT,
           COALESCE(array_to_string(n.ipv4_dns, ','), ''), 
           COALESCE(array_to_string(n.ipv6_dns, ','), ''), 
           n.description
           FROM network_cidrs n 
           JOIN network_regions nr ON n.network_region_id = nr.id 
           ORDER BY nr.name, n.name",
    )
    .fetch_all(&mut *conn)
    .await?;

    for row in rows {
        let name: String = row.get(0);
        let region: String = row.get(1);
        let ipv4_cidr: Option<String> = row.get(2);
        let ipv6_cidr: Option<String> = row.get(3);
        let ipv4_gateway: Option<String> = row.get(4);
        let ipv6_gateway: Option<String> = row.get(5);
        let ipv4_dns: Option<String> = row.get(6);
        let ipv6_dns: Option<String> = row.get(7);
        let description: Option<String> = row.get(8);

        let line = format!(
            "{},{},{},{},{},{},{},{},{}\n",
            escape_csv_field(&name),
            escape_csv_field(&region),
            escape_csv_field(&ipv4_cidr.unwrap_or_default()),
            escape_csv_field(&ipv6_cidr.unwrap_or_default()),
            escape_csv_field(&ipv4_gateway.unwrap_or_default()),
            escape_csv_field(&ipv6_gateway.unwrap_or_default()),
            escape_csv_field(&ipv4_dns.unwrap_or_default()),
            escape_csv_field(&ipv6_dns.unwrap_or_default()),
            escape_csv_field(&description.unwrap_or_default())
        );
        csv.extend_from_slice(line.as_bytes());
    }

    Ok(("networks.csv", csv))
}

async fn export_rooms(
    conn: &mut sqlx::PgConnection,
    utf8_bom: &[u8],
) -> Result<(&'static str, Vec<u8>), AppError> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);

    let rooms = sqlx::query("SELECT id, name, room_type FROM rooms ORDER BY name")
        .fetch_all(&mut *conn)
        .await?;

    let mut room_networks_map: HashMap<uuid::Uuid, Vec<String>> = HashMap::new();
    let mut max_networks = 0;

    for room in &rooms {
        let room_id: uuid::Uuid = room.get(0);
        let networks: Vec<String> = sqlx::query_scalar(
            r"SELECT nr.name || '/' || n.name FROM room_networks rn 
               JOIN network_cidrs n ON rn.network_id = n.id 
               JOIN network_regions nr ON n.network_region_id = nr.id
               WHERE rn.room_id = $1 
               ORDER BY rn.created_at",
        )
        .bind(room_id)
        .fetch_all(&mut *conn)
        .await?;

        if networks.len() > max_networks {
            max_networks = networks.len();
        }
        room_networks_map.insert(room_id, networks);
    }

    let mut header = String::from("名称,类型");
    for i in 1..=max_networks {
        write!(header, ",网络{i}").expect("CSV头部写入失败");
    }
    header.push('\n');
    csv.extend_from_slice(header.as_bytes());

    for room in &rooms {
        let room_id: uuid::Uuid = room.get(0);
        let name: String = room.get(1);
        let room_type: String = room.get(2);
        let networks = room_networks_map.get(&room_id).cloned().unwrap_or_default();

        let room_type_display = match room_type.as_str() {
            "OFFICE" => "办公室",
            "DATA_CENTER" => "数据中心",
            _ => &room_type,
        };

        let mut line = format!("{},{}", escape_csv_field(&name), room_type_display);
        for i in 0..max_networks {
            if i < networks.len() {
                write!(line, ",{}", escape_csv_field(&networks[i])).expect("CSV行写入失败");
            } else {
                line.push(',');
            }
        }
        line.push('\n');
        csv.extend_from_slice(line.as_bytes());
    }

    Ok(("rooms.csv", csv))
}

async fn export_workstations(
    conn: &mut sqlx::PgConnection,
    utf8_bom: &[u8],
) -> Result<(&'static str, Vec<u8>), AppError> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);
    csv.extend_from_slice("名称,房间,IP地址,负责人,描述\n".as_bytes());

    let rows = sqlx::query(
        r"SELECT w.id, w.name, r.name as room_name, w.manager, w.description 
           FROM workstations w 
           JOIN rooms r ON w.room_id = r.id 
           ORDER BY r.name, w.name",
    )
    .fetch_all(&mut *conn)
    .await?;

    for row in rows {
        let id: uuid::Uuid = row.get(0);
        let name: String = row.get(1);
        let room: String = row.get(2);
        let manager: Option<String> = row.get(3);
        let description: Option<String> = row.get(4);

        let ip_address: Option<String> = sqlx::query_scalar(
            "SELECT host(ip_address) FROM ips WHERE workstation_id = $1 AND device_type = 'workstation' LIMIT 1"
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .unwrap_or(None);

        let line = format!(
            "{},{},{},{},{}\n",
            escape_csv_field(&name),
            escape_csv_field(&room),
            escape_csv_field(&ip_address.unwrap_or_default()),
            escape_csv_field(&manager.unwrap_or_default()),
            escape_csv_field(&description.unwrap_or_default())
        );
        csv.extend_from_slice(line.as_bytes());
    }

    Ok(("workstations.csv", csv))
}

async fn export_cabinets(
    conn: &mut sqlx::PgConnection,
    utf8_bom: &[u8],
) -> Result<(&'static str, Vec<u8>), AppError> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);

    let cabinets = sqlx::query(
        r"SELECT c.id, c.name, r.name as room_name, c.description 
           FROM cabinets c 
           JOIN rooms r ON c.room_id = r.id 
           ORDER BY r.name, c.name",
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut cabinet_networks_map: HashMap<uuid::Uuid, Vec<String>> = HashMap::new();
    let mut max_networks = 0;

    for cabinet in &cabinets {
        let cabinet_id: uuid::Uuid = cabinet.get(0);
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
        .await?;

        if networks.len() > max_networks {
            max_networks = networks.len();
        }
        cabinet_networks_map.insert(cabinet_id, networks);
    }

    let mut header = String::from("名称,房间");
    for i in 1..=max_networks {
        write!(header, ",网络{i}").expect("CSV头部写入失败");
    }
    header.push_str(",描述\n");
    csv.extend_from_slice(header.as_bytes());

    for cabinet in &cabinets {
        let cabinet_id: uuid::Uuid = cabinet.get(0);
        let name: String = cabinet.get(1);
        let room: String = cabinet.get(2);
        let description: Option<String> = cabinet.get(3);
        let networks = cabinet_networks_map
            .get(&cabinet_id)
            .cloned()
            .unwrap_or_default();

        let mut line = format!("{},{}", escape_csv_field(&name), escape_csv_field(&room));
        for i in 0..max_networks {
            if i < networks.len() {
                write!(line, ",{}", escape_csv_field(&networks[i])).expect("CSV行写入失败");
            } else {
                line.push(',');
            }
        }
        writeln!(line, ",{}", escape_csv_field(&description.unwrap_or_default())).expect("CSV行写入失败");
        csv.extend_from_slice(line.as_bytes());
    }

    Ok(("cabinets.csv", csv))
}

async fn export_positions(
    conn: &mut sqlx::PgConnection,
    utf8_bom: &[u8],
) -> Result<(&'static str, Vec<u8>), AppError> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);
    csv.extend_from_slice("名称,机柜,起始U,结束U,IP地址,描述\n".as_bytes());

    let rows = sqlx::query(
        r"SELECT p.id, p.name, c.name as cabinet_name, p.start_u, p.end_u, p.description 
           FROM positions p 
           JOIN cabinets c ON p.cabinet_id = c.id 
           ORDER BY c.name, p.start_u",
    )
    .fetch_all(&mut *conn)
    .await?;

    for row in rows {
        let id: uuid::Uuid = row.get(0);
        let name: String = row.get(1);
        let cabinet: String = row.get(2);
        let start_u: i32 = row.get(3);
        let end_u: i32 = row.get(4);
        let description: Option<String> = row.get(5);

        let ip_address: Option<String> = sqlx::query_scalar(
            "SELECT host(ip_address) FROM ips WHERE position_id = $1 AND device_type = 'cabinet_position' LIMIT 1"
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .unwrap_or(None);

        let line = format!(
            "{},{},{},{},{},{}\n",
            escape_csv_field(&name),
            escape_csv_field(&cabinet),
            start_u,
            end_u,
            escape_csv_field(&ip_address.unwrap_or_default()),
            escape_csv_field(&description.unwrap_or_default())
        );
        csv.extend_from_slice(line.as_bytes());
    }

    Ok(("positions.csv", csv))
}

async fn export_switches(
    conn: &mut sqlx::PgConnection,
    utf8_bom: &[u8],
) -> Result<(&'static str, Vec<u8>), AppError> {
    use crate::crypto::decrypt_password;

    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);
    csv.extend_from_slice(
        "名称,IP地址,型号,厂商,位置,SNMP版本,SNMP端口,SNMP Community,SNMP用户名,描述\n".as_bytes(),
    );

    let rows = sqlx::query(
        r"SELECT s.id, s.name, s.model, s.vendor, s.location, s.snmp_version, s.snmp_port,
           s.snmp_community, s.snmp_username, s.description
           FROM switches s 
           ORDER BY s.name",
    )
    .fetch_all(&mut *conn)
    .await?;

    for row in rows {
        let id: uuid::Uuid = row.get(0);
        let name: String = row.get(1);
        let model: Option<String> = row.get(2);
        let vendor: Option<String> = row.get(3);
        let location: Option<String> = row.get(4);
        let snmp_version: Option<String> = row.get(5);
        let snmp_port: Option<i32> = row.get(6);
        let snmp_community: Option<String> = row.get(7);
        let snmp_username: Option<String> = row.get(8);
        let description: Option<String> = row.get(9);

        let ip_address: Option<String> = sqlx::query_scalar(
            "SELECT host(ip_address) FROM ips WHERE position_id = (SELECT id FROM positions WHERE device_type = 'switch' AND device_id = $1) LIMIT 1"
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .unwrap_or(None);

        let decrypted_community = snmp_community
            .as_ref()
            .filter(|c| !c.is_empty())
            .map(|c| decrypt_password(c));

        let line = format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            escape_csv_field(&name),
            escape_csv_field(&ip_address.unwrap_or_default()),
            escape_csv_field(&model.unwrap_or_default()),
            escape_csv_field(&vendor.unwrap_or_default()),
            escape_csv_field(&location.unwrap_or_default()),
            escape_csv_field(&snmp_version.unwrap_or_else(|| "v2c".to_string())),
            snmp_port.unwrap_or(161),
            escape_csv_field(&decrypted_community.unwrap_or_default()),
            escape_csv_field(&snmp_username.unwrap_or_default()),
            escape_csv_field(&description.unwrap_or_default())
        );
        csv.extend_from_slice(line.as_bytes());
    }

    Ok(("switches.csv", csv))
}

async fn export_ip_managers(
    conn: &mut sqlx::PgConnection,
    utf8_bom: &[u8],
) -> Result<(&'static str, Vec<u8>), AppError> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);
    csv.extend_from_slice("工位,机位,网络,IP地址,MAC地址,主机名,状态\n".as_bytes());

    let rows = sqlx::query(
        r"SELECT w.name as workstation_name, p.name as position_name, 
           n.name as network_name, host(im.ip_address), 
           im.mac_address, im.hostname, im.status
           FROM ips im 
           LEFT JOIN workstations w ON im.workstation_id = w.id 
           LEFT JOIN positions p ON im.position_id = p.id 
           LEFT JOIN network_cidrs n ON im.network_id = n.id 
           ORDER BY im.ip_address",
    )
    .fetch_all(&mut *conn)
    .await?;

    for row in rows {
        let workstation: Option<String> = row.get(0);
        let position: Option<String> = row.get(1);
        let network: Option<String> = row.get(2);
        let ip_address: String = row.get(3);
        let mac_address: Option<String> = row.get(4);
        let hostname: Option<String> = row.get(5);
        let status: String = row.get(6);

        let line = format!(
            "{},{},{},{},{},{},{}\n",
            escape_csv_field(&workstation.unwrap_or_default()),
            escape_csv_field(&position.unwrap_or_default()),
            escape_csv_field(&network.unwrap_or_default()),
            escape_csv_field(&ip_address),
            escape_csv_field(&mac_address.unwrap_or_default()),
            escape_csv_field(&hostname.unwrap_or_default()),
            escape_csv_field(&status)
        );
        csv.extend_from_slice(line.as_bytes());
    }

    Ok(("ip_managers.csv", csv))
}

pub async fn import_csv(
    state: web::Data<AppState>,
    mut payload: actix_multipart::Multipart,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let mode = query
        .get("mode")
        .cloned()
        .unwrap_or_else(|| "skip".to_string());
    let overwrite = mode == "overwrite";

    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;

    while let Some(mut field) = payload
        .try_next()
        .await
        .map_err(|e| AppError::Internal(format!("读取文件失败: {e}")))?
    {
        if field.name() == Some("file") {
            filename = field
                .content_disposition()
                .and_then(|cd| cd.get_filename().map(std::string::ToString::to_string));
            let mut data = Vec::new();
            while let Some(chunk) = field.try_next().await.map_err(|e| {
                AppError::Internal(format!("读取文件块失败: {e}"))
            })? {
                data.extend_from_slice(&chunk);
            }
            file_data = Some(data);
            break;
        }
    }

    let file_data =
        file_data.ok_or_else(|| AppError::Validation("请选择要导入的CSV文件".to_string()))?;

    let mut conn = state.pool()?.acquire().await?;

    let mut results = Vec::new();

    if let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(file_data.clone())) {
        for i in 0..zip.len() {
            let mut file = zip.by_index(i)
                .map_err(|e| AppError::Internal(format!("读取ZIP文件项失败: {e}")))?;

            let zip_filename = file.name().to_string();
            if zip_filename.ends_with(".csv") {
                let mut content = String::new();
                file.read_to_string(&mut content)
                    .map_err(|e| AppError::Internal(format!("读取CSV文件失败: {e}")))?;

                let table_name = zip_filename.trim_end_matches(".csv");
                if let Err(e) = process_csv_by_filename(
                    &mut conn,
                    table_name,
                    &content,
                    overwrite,
                    &mut results,
                )
                .await
                {
                    results.push(format!("导入 {zip_filename} 失败: {e}"));
                }
            }
        }
    } else {
        let content = String::from_utf8(file_data).map_err(|e| {
            AppError::Validation(format!(
                "解析CSV文件失败: 文件编码必须是UTF-8 - {e}"
            ))
        })?;

        let table_name = filename
            .as_ref()
            .and_then(|f| f.trim_end_matches(".csv").split('.').next())
            .unwrap_or("unknown");

        if let Err(e) =
            process_csv_by_filename(&mut conn, table_name, &content, overwrite, &mut results).await
        {
            results.push(format!("导入失败: {e}"));
        }
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({ "results": results }),
        "导入完成",
    )))
}

async fn process_csv_by_filename(
    conn: &mut sqlx::PgConnection,
    filename: &str,
    content: &str,
    overwrite: bool,
    results: &mut Vec<String>,
) -> Result<(), String> {
    match filename {
        "network_regions" => import_network_regions(conn, content, overwrite, results).await,
        "networks" => import_networks(conn, content, overwrite, results).await,
        "rooms" => import_rooms(conn, content, overwrite, results).await,
        "workstations" => import_workstations(conn, content, overwrite, results).await,
        "cabinets" => import_cabinets(conn, content, overwrite, results).await,
        "positions" => import_positions(conn, content, overwrite, results).await,
        "switches" => import_switches(conn, content, overwrite, results).await,
        _ => {
            results.push(format!("跳过未知文件: {filename}.csv (支持的文件: network_regions, networks, rooms, workstations, cabinets, positions, switches)"));
            Ok(())
        }
    }
}

async fn import_network_regions(
    conn: &mut sqlx::PgConnection,
    content: &str,
    overwrite: bool,
    results: &mut Vec<String>,
) -> Result<(), String> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut error_count = 0;
    let mut line_num = 1;

    for result in rdr.records() {
        line_num += 1;
        let record = result.map_err(|e| format!("第{line_num}行解析失败: {e}"))?;

        if record.get(0).is_some_and(|s| s == "名称") {
            continue;
        }

        let name = record.get(0).unwrap_or("").trim();
        let description = record.get(1).unwrap_or("").trim();

        if name.is_empty() {
            results.push(format!("第{line_num}行跳过: 名称为空"));
            error_count += 1;
            continue;
        }

        if name.len() > 20 {
            results.push(format!(
                "第{line_num}行跳过: 名称 '{name}' 超过20个字符"
            ));
            error_count += 1;
            continue;
        }

        if description.len() > 255 {
            results.push(format!(
                "第{line_num}行跳过: 网络区域 '{name}' - 描述超过255个字符"
            ));
            error_count += 1;
            continue;
        }

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM network_regions WHERE name = $1")
                .bind(name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| format!("查询网络区域失败: {e}"))?;

        if let Some(id) = existing {
            if overwrite {
                sqlx::query(
                    "UPDATE network_regions SET description = $1, updated_at = NOW() WHERE id = $2",
                )
                .bind(empty_to_none(description))
                .bind(id)
                .execute(&mut *conn)
                .await
                .map_err(|e| format!("更新网络区域 '{name}' 失败: {e}"))?;
                results.push(format!("更新网络区域: {name}"));
                success_count += 1;
            } else {
                results.push(format!("跳过网络区域（已存在）: {name}"));
                skip_count += 1;
            }
        } else {
            sqlx::query("INSERT INTO network_regions (id, name, description, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW())")
                .bind(uuid::Uuid::new_v4())
                .bind(name)
                .bind(empty_to_none(description))
                .execute(&mut *conn)
                .await
                .map_err(|e| format!("插入网络区域 '{name}' 失败: {e}"))?;
            results.push(format!("导入网络区域: {name}"));
            success_count += 1;
        }
    }

    results.push(format!(
        "网络区域导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}

async fn import_networks(
    conn: &mut sqlx::PgConnection,
    content: &str,
    overwrite: bool,
    results: &mut Vec<String>,
) -> Result<(), String> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut error_count = 0;
    let mut line_num = 1;

    for result in rdr.records() {
        line_num += 1;
        let record = result.map_err(|e| format!("第{line_num}行解析失败: {e}"))?;

        if record.get(0).is_some_and(|s| s == "名称") {
            continue;
        }

        let name = record.get(0).unwrap_or("").trim();
        let region_name = record.get(1).unwrap_or("").trim();
        let ipv4_cidr = record.get(2).unwrap_or("").trim();
        let ipv6_cidr = record.get(3).unwrap_or("").trim();
        let ipv4_gateway = record.get(4).unwrap_or("").trim();
        let ipv6_gateway = record.get(5).unwrap_or("").trim();
        let ipv4_dns = record.get(6).unwrap_or("").trim();
        let ipv6_dns = record.get(7).unwrap_or("").trim();
        let description = record.get(8).unwrap_or("").trim();

        if name.is_empty() {
            results.push(format!("第{line_num}行跳过: 名称为空"));
            error_count += 1;
            continue;
        }

        if name.len() > 50 {
            results.push(format!(
                "第{line_num}行跳过: 名称 '{name}' 超过50个字符"
            ));
            error_count += 1;
            continue;
        }

        if description.len() > 255 {
            results.push(format!(
                "第{line_num}行跳过: 网络 '{name}' - 描述超过255个字符"
            ));
            error_count += 1;
            continue;
        }

        if region_name.is_empty() {
            results.push(format!(
                "第{line_num}行跳过: 网络 '{name}' - 网络区域为空"
            ));
            error_count += 1;
            continue;
        }

        let region_id: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM network_regions WHERE name = $1")
                .bind(region_name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| format!("查询网络区域失败: {e}"))?;

        let Some(region_id) = region_id else {
            results.push(format!(
                "第{line_num}行跳过: 网络 '{name}' - 网络区域 '{region_name}' 不存在"
            ));
            error_count += 1;
            continue
        };

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM network_cidrs WHERE name = $1")
                .bind(name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| format!("查询网络失败: {e}"))?;

        if let Some(id) = existing {
            if overwrite {
                let update_result = sqlx::query(r"UPDATE network_cidrs SET 
                    network_region_id = $1, 
                    ipv4_cidr = CASE WHEN $2 = '' THEN NULL ELSE CAST($2 AS CIDR) END,
                    ipv6_cidr = CASE WHEN $3 = '' THEN NULL ELSE CAST($3 AS CIDR) END,
                    ipv4_gateway = CASE WHEN $4 = '' THEN NULL ELSE CAST($4 AS INET) END,
                    ipv6_gateway = CASE WHEN $5 = '' THEN NULL ELSE CAST($5 AS INET) END,
                    ipv4_dns = CASE WHEN $6 = '' THEN NULL ELSE CAST(string_to_array($6, ',') AS INET[]) END,
                    ipv6_dns = CASE WHEN $7 = '' THEN NULL ELSE CAST(string_to_array($7, ',') AS INET[]) END,
                    description = $8, updated_at = NOW() WHERE id = $9")
                    .bind(region_id)
                    .bind(empty_to_none(ipv4_cidr))
                    .bind(empty_to_none(ipv6_cidr))
                    .bind(empty_to_none(ipv4_gateway))
                    .bind(empty_to_none(ipv6_gateway))
                    .bind(empty_to_none(ipv4_dns))
                    .bind(empty_to_none(ipv6_dns))
                    .bind(empty_to_none(description))
                    .bind(id)
                    .execute(&mut *conn)
                    .await;

                match update_result {
                    Ok(_) => {
                        results.push(format!("更新网络: {name}"));
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(format!(
                            "第{line_num}行跳过: 更新网络 '{name}' 失败 - {e}"
                        ));
                        error_count += 1;
                    }
                }
            } else {
                results.push(format!("跳过网络（已存在）: {name}"));
                skip_count += 1;
            }
        } else {
            let insert_result = sqlx::query(r"INSERT INTO network_cidrs (id, name, network_region_id, ipv4_cidr, ipv6_cidr, ipv4_gateway, ipv6_gateway, ipv4_dns, ipv6_dns, description, created_at, updated_at) 
                VALUES ($1, $2, $3, 
                CASE WHEN $4 = '' THEN NULL ELSE CAST($4 AS CIDR) END,
                CASE WHEN $5 = '' THEN NULL ELSE CAST($5 AS CIDR) END,
                CASE WHEN $6 = '' THEN NULL ELSE CAST($6 AS INET) END,
                CASE WHEN $7 = '' THEN NULL ELSE CAST($7 AS INET) END,
                CASE WHEN $8 = '' THEN NULL ELSE CAST(string_to_array($8, ',') AS INET[]) END,
                CASE WHEN $9 = '' THEN NULL ELSE CAST(string_to_array($9, ',') AS INET[]) END,
                $10, NOW(), NOW())")
                .bind(uuid::Uuid::new_v4())
                .bind(name)
                .bind(region_id)
                .bind(empty_to_none(ipv4_cidr))
                .bind(empty_to_none(ipv6_cidr))
                .bind(empty_to_none(ipv4_gateway))
                .bind(empty_to_none(ipv6_gateway))
                .bind(empty_to_none(ipv4_dns))
                .bind(empty_to_none(ipv6_dns))
                .bind(empty_to_none(description))
                .execute(&mut *conn)
                .await;

            match insert_result {
                Ok(_) => {
                    results.push(format!("导入网络: {name}"));
                    success_count += 1;
                }
                Err(e) => {
                    results.push(format!(
                        "第{line_num}行跳过: 插入网络 '{name}' 失败 - {e}"
                    ));
                    error_count += 1;
                }
            }
        }
    }

    results.push(format!(
        "网络导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}

async fn import_rooms(
    conn: &mut sqlx::PgConnection,
    content: &str,
    overwrite: bool,
    results: &mut Vec<String>,
) -> Result<(), String> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut error_count = 0;
    let mut line_num = 1;

    for result in rdr.records() {
        line_num += 1;
        let record = result.map_err(|e| format!("第{line_num}行解析失败: {e}"))?;

        if record.get(0).is_some_and(|s| s == "名称") {
            continue;
        }

        let name = record.get(0).unwrap_or("").trim();
        let room_type_str = record.get(1).unwrap_or("").trim();

        if name.is_empty() {
            results.push(format!("第{line_num}行跳过: 名称为空"));
            error_count += 1;
            continue;
        }

        if name.len() > 50 {
            results.push(format!(
                "第{line_num}行跳过: 名称 '{name}' 超过50个字符"
            ));
            error_count += 1;
            continue;
        }

        let room_type = match room_type_str {
            "办公室" | "OFFICE" | "office" | "" => "OFFICE",
            "数据中心" | "DATA_CENTER" | "data_center" => "DATA_CENTER",
            _ => {
                results.push(format!(
                    "第{line_num}行跳过: 房间 '{name}' - 无效的类型 '{room_type_str}'"
                ));
                error_count += 1;
                continue;
            }
        };

        let network_names: Vec<&str> = record
            .iter()
            .skip(2)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM rooms WHERE name = $1")
                .bind(name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| format!("查询房间失败: {e}"))?;

        if let Some(id) = existing {
            if overwrite {
                let update_result = sqlx::query(
                    "UPDATE rooms SET room_type = $1, updated_at = NOW() WHERE id = $2",
                )
                .bind(room_type)
                .bind(id)
                .execute(&mut *conn)
                .await;

                match update_result {
                    Ok(_) => {
                        let delete_result = sqlx::query("DELETE FROM room_networks WHERE room_id = $1")
                            .bind(id)
                            .execute(&mut *conn)
                            .await;
                        if let Err(e) = delete_result {
                            tracing::warn!("操作失败: {}", e);
                        }

                        let mut linked_networks = Vec::new();
                        for network_name in &network_names {
                            match find_network_id(&mut *conn, network_name).await {
                                Ok(Some(network_id)) => {
                                    let insert_result = sqlx::query("INSERT INTO room_networks (id, room_id, network_id, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW())")
                                        .bind(uuid::Uuid::new_v4())
                                        .bind(id)
                                        .bind(network_id)
                                        .execute(&mut *conn)
                                        .await;
                                    if let Err(e) = insert_result {
                                        tracing::warn!("操作失败: {}", e);
                                    }
                                    linked_networks.push(*network_name);
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    tracing::warn!("查找网络失败: {}", e);
                                }
                            }
                        }
                        results.push(format!(
                            "更新房间: {} (类型: {}, 网络: {})",
                            name,
                            room_type_str,
                            linked_networks.join(", ")
                        ));
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(format!(
                            "第{line_num}行跳过: 更新房间 '{name}' 失败 - {e}"
                        ));
                        error_count += 1;
                    }
                }
            } else {
                results.push(format!("跳过房间（已存在）: {name}"));
                skip_count += 1;
            }
        } else {
            let id = uuid::Uuid::new_v4();
            let insert_result = sqlx::query("INSERT INTO rooms (id, name, room_type, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW())")
                .bind(id)
                .bind(name)
                .bind(room_type)
                .execute(&mut *conn)
                .await;

            match insert_result {
                Ok(_) => {
                    let mut linked_networks = Vec::new();
                    let mut missing_networks = Vec::new();
                    for network_name in &network_names {
                        match find_network_id(&mut *conn, network_name).await {
                            Ok(Some(network_id)) => {
                                if let Err(e) = sqlx::query("INSERT INTO room_networks (id, room_id, network_id, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW())")
                                    .bind(uuid::Uuid::new_v4())
                                    .bind(id)
                                    .bind(network_id)
                                    .execute(&mut *conn)
                                    .await
                                {
                                    tracing::warn!("关联房间网络失败: {}", e);
                                }
                                linked_networks.push(*network_name);
                            }
                            Ok(None) => {
                                missing_networks.push(*network_name);
                            }
                            Err(e) => {
                                tracing::warn!("查找网络失败: {}", e);
                                missing_networks.push(*network_name);
                            }
                        }
                    }

                    if !missing_networks.is_empty() {
                        results.push(format!(
                            "导入房间: {} (类型: {}, 网络: {}, 未找到网络: {})",
                            name,
                            room_type_str,
                            linked_networks.join(", "),
                            missing_networks.join(", ")
                        ));
                    } else if linked_networks.is_empty() {
                        results.push(format!(
                            "导入房间: {name} (类型: {room_type_str}, 无网络关联)"
                        ));
                    } else {
                        results.push(format!(
                            "导入房间: {} (类型: {}, 网络: {})",
                            name,
                            room_type_str,
                            linked_networks.join(", ")
                        ));
                    }
                    success_count += 1;
                }
                Err(e) => {
                    results.push(format!(
                        "第{line_num}行跳过: 插入房间 '{name}' 失败 - {e}"
                    ));
                    error_count += 1;
                }
            }
        }
    }

    results.push(format!(
        "房间导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}

async fn import_workstations(
    conn: &mut sqlx::PgConnection,
    content: &str,
    overwrite: bool,
    results: &mut Vec<String>,
) -> Result<(), String> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut error_count = 0;
    let mut line_num = 1;

    for result in rdr.records() {
        line_num += 1;
        let record = result.map_err(|e| format!("第{line_num}行解析失败: {e}"))?;

        if record.get(0).is_some_and(|s| s == "名称") {
            continue;
        }

        let name = record.get(0).unwrap_or("").trim();
        let room_name = record.get(1).unwrap_or("").trim();
        let ip_address = record.get(2).unwrap_or("").trim();
        let manager = record.get(3).unwrap_or("").trim();
        let description = record.get(4).unwrap_or("").trim();

        if name.is_empty() {
            results.push(format!("第{line_num}行跳过: 名称为空"));
            error_count += 1;
            continue;
        }

        if name.len() > 50 {
            results.push(format!(
                "第{line_num}行跳过: 名称 '{name}' 超过50个字符"
            ));
            error_count += 1;
            continue;
        }

        if manager.len() > 50 {
            results.push(format!(
                "第{line_num}行跳过: 工位 '{name}' - 负责人超过50个字符"
            ));
            error_count += 1;
            continue;
        }

        if description.len() > 255 {
            results.push(format!(
                "第{line_num}行跳过: 工位 '{name}' - 描述超过255个字符"
            ));
            error_count += 1;
            continue;
        }

        if room_name.is_empty() {
            results.push(format!(
                "第{line_num}行跳过: 工位 '{name}' - 房间名称为空"
            ));
            error_count += 1;
            continue;
        }

        let room_id: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM rooms WHERE name = $1")
                .bind(room_name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| format!("查询房间失败: {e}"))?;

        let Some(room_id) = room_id else {
            results.push(format!(
                "第{line_num}行跳过: 工位 '{name}' - 房间 '{room_name}' 不存在"
            ));
            error_count += 1;
            continue
        };

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM workstations WHERE name = $1 AND room_id = $2")
                .bind(name)
                .bind(room_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| format!("查询工位失败: {e}"))?;

        if let Some(id) = existing {
            if overwrite {
                let update_result = sqlx::query("UPDATE workstations SET manager = $1, description = $2, updated_at = NOW() WHERE id = $3")
                    .bind(empty_to_none(manager))
                    .bind(empty_to_none(description))
                    .bind(id)
                    .execute(&mut *conn)
                    .await;

                match update_result {
                    Ok(_) => {
                        if ip_address.is_empty() {
                            results.push(format!("更新工位: {name} (房间: {room_name})"));
                        } else {
                            let existing_ip: Option<uuid::Uuid> = match sqlx::query_scalar(
                                "SELECT id FROM ips WHERE workstation_id = $1 AND device_type = 'workstation' LIMIT 1"
                            )
                            .bind(id)
                            .fetch_optional(&mut *conn)
                            .await {
                                Ok(v) => v,
                                Err(e) => { tracing::warn!("查询现有IP失败: {}", e); None }
                            };

                            let room_network_id: Option<uuid::Uuid> = match sqlx::query_scalar(
                                "SELECT network_id FROM room_networks WHERE room_id = $1 LIMIT 1",
                            )
                            .bind(room_id)
                            .fetch_optional(&mut *conn)
                            .await {
                                Ok(v) => v,
                                Err(e) => { tracing::warn!("查询房间网络失败: {}", e); None }
                            };

                            let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };

                            if let Some(ip_id) = existing_ip {
                                let ip_update_result = sqlx::query(
                                    "UPDATE ips SET ip_address = CAST($1 AS INET), ip_version = $2, network_id = $3, updated_at = NOW() WHERE id = $4"
                                )
                                .bind(ip_address)
                                .bind(ip_version)
                                .bind(room_network_id)
                                .bind(ip_id)
                                .execute(&mut *conn)
                                .await;

                                match ip_update_result {
                                    Ok(_) => results.push(format!(
                                        "更新工位: {name} (房间: {room_name}, IP: {ip_address})"
                                    )),
                                    Err(e) => results.push(format!(
                                        "更新工位: {name} (房间: {room_name}, IP更新失败: {e})"
                                    )),
                                }
                            } else {
                                let ip_manager_id = uuid::Uuid::new_v4();
                                let ip_insert_result = sqlx::query(
                                    "INSERT INTO ips (id, workstation_id, device_type, network_id, ip_address, ip_version, status, created_at, updated_at) VALUES ($1, $2, 'workstation', $3, CAST($4 AS INET), $5, 'active', NOW(), NOW())"
                                )
                                .bind(ip_manager_id)
                                .bind(id)
                                .bind(room_network_id)
                                .bind(ip_address)
                                .bind(ip_version)
                                .execute(&mut *conn)
                                .await;

                                match ip_insert_result {
                                    Ok(_) => results.push(format!(
                                        "更新工位: {name} (房间: {room_name}, IP: {ip_address})"
                                    )),
                                    Err(e) => results.push(format!(
                                        "更新工位: {name} (房间: {room_name}, IP写入失败: {e})"
                                    )),
                                }
                            }
                        }
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(format!(
                            "第{line_num}行跳过: 更新工位 '{name}' 失败 - {e}"
                        ));
                        error_count += 1;
                    }
                }
            } else {
                results.push(format!(
                    "跳过工位（已存在）: {name} (房间: {room_name})"
                ));
                skip_count += 1;
            }
        } else {
            let new_id = uuid::Uuid::new_v4();
            let insert_result = sqlx::query("INSERT INTO workstations (id, name, room_id, manager, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, NOW(), NOW())")
                .bind(new_id)
                .bind(name)
                .bind(room_id)
                .bind(empty_to_none(manager))
                .bind(empty_to_none(description))
                .execute(&mut *conn)
                .await;

            match insert_result {
                Ok(_) => {
                    if ip_address.is_empty() {
                        results.push(format!("导入工位: {name} (房间: {room_name})"));
                    } else {
                        let room_network_id: Option<uuid::Uuid> = match sqlx::query_scalar(
                            "SELECT network_id FROM room_networks WHERE room_id = $1 LIMIT 1",
                        )
                        .bind(room_id)
                        .fetch_optional(&mut *conn)
                        .await {
                            Ok(v) => v,
                            Err(e) => { tracing::warn!("查询房间网络失败: {}", e); None }
                        };

                        let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };
                        let ip_manager_id = uuid::Uuid::new_v4();
                        let ip_insert_result = sqlx::query(
                            "INSERT INTO ips (id, workstation_id, device_type, network_id, ip_address, ip_version, status, created_at, updated_at) VALUES ($1, $2, 'workstation', $3, CAST($4 AS INET), $5, 'active', NOW(), NOW())"
                        )
                        .bind(ip_manager_id)
                        .bind(new_id)
                        .bind(room_network_id)
                        .bind(ip_address)
                        .bind(ip_version)
                        .execute(&mut *conn)
                        .await;

                        match ip_insert_result {
                            Ok(_) => results.push(format!(
                                "导入工位: {name} (房间: {room_name}, IP: {ip_address})"
                            )),
                            Err(e) => results.push(format!(
                                "导入工位: {name} (房间: {room_name}, IP写入失败: {e})"
                            )),
                        }
                    }
                    success_count += 1;
                }
                Err(e) => {
                    results.push(format!(
                        "第{line_num}行跳过: 插入工位 '{name}' 失败 - {e}"
                    ));
                    error_count += 1;
                }
            }
        }
    }

    results.push(format!(
        "工位导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}

async fn import_cabinets(
    conn: &mut sqlx::PgConnection,
    content: &str,
    overwrite: bool,
    results: &mut Vec<String>,
) -> Result<(), String> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut error_count = 0;
    let mut line_num = 1;

    for result in rdr.records() {
        line_num += 1;
        let record = result.map_err(|e| format!("第{line_num}行解析失败: {e}"))?;

        if record.get(0).is_some_and(|s| s == "名称") {
            continue;
        }

        let name = record.get(0).unwrap_or("").trim();
        let room_name = record.get(1).unwrap_or("").trim();
        let description = record
            .get(record.len().saturating_sub(1))
            .unwrap_or("")
            .trim();

        let network_names: Vec<&str> = record
            .iter()
            .skip(2)
            .take(record.len().saturating_sub(3))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        if name.is_empty() {
            results.push(format!("第{line_num}行跳过: 名称为空"));
            error_count += 1;
            continue;
        }

        if name.len() > 50 {
            results.push(format!(
                "第{line_num}行跳过: 名称 '{name}' 超过50个字符"
            ));
            error_count += 1;
            continue;
        }

        if description.len() > 255 {
            results.push(format!(
                "第{line_num}行跳过: 机柜 '{name}' - 描述超过255个字符"
            ));
            error_count += 1;
            continue;
        }

        if room_name.is_empty() {
            results.push(format!(
                "第{line_num}行跳过: 机柜 '{name}' - 房间名称为空"
            ));
            error_count += 1;
            continue;
        }

        let room_id: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM rooms WHERE name = $1")
                .bind(room_name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| format!("查询房间失败: {e}"))?;

        let Some(room_id) = room_id else {
            results.push(format!(
                "第{line_num}行跳过: 机柜 '{name}' - 房间 '{room_name}' 不存在"
            ));
            error_count += 1;
            continue
        };

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM cabinets WHERE name = $1 AND room_id = $2")
                .bind(name)
                .bind(room_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| format!("查询机柜失败: {e}"))?;

        if let Some(id) = existing {
            if overwrite {
                let update_result = sqlx::query("UPDATE cabinets SET description = $1, updated_at = NOW() WHERE id = $2")
                    .bind(empty_to_none(description))
                    .bind(id)
                    .execute(&mut *conn)
                    .await;

                match update_result {
                    Ok(_) => {
                        results.push(format!("更新机柜: {name} (房间: {room_name})"));
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(format!(
                            "第{line_num}行跳过: 更新机柜 '{name}' 失败 - {e}"
                        ));
                        error_count += 1;
                    }
                }
            } else {
                results.push(format!(
                    "跳过机柜（已存在）: {name} (房间: {room_name})"
                ));
                skip_count += 1;
            }
        } else {
            let id = uuid::Uuid::new_v4();
            let insert_result = sqlx::query("INSERT INTO cabinets (id, name, room_id, capacity, description, created_at, updated_at) VALUES ($1, $2, $3, 42, $4, NOW(), NOW())")
                .bind(id)
                .bind(name)
                .bind(room_id)
                .bind(empty_to_none(description))
                .execute(&mut *conn)
                .await;

            match insert_result {
                Ok(_) => {
                    if network_names.is_empty() {
                        results.push(format!(
                            "导入机柜: {name} (房间: {room_name}, 无网络关联)"
                        ));
                    } else {
                        results.push(format!(
                            "导入机柜: {} (房间: {}, 首网络: {})",
                            name, room_name, network_names[0]
                        ));
                    }
                    success_count += 1;
                }
                Err(e) => {
                    results.push(format!(
                        "第{line_num}行跳过: 插入机柜 '{name}' 失败 - {e}"
                    ));
                    error_count += 1;
                }
            }
        }
    }

    results.push(format!(
        "机柜导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}

async fn import_positions(
    conn: &mut sqlx::PgConnection,
    content: &str,
    overwrite: bool,
    results: &mut Vec<String>,
) -> Result<(), String> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut error_count = 0;
    let mut line_num = 1;

    for result in rdr.records() {
        line_num += 1;
        let record = result.map_err(|e| format!("第{line_num}行解析失败: {e}"))?;

        if record.get(0).is_some_and(|s| s == "名称") {
            continue;
        }

        let name = record.get(0).unwrap_or("").trim();
        let cabinet_name = record.get(1).unwrap_or("").trim();
        let start_u_str = record.get(2).unwrap_or("").trim();
        let end_u_str = record.get(3).unwrap_or("").trim();
        let ip_address = record.get(4).unwrap_or("").trim();
        let description = record.get(5).unwrap_or("").trim();

        if name.is_empty() {
            results.push(format!("第{line_num}行跳过: 名称为空"));
            error_count += 1;
            continue;
        }

        if name.len() > 50 {
            results.push(format!(
                "第{line_num}行跳过: 名称 '{name}' 超过50个字符"
            ));
            error_count += 1;
            continue;
        }

        if description.len() > 255 {
            results.push(format!(
                "第{line_num}行跳过: 机位 '{name}' - 描述超过255个字符"
            ));
            error_count += 1;
            continue;
        }

        if cabinet_name.is_empty() {
            results.push(format!(
                "第{line_num}行跳过: 机位 '{name}' - 机柜名称为空"
            ));
            error_count += 1;
            continue;
        }

        if start_u_str.is_empty() {
            results.push(format!("第{line_num}行跳过: 机位 '{name}' - 起始U为空"));
            error_count += 1;
            continue;
        }

        let start_u: i32 = match start_u_str.parse() {
            Ok(v) if (1..=42).contains(&v) => v,
            Ok(v) => {
                results.push(format!(
                    "第{line_num}行跳过: 机位 '{name}' - 起始U '{v}' 超出范围(1-42)"
                ));
                error_count += 1;
                continue;
            }
            Err(_) => {
                results.push(format!(
                    "第{line_num}行跳过: 机位 '{name}' - 起始U '{start_u_str}' 不是有效数字"
                ));
                error_count += 1;
                continue;
            }
        };

        let end_u: i32 = match end_u_str.parse() {
            Ok(v) if v >= start_u && v <= 42 => v,
            Ok(v) => {
                results.push(format!(
                    "第{line_num}行跳过: 机位 '{name}' - 结束U '{v}' 无效(必须 >= 起始U 且 <= 42)"
                ));
                error_count += 1;
                continue;
            }
            Err(_) => start_u,
        };

        let cabinet_id: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM cabinets WHERE name = $1")
                .bind(cabinet_name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| format!("查询机柜失败: {e}"))?;

        let Some(cabinet_id) = cabinet_id else {
            results.push(format!(
                "第{line_num}行跳过: 机位 '{name}' - 机柜 '{cabinet_name}' 不存在"
            ));
            error_count += 1;
            continue
        };

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM positions WHERE name = $1 AND cabinet_id = $2")
                .bind(name)
                .bind(cabinet_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| format!("查询机位失败: {e}"))?;

        if let Some(id) = existing {
            if overwrite {
                let update_result = sqlx::query("UPDATE positions SET start_u = $1, end_u = $2, description = $3, updated_at = NOW() WHERE id = $4")
                    .bind(start_u)
                    .bind(end_u)
                    .bind(empty_to_none(description))
                    .bind(id)
                    .execute(&mut *conn)
                    .await;

                match update_result {
                    Ok(_) => {
                        if ip_address.is_empty() {
                            results.push(format!(
                                "更新机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u})"
                            ));
                        } else {
                            let existing_ip: Option<uuid::Uuid> = match sqlx::query_scalar(
                                "SELECT id FROM ips WHERE position_id = $1 AND device_type = 'cabinet_position' LIMIT 1"
                            )
                            .bind(id)
                            .fetch_optional(&mut *conn)
                            .await {
                                Ok(v) => v,
                                Err(e) => { tracing::warn!("查询现有IP失败: {}", e); None }
                            };

                            let cabinet_network_id: Option<uuid::Uuid> = match sqlx::query_scalar(
                                "SELECT rn.network_id FROM room_networks rn JOIN cabinets c ON c.room_id = rn.room_id WHERE c.id = $1 LIMIT 1"
                            )
                            .bind(cabinet_id)
                            .fetch_optional(&mut *conn)
                            .await {
                                Ok(v) => v,
                                Err(e) => { tracing::warn!("查询机柜网络失败: {}", e); None }
                            };

                            let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };

                            if let Some(ip_id) = existing_ip {
                                let ip_update_result = sqlx::query(
                                    "UPDATE ips SET ip_address = CAST($1 AS INET), ip_version = $2, network_id = $3, updated_at = NOW() WHERE id = $4"
                                )
                                .bind(ip_address)
                                .bind(ip_version)
                                .bind(cabinet_network_id)
                                .bind(ip_id)
                                .execute(&mut *conn)
                                .await;

                                match ip_update_result {
                                    Ok(_) => results.push(format!(
                                        "更新机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP: {ip_address})"
                                    )),
                                    Err(e) => results.push(format!(
                                        "更新机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP更新失败: {e})"
                                    )),
                                }
                            } else {
                                let ip_manager_id = uuid::Uuid::new_v4();
                                let ip_insert_result = sqlx::query(
                                    "INSERT INTO ips (id, position_id, device_type, network_id, ip_address, ip_version, status, created_at, updated_at) VALUES ($1, $2, 'cabinet_position', $3, CAST($4 AS INET), $5, 'active', NOW(), NOW())"
                                )
                                .bind(ip_manager_id)
                                .bind(id)
                                .bind(cabinet_network_id)
                                .bind(ip_address)
                                .bind(ip_version)
                                .execute(&mut *conn)
                                .await;

                                match ip_insert_result {
                                    Ok(_) => results.push(format!(
                                        "更新机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP: {ip_address})"
                                    )),
                                    Err(e) => results.push(format!(
                                        "更新机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP写入失败: {e})"
                                    )),
                                }
                            }
                        }
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(format!(
                            "第{line_num}行跳过: 更新机位 '{name}' 失败 - {e}"
                        ));
                        error_count += 1;
                    }
                }
            } else {
                results.push(format!(
                    "跳过机位（已存在）: {name} (机柜: {cabinet_name})"
                ));
                skip_count += 1;
            }
        } else {
            let new_id = uuid::Uuid::new_v4();
            let insert_result = sqlx::query("INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())")
                .bind(new_id)
                .bind(name)
                .bind(cabinet_id)
                .bind(start_u)
                .bind(end_u)
                .bind(empty_to_none(description))
                .execute(&mut *conn)
                .await;

            match insert_result {
                Ok(_) => {
                    if ip_address.is_empty() {
                        results.push(format!(
                            "导入机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u})"
                        ));
                    } else {
                        let cabinet_network_id: Option<uuid::Uuid> = match sqlx::query_scalar(
                            "SELECT rn.network_id FROM room_networks rn JOIN cabinets c ON c.room_id = rn.room_id WHERE c.id = $1 LIMIT 1",
                        )
                        .bind(cabinet_id)
                        .fetch_optional(&mut *conn)
                        .await {
                            Ok(v) => v,
                            Err(e) => { tracing::warn!("查询机柜网络失败: {}", e); None }
                        };

                        let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };
                        let ip_manager_id = uuid::Uuid::new_v4();
                        let ip_insert_result = sqlx::query(
                            "INSERT INTO ips (id, position_id, device_type, network_id, ip_address, ip_version, status, created_at, updated_at) VALUES ($1, $2, 'cabinet_position', $3, CAST($4 AS INET), $5, 'active', NOW(), NOW())"
                        )
                        .bind(ip_manager_id)
                        .bind(new_id)
                        .bind(cabinet_network_id)
                        .bind(ip_address)
                        .bind(ip_version)
                        .execute(&mut *conn)
                        .await;

                        match ip_insert_result {
                            Ok(_) => results.push(format!(
                                "导入机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP: {ip_address})"
                            )),
                            Err(e) => results.push(format!(
                                "导入机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP写入失败: {e})"
                            )),
                        }
                    }
                    success_count += 1;
                }
                Err(e) => {
                    results.push(format!(
                        "第{line_num}行跳过: 插入机位 '{name}' 失败 - {e}"
                    ));
                    error_count += 1;
                }
            }
        }
    }

    results.push(format!(
        "机位导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}

async fn import_switches(
    conn: &mut sqlx::PgConnection,
    content: &str,
    overwrite: bool,
    results: &mut Vec<String>,
) -> Result<(), String> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut error_count = 0;
    let mut line_num = 1;

    for result in rdr.records() {
        line_num += 1;
        let record = result.map_err(|e| format!("第{line_num}行解析失败: {e}"))?;

        if record.get(0).is_some_and(|s| s == "名称") {
            continue;
        }

        let name = record.get(0).unwrap_or("").trim();
        let ip_address = record.get(1).unwrap_or("").trim();
        let model = record.get(2).unwrap_or("").trim();
        let vendor = record.get(3).unwrap_or("").trim();
        let location = record.get(4).unwrap_or("").trim();
        let snmp_version = record.get(5).unwrap_or("").trim();
        let snmp_port_str = record.get(6).unwrap_or("").trim();
        let snmp_community = record.get(7).unwrap_or("").trim();
        let snmp_username = record.get(8).unwrap_or("").trim();
        let description = record.get(9).unwrap_or("").trim();

        if name.is_empty() {
            results.push(format!("第{line_num}行跳过: 名称为空"));
            error_count += 1;
            continue;
        }

        if name.len() > 100 {
            results.push(format!(
                "第{line_num}行跳过: 名称 '{name}' 超过100个字符"
            ));
            error_count += 1;
            continue;
        }

        if model.len() > 100 {
            results.push(format!(
                "第{line_num}行跳过: 交换机 '{name}' - 型号超过100个字符"
            ));
            error_count += 1;
            continue;
        }

        if vendor.len() > 50 {
            results.push(format!(
                "第{line_num}行跳过: 交换机 '{name}' - 厂商超过50个字符"
            ));
            error_count += 1;
            continue;
        }

        if location.len() > 100 {
            results.push(format!(
                "第{line_num}行跳过: 交换机 '{name}' - 位置超过100个字符"
            ));
            error_count += 1;
            continue;
        }

        if snmp_community.len() > 100 {
            results.push(format!(
                "第{line_num}行跳过: 交换机 '{name}' - SNMP Community超过100个字符"
            ));
            error_count += 1;
            continue;
        }

        if snmp_username.len() > 50 {
            results.push(format!(
                "第{line_num}行跳过: 交换机 '{name}' - SNMP用户名超过50个字符"
            ));
            error_count += 1;
            continue;
        }

        if description.len() > 255 {
            results.push(format!(
                "第{line_num}行跳过: 交换机 '{name}' - 描述超过255个字符"
            ));
            error_count += 1;
            continue;
        }

        let snmp_port: i32 = match snmp_port_str.parse() {
            Ok(v) if v > 0 && v <= 65535 => v,
            Ok(v) => {
                results.push(format!(
                    "第{line_num}行跳过: 交换机 '{name}' - SNMP端口 '{v}' 超出范围(1-65535)"
                ));
                error_count += 1;
                continue;
            }
            Err(_) => 161,
        };

        let snmp_version = match snmp_version {
            "v1" | "v2c" | "v3" => snmp_version.to_string(),
            "" => "v2c".to_string(),
            _ => {
                results.push(format!(
                    "第{line_num}行跳过: 交换机 '{name}' - 无效的SNMP版本 '{snmp_version}' (支持: v1, v2c, v3)"
                ));
                error_count += 1;
                continue;
            }
        };

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM switches WHERE name = $1")
                .bind(name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| format!("查询交换机失败: {e}"))?;

        if let Some(id) = existing {
            if overwrite {
                let update_result = sqlx::query(
                    r"UPDATE switches SET 
                    model = $1, vendor = $2, location = $3, 
                    snmp_version = $4, snmp_port = $5, 
                    snmp_community = $6, snmp_username = $7, 
                    description = $8, updated_at = NOW() WHERE id = $9",
                )
                .bind(empty_to_none(model))
                .bind(empty_to_none(vendor))
                .bind(empty_to_none(location))
                .bind(&snmp_version)
                .bind(snmp_port)
                .bind(empty_to_none(snmp_community))
                .bind(empty_to_none(snmp_username))
                .bind(empty_to_none(description))
                .bind(id)
                .execute(&mut *conn)
                .await;

                match update_result {
                    Ok(_) => {
                        if ip_address.is_empty() {
                            results.push(format!("更新交换机: {name}"));
                        } else {
                            let existing_ip: Option<uuid::Uuid> = match sqlx::query_scalar(
                                "SELECT id FROM ips WHERE position_id = (SELECT id FROM positions WHERE device_type = 'switch' AND device_id = $1) LIMIT 1"
                            )
                            .bind(id)
                            .fetch_optional(&mut *conn)
                            .await {
                                Ok(v) => v,
                                Err(e) => { tracing::warn!("查询现有IP失败: {}", e); None }
                            };

                            let network_id: Option<uuid::Uuid> = match
                                sqlx::query_scalar("SELECT network_id FROM room_networks WHERE room_id = (SELECT room_id FROM cabinets WHERE id = (SELECT cabinet_id FROM positions WHERE device_type = 'switch' AND device_id = $1)) LIMIT 1")
                                    .bind(id)
                                    .fetch_optional(&mut *conn)
                                    .await {
                                Ok(v) => v,
                                Err(e) => { tracing::warn!("查询交换机网络失败: {}", e); None }
                            };

                            let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };

                            if let Some(ip_id) = existing_ip {
                                let ip_update_result = sqlx::query(
                                    "UPDATE ips SET ip_address = CAST($1 AS INET), ip_version = $2, network_id = $3, updated_at = NOW() WHERE id = $4"
                                )
                                .bind(ip_address)
                                .bind(ip_version)
                                .bind(network_id)
                                .bind(ip_id)
                                .execute(&mut *conn)
                                .await;

                                match ip_update_result {
                                    Ok(_) => results
                                        .push(format!("更新交换机: {name} (IP: {ip_address})")),
                                    Err(e) => results
                                        .push(format!("更新交换机: {name} (IP更新失败: {e})")),
                                }
                            } else {
                                let ip_manager_id = uuid::Uuid::new_v4();
                                let ip_insert_result = sqlx::query(
                                    "INSERT INTO ips (id, device_type, network_id, ip_address, ip_version, position_id, status, created_at, updated_at) VALUES ($1, 'switch', $2, CAST($3 AS INET), $4, (SELECT id FROM positions WHERE device_type = 'switch' AND device_id = $5), 'active', NOW(), NOW())"
                                )
                                .bind(ip_manager_id)
                                .bind(network_id)
                                .bind(ip_address)
                                .bind(ip_version)
                                .bind(id)
                                .execute(&mut *conn)
                                .await;

                                match ip_insert_result {
                                    Ok(_) => results
                                        .push(format!("更新交换机: {name} (IP: {ip_address})")),
                                    Err(e) => results
                                        .push(format!("更新交换机: {name} (IP写入失败: {e})")),
                                }
                            }
                        }
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(format!(
                            "第{line_num}行跳过: 更新交换机 '{name}' 失败 - {e}"
                        ));
                        error_count += 1;
                    }
                }
            } else {
                results.push(format!("跳过交换机（已存在）: {name}"));
                skip_count += 1;
            }
        } else {
            let id = uuid::Uuid::new_v4();
            let position_id = uuid::Uuid::new_v4();

            if let Err(e) = sqlx::query(
                "INSERT INTO positions (id, name, device_type, device_id, created_at, updated_at) VALUES ($1, $2, 'switch', $3, NOW(), NOW())"
            )
            .bind(position_id)
            .bind(name)
            .bind(id)
            .execute(&mut *conn)
            .await {
                tracing::warn!("创建交换机位置记录失败: {}", e);
            }

            let insert_result = sqlx::query(
                r"INSERT INTO switches (
                id, name, model, vendor, location, 
                snmp_version, snmp_community, snmp_username, snmp_port, 
                description, created_at, updated_at, position_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW(), NOW(), $11)",
            )
            .bind(id)
            .bind(name)
            .bind(empty_to_none(model))
            .bind(empty_to_none(vendor))
            .bind(empty_to_none(location))
            .bind(&snmp_version)
            .bind(empty_to_none(snmp_community))
            .bind(empty_to_none(snmp_username))
            .bind(snmp_port)
            .bind(empty_to_none(description))
            .bind(position_id)
            .execute(&mut *conn)
            .await;

            match insert_result {
                Ok(_) => {
                    if ip_address.is_empty() {
                        results.push(format!("导入交换机: {name}"));
                    } else {
                        let network_id: Option<uuid::Uuid> = match sqlx::query_scalar(
                            "SELECT network_id FROM room_networks LIMIT 1"
                        )
                        .fetch_optional(&mut *conn)
                        .await {
                            Ok(v) => v,
                            Err(e) => { tracing::warn!("查询网络失败: {}", e); None }
                        };

                        let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };
                        let ip_manager_id = uuid::Uuid::new_v4();
                        let ip_insert_result = sqlx::query(
                            "INSERT INTO ips (id, device_type, network_id, ip_address, ip_version, position_id, status, created_at, updated_at) VALUES ($1, 'switch', $2, CAST($3 AS INET), $4, $5, 'active', NOW(), NOW())"
                        )
                        .bind(ip_manager_id)
                        .bind(network_id)
                        .bind(ip_address)
                        .bind(ip_version)
                        .bind(position_id)
                        .execute(&mut *conn)
                        .await;

                        match ip_insert_result {
                            Ok(_) => {
                                results.push(format!("导入交换机: {name} (IP: {ip_address})"));
                            }
                            Err(e) => {
                                results.push(format!("导入交换机: {name} (IP写入失败: {e})"));
                            }
                        }
                    }
                    success_count += 1;
                }
                Err(e) => {
                    results.push(format!(
                        "第{line_num}行跳过: 插入交换机 '{name}' 失败 - {e}"
                    ));
                    error_count += 1;
                }
            }
        }
    }

    results.push(format!(
        "交换机导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}

pub async fn download_template(
    type_param: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let template_type = type_param.get("type").cloned().unwrap_or_else(|| "all".to_string());
    let utf8_bom = &[0xEF, 0xBB, 0xBF];
    let mut csv_data: Vec<(&str, Vec<u8>)> = Vec::new();

    if template_type == "all" || template_type == "network_regions" {
        let mut csv = Vec::new();
        csv.extend_from_slice(utf8_bom);
        csv.extend_from_slice("名称,描述\n示例网络区域,示例描述\n".as_bytes());
        csv_data.push(("network_regions.csv", csv));
    }

    if template_type == "all" || template_type == "networks" {
        let mut csv = Vec::new();
        csv.extend_from_slice(utf8_bom);
        csv.extend_from_slice("名称,网络区域,IPv4 CIDR,IPv6 CIDR,IPv4网关,IPv6网关,IPv4 DNS,IPv6 DNS,描述\n示例网络,示例网络区域,192.168.1.0/24,,192.168.1.1,,8.8.8.8,,示例描述\n".as_bytes());
        csv_data.push(("networks.csv", csv));
    }

    if template_type == "all" || template_type == "rooms" {
        let mut csv = Vec::new();
        csv.extend_from_slice(utf8_bom);
        csv.extend_from_slice(
            "名称,类型,网络1,网络2\n示例房间,办公室,网络区域/网络名称,\n".as_bytes(),
        );
        csv_data.push(("rooms.csv", csv));
    }

    if template_type == "all" || template_type == "workstations" {
        let mut csv = Vec::new();
        csv.extend_from_slice(utf8_bom);
        csv.extend_from_slice(
            "名称,房间,IP地址,负责人,描述\n示例工位,示例房间,192.168.1.100,管理员,示例描述\n"
                .as_bytes(),
        );
        csv_data.push(("workstations.csv", csv));
    }

    if template_type == "all" || template_type == "cabinets" {
        let mut csv = Vec::new();
        csv.extend_from_slice(utf8_bom);
        csv.extend_from_slice(
            "名称,房间,网络1,网络2,描述\n示例机柜,示例房间,网络区域/网络名称,,示例描述\n"
                .as_bytes(),
        );
        csv_data.push(("cabinets.csv", csv));
    }

    if template_type == "all" || template_type == "positions" {
        let mut csv = Vec::new();
        csv.extend_from_slice(utf8_bom);
        csv.extend_from_slice(
            "名称,机柜,起始U,结束U,IP地址,描述\n示例机位,示例机柜,1,2,192.168.1.101,示例描述\n"
                .as_bytes(),
        );
        csv_data.push(("positions.csv", csv));
    }

    if template_type == "all" || template_type == "switches" {
        let mut csv = Vec::new();
        csv.extend_from_slice(utf8_bom);
        csv.extend_from_slice("名称,IP地址,型号,厂商,位置,SNMP版本,SNMP端口,SNMP Community,SNMP用户名,描述\n示例交换机,192.168.1.1,H3C S5500,H3C,机房A,v2c,161,public,,示例描述\n".as_bytes());
        csv_data.push(("switches.csv", csv));
    }

    let mut buf = Cursor::new(Vec::new());
    let options = FileOptions::<'_, ()>::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    {
        let mut zip = ZipWriter::new(&mut buf);
        for (filename, data) in csv_data {
            zip.start_file(filename, options)
                .map_err(|e| AppError::Internal(format!("创建ZIP文件失败: {e}")))?;
            zip.write_all(&data)
                .map_err(|e| AppError::Internal(format!("写入ZIP文件失败: {e}")))?;
        }
        zip.finish()
            .map_err(|e| AppError::Internal(format!("完成ZIP文件失败: {e}")))?;
    }

    Ok(HttpResponse::Ok()
        .content_type("application/zip")
        .append_header((
            actix_web::http::header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=ipma_import_template_{}.zip",
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            ),
        ))
        .body(buf.into_inner()))
}

pub async fn export_database(
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    let db_config = &state.config.database;

    let output = std::process::Command::new("pg_dump")
        .arg("-h")
        .arg(&db_config.host)
        .arg("-p")
        .arg(db_config.port.to_string())
        .arg("-U")
        .arg(&db_config.username)
        .arg("-d")
        .arg(&db_config.database)
        .arg("--no-owner")
        .arg("--no-acl")
        .arg("--clean")
        .arg("--if-exists")
        .env("PGPASSWORD", &db_config.password)
        .output()
        .map_err(|e| {
            AppError::Internal(format!(
                "执行 pg_dump 失败: {e}。请确保系统已安装 postgresql-client。"
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Internal(format!("pg_dump 执行失败: {stderr}")));
    }

    let sql_content = output.stdout;
    if sql_content.is_empty() {
        return Err(AppError::Internal("导出的 SQL 文件为空".to_string()));
    }

    Ok(HttpResponse::Ok()
        .content_type("application/sql")
        .append_header((
            actix_web::http::header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=ipma_backup_{}.sql",
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            ),
        ))
        .body(sql_content))
}

#[derive(Debug, Deserialize)]
pub struct ClearLogsRequest {
    pub log_type: String,
    pub days: Option<i32>,
}

pub async fn clear_logs(state: web::Data<AppState>, req: web::Json<ClearLogsRequest>) -> Result<HttpResponse, AppError> {
    let days = req.days.unwrap_or(0);

    if days < 0 {
        return Err(AppError::Validation("保留天数不能为负数".to_string()));
    }

    let conn = state.pool()?.get_conn();

    let result = match req.log_type.as_str() {
        "operation" => {
            if days == 0 {
                sqlx::query("DELETE FROM operation_logs")
                    .execute(&conn)
                    .await
            } else {
                sqlx::query(
                    "DELETE FROM operation_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&conn)
                .await
            }
        }
        "login" => {
            if days == 0 {
                sqlx::query("DELETE FROM login_logs")
                    .execute(&conn)
                    .await
            } else {
                sqlx::query(
                    "DELETE FROM login_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&conn)
                .await
            }
        }
        "notification" => {
            if days == 0 {
                sqlx::query("DELETE FROM notifications")
                    .execute(&conn)
                    .await
            } else {
                sqlx::query(
                    "DELETE FROM notifications WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&conn)
                .await
            }
        }
        "all" => {
            let mut deleted = 0u64;

            if days == 0 {
                if let Ok(r) = sqlx::query("DELETE FROM operation_logs")
                    .execute(&conn)
                    .await
                {
                    deleted += r.rows_affected();
                }
                if let Ok(r) = sqlx::query("DELETE FROM login_logs")
                    .execute(&conn)
                    .await
                {
                    deleted += r.rows_affected();
                }
                if let Ok(r) = sqlx::query("DELETE FROM notifications")
                    .execute(&conn)
                    .await
                {
                    deleted += r.rows_affected();
                }
            } else {
                if let Ok(r) = sqlx::query(
                    "DELETE FROM operation_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&conn)
                .await
                {
                    deleted += r.rows_affected();
                }
                if let Ok(r) = sqlx::query(
                    "DELETE FROM login_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&conn)
                .await
                {
                    deleted += r.rows_affected();
                }
                if let Ok(r) = sqlx::query(
                    "DELETE FROM notifications WHERE created_at < NOW() - INTERVAL '1 day' * $1",
                )
                .bind(days)
                .execute(&conn)
                .await
                {
                    deleted += r.rows_affected();
                }
            }

            return Ok(HttpResponse::Ok().json(ApiResponse::success(
                serde_json::json!({ "deleted": deleted }),
                &format!("成功清理 {deleted} 条日志记录"),
            )));
        }
        _ => {
            return Err(AppError::Validation("无效的日志类型".to_string()));
        }
    };

    let r = result?;
    let deleted = r.rows_affected();
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({ "deleted": deleted }),
        &format!("成功清理 {deleted} 条日志记录"),
    )))
}

pub async fn get_logs_stats(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let conn = &state.pool()?.get_conn();

    let operation_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operation_logs")
        .fetch_one(conn)
        .await?;

    let login_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM login_logs")
        .fetch_one(conn)
        .await?;

    let notification_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications")
        .fetch_one(conn)
        .await?;

    let operation_oldest: Option<String> = sqlx::query_scalar(
        "SELECT created_at::text FROM operation_logs ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(conn)
    .await?;

    let login_oldest: Option<String> = sqlx::query_scalar(
        "SELECT created_at::text FROM login_logs ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(conn)
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "operation_logs": { "count": operation_count, "oldest": operation_oldest },
            "login_logs": { "count": login_count, "oldest": login_oldest },
            "notifications": { "count": notification_count }
        }),
        "日志统计获取成功",
    )))
}
