//! 设备系统信息采集（sysDescr/sysObjectID/sysName → 品牌/型号/主机名）。
//!
//! 品牌识别走 [`crate::vendor::identify`]（双证据纯函数，sysObjectID
//! 企业号 + sysDescr 词边界关键词），本模块只负责取数与调用。

use std::time::Duration;

use async_snmp::{Client, Oid, oid};

use foims_common::msg;

use crate::core::{
    SnmpError, SnmpParamsLegacy, build_auth, clean_snmp_text, snmp_target, wrap_snmp_error,
};
use crate::vendor;

/// 经 SNMP 获取的设备识别信息（识别结果推导 + sysName 主机名）。
#[derive(Debug, Clone)]
pub struct DeviceSnmpInfo {
    pub brand: String,
    pub model: String,
    /// 设备主机名（sysName），设备不支持该 OID 时为 None
    pub hostname: Option<String>,
}

/// 采集设备识别信息：一次 GET sysDescr（1.3.6.1.2.1.1.1.0）+ sysObjectID
/// （1.3.6.1.2.1.1.2.0）走 [`vendor::identify`] 推导品牌，
/// 再 GET sysName（1.3.6.1.2.1.1.5.0）取主机名。
///
/// sysDescr 缺失即整体失败；sysObjectID/sysName 部分设备不实现，
/// 异常响应容忍为单证据识别/None。
pub async fn get_device_info_via_snmp(
    params: &SnmpParamsLegacy,
) -> Result<DeviceSnmpInfo, SnmpError> {
    let addr = snmp_target(&params.ip, params.port);
    let timeout = Duration::from_secs(params.timeout_secs);
    let auth = build_auth(params).map_err(SnmpError::Message)?;
    let client = Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .connect()
        .await
        .map_err(|e| wrap_snmp_error("server.snmp.error.session_create_failed", e))?;

    let sys_descr_oid = oid!(1, 3, 6, 1, 2, 1, 1, 1, 0);
    let sys_object_id_oid = oid!(1, 3, 6, 1, 2, 1, 1, 2, 0);
    let response = client
        .get_many(&[sys_descr_oid.clone(), sys_object_id_oid.clone()])
        .await
        .map_err(|e| wrap_snmp_error("server.snmp.error.request_failed", e))?;
    let sys_descr = response
        .varbinds
        .iter()
        .find(|vb| vb.oid == sys_descr_oid)
        .filter(|vb| !vb.value.is_exception())
        .and_then(|vb| vb.value.as_str())
        .map(str::to_string)
        .ok_or_else(|| SnmpError::Message(msg("server.snmp.error.response_malformed")))?;
    // sysObjectID：少数设备不实现，缺失时单证据（sysDescr）仍可识别
    let object_id: Option<Oid> = response
        .varbinds
        .iter()
        .find(|vb| vb.oid == sys_object_id_oid)
        .filter(|vb| !vb.value.is_exception())
        .and_then(|vb| vb.value.as_oid())
        .cloned();

    let identity = vendor::identify(object_id.as_ref(), Some(&sys_descr));

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
        brand: vendor::display_name(identity.vendor_guess).to_string(),
        model: extract_model(&sys_descr),
        hostname,
    })
}

/// 型号取 sysDescr 中前两个含数字的词直接拼接（无分隔符，如
/// 「S5720 5700EI」→ `S57205700EI`）；无数字词时返回原描述，超 50 字符截断。
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

    #[test]
    fn test_extract_model_with_digits() {
        // 两个含数字词无分隔符拼接
        assert_eq!(extract_model("Huawei S5720 5700EI"), "S57205700EI");
        assert_eq!(extract_model("Cisco IOS C2960 Software"), "C2960");
    }

    #[test]
    fn test_extract_model_without_digits() {
        assert_eq!(extract_model("Generic Switch"), "Generic Switch");
        let long_descr = "X".repeat(80);
        let model = extract_model(&long_descr);
        assert_eq!(model.len(), 50, "超长描述应截断为 50 字符");
        assert_eq!(model, "X".repeat(50));
    }
}
