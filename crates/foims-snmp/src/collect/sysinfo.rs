//! 设备系统信息采集（sysDescr/sysName → 品牌/型号/主机名）。

use std::time::Duration;

use async_snmp::{Client, oid};

use crate::core::{
    SnmpError, SnmpParamsLegacy, build_auth, clean_snmp_text, format_snmp_error, snmp_target,
};

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
    // 超时统一取 params.timeout_secs（与 ports/lldp/arp 等采集器同口径）
    let timeout = Duration::from_secs(params.timeout_secs);
    let auth = build_auth(params).map_err(SnmpError::Message)?;
    let client = Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
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
            .and_then(|vb| clean_snmp_text(vb.value.as_str())),
        _ => None,
    };

    Ok(DeviceSnmpInfo {
        brand: identify_vendor(&sys_descr),
        model: extract_model(&sys_descr),
        hostname,
    })
}

/// 厂商标识按词边界匹配：sysDescr 是自由文本，子串匹配会被
/// "ship"/"Sharp" 之类无关词误命中（把整段描述小写后按非字母数字分词）
fn has_vendor_word(sys_descr: &str, word: &str) -> bool {
    sys_descr
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| token == word)
}

fn identify_vendor(sys_descr: &str) -> String {
    // 词边界优先匹配完整厂商词；连字符厂商名（tp-link/d-link）分词后
    // 分别命中第二词；其余保留既有子串启发式（如 hpe/3com 嵌在描述中）
    let has = |word: &str| has_vendor_word(sys_descr, word);
    let lower = sys_descr.to_lowercase();

    if has("cisco") || lower.contains("cisco") {
        return "Cisco".to_string();
    }
    if has("huawei") {
        return "Huawei".to_string();
    }
    if has("h3c") || lower.contains("3com") {
        return "H3C".to_string();
    }
    if has("juniper") {
        return "Juniper".to_string();
    }
    if has("dell") {
        return "Dell".to_string();
    }
    if has("hp") || lower.contains("hpe") || lower.contains("procurve") {
        return "HP/HPE".to_string();
    }
    if has("aruba") || lower.contains("aruba") {
        // 词边界 + 子串双口径：ArubaOS 这类厂商与系统名拼接词仍属专用
        // 品牌名，误命中概率极低（cisco/extreme 同理）
        return "Aruba".to_string();
    }
    if has("netgear") {
        return "Netgear".to_string();
    }
    if has("tp") && lower.contains("link") || has("tplink") {
        return "TP-Link".to_string();
    }
    if has("linksys") {
        return "Linksys".to_string();
    }
    if has("ubiquiti") || has("ubnt") {
        return "Ubiquiti".to_string();
    }
    if has("mikrotik") {
        return "MikroTik".to_string();
    }
    if has("extreme") || lower.contains("extreme") {
        // ExtremeXOS 为系统名拼接词，同 Aruba 口径保留子串兼容
        return "Extreme Networks".to_string();
    }
    if has("alcatel") || lower.contains("alcatel-lucent") {
        return "Alcatel".to_string();
    }
    if has("zyxel") {
        return "ZyXEL".to_string();
    }
    if has("dlink") || (has("d") && lower.contains("link")) {
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

    #[test]
    fn test_identify_vendor_word_boundary_no_substring_false_positive() {
        // C4：子串误配回归——"ship" 含 "hp"、"Sharp" 含 "hp"，词边界匹配后不得命中
        assert_eq!(identify_vendor("Friendship Leader 3000"), "Unknown");
        assert_eq!(identify_vendor("Sharp LCD Panel"), "Unknown");
        // 独立成词的 hp 仍要命中
        assert_eq!(identify_vendor("hp 2920 Switch"), "HP/HPE");
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
