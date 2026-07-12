use crate::types::{DataError, DataProvider, DataResult};
use actix_web::{HttpResponse, web};
use sqlx::Row;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::io::{Cursor, Write};
use zip::{ZipWriter, write::FileOptions};

fn escape_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

pub async fn export_csv<P: DataProvider>(
    provider: P,
    type_param: web::Query<HashMap<String, String>>,
) -> DataResult<HttpResponse> {
    let pool = provider.pool()?;
    let mut conn = pool.acquire().await.map_err(DataError::from)?;

    let export_type = type_param
        .get("type")
        .cloned()
        .unwrap_or_else(|| "all".to_string());
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
        csv_data.push(export_switches(&mut conn, utf8_bom, &provider).await?);
    }

    if export_type == "all" || export_type == "ips" {
        csv_data.push(export_ip_managers(&mut conn, utf8_bom).await?);
    }

    let buf = tokio::task::spawn_blocking(move || {
        let mut buf = Cursor::new(Vec::new());
        let options = FileOptions::<'_, ()>::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        {
            let mut zip = ZipWriter::new(&mut buf);
            for (filename, data) in csv_data {
                zip.start_file(filename, options)
                    .map_err(|e| DataError::Internal(format!("创建ZIP文件失败: {e}")))?;
                zip.write_all(&data)
                    .map_err(|e| DataError::Internal(format!("写入ZIP文件失败: {e}")))?;
            }
            zip.finish()
                .map_err(|e| DataError::Internal(format!("完成ZIP文件失败: {e}")))?;
        }

        Ok::<Vec<u8>, DataError>(buf.into_inner())
    })
    .await
    .map_err(|e| DataError::Internal(format!("ZIP压缩任务失败: {e}")))??;

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
        .body(buf))
}

async fn export_network_regions(
    conn: &mut sqlx::PgConnection,
    utf8_bom: &[u8],
) -> DataResult<(&'static str, Vec<u8>)> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);
    csv.extend_from_slice("名称,描述\n".as_bytes());

    let rows = sqlx::query("SELECT name, description FROM network_regions ORDER BY name")
        .fetch_all(&mut *conn)
        .await
        .map_err(DataError::from)?;

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
) -> DataResult<(&'static str, Vec<u8>)> {
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
    .await
    .map_err(DataError::from)?;

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
) -> DataResult<(&'static str, Vec<u8>)> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);

    let rooms = sqlx::query("SELECT id, name, room_type FROM rooms ORDER BY name")
        .fetch_all(&mut *conn)
        .await
        .map_err(DataError::from)?;

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
        .await
        .map_err(DataError::from)?;

        if networks.len() > max_networks {
            max_networks = networks.len();
        }
        room_networks_map.insert(room_id, networks);
    }

    let mut header = String::from("名称,类型");
    for i in 1..=max_networks {
        write!(header, ",网络{i}").ok();
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
                write!(line, ",{}", escape_csv_field(&networks[i])).ok();
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
) -> DataResult<(&'static str, Vec<u8>)> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);
    csv.extend_from_slice("名称,房间,IP地址,负责人,描述\n".as_bytes());

    let rows = sqlx::query(
        r"SELECT w.id, w.name, r.name as room_name, w.manager, w.description,
                  (SELECT host(i.ip_address) FROM ips i JOIN devices d ON i.device_id = d.id WHERE d.workstation_id = w.id LIMIT 1) as ip_address
           FROM workstations w
           JOIN rooms r ON w.room_id = r.id
           ORDER BY r.name, w.name",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

    for row in rows {
        let name: String = row.get(1);
        let room: String = row.get(2);
        let manager: Option<String> = row.get(3);
        let description: Option<String> = row.get(4);
        let ip_address: Option<String> = row.get(5);

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
) -> DataResult<(&'static str, Vec<u8>)> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);

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
        .await
        .map_err(DataError::from)?;

        if networks.len() > max_networks {
            max_networks = networks.len();
        }
        cabinet_networks_map.insert(cabinet_id, networks);
    }

    let mut header = String::from("名称,房间");
    for i in 1..=max_networks {
        write!(header, ",网络{i}").ok();
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
                write!(line, ",{}", escape_csv_field(&networks[i])).ok();
            } else {
                line.push(',');
            }
        }
        writeln!(
            line,
            ",{}",
            escape_csv_field(&description.unwrap_or_default())
        )
        .ok();
        csv.extend_from_slice(line.as_bytes());
    }

    Ok(("cabinets.csv", csv))
}

