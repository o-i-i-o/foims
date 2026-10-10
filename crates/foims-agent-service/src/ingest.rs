//! Agent 上报入库（设计 docs/agent-design.md §3.3）。
//!
//! 本模块面向 agent 的机器接口（QUIC/HTTP3，无 ApiResponse 信封），错误
//! 以 `{"error": "..."}` 简单格式返回，状态码语义：401 令牌无效、403 已禁用/
//! 吊销、400 请求非法、409 机器标识冲突、413 体积超限、429 上报过于频繁、
//! 500 入库失败。

use axum::Json;
use axum::http::{HeaderMap, StatusCode};
use chrono::{DateTime, Utc};
use foims_common::net::{generate_token_hash, normalize_ipv4_address};
use foims_common::report::AgentReport;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::report_server::ReportContext;

/// 采集时刻与服务端时钟允许的最大偏差（秒，设计 §3.3）
const MAX_CLOCK_SKEW_SECS: i64 = 300;
/// 上报间隔下限（秒，响应控制面不低于该值）
const MIN_REPORT_INTERVAL_SECS: u64 = 10;
/// 温度读数业务范围（℃）：越界视为明显异常，且会溢出 max_temp 的 NUMERIC(5,1)
const TEMP_MIN_C: f64 = -100.0;
const TEMP_MAX_C: f64 = 250.0;
/// 非温度传感器读数上限：NUMERIC(5,1) 可表示的极值（超出会导致数据库写入 500）
const SENSOR_VALUE_LIMIT: f64 = 9999.9;
/// 上报文本字段长度上限（trim 后按字符数计）
const MAX_MACHINE_ID_LEN: usize = 128;
const MAX_HOSTNAME_LEN: usize = 255;
const MAX_OS_LEN: usize = 128;
const MAX_KERNEL_LEN: usize = 128;
const MAX_ARCH_LEN: usize = 32;
const MAX_AGENT_VERSION_LEN: usize = 32;

/// ingest 单条上报的处理结果（状态码 + JSON 响应体）。
type IngestResult = (StatusCode, Json<Value>);

/// 机器接口错误响应（简单 JSON 格式，见模块注释）。
fn error_json(status: StatusCode, message: &str) -> IngestResult {
    (status, Json(json!({ "error": message })))
}

/// 校验 Bearer 令牌格式：`Bearer <64 位十六进制>`，其余形式视为无效。
fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = raw.strip_prefix("Bearer ")?.trim();
    (token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| token.to_ascii_lowercase())
}

/// 校验使用率数值：有限且落在 [0, 100]（纯函数，单测覆盖）。
fn pct_in_range(value: f64) -> bool {
    value.is_finite() && (0.0..=100.0).contains(&value)
}

/// 计算内存使用率：total 为 0（不可得）时返回 None，结果钳制 [0, 100]。
fn mem_usage_pct(total: u64, used: u64) -> Option<f64> {
    if total == 0 {
        return None;
    }
    Some((used as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
}

/// 计算磁盘使用率：取各文件系统 used/total 的最大值，
/// 无 total > 0 的有效项时返回 None，结果钳制 [0, 100]。
fn disk_usage_pct(disks: &[foims_common::report::ReportDisk]) -> Option<f64> {
    disks
        .iter()
        .filter(|d| d.total > 0)
        .map(|d| (d.used as f64 / d.total as f64 * 100.0).clamp(0.0, 100.0))
        .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |m| m.max(v))))
}

/// 取温度传感器的最大读数（kind == "temp"），无有效项时返回 None。
fn max_temp(sensors: &[foims_common::report::ReportSensor]) -> Option<f64> {
    sensors
        .iter()
        .filter(|s| s.kind == "temp" && s.value.is_finite())
        .map(|s| s.value)
        .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |m| m.max(v))))
}

/// 校验采集时刻：RFC 3339 可解析且与服务端时钟偏差不超过 5 分钟。
/// 解析失败/超差返回 Err（纯函数，now 注入便于单测）。
fn validate_collected_at(collected_at: &str, now: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    let parsed = DateTime::parse_from_rfc3339(collected_at)
        .map_err(|e| format!("collected_at 非法（RFC 3339）: {e}"))?
        .with_timezone(&Utc);
    let skew = (parsed - now).num_seconds().abs();
    if skew > MAX_CLOCK_SKEW_SECS {
        return Err(format!(
            "collected_at 与服务端时钟偏差 {skew} 秒，超过 {MAX_CLOCK_SKEW_SECS} 秒上限"
        ));
    }
    Ok(parsed)
}

