//! IPMI 采集：SDR 仓库枚举 + 传感器读数聚合 + 电源状态。
//!
//! 只做纯采集（无并发/退避/落库，这些在 agent-service 侧）：给定已建立会话的
//! client，枚举 SDR 记录、读取温度/风扇/电压传感器，聚合 max_temp 与列表，
//! 输出与 SNMP 快照同键位的 report JSON（`_ipmi_state` 私有键）。

use ruipmi::{IpmiClient, SdrRecord, calc_sensor_reading, parse_sdr_header, parse_sdr_record};
use serde::Serialize;

use crate::core::{DeviceIdInfo, IpmiErrorExt, read_device_id, take_response};

/// Storage netfn（0x0A）与关键命令码（IPMI 规范 §33/§42）。
const NETFN_STORAGE: u8 = 0x0A;
const CMD_RESERVE_SDR: u8 = 0x22;
const CMD_GET_SDR: u8 = 0x40;
/// Get SDR 单次请求最大读取字节（多数 Full/Compact 记录 <64B，一次读满）。
const SDR_READ_CHUNK: u8 = 0xFF;
/// 记录链结束哨兵（Get SDR 返回 next_record_id=0xFFFF 表示遍历完成）。
const SDR_LAST_RECORD: u16 = 0xFFFF;
/// 传感器遍历上限：防御劣质 BMC 返回自指/环链导致死循环。
const SDR_MAX_RECORDS: usize = 512;

/// 传感器类型码（IPMI Table 42-3）：温度/电压/风扇。
const SENSOR_TYPE_TEMPERATURE: u8 = 0x01;
const SENSOR_TYPE_VOLTAGE: u8 = 0x02;
const SENSOR_TYPE_FAN: u8 = 0x04;

/// 单个传感器读数。
#[derive(Debug, Clone, Serialize)]
pub struct SensorValue {
    pub name: String,
    /// 读数（已按 SDR 线性化公式换算）
    pub value: f64,
    /// 单位文本（°C/RPM/Volts，来自 ruipmi sensor_unit_string）
    pub unit: String,
}

/// 一次完整 IPMI 采集的结果快照。
#[derive(Debug, Clone)]
pub struct IpmiReading {
    pub device: DeviceIdInfo,
    pub power_on: bool,
    /// 温度类传感器中的最高读数（无温度传感器为 None）
    pub max_temp: Option<f64>,
    pub temps: Vec<SensorValue>,
    pub fans: Vec<SensorValue>,
    pub volts: Vec<SensorValue>,
}

impl IpmiReading {
    /// 落库 report JSON：与 SNMP 快照同键位（temp 供历史曲线），IPMI 私有明细
    /// 收纳在 `_ipmi_state`，不影响通用告警评估读取的顶层键。
    #[must_use]
    pub fn to_report(&self) -> serde_json::Value {
        let mut state = serde_json::Map::new();
        state.insert("power_on".to_string(), serde_json::json!(self.power_on));
        state.insert(
            "manuf_id".to_string(),
            serde_json::json!(self.device.manuf_id),
        );
        state.insert(
            "product_id".to_string(),
            serde_json::json!(self.device.product_id),
        );
        state.insert(
            "fw_revision".to_string(),
            serde_json::json!(&self.device.fw_revision),
        );
        state.insert("temps".to_string(), serde_json::json!(&self.temps));
        state.insert("fans".to_string(), serde_json::json!(&self.fans));
        state.insert("volts".to_string(), serde_json::json!(&self.volts));
        serde_json::json!({
            "temp": self.max_temp,
            "_ipmi_state": state,
        })
    }
}

/// 温度类传感器聚合：取最高读数，空列表为 `None`（无温度传感器的 BMC）。
pub fn max_of_temps(temps: &[SensorValue]) -> Option<f64> {
    temps.iter().map(|t| t.value).reduce(f64::max)
}

/// 在既有会话上执行完整采集（Get Device ID + Chassis Status + SDR 传感器）。
/// 会话建立/关闭由调用方管理（agent-service 侧的实时 worker 复用会话）。
pub async fn collect(client: &mut IpmiClient) -> Result<IpmiReading, IpmiErrorExt> {
    let device = read_device_id(client).await?;
    let power_on = read_power_status(client).await.unwrap_or(false);
    let (temps, fans, volts) = read_sensors(client).await?;
    let max_temp = max_of_temps(&temps);
    Ok(IpmiReading {
        device,
        power_on,
        max_temp,
        temps,
        fans,
        volts,
    })
}

