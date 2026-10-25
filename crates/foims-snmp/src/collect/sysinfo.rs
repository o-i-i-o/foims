//! 设备系统信息采集（sysDescr/sysName → 品牌/型号/主机名）。

use std::time::Duration;

use async_snmp::{Client, oid};

use crate::core::{SnmpError, SnmpParamsLegacy, build_auth, format_snmp_error, snmp_target};

/// 经 SNMP 获取的设备识别信息（sysDescr 推导 + sysName 主机名）。
#[derive(Debug, Clone)]
pub struct DeviceSnmpInfo {
    pub brand: String,
    pub model: String,
    /// 设备主机名（sysName），设备不支持该 OID 时为 None
    pub hostname: Option<String>,
}

pub async fn get_device_info_via_snmp(
    params: &SnmpParamsLegacy,
) -> Result<DeviceSnmpInfo, SnmpError> {
    let addr = snmp_target(&params.ip, params.port);
    let auth = build_auth(params).map_err(SnmpError::Message)?;
    let client = Client::builder(&addr, auth)
        .construction_timeout(Duration::from_secs(5))
        .request_timeout(Duration::from_secs(5))
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    let sys_descr_result = client
        .get(&oid!(1, 3, 6, 1, 2, 1, 1, 1, 0))
        .await
        .map_err(|e| SnmpError::Message(format!("SNMP请求失败: {}", format_snmp_error(e))))?;
    let sys_descr = sys_descr_result
        .varbinds
        .first()
        .and_then(|vb| vb.value.as_str())
        .map(str::to_string)
        .ok_or_else(|| SnmpError::Message("响应格式不正确: 期望字符串类型".to_string()))?;

    // sysName（1.3.6.1.2.1.1.5.0）：部分设备不实现，异常响应时容忍为 None
    let hostname = match client.get(&oid!(1, 3, 6, 1, 2, 1, 1, 5, 0)).await {
        Ok(v) => v
            .varbinds
            .first()
            .filter(|vb| !vb.value.is_exception())
            .and_then(|vb| vb.value.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        _ => None,
    };

    Ok(DeviceSnmpInfo {
        brand: identify_vendor(&sys_descr),
        model: extract_model(&sys_descr),
        hostname,
    })
}

fn identify_vendor(sys_descr: &str) -> String {
    let lower = sys_descr.to_lowercase();

    if lower.contains("cisco") {
        return "Cisco".to_string();
    }
    if lower.contains("huawei") {
        return "Huawei".to_string();
    }
    if lower.contains("h3c") || lower.contains("3com") {
        return "H3C".to_string();
    }
    if lower.contains("juniper") {
        return "Juniper".to_string();
    }
    if lower.contains("dell") {
        return "Dell".to_string();
    }
    if lower.contains("hp") || lower.contains("hpe") || lower.contains("procurve") {
        return "HP/HPE".to_string();
    }
    if lower.contains("aruba") {
        return "Aruba".to_string();
    }
    if lower.contains("netgear") {
        return "Netgear".to_string();
    }
    if lower.contains("tp-link") || lower.contains("tplink") {
        return "TP-Link".to_string();
    }
    if lower.contains("linksys") {
        return "Linksys".to_string();
    }
    if lower.contains("ubiquiti") || lower.contains("ubnt") {
        return "Ubiquiti".to_string();
    }
    if lower.contains("mikrotik") {
        return "MikroTik".to_string();
    }
    if lower.contains("extreme") {
        return "Extreme Networks".to_string();
    }
    if lower.contains("alcatel") {
        return "Alcatel".to_string();
    }
    if lower.contains("zyxel") {
        return "ZyXEL".to_string();
    }
    if lower.contains("d-link") || lower.contains("dlink") {
        return "D-Link".to_string();
    }

    "Unknown".to_string()
}

fn extract_model(sys_descr: &str) -> String {
    let model: String = sys_descr
        .split_whitespace()
        .filter(|word| word.chars().any(|c| c.is_ascii_digit()))
        .take(2)
        .collect();

    if model.is_empty() {
        if sys_descr.len() > 50 {
            sys_descr.chars().take(50).collect()
        } else {
            sys_descr.to_string()
        }
    } else {
        model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 厂商识别 ====================

    #[test]
    fn test_identify_vendor_known_brands() {
        let cases = [
            ("Cisco IOS Software", "Cisco"),
            ("Huawei Versatile Routing Platform", "Huawei"),
            ("H3C Comware Software", "H3C"),
            ("3Com SuperStack Switch", "H3C"),
            ("Juniper Networks Junos", "Juniper"),
            ("Dell EMC Networking", "Dell"),
            ("HP ProCurve Switch", "HP/HPE"),
            ("HPE OfficeConnect", "HP/HPE"),
            ("ProCurve J9021A", "HP/HPE"),
            ("ArubaOS-CX Switch", "Aruba"),
            ("NETGEAR ProSafe GS724T", "Netgear"),
            ("TP-Link JetStream T2600G", "TP-Link"),
            ("TPLink Web Smart Switch", "TP-Link"),
            ("Linksys LGS318", "Linksys"),
            ("Ubiquiti UniFi Switch", "Ubiquiti"),
            ("UBNT EdgeSwitch", "Ubiquiti"),
            ("MikroTik RouterOS", "MikroTik"),
            ("ExtremeXOS Switch", "Extreme Networks"),
            ("Alcatel-Lucent OS6850", "Alcatel"),
            ("ZyXEL GS1900", "ZyXEL"),
            ("D-Link DGS-1210", "D-Link"),
            ("Dlink Smart Switch", "D-Link"),
        ];
        for (sys_descr, expected) in cases {
            assert_eq!(
                identify_vendor(sys_descr),
                expected,
                "sysDescr={sys_descr} 应识别为 {expected}"
            );
        }
    }

    #[test]
    fn test_identify_vendor_case_insensitive_and_priority() {
        // 大小写不敏感
        assert_eq!(identify_vendor("CISCO IOS"), "Cisco");
        assert_eq!(identify_vendor("HUAWEI S5720"), "Huawei");
        // 优先级：cisco 先于 huawei 判定
        assert_eq!(identify_vendor("cisco device by huawei oem"), "Cisco");
        // 未知厂商
        assert_eq!(identify_vendor("Some Generic Device"), "Unknown");
        assert_eq!(identify_vendor(""), "Unknown");
    }

    // ==================== 型号提取 ====================

    #[test]
    fn test_extract_model_with_digits() {
        // 取前两个含数字的词（无分隔符拼接）
        assert_eq!(extract_model("Huawei S5720 5700EI"), "S57205700EI");
        // 仅一个含数字词
        assert_eq!(extract_model("Cisco IOS C2960 Software"), "C2960");
    }

    #[test]
    fn test_extract_model_without_digits() {
        // 无数字词时短串原样返回
        assert_eq!(extract_model("Generic Switch"), "Generic Switch");
        // 长串（超过 50 字符）截断为前 50 字符
        let long_descr = "X".repeat(80);
        let model = extract_model(&long_descr);
        assert_eq!(model.len(), 50, "超长描述应截断为 50 字符");
        assert_eq!(model, "X".repeat(50));
    }
}