/// 校验上报内容明显非法值：文本字段 trim/判空/长度、CPU/磁盘使用率越界、
/// 传感器读数越界等。通过时返回解析后的采集时刻（UTC，供历史入库）与
/// trim 后的规范化文本字段（供 UPDATE/入库统一使用）。
fn validate_report(
    report: &AgentReport,
    now: DateTime<Utc>,
) -> Result<(DateTime<Utc>, TrimmedReport), String> {
    let trimmed = normalize_report(report)?;
    if !pct_in_range(report.cpu.usage_pct) {
        return Err(format!("cpu.usage_pct 非法: {}", report.cpu.usage_pct));
    }
    for disk in &report.disks {
        if !pct_in_range(disk.util_pct) {
            return Err(format!(
                "disk {} util_pct 非法: {}",
                disk.device, disk.util_pct
            ));
        }
    }
    for sensor in &report.sensors {
        if !sensor.value.is_finite() {
            return Err(format!(
                "sensor {} 读数非法: {}",
                sensor.label, sensor.value
            ));
        }
        if sensor.kind == "temp" {
            // 温度越界会溢出 max_temp NUMERIC(5,1) 上限导致数据库 500
            if !(TEMP_MIN_C..=TEMP_MAX_C).contains(&sensor.value) {
                return Err(format!(
                    "sensor {} 温度读数越界: {}（允许 {TEMP_MIN_C}~{TEMP_MAX_C}℃）",
                    sensor.label, sensor.value
                ));
            }
        } else if !(-SENSOR_VALUE_LIMIT..=SENSOR_VALUE_LIMIT).contains(&sensor.value) {
            return Err(format!(
                "sensor {} 读数越界: {}（|value| ≤ {SENSOR_VALUE_LIMIT}）",
                sensor.label, sensor.value
            ));
        }
    }
    let collected_at = validate_collected_at(&report.collected_at, now)?;
    Ok((collected_at, trimmed))
}

/// 上报文本字段 trim 后的规范化值（校验通过后供入库统一使用）。
struct TrimmedReport {
    machine_id: String,
    hostname: String,
    os: String,
    kernel: String,
    arch: String,
    agent_version: String,
}

/// 校验单个文本字段：trim 后判空 + 长度上限，通过返回 trim 后的值
/// （纯函数，单测覆盖）。
fn check_text_field(name: &str, raw: &str, max_len: usize) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("{name} 不能为空"));
    }
    let len = trimmed.chars().count();
    if len > max_len {
        return Err(format!("{name} 超长: {len} > {max_len}"));
    }
    Ok(trimmed.to_string())
}

/// 校验并规范化上报文本字段：全部字段 trim 后判空并校验长度上限
/// （纯函数，单测覆盖）。
fn normalize_report(report: &AgentReport) -> Result<TrimmedReport, String> {
    Ok(TrimmedReport {
        machine_id: check_text_field("machine_id", &report.machine_id, MAX_MACHINE_ID_LEN)?,
        hostname: check_text_field("hostname", &report.hostname, MAX_HOSTNAME_LEN)?,
        os: check_text_field("os", &report.system.os, MAX_OS_LEN)?,
        kernel: check_text_field("kernel", &report.system.kernel, MAX_KERNEL_LEN)?,
        arch: check_text_field("arch", &report.system.arch, MAX_ARCH_LEN)?,
        agent_version: check_text_field(
            "agent_version",
            &report.agent_version,
            MAX_AGENT_VERSION_LEN,
        )?,
    })
}