/// Get Chassis Status（Chassis netfn 0x00, cmd 0x01）：data[0] bit0=系统电源开启。
async fn read_power_status(client: &mut IpmiClient) -> Result<bool, IpmiErrorExt> {
    let resp = client.get_chassis_status().await?;
    let data = take_response(&resp)?;
    Ok(data.first().is_some_and(|b| b & 0x01 != 0))
}

/// 枚举 SDR 仓库并读取温度/风扇/电压传感器读数。
///
/// Full Sensor Record（Type 0x01）含线性化系数，按 `calc_sensor_reading` 换算；
/// Compact Sensor Record（Type 0x02）的系数解析 ruipmi 未暴露，暂只登记可读名称
/// 对应的温度/风扇不取值（v1 限制，已在 crate 说明中记录）。
async fn read_sensors(
    client: &mut IpmiClient,
) -> Result<(Vec<SensorValue>, Vec<SensorValue>, Vec<SensorValue>), IpmiErrorExt> {
    let reservation = reserve_sdr_repository(client).await?;
    let mut temps = Vec::new();
    let mut fans = Vec::new();
    let mut volts = Vec::new();

    let mut record_id: u16 = 0x0000;
    let mut guard = 0usize;
    while record_id != SDR_LAST_RECORD && guard < SDR_MAX_RECORDS {
        guard += 1;
        let Some((next_id, record)) = read_sdr_record(client, record_id, reservation).await? else {
            break;
        };
        record_id = next_id;

        let Some(header) = parse_sdr_header(&record) else {
            continue;
        };
        let body = &record[5.min(record.len())..];
        match parse_sdr_record(&header, body) {
            SdrRecord::Full(full) => {
                let kind = full.common.sensor_type;
                if !matches!(
                    kind,
                    SENSOR_TYPE_TEMPERATURE | SENSOR_TYPE_FAN | SENSOR_TYPE_VOLTAGE
                ) {
                    continue;
                }
                let raw = match read_sensor_value(client, full.common.sensor_number).await {
                    Ok(v) => v,
                    // 单个传感器读取失败不影响其余枚举（BMC 常有不可读槽位）
                    Err(_) => continue,
                };
                let value = calc_sensor_reading(raw, &full) as f64;
                let unit =
                    ruipmi::sensor_unit_string(full.common.sensor_units_2 & 0x0F).to_string();
                let sensor = SensorValue {
                    name: full.name.clone(),
                    value,
                    unit,
                };
                match kind {
                    SENSOR_TYPE_TEMPERATURE => temps.push(sensor),
                    SENSOR_TYPE_FAN => fans.push(sensor),
                    _ => volts.push(sensor),
                }
            }
            SdrRecord::Compact(_) | SdrRecord::Unknown(_) => {}
        }
    }

    Ok((temps, fans, volts))
}

/// Reserve SDR Repository：返回 2 字节 reservation_id（后续 Get SDR 需携带）。
async fn reserve_sdr_repository(client: &mut IpmiClient) -> Result<[u8; 2], IpmiErrorExt> {
    let resp = client.request(&[NETFN_STORAGE, CMD_RESERVE_SDR]).await?;
    let data = take_response(&resp)?;
    // 部分 BMC 完成码 0 但 reservation 长度不足，按 [0,0] 退化（无保留亦可读）
    Ok([
        data.first().copied().unwrap_or(0),
        data.get(1).copied().unwrap_or(0),
    ])
}

