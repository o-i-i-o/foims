//! SNMP 连通性探测与设备信息系统（test-snmp / snmp-info / snmp-ports）。
//!
//! 探测入参允许与设备库配置混用（请求字段优先）；目标地址受 SSRF 约束，
//! 组播与链路本地段拒绝，回环与私有段只告警放行。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use uuid::Uuid;

use foims_common::AppJson;
use foims_common::DbProvider;
use foims_common::log_info;
use foims_common::{AppError, msg};
use foims_models::SnmpTestRequest;

use crate::collect::ports::get_device_ports_via_snmp;
use crate::collect::sysinfo::get_device_info_via_snmp;
use crate::core::{DecryptedSnmpCredentials, SnmpParamsLegacy, get_device_snmp_config, test_snmp};

/// POST /api/resources/devices/{id}/test-snmp：按设备库已存配置发起连通性
/// 探测；路径 `id` 覆盖请求体的 `device_id`，其余口径同
/// `/api/resources/devices/test-snmp`。
pub async fn test_snmp_connection_by_id<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(device_id): Path<Uuid>,
    AppJson(req): AppJson<SnmpTestRequest>,
) -> Result<Response, AppError> {
    let mut test_req = req;
    test_req.device_id = Some(device_id);
    test_snmp_connection(State(state), AppJson(test_req)).await
}

/// POST /api/resources/devices/test-snmp：向目标地址发起一次性 SNMP 探测，
/// 成功返回 `sysDescr`。请求字段优先、缺省回退设备库配置（凭据经解密）；
/// 设备不存在 404，IP 缺失/非法/组播/链路本地 400，探测失败 400。
pub async fn test_snmp_connection<P: DbProvider>(
    State(state): State<Arc<P>>,
    AppJson(req): AppJson<SnmpTestRequest>,
) -> Result<Response, AppError> {
    let device_id_label = req.device_id.map(|d| d.to_string()).unwrap_or_default();
    let ip_label = req.ip_address.clone().unwrap_or_default();
    let version_label = req.snmp_version.clone().unwrap_or_default();
    log_info!(
        "log.device.snmp.test_request",
        device_id = device_id_label,
        ip = ip_label,
        version = version_label
    );

    let (ip, version, community, username, auth_proto, auth_pass, priv_proto, priv_pass, port) =
        if let Some(device_id) = req.device_id {
            let conn = state.pool()?.get_conn();
            let (switch, ip_address) = get_device_snmp_config(&conn, &device_id).await?;

            let creds = DecryptedSnmpCredentials::from_device_snmp_async(&switch).await?;

            (
                req.ip_address.or(ip_address),
                req.snmp_version
                    .or(switch.snmp_version)
                    .unwrap_or_else(|| "v2c".to_string()),
                req.snmp_community.or(creds.community),
                req.snmp_username.or(switch.snmp_username),
                req.snmp_auth_protocol.or(switch.snmp_auth_protocol),
                req.snmp_auth_password.or(creds.auth_password),
                req.snmp_priv_protocol.or(switch.snmp_priv_protocol),
                req.snmp_priv_password.or(creds.priv_password),
                req.snmp_port.or(switch.snmp_port).unwrap_or(161),
            )
        } else {
            (
                req.ip_address,
                req.snmp_version.unwrap_or_else(|| "v2c".to_string()),
                req.snmp_community,
                req.snmp_username,
                req.snmp_auth_protocol,
                req.snmp_auth_password,
                req.snmp_priv_protocol,
                req.snmp_priv_password,
                req.snmp_port.unwrap_or(161),
            )
        };

    let ip = match ip {
        Some(s) if !s.is_empty() => s,
        _ => return Err(AppError::Validation(msg("server.device.snmp.ip_required"))),
    };

    // 拒绝组播/链路本地地址防止 SSRF；回环与私有地址属机房管理正常目标
    // （本机 snmpd 联调、环回对照设备），仅告警放行。SSRF 口径与 ping/端口探测
    // 共用 foims_common::net::guard_probe_ip，避免多处重复实现
    ip.parse::<std::net::IpAddr>()
        .map_err(|_| AppError::Validation(msg("server.device.snmp.ip_invalid").with("ip", &ip)))
        .and_then(foims_common::net::guard_probe_ip)?;

    let snmp_params = SnmpParamsLegacy {
        ip: ip.clone(),
        port,
        version: version.clone(),
        community: community.clone(),
        username: username.clone(),
        auth_proto: auth_proto.clone(),
        auth_pass: auth_pass.clone(),
        priv_proto: priv_proto.clone(),
        priv_pass: priv_pass.clone(),
        // 连通性探测固定短超时，不沿用设备侧周期采集的超时配置
        timeout_secs: 5,
    };

    // 凭据属敏感物料，日志只记是否设置、不落明文
    let community_label = community.as_ref().map(|_| "set").unwrap_or("none");
    let username_label = username.clone().unwrap_or_default();
    log_info!(
        "log.device.snmp.test_params",
        ip = ip,
        port = port,
        version = version,
        community = community_label,
        username = username_label
    );

    match test_snmp(&snmp_params).await {
        Ok(sys_descr) => Ok(foims_common::ok_json(
            serde_json::json!({ "sysDescr": sys_descr }),
            "server.device.snmp.test_success",
        )),
        Err(e) => Err(AppError::Snmp(
            msg("server.device.snmp.test_failed").with("error", e),
        )),
    }
}