/// 处理单条指标上报：鉴权 → 解析 → 校验 → 机器标识比对 → 事务入库。
///
/// 入库（单事务）：
/// - agents 主行刷新快照列（pending/offline 首报即翻转 active，machine_id
///   缺失时回填，last_seen/first_seen 置位）；
/// - agent_metrics_history 追加曲线精简快照
///   {"cpu","mem","disk","temp","rx_bps","tx_bps"}。
pub async fn ingest(
    ctx: &ReportContext,
    headers: &HeaderMap,
    body: &[u8],
    peer_ip: &str,
) -> IngestResult {
    // 1. Bearer 令牌 → SHA-256 哈希 → 查 agents 行
    let Some(token) = bearer_token(headers) else {
        return error_json(StatusCode::UNAUTHORIZED, "认证失败：缺少合法的 Bearer 令牌");
    };
    let token_hash = generate_token_hash(&token);
    let row = sqlx::query_as::<_, (Uuid, String, Option<String>, Option<DateTime<Utc>>)>(
        "SELECT id, status, machine_id, last_seen FROM agents WHERE token_hash = $1",
    )
    .bind(&token_hash)
    .fetch_optional(&ctx.pool)
    .await;
    let row = match row {
        Ok(r) => r,
        Err(e) => {
            foims_common::log_error!("log.agent.ingest_db_error", error = e);
            return error_json(StatusCode::INTERNAL_SERVER_ERROR, "数据库查询失败");
        }
    };
    let Some((agent_id, status, stored_machine_id, last_seen)) = row else {
        return error_json(StatusCode::UNAUTHORIZED, "认证失败：令牌无效");
    };
    if status == "disabled" || status == "revoked" {
        return error_json(StatusCode::FORBIDDEN, "Agent 已被禁用或吊销，禁止上报");
    }

    // 1b. 上报频控：last_seen 距今小于生效间隔一半时拒绝（429），
    // 防止恶意 agent 高频写库；正常 agent 按下发间隔上报不受影响
    let interval = ctx.report_interval_secs.max(MIN_REPORT_INTERVAL_SECS);
    let min_gap_secs = i64::try_from(interval / 2).unwrap_or(i64::MAX);
    if let Some(last_seen) = last_seen
        && (Utc::now() - last_seen).num_seconds() < min_gap_secs
    {
        return error_json(StatusCode::TOO_MANY_REQUESTS, "上报过于频繁，请稍后再试");
    }

    // 2. 解析与校验（校验通过返回解析后的采集时刻与 trim 后的文本字段）
    let report: AgentReport = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => return error_json(StatusCode::BAD_REQUEST, &format!("上报 JSON 解析失败: {e}")),
    };
    let (collected_at, trimmed) = match validate_report(&report, Utc::now()) {
        Ok(t) => t,
        Err(e) => return error_json(StatusCode::BAD_REQUEST, &e),
    };

    // 3. 机器标识比对：NULL 首报回填；非空且不一致视为令牌被复用到其他机器
    if let Some(existing) = stored_machine_id.as_deref()
        && existing != trimmed.machine_id
    {
        foims_common::log_warn!(
            "log.agent.machine_id_conflict",
            agent_id = agent_id,
            stored = existing,
            reported = trimmed.machine_id
        );
        return error_json(
            StatusCode::CONFLICT,
            "machine_id 与既有绑定不一致，疑似令牌被复用到其他机器",
        );
    }

    // 4. 快照列计算
    let mem_pct = mem_usage_pct(report.memory.total, report.memory.used);
    let disk_pct = disk_usage_pct(&report.disks);
    let temp = max_temp(&report.sensors);
    // 流量合计（bit/s）：各网卡速率求和，无网卡 → null（与 SNMP 轮询
    // 路径的快照键保持一致，供详情网络曲线共用）
    let sum_bps = |get: fn(&foims_common::report::ReportNet) -> f64| -> Option<f64> {
        (!report.nets.is_empty()).then(|| report.nets.iter().map(get).sum())
    };
    let raw_metrics = serde_json::to_value(&report).unwrap_or(Value::Null);
    let history_snapshot = json!({
        "cpu": report.cpu.usage_pct,
        "mem": mem_pct,
        "disk": disk_pct,
        "temp": temp,
        "rx_bps": sum_bps(|n| n.rx_bps),
        "tx_bps": sum_bps(|n| n.tx_bps),
    });
    let ip = normalize_ipv4_address(peer_ip);

    // 5. 单事务入库
    let mut tx = match ctx.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            foims_common::log_error!("log.agent.ingest_db_error", error = e);
            return error_json(StatusCode::INTERNAL_SERVER_ERROR, "数据库事务开启失败");
        }
    };
    let update = sqlx::query(
        r"UPDATE agents SET
            status = CASE WHEN status IN ('pending', 'offline') THEN 'active' ELSE status END,
            machine_id = COALESCE(machine_id, $2),
            hostname = $3,
            ip = $4,
            os = $5,
            kernel = $6,
            arch = $7,
            agent_version = $8,
            cpu_usage = $9,
            mem_usage_pct = $10,
            disk_usage_pct = $11,
            max_temp = $12,
            uptime_secs = $13,
            raw_metrics = $14,
            last_seen = NOW(),
            first_seen = COALESCE(first_seen, NOW())
          WHERE id = $1",
    )
    .bind(agent_id)
    .bind(&trimmed.machine_id)
    .bind(&trimmed.hostname)
    .bind(&ip)
    .bind(&trimmed.os)
    .bind(&trimmed.kernel)
    .bind(&trimmed.arch)
    .bind(&trimmed.agent_version)
    .bind(report.cpu.usage_pct)
    .bind(mem_pct)
    .bind(disk_pct)
    .bind(temp)
    .bind(i64::try_from(report.system.uptime_secs).unwrap_or(i64::MAX))
    .bind(&raw_metrics)
    .execute(&mut *tx)
    .await;
    if let Err(e) = update {
        // machine_id 唯一索引冲突：该机已绑定到其他 token 的安装记录
        // （同机重复下载安装包），按设计返回 409 由运营处理
        let mid_dup = e.as_database_error().is_some_and(|db| {
            db.is_unique_violation()
                && db
                    .constraint()
                    .is_some_and(|c| c.contains("idx_agents_machine_id"))
        });
        if mid_dup {
            let owner = sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM agents WHERE machine_id = $1 AND id <> $2 LIMIT 1",
            )
            .bind(&trimmed.machine_id)
            .bind(agent_id)
            .fetch_one(&ctx.pool)
            .await
            .ok();
            foims_common::log_warn!(
                "log.agent.machine_id_conflict",
                agent_id = agent_id,
                stored = owner
                    .map(|u| u.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                reported = trimmed.machine_id
            );
            return error_json(
                StatusCode::CONFLICT,
                "machine_id 已绑定到其他安装记录（同机重复安装），请在主机监控列表删除旧记录后重试",
            );
        }
        foims_common::log_error!("log.agent.ingest_db_error", error = e);
        return error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "入库失败：agents 更新出错",
        );
    }
    // 唯一索引 (agent_id, collected_at) 冲突时跳过：agent 重试/重放同一
    // 采集时刻的幂等去重，避免重复上报写重
    let insert = sqlx::query(
        r"INSERT INTO agent_metrics_history (agent_id, collected_at, metrics)
          VALUES ($1, $2, $3) ON CONFLICT (agent_id, collected_at) DO NOTHING",
    )
    .bind(agent_id)
    .bind(collected_at)
    .bind(&history_snapshot)
    .execute(&mut *tx)
    .await;
    if let Err(e) = insert {
        foims_common::log_error!("log.agent.ingest_db_error", error = e);
        return error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "入库失败：历史指标写入出错",
        );
    }
    if let Err(e) = tx.commit().await {
        foims_common::log_error!("log.agent.ingest_db_error", error = e);
        return error_json(StatusCode::INTERNAL_SERVER_ERROR, "入库失败：事务提交出错");
    }

    // 6. 告警评估：入库成功后按全局阈值评估本条快照（超阈值边沿发站内
    // 通知）；评估失败仅记日志，不影响已成功的上报响应
    let values = crate::alerts::MetricValues {
        cpu_pct: Some(report.cpu.usage_pct),
        mem_pct,
        disk_pct,
        temp_c: temp,
    };
    if let Err(e) =
        crate::alerts::evaluate_report(&ctx.pool, agent_id, &trimmed.hostname, &ip, values).await
    {
        foims_common::log_error!(
            "log.agent.alert_evaluate_failed",
            agent_id = agent_id,
            error = e
        );
    }

    // 7. 响应控制面：下发上报间隔（不低于下限）与最新版本通告
    (
        StatusCode::OK,
        Json(json!(foims_common::report::ReportResponse {
            report_interval: ctx.report_interval_secs.max(MIN_REPORT_INTERVAL_SECS),
            collectors: std::collections::BTreeMap::new(),
            latest_version: ctx.server_version.clone(),
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collected_at_合法时间通过并返回解析值() {
        let now = Utc::now();
        let ts = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let parsed = validate_collected_at(&ts, now).unwrap();
        assert!((parsed - now).num_seconds().abs() <= 1);
    }

    #[test]
    fn collected_at_偏差超五分钟拒绝() {
        let now = Utc::now();
        let future = (now + chrono::Duration::seconds(MAX_CLOCK_SKEW_SECS + 1)).to_rfc3339();
        let past = (now - chrono::Duration::seconds(MAX_CLOCK_SKEW_SECS + 1)).to_rfc3339();
        assert!(
            validate_collected_at(&future, now).is_err(),
            "未来超差应拒绝"
        );
        assert!(validate_collected_at(&past, now).is_err(), "过去超差应拒绝");
    }

    #[test]
    fn collected_at_格式非法拒绝() {
        assert!(validate_collected_at("not-a-time", Utc::now()).is_err());
        assert!(validate_collected_at("", Utc::now()).is_err());
    }

    #[test]
    fn 使用率范围校验() {
        assert!(pct_in_range(0.0));
        assert!(pct_in_range(100.0));
        assert!(pct_in_range(23.5));
        assert!(!pct_in_range(100.1));
        assert!(!pct_in_range(-0.5));
        assert!(!pct_in_range(f64::NAN));
        assert!(!pct_in_range(f64::INFINITY));
    }

    #[test]
    fn 内存使用率_total为零返回none() {
        assert_eq!(mem_usage_pct(0, 100), None, "total 不可得时应为 None");
        assert_eq!(mem_usage_pct(1000, 250), Some(25.0));
        // used > total（异常上报）钳制为 100
        assert_eq!(mem_usage_pct(1000, 2000), Some(100.0));
    }

    #[test]
    fn 磁盘使用率_取最大值且无有效项为none() {
        assert_eq!(disk_usage_pct(&[]), None, "无磁盘应为 None");
        let mk = |total: u64, used: u64| foims_common::report::ReportDisk {
            device: "sda".to_string(),
            mount: "/".to_string(),
            total,
            used,
            read_iops: 0.0,
            write_iops: 0.0,
            util_pct: 0.0,
        };
        let disks = vec![mk(1000, 100), mk(2000, 1000), mk(0, 500)];
        assert_eq!(
            disk_usage_pct(&disks),
            Some(50.0),
            "取 used/total 最大值且跳过 total=0"
        );
        assert_eq!(disk_usage_pct(&[mk(0, 1)]), None, "全部 total=0 应为 None");
    }

    #[test]
    fn 最高温度_仅取temp类型最大值() {
        let mk = |kind: &str, value: f64| foims_common::report::ReportSensor {
            label: "s".to_string(),
            kind: kind.to_string(),
            value,
        };
        assert_eq!(max_temp(&[]), None);
        assert_eq!(max_temp(&[mk("fan", 1200.0)]), None, "非 temp 不计入");
        assert_eq!(
            max_temp(&[mk("temp", 45.0), mk("temp", 62.5), mk("temp", 51.0)]),
            Some(62.5)
        );
    }

    #[test]
    fn 上报校验_整体非法值拒绝() {
        let mut report = sample_report();
        assert!(validate_report(&report, Utc::now()).is_ok(), "样例应通过");
        report.cpu.usage_pct = 120.0;
        assert!(
            validate_report(&report, Utc::now()).is_err(),
            "CPU 越界应拒绝"
        );
        report.cpu.usage_pct = 50.0;
        report.disks[0].util_pct = -1.0;
        assert!(
            validate_report(&report, Utc::now()).is_err(),
            "磁盘 util 越界应拒绝"
        );
        report.disks[0].util_pct = 10.0;
        report.hostname = "  ".to_string();
        assert!(
            validate_report(&report, Utc::now()).is_err(),
            "空主机名应拒绝"
        );
    }

    #[test]
    fn 上报校验_传感器读数越界拒绝() {
        let mut report = sample_report();
        // 温度业务范围 [-100, 250]：越界（会溢出 max_temp NUMERIC(5,1)）拒绝
        report.sensors[0].kind = "temp".to_string();
        report.sensors[0].value = 10000.0;
        assert!(
            validate_report(&report, Utc::now()).is_err(),
            "温度 10000 应拒绝"
        );
        report.sensors[0].value = -150.0;
        assert!(
            validate_report(&report, Utc::now()).is_err(),
            "温度 -150 应拒绝"
        );
        // 边界值合法
        report.sensors[0].value = -100.0;
        assert!(
            validate_report(&report, Utc::now()).is_ok(),
            "温度 -100 应通过"
        );
        report.sensors[0].value = 250.0;
        assert!(
            validate_report(&report, Utc::now()).is_ok(),
            "温度 250 应通过"
        );
        // 非温度类型按 NUMERIC(5,1) 极值约束：±9999.9 合法，超出拒绝
        report.sensors[0].kind = "fan".to_string();
        report.sensors[0].value = 1200.0;
        assert!(
            validate_report(&report, Utc::now()).is_ok(),
            "风扇 1200 应通过"
        );
        report.sensors[0].value = 10000.0;
        assert!(
            validate_report(&report, Utc::now()).is_err(),
            "非温度 10000 应拒绝"
        );
        report.sensors[0].value = -20000.0;
        assert!(
            validate_report(&report, Utc::now()).is_err(),
            "非温度 -20000 应拒绝"
        );
    }

    #[test]
    fn 上报校验_字段超长拒绝() {
        let cases: Vec<(&str, Box<dyn Fn(&mut AgentReport)>)> = vec![
            (
                "machine_id",
                Box::new(|r: &mut AgentReport| r.machine_id = "m".repeat(129)),
            ),
            (
                "hostname",
                Box::new(|r: &mut AgentReport| r.hostname = "h".repeat(256)),
            ),
            (
                "os",
                Box::new(|r: &mut AgentReport| r.system.os = "o".repeat(129)),
            ),
            (
                "kernel",
                Box::new(|r: &mut AgentReport| r.system.kernel = "k".repeat(129)),
            ),
            (
                "arch",
                Box::new(|r: &mut AgentReport| r.system.arch = "a".repeat(33)),
            ),
            (
                "agent_version",
                Box::new(|r: &mut AgentReport| r.agent_version = "v".repeat(33)),
            ),
        ];
        for (name, mutate) in cases {
            let mut report = sample_report();
            mutate(&mut report);
            assert!(
                validate_report(&report, Utc::now()).is_err(),
                "{name} 超长应拒绝"
            );
        }
    }

    #[test]
    fn 上报校验_空白trim后通过且返回规范化值() {
        let mut report = sample_report();
        report.machine_id = "  m-1234567890abcdef  ".to_string();
        report.hostname = "\t web-01 \n".to_string();
        report.system.os = " Ubuntu ".to_string();
        report.system.kernel = " 5.15 ".to_string();
        report.system.arch = " x86_64 ".to_string();
        report.agent_version = " 0.1.2 ".to_string();
        let (_, trimmed) = validate_report(&report, Utc::now())
            .unwrap_or_else(|e| panic!("带首尾空白的合法值应通过: {e}"));
        assert_eq!(trimmed.machine_id, "m-1234567890abcdef");
        assert_eq!(trimmed.hostname, "web-01");
        assert_eq!(trimmed.os, "Ubuntu");
        assert_eq!(trimmed.kernel, "5.15");
        assert_eq!(trimmed.arch, "x86_64");
        assert_eq!(trimmed.agent_version, "0.1.2");
    }

    #[test]
    fn 文本字段校验_判空与长度() {
        assert!(
            check_text_field("machine_id", "  ", 128).is_err(),
            "纯空白应判空拒绝"
        );
        assert_eq!(
            check_text_field("machine_id", " m-1 ", 128),
            Ok("m-1".to_string()),
            "trim 后返回规范化值"
        );
        assert!(check_text_field("arch", "a".repeat(33).as_str(), 32).is_err());
        assert!(check_text_field("arch", "a".repeat(32).as_str(), 32).is_ok());
    }

    /// 最小合法上报样例（与 foims-common report 测试同构）
    fn sample_report() -> AgentReport {
        let mut report: AgentReport = serde_json::from_str(
            r#"{
                "machine_id": "m-1234567890abcdef",
                "hostname": "web-01",
                "agent_version": "0.1.2",
                "collected_at": "2026-10-10T12:00:00Z",
                "system": {"os": "Ubuntu", "kernel": "5.15", "arch": "x86_64", "uptime_secs": 100},
                "cpu": {"usage_pct": 12.5, "cores": 4, "load1": 0.1, "load5": 0.1, "load15": 0.1},
                "memory": {"total": 1000, "used": 250, "swap_total": 0, "swap_used": 0},
                "disks": [{"device": "sda", "mount": "/", "total": 1000, "used": 100,
                            "read_iops": 0.0, "write_iops": 0.0, "util_pct": 5.0}],
                "nets": [],
                "sensors": [{"label": "cpu", "kind": "temp", "value": 50.0}],
                "processes": 100
            }"#,
        )
        .unwrap();
        // collected_at 动态取当前时刻：固定时间戳随时钟推移会触发偏差校验
        report.collected_at = Utc::now().to_rfc3339();
        report
    }
}