/// 读取一条 SDR 记录（分块补全至 header.record_length）。
/// 返回 `Some((next_record_id, 完整记录字节))`；BMC 无此记录返回 None。
async fn read_sdr_record(
    client: &mut IpmiClient,
    record_id: u16,
    reservation: [u8; 2],
) -> Result<Option<(u16, Vec<u8>)>, IpmiErrorExt> {
    let id = record_id.to_le_bytes();
    let mut record: Vec<u8> = Vec::new();
    let mut offset: u8 = 0;
    let mut next_id: u16 = SDR_LAST_RECORD;
    let mut expected_body: Option<u8> = None;

    loop {
        let want = match expected_body {
            // 已知记录总长（5 字节头 + record_length），按剩余量收敛请求
            Some(len) => {
                let total = usize::from(len) + 5;
                let remaining = total.saturating_sub(record.len());
                (remaining.min(usize::from(SDR_READ_CHUNK))) as u8
            }
            None => SDR_READ_CHUNK,
        };
        if want == 0 {
            break;
        }
        let resp = client
            .request(&[
                NETFN_STORAGE,
                CMD_GET_SDR,
                id[0],
                id[1],
                reservation[0],
                reservation[1],
                offset,
                want,
            ])
            .await?;
        let data = take_response(&resp)?;
        // data = [next_lo, next_hi, record_bytes@offset...]
        if record.is_empty() {
            if data.len() < 2 {
                return Ok(None);
            }
            next_id = u16::from_le_bytes([data[0], data[1]]);
            record.extend_from_slice(&data[2..]);
        } else {
            // 续读响应的前 2 字节仍是 next_record_id，跳过
            record.extend_from_slice(data.get(2..).unwrap_or(&[]));
        }
        if record.len() >= 5 && expected_body.is_none() {
            expected_body = Some(record[4]);
        }
        let got = data.len().saturating_sub(2);
        if got == 0 {
            break;
        }
        offset = offset.saturating_add(got as u8);
        if let Some(len) = expected_body
            && record.len() >= usize::from(len) + 5
        {
            break;
        }
    }

    Ok(Some((next_id, record)))
}

/// Get Sensor Reading（SE netfn 0x04, cmd 0x2D）：返回原始读数字节（data[0]）。
async fn read_sensor_value(client: &mut IpmiClient, sensor_number: u8) -> Result<u8, IpmiErrorExt> {
    let resp = client.get_sensor_reading(sensor_number).await?;
    let data = take_response(&resp)?;
    data.first()
        .copied()
        .ok_or_else(|| crate::core::IpmiErrorExt::Message("传感器读数响应为空".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::DeviceIdInfo;

    fn sensor(name: &str, value: f64, unit: &str) -> SensorValue {
        SensorValue {
            name: name.to_string(),
            value,
            unit: unit.to_string(),
        }
    }

    fn reading(temps: Vec<SensorValue>) -> IpmiReading {
        let max_temp = max_of_temps(&temps);
        IpmiReading {
            device: DeviceIdInfo {
                device_id: 0x20,
                manuf_id: 0x157,
                product_id: 1,
                fw_revision: "2.05".to_string(),
                ipmi_version: 2,
            },
            power_on: true,
            max_temp,
            temps,
            fans: vec![sensor("FAN1", 4200.0, "RPM")],
            volts: vec![sensor("P12V", 12.1, "Volts")],
        }
    }

    #[test]
    fn max_of_temps_picks_highest_or_none() {
        assert_eq!(max_of_temps(&[]), None);
        let temps = vec![
            sensor("CPU", 45.0, "°C"),
            sensor("INLET", 62.5, "°C"),
            sensor("SB", 30.0, "°C"),
        ];
        assert_eq!(max_of_temps(&temps), Some(62.5));
    }

    #[test]
    fn to_report_uses_shared_temp_key_and_private_ipmi_state() {
        let report = reading(vec![sensor("CPU", 55.0, "°C")]).to_report();
        // 顶层 temp 与 SNMP 快照同键位，供通用告警评估与历史曲线读取
        assert_eq!(report["temp"], serde_json::json!(55.0));
        let state = &report["_ipmi_state"];
        assert_eq!(state["power_on"], serde_json::json!(true));
        assert_eq!(state["manuf_id"], serde_json::json!(0x157));
        assert_eq!(state["fw_revision"], serde_json::json!("2.05"));
        assert_eq!(state["fans"].as_array().map(Vec::len), Some(1));
        assert_eq!(state["volts"].as_array().map(Vec::len), Some(1));
        assert_eq!(state["temps"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn to_report_max_temp_null_when_no_thermal_sensor() {
        let report = reading(vec![]).to_report();
        assert_eq!(report["temp"], serde_json::Value::Null);
    }
}