async fn export_positions(
    conn: &mut sqlx::PgConnection,
    utf8_bom: &[u8],
) -> DataResult<(&'static str, Vec<u8>)> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);
    csv.extend_from_slice("名称,机柜,起始U,结束U,IP地址,描述\n".as_bytes());

    let rows = sqlx::query(
        r"SELECT p.id, p.name, c.name as cabinet_name, p.start_u, p.end_u, p.description,
                  (SELECT host(i.ip_address) FROM ips i JOIN devices d ON i.device_id = d.id WHERE d.position_id = p.id LIMIT 1) as ip_address
           FROM positions p
           JOIN cabinets c ON p.cabinet_id = c.id
           ORDER BY c.name, p.start_u",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

    for row in rows {
        let name: String = row.get(1);
        let cabinet: String = row.get(2);
        let start_u: i32 = row.get(3);
        let end_u: i32 = row.get(4);
        let description: Option<String> = row.get(5);
        let ip_address: Option<String> = row.get(6);

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

async fn export_switches<P: DataProvider>(
    conn: &mut sqlx::PgConnection,
    utf8_bom: &[u8],
    provider: &P,
) -> DataResult<(&'static str, Vec<u8>)> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);
    csv.extend_from_slice(
        "名称,IP地址,型号,厂商,位置,SNMP版本,SNMP端口,SNMP Community,SNMP用户名,描述\n".as_bytes(),
    );

    let rows = sqlx::query(
        r"SELECT s.id, s.name, s.model, s.vendor, s.location, s.snmp_version, s.snmp_port,
           s.snmp_community, s.snmp_username, s.description,
           (SELECT host(i.ip_address) FROM ips i JOIN devices d ON i.device_id = d.id WHERE d.position_id = s.position_id LIMIT 1) as ip_address
           FROM switches s
           ORDER BY s.name",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

    for row in rows {
        let name: String = row.get(1);
        let model: Option<String> = row.get(2);
        let vendor: Option<String> = row.get(3);
        let location: Option<String> = row.get(4);
        let snmp_version: Option<String> = row.get(5);
        let snmp_port: Option<i32> = row.get(6);
        let snmp_community: Option<String> = row.get(7);
        let snmp_username: Option<String> = row.get(8);
        let description: Option<String> = row.get(9);
        let ip_address: Option<String> = row.get(10);

        let decrypted_community = if let Some(c) = snmp_community.filter(|c| !c.is_empty()) {
            Some(provider.decrypt_password(&c).await?)
        } else {
            None
        };

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
) -> DataResult<(&'static str, Vec<u8>)> {
    let mut csv = Vec::new();
    csv.extend_from_slice(utf8_bom);
    csv.extend_from_slice("工位,机位,网络,IP地址,MAC地址,主机名,状态\n".as_bytes());

    let rows = sqlx::query(
        r"SELECT w.name as workstation_name, p.name as position_name,
           n.name as network_name, host(im.ip_address),
           im.mac_address, im.hostname, im.status
           FROM ips im
           JOIN devices d ON im.device_id = d.id
           LEFT JOIN workstations w ON d.workstation_id = w.id
           LEFT JOIN positions p ON d.position_id = p.id
           LEFT JOIN network_cidrs n ON im.network_id = n.id
           ORDER BY im.ip_address",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(DataError::from)?;

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

pub async fn download_template(
    type_param: web::Query<HashMap<String, String>>,
) -> DataResult<HttpResponse> {
    let template_type = type_param
        .get("type")
        .cloned()
        .unwrap_or_else(|| "all".to_string());
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
                .map_err(|e| DataError::Internal(format!("创建ZIP文件失败: {e}")))?;
            zip.write_all(&data)
                .map_err(|e| DataError::Internal(format!("写入ZIP文件失败: {e}")))?;
        }
        zip.finish()
            .map_err(|e| DataError::Internal(format!("完成ZIP文件失败: {e}")))?;
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