/// GET /api/resources/devices/{id}/snmp-info：实时读取设备的品牌、型号、
/// 主机名。设备不存在 404，未配置 IP 400，采集失败 400。
pub async fn get_device_info_snmp<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let (switch, ip_address) =
        get_device_snmp_config(&state.pool()?.get_conn(), &device_id).await?;

    let ip_address =
        ip_address.ok_or_else(|| AppError::Validation(msg("server.device.no_ip_configured")))?;

    let snmp_params = switch.to_snmp_params_async(&ip_address).await?;

    match get_device_info_via_snmp(&snmp_params).await {
        Ok(info) => Ok(foims_common::ok_json(
            serde_json::json!({
                "brand": info.brand,
                "model": info.model,
                "hostname": info.hostname
            }),
            "server.device.snmp.info_retrieved",
        )),
        Err(e) => Err(AppError::Snmp(
            msg("server.device.snmp.info_fetch_failed").with("error", e),
        )),
    }
}

/// GET /api/resources/devices/{id}/snmp-ports：实时读取设备端口清单
/// （名称/类型/VLAN/状态/速率/描述），不落库。设备不存在 404，
/// 未配置 IP 400，采集失败 400。
pub async fn get_device_ports_snmp<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let (switch, ip_address) =
        get_device_snmp_config(&state.pool()?.get_conn(), &device_id).await?;

    let ip_address =
        ip_address.ok_or_else(|| AppError::Validation(msg("server.device.no_ip_configured")))?;

    let snmp_params = switch.to_snmp_params_async(&ip_address).await?;

    match get_device_ports_via_snmp(&snmp_params).await {
        Ok(ports) => Ok(foims_common::ok_json(
            ports,
            "server.device.snmp.ports_retrieved",
        )),
        Err(e) => Err(AppError::Snmp(
            msg("server.device.snmp.switch_ports_fetch_failed").with("error", e),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试桩：pool 为 None，迫使探测走直接 IP 输入路径，不触及数据库分支
    #[derive(Clone)]
    struct TestState {
        pool: Option<foims_common::db::DbPool>,
    }

    impl foims_common::DbProvider for TestState {
        fn pool(&self) -> Result<&foims_common::db::DbPool, foims_common::AppError> {
            self.pool.as_ref().ok_or_else(|| {
                foims_common::AppError::Internal(foims_common::msg("server.db.not_initialized"))
            })
        }
    }

    fn make_state() -> Arc<TestState> {
        Arc::new(TestState { pool: None })
    }

    /// 发起 SNMP 连接测试并断言返回的错误消息 key
    async fn assert_snmp_ip_rejected(ip: Option<&str>, expected_key: &str) {
        let req = foims_models::SnmpTestRequest {
            device_id: None,
            ip_address: ip.map(str::to_string),
            snmp_version: Some("v2c".to_string()),
            snmp_community: Some("public".to_string()),
            snmp_username: None,
            snmp_auth_protocol: None,
            snmp_auth_password: None,
            snmp_priv_protocol: None,
            snmp_priv_password: None,
            snmp_port: Some(161),
        };
        let result = test_snmp_connection(State(make_state()), AppJson(req)).await;
        let err = result.err().unwrap_or_else(|| panic!("IP {ip:?} 应被拒绝"));
        assert!(
            matches!(err, AppError::Validation(_)),
            "应返回 Validation 错误，实际: {err}"
        );
        assert_eq!(
            err.message().key(),
            expected_key,
            "IP {ip:?} 的拒绝原因不符"
        );
    }

    #[tokio::test]
    async fn test_snmp_rejects_multicast_addresses() {
        assert_snmp_ip_rejected(Some("224.0.0.1"), "server.device.snmp.multicast_forbidden").await;
        assert_snmp_ip_rejected(Some("239.1.1.1"), "server.device.snmp.multicast_forbidden").await;
        assert_snmp_ip_rejected(Some("ff02::1"), "server.device.snmp.multicast_forbidden").await;
    }

    #[tokio::test]
    async fn test_snmp_rejects_link_local_and_metadata_addresses() {
        assert_snmp_ip_rejected(
            Some("169.254.1.1"),
            "server.device.snmp.link_local_forbidden",
        )
        .await;
        // 元数据端点 169.254.169.254 亦属链路本地段，期望 key 因此是
        // link_local 而非专用分支
        assert_snmp_ip_rejected(
            Some("169.254.169.254"),
            "server.device.snmp.link_local_forbidden",
        )
        .await;
    }

    #[tokio::test]
    async fn test_snmp_rejects_invalid_or_missing_ip() {
        assert_snmp_ip_rejected(Some("not-an-ip"), "server.device.snmp.ip_invalid").await;
        assert_snmp_ip_rejected(Some("999.1.1.1"), "server.device.snmp.ip_invalid").await;
        assert_snmp_ip_rejected(None, "server.device.snmp.ip_required").await;
        assert_snmp_ip_rejected(Some(""), "server.device.snmp.ip_required").await;
    }
}
